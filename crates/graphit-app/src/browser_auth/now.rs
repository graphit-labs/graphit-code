//! Cookie-owned Web Now stream. Broker PROJECT proofs and masks remain authoritative.
use super::*;
use axum::body::{Body, Bytes};
use graphit_core::native_path::NativePath;
use graphit_daemon::now_subscription::RemoteNowSnapshot;
use graphit_daemon::now_subscription::{
    NowRemoteCode, NowSubscriptionConfig, NowSubscriptionError, start_web_now_subscription,
};
use graphit_hub::policy::RequestContext;
use serde_json::json;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::SystemTime,
};
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};

#[derive(Clone)]
struct Routes {
    sessions: BrowserAuth,
    now: crate::source_server::now::NowService,
}
pub(super) fn router(sessions: BrowserAuth, now: crate::source_server::now::NowService) -> Router {
    Router::new()
        .route("/api/now/web/subscribe", post(subscribe))
        .route("/api/now/web/contexts", post(contexts))
        .layer(DefaultBodyLimit::max(now.browser_content().max_input_bytes))
        .layer(axum::middleware::map_response(private_response_async))
        .with_state(Routes { sessions, now })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    selection: crate::task_server::request::Selection,
}
#[derive(Serialize)]
struct Selection<'a> {
    project_dir: NativePath,
    project_id: graphit_core::project_identity::ProjectId,
    project_version: &'a str,
}
#[derive(Serialize)]
struct Update<'a, 'b> {
    selection: &'a Selection<'b>,
    snapshot: &'a RemoteNowSnapshot,
}
#[derive(Serialize)]
struct Contexts<'a, 'b> {
    contexts: &'a [Selection<'b>],
    current: Option<&'a Selection<'b>>,
    projects: Vec<crate::live_project_selections::ProjectName>,
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
struct Delivery {
    lines: ReceiverStream<Bytes>,
    guard: CancelOnDrop,
    entry: Arc<Session>,
    stop: CancellationToken,
    access_expires: SystemTime,
    access_deadline: Instant,
}
impl Stream for Delivery {
    type Item = Result<Bytes, io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.entry.cancelled.is_cancelled()
            || self.entry.expires <= Instant::now()
            || self.stop.is_cancelled()
            || self.guard.0.is_cancelled()
            || SystemTime::now() >= self.access_expires
            || Instant::now() >= self.access_deadline
        {
            self.guard.0.cancel();
            self.lines.close();
            return Poll::Ready(None);
        }
        Pin::new(&mut self.lines)
            .poll_next(cx)
            .map(|item| item.map(Ok))
    }
}
fn subscription_failure(error: &NowSubscriptionError) -> (StatusCode, &'static str) {
    match error {
        NowSubscriptionError::WebAuthentication(_)
        | NowSubscriptionError::Remote(NowRemoteCode::Unauthenticated) => {
            (StatusCode::UNAUTHORIZED, "now_authentication_denied")
        }
        NowSubscriptionError::Remote(NowRemoteCode::Forbidden) => {
            (StatusCode::FORBIDDEN, "now_permission_denied")
        }
        NowSubscriptionError::Http { status: 401, .. } => {
            (StatusCode::UNAUTHORIZED, "now_authentication_denied")
        }
        NowSubscriptionError::Http { status: 403, .. } => {
            (StatusCode::FORBIDDEN, "now_permission_denied")
        }
        _ => (StatusCode::BAD_GATEWAY, "now_subscription_failed"),
    }
}
type ReadyResult = Result<(SystemTime, Instant), (StatusCode, &'static str)>;

async fn execute(
    routes: Routes,
    entry: Arc<Session>,
    input: Input,
    cancellation: Cancellation,
    ready: oneshot::Sender<ReadyResult>,
    send: mpsc::Sender<Bytes>,
) {
    let _job_guard = CancelOnDrop(cancellation.clone());
    let shared = &routes.sessions.0;
    let mut ready = Some(ready);
    let result = {
        let work = async {
            let original = RequestContext::service(routes.now.browser_content(), &cancellation)
                .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "now_unavailable"))?;
            let authentication = RequestContext::new(
                routes.now.browser_content(),
                &cancellation,
                Instant::now() + shared.policy.request_timeout,
            )
            .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "now_unavailable"))?;
            let credentials = shared
                .credentials(&entry, authentication.snapshot())
                .await
                .map_err(|_| (StatusCode::UNAUTHORIZED, "now_authentication_denied"))?;
            let access = zeroize::Zeroizing::new(credentials.access_token().to_owned());
            let access_expires = credentials.expires_at();
            let access_deadline = credentials.expires_mono();
            drop(credentials);
            drop(authentication);
            let flow = shared
                .flow(entry.provider)
                .map_err(|_| (StatusCode::UNAUTHORIZED, "now_authentication_denied"))?;
            let (_, verifier) = &shared.providers[entry.provider];
            let caller = flow
                .verify_access(
                    &entry.registered,
                    &access,
                    BrokerRequestContext::new(original.snapshot()),
                )
                .await
                .map_err(|_| (StatusCode::UNAUTHORIZED, "now_authentication_denied"))?;
            let selected = routes
                .now
                .browser_selection(&input.selection, original.snapshot())
                .map_err(|_| (StatusCode::BAD_REQUEST, "now_selection_unavailable"))?;
            let client = routes
                .now
                .browser_client(verifier)
                .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "now_unavailable"))?;
            let limits = routes.now.browser_websocket_limits();
            let mut subscriber = start_web_now_subscription(
                &client,
                &caller,
                verifier,
                &original,
                NowSubscriptionConfig {
                    project_id: selected.1,
                    version: Some(selected.2.version()),
                    instance_id: None,
                    command_bytes: limits.send_bytes,
                    snapshot_bytes: limits.message_bytes,
                },
            )
            .await
            .map_err(|e| subscription_failure(&e))?;
            let first = subscriber
                .next()
                .await
                .map_err(|e| subscription_failure(&e))?;
            let selection = Selection {
                project_dir: NativePath::new(selected.0.root().to_owned())
                    .map_err(|_| (StatusCode::BAD_REQUEST, "now_selection_unavailable"))?,
                project_id: selected.1,
                project_version: selected.2.version(),
            };
            if let Some(ready) = ready.take() {
                let _ = ready.send(Ok((access_expires, access_deadline)));
            }
            let delivery = async {
                let mut pending = Some(first);
                loop {
                    let next = subscriber.next();
                    tokio::pin!(next);
                    loop {
                        tokio::select! {
                            permit = send.reserve(), if pending.is_some() => {
                                let permit = permit.map_err(|_| (StatusCode::SERVICE_UNAVAILABLE,"now_cancelled"))?;
                                let current = routes.now.browser_selection(&input.selection, original.snapshot()).map_err(|_| (StatusCode::FORBIDDEN,"now_selection_retired"))?;
                                if current.0.root() != selected.0.root() || current.1 != selected.1 || current.2 != selected.2 || entry.cancelled.is_cancelled() || entry.expires <= Instant::now() || shared.stop.is_cancelled() {
                                    return Err((StatusCode::UNAUTHORIZED,"now_authentication_denied"));
                                }
                                caller.check(verifier, BrokerRequestContext::new(original.snapshot())).map_err(|_| (StatusCode::UNAUTHORIZED,"now_authentication_denied"))?;
                                let snapshot = pending.take().expect("pending delivery");
                                let mut bytes = routes.now.browser_encode(&Update { selection: &selection, snapshot: &snapshot }).map_err(|_| (StatusCode::SERVICE_UNAVAILABLE,"now_output_limit"))?;
                                bytes.push(b'\n');
                                permit.send(Bytes::from(bytes));
                            }
                            result = &mut next => {
                                pending = Some(result.map_err(|e| subscription_failure(&e))?);
                                break;
                            }
                        }
                    }
                }
            };
            let result: Result<(), (StatusCode, &'static str)> = tokio::select! {
                result = delivery => result,
                () = tokio::time::sleep_until(access_deadline.into()) => Err((StatusCode::UNAUTHORIZED, "now_authentication_denied")),
            };
            let _ = subscriber.shutdown().await;
            result
        };
        tokio::pin!(work);
        tokio::select! {
            result = &mut work => result,
            () = cancellation.cancelled() => return,
            () = shared.stop.cancelled() => return,
            () = entry.cancelled.cancelled() => return,
            () = routes.now.browser_stopped().cancelled() => return,
            () = tokio::time::sleep_until(entry.expires.into()) => return,
        }
    };
    // An ended authenticated stream withdraws its view. No stale snapshot survives.
    if let Err(failure) = result {
        if let Some(ready) = ready.take() {
            let _ = ready.send(Err(failure));
        }
        cancellation.cancel();
    }
}
async fn subscribe(
    State(routes): State<Routes>,
    jar: CookieJar,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let shared = &routes.sessions.0;
    if let Err(error) = shared.origin(&headers) {
        return error.into_response();
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        != Some("application/json")
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let (_, entry) = match shared.session(&jar).await {
        Ok(v) => v,
        Err(error) => return (shared.clear(jar, false), error).into_response(),
    };
    let input = match serde_json::from_slice::<Input>(&body) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let cancellation = Cancellation::default();
    let guard = CancelOnDrop(cancellation.clone());
    let (send, receive) = mpsc::channel(1);
    let (ready, received) = oneshot::channel();
    {
        let mut jobs = shared.jobs.lock().await;
        while jobs.try_join_next().is_some() {}
        if shared.stop.is_cancelled() {
            return Failure::Unavailable.into_response();
        }
        jobs.spawn(execute(
            routes.clone(),
            entry.clone(),
            input,
            cancellation.clone(),
            ready,
            send,
        ));
    }
    let deadline = Instant::now() + shared.policy.request_timeout;
    let result = tokio::select! {
        value=received => value.unwrap_or(Err((StatusCode::BAD_GATEWAY,"now_subscription_failed"))),
        ()=shared.stop.cancelled() => Err((StatusCode::SERVICE_UNAVAILABLE,"now_unavailable")),
        ()=entry.cancelled.cancelled() => Err((StatusCode::UNAUTHORIZED,"now_authentication_denied")),

        () = tokio::time::sleep_until(deadline.into()) => Err((StatusCode::GATEWAY_TIMEOUT,"now_deadline")),
    };
    let (access_expires, access_deadline) = match result {
        Ok(validity) => validity,
        Err((status, code)) => return (status, Json(json!({"error":code}))).into_response(),
    };
    let stream = Delivery {
        lines: ReceiverStream::new(receive),
        guard,
        entry,
        stop: shared.stop.clone(),
        access_expires,
        access_deadline,
    };
    (
        [
            (header::CONTENT_TYPE, "application/x-ndjson"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

async fn contexts(
    State(routes): State<Routes>,
    jar: CookieJar,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let shared = &routes.sessions.0;
    if let Err(error) = shared.origin(&headers) {
        return error.into_response();
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        != Some("application/json")
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    if !serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&body)
        .is_ok_and(|v| v.is_empty())
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let (_, entry) = match shared.session(&jar).await {
        Ok(v) => v,
        Err(error) => return (shared.clear(jar, false), error).into_response(),
    };
    let cancellation = Cancellation::default();
    let _guard = CancelOnDrop(cancellation.clone());
    let deadline = Instant::now() + shared.policy.request_timeout;
    let (send, receive) = oneshot::channel();
    {
        let mut jobs = shared.jobs.lock().await;
        while jobs.try_join_next().is_some() {}
        if shared.stop.is_cancelled() {
            return Failure::Unavailable.into_response();
        }
        let routes = routes.clone();
        let entry = entry.clone();
        let cancellation = cancellation.clone();
        jobs.spawn(async move {
            let work = context_read(&routes, &entry, &cancellation, deadline);
            tokio::pin!(work);
            let result=tokio::select! {
                result=&mut work=>result,
                ()=cancellation.cancelled()=>Err((StatusCode::SERVICE_UNAVAILABLE,"now_cancelled")),
                ()=entry.cancelled.cancelled()=>Err((StatusCode::UNAUTHORIZED,"now_authentication_denied")),
                ()=routes.sessions.0.stop.cancelled()=>Err((StatusCode::SERVICE_UNAVAILABLE,"now_unavailable")),
                ()=routes.now.browser_stopped().cancelled()=>Err((StatusCode::SERVICE_UNAVAILABLE,"now_unavailable")),
                ()=tokio::time::sleep_until(deadline.into())=>Err((StatusCode::GATEWAY_TIMEOUT,"now_deadline")),
            };
            let _=send.send(result);
        });
    }
    let result = tokio::select! { value=receive=>value.unwrap_or(Err((StatusCode::SERVICE_UNAVAILABLE,"now_unavailable"))),()=shared.stop.cancelled()=>Err((StatusCode::SERVICE_UNAVAILABLE,"now_unavailable")),()=entry.cancelled.cancelled()=>Err((StatusCode::UNAUTHORIZED,"now_authentication_denied")),()=tokio::time::sleep_until(deadline.into())=>Err((StatusCode::GATEWAY_TIMEOUT,"now_deadline")) };
    match result {
        Ok(bytes) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
        Err((status, code)) => (status, Json(json!({"error":{"code":code}}))).into_response(),
    }
}

async fn context_read(
    routes: &Routes,
    entry: &Session,
    cancellation: &Cancellation,
    deadline: Instant,
) -> Result<Vec<u8>, (StatusCode, &'static str)> {
    let original = RequestContext::new(routes.now.browser_content(), cancellation, deadline)
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "now_unavailable"))?;
    let credentials = routes
        .sessions
        .0
        .credentials(entry, original.snapshot())
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, "now_authentication_denied"))?;
    let access = zeroize::Zeroizing::new(credentials.access_token().to_owned());
    drop(credentials);
    let flow = routes
        .sessions
        .0
        .flow(entry.provider)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "now_authentication_denied"))?;
    let (_, verifier) = &routes.sessions.0.providers[entry.provider];
    let caller = flow
        .verify_access(
            &entry.registered,
            &access,
            BrokerRequestContext::new(original.snapshot()),
        )
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, "now_authentication_denied"))?;
    let client = routes
        .now
        .browser_client(verifier)
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "now_unavailable"))?;
    let limits = routes.now.browser_websocket_limits();
    let (projects, current) = routes
        .now
        .browser_projects()
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "now_selection_unavailable"))?;
    let mut contexts = Vec::new();
    let mut admitted_indices = Vec::new();
    let mut selected_current = None;
    for (index, (paths, id, version)) in projects.iter().enumerate() {
        let selection = crate::task_server::request::Selection {
            project_dir: Some(
                NativePath::new(paths.root().to_owned())
                    .map_err(|_| (StatusCode::BAD_REQUEST, "now_selection_unavailable"))?,
            ),
            project_id: Some(*id),
            project_version: Some(version.version().to_owned()),
        };
        routes
            .now
            .browser_selection(&selection, original.snapshot())
            .map_err(|_| (StatusCode::BAD_REQUEST, "now_selection_unavailable"))?;
        let result = async {
            let mut subscriber = start_web_now_subscription(
                &client,
                &caller,
                verifier,
                &original,
                NowSubscriptionConfig {
                    project_id: *id,
                    version: Some(version.version()),
                    instance_id: None,
                    command_bytes: limits.send_bytes,
                    snapshot_bytes: limits.message_bytes,
                },
            )
            .await?;
            let snapshot = subscriber.next().await;
            let closed = subscriber.shutdown().await;
            snapshot?;
            closed
        }
        .await;
        match result {
            Ok(()) => {}
            Err(error) if subscription_failure(&error).0 == StatusCode::FORBIDDEN => continue,
            Err(error) => return Err(subscription_failure(&error)),
        }
        if entry.cancelled.is_cancelled()
            || entry.expires <= Instant::now()
            || routes.sessions.0.stop.is_cancelled()
        {
            return Err((StatusCode::UNAUTHORIZED, "now_authentication_denied"));
        }
        let context = Selection {
            project_dir: selection.project_dir.expect("retained native path"),
            project_id: *id,
            project_version: version.version(),
        };
        if Some(index) == current {
            selected_current = Some(contexts.len());
        }
        contexts.push(context);
        admitted_indices.push(index);
    }
    caller
        .check(verifier, BrokerRequestContext::new(original.snapshot()))
        .map_err(|_| (StatusCode::UNAUTHORIZED, "now_authentication_denied"))?;
    let current = selected_current
        .or_else(|| (!contexts.is_empty()).then_some(0))
        .and_then(|index| contexts.get(index));
    let bytes = routes
        .now
        .browser_encode(&Contexts {
            contexts: &contexts,
            current,
            projects: crate::live_project_selections::project_names(
                admitted_indices.iter().map(|index| &projects[*index]),
                original.snapshot(),
            )
            .map_err(|_| (StatusCode::BAD_REQUEST, "now_selection_unavailable"))?,
        })
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "now_output_limit"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        browser_auth::tests::{Fixture, policy},
        source_server::test_fixture as source,
    };
    use graphit_auth::Permission;
    use graphit_core::{brand::Brand, config::ConfigSnapshot, module_settings::Module};
    use tokio_stream::StreamExt;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn expired_delivery_discards_queued_frame_and_joins_cancelled_worker() {
        let mut sources = source::fixture(
            &[
                Permission::Read(Module::Task),
                Permission::ManageAuthProfiles,
            ],
            Brand::default(),
            graphit_hub::NativeBinding::Codex,
        );
        sources
            .server
            .attach_now(
                None,
                &ConfigSnapshot::new(sources.paths.brand(), []).unwrap(),
            )
            .unwrap();
        let fixture = Fixture::new(policy()).await;
        let _cookie = fixture.login().await;
        let entry = fixture
            .service
            .0
            .sessions
            .lock()
            .await
            .values()
            .next()
            .unwrap()
            .clone();
        let selection = crate::task_server::request::Selection {
            project_dir: None,
            project_id: Some(sources.projects[0].1),
            project_version: Some(sources.projects[0].2.version().to_owned()),
        };
        // Keep the actual read worker inside credentials acquisition until delivery cancels it.
        let credentials = entry.credentials.lock().await;
        for (access_expires, access_deadline) in [
            (
                SystemTime::now() - Duration::from_secs(1),
                Instant::now() + Duration::from_secs(60),
            ),
            (
                SystemTime::now() + Duration::from_secs(60),
                Instant::now() - Duration::from_secs(1),
            ),
        ] {
            let cancellation = Cancellation::default();
            let (send, receive) = mpsc::channel(1);
            send.send(Bytes::from_static(b"stale queued frame\n"))
                .await
                .unwrap();
            let (ready, _received) = oneshot::channel();
            let worker = tokio::spawn(execute(
                Routes {
                    sessions: fixture.service.clone(),
                    now: sources.server.now_service().unwrap(),
                },
                entry.clone(),
                Input {
                    selection: crate::task_server::request::Selection {
                        project_dir: None,
                        project_id: selection.project_id,
                        project_version: selection.project_version.clone(),
                    },
                },
                cancellation.clone(),
                ready,
                send,
            ));
            tokio::task::yield_now().await;
            assert!(
                !worker.is_finished(),
                "actual worker waits for the held credential lock"
            );
            let mut body = Delivery {
                lines: ReceiverStream::new(receive),
                guard: CancelOnDrop(cancellation.clone()),
                entry: entry.clone(),
                stop: fixture.service.0.stop.clone(),
                access_expires,
                access_deadline,
            };
            assert!(
                body.next().await.is_none(),
                "expired credentials cannot release a queued frame"
            );
            assert!(cancellation.is_cancelled());
            tokio::time::timeout(Duration::from_secs(1), worker)
                .await
                .expect("cancelled worker joins without token TTL or Broker")
                .expect("worker completion");
        }
        drop(credentials);
        fixture.service.shutdown().await.unwrap();
        sources.server.shutdown().await.unwrap();
    }
}
