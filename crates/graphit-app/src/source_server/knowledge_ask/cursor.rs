//! Cursor one-shot Ask with the real borrowed PROJECT admission and Process custody.
use super::*;
use crate::{
    knowledge_ask_service::AskService,
    knowledge_question::{DocumentaryQuestionContent, PreparedDocumentaryQuestion, check_read},
};
use graphit_ai::{
    cli_cursor::{CursorReceipt, CursorSink, run_cursor},
    cli_cursor_wire::CursorEventView,
};
use graphit_auth::ProjectReadAdmission;
use std::{path::Path, sync::Arc};

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
    };
    let mut execution = run_cursor(
        original.snapshot(),
        &host,
        service.cursor_request(cwd, &input.model, prompt),
        service.cursor_limits(),
        &mut sink,
    )
    .await;
    // Transfer actual Remaining before formatting an error or projecting any native data.
    match &mut execution {
        Ok(receipt) => service.retain_cursor(receipt),
        Err(error) => service.retain_cursor(error.receipt_mut()),
    }
    let (receipt, error) = match &execution {
        Ok(receipt) => (receipt, None),
        Err(error) => (error.receipt(), Some(format!("{:?}", error.reason()))),
    };
    let delivery = check_read(original.snapshot(), admission);
    let answer = delivery.as_ref().ok().and_then(|()| receipt.answer());
    let status = if error.is_some() {
        "failed"
    } else {
        match answer {
            Some(text) if text.trim().is_empty() => "empty_answer",
            Some(_) => "answer",
            None => "evidence_unavailable",
        }
    };
    let mut completion = json!({"status": status, "requested_model": input.model});
    if let Some(answer) = answer {
        completion["answer"] = json!(answer);
    }
    let mut value = receipt_value(receipt);
    value["completion"] = completion;
    let payload = json!({"status":status,"generation":generation_value(&generation),
        "provided_slugs":documents.iter().map(|document| document.slug.as_str()).collect::<Vec<_>>(),
        "execution_error":error,"receipt":value});
    let result = if error.is_some() {
        crate::mcp_output::typed_error(payload)
    } else {
        crate::mcp_output::typed(payload)
    };
    service.reap().await;
    match (delivery, check_read(original.snapshot(), admission)) {
        (Ok(()), Ok(())) => result,
        _ => delivery_denied(result, "ProjectDeliveryDenied"),
    }
}
fn receipt_value(receipt: &CursorReceipt) -> Value {
    let counts = receipt.accepted();
    let mut value = json!({"provider":"cursor","known_events":receipt.known_events(),
        "frames":receipt.frames(),"classification":format!("{:?}",receipt.classification()),
        "accepted":{"stdout":counts.stdout,"stderr":counts.stderr},
        "native_result_json":receipt.native_result_json()});
    if let Some(init) = receipt.observed_init() {
        value["observed_init"] = json!({"session_id":init.session_id,"model":init.model});
    }
    if let Some(result) = receipt.observed_result() {
        value["observed_result"] = json!({"duration_ms":result.duration_ms, "duration_api_ms":result.duration_api_ms, "request_id":result.request_id});
    }
    if let Some(process) = receipt.process() {
        value["process"] = process_value(process);
    }
    value
}
struct AuthoritySink<'a, 'admission, 'sink> {
    original: &'a Arc<SnapshotContext>,
    admission: &'a ProjectReadAdmission<'admission>,
    progress: &'a mut ProgressSink<'sink>,
}
impl CursorSink for AuthoritySink<'_, '_, '_> {
    async fn push<'a>(
        &'a mut self,
        event: CursorEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(SinkFailure::Closed);
        }
        check_read(self.original, self.admission).map_err(|_| SinkFailure::Closed)?;
        self.progress.emit(&ProgressEvent(event), original).await?;
        check_read(self.original, self.admission).map_err(|_| SinkFailure::Closed)
    }
}
struct ProgressEvent<'a, 'native>(&'a graphit_ai::cli_cursor_wire::CursorEvent<'native>);
impl Serialize for ProgressEvent<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("provider", "cursor")?;
        map.serialize_entry("event", self.0.native)?;
        map.end()
    }
}
