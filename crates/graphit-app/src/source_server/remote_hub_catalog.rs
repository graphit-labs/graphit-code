//! Authenticated Source adapter for available remote Hub projects.
use super::*;
use crate::remote_file_install::client::Transport;
use crate::remote_hub_catalog::{
    self as catalog,
    client::{ProjectsArguments, SearchArguments},
};
use graphit_auth::{Obligation, Permission, ServiceMode};
use graphit_core::{artifact_store::ScopeContext, module_settings::Module};

pub(super) const TOOL: &str = "graphit_hub_projects";
pub(super) const SEARCH_TOOL: &str = "graphit_hub_search";
pub(super) fn search_definition() -> Tool {
    Tool::new(SEARCH_TOOL, "List current artifact manifests of an explicit authorized remote PROJECT. Exact kind/id/version filters; no payload reads.", json!({"type":"object","properties":{
        "transport":{"type":"string","enum":["broker","standalone"]},"provider":{"type":"string"},"profile":{"type":"string"},"identity_provider":{"type":"string"},"producer":{"type":"string"},
        "artifact_type":{"type":"string","enum":["agent","ast","command","knowledge","language","mcp","power","rule","skill","workflow"]},"id":{"type":"string"},"version":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":1000},"cursor":{"type":"string"}
    },"required":["transport","provider","producer","limit"],"additionalProperties":false}).as_object().expect("static schema").clone())
}
enum Query {
    Projects(ProjectsArguments),
    Search(SearchArguments),
}

pub(super) fn definition() -> Tool {
    Tool::new(TOOL,"Discover currently authorized remote Hub project metadata. Discovery does not grant artifact or payload access.",json!({"type":"object","properties":{
        "transport":{"type":"string","enum":["broker","standalone"]},"provider":{"type":"string"},"profile":{"type":"string"},"identity_provider":{"type":"string"},"id":{"type":"string"},"name_prefix":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":1000},"cursor":{"type":"string"}
    },"required":["transport","provider","limit"],"additionalProperties":false,"not":{"required":["id","name_prefix"]}}).as_object().expect("static schema").clone())
}
fn failure(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"status":"error","code":code}))
}
fn operation_failure(error: io::Error) -> CallToolResult {
    let code = if catalog::is_restart(&error) {
        "catalog_restart_required"
    } else {
        match error.kind() {
            io::ErrorKind::PermissionDenied => "authentication_or_permission_denied",
            io::ErrorKind::Interrupted => "request_cancelled",
            io::ErrorKind::TimedOut => "request_deadline_expired",
            io::ErrorKind::InvalidInput => "invalid_arguments",
            io::ErrorKind::InvalidData => "invalid_remote_catalog",
            io::ErrorKind::FileTooLarge => "catalog_too_large",
            io::ErrorKind::NotConnected => "catalog_not_configured",
            io::ErrorKind::NotFound => "catalog_not_found",
            _ => "catalog_request_failed",
        }
    };
    failure(code)
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
async fn execute(
    state: &State,
    input: Value,
    credential: &str,
    request: &RequestContext<'_>,
    search: bool,
) -> io::Result<Value> {
    let query = if search {
        Query::Search(
            serde_json::from_value(input)
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
        )
    } else {
        Query::Projects(
            serde_json::from_value(input)
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
        )
    };
    let policy = &state.policy.native_install.hub;
    let (transport, provider, profile, identity_provider) = match &query {
        Query::Projects(input) => {
            input.validate(policy)?;
            (
                input.transport,
                input.provider.as_str(),
                input.profile.as_deref(),
                input.identity_provider.as_deref(),
            )
        }
        Query::Search(input) => {
            input.validate(policy)?;
            (
                input.transport,
                input.provider.as_str(),
                input.profile.as_deref(),
                input.identity_provider.as_deref(),
            )
        }
    };
    let issuer = state.auth.issuer();
    let caller = issuer
        .authenticate_runtime(credential, request)
        .map_err(denied)?;
    if caller
        .authorize(Permission::Read(Module::Hub))
        .map_err(denied)?
        != Obligation::Unconditional
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut service = match transport {
        Transport::Broker => issuer.capture_hub_service(&caller, provider),
        Transport::Standalone => issuer.capture_standalone_s3_service(&caller, provider, profile),
    }
    .map_err(denied)?;
    let verifier = if service.mode().map_err(denied)? == ServiceMode::BrokerNative {
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .get(&mut service)
                .map_err(io::Error::other)?,
        )
    } else {
        None
    };
    let lease = match verifier.as_deref() {
        Some(verifier) => service.native_credential(verifier).await.map_err(denied)?,
        None => service.credential().map_err(denied)?,
    };
    let mut identity = identity_provider
        .map(|provider| issuer.capture_hub_service(&caller, provider))
        .transpose()
        .map_err(denied)?;
    let identity_verifier = if let Some(identity) = identity.as_mut() {
        if identity.mode().map_err(denied)? != ServiceMode::BrokerNative {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Some(
            state
                .native_verifiers
                .as_ref()
                .ok_or(io::ErrorKind::NotConnected)?
                .get(identity)
                .map_err(io::Error::other)?,
        )
    } else {
        None
    };
    let identity_lease = match (identity.as_mut(), identity_verifier.as_deref()) {
        (Some(identity), Some(verifier)) => {
            Some(identity.native_credential(verifier).await.map_err(denied)?)
        }
        _ => None,
    };
    let subject = match &identity_lease {
        Some(identity) => identity.user_subject().map_err(denied)?,
        None => lease.user_subject().map_err(denied)?,
    };
    let live = state.auth.installation().live;
    let before = || -> io::Result<()> {
        request.check()?;
        if issuer
            .authenticate_runtime(credential, request)
            .map_err(denied)?
            .authorize(Permission::Read(Module::Hub))
            .map_err(denied)?
            != Obligation::Unconditional
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        if let Some(identity) = &identity_lease {
            identity.check().map_err(denied)?;
        }
        live.revalidate_bounded(&ScopeContext::from_snapshot(request.snapshot()))?;
        Ok(())
    };
    before()?;
    let capture = catalog::Catalog {
        lease: &lease,
        subject,
        policy,
        http: &state.native_http,
        allow_loopback_http: state.native_allow_loopback_http,
        before: &before,
    };
    let mut result = match query {
        Query::Projects(input) => serde_json::to_value(catalog::projects(&capture, &input).await?)
            .map_err(io::Error::other)?,
        Query::Search(input) => {
            serde_json::to_value(catalog::artifacts::search(capture, &input).await?)
                .map_err(io::Error::other)?
        }
    };
    before()?;
    result["status"] = "ok".into();
    if serde_json::to_vec(&result).map_err(io::Error::other)?.len()
        > request.content_limits().max_output_bytes
    {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    before()?;
    Ok(result)
}
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let search = match call.name.as_ref() {
        TOOL => false,
        SEARCH_TOOL => true,
        _ => return failure("invalid_arguments"),
    };
    let input = Value::Object(call.arguments.unwrap_or_default());
    if serde_json::to_vec(&input)
        .map_or(true, |v| v.len() > server.0.policy.content.max_input_bytes)
    {
        return failure("invalid_arguments");
    }
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
    if parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
        != Some("runtime")
    {
        return failure("runtime_management_required");
    }
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
        let mut jobs = tokio::select! {jobs=state.jobs.lock()=>jobs,()=context.ct.cancelled()=>return failure("request_cancelled"),()=state.stop.cancelled()=>return failure("stopped"),()=tokio::time::sleep_until(deadline.into())=>return failure("request_deadline_expired")};
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("stopped");
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(async {
                let request = RequestContext::new(owner.policy.content, &original_cancel, deadline)
                    .map_err(io::Error::other)?;
                execute(&owner, input, &credential, &request, search).await
            });
            let _ = send.send(result);
        });
    }
    tokio::select! {result=receive=>match result{Ok(Ok(payload))=>crate::mcp_output::typed(payload),Ok(Err(error))=>operation_failure(error),Err(_)=>failure("catalog_job_failed")},()=context.ct.cancelled()=>failure("request_cancelled"),()=state.stop.cancelled()=>failure("stopped"),()=tokio::time::sleep_until(deadline.into())=>failure("request_deadline_expired")}
}
