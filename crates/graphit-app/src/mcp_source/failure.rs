//! Private causes remain owned; MCP receives only finite SDK categories.
use rmcp::{
    ServiceError, service::ClientInitializeError,
    transport::streamable_http_client::StreamableHttpError,
};
use std::{error::Error, io};

pub(super) struct ForwardFailure {
    stage: &'static str,
    cause: io::Error,
    cleanup: Option<io::Error>,
}

impl ForwardFailure {
    pub(super) fn new(stage: &'static str, cause: io::Error) -> Self {
        Self {
            stage,
            cause,
            cleanup: None,
        }
    }

    pub(super) fn finish<T>(
        call: io::Result<T>,
        cleanup: io::Result<()>,
        valid: io::Result<()>,
    ) -> Result<T, Self> {
        let cleanup = cleanup.err();
        if let Err(cause) = valid {
            return Err(Self {
                stage: "original",
                cause,
                cleanup,
            });
        }
        match (call, cleanup) {
            (Err(cause), cleanup) => Err(Self {
                stage: "call",
                cause,
                cleanup,
            }),
            (Ok(_), Some(cause)) => Err(Self::new("cleanup", cause)),
            (Ok(value), None) => Ok(value),
        }
    }

    /// Only a tool-call RPC rejection under a valid Original crosses as an RPC error.
    /// Remote messages/data are private; retain the code and our finite diagnostics.
    pub(super) fn protocol_error(&self) -> Option<rmcp::ErrorData> {
        if self.stage != "call" {
            return None;
        }
        let ServiceError::McpError(error) = self.cause.get_ref()?.downcast_ref::<ServiceError>()?
        else {
            return None;
        };
        Some(rmcp::ErrorData::new(error.code, self.message(), None))
    }

    pub(super) fn terminal(self, kind: io::ErrorKind) -> Self {
        Self {
            stage: "forward",
            cause: kind.into(),
            cleanup: self.cleanup,
        }
    }

    pub(super) fn message(&self) -> String {
        let category = category(&self.cause);
        match &self.cleanup {
            Some(cleanup) => format!(
                "Source forwarding failed (stage={}, category={category}, cleanup={})",
                self.stage,
                category_of_io(cleanup)
            ),
            None => format!(
                "Source forwarding failed (stage={}, category={category})",
                self.stage
            ),
        }
    }
}

fn category_of_io(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::Interrupted => "cancelled",
        io::ErrorKind::TimedOut => "deadline",
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::InvalidData => "invalid_data",
        io::ErrorKind::InvalidInput => "invalid_input",
        io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::UnexpectedEof
        | io::ErrorKind::NotConnected => "connection",
        _ => error.get_ref().map_or("io", |source| category(source)),
    }
}

fn category(error: &(dyn Error + 'static)) -> &'static str {
    if let Some(error) = error.downcast_ref::<io::Error>() {
        return category_of_io(error);
    }
    if let Some(error) = error.downcast_ref::<ServiceError>() {
        return match error {
            ServiceError::TransportSend(error) => category(error.error.as_ref()),
            ServiceError::TransportClosed => "transport_closed",
            ServiceError::Timeout { .. } => "sdk_timeout",
            ServiceError::Cancelled { .. } => "sdk_cancelled",
            ServiceError::McpError(_) => "rpc_error",
            ServiceError::UnexpectedResponse => "unexpected_response",
            _ => "sdk_service",
        };
    }
    if let Some(error) = error.downcast_ref::<ClientInitializeError>() {
        return match error {
            ClientInitializeError::TransportError { error, .. } => category(error.error.as_ref()),
            ClientInitializeError::ConnectionClosed(_) => "transport_closed",
            ClientInitializeError::Cancelled => "sdk_cancelled",
            ClientInitializeError::JsonRpcError(_) => "rpc_error",
            ClientInitializeError::NoCompatibleProtocolVersion { .. } => "protocol_version",
            _ => "sdk_initialize",
        };
    }
    if let Some(error) = error.downcast_ref::<StreamableHttpError<reqwest::Error>>() {
        return match error {
            StreamableHttpError::Io(error) => category_of_io(error),
            StreamableHttpError::Client(error) => category(error),
            StreamableHttpError::SessionExpired => "session_expired",
            StreamableHttpError::SessionRecoveryTimeout => "session_recovery_timeout",
            StreamableHttpError::ControlRequestTimeout => "control_timeout",
            StreamableHttpError::AuthRequired(_) => "authentication_required",
            StreamableHttpError::InsufficientScope(_) => "insufficient_scope",
            StreamableHttpError::UnexpectedEndOfStream => "stream_ended",
            StreamableHttpError::UnexpectedServerResponse(_) => "invalid_response",
            StreamableHttpError::UnexpectedContentType(_) => "content_type",
            StreamableHttpError::Deserialize(_) => "response_decode",
            StreamableHttpError::TransportChannelClosed => "transport_closed",
            _ => "sdk_http",
        };
    }
    if let Some(error) = error.downcast_ref::<reqwest::Error>() {
        return match error.status().map(|status| status.as_u16()) {
            Some(401) => "http_unauthorized",
            Some(403) => "http_forbidden",
            Some(404) => "http_not_found",
            Some(_) => "http_status",
            None if error.is_timeout() => "http_timeout",
            None if error.is_connect() => "http_connect",
            None if error.is_body() => "http_body",
            None if error.is_decode() => "http_decode",
            None => "http_request",
        };
    }
    if let Some(error) = error.downcast_ref::<tokio::task::JoinError>() {
        return if error.is_panic() {
            "worker_panicked"
        } else {
            "worker_cancelled"
        };
    }
    error.source().map_or("other", category)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::transport::DynamicTransportError;
    use std::any::TypeId;

    #[test]
    fn sdk_recovery_error_and_cleanup_failure_are_both_retained() {
        let native = ServiceError::TransportSend(DynamicTransportError::from_parts(
            "private-transport-canary",
            TypeId::of::<()>(),
            Box::new(StreamableHttpError::<reqwest::Error>::SessionRecoveryTimeout),
        ));
        let failure = ForwardFailure::finish::<()>(
            Err(io::Error::other(native)),
            Err(io::Error::other("private-cleanup-canary")),
            Ok(()),
        )
        .err()
        .unwrap();
        assert!(failure.cause.get_ref().unwrap().is::<ServiceError>());
        assert!(failure.cleanup.is_some());
        assert_eq!(
            failure.message(),
            "Source forwarding failed (stage=call, category=session_recovery_timeout, cleanup=other)"
        );
    }

    #[test]
    fn diagnostics_never_format_private_sdk_response_or_rpc_data() {
        for source in [
            io::Error::other(
                StreamableHttpError::<reqwest::Error>::UnexpectedServerResponse(
                    "https://private.invalid/?bearer=private-response-canary".into(),
                ),
            ),
            io::Error::other(ServiceError::McpError(rmcp::ErrorData::internal_error(
                "private-rpc-canary",
                Some(serde_json::json!({"secret":"private-data-canary"})),
            ))),
        ] {
            let failure = ForwardFailure::new("call", source);
            let message = failure.message();
            assert!(!message.contains("private"));
            assert!(!message.contains("https"));
            assert!(
                message.contains("category=invalid_response")
                    || message.contains("category=rpc_error")
            );
        }
    }

    #[test]
    fn invalid_original_prevents_success_and_cleanup_failure_remains_distinct() {
        let original =
            ForwardFailure::finish(Ok("unusable"), Ok(()), Err(io::ErrorKind::TimedOut.into()))
                .err()
                .unwrap();
        assert_eq!(
            original.message(),
            "Source forwarding failed (stage=original, category=deadline)"
        );
        let cleanup = ForwardFailure::finish(
            Ok("unusable"),
            Err(io::ErrorKind::BrokenPipe.into()),
            Ok(()),
        )
        .err()
        .unwrap();
        assert_eq!(
            cleanup.message(),
            "Source forwarding failed (stage=cleanup, category=connection)"
        );
    }
    #[test]
    fn call_rpc_code_is_preserved_without_forwarding_remote_message_or_data() {
        let failure = ForwardFailure::finish::<()>(
            Err(io::Error::other(ServiceError::McpError(
                rmcp::ErrorData::invalid_params(
                    "private-rpc-canary",
                    Some(serde_json::json!({"secret":"private-data-canary"})),
                ),
            ))),
            Err(io::Error::other("private-cleanup-canary")),
            Ok(()),
        )
        .err()
        .unwrap();
        let error = failure.protocol_error().unwrap();
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(error.data.is_none());
        assert_eq!(
            error.message,
            "Source forwarding failed (stage=call, category=rpc_error, cleanup=other)"
        );
        assert!(!serde_json::to_string(&error).unwrap().contains("private"));
    }

    #[test]
    fn original_and_non_call_failures_do_not_become_remote_rpc_errors() {
        for valid in [io::ErrorKind::Interrupted, io::ErrorKind::TimedOut] {
            let failure = ForwardFailure::finish::<()>(
                Err(io::Error::other(ServiceError::McpError(
                    rmcp::ErrorData::invalid_params("private", None),
                ))),
                Ok(()),
                Err(valid.into()),
            )
            .err()
            .unwrap();
            assert!(failure.protocol_error().is_none());
            assert_eq!(failure.stage, "original");
        }
        let failure = ForwardFailure::new(
            "connect",
            io::Error::other(ServiceError::McpError(rmcp::ErrorData::invalid_params(
                "private", None,
            ))),
        );
        assert!(failure.protocol_error().is_none());
        let result = rmcp::model::CallToolResult::structured_error(
            serde_json::json!({"error":{"code":"invalid_arguments"}}),
        );
        let forwarded = ForwardFailure::finish(Ok(result.clone()), Ok(()), Ok(()))
            .unwrap_or_else(|_| panic!("domain result must remain successful RPC"));
        assert_eq!(forwarded, result);
        assert_eq!(forwarded.is_error, Some(true));
    }
    #[test]
    fn late_terminal_signal_prevents_rpc_delivery_and_retains_cleanup_diagnostics() {
        for (kind, category) in [
            (io::ErrorKind::Interrupted, "cancelled"),
            (io::ErrorKind::TimedOut, "deadline"),
        ] {
            let remote = rmcp::ErrorData::invalid_params(
                "private-rpc-canary",
                Some(serde_json::json!({"secret":"private-data-canary"})),
            );
            let failure = ForwardFailure::finish::<()>(
                Err(io::Error::other(ServiceError::McpError(remote))),
                Err(io::ErrorKind::BrokenPipe.into()),
                Ok(()),
            )
            .err()
            .unwrap();
            assert!(failure.protocol_error().is_some());
            let terminal = failure.terminal(kind);
            assert!(terminal.protocol_error().is_none());
            assert!(terminal.cleanup.is_some());
            assert_eq!(
                terminal.message(),
                format!(
                    "Source forwarding failed (stage=forward, category={category}, cleanup=connection)"
                )
            );
            assert!(!terminal.message().contains("private"));
        }
    }
}
