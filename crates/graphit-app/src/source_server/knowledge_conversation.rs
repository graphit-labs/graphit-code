//! Incoming MCP-only USER conversation operations using service-owned native turns.
mod antigravity;
mod claude;
mod copilot;
mod cursor;
mod grok;
mod kimi;
mod opencode;
mod qwen;
use super::*;
use crate::{
    knowledge_conversation::{
        ConversationCustodyEntry, ConversationDeletion, ConversationPersistence,
    },
    knowledge_question::{KnowledgeQuestionModel, prepare_knowledge_codex},
};
use graphit_ai::{
    cli_codex::{CodexSink, CodexStreamLimits, run_codex},
    cli_codex_wire::CodexEventView,
    codex_completion::AttemptPurpose,
    knowledge_conversation::{
        ConversationBinding, ConversationEngine, ConversationSource, KiroConversationTarget,
        KnowledgeConversationStore,
    },
};
use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority,
    broker::{
        BrokerRequestContext,
        project_access::{BrokerProjectCaller, BrokerProjectScope, BrokerRecordRead},
    },
};
use graphit_core::artifact_store::{StoreOwner, UserIdentity};
use graphit_process::SinkFailure;

pub(super) const TOOL: &str = "graphit_knowledge_conversation";
pub(super) fn definition() -> Tool {
    Tool::new(TOOL, "Create, continue, list, delete Graphit metadata, or clear a bound native Kiro session for the verified Incoming MCP principal. Continue requires fresh selected PROJECT sources. Clear requires model and PROJECT access, but no query or corpus; it uses the agent's advertised session/delete capability to remove the session from its list, then removes the Graphit binding after real cleanup. Native storage behavior is agent-defined. Delete removes only Graphit metadata.", json!({"type":"object","properties":{"operation":{"enum":["create","continue","list","delete","clear"]},"provider":{"type":"string"},"conversation_id":{"type":"string"},"query":{"type":"string"},"selected_slugs":{"type":"array","items":{"type":"string"}},"model":{"type":"string"},"project_dir":{"type":"string"},"project_version":{"type":"string"}},"required":["operation","provider"],"additionalProperties":false}).as_object().expect("schema").clone())
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Arguments {
    Create {
        provider: String,
        conversation_id: String,
        query: String,
        selected_slugs: Vec<String>,
        model: String,
        #[serde(default, deserialize_with = "present")]
        project_dir: Option<String>,
        #[serde(default, deserialize_with = "present")]
        project_version: Option<String>,
    },
    Continue {
        provider: String,
        conversation_id: String,
        query: String,
        selected_slugs: Vec<String>,
        model: String,
        #[serde(default, deserialize_with = "present")]
        project_dir: Option<String>,
        #[serde(default, deserialize_with = "present")]
        project_version: Option<String>,
    },
    List {
        provider: String,
    },
    Clear {
        provider: String,
        conversation_id: String,
        model: String,
        #[serde(default, deserialize_with = "present")]
        project_dir: Option<String>,
        #[serde(default, deserialize_with = "present")]
        project_version: Option<String>,
    },
    Delete {
        provider: String,
        conversation_id: String,
    },
}
impl Arguments {
    fn provider(&self) -> &str {
        match self {
            Self::Create { provider, .. }
            | Self::Continue { provider, .. }
            | Self::List { provider }
            | Self::Clear { provider, .. }
            | Self::Delete { provider, .. } => provider,
        }
    }
}
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input = match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
        Ok(input) => input,
        Err(_) => return failure("Knowledge conversation arguments invalid"),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("Knowledge conversation Incoming MCP credentials required");
    };
    if parts.headers.contains_key("x-graphit-auth-origin") {
        return failure("Knowledge conversation requires Incoming MCP authentication");
    }
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return failure("Knowledge conversation bearer required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let (send, receive) = oneshot::channel();
    let progress =
        knowledge_ask::ProgressSink::mcp(context.peer, context.meta.get_progress_token());
    let handle = Handle::current();
    {
        let mut jobs = tokio::select! { jobs = state.jobs.lock() => jobs,
        () = context.ct.cancelled() => return failure("Knowledge conversation cancelled"),
        () = state.stop.cancelled() => return failure("Knowledge conversation stopped"),
        () = tokio::time::sleep_until(deadline.into()) => return failure("Knowledge conversation deadline") };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("Knowledge conversation stopped");
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(execute(
                &owner,
                input,
                &credential,
                &original_cancel,
                deadline,
                progress,
            ));
            let _ = send.send(result);
        });
    }
    tokio::select! {
    result = receive => match result {
        Ok(result) if context.ct.is_cancelled() || cancel.is_cancelled() || state.stop.is_cancelled() || Instant::now() >= deadline => deny_delivery(result,"CancelledOrExpired"),
        Ok(result) => result, Err(_) => failure("Knowledge conversation native worker failed") },
    () = context.ct.cancelled() => failure("Knowledge conversation cancelled"),
    () = state.stop.cancelled() => failure("Knowledge conversation stopped"),
    () = tokio::time::sleep_until(deadline.into()) => failure("Knowledge conversation deadline") }
}
async fn execute(
    state: &State,
    input: Arguments,
    credential: &str,
    cancel: &Cancellation,
    deadline: Instant,
    progress: knowledge_ask::ProgressSink<'static>,
) -> CallToolResult {
    let Some(access) = state.conversations.as_ref() else {
        return failure("Knowledge conversations are not configured");
    };
    let Some((_, verifier)) = access
        .providers
        .iter()
        .find(|(name, _)| name == input.provider())
    else {
        return failure("Knowledge conversation provider unavailable");
    };
    let original = match RequestContext::new(state.policy.content, cancel, deadline) {
        Ok(v) => v,
        Err(_) => return failure("Knowledge conversation request invalid"),
    };
    let caller = match verifier
        .verify(credential, BrokerRequestContext::new(original.snapshot()))
        .await
    {
        Ok(v) => v,
        Err(_) => return failure("Knowledge conversation authentication denied"),
    };
    let issuer = state.auth.issuer();
    let verified = match issuer.user_from_incoming(&caller, verifier, &original) {
        Ok(v) => v,
        Err(_) => return failure("Knowledge conversation identity denied"),
    };
    let identity = match verified
        .issuer()
        .and_then(|i| verified.subject().map(|s| (i, s)))
    {
        Ok((i, s)) => match UserIdentity::new(i.to_owned(), s.to_owned()) {
            Ok(v) => v,
            Err(_) => return failure("Knowledge conversation identity invalid"),
        },
        Err(_) => return failure("Knowledge conversation identity denied"),
    };
    let StoreOwner::Installation { installation_id } = state.auth.installation().live.owner()
    else {
        return failure("Knowledge conversation installation invalid");
    };
    let live = match CapturedPublicationScope::user(
        &state.policy.installation,
        installation_id.clone(),
        identity,
        state.policy.scope,
        original.snapshot(),
    ) {
        Ok(v) => v,
        Err(e) => {
            return failure(&format!(
                "Knowledge conversation USER capture: {:?}",
                e.kind()
            ));
        }
    };
    let record = match live.binding() {
        Ok(v) => v,
        Err(e) => {
            return failure(&format!(
                "Knowledge conversation USER binding: {:?}",
                e.kind()
            ));
        }
    };
    let user = ScopeCapture {
        live: &live,
        record: &record,
    };
    let read = match issuer.admit_incoming_user_knowledge_read(
        &caller,
        verifier,
        &original,
        input.provider(),
        user,
    ) {
        Ok(v) => v,
        Err(_) => return failure("Knowledge conversation USER read denied"),
    };
    let write = if matches!(input, Arguments::List { .. }) {
        None
    } else {
        match issuer.admit_incoming_user_knowledge_write(
            &caller,
            verifier,
            &original,
            input.provider(),
            user,
        ) {
            Ok(v) => Some(v),
            Err(_) => return failure("Knowledge conversation USER mutation denied"),
        }
    };
    if matches!(input, Arguments::Create { .. } | Arguments::Continue { .. })
        && state.ask.as_ref().and_then(|ask| ask.service()).is_none()
    {
        return failure(
            "Knowledge conversation requires a configured captured completion provider",
        );
    }
    let store = match KnowledgeConversationStore::capture(
        live.clone(),
        Arc::clone(original.snapshot()),
        access.limits,
    ) {
        Ok(v) => v,
        Err(e) => return failure(&e.to_string()),
    };
    let user_allowed = || {
        read.recheck().is_ok()
            && write.as_ref().is_none_or(|w| w.recheck().is_ok())
            && original.snapshot().check().is_ok()
            && !state.stop.is_cancelled()
    };
    if !user_allowed() {
        return failure("Knowledge conversation USER authorization changed");
    }
    match &input {
        Arguments::List { .. } => {
            let result = store.list(original.snapshot());
            if !user_allowed() {
                return failure("Knowledge conversation delivery denied");
            }
            return match result {
                Ok(records) => {
                    crate::mcp_output::typed(json!({"status":"listed","conversations":records}))
                }
                Err(e) => failure(&e.to_string()),
            };
        }
        Arguments::Delete {
            conversation_id, ..
        } => {
            let result = store.delete(conversation_id, original.snapshot());
            if !user_allowed() {
                return failure("Knowledge conversation delivery denied");
            }
            return match result {
                Ok(()) => crate::mcp_output::typed(
                    json!({"status":"deleted","conversation_id":conversation_id}),
                ),
                Err(e) => failure(&e.to_string()),
            };
        }
        _ => (),
    }
    let creating = matches!(input, Arguments::Create { .. });
    let (conversation_id, model, project_dir, project_version) = match &input {
        Arguments::Create {
            conversation_id,
            model,
            project_dir,
            project_version,
            ..
        }
        | Arguments::Continue {
            conversation_id,
            model,
            project_dir,
            project_version,
            ..
        }
        | Arguments::Clear {
            conversation_id,
            model,
            project_dir,
            project_version,
            ..
        } => (conversation_id, model, project_dir, project_version),
        _ => unreachable!(),
    };
    if project_dir
        .as_deref()
        .is_some_and(|v| v.is_empty() || v.contains('\0'))
        || project_version
            .as_deref()
            .is_some_and(|v| v.is_empty() || v.contains('\0'))
    {
        return failure("Knowledge conversation project invalid");
    }
    let snapshot = match state.live_projects.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return failure("Knowledge conversation PROJECT selection unavailable"),
    };
    let projects = &snapshot.projects;
    let Some(current) = snapshot.current else {
        return failure("Knowledge conversation PROJECT selection unavailable");
    };
    let root = project_dir
        .as_deref()
        .map(Path::new)
        .unwrap_or(projects[current].0.root());
    let selected = if project_version.is_none() && root == projects[current].0.root() {
        current
    } else {
        match select_project(projects, root, project_version.as_deref()) {
            Ok(v) => v,
            Err(e) => return failure(e),
        }
    };
    let (paths, id, version) = &projects[selected];
    if !matches!(state.live_projects.is_active(paths, *id, version), Ok(true)) {
        return failure("Knowledge conversation PROJECT selection unavailable");
    }
    let project = match CapturedPublicationScope::project(
        paths,
        *id,
        state.policy.scope,
        original.snapshot(),
    ) {
        Ok(v) => v,
        Err(e) => {
            return failure(&format!(
                "Knowledge conversation PROJECT capture: {:?}",
                e.kind()
            ));
        }
    };
    let binding = match project.binding() {
        Ok(v) => v,
        Err(e) => {
            return failure(&format!(
                "Knowledge conversation PROJECT binding: {:?}",
                e.kind()
            ));
        }
    };
    let authority = ProjectRecordAuthority::new(issuer);
    let project_read = match authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Incoming(&caller),
            verifier,
            backend: &access.backend,
            original: &original,
            scope: BrokerProjectScope {
                installation: state.auth.installation(),
                project: ProjectCapture {
                    scope: ScopeCapture {
                        live: &project,
                        record: &binding,
                    },
                    version,
                },
                providers: &[],
            },
            provider: input.provider(),
            module: ProjectReadModule::Knowledge,
        })
        .await
    {
        Ok(v) => v,
        Err(_) => return failure("Knowledge conversation PROJECT read denied"),
    };
    let Some(service) = state.ask.as_ref().and_then(|a| a.service()) else {
        return failure("Knowledge conversation requires a supported startup native binding");
    };
    let conversation_engine = service.conversation_engine();
    service.reap().await;
    if matches!(input, Arguments::Clear { .. }) {
        let Some(engine) = service.kiro_engine() else {
            return failure("Native session clear requires a startup Kiro binding");
        };
        if model.is_empty()
            || model.contains('\0')
            || model.len() > service.policy.completion.max_model_id_bytes
        {
            return failure("Native session clear model invalid");
        }
        let check = || -> std::io::Result<()> {
            original.snapshot().check()?;
            if state.stop.is_cancelled() {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            read.recheck().map_err(std::io::Error::other)?;
            if let Some(write) = &write {
                write.recheck().map_err(std::io::Error::other)?;
            }
            crate::knowledge_question::check_read(original.snapshot(), &project_read)
                .map_err(std::io::Error::other)
        };
        let allowed = || check().is_ok();
        if !allowed() {
            return failure("Native session clear authorization changed before lookup");
        }
        let project_id = id.to_string();
        let turn = match store.begin_kiro_delete(
            conversation_id,
            KiroConversationTarget {
                engine: conversation_engine,
                model,
                program: service.program(),
                project_id: &project_id,
                project_version: version.version(),
            },
            original.snapshot(),
        ) {
            Ok(turn) => turn,
            Err(error) => return failure(&error.to_string()),
        };
        let custody = service.hold_conversation_turn(turn);
        let mut decision =
            crate::knowledge_conversation::ConversationDecisionGuard::new(Arc::clone(&custody));
        let request = match custody.configure_kiro_delete(service.program(), original.snapshot()) {
            Ok(request) => request,
            Err(error) => {
                custody.release_unsubmitted();
                return failure(&error.to_string());
            }
        };
        if !allowed() {
            custody.release_unsubmitted();
            return failure(
                "Native session clear authorization changed while waiting for USER writer",
            );
        }
        let (transport, owner) = match service
            .kiro_process(
                paths.root(),
                original.snapshot(),
                Some(Arc::clone(&custody)),
                &allowed,
            )
            .await
        {
            Ok(value) => value,
            Err(error) => {
                custody.release_unsubmitted();
                return failure(&format!("Native session clear launch: {:?}", error.kind()));
            }
        };
        decision.submitted();
        let mut progress = progress;
        let mut sink = knowledge_ask::KiroAuthoritySink {
            progress: &mut progress,
            allowed: &allowed,
        };
        let execution = graphit_ai::cli_kiro::delete_kiro_session(
            original.snapshot(),
            transport,
            engine,
            request,
            service.kiro_limits(),
            &mut sink,
            &check,
        )
        .await;
        let join = service.join_kiro(&owner).await;
        let joined = matches!(&join, Ok(true));
        let clean = owner.clean_joined().await;
        let receipt = match &execution {
            Ok(receipt) => receipt,
            Err(error) => error.receipt(),
        };
        let native_success = execution.is_ok() && receipt.delete_response().is_some();
        let deletion = custody.finish_delete(
            original.snapshot(),
            native_success && joined && clean && allowed(),
        );
        decision.decided();
        let (local_status, local_error) = match deletion {
            ConversationDeletion::Deleted => ("deleted", None),
            ConversationDeletion::Failed(error) => ("failed", Some(error.to_string())),
            ConversationDeletion::Pending => ("pending_process", None),
            ConversationDeletion::NotStarted => ("not_started", None),
            ConversationDeletion::Denied => ("retained", None),
        };
        let mut payload = json!({
            "status": if native_success && joined && clean && local_status == "deleted" { "cleared" } else { "clear_failed" },
            "conversation_id":conversation_id,
            "receipt": {
                "provider":"kiro", "engine":format!("{engine:?}"), "requested_model":model,
                "initialize_response_observed":receipt.initialize_response().is_some(),
                "delete_capability":receipt.initialize_response().map(|r| r.agent_capabilities.session_capabilities.delete.is_some()),
                "delete_response_observed":receipt.delete_response().is_some(),
                "driver_success":receipt.driver().map(Result::is_ok),
                "observed_notifications":receipt.observed(), "accepted_notifications":receipt.accepted(),
                "process":owner.public_observation().await,
            },
            "local_deletion":{"status":local_status, "error":local_error},
            "execution_error":execution.as_ref().err().map(|error| format!("{:?}", error.reason())),
            "sdk_error_code":execution.as_ref().err().and_then(|error| error.sdk_error()).map(|error| i32::from(error.code)),
            "join_error":join.as_ref().err().map(|error| format!("{:?}", error.kind())),
        });
        let success = native_success && joined && clean && local_status == "deleted";
        service.reap().await;
        let authorized = allowed();
        if !authorized {
            payload["status"] = json!("clear_failed");
        }
        let response = if success && authorized {
            crate::mcp_output::typed(payload)
        } else {
            crate::mcp_output::typed_error(payload)
        };
        return if authorized {
            response
        } else {
            deny_delivery(response, "UserOrProjectDeliveryDenied")
        };
    }
    let (query, selected_slugs) = match &input {
        Arguments::Create {
            query,
            selected_slugs,
            ..
        }
        | Arguments::Continue {
            query,
            selected_slugs,
            ..
        } => (query, selected_slugs),
        _ => unreachable!(),
    };
    if conversation_engine == ConversationEngine::OpenCode {
        return opencode::execute(opencode::Invocation {
            service,
            original: &original,
            project: &project_read,
            user_read: &read,
            user_write: write.as_ref().expect("mutation admitted"),
            stop: &state.stop,
            store: &store,
            creating,
            conversation_id,
            model,
            query,
            selected_slugs,
            cwd: paths.root(),
            project_id: id.to_string(),
            project_version: version.version(),
            progress,
        })
        .await;
    }
    if conversation_engine == ConversationEngine::Copilot {
        return copilot::execute(copilot::Invocation {
            service,
            original: &original,
            project: &project_read,
            user_read: &read,
            user_write: write.as_ref().expect("mutation admitted"),
            stop: &state.stop,
            store: &store,
            creating,
            conversation_id,
            model,
            query,
            selected_slugs,
            cwd: paths.root(),
            project_id: id.to_string(),
            project_version: version.version(),
            progress,
        })
        .await;
    }
    if conversation_engine == ConversationEngine::Claude {
        return claude::execute(claude::Invocation {
            service,
            original: &original,
            project: &project_read,
            user_read: &read,
            user_write: write.as_ref().expect("mutation admitted"),
            stop: &state.stop,
            store: &store,
            creating,
            conversation_id,
            model,
            query,
            selected_slugs,
            cwd: paths.root(),
            project_id: id.to_string(),
            project_version: version.version(),
            progress,
        })
        .await;
    }
    if conversation_engine == ConversationEngine::Qwen {
        return qwen::execute(qwen::Invocation {
            service,
            original: &original,
            project: &project_read,
            user_read: &read,
            user_write: write.as_ref().expect("mutation admitted"),
            stop: &state.stop,
            store: &store,
            creating,
            conversation_id,
            model,
            query,
            selected_slugs,
            cwd: paths.root(),
            project_id: id.to_string(),
            project_version: version.version(),
            progress,
        })
        .await;
    }
    if conversation_engine == ConversationEngine::Cursor {
        return cursor::execute(cursor::Invocation {
            service,
            original: &original,
            project: &project_read,
            user_read: &read,
            user_write: write.as_ref().expect("mutation admitted"),
            stop: &state.stop,
            store: &store,
            creating,
            conversation_id,
            model,
            query,
            selected_slugs,
            cwd: paths.root(),
            project_id: id.to_string(),
            project_version: version.version(),
            progress,
        })
        .await;
    }
    if conversation_engine == ConversationEngine::Antigravity {
        return antigravity::execute(antigravity::Invocation {
            service,
            original: &original,
            project: &project_read,
            user_read: &read,
            user_write: write.as_ref().expect("mutation admitted"),
            stop: &state.stop,
            store: &store,
            creating,
            conversation_id,
            model,
            query,
            selected_slugs,
            cwd: paths.root(),
            project_id: id.to_string(),
            project_version: version.version(),
            progress,
        })
        .await;
    }
    if conversation_engine == ConversationEngine::Kimi {
        return kimi::execute(kimi::Invocation {
            service,
            original: &original,
            project: &project_read,
            user_read: &read,
            user_write: write.as_ref().expect("mutation admitted"),
            stop: &state.stop,
            store: &store,
            creating,
            conversation_id,
            model,
            query,
            selected_slugs,
            cwd: paths.root(),
            project_id: id.to_string(),
            project_version: version.version(),
            progress,
        })
        .await;
    }
    if conversation_engine == ConversationEngine::Grok {
        return grok::execute(grok::Invocation {
            service,
            original: &original,
            project: &project_read,
            user_read: &read,
            user_write: write.as_ref().expect("mutation admitted"),
            stop: &state.stop,
            store: &store,
            creating,
            conversation_id,
            model,
            query,
            selected_slugs,
            cwd: paths.root(),
            project_id: id.to_string(),
            project_version: version.version(),
            progress,
        })
        .await;
    }
    if service.kiro_engine().is_some() {
        let slugs: Vec<_> = selected_slugs.iter().map(String::as_str).collect();
        let prepared = match crate::knowledge_question::prepare_documentary_question(
            original.snapshot(),
            &project_read,
            query,
            &slugs,
            model,
            service.policy.completion.max_model_id_bytes,
            service.policy.question,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => return failure(&e.to_string()),
        };
        let Some(documents) = prepared.documents() else {
            let response = match prepared.into_execution() {
                Ok(crate::knowledge_question::DocumentaryQuestionContent::Empty(empty)) => {
                    knowledge_ask::terminal(empty)
                }
                Ok(_) => unreachable!(),
                Err(e) => failure(&e.to_string()),
            };
            return if user_allowed() && project_read.recheck().is_ok() {
                response
            } else {
                deny_delivery(response, "UserOrProjectDeliveryDenied")
            };
        };
        let metadata = ConversationBinding {
            provider: "kiro".into(),
            engine: conversation_engine,
            model: model.clone(),
            program: service.program().to_owned(),
            project_id: id.to_string(),
            project_version: version.version().to_owned(),
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
        let allowed = || {
            user_allowed()
                && crate::knowledge_question::check_read(original.snapshot(), &project_read).is_ok()
        };
        if !allowed() {
            return failure("Knowledge conversation authorization changed before execution");
        }
        let turn = match if creating {
            store.begin_create(conversation_id, metadata, original.snapshot())
        } else {
            store.begin_turn(conversation_id, &metadata, original.snapshot())
        } {
            Ok(v) => v,
            Err(e) => return failure(&e.to_string()),
        };
        let custody = service.hold_conversation_turn(turn);
        let mut decision =
            crate::knowledge_conversation::ConversationDecisionGuard::new(Arc::clone(&custody));
        let session =
            match custody.configure_kiro(service.program(), paths.root(), original.snapshot()) {
                Ok(v) => v,
                Err(e) => {
                    custody.release_unsubmitted();
                    return failure(&e.to_string());
                }
            };
        if !allowed() {
            custody.release_unsubmitted();
            return failure(
                "Knowledge conversation authorization changed while waiting for USER writer",
            );
        }
        let content = match prepared.into_execution() {
            Ok(v) => v,
            Err(e) => {
                custody.release_unsubmitted();
                return failure(&e.to_string());
            }
        };
        let crate::knowledge_question::DocumentaryQuestionContent::Ready {
            generation,
            documents,
            prompt,
        } = content
        else {
            unreachable!()
        };
        let (transport, owner) = match service
            .kiro_process(
                paths.root(),
                original.snapshot(),
                Some(Arc::clone(&custody)),
                &allowed,
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                custody.release_unsubmitted();
                return failure(&format!(
                    "Knowledge conversation Kiro launch: {:?}",
                    e.kind()
                ));
            }
        };
        decision.submitted();
        let mut progress = progress;
        let mut sink = knowledge_ask::KiroAuthoritySink {
            progress: &mut progress,
            allowed: &allowed,
        };
        let execution = graphit_ai::cli_kiro::run_kiro(
            original.snapshot(),
            transport,
            service.kiro_engine().expect("Kiro binding"),
            session,
            service.kiro_prompt(prompt, model),
            service.kiro_limits(),
            &mut sink,
        )
        .await;
        let joined = service.join_kiro(&owner).await.unwrap_or(false);
        let clean = owner.clean_joined().await;
        let receipt = match &execution {
            Ok(r) => r,
            Err(e) => e.receipt(),
        };
        // Load's requested ID exists before its RPC response. Only an actual
        // successful New/Load response confirms native session continuity.
        if (receipt.new_response().is_some() || receipt.load_response().is_some())
            && let Some(id) = receipt.session()
        {
            custody.observe_kiro(id, original.snapshot());
        }
        let persistence = custody.finish(original.snapshot(), allowed() && joined && clean);
        decision.decided();
        let response = knowledge_ask::kiro_terminal(
            &generation,
            &documents,
            &execution,
            &owner,
            model,
            joined && clean && allowed(),
        )
        .await;
        let mut payload = response
            .structured_content
            .unwrap_or_else(|| json!({"status":"failed"}));
        let persisted = match persistence {
            ConversationPersistence::Persisted(record) => {
                payload["persistence"] = json!({"status":"persisted","conversation":record});
                true
            }
            ConversationPersistence::Failed(e) => {
                payload["persistence"] = json!({"status":"failed","error":e.to_string()});
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
        service.reap().await;
        let response = if persisted && response.is_error != Some(true) {
            crate::mcp_output::typed(payload)
        } else {
            crate::mcp_output::typed_error(payload)
        };
        return if persisted && allowed() {
            response
        } else {
            deny_delivery(response, "ContinuationOrDeliveryDenied")
        };
    }
    let host = match service.host() {
        Ok(v) => v,
        Err(e) => return failure(&format!("Knowledge conversation host: {:?}", e.kind())),
    };
    let slugs: Vec<_> = selected_slugs.iter().map(String::as_str).collect();
    let prepared = match prepare_knowledge_codex(
        original.snapshot(),
        &project_read,
        query,
        &slugs,
        KnowledgeQuestionModel {
            requested_model: model,
            wire_limits: &service.policy.wire,
            completion_limits: service.policy.completion,
        },
        service.policy.question,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return failure(&e.to_string()),
    };
    let Some(documents) = prepared.documents() else {
        let response = match prepared.complete_without_execution() {
            Ok(result) => knowledge_ask::terminal(result),
            Err(e) => failure(&e.to_string()),
        };
        return if user_allowed() && project_read.recheck().is_ok() {
            response
        } else {
            deny_delivery(response, "UserOrProjectDeliveryDenied")
        };
    };
    let metadata = ConversationBinding {
        provider: "codex".into(),
        engine: ConversationEngine::Codex,
        model: model.clone(),
        program: service.program().to_owned(),
        project_id: id.to_string(),
        project_version: version.version().to_owned(),
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
    if !user_allowed() || project_read.recheck().is_err() {
        return failure("Knowledge conversation authorization changed before execution");
    }
    let turn = match if creating {
        store.begin_create(conversation_id, metadata, original.snapshot())
    } else {
        store.begin_turn(conversation_id, &metadata, original.snapshot())
    } {
        Ok(v) => v,
        Err(e) => return failure(&e.to_string()),
    };
    let custody = service.hold_conversation_turn(turn);
    let mut request = service.request(paths.root(), model);
    if let Err(error) = custody.configure(&mut request, original.snapshot()) {
        custody.release_unsubmitted();
        return failure(&error.to_string());
    }
    // USER writer acquisition may have waited: admit the same caller again before polling.
    if !user_allowed() || project_read.recheck().is_err() {
        custody.release_unsubmitted(); // No transport future has been constructed or polled.
        return failure(
            "Knowledge conversation authorization changed while waiting for USER writer",
        );
    }
    let allowed = || user_allowed() && project_read.recheck().is_ok();
    let mut sink = ConversationSink {
        progress,
        custody: Arc::clone(&custody),
        original: original.snapshot(),
        allowed: &allowed,
    };
    let execution_custody = Arc::clone(&custody);
    let result = prepared
        .execute(|mut run, prompt| async move {
            request.stdin = Some(prompt.into_bytes().into());
            let mut execution = run_codex(
                &mut run,
                &host,
                request,
                AttemptPurpose::Request,
                None,
                CodexStreamLimits {
                    stdout_bytes: service.policy.process.stdout_bytes,
                    stderr_bytes: service.policy.process.stderr_bytes,
                },
                &mut sink,
            )
            .await;
            execution_custody.retain(match &mut execution {
                Ok(r) => r,
                Err(e) => e.receipt_mut(),
            });
            if let Err(failure) = &execution {
                execution_custody.release_pre_submit_failure(failure);
            }
            execution
        })
        .await;
    if result
        .as_ref()
        .err()
        .is_some_and(|error| error.execution().is_none())
    {
        // Prepared execute rejected before invoking our transport closure.
        custody.release_unsubmitted();
    }
    let persistence = custody.finish(original.snapshot(), allowed());
    service.reap().await;
    let response = match result {
        Ok(result) => knowledge_ask::terminal(result),
        Err(error) => {
            let mut payload = json!({"status":"failed","error":error.to_string()});
            if let Some(execution) = error.execution() {
                let receipt = match execution {
                    Ok(r) => r,
                    Err(e) => {
                        payload["execution_error"] = json!(format!("{:?}", e.reason()));
                        e.receipt()
                    }
                };
                payload["receipt"] = knowledge_ask::receipt_value(receipt, false);
            }
            crate::mcp_output::typed_error(payload)
        }
    };
    let mut payload = response
        .structured_content
        .unwrap_or_else(|| json!({"status":"failed"}));
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
    let response = if !persisted || response.is_error == Some(true) {
        crate::mcp_output::typed_error(payload)
    } else {
        crate::mcp_output::typed(payload)
    };
    if !persisted || !allowed() {
        deny_delivery(response, "ContinuationOrDeliveryDenied")
    } else {
        response
    }
}
fn deny_delivery(mut response: CallToolResult, reason: &str) -> CallToolResult {
    if let Some(payload) = response.structured_content.as_mut()
        && payload.get("status").and_then(Value::as_str) == Some("cleared")
    {
        payload["status"] = json!("clear_failed");
    }
    if let Some(payload) = response.structured_content.as_mut()
        && let Some(persistence) = payload
            .get_mut("persistence")
            .and_then(Value::as_object_mut)
    {
        persistence.remove("conversation");
    }
    knowledge_ask::delivery_denied(response, reason)
}
struct ConversationSink<'a> {
    progress: knowledge_ask::ProgressSink<'static>,
    custody: Arc<ConversationCustodyEntry>,
    original: &'a Arc<graphit_core::file_io::SnapshotContext>,
    allowed: &'a (dyn Fn() -> bool + Sync),
}
impl CodexSink for ConversationSink<'_> {
    async fn push<'a>(
        &'a mut self,
        event: CodexEventView<'a>,
        original: &'a graphit_core::file_io::SnapshotContext,
    ) -> Result<(), SinkFailure> {
        if !(self.allowed)() {
            return Err(SinkFailure::Closed);
        }
        if let CodexEventView::ThreadStarted { thread_id } = event {
            return if self.custody.observe(thread_id, self.original) {
                Ok(())
            } else {
                Err(SinkFailure::ConsumerFailed)
            };
        }
        self.progress.push(event, original).await?;
        if !(self.allowed)() {
            return Err(SinkFailure::Closed);
        }
        Ok(())
    }
}
