//! Native PROJECT Hub/read admission for the shared authored Resources reader.
use super::*;
use crate::authored_resources::{
    self as resources,
    inputs::{Operation, Query},
};
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority};
use graphit_core::{artifact_store::CapturedPublicationScope, native_path::NativePath};

fn operation(name: &str) -> Option<Operation> {
    match name {
        resources::LIST => Some(Operation::List),
        resources::REVIEW => Some(Operation::Review),
        resources::CONTENT => Some(Operation::Content),
        _ => None,
    }
}
pub(super) fn definition(name: &str) -> Option<Tool> {
    let operation = operation(name)?;
    let kinds = json!({"type":"string","enum":["agent","ast","command","knowledge","language","mcp","power","rule","skill","workflow"]});
    let mut query = json!({"type":"object","properties":{
        "artifact_type":kinds,"id":{"type":"string"},"version":{"type":"string"}
    },"additionalProperties":false});
    let description = match operation {
        Operation::List => {
            query["properties"]["limit"] = json!({"type":"integer","minimum":1});
            query["properties"]["cursor"] = json!({"type":"string"});
            query["required"] = json!(["limit"]);
            "List authored Resources identities in an authorized retained PROJECT/version. Returns exact kind/id/version tuples and a bound pagination cursor; no provider or remote storage access."
        }
        Operation::Review => {
            query["properties"]["offset"] = json!({"type":"integer","minimum":0});
            query["properties"]["limit"] = json!({"type":"integer","minimum":1});
            query["required"] = json!(["artifact_type", "id", "version", "limit"]);
            "Review one exact authored Resources tuple in an authorized PROJECT/version. Lists captured relative files and directories; no host paths or remote storage access."
        }
        Operation::Content => {
            query["properties"]["path"] = json!({"type":"string","description":"Text file path relative to the selected Resources tree."});
            query["required"] = json!(["artifact_type", "id", "version", "path"]);
            "Read a relative text file from one exact authored Resources tuple in an authorized PROJECT/version. No installed artifact or arbitrary host path fallback."
        }
    };
    Some(Tool::new(
        name.to_owned(),
        description,
        json!({"type":"object","properties":{
        "project_dir":{"type":"string"},
        "project_version":{"type":"string"},"query":query
    },"required":["project_dir","query"],"additionalProperties":false})
        .as_object()
        .expect("static schema")
        .clone(),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    project_dir: NativePath,
    #[serde(default, deserialize_with = "present")]
    project_version: Option<String>,
    query: serde_json::Map<String, Value>,
}
impl Arguments {
    fn query(mut self, operation: Operation, id: ProjectId, version: &str) -> io::Result<Query> {
        if self.query.contains_key("project_id") || self.query.contains_key("project_version") {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.query.insert("project_id".into(), json!(id));
        self.query.insert("project_version".into(), json!(version));
        Query::decode(
            &serde_json::to_vec(&self.query).map_err(io::Error::other)?,
            operation,
        )
    }
}
fn failure(code: &str, limit: usize) -> CallToolResult {
    let body = json!({"status":"error","code":code});
    if serde_json::to_vec(&body).is_ok_and(|bytes| bytes.len() <= limit) {
        crate::mcp_output::typed_error(body)
    } else {
        CallToolResult::error(Vec::new())
    }
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
fn operation_failure(error: io::Error, limit: usize) -> CallToolResult {
    let code = match error
        .get_ref()
        .and_then(|error| error.downcast_ref::<resources::ReadFailure>())
    {
        Some(resources::ReadFailure::SourceMissing) => "authored_source_not_found",
        Some(resources::ReadFailure::FileMissing) => "authored_file_not_found",
        Some(resources::ReadFailure::NotText) => "authored_content_not_text",
        None => match error.kind() {
            io::ErrorKind::PermissionDenied => "authentication_or_permission_denied",
            io::ErrorKind::Interrupted => "request_cancelled",
            io::ErrorKind::TimedOut => "request_deadline_expired",
            io::ErrorKind::InvalidInput => "invalid_arguments",
            io::ErrorKind::InvalidData => "invalid_authored_source",
            io::ErrorKind::NotFound => "project_unavailable",
            io::ErrorKind::AlreadyExists => "project_selection_ambiguous",
            io::ErrorKind::FileTooLarge => "authored_resources_too_large",
            io::ErrorKind::NotConnected => "stopped",
            _ => "authored_resources_failed",
        },
    };
    failure(code, limit)
}
fn execute(
    state: &State,
    input: Arguments,
    operation: Operation,
    credential: &str,
    local: bool,
    original: &RequestContext<'_>,
) -> io::Result<Value> {
    original.check()?;
    let projects = state.live_projects.snapshot()?;
    let index = select_project(
        &projects.projects,
        input.project_dir.as_path(),
        input.project_version.as_deref(),
    )
    .map_err(|code| {
        io::Error::from(if code.ends_with("ambiguous") {
            io::ErrorKind::AlreadyExists
        } else {
            io::ErrorKind::NotFound
        })
    })?;
    let (paths, id, version) = projects
        .projects
        .get(index)
        .ok_or(io::ErrorKind::NotFound)?;
    let query = input.query(operation, *id, version.version())?;
    let policy = &state.policy.native_install.hub;
    query.validate(policy)?;
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, original)
    } else {
        issuer.authenticate_runtime(credential, original)
    }
    .map_err(denied)?;
    let scope =
        CapturedPublicationScope::project(paths, *id, state.policy.scope, original.snapshot())?;
    let binding = scope.binding()?;
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_read(
            &caller,
            original,
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
            ProjectReadModule::Hub,
        )
        .map_err(denied)?;
    let mut before = || -> io::Result<()> {
        original.check()?;
        if state.stop.is_cancelled() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        admission.recheck().map_err(denied)?;
        if !state.live_projects.is_active(paths, *id, version)? {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(())
    };
    before()?;
    let bytes = resources::read((paths, *id), &scope, query, policy, original, &mut before)?;
    let body = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    before()?;
    Ok(body)
}
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let limit = server.0.policy.content.max_output_bytes;
    let Some(operation) = operation(call.name.as_ref()) else {
        return failure("invalid_arguments", limit);
    };
    let body = Value::Object(call.arguments.unwrap_or_default());
    if serde_json::to_vec(&body).map_or(true, |bytes| {
        bytes.len() > server.0.policy.content.max_input_bytes
    }) {
        return failure("invalid_arguments", limit);
    }
    let input: Arguments = match serde_json::from_value(body) {
        Ok(input) => input,
        Err(_) => return failure("invalid_arguments", limit),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("incoming_credentials_required", limit);
    };
    if parts.headers.contains_key(axum::http::header::ORIGIN) {
        return failure("browser_origin_not_allowed", limit);
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
        return failure("ambiguous_or_missing_credentials", limit);
    }
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|value| value.to_str().ok())
    {
        Some("local") => true,
        Some("runtime") => false,
        _ => return failure("credential_origin_required", limit),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
    else {
        return failure("credential_required", limit);
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let (send, mut receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! { jobs=state.jobs.lock()=>jobs,
        ()=context.ct.cancelled()=>return failure("request_cancelled",limit),
        ()=state.stop.cancelled()=>return failure("stopped",limit),
        ()=tokio::time::sleep_until(deadline.into())=>return failure("request_deadline_expired",limit) };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("stopped", limit);
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = (|| {
                let original =
                    RequestContext::new(owner.policy.content, &original_cancel, deadline)
                        .map_err(io::Error::other)?;
                execute(&owner, input, operation, &credential, local, &original)
            })();
            let _ = send.send(result);
        });
    }
    let (result, terminal) = tokio::select! {
        result=&mut receive=>(result,None),
        ()=context.ct.cancelled()=>{cancel.cancel();(receive.await,Some("request_cancelled"))},
        ()=state.stop.cancelled()=>{cancel.cancel();(receive.await,Some("stopped"))},
        ()=tokio::time::sleep_until(deadline.into())=>{cancel.cancel();(receive.await,Some("request_deadline_expired"))},
    };
    let terminal = terminal.or_else(|| {
        if context.ct.is_cancelled() {
            Some("request_cancelled")
        } else if state.stop.is_cancelled() {
            Some("stopped")
        } else if Instant::now() >= deadline {
            Some("request_deadline_expired")
        } else {
            None
        }
    });
    if let Some(code) = terminal {
        return failure(code, limit);
    }
    match result {
        Ok(Ok(body)) => crate::mcp_output::typed(body),
        Ok(Err(error)) => operation_failure(error, limit),
        Err(_) => failure("authored_resources_job_failed", limit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(query: Value) -> Value {
        json!({"project_dir":NativePath::new(std::env::current_dir().unwrap()).unwrap(),"project_version":"main","query":query})
    }
    #[test]
    fn native_selection_cannot_be_shadowed_by_query_fields() {
        let id = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();
        for field in ["project_id", "project_version"] {
            let mut query = json!({"limit":1});
            query[field] = json!("other");
            let arguments: Arguments = serde_json::from_value(input(query)).unwrap();
            assert!(arguments.query(Operation::List, id, "main").is_err());
        }
        let mut outer = input(json!({"limit":1}));
        outer["provider"] = json!("unexpected");
        assert!(serde_json::from_value::<Arguments>(outer).is_err());
        let mut outer = input(json!({"limit":1}));
        outer["project_version"] = Value::Null;
        assert!(serde_json::from_value::<Arguments>(outer).is_err());
        let arguments: Arguments = serde_json::from_value(input(json!({"limit":1}))).unwrap();
        let query = arguments
            .query(Operation::List, id, "selected-version")
            .unwrap();
        assert_eq!(query.selection(), (id, "selected-version"));
    }
    #[test]
    fn native_read_errors_do_not_expose_private_paths_or_overflow_output() {
        let result = operation_failure(
            io::Error::new(io::ErrorKind::PermissionDenied, "private host path"),
            4096,
        );
        assert_eq!(
            result.structured_content.unwrap(),
            json!({"status":"error","code":"authentication_or_permission_denied"})
        );
        for limit in [1, 16] {
            let result = failure("authored_resources_failed", limit);
            assert_eq!(result.is_error, Some(true));
            assert!(result.structured_content.is_none());
            assert!(result.content.is_empty());
        }
    }
}
