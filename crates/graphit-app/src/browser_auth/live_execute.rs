//! Web execution owns its job independently of the response and SSE subscribers.
use super::*;
use crate::{
    knowledge_live::{CreateRequest, SendRequest, Session as LiveSession},
    task_server::{
        TaskServer,
        browser::{codec, live::Error as LiveFailure},
    },
};
use graphit_auth::broker::project_access::BrokerProjectAccess;
use std::net::SocketAddr;

mod operation;

#[derive(Clone)]
struct Routes {
    sessions: BrowserAuth,
    tasks: TaskServer,
    backend: Arc<BrokerProjectAccess>,
    address: SocketAddr,
    executable: std::path::PathBuf,
    source: Option<crate::source_server::SourceServer>,
    mcp: super::live_mcp::LiveMcpBroker,
}

enum Command {
    Create(Box<CreateRequest>),
    Send { id: String, input: SendRequest },
}

type Error = super::live::Error;
type Reply = oneshot::Sender<Result<LiveSession, Error>>;

pub(super) fn router(
    sessions: BrowserAuth,
    tasks: TaskServer,
    backend: Arc<BrokerProjectAccess>,
    address: SocketAddr,
    executable: std::path::PathBuf,
    source: Option<crate::source_server::SourceServer>,
) -> Router {
    let limits = tasks.browser_policy();
    let mcp = super::live_mcp::LiveMcpBroker::new();
    let ingress = mcp.router(limits.0, limits.2);
    Router::new()
        .route("/api/live/sessions", post(create))
        .route("/api/live/sessions/{id}/messages", post(send))
        .layer(DefaultBodyLimit::max(limits.0.max_input_bytes))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes {
            sessions,
            tasks,
            backend,
            address,
            executable,
            source,
            mcp,
        })
        .merge(ingress)
}

fn io_failure(cause: io::Error) -> Error {
    Error::Live(LiveFailure::Live(cause.into()))
}

async fn mutation(
    routes: Routes,
    jar: CookieJar,
    headers: HeaderMap,
    request: axum::extract::Request,
    id: Option<String>,
) -> Response {
    // Capture before reading the body or waiting for credential refresh.
    let arrival = Instant::now();
    let shared = &routes.sessions.0;
    if let Err(error) = shared.origin(&headers) {
        return error.into_response();
    }
    let policy = routes.tasks.browser_policy();
    let Some(session_deadline) = arrival.checked_add(policy.1) else {
        return Failure::Invalid.into_response();
    };
    let session = tokio::select! {
        result = shared.session(&jar) => result,
        () = shared.stop.cancelled() => return Failure::Unavailable.into_response(),
        () = policy.2.cancelled() => return Failure::Unavailable.into_response(),
        () = tokio::time::sleep_until(session_deadline.into()) => return io_failure(io::ErrorKind::TimedOut.into()).into_response(),
    };
    let (web_id, entry) = match session {
        Ok(value) => value,
        Err(error) => return (shared.clear(jar, false), error).into_response(),
    };
    let Some(deadline) = arrival
        .checked_add(policy.1)
        .map(|value| value.min(entry.expires))
    else {
        return Failure::Invalid.into_response();
    };
    if deadline <= Instant::now() {
        return io_failure(io::ErrorKind::TimedOut.into()).into_response();
    }
    let body = tokio::select! {
        value = axum::body::to_bytes(request.into_body(), policy.0.max_input_bytes) => match value {
            Ok(value) => value,
            Err(_) => return (axum::http::StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({"error":{"code":"invalid_body"}}))).into_response(),
        },
        () = shared.stop.cancelled() => return Failure::Unavailable.into_response(),
        () = policy.2.cancelled() => return Failure::Unavailable.into_response(),
        () = entry.cancelled.cancelled() => return Failure::Unauthorized.into_response(),
        () = tokio::time::sleep_until(deadline.into()) => return io_failure(io::ErrorKind::TimedOut.into()).into_response(),
    };
    let command = match id {
        Some(id) => match codec::decode(&body, policy.0) {
            Ok(input) => Command::Send { id, input },
            Err(_) => return Failure::Invalid.into_response(),
        },
        None => match codec::decode(&body, policy.0) {
            Ok(input) => Command::Create(input),
            Err(_) => return Failure::Invalid.into_response(),
        },
    };
    drop(body);
    let (reply, receive) = oneshot::channel();
    let job_routes = routes.clone();
    let job_entry = Arc::clone(&entry);
    let work = async move {
        let cancellation = Cancellation::default();
        let mut reply = Some(reply);
        let outcome = operation::execute(
            &job_routes,
            &web_id,
            &job_entry,
            &cancellation,
            deadline,
            command,
            &mut reply,
        )
        .await;
        if let Some(reply) = reply {
            let _ = reply.send(outcome);
        }
    };
    let queued = tokio::select! {
        result = routes.tasks.retain_browser_ask(work) => result,
        () = shared.stop.cancelled() => return Failure::Unavailable.into_response(),
        () = policy.2.cancelled() => return Failure::Unavailable.into_response(),
        () = entry.cancelled.cancelled() => return Failure::Unauthorized.into_response(),
        () = tokio::time::sleep_until(deadline.into()) => return io_failure(io::ErrorKind::TimedOut.into()).into_response(),
    };
    if queued.is_err() {
        return Failure::Unavailable.into_response();
    }
    let result = tokio::select! {
        result = receive => result.unwrap_or(Err(Error::Session(Failure::Unavailable))),
        () = shared.stop.cancelled() => Err(Error::Session(Failure::Unavailable)),
        () = policy.2.cancelled() => Err(Error::Session(Failure::Unavailable)),
        () = tokio::time::sleep_until(deadline.into()) => Err(io_failure(io::ErrorKind::TimedOut.into())),
    };
    if entry.cancelled.is_cancelled() || shared.stop.is_cancelled() || policy.2.is_cancelled() {
        return Failure::Unauthorized.into_response();
    }
    if Instant::now() >= deadline {
        return io_failure(io::ErrorKind::TimedOut.into()).into_response();
    }
    match result {
        Ok(session) => match codec::encode_response(&session, policy.0.max_output_bytes) {
            Ok(bytes) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
            Err(_) => io_failure(io::ErrorKind::FileTooLarge.into()).into_response(),
        },
        Err(error) => error.into_response(),
    }
}

async fn create(
    State(routes): State<Routes>,
    jar: CookieJar,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Response {
    mutation(routes, jar, headers, request, None).await
}
async fn send(
    State(routes): State<Routes>,
    axum::extract::Path(id): axum::extract::Path<String>,
    jar: CookieJar,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> Response {
    mutation(routes, jar, headers, request, Some(id)).await
}
