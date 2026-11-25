//! Selected Live evidence under fresh Web, USER and source admissions.
use super::*;
use crate::{live_agent_read, reference_explorer, task_server::TaskServer};
use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
};
use graphit_auth::broker::project_access::BrokerProjectAccess;
use graphit_hub::policy::RequestContext;

#[derive(Clone)]
struct Routes {
    sessions: BrowserAuth,
    tasks: TaskServer,
    backend: Arc<BrokerProjectAccess>,
}

pub(super) fn router(
    sessions: BrowserAuth,
    tasks: TaskServer,
    backend: Arc<BrokerProjectAccess>,
) -> Router {
    let maximum = tasks.browser_policy().0.max_input_bytes;
    Router::new()
        .route("/api/live/sessions/{id}/knowledge/page", get(read))
        .layer(DefaultBodyLimit::max(maximum))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes {
            sessions,
            tasks,
            backend,
        })
}

enum Error {
    Session(Failure),
    Catalog(Box<Response>),
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
            Self::Catalog(response) => *response,
        }
    }
}

async fn execute(
    routes: &Routes,
    entry: &Session,
    id: &str,
    cancellation: &Cancellation,
    deadline: Instant,
    live_id: &str,
    query: &PageQuery,
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
        let bytes = live_agent_read::page(
            &routes.tasks,
            (&caller, verifier, provider),
            &routes.backend,
            &original,
            live_id,
            &query.generation,
            &query.page,
            query
                .context
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| Failure::Invalid)?
                .as_ref(),
            query
                .reference
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| Failure::Invalid)?
                .as_ref(),
        )
        .await
        .map_err(|error| Error::Catalog(Box::new(read_failure(error))))?;
        original.snapshot().check().map_err(|error| {
            Error::Catalog(crate::live_agent_catalog::failure(
                reference_explorer::Failure::Physical(error),
            ))
        })?;
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

fn readonly_origin(shared: &Shared, headers: &HeaderMap) -> Result<(), Failure> {
    if !headers.contains_key(header::ORIGIN)
        && headers
            .get("sec-fetch-site")
            .and_then(|value| value.to_str().ok())
            == Some("same-origin")
        && matches!(
            headers
                .get("sec-fetch-mode")
                .and_then(|value| value.to_str().ok()),
            Some("cors" | "same-origin")
        )
    {
        return if shared.stop.is_cancelled() {
            Err(Failure::Unavailable)
        } else {
            Ok(())
        };
    }
    shared.origin(headers)
}

#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PageQuery {
    generation: String,
    page: String,
    context: Option<String>,
    reference: Option<String>,
}

fn read_failure(error: crate::live_agent_tools::Error) -> Response {
    use crate::live_agent_tools::Error as E;
    let (status, code) = match error {
        E::ContextChanged => (StatusCode::CONFLICT, "context_changed"),
        E::Read(crate::reference_explorer::Failure::Physical(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            (StatusCode::NOT_FOUND, "not_found")
        }
        E::Read(crate::reference_explorer::Failure::Physical(error))
            if error.kind() == std::io::ErrorKind::PermissionDenied =>
        {
            (StatusCode::FORBIDDEN, "permission_denied")
        }
        E::Read(error) => error.response(),
        E::Live(error) => crate::task_server::browser::live::response(&error),
        E::Web(_) => (StatusCode::FORBIDDEN, "authentication_or_permission_denied"),
        E::Knowledge(_) => (StatusCode::CONFLICT, "knowledge_read_failed"),
    };
    (status, Json(serde_json::json!({"error":{"code":code}}))).into_response()
}

async fn read(
    State(routes): State<Routes>,
    axum::extract::Path(live_id): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<PageQuery>,
    jar: CookieJar,
    request: Request,
) -> Response {
    let arrival = Instant::now();
    let shared = &routes.sessions.0;
    if let Err(error) = readonly_origin(shared, request.headers()) {
        return error.into_response();
    }
    if request
        .uri()
        .query()
        .is_none_or(|query| query.len() > routes.tasks.browser_policy().0.max_input_bytes)
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
    if !body.is_empty() {
        return Failure::Invalid.into_response();
    }
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
            let _ = send.send(
                execute(
                    &routes,
                    &entry,
                    &id,
                    &cancellation,
                    deadline,
                    &live_id,
                    &query,
                )
                .await,
            );
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
