//! Runtime history ingress. Browser/Native USER callers use their separate real admission.
use super::*;
use crate::knowledge_live::{Access, LiveError, LiveSessionService};

pub(super) const TOOL: &str = "graphit_knowledge_live_history";
use crate::knowledge_live::HistoryOperation;
pub(super) fn definition() -> Tool {
    Tool::new(TOOL, "Read or control Live history owned by the authenticated installation. Execution is independently owned; disconnecting a reader does not cancel a run.",
        json!({"type":"object","properties":{"operation":{"enum":["list","get","replay","cancel","close","delete"]},"id":{"type":"string"},"cursor":{"type":["object","null"],"properties":{"generation":{"type":"string"},"sequence":{"type":"string","pattern":"^(0|[1-9][0-9]*)$"}},"required":["generation","sequence"],"additionalProperties":false}},"required":["operation"],"additionalProperties":false}).as_object().expect("object schema").clone())
}
use crate::knowledge_live::dispatch;
pub(super) fn error(error: &LiveError) -> CallToolResult {
    let code = match error.primary_cause() {
        LiveError::Busy => "busy",
        LiveError::Conflict => "context_changed",
        LiveError::Closed => "closed",
        LiveError::NotReady => "not_ready",
        _ => match error.kind() {
            io::ErrorKind::Interrupted => "cancelled",
            io::ErrorKind::TimedOut => "deadline",
            io::ErrorKind::PermissionDenied => "permission_denied",
            io::ErrorKind::NotFound => "not_found",
            io::ErrorKind::AlreadyExists => "conflict",
            io::ErrorKind::InvalidData => "corrupt",
            io::ErrorKind::InvalidInput => "invalid_context",
            io::ErrorKind::FileTooLarge | io::ErrorKind::OutOfMemory => "limit",
            io::ErrorKind::WouldBlock => "busy",
            _ => "storage",
        },
    };
    crate::mcp_output::typed_error(
        json!({"error":{"code":code,"journal_effect":format!("{:?}",error.effect()),"diagnostic":error.diagnostic()}}),
    )
}
pub(super) async fn call(
    server: &SourceServer,
    live: Arc<LiveSessionService>,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let input = match serde_json::from_value::<HistoryOperation>(Value::Object(
        call.arguments.unwrap_or_default(),
    )) {
        Ok(input) => input,
        Err(_) => return error(&LiveError::Io(io::ErrorKind::InvalidInput.into())),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return error(&LiveError::Io(io::ErrorKind::PermissionDenied.into()));
    };
    if parts
        .headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
        != Some("runtime")
    {
        return error(&LiveError::Io(io::ErrorKind::PermissionDenied.into()));
    }
    let Some(raw) = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
    else {
        return error(&LiveError::Io(io::ErrorKind::PermissionDenied.into()));
    };
    let credential = Zeroizing::new(raw.to_owned());
    let state = Arc::clone(&server.0);
    let Some(deadline) = Instant::now().checked_add(state.policy.timeout) else {
        return error(&LiveError::Io(io::ErrorKind::InvalidInput.into()));
    };
    let cancel = Cancellation::default();
    let _waiter = CancelOnDrop(cancel.clone());
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = state.jobs.lock() => jobs,
            () = context.ct.cancelled() => return error(&LiveError::Io(io::ErrorKind::Interrupted.into())),
            () = state.stop.cancelled() => return error(&LiveError::Io(io::ErrorKind::Interrupted.into())),
            () = tokio::time::sleep_until(deadline.into()) => return error(&LiveError::Io(io::ErrorKind::TimedOut.into())),
        };
        jobs.reap();
        let owner = Arc::clone(&state);
        jobs.tasks.spawn_blocking(move || {
            let result = (|| {
                let original = RequestContext::new(owner.policy.content, &cancel, deadline)
                    .map_err(io::Error::other)?;
                let access = Access::service(&owner.auth, &credential, &original)?;
                dispatch(&live, &access, input)
            })();
            let _ = send.send(result);
        });
    }
    tokio::select! {
        result = receive => match result {
            Ok(Ok(value)) => crate::mcp_output::typed(value.value),
            Ok(Err(failure)) => error(&failure),
            Err(_) => error(&LiveError::Io(io::ErrorKind::BrokenPipe.into())),
        },
        () = context.ct.cancelled() => error(&LiveError::Io(io::ErrorKind::Interrupted.into())),
        () = state.stop.cancelled() => error(&LiveError::Io(io::ErrorKind::Interrupted.into())),
        () = tokio::time::sleep_until(deadline.into()) => error(&LiveError::Io(io::ErrorKind::TimedOut.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::native_document_file::NativeDocumentJournalEffect;

    #[test]
    fn runtime_wrapped_errors_keep_primary_code_effect_and_safe_persistence_diagnostic() {
        for (primary, expected) in [
            (LiveError::Conflict, "context_changed"),
            (LiveError::NotReady, "not_ready"),
            (LiveError::Busy, "busy"),
            (LiveError::Closed, "closed"),
        ] {
            let mut history = LiveError::after_effect(
                io::Error::new(primary.kind(), primary),
                NativeDocumentJournalEffect::Applied,
            );
            if let LiveError::History(failure) = &mut history {
                failure.secondary = Some(io::ErrorKind::PermissionDenied);
            }
            let wrapped = LiveError::Preparation {
                cause: Box::new(history),
                persistence: Some(Box::new(LiveError::after_effect(
                    io::Error::new(io::ErrorKind::Interrupted, "private-native-path-and-token"),
                    NativeDocumentJournalEffect::Unknown,
                ))),
                effect: NativeDocumentJournalEffect::Unknown,
            };
            let response = serde_json::to_value(error(&wrapped)).unwrap();
            assert_eq!(response["isError"], true);
            let envelope = &response["structuredContent"]["error"];
            assert_eq!(envelope["code"], expected);
            assert_eq!(envelope["journal_effect"], "Unknown");
            assert_eq!(envelope["diagnostic"]["secondary_kind"], "PermissionDenied");
            assert_eq!(envelope["diagnostic"]["persistence"]["kind"], "Interrupted");
            assert_eq!(
                envelope["diagnostic"]["persistence"]["journal_effect"],
                "Unknown"
            );
            assert!(
                !response
                    .to_string()
                    .contains("private-native-path-and-token")
            );
        }
    }
}
