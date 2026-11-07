//! Authenticated Root MCP presentation of the pure Knowledge identity producer.
use super::*;
use graphit_auth::{Obligation, Permission};
use graphit_core::module_settings::Module;

pub(super) fn name(brand: &graphit_core::brand::Brand) -> String {
    brand
        .mcp_tool_name(&["knowledge", "id"])
        .expect("closed Knowledge identity tool name")
}

pub(super) fn definition(brand: &graphit_core::brand::Brand) -> Tool {
    Tool::new(
        name(brand),
        crate::knowledge_id::DESCRIPTION,
        crate::knowledge_id::schema()
            .as_object()
            .expect("identity input schema")
            .clone(),
    )
    .with_annotations(rmcp::model::ToolAnnotations::new().read_only(true))
}

fn failed(code: &str) -> CallToolResult {
    crate::mcp_output::typed_error(json!({"code":code}))
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failed("incoming_credentials_required");
    };
    let local = match parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|value| value.to_str().ok())
    {
        Some("local") => true,
        Some("runtime") => false,
        _ => return failed("unsupported_credential_origin"),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
    else {
        return failed("incoming_credentials_required");
    };
    let credential = Zeroizing::new(raw.to_owned());
    let input = call.arguments.map(Value::Object);
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = context.ct.cancelled() => return failed("request_cancelled_or_expired"),
            () = state.stop.cancelled() => return failed("service_stopped"),
            () = tokio::time::sleep_until(deadline.into()) => return failed("request_cancelled_or_expired"),
        };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failed("service_stopped");
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = (|| {
                let original =
                    RequestContext::new(owner.policy.content, &original_cancel, deadline)
                        .map_err(|_| json!({"code":"request_cancelled_or_expired"}))?;
                let caller = if local {
                    owner
                        .auth
                        .issuer()
                        .authenticate_local(&credential, &original)
                } else {
                    owner
                        .auth
                        .issuer()
                        .authenticate_runtime(&credential, &original)
                }
                .map_err(|_| json!({"code":"authentication_or_permission_denied"}))?;
                if caller
                    .authorize(Permission::Read(Module::Knowledge))
                    .map_err(|_| json!({"code":"authentication_or_permission_denied"}))?
                    != Obligation::Unconditional
                {
                    return Err(json!({"code":"authentication_or_permission_denied"}));
                }
                let value = crate::knowledge_id::calculate(input, owner.policy.content)?;
                original
                    .snapshot()
                    .check()
                    .map_err(|_| json!({"code":"request_cancelled_or_expired"}))?;
                if caller
                    .authorize(Permission::Read(Module::Knowledge))
                    .map_err(|_| json!({"code":"authentication_or_permission_denied"}))?
                    != Obligation::Unconditional
                {
                    return Err(json!({"code":"authentication_or_permission_denied"}));
                }
                Ok(value)
            })();
            let _ = send.send(result);
        });
    }
    tokio::select! {
        result = receive => match result {
            Ok(_) if context.ct.is_cancelled() || cancel.is_cancelled() => failed("request_cancelled_or_expired"),
            Ok(_) if state.stop.is_cancelled() => failed("service_stopped"),
            Ok(_) if Instant::now() >= deadline => failed("request_cancelled_or_expired"),
            Ok(Ok(value)) => crate::mcp_output::typed(value),
            Ok(Err(cause)) => crate::mcp_output::typed_error(cause),
            Err(_) => failed("identity_calculation_failed"),
        },
        () = context.ct.cancelled() => failed("request_cancelled_or_expired"),
        () = state.stop.cancelled() => failed("service_stopped"),
        () = tokio::time::sleep_until(deadline.into()) => failed("request_cancelled_or_expired"),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
