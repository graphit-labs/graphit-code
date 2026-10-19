//! Explicit global NativeRoot installation on the existing Source request owner.
use super::*;
use graphit_auth::{ExternalEffect, Permission, ServiceMode};
use graphit_core::artifact_store::StoreOwner;
use graphit_hub::registry::HubScope;

pub(super) const TOOL: &str = "graphit_hub_install_native";
pub(super) fn definition() -> Tool {
    Tool::new(TOOL,"Install one exact remote NativeRoot under the authenticated installation; Runtime Native installation and HubRead authority are required separately.",json!({"type":"object","properties":{"installation":{"type":"object","properties":{"provider":{"type":"string"},"producer":{"type":"string"},"artifact_type":{"type":"string","enum":["ast","knowledge"]},"id":{"type":"string"},"version":{"type":"string"}},"required":["provider","producer","artifact_type","id","version"],"additionalProperties":false}},"required":["installation"],"additionalProperties":false}).as_object().unwrap().clone())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    installation: crate::native_hub_install::Selection,
}
fn failure(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"status":"error","code":code}))
}
fn operation_failure(error: io::Error) -> CallToolResult {
    let code = match error.kind() {
        io::ErrorKind::PermissionDenied => "authentication_or_permission_denied",
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut => "request_cancelled_or_expired",
        io::ErrorKind::InvalidInput => "invalid_arguments",
        io::ErrorKind::InvalidData => "invalid_remote_root_or_manifest",
        io::ErrorKind::NotFound => "native_release_not_found",
        _ => "native_installation_failed",
    };
    if let Some(invalid) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<crate::native_hub_install::InvalidRelease>())
    {
        return crate::mcp_output::typed_error(
            json!({"status":"error","code":code,"details":invalid}),
        );
    }
    if let Some(effect) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<crate::native_hub_install::InstallationFailure>())
    {
        return crate::mcp_output::typed_error(
            json!({"status":"error","code":code,"registry":effect.registry}),
        );
    }
    failure(code)
}
async fn execute(
    state: &State,
    input: Arguments,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    execute_selected(state, input, None, credential, request).await
}

pub(super) async fn update(
    state: &State,
    installation: crate::native_hub_install::Selection,
    previous: &graphit_hub::registry::Installation,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    execute_selected(
        state,
        Arguments { installation },
        Some(previous),
        credential,
        request,
    )
    .await
}

async fn execute_selected(
    state: &State,
    input: Arguments,
    previous: Option<&graphit_hub::registry::Installation>,
    credential: &str,
    request: &RequestContext<'_>,
) -> io::Result<Value> {
    input
        .installation
        .validate(&state.policy.native_install.hub)?;
    let installation = state.auth.installation();
    let StoreOwner::Installation { installation_id } = installation.live.owner() else {
        return Err(io::ErrorKind::PermissionDenied.into());
    };
    if installation.live.owner_root() != state.policy.installation.root()
        || state.policy.registry.path().parent() != Some(state.policy.installation.root())
        || state.policy.registry.scope()
            != &(HubScope::Global {
                installation_id: installation_id.as_str().to_owned(),
            })
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let issuer = state.auth.issuer();
    let caller = issuer
        .authenticate_runtime(credential, request)
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    // Retain the ordinary HubRead lease after its normal renewal. Capture a distinct
    // Native installation proof afterwards, so refresh cannot stale that proof.
    let mut service = issuer
        .capture_hub_service(&caller, &input.installation.provider)
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    if service.mode().map_err(io::Error::other)? != ServiceMode::BrokerNative {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let cache = state
        .native_verifiers
        .as_ref()
        .ok_or(io::ErrorKind::NotConnected)?;
    let verifier = cache.get(&mut service).map_err(io::Error::other)?;
    let lease = service
        .native_credential(&verifier)
        .await
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    let native_caller = issuer
        .authenticate_runtime(credential, request)
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?
        .authenticate_native(&verifier)
        .await
        .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    for permission in [
        Permission::Configure,
        Permission::ManageCache,
        Permission::External(ExternalEffect::PublishedFilesystem),
    ] {
        native_caller
            .authorize_native_installation(permission)
            .map_err(|cause| io::Error::new(io::ErrorKind::PermissionDenied, cause))?;
    }
    let storage = crate::native_storage_credentials::broker_credentials(
        &lease,
        input.installation.producer,
        &state.native_http,
        state.native_allow_loopback_http,
    )
    .await
    .map_err(io::Error::from)?;
    let input = crate::native_hub_install::InstallInput {
        selection: &input.installation,
        caller: &native_caller,
        lease: &lease,
        storage: &storage,
        installation: &state.policy.installation,
        registry: &state.policy.registry,
        policy: &state.policy.native_install.hub,
    };
    match previous {
        Some(previous) => crate::native_hub_install::update(input, previous, request).await,
        None => crate::native_hub_install::install(input, request).await,
    }
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input: Arguments =
        match serde_json::from_value(Value::Object(call.arguments.unwrap_or_default())) {
            Ok(input) => input,
            Err(_) => return failure("invalid_arguments"),
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
    if parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|value| value.to_str().ok())
        != Some("runtime")
    {
        return failure("runtime_native_installation_required");
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
                execute(&owner, input, &credential, &request).await
            });
            let _ = send.send(result);
        });
    }
    tokio::select! {result=receive=>match result{Ok(Ok(payload))=>crate::mcp_output::typed(payload),Ok(Err(error))=>operation_failure(error),Err(_)=>failure("native_job_failed")},()=context.ct.cancelled()=>failure("request_cancelled_or_expired"),()=state.stop.cancelled()=>failure("stopped"),()=tokio::time::sleep_until(deadline.into())=>failure("request_cancelled_or_expired")}
}
