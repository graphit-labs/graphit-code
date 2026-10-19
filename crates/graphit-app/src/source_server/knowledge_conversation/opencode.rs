//! OpenCode continuation borrows the actual Incoming USER and PROJECT admissions.
use super::*;
use crate::{
    knowledge_ask_service::AskService,
    knowledge_conversation::ConversationDecisionGuard,
    knowledge_question::{DocumentaryQuestionContent, check_read, prepare_documentary_question},
};
use graphit_ai::{
    cli_opencode::{OpencodePhase, OpencodeRequest, OpencodeSink, run_opencode},
    cli_opencode_wire::OpencodeUpdate,
};
use graphit_auth::{ProjectReadAdmission, UserKnowledgeReadAdmission, UserKnowledgeWriteAdmission};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{io, path::Path, time::Instant};

pub(super) struct Invocation<'a, 'user, 'project> {
    pub service: &'a AskService,
    pub original: &'a RequestContext<'user>,
    pub project: &'a ProjectReadAdmission<'project>,
    pub user_read: &'a UserKnowledgeReadAdmission<'user>,
    pub user_write: &'a UserKnowledgeWriteAdmission<'user>,
    pub stop: &'a CancellationToken,
    pub store: &'a KnowledgeConversationStore,
    pub creating: bool,
    pub conversation_id: &'a str,
    pub model: &'a str,
    pub query: &'a str,
    pub selected_slugs: &'a [String],
    pub cwd: &'a Path,
    pub project_id: String,
    pub project_version: &'a str,
    pub progress: knowledge_ask::ProgressSink<'static>,
}
pub(super) async fn execute(input: Invocation<'_, '_, '_>) -> CallToolResult {
    let slugs: Vec<_> = input.selected_slugs.iter().map(String::as_str).collect();
    let prepared = match prepare_documentary_question(
        input.original.snapshot(),
        input.project,
        input.query,
        &slugs,
        input.model,
        input.service.policy.completion.max_model_id_bytes,
        input.service.policy.question,
    )
    .await
    {
        Ok(prepared) => prepared,
        Err(error) => return failure(&error.to_string()),
    };
    let Some(documents) = prepared.documents() else {
        let response = match prepared.into_execution() {
            Ok(DocumentaryQuestionContent::Empty(empty)) => knowledge_ask::terminal(empty),
            Ok(_) => unreachable!(),
            Err(error) => failure(&error.to_string()),
        };
        return if input.user_read.recheck().is_ok()
            && input.user_write.recheck().is_ok()
            && check_read(input.original.snapshot(), input.project).is_ok()
            && !input.stop.is_cancelled()
        {
            response
        } else {
            deny_delivery(response, "UserOrProjectDeliveryDenied")
        };
    };
    let metadata = ConversationBinding {
        provider: "opencode".into(),
        engine: ConversationEngine::OpenCode,
        model: input.model.into(),
        program: input.service.program().to_owned(),
        project_id: input.project_id,
        project_version: input.project_version.into(),
        sources: documents
            .iter()
            .map(|d| ConversationSource {
                slug: d.slug.clone(),
                source_path: d.source_path.clone(),
                physical_digest: d.physical_digest.map(|d| *d.as_bytes()),
                compilation_digest: *d.compilation_digest.as_bytes(),
                recipe: d.recipe.clone(),
                fallback_title: d.recipe_parameters.fallback_title.clone(),
                kind_on_absent: d.recipe_parameters.kind_on_absent.clone(),
                max_summary_bytes: d.recipe_parameters.max_summary_bytes,
            })
            .collect(),
    };
    let content = match prepared.into_execution() {
        Ok(content) => content,
        Err(error) => return failure(&error.to_string()),
    };
    let DocumentaryQuestionContent::Ready {
        generation,
        documents,
        prompt,
    } = content
    else {
        unreachable!()
    };
    let mut sink = Sink {
        original: input.original.snapshot(),
        project: input.project,
        user_read: input.user_read,
        user_write: input.user_write,
        stop: input.stop,
        custody: None,
        progress: input.progress,
        accepted: 0,
    };
    if sink.check().is_err() {
        return failure("Knowledge conversation authorization changed before execution");
    }
    let turn = match if input.creating {
        input
            .store
            .begin_create(input.conversation_id, metadata, input.original.snapshot())
    } else {
        input
            .store
            .begin_turn(input.conversation_id, &metadata, input.original.snapshot())
    } {
        Ok(turn) => turn,
        Err(error) => return failure(&error.to_string()),
    };
    let custody = input.service.hold_conversation_turn(turn);
    let mut decision = ConversationDecisionGuard::new(Arc::clone(&custody));
    let session = match custody.configure_opencode(
        input.service.program(),
        input.model,
        input.cwd,
        input.original.snapshot(),
    ) {
        Ok(session) => session,
        Err(error) => return failure(&error.to_string()),
    };
    if sink.check().is_err() {
        return failure(
            "Knowledge conversation authorization changed while waiting for USER writer",
        );
    }
    sink.custody = Some(Arc::clone(&custody));
    // Startup only waits before spawning. It associates the owner synchronously
    // before returning this transport, so no await can hide an installed owner.
    let (transport, owner) = match input
        .service
        .start_opencode(
            input.original.snapshot(),
            input.project,
            input.cwd,
            Some(&custody),
        )
        .await
    {
        Ok(started) => started,
        Err(error) => {
            custody.release_unsubmitted();
            decision.decided();
            return failure(&error.to_string());
        }
    };
    decision.submitted();
    let execution = run_opencode(
        input.original.snapshot(),
        transport,
        OpencodeRequest {
            session,
            cwd: input.cwd.to_owned(),
            model: Some(input.model.to_owned()),
            agent: None,
            prompt,
        },
        input.service.opencode_limits(),
        &mut sink,
    )
    .await;
    if execution.is_err() || sink.check().is_err() {
        owner.stop();
    }
    let cleanup_deadline = Instant::now()
        .checked_add(input.service.policy.process.cleanup)
        .unwrap_or_else(Instant::now);
    let joined = custody.observe_opencode(cleanup_deadline).await;
    let clean = joined && owner.clean().await;
    let process = owner.public_observation().await;
    let receipt = match &execution {
        Ok(receipt) => receipt,
        Err(error) => error.receipt(),
    };
    let error = execution
        .as_ref()
        .err()
        .map(|error| format!("{:?}", error.reason()));
    let allowed = execution.is_ok()
        && joined
        && clean
        && receipt.eligible_text().is_some()
        && sink.check().is_ok();
    let persistence = custody.finish(input.original.snapshot(), allowed);
    decision.decided();
    let persisted = matches!(&persistence, ConversationPersistence::Persisted(_));
    let answer = (persisted && allowed)
        .then(|| receipt.eligible_text())
        .flatten();
    let status = if !persisted || !allowed {
        "failed"
    } else {
        match answer {
            Some(text) if text.trim().is_empty() => "empty_answer",
            Some(_) => "answer",
            None => "evidence_unavailable",
        }
    };
    let mut completion = json!({"status":status,"requested_model":input.model});
    if let Some(answer) = answer {
        completion["answer"] = json!(answer);
    }
    // Create/Load IDs, history and configuration remain private. Only the current
    // prompt's native terminal can be projected after successful persistence.
    let mut native = json!({"provider":"opencode",
        "session_start":if input.creating {"create"} else {"load"},
        "observed_notifications":receipt.observed(),"replayed_notifications":receipt.replayed(),
        "accepted_notifications":sink.accepted,"denied_permissions":receipt.denied_permissions(),
        "sdk_driver_completed":receipt.driver().map(Result::is_ok),
        "cancel_enqueued":receipt.cancel_enqueue().map(Result::is_ok),
        "completion":completion,"process":process});
    if persisted && allowed {
        native["native_result_json"] =
            json!(json!({"prompt_response":receipt.response()}).to_string());
    }
    let mut payload = json!({"status":status,"generation":knowledge_ask::generation_value(&generation),
        "provided_slugs":documents.iter().map(|document|document.slug.as_str()).collect::<Vec<_>>(),
        "execution_error":error,"receipt":native});
    payload["persistence"] = match persistence {
        ConversationPersistence::Persisted(record) => {
            json!({"status":"persisted","conversation":record})
        }
        ConversationPersistence::Failed(error) => {
            json!({"status":"failed","error":error.to_string()})
        }
        ConversationPersistence::Pending => json!({"status":"pending_process"}),
        ConversationPersistence::NotStarted => json!({"status":"not_started"}),
        ConversationPersistence::Denied => json!({"status":"denied"}),
    };
    let response = if persisted && allowed {
        crate::mcp_output::typed(payload)
    } else {
        crate::mcp_output::typed_error(payload)
    };
    input.service.reap().await;
    if !persisted || sink.check().is_err() {
        deny_delivery(response, "ContinuationOrDeliveryDenied")
    } else {
        response
    }
}
struct Sink<'a, 'user, 'project> {
    original: &'a Arc<graphit_core::file_io::SnapshotContext>,
    project: &'a ProjectReadAdmission<'project>,
    user_read: &'a UserKnowledgeReadAdmission<'user>,
    user_write: &'a UserKnowledgeWriteAdmission<'user>,
    stop: &'a CancellationToken,
    custody: Option<Arc<ConversationCustodyEntry>>,
    progress: knowledge_ask::ProgressSink<'static>,
    accepted: u64,
}
impl Sink<'_, '_, '_> {
    fn check(&self) -> io::Result<()> {
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.user_read
            .recheck()
            .map_err(|_| io::ErrorKind::PermissionDenied)?;
        self.user_write
            .recheck()
            .map_err(|_| io::ErrorKind::PermissionDenied)?;
        check_read(self.original, self.project).map_err(|_| io::ErrorKind::PermissionDenied.into())
    }
}
impl OpencodeSink for Sink<'_, '_, '_> {
    fn recheck(&self, original: &graphit_core::file_io::SnapshotContext) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.check()
    }
    fn session_bound(
        &mut self,
        id: &str,
        loaded: bool,
        original: &graphit_core::file_io::SnapshotContext,
    ) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.check()?;
        if !self
            .custody
            .as_ref()
            .expect("turn held before native execution")
            .observe_opencode_session(id, loaded, self.original)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.check()
    }
    async fn push<'a>(
        &'a mut self,
        phase: OpencodePhase,
        event: &'a OpencodeUpdate,
        original: &'a graphit_core::file_io::SnapshotContext,
    ) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.check()?;
        self.progress
            .emit(&ProgressEvent(phase, event), original)
            .await
            .map_err(|_| io::ErrorKind::BrokenPipe)?;
        self.accepted = self
            .accepted
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        self.check()
    }
}
struct ProgressEvent<'a>(OpencodePhase, &'a OpencodeUpdate);
impl Serialize for ProgressEvent<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("provider", "opencode")?;
        map.serialize_entry("phase", &format!("{:?}", self.0))?;
        match self.1 {
            OpencodeUpdate::Known(event) | OpencodeUpdate::Child(event) => {
                map.serialize_entry("event", event.as_ref())?
            }
            OpencodeUpdate::Unknown(params) => map.serialize_entry("event", params)?,
        }
        map.end()
    }
}
