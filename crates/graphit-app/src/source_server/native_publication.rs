//! Native publication on the existing Source worker and admission boundary.
use super::*;
use crate::native_publication::{PublicationFailure, PublicationMetadata};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ServiceMode};
use graphit_core::{artifact_store::CapturedPublicationScope, native_path::NativePath};
use std::num::{NonZeroU32, NonZeroUsize};

pub(super) const TOOL: &str = "graphit_hub_publish_native";
pub(super) fn definition() -> Tool {
    Tool::new(
        TOOL,
        "Publish captured PROJECT AST or Knowledge SDK snapshots through the active Hub provider. PROJECT read and HubWrite authority are required separately.",
        json!({
            "type":"object",
            "properties":{
                "publication":{
                    "type":"object",
                    "properties":{
                        "provider":{"type":"string","description":"Configured provider of the active authenticated profile."},
                        "artifact_type":{"type":"string","enum":["ast","knowledge"]},
                        "id":{"type":"string","description":"Full artifact ID, including its namespace."},
                        "version":{"type":"string","description":"Concrete artifact release, separate from project_version; latest is not accepted."},
                        "name":{"type":"string"},
                        "description":{"type":["string","null"]},
                        "author":{"type":["string","null"]},
                        "tags":{"type":"array","items":{"type":"string"}},
                        "dependencies":{"type":["array","null"],"description":"Omission preserves current dependencies; an empty array clears them.","items":{"type":"object","properties":{
                            "artifact_type":{"type":"string","enum":["skill","command","workflow","agent","rule","mcp","ast","knowledge","language","power"]},
                            "id":{"type":"string"},"selector":{"type":"string"}
                        },"required":["artifact_type","id","selector"],"additionalProperties":false}}
                    },
                    "required":["provider","artifact_type","id","version","name"],
                    "additionalProperties":false
                },
                "project_dir":{"type":"string","description":"Absolute PROJECT path as a literal Unicode string."},
                "project_version":{"type":"string"}
            },
            "required":["publication","project_dir"],
            "additionalProperties":false
        }).as_object().unwrap().clone(),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    publication: PublicationMetadata,
    project_dir: NativePath,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
}
fn failure(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"status":"error","code":code}))
}
fn operation_failure(error: io::Error) -> CallToolResult {
    let metadata = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<crate::hub_project_metadata::Failure>());
    let error = metadata.map_or(&error, |effect| &effect.cause);
    let code = match error.kind() {
        io::ErrorKind::PermissionDenied => "authentication_or_permission_denied",
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => "request_cancelled_or_expired",
        io::ErrorKind::InvalidInput => "invalid_arguments",
        io::ErrorKind::AlreadyExists => "native_publication_conflict",
        _ => "native_publication_failed",
    };
    let mut body = if let Some(effect) = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<PublicationFailure>())
    {
        json!({"status":"error","code":code,"stage":effect.stage,"publication":effect.receipt,"cause":crate::native_publication::cause_code(&effect.cause),"owned_temporary_residue":crate::native_publication::cleanup_residue(&effect.cause),"upload_cleanup":crate::native_publication::cleanup_causes(&effect.cause)})
    } else {
        json!({"status":"error","code":code})
    };
    if let Some(effect) = metadata {
        body["project_metadata"] = json!(effect.effect);
        if body.get("stage").is_none() {
            body["stage"] = json!(effect.stage);
            body["cause"] = json!(crate::native_publication::cause_code(error));
        }
    }
    crate::mcp_output::typed_error(body)
}

async fn execute(
    state: &State,
    selected: &(ProjectPaths, ProjectId, VersionContext),
    input: Arguments,
    credential: &str,
    local: bool,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    let (paths, id, version) = selected;
    if !state.live_projects.is_active(paths, *id, version)? {
        return Err(io::ErrorKind::NotFound.into());
    }
    let limits = crate::native_publication::metadata_limits(
        state.policy.content,
        state.policy.registry.limits().projection(),
    );
    input.publication.validate(limits)?;
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, request)
    } else {
        issuer.authenticate_runtime(credential, request)
    }
    .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let scope =
        CapturedPublicationScope::project(paths, *id, state.policy.scope, request.snapshot())?;
    let binding = scope.binding()?;
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_read(
            &caller,
            request,
            ScopeInput {
                installation: state.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: state.policy.binding,
                global_version: "",
            },
            match input.publication.artifact_type {
                graphit_hub::ArtifactType::Ast => ProjectReadModule::Ast,
                graphit_hub::ArtifactType::Knowledge => ProjectReadModule::Knowledge,
                _ => return Err(io::ErrorKind::InvalidInput.into()),
            },
        )
        .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let mut selection = issuer
        .capture_hub_write_service(&caller, &input.publication.provider)
        .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let verifier = if selection.mode().map_err(io::Error::other)? == ServiceMode::BrokerNative {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .get(&mut selection)
                .map_err(io::Error::other)?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => selection.native_credential(verifier).await,
        None => selection.credential(),
    }
    .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
    let storage = crate::native_storage_credentials::broker_credentials(
        &lease,
        *id,
        &state.native_http,
        state.native_allow_loopback_http,
    )
    .await
    .map_err(io::Error::from)?;
    let check = || -> io::Result<()> {
        request.check()?;
        admission.recheck().map_err(io::Error::other)?;
        lease.check().map_err(io::Error::other)?;
        storage.check().map_err(io::Error::other)?;
        if !state.live_projects.is_active(paths, *id, version)? {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(())
    };
    check()?;
    let project_metadata = crate::hub_project_metadata::publish(
        storage.store.as_ref(),
        &storage.prefix,
        (paths, *id),
        limits.bytes,
        request,
        &check,
    )
    .await?;
    let operation: io::Result<Value> = async {
        let receipt = match input.publication.artifact_type {
            graphit_hub::ArtifactType::Ast => {
                let base = state.graph.project_config()?;
                let capture = crate::project_graph::PreparedRead::capture(
                    &state.policy,
                    &state.live_projects,
                    (paths, *id, version),
                    Arc::clone(request.snapshot()),
                )
                .map_err(io::Error::other)?;
                let (_staged, graph) = state
                    .graph
                    .capture_current_export(
                        &capture,
                        paths,
                        base,
                        &state.live_projects,
                        request.snapshot(),
                    )
                    .await
                    .map_err(io::Error::other)?;
                let components = crate::native_publication::ast_components(&graph, request).await?;
                let reader = graph
                    .source_reader(request.snapshot())
                    .map_err(io::Error::other)?;
                crate::native_publication::publish(
                    crate::native_publication::PublishInput {
                        store: storage.store.as_ref(),
                        storage_prefix: &storage.prefix,
                        producer: *id,
                        metadata: &input.publication,
                        reader,
                        components: &components,
                        limits,
                    },
                    request,
                    || {
                        check()?;
                        capture
                            .check(&state.policy, base, &state.live_projects)
                            .map_err(io::Error::other)
                    },
                )
                .await?
            }
            graphit_hub::ArtifactType::Knowledge => {
                let index = graphit_data::knowledge_index::KnowledgeIndexReadLimits {
                    native: state.policy.index,
                    batch_rows: NonZeroU32::new(8192).unwrap(),
                    content: graphit_data::knowledge_index::KnowledgeIndexContentLimits {
                        max_identifier_bytes: NonZeroUsize::new(
                            state.policy.content.max_scalar_bytes,
                        )
                        .ok_or(io::ErrorKind::InvalidInput)?,
                    },
                };
                let Some(frozen) = crate::knowledge_package_export::capture_native_knowledge(
                    request, &admission, &caller, index,
                )
                .await?
                else {
                    return Ok(json!({"status":"no_corpus","project_metadata":project_metadata}));
                };
                crate::native_publication::publish(
                    crate::native_publication::PublishInput {
                        store: storage.store.as_ref(),
                        storage_prefix: &storage.prefix,
                        producer: *id,
                        metadata: &input.publication,
                        reader: &frozen.reader,
                        components: &frozen.components,
                        limits,
                    },
                    request,
                    check,
                )
                .await?
            }
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        };
        // The returned receipt records the catalog's known effect, even after a final guard failure.
        Ok(json!({"status":"published","publication":receipt,"project_metadata":project_metadata}))
    }
    .await;
    operation
        .map_err(|cause| crate::hub_project_metadata::artifact_failure(project_metadata, cause))
}

enum Input {
    Native(Arguments),
    File(super::file_publication::Arguments),
}
impl Input {
    fn project_dir(&self) -> &std::path::Path {
        match self {
            Self::Native(input) => input.project_dir.as_path(),
            Self::File(input) => input.project_dir.as_path(),
        }
    }
    fn project_version(&self) -> Option<&str> {
        match self {
            Self::Native(input) => input.project_version.as_deref(),
            Self::File(input) => input.project_version.as_deref(),
        }
    }
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let is_file = call.name == super::file_publication::TOOL;
    let arguments = Value::Object(call.arguments.unwrap_or_default());
    let input = if is_file {
        serde_json::from_value(arguments).map(Input::File)
    } else if call.name == TOOL {
        serde_json::from_value(arguments).map(Input::Native)
    } else {
        return failure("invalid_arguments");
    };
    let input = match input {
        Ok(input) => input,
        Err(_) => return failure("invalid_arguments"),
    };
    if !input.project_dir().is_absolute() {
        return failure("invalid_arguments");
    }
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return failure("selection_unavailable"),
    };
    let selected = match select_project(
        &snapshot.projects,
        input.project_dir(),
        input.project_version(),
    ) {
        Ok(selected) => selected,
        Err(code) => return failure(code),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("incoming_credentials_required");
    };
    if parts.headers.contains_key(axum::http::header::ORIGIN) {
        return failure("browser_origin_not_allowed");
    }
    if parts
        .headers
        .get_all(axum::http::header::AUTHORIZATION)
        .iter()
        .count()
        != 1
        || parts
            .headers
            .get_all("x-graphit-auth-origin")
            .iter()
            .count()
            != 1
    {
        return failure("ambiguous_or_missing_credentials");
    }
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure("credential_origin_required"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return failure("credential_required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let handle = Handle::current();
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {jobs=state.jobs.lock()=>jobs,()=context.ct.cancelled()=>return failure("request_cancelled_or_expired"),()=state.stop.cancelled()=>return failure("stopped"),()=tokio::time::sleep_until(deadline.into())=>return failure("request_cancelled_or_expired")};
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("stopped");
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(async {
                let request = RequestContext::new(owner.policy.content, &original_cancel, deadline)
                    .map_err(io::Error::other)?;
                match input {
                    Input::Native(input) => {
                        execute(
                            &owner,
                            &snapshot.projects[selected],
                            input,
                            &credential,
                            local,
                            &request,
                        )
                        .await
                    }
                    Input::File(input) => {
                        super::file_publication::execute(
                            &owner,
                            &snapshot.projects[selected],
                            input,
                            &credential,
                            local,
                            &request,
                        )
                        .await
                    }
                }
            });
            let _ = send.send(result);
        });
    }
    tokio::select! {result=receive=>match result{Ok(Ok(payload))=>crate::mcp_output::typed(payload),Ok(Err(error))=>if is_file {super::file_publication::operation_failure(error)} else {operation_failure(error)},Err(_)=>failure(if is_file {"file_job_failed"} else {"native_job_failed"})},()=context.ct.cancelled()=>failure("request_cancelled_or_expired"),()=state.stop.cancelled()=>failure("stopped"),()=tokio::time::sleep_until(deadline.into())=>failure("request_cancelled_or_expired")}
}
