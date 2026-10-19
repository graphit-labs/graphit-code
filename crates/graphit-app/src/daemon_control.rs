//! Runtime-authenticated administration on the daemon's existing listener.
use axum::{
    Router,
    extract::{DefaultBodyLimit, State as AxumState},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use graphit_auth::{Obligation, Permission};
use graphit_core::{
    file_lock::Cancellation, native_document_file::NativeDocumentFileLimits,
    project_paths::InstallationPaths,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use serde::{Deserialize, Serialize};
use std::{
    io,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub const ROUTE: &str = "/admin/daemon";
#[derive(Clone, Copy)]
pub(crate) struct StopAcknowledgement {
    pub(crate) deadline: Instant,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Status,
    Stop,
    OpenUi,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Running,
    Stopping,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub pid: u32,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_url: Option<String>,
}

impl Reply {
    pub(crate) fn status(stop: &CancellationToken) -> Self {
        Self {
            pid: std::process::id(),
            state: if stop.is_cancelled() {
                State::Stopping
            } else {
                State::Running
            },
            ui_url: None,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Command {
    operation: Operation,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_pid: Option<std::num::NonZeroU32>,
}

struct Shared {
    auth: Arc<DaemonAuth>,
    content: ContentLimits,
    timeout: Duration,
    stop: CancellationToken,
    ui_url: Option<String>,
}

pub(crate) fn router(
    auth: Arc<DaemonAuth>,
    content: ContentLimits,
    timeout: Duration,
    stop: CancellationToken,
    ui_url: Option<String>,
) -> Router {
    Router::new()
        .route(ROUTE, post(command))
        .layer(DefaultBodyLimit::max(content.max_input_bytes))
        .with_state(Arc::new(Shared {
            auth,
            content,
            timeout,
            stop,
            ui_url,
        }))
}

async fn command(
    AxumState(server): AxumState<Arc<Shared>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // Browser and project authorization never confer daemon administration.
    if headers.contains_key(header::ORIGIN)
        || headers.get_all("x-graphit-auth-origin").iter().count() != 1
        || headers
            .get("x-graphit-auth-origin")
            .and_then(|v| v.to_str().ok())
            != Some("runtime")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if headers.get_all(header::AUTHORIZATION).iter().count() != 1 {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(raw) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|raw| {
            !raw.is_empty() && !raw.bytes().any(|b| b.is_ascii_whitespace() || b == b',')
        })
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let credential = Zeroizing::new(raw.to_owned());
    let input = match serde_json::from_slice::<Command>(&body) {
        Ok(input) => input,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let cancellation = Cancellation::default();
    let Some(deadline) = Instant::now().checked_add(server.timeout) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let original = match RequestContext::new(server.content, &cancellation, deadline) {
        Ok(original) => original,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let caller = match server
        .auth
        .issuer()
        .authenticate_runtime(&credential, &original)
    {
        Ok(caller) => caller,
        Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
    };
    if !matches!(
        caller.authorize(Permission::Configure),
        Ok(Obligation::Unconditional)
    ) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if original.check().is_err() {
        return StatusCode::REQUEST_TIMEOUT.into_response();
    }
    // A target constrains an already authorized operation; it confers no authority.
    if input
        .expected_pid
        .is_some_and(|pid| pid.get() != std::process::id())
    {
        return (
            StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"error":"daemon_process_changed"})),
        )
            .into_response();
    }
    if input.operation == Operation::OpenUi && server.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let ui_url = if input.operation == Operation::OpenUi {
        match &server.ui_url {
            Some(url) => Some(url.clone()),
            None => {
                return (
                    StatusCode::CONFLICT,
                    axum::Json(serde_json::json!({"error":"browser_not_configured"})),
                )
                    .into_response();
            }
        }
    } else {
        None
    };
    let mut reply = Reply::status(&server.stop);
    if input.operation == Operation::Stop {
        reply.state = State::Stopping;
    }
    reply.ui_url = ui_url;
    match serde_json::to_vec(&reply) {
        Ok(body) if body.len() <= server.content.max_output_bytes => {
            if original.check().is_err()
                || (input.operation == Operation::OpenUi && server.stop.is_cancelled())
            {
                return StatusCode::REQUEST_TIMEOUT.into_response();
            }
            let mut response = ([(header::CONTENT_TYPE, "application/json")], body).into_response();
            if input.operation == Operation::Stop {
                response
                    .extensions_mut()
                    .insert(StopAcknowledgement { deadline });
                server.stop.cancel();
            }
            response
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// Authenticate the exact discovered loopback listener with the live native key.
/// The caller's deadline and cancellation cover credential reads and all HTTP I/O.
/// The shared native client must disable proxies and redirects so credentials stay
/// on that listener.
pub async fn request(
    installation: &InstallationPaths,
    address: SocketAddr,
    client: &reqwest::Client,
    limits: NativeDocumentFileLimits,
    original: &RequestContext<'_>,
    operation: Operation,
    expected_pid: Option<std::num::NonZeroU32>,
) -> io::Result<Reply> {
    original.check()?;
    let deadline = tokio::time::Instant::from_std(original.snapshot().required_deadline()?);
    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "daemon listener must be loopback",
        ));
    }
    let headers = crate::ast_source::runtime_headers(installation, limits, original).await?;
    let work = async {
        let mut response = client
            .post(format!("http://{address}{ROUTE}"))
            .headers(headers.into_iter().collect())
            .json(&Command {
                operation,
                expected_pid,
            })
            .send()
            .await
            .map_err(io::Error::other)?;
        let status = response.status();
        // This protocol returns only a PID and state; cap unexpected error bodies too.
        const MAXIMUM_RESPONSE_BYTES: usize = 4096;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(io::Error::other)? {
            if chunk.len() > MAXIMUM_RESPONSE_BYTES.saturating_sub(body.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "daemon response exceeds request limit",
                ));
            }
            body.extend_from_slice(&chunk);
        }
        original.check()?;
        if !status.is_success() {
            let kind = match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => io::ErrorKind::PermissionDenied,
                StatusCode::BAD_REQUEST => io::ErrorKind::InvalidInput,
                _ => io::ErrorKind::Other,
            };
            return Err(io::Error::new(
                kind,
                format!(
                    "daemon control HTTP {status}: {}",
                    String::from_utf8_lossy(&body)
                ),
            ));
        }
        let reply: Reply = serde_json::from_slice(&body)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if reply.pid == 0
            || expected_pid.is_some_and(|pid| pid.get() != reply.pid)
            || (operation == Operation::Stop && reply.state != State::Stopping)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid daemon control acknowledgement",
            ));
        }
        if operation == Operation::OpenUi {
            if reply.state != State::Running {
                return Err(io::ErrorKind::NotConnected.into());
            }
            let value = reply
                .ui_url
                .as_deref()
                .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
            validate_ui_url(value)?;
        } else if reply.ui_url.is_some() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(reply)
    };
    let result = tokio::select! {
        result = work => result,
        () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
        () = tokio::time::sleep_until(deadline) => Err(io::ErrorKind::TimedOut.into()),
    };
    original.check()?;
    result
}

/// Validate only the closed Browser entry URL, never credentials or arbitrary schemes.
pub(crate) fn validate_ui_url(raw: &str) -> io::Result<url::Url> {
    let url = url::Url::parse(raw).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"));
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/task/gantt"
        || url.host_str().is_none()
        || (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Browser entry URL",
        ));
    }
    Ok(url)
}

#[cfg(test)]
mod tests;
