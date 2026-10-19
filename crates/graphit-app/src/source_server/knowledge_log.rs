//! Root PROJECT synchronization history through the current Source job owner.
use super::*;
use crate::knowledge_log::{Input, failure as history_failure, view};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority};
use graphit_core::native_path::NativePath;
use graphit_data::knowledge_index::{KnowledgeIndexContentLimits, KnowledgeIndexReadLimits};
use std::num::{NonZeroU32, NonZeroUsize};

pub(super) const TOOL: &str = "graphit_wiki_log";
pub(super) fn definition() -> Tool {
    Tool::new(TOOL,"Read committed Knowledge document synchronization history for the authorized PROJECT and version. Native cursors grant no access; semantic-only transactions are excluded.",json!({
        "type":"object","properties":{
            "history":{"type":"object","properties":{"limit":{"type":"integer","minimum":1},"cursor":{"type":"string"}},"required":["limit"],"additionalProperties":false},
            "project_dir":{"type":"string"},"project_version":{"type":"string"}
        },"required":["history"],"additionalProperties":false,
    }).as_object().expect("closed history schema").clone())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    history: Input,
    #[serde(default, deserialize_with = "present")]
    project_dir: Option<NativePath>,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
}
impl Arguments {
    fn select(
        &self,
        state: &State,
        snapshot: &crate::live_project_selections::LiveProjectSnapshot,
    ) -> Result<usize, Value> {
        self.history.validate(state.policy.content)?;
        if self.project_version.as_deref().is_some_and(|v| {
            v.is_empty() || v.contains('\0') || v.len() > state.policy.content.max_scalar_bytes
        }) {
            return Err(code("invalid_project_version"));
        }
        let current = snapshot
            .current
            .ok_or_else(|| code("selection_unavailable"))?;
        let root = self
            .project_dir
            .as_ref()
            .map(NativePath::as_path)
            .unwrap_or(snapshot.projects[current].0.root());
        if self.project_version.is_none() && root == snapshot.projects[current].0.root() {
            Ok(current)
        } else {
            select_read_project(&snapshot.projects, root, self.project_version.as_deref())
                .map_err(|_| code("selection_unavailable"))
        }
    }
}
fn code(value: &'static str) -> Value {
    json!({"code":value})
}
fn failure(mut value: Value) -> CallToolResult {
    value["status"] = "error".into();
    crate::mcp_output::typed_error(value)
}
fn io_failure(cause: io::Error) -> Value {
    match cause.kind() {
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => {
            code("request_cancelled_or_expired")
        }
        io::ErrorKind::PermissionDenied => code("authentication_or_permission_denied"),
        _ => json!({"code":"history_capture_failed","io_kind":format!("{:?}",cause.kind())}),
    }
}
async fn execute(
    state: &State,
    selected: &(ProjectPaths, ProjectId, VersionContext),
    input: Arguments,
    credential: &str,
    local: bool,
    request: &RequestContext<'_>,
) -> Result<Value, Value> {
    request.check().map_err(io_failure)?;
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, request)
    } else {
        issuer.authenticate_runtime(credential, request)
    }
    .map_err(|_| code("authentication_or_permission_denied"))?;
    let (paths, id, version) = selected;
    if !state
        .live_projects
        .is_active(paths, *id, version)
        .map_err(io_failure)?
    {
        return Err(code("selection_unavailable"));
    }
    let capture =
        CapturedPublicationScope::project(paths, *id, state.policy.scope, request.snapshot())
            .map_err(io_failure)?;
    let record = capture.binding().map_err(io_failure)?;
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(
            &caller,
            request,
            ScopeInput {
                installation: state.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &capture,
                        record: &record,
                    },
                    version,
                }),
                providers: &[],
                binding: state.policy.binding,
                global_version: "",
            },
            ProjectReadModule::Knowledge,
        )
        .map_err(|_| code("authentication_or_permission_denied"))?;
    let limits = KnowledgeIndexReadLimits {
        native: state.policy.index,
        batch_rows: NonZeroU32::new(8192).expect("native scanner batch"),
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: NonZeroUsize::new(state.policy.content.max_scalar_bytes)
                .ok_or_else(|| code("invalid_policy"))?,
        },
    };
    let history = graphit_data::knowledge_sync_history::read_native_knowledge_sync_history(
        request.snapshot(),
        &read,
        input.history.native(),
        limits,
    )
    .await
    .map_err(history_failure)?;
    let value = view(history);
    read.recheck()
        .map_err(|_| code("authentication_or_permission_denied"))?;
    if !state
        .live_projects
        .is_active(paths, *id, version)
        .map_err(io_failure)?
    {
        return Err(code("selection_unavailable"));
    }
    request.check().map_err(io_failure)?;
    if serde_json::to_vec(&value)
        .map_err(|_| code("response_encoding_failed"))?
        .len()
        > state.policy.content.max_output_bytes
    {
        return Err(code("response_limit"));
    }
    Ok(value)
}
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input: Arguments =
        match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
            Ok(value) => value,
            Err(_) => return failure(code("invalid_arguments")),
        };
    let snapshot = match server.0.live_projects.snapshot() {
        Ok(value) => value,
        Err(_) => return failure(code("selection_unavailable")),
    };
    let selected = match input.select(&server.0, &snapshot) {
        Ok(value) => value,
        Err(value) => return failure(value),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure(code("incoming_credentials_required"));
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure(code("credential_origin_required")),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return failure(code("credential_required"));
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
        let mut jobs = tokio::select! {
            jobs=state.jobs.lock()=>jobs,
            ()=context.ct.cancelled()=>return failure(code("request_cancelled_or_expired")),
            ()=state.stop.cancelled()=>return failure(code("stopped")),
            ()=tokio::time::sleep_until(deadline.into())=>return failure(code("request_cancelled_or_expired")),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure(code("stopped"));
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(async {
                let request = RequestContext::new(owner.policy.content, &original_cancel, deadline)
                    .map_err(|cause| {
                        use graphit_hub::{native::ContentError, policy::RequestError};
                        let kind = match &cause {
                            RequestError::Content(ContentError::Cancelled) => {
                                io::ErrorKind::Interrupted
                            }
                            RequestError::Content(ContentError::TimedOut) => {
                                io::ErrorKind::TimedOut
                            }
                            _ => io::ErrorKind::InvalidInput,
                        };
                        io_failure(io::Error::new(kind, cause))
                    })?;
                execute(
                    &owner,
                    &snapshot.projects[selected],
                    input,
                    &credential,
                    local,
                    &request,
                )
                .await
            });
            let _ = send.send(result);
        });
    }
    tokio::select! {
        result=receive=>match result{
            Ok(_)if context.ct.is_cancelled()||cancel.is_cancelled()||state.stop.is_cancelled()=>failure(code("request_cancelled_or_expired")),
            Ok(_)if Instant::now()>=deadline=>failure(code("request_cancelled_or_expired")),
            Ok(Ok(value))=>crate::mcp_output::typed(value),Ok(Err(value))=>failure(value),Err(_)=>failure(code("native_job_failed")),
        },
        ()=context.ct.cancelled()=>failure(code("request_cancelled_or_expired")),
        ()=state.stop.cancelled()=>failure(code("stopped")),
        ()=tokio::time::sleep_until(deadline.into())=>failure(code("request_cancelled_or_expired")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_project_version_log_uses_existing_context_and_preserves_exact_liveness() {
        let fixture = super::super::test_fixture::fixture(
            &[graphit_auth::Permission::Read(
                graphit_core::module_settings::Module::Knowledge,
            )],
            graphit_core::brand::Brand::default(),
            NativeBinding::Codex,
        );
        let (origin, id, current) = fixture.projects[0].clone();
        let root = fixture.temp.path().join("retained");
        std::fs::create_dir(&root).unwrap();
        let retained = ProjectPaths::at(&root, &fixture.paths).unwrap();
        graphit_core::project_identity::ProjectIdentityStore::at(&retained)
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .ensure_with_id(
                &graphit_core::project_identity::ProjectMetadata::new("project").unwrap(),
                id,
            )
            .unwrap();
        let checkout = graphit_core::git_checkout::GitCheckout::inspect(
            &root,
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
        let version =
            VersionContext::select(&retained, checkout.as_checkout(), "retained", None, "")
                .unwrap();
        fixture
            .server
            .0
            .live_projects
            .replace(
                Arc::new(vec![
                    (origin.clone(), id, current),
                    (retained.clone(), id, version.clone()),
                ]),
                0,
            )
            .unwrap();
        let input: Arguments = serde_json::from_value(json!({
            "history":{"limit":1}, "project_dir":origin.root(), "project_version":"retained",
        }))
        .unwrap();
        let snapshot = fixture.server.0.live_projects.snapshot().unwrap();
        assert_eq!(input.select(&fixture.server.0, &snapshot).unwrap(), 1);
        assert!(
            fixture
                .server
                .0
                .live_projects
                .is_active(&retained, id, &version)
                .unwrap()
        );
        let absent: Arguments = serde_json::from_value(json!({
            "history":{"limit":1}, "project_dir":origin.root(), "project_version":"absent",
        }))
        .unwrap();
        assert!(absent.select(&fixture.server.0, &snapshot).is_err());
        assert!(
            super::super::select_project(&snapshot.projects, origin.root(), Some("retained"))
                .is_err()
        );
        let other_root = fixture.temp.path().join("other-retained");
        std::fs::create_dir(&other_root).unwrap();
        let other = ProjectPaths::at(&other_root, &fixture.paths).unwrap();
        graphit_core::project_identity::ProjectIdentityStore::at(&other)
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .ensure_with_id(
                &graphit_core::project_identity::ProjectMetadata::new("project").unwrap(),
                id,
            )
            .unwrap();
        let checkout = graphit_core::git_checkout::GitCheckout::inspect(
            &other_root,
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
        let other_version =
            VersionContext::select(&other, checkout.as_checkout(), "retained", None, "").unwrap();
        let mut ambiguous = snapshot.projects.as_ref().clone();
        ambiguous.push((other, id, other_version));
        fixture
            .server
            .0
            .live_projects
            .replace(Arc::new(ambiguous), 0)
            .unwrap();
        let expanded = fixture.server.0.live_projects.snapshot().unwrap();
        assert!(input.select(&fixture.server.0, &expanded).is_err());
        let exact: Arguments = serde_json::from_value(json!({
            "history":{"limit":1}, "project_dir":retained.root(), "project_version":"retained",
        }))
        .unwrap();
        assert_eq!(exact.select(&fixture.server.0, &expanded).unwrap(), 1);
        fixture
            .server
            .0
            .live_projects
            .replace(Arc::clone(&snapshot.projects), 0)
            .unwrap();
        fixture
            .server
            .0
            .live_projects
            .retire_checkout(id, retained.root())
            .unwrap();
        assert!(
            !fixture
                .server
                .0
                .live_projects
                .is_active(&retained, id, &version)
                .unwrap()
        );
        assert!(
            input
                .select(
                    &fixture.server.0,
                    &fixture.server.0.live_projects.snapshot().unwrap()
                )
                .is_err()
        );
    }
}
