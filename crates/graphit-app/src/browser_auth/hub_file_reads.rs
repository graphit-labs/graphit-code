//! FILE readonly operations retain the verified Web and PROJECT admissions.
use super::*;
use crate::{
    source_server::{SourceServer, web_file_reads::inputs::Query as FileQuery},
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
    backend: Arc<graphit_auth::broker::project_access::BrokerProjectAccess>,
}
pub(super) fn router(
    sessions: BrowserAuth,
    tasks: TaskServer,
    source: SourceServer,
    backend: Arc<graphit_auth::broker::project_access::BrokerProjectAccess>,
) -> Router {
    let maximum = tasks
        .browser_policy()
        .0
        .max_input_bytes
        .min(source.policy().native_install.hub.content().max_input_bytes);
    Router::new()
        .route("/api/hub/contexts", post(contexts))
        .route("/api/hub/files/content", post(content))
        .route("/api/hub/installed/files", post(list))
        .layer(DefaultBodyLimit::max(maximum))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes {
            sessions,
            tasks,
            source,
            backend,
        })
}
enum Error {
    Session(Failure),
    Catalog(io::Error),
}
enum Query {
    Files(FileQuery),
    Contexts,
}
#[derive(Clone, Copy)]
enum Operation {
    Content,
    List,
    Contexts,
}
impl Operation {
    fn decode(self, body: &[u8]) -> io::Result<Query> {
        match self {
            Self::Content | Self::List => {
                FileQuery::decode(body, matches!(self, Self::List)).map(Query::Files)
            }
            Self::Contexts => {
                let object =
                    serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(body)
                        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
                if object.is_empty() {
                    Ok(Query::Contexts)
                } else {
                    Err(io::ErrorKind::InvalidInput.into())
                }
            }
        }
    }
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
                let (status, code) = match error.kind() {
                    io::ErrorKind::PermissionDenied => (StatusCode::FORBIDDEN, "permission_denied"),
                    io::ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_request"),
                    io::ErrorKind::NotFound => (StatusCode::NOT_FOUND, "file_not_found"),
                    io::ErrorKind::AlreadyExists => (StatusCode::CONFLICT, "state_conflict"),
                    io::ErrorKind::Interrupted => (
                        StatusCode::from_u16(499).expect("valid HTTP status"),
                        "cancelled",
                    ),
                    io::ErrorKind::TimedOut => (StatusCode::GATEWAY_TIMEOUT, "deadline"),
                    io::ErrorKind::NotConnected => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
                    io::ErrorKind::FileTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "limit"),
                    _ => (StatusCode::INTERNAL_SERVER_ERROR, "file_read_unavailable"),
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
        let bytes = match query {
            Query::Files(query) => {
                routes
                    .source
                    .web_file_read(
                        &caller,
                        verifier,
                        provider,
                        routes.backend.as_ref(),
                        &original,
                        query,
                    )
                    .await
            }
            Query::Contexts => {
                routes
                    .source
                    .web_hub_contexts(
                        &caller,
                        verifier,
                        provider,
                        routes.backend.as_ref(),
                        &original,
                    )
                    .await
            }
        }
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
async fn content(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Content).await
}
async fn list(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::List).await
}
async fn contexts(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Contexts).await
}
async fn respond(
    routes: Routes,
    jar: CookieJar,
    request: Request,
    operation: Operation,
) -> Response {
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
    let query = match operation.decode(&body) {
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
