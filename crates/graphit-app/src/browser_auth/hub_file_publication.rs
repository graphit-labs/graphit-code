//! One actual Web publisher owns its finite request through storage quiescence.
use super::*;
use crate::{
    source_server::{
        SourceServer,
        web_file_publication::{Query, failure_body},
        web_hub_upload,
    },
    task_server::TaskServer,
};
use axum::{
    body::Bytes,
    extract::{FromRequest, Multipart, Request},
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
    let upload_maximum = usize::try_from(
        source
            .policy()
            .native_install
            .hub
            .copy()
            .total_file_bytes_limit(),
    )
    .ok()
    .and_then(|bytes| {
        maximum
            .checked_mul(2)
            .and_then(|overhead| bytes.checked_add(overhead))
    })
    .unwrap_or(0);
    Router::new()
        .route(
            "/api/hub/files/publish",
            post(respond).layer(DefaultBodyLimit::max(maximum)),
        )
        .route(
            "/api/hub/upload",
            post(respond_upload).layer(DefaultBodyLimit::max(upload_maximum)),
        )
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes {
            sessions,
            tasks,
            source,
            backend,
        })
}
enum Submission {
    Resources(Query),
    Received(Request),
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
    if crate::native_publication::cause_code(error) == "conditional_conflict" {
        return (StatusCode::CONFLICT, "file_publication_conflict");
    }
    match error.kind() {
        io::ErrorKind::PermissionDenied => {
            (StatusCode::FORBIDDEN, "authentication_or_permission_denied")
        }
        io::ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_arguments"),
        io::ErrorKind::NotFound => (StatusCode::NOT_FOUND, "publication_source_not_found"),
        io::ErrorKind::AlreadyExists | io::ErrorKind::WouldBlock => {
            (StatusCode::CONFLICT, "file_publication_conflict")
        }
        io::ErrorKind::FileTooLarge => {
            (StatusCode::PAYLOAD_TOO_LARGE, "file_publication_too_large")
        }
        io::ErrorKind::Interrupted => (
            StatusCode::from_u16(499).expect("valid HTTP status"),
            "cancelled",
        ),
        io::ErrorKind::TimedOut => (StatusCode::GATEWAY_TIMEOUT, "deadline"),
        io::ErrorKind::NotConnected => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        io::ErrorKind::Unsupported => (StatusCode::NOT_IMPLEMENTED, "unsupported"),
        io::ErrorKind::InvalidData => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_publication_source",
        ),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "file_publication_failed"),
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
        "publication",
        "project_metadata",
        "stage",
        "cause",
        "owned_temporary_residue",
        "observation_failed",
    ] {
        if let Some(value) = outcome.get(field) {
            effects.insert(field.into(), value.clone());
        }
    }
    let summary = json!({"error":{"code":"file_publication_too_large"},"inspection_required":true,"selection":"request","effects":effects});
    let minimal = json!({"error":{"code":"file_publication_too_large"},"effects":"inspect_selected_target","inspection_required":true});
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
            json!({"error":{"code":"previous_job_failed"},"outcome":{"status":"error","publication":"not_attempted","project_metadata":"not_written"}}),
        ),
        Err(Error::Worker) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"error":{"code":"file_publication_job_failed"},"outcome":{"status":"error","publication":"unknown","project_metadata":"unknown","observation_failed":true}}),
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
    query: Submission,
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
        let outcome = match query {
            Submission::Resources(query) => {
                routes
                    .source
                    .web_file_publication(
                        &caller,
                        verifier,
                        provider,
                        routes.backend.as_ref(),
                        &original,
                        query,
                    )
                    .await
            }
            Submission::Received(request) => {
                let multipart = Multipart::from_request(request, &())
                    .await
                    .map_err(|_| Error::Effects(io::ErrorKind::InvalidInput.into()))?;
                routes
                    .source
                    .web_hub_upload(
                        &caller,
                        verifier,
                        provider,
                        routes.backend.as_ref(),
                        &original,
                        web_hub_upload::Input {
                            multipart,
                            metadata_limit: limits.max_input_bytes,
                        },
                    )
                    .await
            }
        }
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
    respond_inner(routes, jar, request, false).await
}
async fn respond_upload(
    State(routes): State<Routes>,
    jar: CookieJar,
    request: Request,
) -> Response {
    let maximum = routes.tasks.browser_policy().0.max_output_bytes.min(
        routes
            .source
            .policy()
            .native_install
            .hub
            .content()
            .max_output_bytes,
    );
    let response = respond_inner(routes, jar, request, true).await;
    upload_response(response, maximum).await
}
async fn upload_response(response: Response, maximum: usize) -> Response {
    let (mut parts, body) = response.into_parts();
    match axum::body::to_bytes(body, maximum).await {
        Ok(bytes) => Response::from_parts(parts, axum::body::Body::from(bytes)),
        Err(_) => {
            let replacement = bounded_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({"inspection_required":true,"selection":"request"}),
                maximum,
            );
            let (mut replacement_parts, body) = replacement.into_parts();
            replacement_parts.headers.insert(
                "x-graphit-inspection-required",
                header::HeaderValue::from_static("request-selection"),
            );
            parts.status = replacement_parts.status;
            parts.headers.remove(header::CONTENT_LENGTH);
            parts.headers.extend(replacement_parts.headers);
            Response::from_parts(parts, body)
        }
    }
}
async fn respond_inner(
    routes: Routes,
    jar: CookieJar,
    request: Request,
    received: bool,
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
            .is_none_or(|value| {
                value.split(';').next().map(str::trim)
                    != Some(if received {
                        "multipart/form-data"
                    } else {
                        "application/json"
                    })
            })
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
    let query = if received {
        Submission::Received(request)
    } else {
        let body = tokio::select! {body=Bytes::from_request(request,&())=>match body{Ok(bytes)=>bytes,Err(error)=>return error.into_response()},()=entry.cancelled.cancelled()=>return Failure::Unauthorized.into_response(),()=shared.stop.cancelled()=>return Failure::Unavailable.into_response(),()=stop.cancelled()=>return Failure::Unavailable.into_response(),()=tokio::time::sleep_until(deadline.into())=>return Failure::Broker(BrokerError::Deadline).into_response()};
        let query = match Query::decode(&body) {
            Ok(query) => query,
            Err(error) => return finish(Err(Error::Effects(error)), None, maximum),
        };
        if let Err(error) = query.validate(&routes.source.policy().native_install.hub) {
            return finish(Err(Error::Effects(error)), None, maximum);
        }
        Submission::Resources(query)
    };
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
    fn receipt() -> crate::file_publication::Receipt {
        crate::file_publication::Receipt {
            producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            artifact_type: graphit_hub::ArtifactType::Skill,
            id: "fixture/published".into(),
            version: "release/one".into(),
            archive_sha256: "ab".repeat(32),
            payload: "published",
            catalog: "published",
        }
    }
    #[tokio::test]
    async fn late_session_and_kernel_failures_retain_actual_publication_effects() {
        let published =
            json!({"status":"published","publication":receipt(),"project_metadata":"published"});
        let response = finish(Ok(published.clone()), Some(Failure::Unauthorized), 8192);
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body(response).await["outcome"], published);
        let cause = io::Error::new(
            io::ErrorKind::TimedOut,
            crate::file_publication::Failure {
                receipt: receipt(),
                stage: "delivery",
                cause: io::Error::other("private failure evidence"),
            },
        );
        let cause = crate::hub_project_metadata::artifact_failure("published", cause);
        let response = finish(Err(Error::Effects(cause)), None, 8192);
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        let result = body(response).await;
        assert_eq!(result["outcome"]["project_metadata"], "published");
        assert_eq!(result["outcome"]["stage"], "delivery");
        assert_eq!(result["outcome"]["publication"]["payload"], "published");
        assert_eq!(result["outcome"]["publication"]["catalog"], "published");
        assert!(!result.to_string().contains("private failure evidence"));
    }
    #[tokio::test]
    async fn upload_early_error_quotas_keep_session_clear_and_inspection_headers() {
        for maximum in [0, 1, 32, 64] {
            let response = Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(header::SET_COOKIE, "graphit-session=; Max-Age=0; HttpOnly")
                .header(header::CACHE_CONTROL, "private, no-store")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    json!({"error":"x".repeat(256)}).to_string(),
                ))
                .unwrap();
            let response = upload_response(response, maximum).await;
            assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
            assert_eq!(
                response.headers()["x-graphit-inspection-required"],
                "request-selection"
            );
            assert!(response.headers().contains_key(header::SET_COOKIE));
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "private, no-store"
            );
            assert!(
                axum::body::to_bytes(response.into_body(), 1024)
                    .await
                    .unwrap()
                    .len()
                    <= maximum
            );
        }
    }
    #[tokio::test]
    async fn received_native_failure_keeps_its_actual_descriptor_and_catalog_effects() {
        let receipt = crate::native_publication::PublicationReceipt {
            producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            artifact_type: graphit_hub::ArtifactType::Knowledge,
            id: "fixture/received".into(),
            version: "release/one".into(),
            root_sha256: "ab".repeat(32),
            catalog: "published",
        };
        let cause = io::Error::new(
            io::ErrorKind::TimedOut,
            crate::native_publication::PublicationFailure {
                receipt,
                stage: "delivery",
                cause: io::Error::new(io::ErrorKind::TimedOut, "private native path"),
            },
        );
        let error = crate::hub_project_metadata::artifact_failure("published", cause);
        let response = finish(Err(Error::Effects(error)), None, 8192);
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        let result = body(response).await;
        assert_eq!(result["outcome"]["publication"]["catalog"], "published");
        assert_eq!(
            result["outcome"]["publication"]["artifact_type"],
            "knowledge"
        );
        assert_eq!(result["outcome"]["project_metadata"], "published");
        assert_eq!(result["outcome"]["stage"], "delivery");
        assert!(!result.to_string().contains("private native path"));
    }
    #[tokio::test]
    async fn previous_job_failure_does_not_attribute_unstarted_effects_to_this_request() {
        let previous = body(finish(Err(Error::PreviousWorker), None, 8192)).await;
        assert_eq!(previous["error"]["code"], "previous_job_failed");
        assert_eq!(previous["outcome"]["publication"], "not_attempted");
        assert_eq!(previous["outcome"]["project_metadata"], "not_written");
        let own = body(finish(Err(Error::Worker), None, 8192)).await;
        assert_eq!(own["outcome"]["publication"], "unknown");
        assert_eq!(own["outcome"]["project_metadata"], "unknown");
    }
    #[tokio::test]
    async fn publication_quotas_require_original_selection_inspection_even_without_json() {
        for maximum in [1, 128, 256] {
            let result = json!({"status":"published","project_metadata":"published","publication":receipt()});
            let response = bounded_response(StatusCode::OK, result, maximum);
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
                assert_eq!(
                    serde_json::from_slice::<Value>(&bytes).unwrap()["inspection_required"],
                    true
                );
            }
        }
    }
}
