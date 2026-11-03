//! Authenticated Ask dispatch and the public native-event projection.
use super::*;
use crate::knowledge_question::{
    KnowledgeQuestionDisposition, KnowledgeQuestionError, KnowledgeQuestionModel,
    KnowledgeQuestionResult, prepare_documentary_question,
};
use graphit_ai::{
    cli_codex::{CodexReceipt, CodexSink, CodexStreamLimits, run_codex},
    cli_codex_wire::*,
    codex_completion::{AttemptPurpose, CompletionOutcome, UsageEvidence},
};
use graphit_auth::ProjectRecordAuthority;
use graphit_core::file_io::SnapshotContext;
use graphit_process::SinkFailure;
use rmcp::{
    model::{ProgressNotificationParam, ProgressToken},
    service::Peer,
};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::value::RawValue;

pub(in crate::source_server) mod antigravity;
mod claude;
mod copilot;
mod cursor;
pub(in crate::source_server) mod grok;
pub(in crate::source_server) mod kimi;
mod opencode;
mod qwen;

pub(super) const TOOL: &str = "graphit_knowledge_ask";
pub(super) fn available(state: &State) -> bool {
    state.ask.as_ref().and_then(|ask| ask.service()).is_some()
}
pub(super) fn definition() -> Tool {
    Tool::new(TOOL, "Answer a question from explicitly selected slugs or ranked retrieval in one authorized PROJECT Knowledge corpus using the startup Antigravity, Claude, Codex, Cursor, Grok, Qwen, Kimi, Copilot, Kiro or OpenCode completion provider. Progress contains observed native events; provided slugs and requested model do not confirm model citations or execution identity.",
        json!({"type":"object","properties":{"query":{"type":"string"},"selected_slugs":{"type":"array","items":{"type":"string"}},"model":{"type":"string"},"retrieval":{"type":"object","properties":{"mode":{"type":"string","enum":["fts","semantic","hybrid"]},"top_k":{"type":"integer","minimum":1},"metric":{"type":"string","enum":["cosine","l2","dot"]}},"required":["mode","top_k"],"additionalProperties":false},"project_dir":{"type":"string"},"project_version":{"type":"string"}},"required":["query","model"],"oneOf":[{"required":["selected_slugs"]},{"required":["retrieval"]}],"additionalProperties":false}).as_object().expect("object schema").clone())
}
#[derive(Deserialize)]
#[serde(try_from = "AskWire")]
pub(crate) struct AskArguments {
    input: crate::foreground_knowledge::ask::Input,
    project_dir: Option<String>,
    project_version: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskWire {
    query: String,
    #[serde(default, deserialize_with = "present")]
    selected_slugs: Option<Vec<String>>,
    model: String,
    #[serde(default, deserialize_with = "present")]
    retrieval: Option<crate::foreground_knowledge::ask::Retrieval>,
    #[serde(default, deserialize_with = "present")]
    project_dir: Option<String>,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
}
impl TryFrom<AskWire> for AskArguments {
    type Error = &'static str;
    fn try_from(wire: AskWire) -> Result<Self, Self::Error> {
        let input = crate::foreground_knowledge::ask::Input {
            query: wire.query,
            selected_slugs: wire.selected_slugs,
            model: wire.model,
            retrieval: wire.retrieval,
        };
        input.validate_selection()?;
        Ok(Self {
            input,
            project_dir: wire.project_dir,
            project_version: wire.project_version,
        })
    }
}
impl AskArguments {
    fn select(
        &self,
        snapshot: &crate::live_project_selections::LiveProjectSnapshot,
    ) -> Result<usize, &'static str> {
        let current = snapshot.current.ok_or("PROJECT selection unavailable")?;
        if self
            .project_dir
            .as_deref()
            .is_some_and(|value| value.is_empty() || value.contains('\0'))
            || self
                .project_version
                .as_deref()
                .is_some_and(|value| value.is_empty() || value.contains('\0'))
        {
            return Err("Knowledge Ask project selection invalid");
        }
        let root = self
            .project_dir
            .as_deref()
            .map(Path::new)
            .unwrap_or(snapshot.projects[current].0.root());
        if self.project_version.is_none() && root == snapshot.projects[current].0.root() {
            return Ok(current);
        }
        select_project(&snapshot.projects, root, self.project_version.as_deref())
    }
}
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input: AskArguments =
        match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
            Ok(input) => input,
            Err(_) => return failure("Knowledge Ask arguments invalid"),
        };
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return failure("PROJECT selection unavailable"),
    };
    let selected = match input.select(&snapshot) {
        Ok(index) => index,
        Err(message) => return failure(message),
    };
    let (paths, id, version) = &snapshot.projects[selected];
    if !matches!(
        server.0.live_projects.is_active(paths, *id, version),
        Ok(true)
    ) {
        return failure("Knowledge PROJECT selection unavailable");
    }
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("Knowledge Ask incoming HTTP credentials required");
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|value| value.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure("Knowledge Ask credential origin missing or unsupported"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
    else {
        return failure("Knowledge Ask bearer credential required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let (send, receive) = oneshot::channel();
    let mut sink = ProgressSink::mcp(context.peer, context.meta.get_progress_token());
    let handle = Handle::current();
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = context.ct.cancelled() => return failure("Knowledge Ask request cancelled"),
            () = state.stop.cancelled() => return failure("Knowledge Ask service stopped"),
            () = tokio::time::sleep_until(deadline.into()) => return failure("Knowledge Ask request deadline expired"),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("Knowledge Ask service stopped");
        }
        let owner = Arc::clone(&state);
        // Native captures/Data reads run on the already-owned blocking executor.
        // Cancellation of the HTTP waiter cannot drop this execution or its receipt.
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(execute(
                &owner,
                &snapshot.projects[selected],
                input,
                &credential,
                local,
                &original_cancel,
                deadline,
                &mut sink,
            ));
            let _ = send.send(result);
        });
    }
    tokio::select! {
        result = receive => match result {
            Ok(result) => {
                let denial = if context.ct.is_cancelled() || cancel.is_cancelled() {
                    Some("Cancelled")
                } else if state.stop.is_cancelled() {
                    Some("ServiceStopped")
                } else if Instant::now() >= deadline {
                    Some("Deadline")
                } else { None };
                match denial { Some(reason) => delivery_denied(result, reason), None => result }
            }
            Err(_) => failure("Knowledge Ask native operation failed"),
        },
        () = context.ct.cancelled() => failure("Knowledge Ask request cancelled"),
        () = state.stop.cancelled() => failure("Knowledge Ask service stopped"),
        () = tokio::time::sleep_until(deadline.into()) => failure("Knowledge Ask request deadline expired"),
    }
}

#[allow(clippy::too_many_arguments)] // Distinct admitted request, credential and transport owners.
async fn execute(
    state: &State,
    selection: &(
        graphit_core::project_paths::ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
    input: AskArguments,
    credential: &str,
    local: bool,
    cancel: &Cancellation,
    deadline: Instant,
    sink: &mut ProgressSink<'_>,
) -> CallToolResult {
    let original = match RequestContext::new(state.policy.content, cancel, deadline) {
        Ok(original) => original,
        Err(_) => return failure("Knowledge Ask request context invalid"),
    };
    let caller = if local {
        state
            .auth
            .issuer()
            .authenticate_local(credential, &original)
    } else {
        state
            .auth
            .issuer()
            .authenticate_runtime(credential, &original)
    };
    let caller = match caller {
        Ok(caller) => caller,
        Err(_) => return failure("Knowledge Ask authentication or permission denied"),
    };
    let (paths, id, version) = selection;
    let live = match CapturedPublicationScope::project(
        paths,
        *id,
        state.policy.scope,
        original.snapshot(),
    ) {
        Ok(live) => live,
        Err(error) => {
            return failure(&format!(
                "Knowledge Ask project capture: {:?}",
                error.kind()
            ));
        }
    };
    let record = match live.binding() {
        Ok(record) => record,
        Err(error) => {
            return failure(&format!(
                "Knowledge Ask project binding: {:?}",
                error.kind()
            ));
        }
    };
    let scope = ScopeInput {
        installation: state.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: state.policy.binding,
        global_version: "",
    };
    let authority = ProjectRecordAuthority::new(state.auth.issuer());
    // Optional startup failure is returned only after authentication and project admission.
    let admission = match authority.admit_authenticated_read(
        &caller,
        &original,
        scope,
        graphit_auth::ProjectReadModule::Knowledge,
    ) {
        Ok(admission) => admission,
        Err(error) => return failure(&format!("Knowledge Ask authorization: {error}")),
    };
    let Some(availability) = state.ask.as_ref() else {
        return failure(
            "Knowledge Ask completion provider was not captured; configure a supported ai.cli and restart the service",
        );
    };
    let Some(service) = availability.service() else {
        return failure(&availability.unavailable_message());
    };
    let needs_model =
        input.input.retrieval.as_ref().is_some_and(|retrieval| {
            retrieval.mode != crate::foreground_knowledge::search::Mode::Fts
        });
    let mut resources = if needs_model {
        match state.semantic_model() {
            Ok(resources) => resources,
            Err(error) => return failure(&error.to_string()),
        }
    } else {
        None
    };
    if resources.is_some() {
        let Some(hook) = state.hook.as_ref() else {
            return failure("Knowledge Ask project configuration unavailable");
        };
        let modules =
            match hook.selected_modules(&state.policy.installation, paths, original.snapshot()) {
                Ok(modules) => modules,
                Err(error) => return failure(&error.to_string()),
            };
        if !modules.enabled(graphit_core::module_settings::Module::Embedding) {
            resources = None;
        }
    }
    let mut embedding_service = match resources.as_ref() {
        Some(_) => match state
            .auth
            .issuer()
            .capture_service(&caller, graphit_auth::ServicePurpose::Embedding)
        {
            Ok(service) => Some(service),
            Err(error) => return failure(&error.to_string()),
        },
        None => None,
    };
    execute_admitted(
        service,
        &original,
        &admission,
        paths.root(),
        &input.input,
        QuestionRetrieval {
            content: state.policy.content,
            model: resources.as_deref().zip(embedding_service.as_mut()).map(
                |(resources, service)| crate::foreground_knowledge::search::QuestionModel {
                    resources,
                    service,
                    rpc_timeout: state.policy.timeout,
                },
            ),
        },
        sink,
    )
    .await
}

pub(crate) struct QuestionRetrieval<'a, 'lease> {
    pub(crate) content: graphit_hub::native::ContentLimits,
    pub(crate) model: Option<crate::foreground_knowledge::search::QuestionModel<'a, 'lease>>,
}

/// Execute one question with the caller's actual read grant; authentication stays at ingress.
pub(crate) async fn execute_admitted(
    service: &crate::knowledge_ask_service::AskService,
    original: &RequestContext<'_>,
    admission: &graphit_auth::ProjectReadAdmission<'_>,
    cwd: &Path,
    input: &crate::foreground_knowledge::ask::Input,
    retrieval: QuestionRetrieval<'_, '_>,
    sink: &mut ProgressSink<'_>,
) -> CallToolResult {
    service.reap().await;
    if let Err(error) = input.validate(retrieval.content) {
        return failure(error);
    }
    if input.model.len() > service.policy.completion.max_model_id_bytes {
        return failure("Knowledge Ask model id limit");
    }
    let (prepared, observation) = if let Some(selection) = &input.retrieval {
        if selection.top_k.get() > service.policy.question.max_selected_pages.get() {
            return failure("Knowledge Ask selected pages limit");
        }
        let limits = graphit_data::knowledge_search::KnowledgeSearchLimits {
            read: service.policy.question.read,
            max_query_bytes: service.policy.question.max_query_bytes,
            max_output_bytes: match std::num::NonZeroUsize::new(retrieval.content.max_output_bytes)
            {
                Some(limit) => limit,
                None => return failure("Knowledge Ask response policy invalid"),
            },
        };
        let preview = match crate::foreground_knowledge::search::preview_limits(retrieval.content) {
            Ok(preview) => preview,
            Err(error) => return failure(&error.to_string()),
        };
        let result = match crate::foreground_knowledge::search::retrieve_question(
            original,
            admission,
            &input.query,
            selection,
            limits,
            preview,
            retrieval.model,
        )
        .await
        {
            Ok(result) => result,
            Err(error) => return failure(&format!("Knowledge Ask retrieval: {error}")),
        };
        let Some(documents) = result.selected else {
            let status = result
                .observation
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("unavailable");
            return crate::mcp_output::typed(
                json!({"status":status,"retrieval":result.observation}),
            );
        };
        let prepared = match crate::knowledge_question::prepare_selected_documentary_question(
            original.snapshot(),
            admission,
            &input.query,
            documents,
            service.policy.question,
        ) {
            Ok(prepared) => prepared,
            Err(error) => return failure(&error.to_string()),
        };
        (prepared, Some(result.observation))
    } else {
        let slugs: Vec<_> = input
            .selected_slugs
            .as_deref()
            .expect("validated explicit selection")
            .iter()
            .map(String::as_str)
            .collect();
        let prepared = match prepare_documentary_question(
            original.snapshot(),
            admission,
            &input.query,
            &slugs,
            &input.model,
            service.policy.completion.max_model_id_bytes,
            service.policy.question,
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(error) => return failure(&error.to_string()),
        };
        (prepared, None)
    };
    let result = execute_prepared(service, original, admission, cwd, input, prepared, sink).await;
    if let Some(observation) = observation {
        let mut payload = result
            .structured_content
            .unwrap_or_else(|| json!({"error_content":result.content}));
        payload["retrieval"] = observation;
        if result.is_error == Some(true) {
            crate::mcp_output::typed_error(payload)
        } else {
            crate::mcp_output::typed(payload)
        }
    } else {
        result
    }
}

async fn execute_prepared(
    service: &crate::knowledge_ask_service::AskService,
    original: &RequestContext<'_>,
    admission: &graphit_auth::ProjectReadAdmission<'_>,
    cwd: &Path,
    input: &crate::foreground_knowledge::ask::Input,
    prepared: crate::knowledge_question::PreparedDocumentaryQuestion<'_, '_, '_>,
    sink: &mut ProgressSink<'_>,
) -> CallToolResult {
    if service.is_copilot() {
        return copilot::execute(service, original, admission, cwd, input, prepared, sink).await;
    }
    if service.is_kimi() {
        return kimi::execute(service, original, admission, cwd, input, prepared, sink).await;
    }
    if service.is_opencode() {
        return opencode::execute(service, original, admission, cwd, input, prepared, sink).await;
    }
    if service.is_grok() {
        return grok::execute(service, original, admission, cwd, input, prepared, sink).await;
    }
    if service.is_antigravity() {
        return antigravity::execute(service, original, admission, cwd, input, prepared, sink)
            .await;
    }
    if service.is_cursor() {
        return cursor::execute(service, original, admission, cwd, input, prepared, sink).await;
    }
    if service.is_qwen() {
        return qwen::execute(service, original, admission, cwd, input, prepared, sink).await;
    }
    if service.is_claude() {
        return claude::execute(service, original, admission, cwd, input, prepared, sink).await;
    }
    if service.kiro_engine().is_some() {
        let content = match prepared.into_execution() {
            Ok(v) => v,
            Err(e) => return failure(&e.to_string()),
        };
        let crate::knowledge_question::DocumentaryQuestionContent::Ready {
            generation,
            documents,
            prompt,
        } = content
        else {
            let crate::knowledge_question::DocumentaryQuestionContent::Empty(empty) = content
            else {
                unreachable!()
            };
            return terminal(empty);
        };
        let allowed =
            || crate::knowledge_question::check_read(original.snapshot(), admission).is_ok();
        let (transport, owner) = match service
            .kiro_process(cwd, original.snapshot(), None, &allowed)
            .await
        {
            Ok(v) => v,
            Err(e) => return failure(&format!("Knowledge Ask Kiro launch: {:?}", e.kind())),
        };
        let mut guarded = KiroAuthoritySink {
            progress: sink,
            allowed: &allowed,
        };
        let execution = graphit_ai::cli_kiro::run_kiro(
            original.snapshot(),
            transport,
            service.kiro_engine().expect("Kiro binding"),
            service.kiro_new_session(cwd, &input.model),
            service.kiro_prompt(prompt, &input.model),
            service.kiro_limits(),
            &mut guarded,
        )
        .await;
        let joined = service.join_kiro(&owner).await.unwrap_or(false);
        let clean = owner.clean_joined().await;
        let response = kiro_terminal(
            &generation,
            &documents,
            &execution,
            &owner,
            &input.model,
            joined && clean && allowed(),
        )
        .await;
        service.reap().await;
        return if allowed() {
            response
        } else {
            delivery_denied(response, "ProjectDeliveryDenied")
        };
    }
    let host = match service.host() {
        Ok(host) => host,
        Err(error) => return failure(&format!("Knowledge Ask host: {:?}", error.kind())),
    };
    let prepared = prepared.into_codex(KnowledgeQuestionModel {
        requested_model: &input.model,
        wire_limits: &service.policy.wire,
        completion_limits: service.policy.completion,
    });
    let mut result = match prepared {
        Ok(prepared) => {
            prepared
                .execute(|mut run, prompt| async move {
                    let mut request = service.request(cwd, &input.model);
                    request.stdin = Some(prompt.into_bytes().into());
                    run_codex(
                        &mut run,
                        &host,
                        request,
                        AttemptPurpose::Request,
                        None,
                        CodexStreamLimits {
                            stdout_bytes: service.policy.process.stdout_bytes,
                            stderr_bytes: service.policy.process.stderr_bytes,
                        },
                        sink,
                    )
                    .await
                })
                .await
        }
        Err(error) => Err(error),
    };
    // Synchronous transfer precedes serialization and every subsequent cancellable await.
    match &mut result {
        Ok(KnowledgeQuestionResult::Executed { receipt, .. }) => service.retain(receipt),
        Err(error) => {
            if let Some(execution) = error.execution_mut() {
                service.retain(match execution {
                    Ok(receipt) => receipt,
                    Err(failure) => failure.receipt_mut(),
                });
            }
        }
        _ => (),
    }
    service.reap().await;
    let response = match result {
        Ok(result) => terminal(result),
        Err(error) => {
            let mut payload = json!({"status":"failed","error":error.to_string()});
            {
                payload["category"] = json!(error.error().to_string());
                if let Some(execution) = error.execution() {
                    let receipt = match execution {
                        Ok(receipt) => receipt,
                        Err(error) => {
                            payload["execution_error"] = json!(format!("{:?}", error.reason()));
                            error.receipt()
                        }
                    };
                    payload["receipt"] = receipt_value(
                        receipt,
                        matches!(error.error(), KnowledgeQuestionError::Codex),
                    );
                }
            }
            crate::mcp_output::typed_error(payload)
        }
    };
    // Projection can consume time too. Preserve the already-owned receipt while
    // denying delivery after cancellation/deadline of the same Original.
    match original.snapshot().check() {
        Ok(()) => response,
        Err(error) => delivery_denied(response, &format!("{:?}", error.kind())),
    }
}
fn usage_value(usage: UsageEvidence) -> Value {
    match usage {
        UsageEvidence::Observed(value) => {
            json!({"provenance":"observed","input_tokens":value.input_tokens,"cached_input_tokens":value.cached_input_tokens,"output_tokens":value.output_tokens,"reasoning_output_tokens":value.reasoning_output_tokens,"cache_write_input_tokens":value.cache_write_input_tokens})
        }
        UsageEvidence::Estimated(value) => {
            json!({"provenance":"estimated","input_text_bytes":value.input_text_bytes,"output_text_bytes":value.output_text_bytes,"input_tokens":value.input_tokens(),"output_tokens":value.output_tokens(),"estimator":format!("{:?}",value.estimator()),"coverage":format!("{:?}",value.coverage())})
        }
        UsageEvidence::Unavailable(reason) => {
            json!({"provenance":"unavailable","reason":format!("{reason:?}")})
        }
    }
}
pub(crate) fn delivery_denied(response: CallToolResult, reason: &str) -> CallToolResult {
    let mut payload = response
        .structured_content
        .unwrap_or_else(|| json!({"status":"failed", "error_content":response.content}));
    payload["status"] = json!("failed");
    payload["delivery_error"] = json!(reason);
    suppress_answer(&mut payload);
    crate::mcp_output::typed_error(payload)
}
fn suppress_answer(payload: &mut Value) {
    if let Some(receipt) = payload.get_mut("receipt").and_then(Value::as_object_mut) {
        // Native terminal projections can include answer text.
        receipt.remove("native_result_json");
        receipt.remove("native_result");
    }

    if let Some(completion) = payload
        .get_mut("receipt")
        .and_then(|receipt| receipt.get_mut("completion"))
        .and_then(Value::as_object_mut)
    {
        completion.remove("answer");
    }
}
pub(super) fn receipt_value(receipt: &CodexReceipt<'_>, allow_answer: bool) -> Value {
    let counts = receipt.accepted();
    let mut value = json!({"usage":usage_value(receipt.usage()),"frames":receipt.frames(),"known_events":receipt.known_events(),"classification":format!("{:?}",receipt.classification()),"accepted":{"stdout":counts.stdout,"stderr":counts.stderr}});
    if let Some(completion) = receipt.completion() {
        let mut completion = match completion {
            CompletionOutcome::Answer(answer) => {
                let end = answer.end();
                json!({"status":"answer","answer":answer.text(),"requested_model":end.requested_model(),"ordinal":end.ordinal(),"purpose":format!("{:?}",end.purpose()),"usage":usage_value(end.usage()),"native_rpc_calls":"unobserved"})
            }
            CompletionOutcome::EvidenceUnavailable(end) => {
                json!({"status":"evidence_unavailable","requested_model":end.requested_model(),"ordinal":end.ordinal(),"purpose":format!("{:?}",end.purpose()),"reason":format!("{:?}",end.reason()),"usage":usage_value(end.usage()),"native_rpc_calls":"unobserved"})
            }
            CompletionOutcome::Failed(error) => {
                json!({"status":"failed","kind":format!("{:?}",error.kind()),"io_kind":error.io_kind().map(|kind|format!("{kind:?}")),"ordinal":error.ordinal(),"purpose":error.purpose().map(|purpose|format!("{purpose:?}")),"usage":usage_value(error.usage()),"native_rpc_calls":"unobserved"})
            }
        };
        if !allow_answer && let Some(completion) = completion.as_object_mut() {
            completion.remove("answer");
        }
        value["completion"] = completion;
    }
    if let Some(process) = receipt.process() {
        value["process"] = process_value(process);
    }
    value
}
pub(super) fn process_value(
    process: &Result<graphit_process::Outcome, graphit_process::Failure>,
) -> Value {
    let (status, cleanup, pipes, observed, delivered, reason) = match process {
        Ok(process) => (
            Some(process.status),
            process.cleanup,
            process.pipes,
            process.observed,
            process.delivered,
            None,
        ),
        Err(error) => (
            error.status,
            error.cleanup,
            error.pipes,
            error.observed,
            error.delivered,
            Some(format!("{:?}", error.reason)),
        ),
    };
    json!({"exit_code":status.and_then(|status|status.code()),"status_success":status.map(|status|status.success()),"cleanup":format!("{cleanup:?}"),"stdout_end":format!("{:?}",pipes.stdout),"stderr_end":format!("{:?}",pipes.stderr),"observed":{"stdout":observed.stdout,"stderr":observed.stderr},"delivered":{"stdout":delivered.stdout,"stderr":delivered.stderr},"reason":reason})
}
pub(super) fn generation_value(
    generation: &graphit_data::knowledge_index::KnowledgeGeneration,
) -> Value {
    json!({"corpus_uri":generation.corpus_uri,"version":generation.version,"physical_branch":generation.physical_branch,"inherited_target":generation.inherited_target})
}
pub(super) fn terminal(result: KnowledgeQuestionResult<'_>) -> CallToolResult {
    let (payload, error) = match result {
        KnowledgeQuestionResult::NoCorpus => (json!({"status":"no_corpus"}), false),
        KnowledgeQuestionResult::PublishedEmpty { generation } => (
            json!({"status":"published_empty","generation":generation_value(&generation)}),
            false,
        ),
        KnowledgeQuestionResult::EmptySelection { generation } => (
            json!({"status":"empty_selection","generation":generation_value(&generation)}),
            false,
        ),
        KnowledgeQuestionResult::Executed {
            generation,
            provided_pages,
            receipt,
            disposition,
        } => {
            let status = match disposition {
                KnowledgeQuestionDisposition::Answer => "answer",
                KnowledgeQuestionDisposition::EmptyAnswer => "empty_answer",
                KnowledgeQuestionDisposition::EvidenceUnavailable => "evidence_unavailable",
                KnowledgeQuestionDisposition::Failed => "failed",
                KnowledgeQuestionDisposition::NoCompletionObservation => {
                    "no_completion_observation"
                }
            };
            (
                json!({"status":status,"generation":generation_value(&generation),"provided_slugs":provided_pages.iter().map(|page|page.slug.as_str()).collect::<Vec<_>>(),"receipt":receipt_value(&receipt, true)}),
                matches!(
                    disposition,
                    KnowledgeQuestionDisposition::Failed
                        | KnowledgeQuestionDisposition::NoCompletionObservation
                ),
            )
        }
    };
    if error {
        crate::mcp_output::typed_error(payload)
    } else {
        crate::mcp_output::typed(payload)
    }
}

enum ProgressTarget {
    Mcp {
        peer: Peer<RoleServer>,
        token: Option<ProgressToken>,
    },
    Browser(tokio::sync::mpsc::Sender<axum::response::sse::Event>),
}
pub(crate) struct ProgressSink<'a> {
    target: ProgressTarget,
    ordinal: u64,
    allowed: Option<&'a (dyn Fn() -> bool + Sync)>,
}
impl ProgressSink<'_> {
    pub(super) fn mcp(peer: Peer<RoleServer>, token: Option<ProgressToken>) -> Self {
        Self {
            target: ProgressTarget::Mcp { peer, token },
            ordinal: 0,
            allowed: None,
        }
    }
    pub(crate) fn browser<'a>(
        send: tokio::sync::mpsc::Sender<axum::response::sse::Event>,
        allowed: &'a (dyn Fn() -> bool + Sync),
    ) -> ProgressSink<'a> {
        ProgressSink {
            target: ProgressTarget::Browser(send),
            ordinal: 0,
            allowed: Some(allowed),
        }
    }
    fn check(&self, original: &SnapshotContext) -> Result<(), SinkFailure> {
        original.check().map_err(|_| SinkFailure::Closed)?;
        if self.allowed.is_some_and(|allowed| !allowed()) {
            return Err(SinkFailure::Closed);
        }
        Ok(())
    }
    pub(super) async fn emit(
        &mut self,
        event: &(impl Serialize + Sync),
        original: &SnapshotContext,
    ) -> Result<(), SinkFailure> {
        self.check(original)?;
        self.ordinal = self
            .ordinal
            .checked_add(1)
            .ok_or(SinkFailure::ConsumerFailed)?;
        if matches!(self.target, ProgressTarget::Mcp { token: None, .. }) {
            return Ok(());
        }
        let message = serde_json::to_string(event).map_err(|_| SinkFailure::ConsumerFailed)?;
        self.check(original)?;
        let deadline = original
            .required_deadline()
            .map_err(|_| SinkFailure::Closed)?;
        let delivery = async {
            match &self.target {
                ProgressTarget::Mcp {
                    peer,
                    token: Some(token),
                } => peer
                    .notify_progress(
                        ProgressNotificationParam::new(token.clone(), self.ordinal as f64)
                            .with_message(message),
                    )
                    .await
                    .map_err(|_| SinkFailure::Closed),
                ProgressTarget::Mcp { token: None, .. } => Ok(()),
                ProgressTarget::Browser(send) => send
                    .send(
                        axum::response::sse::Event::default()
                            .event("progress")
                            .data(message),
                    )
                    .await
                    .map_err(|_| SinkFailure::Closed),
            }
        };
        tokio::select! {
            result = delivery => result?,
            () = original.cancellation().cancelled() => return Err(SinkFailure::Closed),
            () = tokio::time::sleep_until(deadline.into()) => return Err(SinkFailure::Closed),
        }
        self.check(original)
    }
}
impl CodexSink for ProgressSink<'_> {
    async fn push<'a>(
        &'a mut self,
        event: CodexEventView<'a>,
        original: &'a SnapshotContext,
    ) -> Result<(), SinkFailure> {
        self.emit(&NativeEvent(&event), original).await
    }
}

/// Serialize borrowed, decoder-validated raw JSON without numeric coercion.
struct Raw<'v, 'n>(&'v JsonView<'n>);
impl Serialize for Raw<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let raw = serde_json::from_str::<&RawValue>(self.0.as_str())
            .map_err(serde::ser::Error::custom)?;
        raw.serialize(serializer)
    }
}
struct RawList<'v, 'n>(&'v CodexRawJsonList<'n>);
impl Serialize for RawList<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for value in self.0.iter() {
            sequence.serialize_element(&Raw(&value))?;
        }
        sequence.end()
    }
}
struct NativeEvent<'v, 'n>(&'v CodexEventView<'n>);
impl Serialize for NativeEvent<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match self.0 {
            CodexEventView::ThreadStarted { thread_id } => {
                map.serialize_entry("type", "thread.started")?;
                map.serialize_entry("thread_id", thread_id)?;
            }
            CodexEventView::TurnStarted => map.serialize_entry("type", "turn.started")?,
            CodexEventView::TurnCompleted { usage } => {
                map.serialize_entry("type", "turn.completed")?;
                map.serialize_entry("usage",&json!({"input_tokens":usage.input_tokens,"cached_input_tokens":usage.cached_input_tokens,"output_tokens":usage.output_tokens,"reasoning_output_tokens":usage.reasoning_output_tokens,"cache_write_input_tokens":usage.cache_write_input_tokens}))?;
            }
            CodexEventView::TurnFailed { message } => {
                map.serialize_entry("type", "turn.failed")?;
                map.serialize_entry("message", message)?;
            }
            CodexEventView::Error { message } => {
                map.serialize_entry("type", "error")?;
                map.serialize_entry("message", message)?;
            }
            CodexEventView::Item { phase, item } => {
                map.serialize_entry(
                    "type",
                    match phase {
                        CodexItemPhase::Started => "item.started",
                        CodexItemPhase::Updated => "item.updated",
                        CodexItemPhase::Completed => "item.completed",
                    },
                )?;
                map.serialize_entry("item", &NativeItem(item))?;
            }
        }
        map.end()
    }
}
struct NativeItem<'v, 'n>(&'v CodexItemView<'n>);
fn status(value: CodexStatus) -> &'static str {
    match value {
        CodexStatus::InProgress => "in_progress",
        CodexStatus::Completed => "completed",
        CodexStatus::Failed => "failed",
        CodexStatus::Declined => "declined",
    }
}
impl Serialize for NativeItem<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("outer_id", self.0.outer_id)?;
        match &self.0.body {
            CodexItemBodyView::AgentMessage { text } | CodexItemBodyView::Reasoning { text } => {
                map.serialize_entry(
                    "type",
                    if matches!(self.0.body, CodexItemBodyView::AgentMessage { .. }) {
                        "agent_message"
                    } else {
                        "reasoning"
                    },
                )?;
                map.serialize_entry("text", text)?;
            }
            CodexItemBodyView::CommandExecution {
                command,
                aggregated_output,
                exit_code,
                status: state,
            } => {
                map.serialize_entry("type", "command_execution")?;
                map.serialize_entry("command", command)?;
                map.serialize_entry("aggregated_output", aggregated_output)?;
                map.serialize_entry("exit_code", exit_code)?;
                map.serialize_entry("status", status(*state))?;
            }
            CodexItemBodyView::FileChange {
                changes,
                status: state,
            } => {
                map.serialize_entry("type", "file_change")?;
                map.serialize_entry("changes",&changes.iter().map(|change|json!({"path":change.path,"kind":match change.kind { CodexChangeKind::Add=>"add",CodexChangeKind::Delete=>"delete",CodexChangeKind::Update=>"update" }})).collect::<Vec<_>>())?;
                map.serialize_entry("status", status(*state))?;
            }
            CodexItemBodyView::McpToolCall {
                server,
                tool,
                arguments,
                result,
                error,
                status: state,
            } => {
                map.serialize_entry("type", "mcp_tool_call")?;
                map.serialize_entry("server", server)?;
                map.serialize_entry("tool", tool)?;
                map.serialize_entry("arguments", &Raw(arguments))?;
                if let Some(result) = result {
                    map.serialize_entry("result", &NativeMcpResult(result))?;
                }
                if let Some(error) = error {
                    map.serialize_entry("error", error)?;
                }
                map.serialize_entry("status", status(*state))?;
            }
            CodexItemBodyView::CollabToolCall {
                tool,
                sender_thread_id,
                receiver_thread_ids,
                prompt,
                agents_states,
                status: state,
            } => {
                map.serialize_entry("type", "collab_tool_call")?;
                map.serialize_entry(
                    "tool",
                    match tool {
                        CodexCollabTool::SpawnAgent => "spawn_agent",
                        CodexCollabTool::SendInput => "send_input",
                        CodexCollabTool::Wait => "wait",
                        CodexCollabTool::CloseAgent => "close_agent",
                    },
                )?;
                map.serialize_entry("sender_thread_id", sender_thread_id)?;
                map.serialize_entry(
                    "receiver_thread_ids",
                    &receiver_thread_ids.iter().collect::<Vec<_>>(),
                )?;
                if let Some(prompt) = prompt {
                    map.serialize_entry("prompt", prompt)?;
                }
                map.serialize_entry("agents_states",&agents_states.iter().map(|agent|json!({"id":agent.id,"status":match agent.status { CodexAgentStatus::PendingInit=>"pending_init",CodexAgentStatus::Running=>"running",CodexAgentStatus::Interrupted=>"interrupted",CodexAgentStatus::Completed=>"completed",CodexAgentStatus::Errored=>"errored",CodexAgentStatus::Shutdown=>"shutdown",CodexAgentStatus::NotFound=>"not_found" },"message":agent.message})).collect::<Vec<_>>())?;
                map.serialize_entry("status", status(*state))?;
            }
            CodexItemBodyView::WebSearch {
                inner_id,
                query,
                action,
                results,
            } => {
                map.serialize_entry("type", "web_search")?;
                map.serialize_entry("inner_id", inner_id)?;
                map.serialize_entry("query", query)?;
                map.serialize_entry("action", &NativeWebAction(action))?;
                if let Some(results) = results {
                    map.serialize_entry("results", &RawList(results))?;
                }
            }
            CodexItemBodyView::TodoList { items } => {
                map.serialize_entry("type", "todo_list")?;
                map.serialize_entry(
                    "items",
                    &items
                        .iter()
                        .map(|item| json!({"text":item.text,"completed":item.completed}))
                        .collect::<Vec<_>>(),
                )?;
            }
            CodexItemBodyView::Error { message } => {
                map.serialize_entry("type", "error")?;
                map.serialize_entry("message", message)?;
            }
        }
        map.end()
    }
}
struct NativeMcpResult<'v, 'n>(&'v CodexMcpResultView<'n>);
impl Serialize for NativeMcpResult<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("content", &RawList(&self.0.content))?;
        if let Some(meta) = &self.0.meta {
            map.serialize_entry("_meta", &Raw(meta))?;
        }
        if let Some(content) = &self.0.structured_content {
            map.serialize_entry("structured_content", &Raw(content))?;
        }
        map.end()
    }
}
struct NativeWebAction<'v, 'n>(&'v CodexWebActionView<'n>);
impl Serialize for NativeWebAction<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match self.0 {
            CodexWebActionView::Search { query, queries } => {
                map.serialize_entry("type", "search")?;
                if let Some(query) = query {
                    map.serialize_entry("query", query)?;
                }
                if let Some(queries) = queries {
                    map.serialize_entry("queries", &queries.iter().collect::<Vec<_>>())?;
                }
            }
            CodexWebActionView::OpenPage { url } => {
                map.serialize_entry("type", "open_page")?;
                if let Some(url) = url {
                    map.serialize_entry("url", url)?;
                }
            }
            CodexWebActionView::FindInPage { url, pattern } => {
                map.serialize_entry("type", "find_in_page")?;
                if let Some(url) = url {
                    map.serialize_entry("url", url)?;
                }
                if let Some(pattern) = pattern {
                    map.serialize_entry("pattern", pattern)?;
                }
            }
            CodexWebActionView::Other => map.serialize_entry("type", "other")?,
        }
        map.end()
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;

pub(super) struct KiroAuthoritySink<'a, 'sink> {
    pub(super) progress: &'a mut ProgressSink<'sink>,
    pub(super) allowed: &'a (dyn Fn() -> bool + Sync),
}
impl graphit_ai::cli_kiro::KiroSink for KiroAuthoritySink<'_, '_> {
    async fn push<'a>(
        &'a mut self,
        phase: graphit_ai::cli_kiro::KiroDeliveryPhase,
        event: &'a agent_client_protocol::schema::v1::SessionNotification,
        original: &'a SnapshotContext,
    ) -> std::io::Result<()> {
        original.check()?;
        if !(self.allowed)() {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        // The private session ID is excluded; only the native update is public.
        self.progress
            .emit(
                &json!({"provider":"kiro","phase":format!("{phase:?}"),"update":event.update}),
                original,
            )
            .await
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        original.check()?;
        if !(self.allowed)() {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }
}
pub(super) async fn kiro_terminal(
    generation: &graphit_data::knowledge_index::KnowledgeGeneration,
    documents: &[graphit_data::knowledge_index::KnowledgeHead],
    execution: &Result<graphit_ai::cli_kiro::KiroReceipt, graphit_ai::cli_kiro::KiroFailure>,
    owner: &crate::knowledge_kiro_process::KiroProcessOwner,
    model: &str,
    allow_answer: bool,
) -> CallToolResult {
    let (receipt, error) = match execution {
        Ok(r) => (r, None),
        Err(e) => (e.receipt(), Some(format!("{:?}", e.reason()))),
    };
    let answer = if allow_answer { receipt.answer() } else { None };
    let status = if answer.is_some() { "answer" } else { "failed" };
    let mut completion = json!({"status":status,"requested_model":model});
    if let Some(answer) = answer {
        completion["answer"] = json!(answer);
    }
    let payload = json!({"status":status,"generation":generation_value(generation),"provided_slugs":documents.iter().map(|d|d.slug.as_str()).collect::<Vec<_>>(),"execution_error":error,"receipt":{"provider":"kiro","engine":format!("{:?}",receipt.engine()),"completion":completion,"observed_notifications":receipt.observed(),"accepted_notifications":receipt.accepted(),"new_response_observed":receipt.new_response().is_some(),"load_response_observed":receipt.load_response().is_some(),"configuration_response_observed":receipt.config_response().is_some(),"stop_reason":receipt.response().map(|r|format!("{:?}",r.stop_reason)),"driver_success":receipt.driver().map(Result::is_ok),"cancel_enqueued":receipt.cancel_enqueue().map(Result::is_ok),"process":owner.public_observation().await}});
    if answer.is_some() {
        crate::mcp_output::typed(payload)
    } else {
        crate::mcp_output::typed_error(payload)
    }
}
