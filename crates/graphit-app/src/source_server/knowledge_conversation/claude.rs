//! Claude continuation borrows the actual Incoming USER and PROJECT admissions.
use super::*;
use crate::{
    knowledge_ask_service::AskService,
    knowledge_conversation::ConversationDecisionGuard,
    knowledge_question::{DocumentaryQuestionContent, check_read, prepare_documentary_question},
};
use graphit_ai::{
    cli_claude::{ClaudeReceipt, ClaudeSink, run_claude},
    cli_claude_wire::{ClaudeEventData, ClaudeEventView},
};
use graphit_auth::{ProjectReadAdmission, UserKnowledgeReadAdmission, UserKnowledgeWriteAdmission};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::path::Path;

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
        provider: "claude".into(),
        engine: ConversationEngine::Claude,
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
    let mut request = input.service.claude_request(input.cwd, input.model, prompt);
    if let Err(error) = custody.configure_claude(&mut request, input.original.snapshot()) {
        custody.release_unsubmitted();
        decision.decided();
        return failure(&error.to_string());
    }
    if sink.check().is_err() {
        custody.release_unsubmitted();
        decision.decided();
        return failure(
            "Knowledge conversation authorization changed while waiting for USER writer",
        );
    }
    sink.custody = Some(Arc::clone(&custody));
    decision.submitted();
    let mut execution = run_claude(
        input.original.snapshot(),
        &host,
        request,
        input.service.claude_limits(),
        &mut sink,
    )
    .await;
    // No projection or await may precede transfer of the actual Process receipt.
    match &mut execution {
        Ok(receipt) => custody.retain_claude(receipt),
        Err(error) => custody.retain_claude(error.receipt_mut()),
    }
    if let Err(error) = &execution {
        custody.release_pre_submit_claude_failure(error);
    }
    let allowed = execution.is_ok() && sink.check().is_ok();
    let persistence = custody.finish(input.original.snapshot(), allowed);
    decision.decided();
    let (receipt, error) = match &execution {
        Ok(receipt) => (receipt, None),
        Err(error) => (error.receipt(), Some(format!("{:?}", error.reason()))),
    };
    let answer = allowed.then(|| receipt.answer()).flatten();
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
    let mut native = receipt_value(receipt);
    native["completion"] = completion;
    let mut payload = json!({"status":status,"generation":knowledge_ask::generation_value(&generation),"provided_slugs":documents.iter().map(|d|d.slug.as_str()).collect::<Vec<_>>(),"execution_error":error,"receipt":native});
    let persisted = match persistence {
        ConversationPersistence::Persisted(record) => {
            payload["persistence"] = json!({"status":"persisted","conversation":record});
            true
        }
        ConversationPersistence::Failed(error) => {
            payload["persistence"] = json!({"status":"failed","error":error.to_string()});
            false
        }
        ConversationPersistence::Pending => {
            payload["persistence"] = json!({"status":"pending_process"});
            false
        }
        ConversationPersistence::NotStarted => {
            payload["persistence"] = json!({"status":"not_started"});
            false
        }
        ConversationPersistence::Denied => {
            payload["persistence"] = json!({"status":"denied"});
            false
        }
    };
    let response = if persisted && error.is_none() {
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
fn receipt_value(receipt: &ClaudeReceipt) -> Value {
    let counts = receipt.accepted();
    let mut value = json!({"provider":"claude","frames":receipt.frames(),"known_events":receipt.known_events(),"classification":format!("{:?}",receipt.classification()),"accepted":{"stdout":counts.stdout,"stderr":counts.stderr},"native_result_json":receipt.native_result_json()});
    if let Some(init) = receipt.observed_init() {
        value["observed_init"] = json!({"session_id":init.session_id,"model":init.model});
    }
    if let Some(process) = receipt.process() {
        value["process"] = knowledge_ask::process_value(process);
    }
    value
}
struct Sink<'a, 'user, 'project> {
    original: &'a Arc<graphit_core::file_io::SnapshotContext>,
    project: &'a ProjectReadAdmission<'project>,
    user_read: &'a UserKnowledgeReadAdmission<'user>,
    user_write: &'a UserKnowledgeWriteAdmission<'user>,
    stop: &'a CancellationToken,
    custody: Option<Arc<ConversationCustodyEntry>>,
    progress: knowledge_ask::ProgressSink<'static>,
}
impl Sink<'_, '_, '_> {
    fn check(&self) -> Result<(), SinkFailure> {
        if self.stop.is_cancelled() {
            return Err(SinkFailure::Closed);
        }
        self.user_read.recheck().map_err(|_| SinkFailure::Closed)?;
        self.user_write.recheck().map_err(|_| SinkFailure::Closed)?;
        check_read(self.original, self.project).map_err(|_| SinkFailure::Closed)
    }
}
impl ClaudeSink for Sink<'_, '_, '_> {
    async fn push<'a>(
        &'a mut self,
        event: ClaudeEventView<'a>,
        original: &'a graphit_core::file_io::SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if !std::ptr::eq(self.original.as_ref(), original) {
            return Err(SinkFailure::Closed);
        }
        self.check()?;
        if let ClaudeEventData::Init(init) = &event.data
            && !self
                .custody
                .as_ref()
                .expect("turn held before native execution")
                .observe_claude(&init.session_id, self.original)
        {
            return Err(SinkFailure::ConsumerFailed);
        }
        self.progress.emit(&ProgressEvent(event), original).await?;
        self.check()
    }
}
struct ProgressEvent<'a, 'native>(&'a graphit_ai::cli_claude_wire::ClaudeEvent<'native>);
impl Serialize for ProgressEvent<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("provider", "claude")?;
        map.serialize_entry("event", self.0.native)?;
        map.end()
    }
}
