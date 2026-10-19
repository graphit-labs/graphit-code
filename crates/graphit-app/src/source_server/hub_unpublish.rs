//! Remote catalog retraction through the Native management ingress.
use super::*;
use crate::hub_unpublish::{self as kernel, Arguments};
use graphit_auth::ServiceMode;

pub(super) const TOOL: &str = "graphit_hub_unpublish";

pub(super) fn definition() -> Tool {
    Tool::new(TOOL,
        "Retract an explicit remote Broker artifact version or all observed catalog versions. Payloads, source and local installations are preserved; no local PROJECT selection is required.",
        json!({"type":"object","properties":{
            "provider":{"type":"string"},"producer":{"type":"string"},
            "artifact_type":{"type":"string","enum":["agent","ast","command","knowledge","language","mcp","power","rule","skill","workflow"]},
            "id":{"type":"string"},"target":{"oneOf":[
                {"type":"object","properties":{"kind":{"const":"version"},"version":{"type":"string"},"expected_sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"}},"required":["kind","version"],"additionalProperties":false},
                {"type":"object","properties":{"kind":{"const":"artifact"}},"required":["kind"],"additionalProperties":false}
            ]}
        },"required":["provider","producer","artifact_type","id","target"],"additionalProperties":false})
        .as_object().expect("static schema").clone())
}

fn failure(code: &str, limit: usize) -> CallToolResult {
    delivery(
        crate::mcp_output::typed_error(
            json!({"status":"error","code":code,"retraction":{"catalog":"not_attempted"}}),
        ),
        None,
        limit,
    )
}
fn denied(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error)
}
fn operation_failure(error: io::Error) -> CallToolResult {
    let cause = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<kernel::Failure>())
        .map_or(&error, |effect| &effect.cause);
    let kind = match cause
        .get_ref()
        .and_then(|error| error.downcast_ref::<object_store::Error>())
    {
        Some(object_store::Error::PermissionDenied { .. }) => io::ErrorKind::PermissionDenied,
        Some(object_store::Error::NotSupported { .. }) => io::ErrorKind::Unsupported,
        Some(object_store::Error::NotFound { .. }) => io::ErrorKind::NotFound,
        _ => cause.kind(),
    };
    let code = match (crate::native_publication::cause_code(cause), kind) {
        ("conditional_conflict", _) | (_, io::ErrorKind::AlreadyExists) => "hub_unpublish_conflict",
        (_, io::ErrorKind::PermissionDenied) => "authentication_or_permission_denied",
        (_, io::ErrorKind::Interrupted) => "request_cancelled",
        (_, io::ErrorKind::TimedOut) => "request_deadline_expired",
        (_, io::ErrorKind::InvalidInput) => "invalid_arguments",
        (_, io::ErrorKind::InvalidData) => "invalid_remote_catalog",
        (_, io::ErrorKind::NotFound) => "remote_catalog_not_found",
        (_, io::ErrorKind::FileTooLarge) => "hub_unpublish_too_large",
        (_, io::ErrorKind::NotConnected) => "hub_unpublish_unavailable",
        (_, io::ErrorKind::Unsupported) => "hub_unpublish_unsupported",
        _ => "hub_unpublish_failed",
    };
    let mut body = super::web_hub_unpublish::failure_body(&error);
    body["code"] = json!(code);
    crate::mcp_output::typed_error(body)
}

async fn execute(
    state: &State,
    input: Arguments,
    credential: &str,
    local: bool,
    original: &RequestContext<'_>,
) -> io::Result<Value> {
    let policy = &state.policy.native_install.hub;
    input.validate(policy)?;
    let issuer = state.auth.issuer();
    let caller = if local {
        issuer.authenticate_local(credential, original)
    } else {
        issuer.authenticate_runtime(credential, original)
    }
    .map_err(denied)?;
    let mut selection = issuer
        .capture_hub_write_service(&caller, &input.provider)
        .map_err(denied)?;
    if selection.mode().map_err(denied)? != ServiceMode::BrokerNative {
        return Err(io::ErrorKind::Unsupported.into());
    }
    let verifier = state
        .native_verifiers
        .as_ref()
        .ok_or(io::ErrorKind::NotConnected)?
        .get(&mut selection)
        .map_err(io::Error::other)?;
    let lease = selection
        .native_credential(&verifier)
        .await
        .map_err(denied)?;
    let before = || -> io::Result<()> {
        original.check()?;
        if state.stop.is_cancelled() {
            return Err(io::ErrorKind::NotConnected.into());
        }
        lease.check().map_err(denied)
    };
    before()?;
    let storage = crate::native_storage_credentials::broker_credentials(
        &lease,
        input.producer,
        &state.native_http,
        state.native_allow_loopback_http,
    )
    .await
    .map_err(io::Error::from)?;
    let current = || -> io::Result<()> {
        before()?;
        storage.check().map_err(io::Error::from)
    };
    current()?;
    let receipt = kernel::execute(
        kernel::Input {
            arguments: &input,
            storage: &storage,
            client: &state.native_http,
            policy,
        },
        original,
        &current,
    )
    .await?;
    if let Err(cause) = current() {
        return Err(io::Error::new(
            cause.kind(),
            kernel::Failure {
                receipt,
                stage: "delivery",
                cause,
            },
        ));
    }
    Ok(json!({"status":"unpublished","retraction":receipt}))
}

fn completed(result: Result<io::Result<Value>, oneshot::error::RecvError>) -> CallToolResult {
    match result {
        Ok(Ok(body)) => crate::mcp_output::typed(body),
        Ok(Err(error)) => operation_failure(error),
        Err(_) => crate::mcp_output::typed_error(
            json!({"status":"error","code":"hub_unpublish_job_failed",
            "retraction":{"catalog":"unknown"},"observation_failed":true,"inspection_required":true}),
        ),
    }
}
fn delivery(mut result: CallToolResult, terminal: Option<&str>, limit: usize) -> CallToolResult {
    if let Some(code) = terminal {
        result = crate::mcp_output::typed_error(json!({"status":"error","code":code,
            "outcome":result.structured_content}));
    }
    let Some(body) = result.structured_content.as_ref() else {
        return result;
    };
    if serde_json::to_vec(body).is_ok_and(|bytes| bytes.len() <= limit) {
        return result;
    }
    let outcome = body.get("outcome").unwrap_or(body);
    let mut effects = serde_json::Map::new();
    for field in ["retraction", "stage", "cause", "observation_failed"] {
        if let Some(value) = outcome.get(field) {
            effects.insert(field.into(), value.clone());
        }
    }
    let summary = json!({"status":"error","code":"hub_unpublish_too_large",
        "inspection_required":true,"selection":"request","effects":effects});
    let minimal = json!({"status":"error","code":"hub_unpublish_too_large",
        "inspection_required":true,"selection":"request"});
    for candidate in [summary, minimal] {
        if serde_json::to_vec(&candidate).is_ok_and(|bytes| bytes.len() <= limit) {
            return crate::mcp_output::typed_error(candidate);
        }
    }
    // A response budget that cannot hold the inspection envelope still reports a
    // protocol error. No receipt is truncated and no effect is claimed absent.
    CallToolResult::error(Vec::new())
}

pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let limit = server.0.policy.content.max_output_bytes;
    if call.name != TOOL {
        return failure("invalid_arguments", limit);
    }
    let body = Value::Object(call.arguments.unwrap_or_default());
    if serde_json::to_vec(&body).map_or(true, |v| v.len() > server.0.policy.content.max_input_bytes)
    {
        return failure("invalid_arguments", limit);
    }
    let input: Arguments = match serde_json::from_value(body) {
        Ok(input) => input,
        Err(_) => return failure("invalid_arguments", limit),
    };
    if input.validate(&server.0.policy.native_install.hub).is_err() {
        return failure("invalid_arguments", limit);
    }
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
        .and_then(|v| v.to_str().ok())
    {
        Some("runtime") => false,
        Some("local") => true,
        _ => return failure("credential_origin_required", limit),
    };
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return failure("credential_required", limit);
    };
    let credential = Zeroizing::new(raw.to_owned());
    let cancel = Cancellation::default();
    let _guard = CancelOnDrop(cancel.clone());
    let original_cancel = cancel.clone();
    let state = Arc::clone(&server.0);
    let deadline = Instant::now() + state.policy.timeout;
    let handle = Handle::current();
    let (send, mut receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! { jobs=state.jobs.lock()=>jobs,
        ()=context.ct.cancelled()=>return failure("request_cancelled", limit),
        ()=state.stop.cancelled()=>return failure("stopped", limit),
        ()=tokio::time::sleep_until(deadline.into())=>return failure("request_deadline_expired", limit) };
        jobs.reap();
        if state.stop.is_cancelled() {
            return failure("stopped", limit);
        }
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = handle.block_on(async {
                let original =
                    RequestContext::new(owner.policy.content, &original_cancel, deadline)
                        .map_err(io::Error::other)?;
                execute(&owner, input, &credential, local, &original).await
            });
            let _ = send.send(result);
        });
    }
    let (result, terminal) = tokio::select! {
        result=&mut receive => (result, None),
        ()=context.ct.cancelled()=>{ cancel.cancel(); (receive.await, Some("request_cancelled")) },
        ()=state.stop.cancelled()=>{ cancel.cancel(); (receive.await, Some("stopped")) },
        ()=tokio::time::sleep_until(deadline.into())=>{ cancel.cancel(); (receive.await, Some("request_deadline_expired")) },
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
    delivery(
        completed(result),
        terminal,
        state.policy.content.max_output_bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_delivery_preserves_known_effect_after_cancel() {
        let result = crate::mcp_output::typed(json!({"status":"unpublished",
            "retraction":{"catalog":"removed","targeted_versions":["release/1"]}}));
        let result = delivery(result, Some("request_cancelled"), 8192);
        assert_eq!(result.is_error, Some(true));
        let body = result.structured_content.unwrap();
        assert_eq!(body["code"], "request_cancelled");
        assert_eq!(body["outcome"]["status"], "unpublished");
        assert_eq!(body["outcome"]["retraction"]["catalog"], "removed");
    }
    #[test]
    fn native_delivery_retains_typed_uncertainty_and_never_exposes_cause_text() {
        let result = operation_failure(io::Error::new(
            io::ErrorKind::TimedOut,
            kernel::Failure {
                receipt: kernel::Receipt {
                    producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                    artifact_type: graphit_hub::ArtifactType::Skill,
                    id: "team/tool".into(),
                    targeted_versions: vec!["release/1".into()],
                    desired_latest: None,
                    catalog: "uncertain",
                },
                stage: "catalog_delete",
                cause: io::Error::new(io::ErrorKind::TimedOut, "private transport diagnostic"),
            },
        ));
        assert_eq!(result.is_error, Some(true));
        let body = result.structured_content.unwrap();
        assert_eq!(body["code"], "request_deadline_expired");
        assert_eq!(body["retraction"]["catalog"], "uncertain");
        assert!(!body.to_string().contains("private transport diagnostic"));
        let oversized = delivery(crate::mcp_output::typed(body), None, 128);
        assert_eq!(oversized.is_error, Some(true));
        let body = oversized.structured_content.unwrap();
        assert_eq!(body["inspection_required"], true);
        assert_eq!(body["selection"], "request");
        assert!(serde_json::to_vec(&body).unwrap().len() <= 128);
    }

    #[test]
    fn native_delivery_keeps_valid_tiny_response_budgets() {
        for maximum in [1, 64, 128] {
            let result = delivery(
                crate::mcp_output::typed(json!({"status":"unpublished",
                "retraction":{"catalog":"removed","id":"long/".repeat(128),
                    "targeted_versions":["release/one","release/two"]}})),
                None,
                maximum,
            );
            assert_eq!(result.is_error, Some(true));
            if let Some(body) = result.structured_content {
                assert!(serde_json::to_vec(&body).unwrap().len() <= maximum);
                assert_eq!(body["inspection_required"], true);
                assert!(body.get("retraction").is_none());
            } else {
                assert!(result.content.is_empty());
            }
            let early = failure("invalid_arguments", maximum);
            assert_eq!(early.is_error, Some(true));
            if let Some(body) = early.structured_content {
                assert!(serde_json::to_vec(&body).unwrap().len() <= maximum);
            } else {
                assert!(early.content.is_empty());
            }
        }
    }
    #[test]
    fn native_storage_errors_keep_finite_codes_with_and_without_effects() {
        for (kind, expected) in [
            (0, "authentication_or_permission_denied"),
            (1, "hub_unpublish_unsupported"),
            (2, "remote_catalog_not_found"),
        ] {
            for wrapped in [false, true] {
                let source = Box::new(io::Error::other("private signed URL or transport detail"));
                let storage = match kind {
                    0 => object_store::Error::PermissionDenied {
                        path: "private/key".into(),
                        source,
                    },
                    1 => object_store::Error::NotSupported { source },
                    _ => object_store::Error::NotFound {
                        path: "private/key".into(),
                        source,
                    },
                };
                let error = io::Error::other(storage);
                let error = if wrapped {
                    io::Error::other(kernel::Failure {
                        receipt: kernel::Receipt {
                            producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                            artifact_type: graphit_hub::ArtifactType::Skill,
                            id: "team/tool".into(),
                            targeted_versions: vec!["release/1".into()],
                            desired_latest: None,
                            catalog: "rejected",
                        },
                        stage: "catalog_update",
                        cause: error,
                    })
                } else {
                    error
                };
                let result = operation_failure(error);
                assert_eq!(result.is_error, Some(true));
                let body = result.structured_content.unwrap();
                assert_eq!(body["code"], expected);
                assert_eq!(
                    body["retraction"]["catalog"],
                    if wrapped { "rejected" } else { "not_attempted" }
                );
                assert!(!body.to_string().contains("private"));
            }
        }
    }
}
