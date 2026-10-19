//! Kimi continuation borrows the actual Incoming USER and PROJECT admissions.
use super::*;
use crate::{
    knowledge_ask_service::AskService,
    knowledge_conversation::ConversationDecisionGuard,
    knowledge_question::{DocumentaryQuestionContent, check_read, prepare_documentary_question},
};
use graphit_ai::cli_kimi::{KimiReceipt, KimiRequest, KimiSink, run_kimi};
use graphit_auth::{ProjectReadAdmission, UserKnowledgeReadAdmission, UserKnowledgeWriteAdmission};
use serde_json::value::RawValue;
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
        provider: "kimi".into(),
        engine: ConversationEngine::Kimi,
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
    let session = match custody.configure_kimi(
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
    // Startup may spawn a native child: abandonment must already retain the writer.
    decision.submitted();
    let owner = match input
        .service
        .start_kimi(
            input.original.snapshot(),
            input.project,
            input.cwd,
            Some(&custody),
        )
        .await
    {
        Ok(owner) => owner,
        Err(error) => {
            // The service associated every returned owner before post-spawn checks.
            if let Some(owner) = custody.kimi_server() {
                owner.stop();
                let deadline = Instant::now()
                    .checked_add(input.service.policy.process.cleanup)
                    .unwrap_or_else(Instant::now);
                let observed = owner.observe(deadline).await;
                custody.observed_kimi(&observed);
                let persistence = custody.finish(input.original.snapshot(), false);
                decision.decided();
                let state = match persistence {
                    ConversationPersistence::Pending => "pending_process",
                    ConversationPersistence::NotStarted => "not_started",
                    ConversationPersistence::Failed(_) => "failed",
                    ConversationPersistence::Denied => "denied",
                    ConversationPersistence::Persisted(_) => {
                        unreachable!("denied startup cannot commit")
                    }
                };
                return crate::mcp_output::typed_error(json!({
                    "status":"failed","execution_error":error.to_string(),
                    "persistence":{"status":state},
                    "receipt":{"provider":"kimi","accepted_notifications":sink.accepted,
                        "completion":{"status":"failed","requested_model":input.model},
                        "process":{"joined":observed.joined,
                            "clean":observed.clean,"status_code":observed.status_code,
                            "cleanup":observed.cleanup.map(|value|format!("{value:?}")),
                            "reason":observed.process_reason.map(|value|format!("{value:?}")),
                            "stdout_eof":observed.stdout_eof,"stderr_eof":observed.stderr_eof,
                            "driver_cancelled":observed.driver_cancelled}}
                }));
            }
            custody.release_unsubmitted();
            decision.decided();
            return failure(&error.to_string());
        }
    };
    let endpoint = owner.ready(input.original.snapshot()).await;
    let (execution, startup_error) = match endpoint {
        Ok(endpoint) => (
            Some(
                run_kimi(
                    input.original.snapshot(),
                    input.project,
                    &endpoint,
                    KimiRequest {
                        session,
                        cwd: input.cwd.to_owned(),
                        model: Some(input.model.to_owned()),
                        prompt,
                    },
                    input.service.kimi_limits(),
                    &mut sink,
                )
                .await,
            ),
            None,
        ),
        Err(reason) => (None, Some(format!("{reason:?}"))),
    };
    if execution.as_ref().is_none_or(Result::is_err) || sink.check().is_err() {
        owner.stop();
    }
    let cleanup_deadline = Instant::now()
        .checked_add(input.service.policy.process.cleanup)
        .unwrap_or_else(Instant::now);
    let observed = owner.observe(cleanup_deadline).await;
    // The same owner is already in durable custody; transfer its actual observation
    // synchronously before deciding persistence or projecting a response.
    custody.observed_kimi(&observed);
    let receipt = execution.as_ref().map(|result| match result {
        Ok(receipt) => receipt,
        Err(error) => error.receipt(),
    });
    let bound = receipt
        .and_then(KimiReceipt::session)
        .is_some_and(|id| custody.observe_kimi(id, input.original.snapshot()));
    let error = execution
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(|error| format!("{:?}", error.reason()))
        .or(startup_error);
    let allowed = execution.as_ref().is_some_and(Result::is_ok)
        && observed.joined
        && observed.clean
        && bound
        && receipt.and_then(KimiReceipt::eligible_text).is_some()
        && sink.check().is_ok();
    let persistence = custody.finish(input.original.snapshot(), allowed);
    decision.decided();
    let persisted = matches!(&persistence, ConversationPersistence::Persisted(_));
    let answer = (persisted && allowed)
        .then(|| receipt.and_then(KimiReceipt::eligible_text))
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
    // Native selectors stay private; the response and correlated turn confirm the session.
    let mut native = json!({"provider":"kimi","accepted_notifications":sink.accepted,
        "session_start":if input.creating {"create"} else {"resume"},
        "server_version":receipt.and_then(KimiReceipt::version),"completion":completion,
        "process":{"joined":observed.joined,
        "clean":observed.clean,"status_code":observed.status_code,
        "cleanup":observed.cleanup.map(|v|format!("{v:?}")),
        "reason":observed.process_reason.map(|v|format!("{v:?}")),
        "stdout_eof":observed.stdout_eof,"stderr_eof":observed.stderr_eof,
        "driver_cancelled":observed.driver_cancelled}});
    if persisted
        && allowed
        && let Some(terminal) = receipt.and_then(KimiReceipt::terminal)
    {
        native["native_result_json"] = json!(terminal.get());
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
impl KimiSink for Sink<'_, '_, '_> {
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
            .emit(&knowledge_ask::kimi::ProgressEvent(event), original)
            .await
            .map_err(|_| io::ErrorKind::BrokenPipe)?;
        self.accepted = self
            .accepted
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        self.check()
    }
}
