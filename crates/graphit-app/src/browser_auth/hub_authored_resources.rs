//! Authored Resources reads retain their Web admission and owned worker until quiescence.
use super::*;
use crate::{
    authored_resources::{
        ReadFailure,
        inputs::{Operation, Query},
    },
    source_server::SourceServer,
    task_server::TaskServer,
};
use axum::{
    body::{Bytes, HttpBody},
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
    let output_maximum = tasks.browser_policy().0.max_output_bytes.min(
        source
            .policy()
            .native_install
            .hub
            .content()
            .max_output_bytes,
    );
    Router::new()
        .route("/api/hub/authored/list", post(list))
        .route("/api/hub/authored/review", post(review))
        .route("/api/hub/authored/content", post(content))
        .layer(DefaultBodyLimit::max(maximum))
        .layer(axum::middleware::map_response(move |response: Response| async move {
            bounded_existing_response(response, output_maximum)
        }))
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
    Read(io::Error),
    PreviousWorker,
    Worker,
}
impl From<Failure> for Error {
    fn from(value: Failure) -> Self {
        Self::Session(value)
    }
}
fn classified(error: &io::Error) -> (StatusCode, &'static str) {
    if let Some(failure) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<ReadFailure>())
    {
        return match failure {
            ReadFailure::SourceMissing => (StatusCode::NOT_FOUND, "authored_source_not_found"),
            ReadFailure::FileMissing => (StatusCode::NOT_FOUND, "authored_file_not_found"),
            ReadFailure::NotText => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "authored_content_not_text",
            ),
        };
    }
    match error.kind() {
        io::ErrorKind::PermissionDenied => {
            (StatusCode::FORBIDDEN, "authentication_or_permission_denied")
        }
        io::ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_arguments"),
        io::ErrorKind::NotFound => (StatusCode::NOT_FOUND, "authored_source_not_found"),
        io::ErrorKind::AlreadyExists | io::ErrorKind::WouldBlock => {
            (StatusCode::CONFLICT, "conflict")
        }
        io::ErrorKind::FileTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "authored_too_large"),
        io::ErrorKind::Interrupted => (
            StatusCode::from_u16(499).expect("valid HTTP status"),
            "cancelled",
        ),
        io::ErrorKind::TimedOut => (StatusCode::GATEWAY_TIMEOUT, "deadline"),
        io::ErrorKind::NotConnected => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        io::ErrorKind::Unsupported => (StatusCode::NOT_IMPLEMENTED, "unsupported"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "authored_read_failed"),
    }
}
fn encoded_response(status: StatusCode, bytes: Vec<u8>) -> Response {
    let mut response = Response::new(axum::body::Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    response
}
fn bounded_response(status: StatusCode, bytes: Vec<u8>, maximum: usize) -> Response {
    if bytes.len() <= maximum {
        return encoded_response(status, bytes);
    }
    let minimal = br#"{"error":{"code":"authored_too_large"}}"#;
    encoded_response(
        StatusCode::PAYLOAD_TOO_LARGE,
        if minimal.len() <= maximum {
            minimal.to_vec()
        } else {
            Vec::new()
        },
    )
}
/// Bound local authentication/extractor responses without changing their normal envelope.
fn bounded_existing_response(response: Response, maximum: usize) -> Response {
    if response
        .body()
        .size_hint()
        .upper()
        .is_some_and(|upper| usize::try_from(upper).is_ok_and(|upper| upper <= maximum))
    {
        return response;
    }
    let (mut parts, _) = response.into_parts();
    let replacement =
        failure_response(StatusCode::PAYLOAD_TOO_LARGE, "authored_too_large", maximum);
    parts.status = replacement.status();
    // Cookie clearing and cache/authentication headers remain effective after body replacement.
    for name in [
        header::CONTENT_LENGTH,
        header::CONTENT_ENCODING,
        header::CONTENT_RANGE,
        header::TRANSFER_ENCODING,
    ] {
        parts.headers.remove(name);
    }
    parts.headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    Response::from_parts(parts, replacement.into_body())
}
fn failure_response(status: StatusCode, code: &'static str, maximum: usize) -> Response {
    let bytes =
        serde_json::to_vec(&json!({"error":{"code":code}})).expect("finite authored error JSON");
    bounded_response(status, bytes, maximum)
}
fn finish(result: Result<Vec<u8>, Error>, terminal: Option<Failure>, maximum: usize) -> Response {
    // A terminal request cannot deliver any authored bytes, including an earlier successful read.
    if let Some(cause) = terminal {
        let (status, code) = match cause {
            Failure::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Failure::Broker(BrokerError::Deadline) => (StatusCode::GATEWAY_TIMEOUT, "deadline"),
            _ => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        };
        return failure_response(status, code, maximum);
    }
    match result {
        Ok(bytes) => bounded_response(StatusCode::OK, bytes, maximum),
        Err(Error::Read(error)) => {
            let (status, code) = classified(&error);
            failure_response(status, code, maximum)
        }
        Err(Error::Session(error)) => bounded_existing_response(error.into_response(), maximum),
        Err(Error::PreviousWorker | Error::Worker) => failure_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "authored_read_failed",
            maximum,
        ),
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
            .web_authored_resource(
                &caller,
                verifier,
                provider,
                routes.backend.as_ref(),
                &original,
                query,
            )
            .await
            .map_err(Error::Read)?;
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
async fn list(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::List).await
}
async fn review(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Review).await
}
async fn content(State(routes): State<Routes>, jar: CookieJar, request: Request) -> Response {
    respond(routes, jar, request, Operation::Content).await
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
    let query = match Query::decode(&body, operation) {
        Ok(query) => query,
        Err(error) => return finish(Err(Error::Read(error)), None, maximum),
    };
    if let Err(error) = query.validate(&routes.source.policy().native_install.hub) {
        return finish(Err(Error::Read(error)), None, maximum);
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
    use serde_json::Value;

    async fn body(response: Response) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), 4096)
            .await
            .expect("bounded authored response")
            .to_vec()
    }

    #[tokio::test]
    async fn session_errors_obey_tiny_quotas_and_preserve_normal_auth_responses() {
        let response = finish(Err(Error::Session(Failure::Unauthorized)), None, 1);
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(body(response).await.is_empty());

        let expected = body(Failure::Unauthorized.into_response()).await;
        let mut original = Failure::Unauthorized.into_response();
        original.headers_mut().insert(
            header::SET_COOKIE,
            header::HeaderValue::from_static("session=; Max-Age=0; HttpOnly; SameSite=Strict"),
        );
        let expected_headers = original.headers().clone();
        let response = bounded_existing_response(original, expected.len());
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers(), &expected_headers);
        assert_eq!(body(response).await, expected);

        let mut oversized = Failure::Unauthorized.into_response();
        oversized.headers_mut().insert(
            header::SET_COOKIE,
            expected_headers[header::SET_COOKIE].clone(),
        );
        oversized.headers_mut().insert(
            header::CONTENT_LENGTH,
            header::HeaderValue::from_str(&expected.len().to_string()).expect("length header"),
        );
        let response = bounded_existing_response(oversized, 1);
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            response.headers()[header::SET_COOKIE],
            expected_headers[header::SET_COOKIE]
        );
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            expected_headers[header::CACHE_CONTROL]
        );
        assert!(!response.headers().contains_key(header::CONTENT_LENGTH));
        assert!(body(response).await.is_empty());
    }

    #[tokio::test]
    async fn terminal_requests_discard_successes_and_internal_failures() {
        for terminal in 0..3 {
            for result in [
                Ok(br#"{"content":"private authored text"}"#.to_vec()),
                Err(Error::Read(io::Error::other("private filesystem detail"))),
                Err(Error::Worker),
                Err(Error::Session(Failure::Unauthorized)),
            ] {
                let (cause, status, code) = match terminal {
                    0 => (
                        Failure::Unauthorized,
                        StatusCode::UNAUTHORIZED,
                        "unauthorized",
                    ),
                    1 => (
                        Failure::Broker(BrokerError::Deadline),
                        StatusCode::GATEWAY_TIMEOUT,
                        "deadline",
                    ),
                    _ => (
                        Failure::Unavailable,
                        StatusCode::SERVICE_UNAVAILABLE,
                        "unavailable",
                    ),
                };
                let response = finish(result, Some(cause), 4096);
                assert_eq!(response.status(), status);
                let bytes = body(response).await;
                assert_eq!(
                    serde_json::from_slice::<Value>(&bytes).expect("terminal JSON"),
                    json!({"error":{"code":code}})
                );
            }
        }
    }

    #[tokio::test]
    async fn response_limits_cover_content_and_error_envelopes() {
        let payload = br#"{"status":"ok","content":{"content":"exact"}}"#.to_vec();
        let response = finish(Ok(payload.clone()), None, payload.len());
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body(response).await, payload);

        let error_bytes = br#"{"error":{"code":"authored_too_large"}}"#;
        for maximum in 0..=payload.len() {
            for result in [
                Ok(vec![b'x'; maximum + 1]),
                Err(Error::Read(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "private bound detail",
                ))),
            ] {
                let response = finish(result, None, maximum);
                assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
                let bytes = body(response).await;
                assert!(bytes.len() <= maximum);
                if error_bytes.len() <= maximum {
                    assert_eq!(bytes, error_bytes);
                } else {
                    assert!(bytes.is_empty());
                }
            }
        }
    }

    #[tokio::test]
    async fn missing_source_file_and_nontext_remain_distinct_and_sanitized() {
        for (error, status, code) in [
            (
                io::Error::new(io::ErrorKind::NotFound, ReadFailure::SourceMissing),
                StatusCode::NOT_FOUND,
                "authored_source_not_found",
            ),
            (
                io::Error::new(io::ErrorKind::NotFound, ReadFailure::FileMissing),
                StatusCode::NOT_FOUND,
                "authored_file_not_found",
            ),
            (
                io::Error::new(io::ErrorKind::InvalidData, ReadFailure::NotText),
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "authored_content_not_text",
            ),
            (
                io::Error::new(io::ErrorKind::InvalidData, "private capture detail"),
                StatusCode::INTERNAL_SERVER_ERROR,
                "authored_read_failed",
            ),
            (
                io::Error::new(io::ErrorKind::PermissionDenied, "private admission detail"),
                StatusCode::FORBIDDEN,
                "authentication_or_permission_denied",
            ),
        ] {
            let response = finish(Err(Error::Read(error)), None, 4096);
            assert_eq!(response.status(), status);
            assert_eq!(
                serde_json::from_slice::<Value>(&body(response).await)
                    .expect("finite read failure JSON"),
                json!({"error":{"code":code}})
            );
        }
    }
}
