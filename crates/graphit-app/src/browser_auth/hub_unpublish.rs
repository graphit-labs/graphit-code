//! Remote catalog retraction retains its request until owned work is quiescent.
use super::*;
use crate::{
    hub_unpublish::Arguments,
    source_server::{SourceServer, web_hub_unpublish::failure_body},
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
}
pub(super) fn router(sessions: BrowserAuth, tasks: TaskServer, source: SourceServer) -> Router {
    let maximum = tasks
        .browser_policy()
        .0
        .max_input_bytes
        .min(source.policy().native_install.hub.content().max_input_bytes);
    Router::new()
        .route("/api/hub/unpublish", post(respond))
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
    let cause = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<crate::hub_unpublish::Failure>())
        .map_or(error, |failure| &failure.cause);
    if let Some(storage) = cause
        .get_ref()
        .and_then(|error| error.downcast_ref::<object_store::Error>())
    {
        match storage {
            object_store::Error::PermissionDenied { .. } => {
                return (StatusCode::FORBIDDEN, "authentication_or_permission_denied");
            }
            object_store::Error::NotSupported { .. } => {
                return (StatusCode::NOT_IMPLEMENTED, "unsupported");
            }
            object_store::Error::NotFound { .. } => {
                return (StatusCode::NOT_FOUND, "remote_catalog_not_found");
            }
            _ => (),
        }
    }
    if crate::native_publication::cause_code(error) == "conditional_conflict" {
        return (StatusCode::CONFLICT, "hub_unpublish_conflict");
    }
    match error.kind() {
        io::ErrorKind::PermissionDenied => {
            (StatusCode::FORBIDDEN, "authentication_or_permission_denied")
        }
        io::ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_arguments"),
        io::ErrorKind::NotFound => (StatusCode::NOT_FOUND, "remote_catalog_not_found"),
        io::ErrorKind::AlreadyExists | io::ErrorKind::WouldBlock => {
            (StatusCode::CONFLICT, "hub_unpublish_conflict")
        }
        io::ErrorKind::FileTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "hub_unpublish_too_large"),
        io::ErrorKind::Interrupted => (
            StatusCode::from_u16(499).expect("valid HTTP status"),
            "cancelled",
        ),
        io::ErrorKind::TimedOut => (StatusCode::GATEWAY_TIMEOUT, "deadline"),
        io::ErrorKind::NotConnected => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        io::ErrorKind::Unsupported => (StatusCode::NOT_IMPLEMENTED, "unsupported"),
        io::ErrorKind::InvalidData => (StatusCode::INTERNAL_SERVER_ERROR, "invalid_remote_catalog"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "hub_unpublish_failed"),
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
/// Omit a whole receipt when it cannot fit; never shorten identities or versions.
fn bounded_response(status: StatusCode, body: Value, maximum: usize) -> Response {
    if let Ok(bytes) = serde_json::to_vec(&body)
        && bytes.len() <= maximum
    {
        return encoded_response(status, bytes, false);
    }
    let outcome = body.get("outcome").unwrap_or(&body);
    let mut effects = serde_json::Map::new();
    for field in ["retraction", "stage", "cause", "observation_failed"] {
        if let Some(value) = outcome.get(field) {
            effects.insert(field.into(), value.clone());
        }
    }
    let summary = json!({"error":{"code":"hub_unpublish_too_large"},"inspection_required":true,"selection":"request","effects":effects});
    let minimal = json!({"error":{"code":"hub_unpublish_too_large"},"effects":"inspect_selected_target","inspection_required":true});
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
            let body = json!({"error":{"code":code},"outcome":failure_body(&error)});
            (status, body)
        }
        Err(Error::Session(error)) => return error.into_response(),
        Err(Error::PreviousWorker) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error":{"code":"previous_job_failed"},"outcome":{"status":"error","retraction":{"catalog":"not_attempted"}}}),
        ),
        Err(Error::Worker) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error":{"code":"hub_unpublish_job_failed"},"outcome":{"status":"error","retraction":{"catalog":"unknown"},"observation_failed":true}}),
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
    query: Arguments,
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
            .web_hub_unpublish(&caller, verifier, provider, &original, query)
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
async fn respond(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
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
    let maximum = limits.max_output_bytes.min(
        routes
            .source
            .policy()
            .native_install
            .hub
            .content()
            .max_output_bytes,
    );
    let Some(deadline) = arrival.checked_add(timeout) else {
        return Failure::Invalid.into_response();
    };
    let (id, entry) = tokio::select! {session=shared.session(&jar)=>match session{Ok(value)=>value,Err(error)=>return(shared.clear(jar,false),error).into_response()},()=shared.stop.cancelled()=>return Failure::Unavailable.into_response(),()=stop.cancelled()=>return Failure::Unavailable.into_response(),()=tokio::time::sleep_until(deadline.into())=>return Failure::Broker(BrokerError::Deadline).into_response()};
    let deadline = deadline.min(entry.expires);
    let cancellation = Cancellation::default();
    let _guard = live::CancelOnDrop(cancellation.clone());
    let body = tokio::select! {body=Bytes::from_request(request,&())=>match body{Ok(bytes)=>bytes,Err(error)=>return error.into_response()},()=entry.cancelled.cancelled()=>return Failure::Unauthorized.into_response(),()=shared.stop.cancelled()=>return Failure::Unavailable.into_response(),()=stop.cancelled()=>return Failure::Unavailable.into_response(),()=tokio::time::sleep_until(deadline.into())=>return Failure::Broker(BrokerError::Deadline).into_response()};
    let query = match serde_json::from_slice::<Arguments>(&body) {
        Ok(query) => query,
        Err(_) => {
            return finish(
                Err(Error::Effects(io::ErrorKind::InvalidInput.into())),
                None,
                maximum,
            );
        }
    };
    if let Err(error) = query.validate(&routes.source.policy().native_install.hub) {
        return finish(Err(Error::Effects(error)), None, maximum);
    }
    let (send, mut receive) = oneshot::channel();
    {
        let mut jobs = tokio::select! {jobs=shared.jobs.lock()=>jobs,()=entry.cancelled.cancelled()=>return Failure::Unauthorized.into_response(),()=shared.stop.cancelled()=>return Failure::Unavailable.into_response(),()=stop.cancelled()=>return Failure::Unavailable.into_response(),()=tokio::time::sleep_until(deadline.into())=>return Failure::Broker(BrokerError::Deadline).into_response()};
        while let Some(result) = jobs.try_join_next() {
            if result.is_err() {
                return finish(Err(Error::PreviousWorker), None, maximum);
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
    let response = finish(result, terminal, maximum);
    if clear {
        (shared.clear(jar, false), response).into_response()
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn body(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    fn receipt() -> crate::hub_unpublish::Receipt {
        crate::hub_unpublish::Receipt {
            producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            artifact_type: graphit_hub::ArtifactType::Skill,
            id: "fixture/".to_owned() + &"publication".repeat(16),
            targeted_versions: vec!["release/one".into(), "release/two".into()],
            desired_latest: None,
            catalog: "removed",
        }
    }
    #[tokio::test]
    async fn late_session_and_kernel_failure_preserve_retraction_effects() {
        let unpublished = json!({"status":"unpublished","retraction":receipt()});
        let response = finish(Ok(unpublished.clone()), Some(Failure::Unauthorized), 8192);
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body(response).await["outcome"], unpublished);
        let cause = io::Error::new(
            io::ErrorKind::TimedOut,
            crate::hub_unpublish::Failure {
                receipt: receipt(),
                stage: "delivery",
                cause: io::Error::new(io::ErrorKind::TimedOut, "private failure evidence"),
            },
        );
        let response = finish(Err(Error::Effects(cause)), None, 8192);
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        let result = body(response).await;
        assert_eq!(result["outcome"]["stage"], "delivery");
        assert_eq!(result["outcome"]["retraction"]["catalog"], "removed");
        assert_eq!(
            result["outcome"]["retraction"]["targeted_versions"],
            json!(["release/one", "release/two"])
        );
        assert!(!result.to_string().contains("private failure evidence"));
    }
    #[tokio::test]
    async fn previous_job_failure_keeps_this_retraction_not_attempted() {
        let previous = body(finish(Err(Error::PreviousWorker), None, 8192)).await;
        assert_eq!(previous["error"]["code"], "previous_job_failed");
        assert_eq!(
            previous["outcome"]["retraction"]["catalog"],
            "not_attempted"
        );
        let own = body(finish(Err(Error::Worker), None, 8192)).await;
        assert_eq!(own["outcome"]["retraction"]["catalog"], "unknown");
        assert_eq!(own["outcome"]["observation_failed"], true);
    }
    #[tokio::test]
    async fn bounded_retraction_requires_inspection_without_truncating_receipt() {
        for maximum in [1, 128, 256] {
            let response = bounded_response(
                StatusCode::OK,
                json!({"status":"unpublished","retraction":receipt()}),
                maximum,
            );
            assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
            assert_eq!(
                response.headers()["x-graphit-inspection-required"],
                "request-selection"
            );
            let bytes = axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap();
            assert!(bytes.len() <= maximum);
            if !bytes.is_empty() {
                let result: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(result["inspection_required"], true);
                assert_eq!(result["effects"], "inspect_selected_target");
            }
        }
    }
}
