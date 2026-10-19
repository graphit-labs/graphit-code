//! HTTP owns each mutable FILE job until its physical phase is quiescent.
use super::*;
use crate::{
    source_server::{
        SourceServer,
        web_file_mutations::{
            FileEffectFailure,
            inputs::{Operation, Query},
        },
    },
    task_server::TaskServer,
};
use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
};
use graphit_hub::policy::RequestContext;
use serde_json::Value;
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
        .route("/api/hub/files/install", post(install))
        .route("/api/hub/authored/install", post(install_local))
        .route("/api/hub/files/update", post(update))
        .route("/api/hub/files/remove", post(remove))
        .route("/api/hub/files/recover", post(recover))
        .route("/api/hub/files/forget", post(forget))
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
    Effects(io::Error),
    PreviousWorker,
    Worker,
}
impl From<Failure> for Error {
    fn from(value: Failure) -> Self {
        Self::Session(value)
    }
}
fn classified(error: &io::Error) -> (StatusCode, &'static str) {
    let local = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<FileEffectFailure>())
        .is_some_and(|effect| effect.outcome["operation"] == "install_local");
    match error.kind() {
        io::ErrorKind::PermissionDenied => {
            (StatusCode::FORBIDDEN, "authentication_or_permission_denied")
        }
        io::ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_arguments"),
        io::ErrorKind::NotFound => (StatusCode::NOT_FOUND, "artifact_not_found"),
        io::ErrorKind::AlreadyExists | io::ErrorKind::WouldBlock => {
            (StatusCode::CONFLICT, "installation_conflict")
        }
        io::ErrorKind::FileTooLarge if local => {
            (StatusCode::PAYLOAD_TOO_LARGE, "local_artifact_too_large")
        }
        io::ErrorKind::FileTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "remote_file_too_large"),
        io::ErrorKind::Interrupted => (
            StatusCode::from_u16(499).expect("valid HTTP status"),
            "cancelled",
        ),
        io::ErrorKind::TimedOut => (StatusCode::GATEWAY_TIMEOUT, "deadline"),
        io::ErrorKind::NotConnected => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        io::ErrorKind::Unsupported => (StatusCode::NOT_IMPLEMENTED, "unsupported"),
        io::ErrorKind::InvalidData if local => {
            (StatusCode::INTERNAL_SERVER_ERROR, "invalid_local_artifact")
        }
        io::ErrorKind::InvalidData => (StatusCode::INTERNAL_SERVER_ERROR, "invalid_remote_file"),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "file_installation_failed",
        ),
    }
}
fn encoded_response(status: StatusCode, bytes: Vec<u8>, inspection: bool) -> Response {
    let mut response = Response::new(axum::body::Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    if inspection {
        response.headers_mut().insert(
            "x-graphit-inspection-required",
            header::HeaderValue::from_static("request-selection"),
        );
    }
    response
}

/// The quota applies to the final envelope, including effect-bearing errors.
/// Omit a whole receipt when it cannot fit; never shorten identities or paths.
fn bounded_response(status: StatusCode, body: Value, maximum: usize) -> Response {
    if let Ok(bytes) = serde_json::to_vec(&body)
        && bytes.len() <= maximum
    {
        return encoded_response(status, bytes, false);
    }
    let outcome = body.get("outcome").unwrap_or(&body);
    let mut effects = serde_json::Map::new();
    for field in [
        "cache",
        "publication",
        "cleanup_pending",
        "stage_cleanup_pending",
        "blocked_by_pending",
        "observation_failed",
        "payload_preserved",
    ] {
        if let Some(value) = outcome.get(field) {
            effects.insert(field.into(), value.clone());
        }
    }
    let summary = json!({"error":{"code":"remote_file_too_large"},"inspection_required":true,"selection":"request","effects":effects});
    let minimal = json!({"error":{"code":"remote_file_too_large"},"effects":"inspect_selected_target","inspection_required":true});
    for candidate in [summary, minimal] {
        if let Ok(bytes) = serde_json::to_vec(&candidate)
            && bytes.len() <= maximum
        {
            return encoded_response(StatusCode::PAYLOAD_TOO_LARGE, bytes, true);
        }
    }
    // A quota smaller than the finite inspection envelope cannot hold JSON.
    // The status and fixed header retain that obligation without exceeding it.
    encoded_response(StatusCode::PAYLOAD_TOO_LARGE, Vec::new(), true)
}

fn finish(result: Result<Value, Error>, terminal: Option<Failure>, maximum: usize) -> Response {
    let overridden = terminal.map(|cause| match cause {
        Failure::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
        Failure::Broker(BrokerError::Deadline) => (StatusCode::GATEWAY_TIMEOUT, "deadline"),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    });
    let (status, body) = match result {
        Ok(outcome) => match overridden {
            Some((status, code)) => (status, json!({"error":{"code":code},"outcome":outcome})),
            None => (StatusCode::OK, outcome),
        },
        Err(Error::Effects(error)) => {
            let (status, code) = overridden.unwrap_or_else(|| classified(&error));
            let mut body = json!({"error":{"code":code}});
            if let Some(effect) = error
                .get_ref()
                .and_then(|error| error.downcast_ref::<FileEffectFailure>())
            {
                body["outcome"] = effect.outcome.clone();
            }
            (status, body)
        }
        Err(Error::Session(error)) => return error.into_response(),
        Err(Error::PreviousWorker) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error":{"code":"file_previous_job_failed"},"outcome":{"status":"error","cache":"not_attempted","publication":"not_attempted"}}),
        ),
        Err(Error::Worker) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error":{"code":"file_job_failed"},"outcome":{"status":"error","publication":{"change":"Unknown","committed":null},"observation_failed":true}}),
        ),
    };
    bounded_response(status, body, maximum)
}
async fn execute(
    routes: &Routes,
    entry: &Session,
    id: &str,
    cancellation: &Cancellation,
    deadline: Instant,
    query: Query,
) -> Result<Value, Error> {
    let shared = &routes.sessions.0;
    let (limits, _, _) = routes.tasks.browser_policy();
    let original =
        RequestContext::new(limits, cancellation, deadline).map_err(|_| Failure::Invalid)?;
    let result = async {
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
        let outcome = routes
            .source
            .web_file_mutation(
                &caller,
                verifier,
                provider,
                routes.backend.as_ref(),
                &original,
                query,
            )
            .await
            .map_err(Error::Effects)?;
        Ok(outcome)
    }
    .await;
    if let Err(Error::Session(error)) = &result
        && session::invalidates(error)
    {
        shared.invalidate(id, entry).await;
    }
    result
}
async fn install(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Install).await
}
async fn install_local(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::InstallLocal).await
}
async fn update(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Update).await
}
async fn remove(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Remove).await
}
async fn recover(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Recover).await
}
async fn forget(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Forget).await
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
    let (limits, timeout, stop) = routes.tasks.browser_policy();
    let Some(deadline) = arrival.checked_add(timeout) else {
        return Failure::Invalid.into_response();
    };
    let (id, entry) = tokio::select! {session=shared.session(&jar)=>match session{Ok(value)=>value,Err(error)=>return(shared.clear(jar,false),error).into_response()},()=shared.stop.cancelled()=>return Failure::Unavailable.into_response(),()=stop.cancelled()=>return Failure::Unavailable.into_response(),()=tokio::time::sleep_until(deadline.into())=>return Failure::Broker(BrokerError::Deadline).into_response()};
    let deadline = deadline.min(entry.expires);
    let cancellation = Cancellation::default();
    let _guard = live::CancelOnDrop(cancellation.clone());
    let body = tokio::select! {body=Bytes::from_request(request,&())=>match body{Ok(bytes)=>bytes,Err(error)=>return error.into_response()},()=entry.cancelled.cancelled()=>return Failure::Unauthorized.into_response(),()=shared.stop.cancelled()=>return Failure::Unavailable.into_response(),()=stop.cancelled()=>return Failure::Unavailable.into_response(),()=tokio::time::sleep_until(deadline.into())=>return Failure::Broker(BrokerError::Deadline).into_response()};
    let query = match Query::decode(&body, operation) {
        Ok(query) => query,
        Err(error) => return finish(Err(Error::Effects(error)), None, limits.max_output_bytes),
    };
    if let Err(error) = query.validate(&routes.source.policy().native_install.hub) {
        return finish(Err(Error::Effects(error)), None, limits.max_output_bytes);
    }
    let (send, mut receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {jobs=shared.jobs.lock()=>jobs,()=entry.cancelled.cancelled()=>return Failure::Unauthorized.into_response(),()=shared.stop.cancelled()=>return Failure::Unavailable.into_response(),()=stop.cancelled()=>return Failure::Unavailable.into_response(),()=tokio::time::sleep_until(deadline.into())=>return Failure::Broker(BrokerError::Deadline).into_response()};
        while let Some(result) = jobs.try_join_next() {
            if result.is_err() {
                return finish(Err(Error::PreviousWorker), None, limits.max_output_bytes);
            }
        }
        let routes = routes.clone();
        let entry = Arc::clone(&entry);
        let id = id.clone();
        let cancellation = cancellation.clone();
        let runtime = tokio::runtime::Handle::current();
        jobs.spawn_blocking(move || {
            let result = runtime.block_on(execute(
                &routes,
                &entry,
                &id,
                &cancellation,
                deadline,
                query,
            ));
            let _ = send.send(result);
        });
    }
    let result = tokio::select! {result=&mut receive=>Some(result),()=entry.cancelled.cancelled()=>None,()=shared.stop.cancelled()=>None,()=stop.cancelled()=>None,()=tokio::time::sleep_until(deadline.into())=>None};
    let result = match result {
        Some(result) => result,
        None => {
            cancellation.cancel();
            receive.await
        }
    }
    .unwrap_or(Err(Error::Worker));
    let terminal = if entry.cancelled.is_cancelled() || entry.expires <= Instant::now() {
        Some(Failure::Unauthorized)
    } else if shared.stop.is_cancelled() || stop.is_cancelled() {
        Some(Failure::Unavailable)
    } else if deadline <= Instant::now() {
        Some(Failure::Broker(BrokerError::Deadline))
    } else {
        None
    };
    let clear = matches!(terminal, Some(Failure::Unauthorized));
    let response = finish(result, terminal, limits.max_output_bytes);
    if clear {
        (shared.clear(jar, false), response).into_response()
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worker_failure_envelopes_keep_current_effects_unattempted_before_dispatch() {
        let previous = finish(Err(Error::PreviousWorker), None, 4096);
        assert_eq!(previous.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let previous: Value = serde_json::from_slice(
            &axum::body::to_bytes(previous.into_body(), 4096)
                .await
                .expect("finite previous-worker envelope"),
        )
        .expect("JSON previous-worker envelope");
        assert_eq!(
            previous,
            json!({"error":{"code":"file_previous_job_failed"},"outcome":{"status":"error","cache":"not_attempted","publication":"not_attempted"}})
        );

        let current = finish(Err(Error::Worker), None, 4096);
        assert_eq!(current.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let current: Value = serde_json::from_slice(
            &axum::body::to_bytes(current.into_body(), 4096)
                .await
                .expect("finite current-worker envelope"),
        )
        .expect("JSON current-worker envelope");
        assert_eq!(
            current,
            json!({"error":{"code":"file_job_failed"},"outcome":{"status":"error","publication":{"change":"Unknown","committed":null},"observation_failed":true}})
        );
    }
}
