//! Copilot continuation borrows the actual Incoming USER and PROJECT admissions.
use super::*;
use crate::{
    knowledge_ask_service::AskService,
    knowledge_conversation::ConversationDecisionGuard,
    knowledge_question::{DocumentaryQuestionContent, check_read, prepare_documentary_question},
};
use graphit_ai::cli_copilot::{CopilotRequest, CopilotSink, run_copilot};
use graphit_auth::{ProjectReadAdmission, UserKnowledgeReadAdmission, UserKnowledgeWriteAdmission};
use serde::{Serialize, Serializer, ser::SerializeMap};
use serde_json::value::RawValue;
use std::{io, path::Path};

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
    pub instruction: &'a str,
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
        input.instruction,
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
        provider: "copilot".into(),
        engine: ConversationEngine::Copilot,
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
    let host = match input.service.host() {
        Ok(host) => host,
        Err(error) => return failure(&format!("Knowledge conversation host: {:?}", error.kind())),
    };
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
    let session = match custody.configure_copilot(
        input.service.program(),
        input.model,
        input.cwd,
        input.original.snapshot(),
    ) {
        Ok(session) => session,
        Err(error) => {
            custody.release_unsubmitted();
            decision.decided();
            return failure(&error.to_string());
        }
    };
    if sink.check().is_err() {
        custody.release_unsubmitted();
        decision.decided();
        return failure(
            "Knowledge conversation authorization changed while waiting for USER writer",
        );
    }
    sink.custody = Some(Arc::clone(&custody));
    // Acquire and register the USER writer before polling the Process producer.
    decision.submitted();
    let mut execution = run_copilot(
        input.original.snapshot(),
        input.project,
        &host,
        CopilotRequest {
            session,
            command: input.service.copilot_request(input.cwd),
            model: input.model.into(),
            prompt,
        },
        input.service.copilot_limits(),
        &mut sink,
    )
    .await;
    // No await or projection may precede custody of the actual Process receipt.
    match &mut execution {
        Ok(receipt) => custody.retain_copilot(receipt),
        Err(error) => custody.retain_copilot(error.receipt_mut()),
    }
    let receipt = match &execution {
        Ok(receipt) => receipt,
        Err(error) => error.receipt(),
    };
    let public_terminal = receipt.terminal().map(public_event).transpose();
    let error = execution
        .as_ref()
        .err()
        .map(|error| format!("{:?}", error.reason()))
        .or_else(|| {
            public_terminal
                .as_ref()
                .err()
                .map(|_| "InvalidNativeProjection".to_owned())
        });
    let allowed = error.is_none() && receipt.eligible_text().is_some() && sink.check().is_ok();
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
    // Native selectors and binding paths stay private. Session binding is not a
    // fabricated native event. Resume validates the observed native session identity.
    let mut native = json!({"provider":"copilot","accepted_notifications":sink.accepted,
        "session_start":if input.creating {"create"} else {"resume"},"completion":completion});
    if let Some(process) = receipt.process() {
        native["process"] = knowledge_ask::process_value(process);
    }
    if persisted
        && allowed
        && let Ok(Some(terminal)) = public_terminal
    {
        native["native_result"] = terminal;
    }
    let mut payload = json!({"status":status,"generation":knowledge_ask::generation_value(&generation),
        "provided_slugs":documents.iter().map(|d|d.slug.as_str()).collect::<Vec<_>>(),
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
impl CopilotSink for Sink<'_, '_, '_> {
    fn recheck(&self, original: &graphit_core::file_io::SnapshotContext) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.check()
    }
    fn session_bound(
        &mut self,
        id: &str,
        resumed: bool,
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
            .observe_copilot(id, resumed, self.original)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.check()
    }
    async fn push<'a>(
        &'a mut self,
        event: &'a RawValue,
        original: &'a graphit_core::file_io::SnapshotContext,
    ) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        self.check()?;
        self.progress
            .emit(
                &ProgressEvent(public_event(event).map_err(|_| io::ErrorKind::InvalidData)?),
                original,
            )
            .await
            .map_err(|_| io::ErrorKind::BrokenPipe)?;
        self.accepted = self
            .accepted
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        self.check()
    }
}
// The AI receipt keeps the complete native frame. Public continuation events
// omit private native selectors and tool arguments rather than claiming raw bytes.
fn public_event(event: &RawValue) -> Result<Value, serde_json::Error> {
    fn sanitize(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.remove("sessionId");
                fields.remove("arguments");
                for value in fields.values_mut() {
                    sanitize(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    sanitize(value);
                }
            }
            _ => (),
        }
    }
    let mut value = serde_json::from_str(event.get())?;
    sanitize(&mut value);
    Ok(value)
}
struct ProgressEvent(Value);
impl Serialize for ProgressEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("provider", "copilot")?;
        map.serialize_entry("event", &self.0)?;
        map.end()
    }
}
