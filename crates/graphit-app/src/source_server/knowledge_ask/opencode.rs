//! OpenCode one-shot Ask with the actual borrowed admission and retained child owner.
use super::*;
use crate::{
    knowledge_ask_service::AskService,
    knowledge_question::{DocumentaryQuestionContent, PreparedDocumentaryQuestion, check_read},
};
use graphit_ai::{
    cli_opencode::{OpencodePhase, OpencodeReceipt, OpencodeSink, run_opencode},
    cli_opencode_wire::OpencodeUpdate,
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
    let (transport, owner) = match service
        .start_opencode(original.snapshot(), admission, cwd, None)
        .await
    {
        Ok(started) => started,
        Err(error) => return failure(&error.to_string()),
    };
    let mut sink = AuthoritySink {
        original: original.snapshot(),
        admission,
        progress,
    };
    let execution = run_opencode(
        original.snapshot(),
        transport,
        graphit_ai::cli_opencode::OpencodeRequest {
            session: graphit_ai::cli_opencode::OpencodeSessionStart::Create,
            cwd: cwd.to_owned(),
            model: Some(input.model.clone()),
            agent: None,
            prompt,
        },
        service.opencode_limits(),
        &mut sink,
    )
    .await;
    // Custody was registered before run_opencode's first await; join borrows that same owner.
    let joined = service.join_opencode(&owner).await.unwrap_or(false);
    let clean = owner.clean().await;
    let (receipt, error) = match &execution {
        Ok(receipt) => (receipt, None),
        Err(error) => (error.receipt(), Some(format!("{:?}", error.reason()))),
    };
    let delivery = check_read(original.snapshot(), admission);
    let answer = (error.is_none() && joined && clean && delivery.is_ok())
        .then(|| receipt.eligible_text())
        .flatten();
    let status = if error.is_some() || !joined || !clean {
        "failed"
    } else {
        match answer {
            Some(text) if text.trim().is_empty() => "empty_answer",
            Some(_) => "answer",
            None => "evidence_unavailable",
        }
    };
    let mut value = receipt_value(receipt);
    let mut completion = json!({"status":status,"requested_model":input.model});
    if let Some(answer) = answer {
        completion["answer"] = json!(answer);
    }
    value["completion"] = completion;
    value["process"] = owner.public_observation().await;
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
fn receipt_value(receipt: &OpencodeReceipt) -> Value {
    // Semantic SDK responses only. Private pre-SDK bytes never enter this projection.
    let native = json!({"initialize":receipt.initialize(),"session":receipt.created(),
        "configured":receipt.configured(),"prompt_response":receipt.response()});
    json!({"provider":"opencode","observed_notifications":receipt.observed(),
        "accepted_notifications":receipt.accepted(),"denied_permissions":receipt.denied_permissions(),
        "sdk_driver_completed":receipt.driver().map(Result::is_ok),
        "cancel_enqueued":receipt.cancel_enqueue().map(Result::is_ok),
        "native_result_json":native.to_string()})
}
struct AuthoritySink<'a, 'admission, 'sink> {
    original: &'a Arc<SnapshotContext>,
    admission: &'a ProjectReadAdmission<'admission>,
    progress: &'a mut ProgressSink<'sink>,
}
impl OpencodeSink for AuthoritySink<'_, '_, '_> {
    fn recheck(&self, original: &SnapshotContext) -> io::Result<()> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        check_read(self.original, self.admission)
            .map_err(|_| io::ErrorKind::PermissionDenied.into())
    }
    fn session_bound(&mut self, _: &str, _: bool, original: &SnapshotContext) -> io::Result<()> {
        self.recheck(original)
    }
    async fn push<'a>(
        &'a mut self,
        phase: OpencodePhase,
        update: &'a OpencodeUpdate,
        original: &'a SnapshotContext,
    ) -> io::Result<()> {
        self.recheck(original)?;
        self.progress
            .emit(&ProgressEvent(phase, update), original)
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        self.recheck(original)
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
