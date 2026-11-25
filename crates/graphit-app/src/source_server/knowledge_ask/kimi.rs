//! Native Kimi Ask with the original Knowledge admission and registered Process owner.
use super::*;
use crate::{
    knowledge_ask_service::AskService,
    knowledge_question::{DocumentaryQuestionContent, PreparedDocumentaryQuestion, check_read},
};
use graphit_ai::cli_kimi::{KimiReceipt, KimiRequest, KimiSink, run_kimi};
use graphit_auth::ProjectReadAdmission;
use std::{io, path::Path, sync::Arc};

pub(super) async fn execute(
    service: &AskService,
    original: &RequestContext<'_>,
    admission: &ProjectReadAdmission<'_>,
    cwd: &Path,
    input: &crate::foreground_knowledge::ask::Input,
    prepared: PreparedDocumentaryQuestion<'_, '_, '_>,
    progress: &mut ProgressSink<'_>,
) -> CallToolResult {
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
        let DocumentaryQuestionContent::Empty(empty) = content else {
            unreachable!()
        };
        return terminal(empty);
    };
    let owner = match service
        .start_kimi(original.snapshot(), admission, cwd, None)
        .await
    {
        Ok(owner) => owner,
        Err(error) => return failure(&error.to_string()),
    };
    let mut sink = AuthoritySink {
        original: original.snapshot(),
        admission,
        progress,
        accepted: 0,
    };
    let endpoint = owner.ready(original.snapshot()).await;
    let (execution, startup_error) = match endpoint {
        Ok(endpoint) => (
            Some(
                run_kimi(
                    original.snapshot(),
                    admission,
                    &endpoint,
                    KimiRequest {
                        session: graphit_ai::cli_kimi::KimiSessionStart::Create,
                        cwd: cwd.to_owned(),
                        model: Some(input.model.clone()),
                        prompt,
                    },
                    service.kimi_limits(),
                    &mut sink,
                )
                .await,
            ),
            None,
        ),
        Err(reason) => (None, Some(format!("{reason:?}"))),
    };
    let error = match &execution {
        Some(Ok(_)) => None,
        Some(Err(error)) => Some(format!("{:?}", error.reason())),
        None => startup_error,
    };
    if error.is_some() {
        owner.stop();
    }
    let cleanup_deadline = Instant::now()
        .checked_add(service.policy.process.cleanup)
        .unwrap_or_else(Instant::now);
    let observed = owner.observe(cleanup_deadline).await;
    let delivery = check_read(original.snapshot(), admission);
    let receipt = execution.as_ref().map(|result| match result {
        Ok(receipt) => receipt,
        Err(error) => error.receipt(),
    });
    let answer = (error.is_none() && observed.joined && observed.clean && delivery.is_ok())
        .then(|| receipt.and_then(KimiReceipt::eligible_text))
        .flatten();
    let status = if error.is_some() || !observed.joined || !observed.clean {
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
    let mut value = json!({"provider":"kimi","accepted_notifications":sink.accepted,
        "session_id":receipt.and_then(KimiReceipt::session),"prompt_id":receipt.and_then(KimiReceipt::prompt),
        "turn_id":receipt.and_then(KimiReceipt::turn),"server_version":receipt.and_then(KimiReceipt::version),
        "completion":completion,"process":{"joined":observed.joined,"clean":observed.clean,
        "status_code":observed.status_code,"cleanup":observed.cleanup.map(|value|format!("{value:?}")),
        "reason":observed.process_reason.map(|value|format!("{value:?}")),
        "stdout_eof":observed.stdout_eof,"stderr_eof":observed.stderr_eof,"driver_cancelled":observed.driver_cancelled}});
    // Failure payloads remain private in the native receipt; do not publish upstream messages.
    if error.is_none()
        && delivery.is_ok()
        && let Some(terminal) = receipt.and_then(KimiReceipt::terminal)
    {
        value["native_result_json"] = json!(terminal.get());
    }
    let payload = json!({"status":status,"generation":generation_value(&generation),
        "provided_slugs":documents.iter().map(|document|document.slug.as_str()).collect::<Vec<_>>(),
        "execution_error":error,"receipt":value});
    let response = if status == "failed" {
        crate::mcp_output::typed_error(payload)
    } else {
        crate::mcp_output::typed(payload)
    };
    service.reap().await;
    match (delivery, check_read(original.snapshot(), admission)) {
        (Ok(()), Ok(())) => response,
        _ => delivery_denied(response, "ProjectDeliveryDenied"),
    }
}
struct AuthoritySink<'a, 'admission, 'sink> {
    original: &'a Arc<SnapshotContext>,
    admission: &'a ProjectReadAdmission<'admission>,
    progress: &'a mut ProgressSink<'sink>,
    accepted: u64,
}
impl KimiSink for AuthoritySink<'_, '_, '_> {
    async fn push<'a>(
        &'a mut self,
        event: &'a RawValue,
        original: &'a SnapshotContext,
    ) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        check_read(self.original, self.admission).map_err(|_| io::ErrorKind::PermissionDenied)?;
        self.progress
            .emit(&ProgressEvent(event), original)
            .await
            .map_err(|_| io::ErrorKind::BrokenPipe)?;
        // This is the actual successful callback prefix consumed by the existing MCP gate.
        self.accepted = self
            .accepted
            .checked_add(1)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        check_read(self.original, self.admission)
            .map_err(|_| io::ErrorKind::PermissionDenied.into())
    }
}

pub(in crate::source_server) struct ProgressEvent<'a>(pub(in crate::source_server) &'a RawValue);
impl Serialize for ProgressEvent<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("provider", "kimi")?;
        map.serialize_entry("event", self.0)?;
        map.end()
    }
}
