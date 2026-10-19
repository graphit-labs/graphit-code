//! Native package export reads one admitted immutable graph and publishes one file.
use super::*;
use graphit_auth::{
    ExternalEffect, Obligation, Permission, ProjectReadModule, ProjectRecordAuthority, ScopeInput,
};
use graphit_core::native_path::NativePath;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use serde_json::{Value, json};
use std::path::{Component, Path, PathBuf};

mod live;

pub(super) const TOOL: &str = "graphit_ast_export";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    output: NativePath,
    context: Option<String>,
    project_dir: Option<NativePath>,
    project_version: Option<String>,
}
fn failure(code: &str, published: Option<Value>) -> CallToolResult {
    let mut value = json!({"status":"error","code":code});
    if let Some(published) = published {
        value["published"] = published;
    }
    crate::mcp_output::typed_error(value)
}
// Resolve the existing parent once, reject self-publication before creating a temporary file.
fn destination(input: &Path, source: &Path) -> io::Result<PathBuf> {
    if !input.is_absolute() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let name = input.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    if !matches!(
        Path::new(name).components().next(),
        Some(Component::Normal(_))
    ) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let parent = input
        .parent()
        .ok_or(io::ErrorKind::InvalidInput)?
        .canonicalize()?;
    let path = parent.join(name);
    if path.starts_with(source) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "package output is inside the captured graph",
        ));
    }
    Ok(path)
}
impl GraphServer {
    /// Retain the caller's actual PROJECT admission while exporting its current
    /// FILE cohort. The returned staging owner must live through upload/package.
    pub(crate) async fn capture_current_export(
        &self,
        capture: &crate::project_graph::PreparedRead,
        paths: &graphit_core::project_paths::ProjectPaths,
        base: &graphit_core::config::ConfigSnapshot,
        selections: &crate::live_project_selections::LiveProjectSelections,
        original: &Arc<graphit_core::file_io::SnapshotContext>,
    ) -> Result<
        (
            graphit_data::graph_writer::IcebugOutput,
            Arc<graphit_data::CapturedLocalGraph>,
        ),
        crate::project_graph::ReadError,
    > {
        if paths.root() != capture.project().version.scope() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied).into());
        }
        capture
            .open_live(
                self.0.auth.installation(),
                &self.0.live_ast,
                &self.0.policy,
                base,
                selections,
            )
            .await?;
        let index = capture
            .live_index()
            .ok_or(crate::project_graph::ReadError::Absent)?;
        let config = crate::ast_profile_loader::project_config(
            &self.0.policy.installation,
            paths,
            base,
            original,
        )?;
        let (output, graph) = live::capture(&self.0, capture, index, &config, original).await?;
        capture.check(&self.0.policy, base, selections)?;
        Ok((output, graph))
    }

    pub(super) async fn call_export_mcp(
        &self,
        call: CallToolRequestParams,
        headers: HeaderMap,
        cancelled: CancellationToken,
    ) -> CallToolResult {
        let arguments = Value::Object(call.arguments.unwrap_or_default());
        if cancelled.is_cancelled() {
            return failure("request_cancelled_or_expired", None);
        }
        if serde_json::to_vec(&arguments).map_or(true, |bytes| {
            bytes.len() > self.0.policy.content.max_input_bytes
        }) {
            return failure("invalid_arguments", None);
        }
        let input: Input = match serde_json::from_value(arguments) {
            Ok(input) => input,
            Err(_) => return failure("invalid_arguments", None),
        };
        if !input.output.as_path().is_absolute()
            || input.context.is_some()
                && (input.project_dir.is_some() || input.project_version.is_some())
            || input.project_version.as_deref().is_some_and(|version| {
                version.is_empty()
                    || version.contains('\0')
                    || version.len() > self.0.policy.content.max_scalar_bytes
            })
        {
            return failure("invalid_arguments", None);
        }
        let credentials = match credential(&headers) {
            Ok(value) => value,
            Err(_) => return failure("authentication_or_permission_denied", None),
        };
        let cancel = Cancellation::default();
        let _guard = CancelOnDrop(cancel.clone());
        let Some(deadline) = Instant::now().checked_add(self.0.policy.timeout) else {
            return failure("invalid_deadline", None);
        };
        let state = Arc::clone(&self.0);
        let request_cancel = cancel.clone();
        let (send, receive) = oneshot::channel();
        {
            let mut jobs = tokio::select! {
                jobs=state.jobs.lock()=>jobs,
                ()=cancelled.cancelled()=>return failure("request_cancelled_or_expired",None),
                ()=state.stop.cancelled()=>return failure("stopped",None),
                ()=tokio::time::sleep_until(deadline.into())=>return failure("request_cancelled_or_expired",None),
            };
            while jobs.try_join_next().is_some() {}
            let owner = Arc::clone(&state);
            jobs.spawn_blocking(move || {
                let mut published = None;
                let result = export(
                    &owner,
                    &credentials.0,
                    credentials.1,
                    input,
                    &request_cancel,
                    deadline,
                    &mut published,
                );
                let result = match result {
                    Ok(value) => crate::mcp_output::typed(value),
                    Err(error) => {
                        let status = error.into_response().status();
                        let code = match status {
                            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => {
                                "authentication_or_permission_denied"
                            }
                            StatusCode::BAD_REQUEST => "invalid_arguments",
                            StatusCode::NOT_FOUND => "index_absent",
                            StatusCode::GATEWAY_TIMEOUT | StatusCode::SERVICE_UNAVAILABLE => {
                                "request_cancelled_or_expired"
                            }
                            _ => "ast_export_failed",
                        };
                        failure(code, published)
                    }
                };
                let _ = send.send(result);
            });
        }
        tokio::select! {
            result=receive=>result.unwrap_or_else(|_|failure("native_job_failed",None)),
            ()=cancelled.cancelled()=>failure("request_cancelled_or_expired",None),
            ()=state.stop.cancelled()=>failure("stopped",None),
            ()=tokio::time::sleep_until(deadline.into())=>failure("request_cancelled_or_expired",None),
        }
    }
}
fn export(
    state: &Arc<Shared>,
    credential: &str,
    local: bool,
    input: Input,
    cancel: &Cancellation,
    deadline: Instant,
    published: &mut Option<Value>,
) -> Result<Value, ReadFailure> {
    let original = RequestContext::new(state.policy.content, cancel, deadline)
        .map_err(|_| ReadFailure::Context)?;
    original.check().map_err(|_| ReadFailure::Context)?;
    if state.stop.is_cancelled() {
        return Err(ReadFailure::Context);
    }
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, &original)
    } else {
        issuer.authenticate_runtime(credential, &original)
    }
    .map_err(|error| ReadFailure::Imported(ImportedGraphError::Authentication(error)))?;
    caller
        .authorize(Permission::Read(graphit_core::module_settings::Module::Ast))
        .map_err(|error| ReadFailure::Imported(ImportedGraphError::Authentication(error)))?;
    if caller
        .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
        .map_err(|error| ReadFailure::Imported(ImportedGraphError::Authentication(error)))?
        != Obligation::Unconditional
    {
        return Err(ReadFailure::Imported(ImportedGraphError::Authentication(
            graphit_auth::AuthError::Denied,
        )));
    }
    let write = |graph: &graphit_data::CapturedLocalGraph| -> Result<Value, ReadFailure> {
        original.check().map_err(|_| ReadFailure::Context)?;
        let reader = graph
            .source_reader(original.snapshot())
            .map_err(|error| ReadFailure::Output(io::Error::other(error)))?;
        let path = destination(
            input.output.as_path(),
            reader
                .source_path(original.snapshot())
                .map_err(ReadFailure::Output)?,
        )
        .map_err(|error| {
            if error.kind() == io::ErrorKind::InvalidInput {
                ReadFailure::BadInput
            } else {
                ReadFailure::Output(error)
            }
        })?;
        let output = NativePath::new(path.clone()).map_err(ReadFailure::Output)?;
        let result = json!({"status":"exported","artifact_type":"ast","output":output});
        graphit_hub::native_package::export_ast_graph(reader, &path, original.snapshot())
            .map_err(|error| ReadFailure::Output(io::Error::other(error)))?;
        Ok(result)
    };
    let result = if let Some(context) = input.context.as_deref() {
        let (id, version) = context.rsplit_once('@').ok_or(ReadFailure::BadInput)?;
        let projection = state.policy.registry.limits().projection();
        let id = ArtifactId::new(id, projection).map_err(|_| ReadFailure::BadInput)?;
        let version =
            ConcreteVersion::new(version, projection).map_err(|_| ReadFailure::BadInput)?;
        let read = issuer
            .admit_authenticated_imported_ast_read(&caller, state.auth.installation())
            .map_err(|error| ReadFailure::Imported(ImportedGraphError::Authentication(error)))?;
        let graph = graphit_ast::imported_graph::open_imported_graph_admitted(
            &read,
            ImportedGraphInput {
                registry: &state.policy.registry,
                id: &id,
                version: &version,
                installation: read.installation(),
                file_policy: &state.policy.file,
                read_limits: state.policy.read,
                graph_policy: state.policy.native_install.graph,
                directory_limits: state.policy.index,
                request: &original,
            },
        )?;
        let reader = graph.reader()?;
        let result = write(reader.as_ref())?;
        *published = Some(result.clone());
        graph.revalidate()?;
        read.recheck()
            .map_err(|error| ReadFailure::Imported(ImportedGraphError::Authentication(error)))?;
        result
    } else {
        let access = state.project.as_ref().ok_or(ReadFailure::Context)?;
        let snapshot = access.live.snapshot().map_err(ReadFailure::Output)?;
        let selection = ProjectSelection {
            project_dir: input.project_dir,
            project_version: input.project_version,
        };
        let index = select_project(state, &selection, &snapshot)?;
        let selected = &snapshot.projects[index];
        let capture = crate::project_graph::PreparedRead::capture(
            &state.policy,
            &access.live,
            (&selected.0, selected.1, &selected.2),
            Arc::clone(original.snapshot()),
        )
        .map_err(ReadFailure::Project)?;
        capture
            .check(&state.policy, &access.config, &access.live)
            .map_err(ReadFailure::Project)?;
        let authority = ProjectRecordAuthority::new(issuer);
        let read = authority
            .admit_authenticated_read(
                &caller,
                &original,
                ScopeInput {
                    installation: state.auth.installation(),
                    project: Some(capture.project()),
                    providers: &[],
                    binding: state.policy.binding,
                    global_version: "",
                },
                ProjectReadModule::Ast,
            )
            .map_err(|error| {
                ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
            })?;
        read.recheck().map_err(|error| {
            ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
        })?;
        let live_root = crate::foreground_ast_sources::progressive::location(
            state.auth.installation(),
            capture.project(),
        )
        .map_err(ReadFailure::Output)?
        .0;
        destination(input.output.as_path(), &live_root).map_err(|error| {
            if error.kind() == io::ErrorKind::InvalidInput {
                ReadFailure::BadInput
            } else {
                ReadFailure::Output(error)
            }
        })?;
        let (_staged, graph) = tokio::runtime::Handle::current()
            .block_on(GraphServer(Arc::clone(state)).capture_current_export(
                &capture,
                &selected.0,
                &access.config,
                &access.live,
                original.snapshot(),
            ))
            .map_err(ReadFailure::Project)?;
        let result = write(&graph)?;
        *published = Some(result.clone());
        read.recheck().map_err(|error| {
            ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
        })?;
        capture
            .check(&state.policy, &access.config, &access.live)
            .map_err(ReadFailure::Project)?;
        result
    };
    if caller
        .authorize(Permission::External(ExternalEffect::PublishedFilesystem))
        .map_err(|error| ReadFailure::Imported(ImportedGraphError::Authentication(error)))?
        != Obligation::Unconditional
    {
        return Err(ReadFailure::Imported(ImportedGraphError::Authentication(
            graphit_auth::AuthError::Denied,
        )));
    }
    original.check().map_err(|_| ReadFailure::Context)?;
    if state.stop.is_cancelled() {
        return Err(ReadFailure::Context);
    }
    Ok(result)
}
