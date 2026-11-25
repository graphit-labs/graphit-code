//! Remote Hub metadata through the existing cookie-authenticated Web owner.
use super::*;
use crate::{
    source_server::{SourceServer, web_hub_catalog::Query},
    task_server::TaskServer,
};
use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
};
use graphit_hub::policy::RequestContext;
use std::io;

#[derive(Clone)]
struct Routes {
    sessions: BrowserAuth,
    tasks: TaskServer,
    source: SourceServer,
}
pub(super) fn router(sessions: BrowserAuth, tasks: TaskServer, source: SourceServer) -> Router {
    let maximum = tasks
        .browser_policy()
        .0
        .max_input_bytes
        .min(source.policy().native_install.hub.content().max_input_bytes);
    Router::new()
        .route("/api/hub/remote/projects", post(projects))
        .route("/api/hub/remote/search", post(search))
        .layer(DefaultBodyLimit::max(maximum))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes {
            sessions,
            tasks,
            source,
        })
}
enum Error {
    Session(Failure),
    Catalog(io::Error),
}
impl From<Failure> for Error {
    fn from(value: Failure) -> Self {
        Self::Session(value)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Self::Session(error) => error.into_response(),
            Self::Catalog(error) => {
                let (status, code) = if crate::remote_hub_catalog::is_restart(&error) {
                    (StatusCode::CONFLICT, "catalog_restart_required")
                } else {
                    match error.kind() {
                        io::ErrorKind::PermissionDenied => {
                            (StatusCode::FORBIDDEN, "permission_denied")
                        }
                        io::ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_request"),
                        io::ErrorKind::NotFound => (StatusCode::NOT_FOUND, "catalog_not_found"),
                        io::ErrorKind::Interrupted => (
                            StatusCode::from_u16(499).expect("valid HTTP status"),
                            "cancelled",
                        ),
                        io::ErrorKind::TimedOut => (StatusCode::GATEWAY_TIMEOUT, "deadline"),
                        io::ErrorKind::NotConnected => {
                            (StatusCode::SERVICE_UNAVAILABLE, "unavailable")
                        }
                        io::ErrorKind::FileTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "limit"),
                        _ => (StatusCode::INTERNAL_SERVER_ERROR, "hub_catalog_unavailable"),
                    }
                };
                (status, Json(json!({"error":{"code":code}}))).into_response()
            }
        }
    }
}
async fn execute(
    routes: &Routes,
    entry: &Session,
    id: &str,
    cancellation: &Cancellation,
    deadline: Instant,
    query: Query,
) -> Result<Vec<u8>, Error> {
    let shared = &routes.sessions.0;
    let (limits, _, stop) = routes.tasks.browser_policy();
    if deadline <= Instant::now() {
        return Err(Failure::Broker(BrokerError::Deadline).into());
    }
    let original =
        RequestContext::new(limits, cancellation, deadline).map_err(|_| Failure::Invalid)?;
    let work = async {
        let credentials = shared.credentials(entry, original.snapshot()).await?;
        let access = zeroize::Zeroizing::new(credentials.access_token().to_owned());
        drop(credentials);
        let flow = shared.flow(entry.provider)?;
        let (provider, verifier) = &shared.providers[entry.provider];
        let context = BrokerRequestContext::new(original.snapshot());
        let caller = flow
            .verify_access(&entry.registered, &access, context)
            .await
            .map_err(Failure::from)?;
        caller.check(verifier, context).map_err(Failure::from)?;
        let bytes = routes
            .source
            .web_remote_hub_catalog(&caller, verifier, provider, &original, query)
            .await
            .map_err(Error::Catalog)?;
        if bytes.len() > limits.max_output_bytes {
            return Err(Error::Catalog(io::ErrorKind::FileTooLarge.into()));
        }
        original.check().map_err(Error::Catalog)?;
        if entry.cancelled.is_cancelled() || entry.expires <= Instant::now() {
            return Err(Failure::Unauthorized.into());
        }
        if shared.stop.is_cancelled() || stop.is_cancelled() {
            return Err(Failure::Unavailable.into());
        }
        Ok(bytes)
    }
    .await;
    if let Err(Error::Session(error)) = &work
        && session::invalidates(error)
    {
        shared.invalidate(id, entry).await;
    }
    work
}
async fn projects(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, false).await
}
async fn search(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, true).await
}
async fn respond(routes: Routes, jar: CookieJar, request: Request, search: bool) -> Response {
    let arrival = Instant::now();
    let shared = &routes.sessions.0;
    if let Err(error) = shared.origin(request.headers()) {
        return error.into_response();
    }
    if request.uri().query().is_some()
        || request
            .headers()
            .get_all(header::CONTENT_TYPE)
            .iter()
            .count()
            != 1
        || request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|value| value.split(';').next().map(str::trim) != Some("application/json"))
    {
        return Failure::Invalid.into_response();
    }
    let (_, timeout, stop) = routes.tasks.browser_policy();
    let Some(deadline) = arrival.checked_add(timeout) else {
        return Failure::Invalid.into_response();
    };
    let (id, entry) = tokio::select! {
        session = shared.session(&jar) => match session {
            Ok(value) => value,
            Err(error) => return (shared.clear(jar, false), error).into_response(),
        },
        () = shared.stop.cancelled() => return Failure::Unavailable.into_response(),
        () = stop.cancelled() => return Failure::Unavailable.into_response(),
        () = tokio::time::sleep_until(deadline.into()) => return Failure::Broker(BrokerError::Deadline).into_response(),
    };
    let deadline = deadline.min(entry.expires);
    if deadline <= Instant::now() {
        return Failure::Broker(BrokerError::Deadline).into_response();
    }
    let cancellation = Cancellation::default();
    let _guard = live::CancelOnDrop(cancellation.clone());
    let body = tokio::select! {
        body = Bytes::from_request(request, &()) => match body {
            Ok(body) => body,
            Err(error) => return error.into_response(),
        },
        () = entry.cancelled.cancelled() => return Failure::Unauthorized.into_response(),
        () = shared.stop.cancelled() => return Failure::Unavailable.into_response(),
        () = stop.cancelled() => return Failure::Unavailable.into_response(),
        () = tokio::time::sleep_until(deadline.into()) => return Failure::Broker(BrokerError::Deadline).into_response(),
    };
    let query = match Query::decode(&body, search) {
        Ok(query) => query,
        Err(error) => return Error::Catalog(error).into_response(),
    };
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {
            jobs = shared.jobs.lock() => jobs,
            () = entry.cancelled.cancelled() => return Failure::Unauthorized.into_response(),
            () = shared.stop.cancelled() => return Failure::Unavailable.into_response(),
            () = stop.cancelled() => return Failure::Unavailable.into_response(),
            () = tokio::time::sleep_until(deadline.into()) => return Failure::Broker(BrokerError::Deadline).into_response(),
        };
        while jobs.try_join_next().is_some() {}
        let routes = routes.clone();
        let entry = Arc::clone(&entry);
        let id = id.clone();
        jobs.spawn(async move {
            let _ = send.send(execute(&routes, &entry, &id, &cancellation, deadline, query).await);
        });
    }
    let result = tokio::select! {
        result = receive => result.unwrap_or(Err(Error::Session(Failure::Unavailable))),
        () = entry.cancelled.cancelled() => Err(Error::Session(Failure::Unauthorized)),
        () = shared.stop.cancelled() => Err(Error::Session(Failure::Unavailable)),
        () = stop.cancelled() => Err(Error::Session(Failure::Unavailable)),
        () = tokio::time::sleep_until(deadline.into()) => Err(Error::Session(Failure::Broker(BrokerError::Deadline))),
    };
    if entry.cancelled.is_cancelled() || entry.expires <= Instant::now() {
        return (shared.clear(jar, false), Failure::Unauthorized).into_response();
    }
    if shared.stop.is_cancelled() || stop.is_cancelled() {
        return Failure::Unavailable.into_response();
    }
    if deadline <= Instant::now() {
        return Failure::Broker(BrokerError::Deadline).into_response();
    }
    match result {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
        Err(error) => error.into_response(),
    }
}
