//! Native Copilot Ask with the original Knowledge admission and registered Process owner.
use super::*;
use crate::{
    knowledge_ask_service::AskService,
    knowledge_question::{DocumentaryQuestionContent, PreparedDocumentaryQuestion, check_read},
};
use graphit_ai::cli_copilot::{
    CopilotReceipt, CopilotRequest, CopilotSessionStart, CopilotSink, run_copilot,
};
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
    if let Err(error) = check_read(original.snapshot(), admission) {
        return failure(&error.to_string());
    }
    let host = match service.host() {
        Ok(host) => host,
        Err(error) => return failure(&format!("Knowledge Ask host: {:?}", error.kind())),
    };
    let mut sink = AuthoritySink {
        original: original.snapshot(),
        admission,
        progress,
        accepted: 0,
    };
    let mut execution = run_copilot(
        original.snapshot(),
        admission,
        &host,
        CopilotRequest {
            session: CopilotSessionStart::Create,
            command: service.copilot_request(cwd),
            model: input.model.clone(),
            prompt,
        },
        service.copilot_limits(),
        &mut sink,
    )
    .await;
    match &mut execution {
        Ok(receipt) => service.retain_copilot(receipt),
        Err(error) => service.retain_copilot(error.receipt_mut()),
    }
    let error = execution
        .as_ref()
        .err()
        .map(|error| format!("{:?}", error.reason()));
    let delivery = check_read(original.snapshot(), admission);
    let receipt = Some(match &execution {
        Ok(receipt) => receipt,
        Err(error) => error.receipt(),
    });
    let answer = (error.is_none() && delivery.is_ok())
        .then(|| receipt.and_then(CopilotReceipt::eligible_text))
        .flatten();
    let status = if error.is_some() {
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
    let mut value = json!({"provider":"copilot","accepted_notifications":sink.accepted,
        "session_id":receipt.and_then(CopilotReceipt::session),"prompt_id":receipt.and_then(CopilotReceipt::prompt),
        "turn_id":receipt.and_then(CopilotReceipt::turn),"completion":completion});
    if let Some(process) = receipt.and_then(CopilotReceipt::process) {
        value["process"] = process_value(process);
    }
    // Failure payloads remain private in the native receipt; do not publish upstream messages.
    if error.is_none()
        && delivery.is_ok()
        && let Some(terminal) = receipt.and_then(CopilotReceipt::terminal)
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
impl CopilotSink for AuthoritySink<'_, '_, '_> {
    fn recheck(&self, original: &SnapshotContext) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        check_read(self.original, self.admission)
            .map_err(|_| io::ErrorKind::PermissionDenied.into())
    }
    fn session_bound(&mut self, _: &str, _: bool, original: &SnapshotContext) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        check_read(self.original, self.admission)
            .map_err(|_| io::ErrorKind::PermissionDenied.into())
    }
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

struct ProgressEvent<'a>(&'a RawValue);
impl Serialize for ProgressEvent<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("provider", "copilot")?;
        map.serialize_entry("event", self.0)?;
        map.end()
    }
}
