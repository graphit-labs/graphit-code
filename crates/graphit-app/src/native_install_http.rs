//! Native installation on the existing daemon owner and listener.
#[cfg(test)]
#[path = "../tests/support/native_install_http_lifecycle.rs"]
mod tests;
use crate::native_install::{self, InstallExecution, InstallRequest, InstallResources};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use graphit_core::{
    config::{ConfigSnapshot, ConfigSource},
    file_io::SnapshotContext,
    file_lock::Cancellation,
    project_paths::InstallationPaths,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_data::LocalGraphPolicy;
use graphit_hub::{
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
    registry::HubRegistry,
};
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};
#[cfg(all(test, target_os = "linux"))]
pub(crate) use tests::auth as auth_fixture;
use tokio::{
    runtime::Handle,
    sync::{Mutex, mpsc},
    task::JoinSet,
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub const ROUTE: &str = "/admin/packages/install";

/// Immutable installation inputs selected by the process owner, excluding PROJECT.
#[derive(Clone)]
pub struct NativeInstallPolicy {
    pub hub: HubResourcePolicy,
    pub graph: LocalGraphPolicy,
    pub timeout: Duration,
}
impl NativeInstallPolicy {
    pub fn capture(config: &ConfigSnapshot, original: &SnapshotContext) -> io::Result<Self> {
        original.check()?;
        let global = ConfigSnapshot::new(
            config.brand(),
            [
                ConfigSource::Distribution,
                ConfigSource::Global,
                ConfigSource::Environment,
                ConfigSource::Command,
            ]
            .into_iter()
            .filter_map(|source| config.layer(source).cloned()),
        )?;
        let hub = HubResourcePolicy::resolve(&global).map_err(io::Error::other)?;
        let graph = LocalGraphPolicy::resolve(&global, original)?;
        let timeout = Duration::from_millis(hub.limit(ResourceSetting::RequestDeadlineMs).value);
        if timeout.is_zero() || Instant::now().checked_add(timeout).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid native installation deadline",
            ));
        }
        original.check()?;
        Ok(Self {
            hub,
            graph,
            timeout,
        })
    }
}
struct Completion {
    execution: InstallExecution,
    delivery: io::Result<()>,
}
struct Shared {
    auth: Arc<DaemonAuth>,
    installation: InstallationPaths,
    registry: HubRegistry,
    policy: NativeInstallPolicy,
    stop: CancellationToken,
    jobs: Mutex<JoinSet<io::Result<Completion>>>,
}
#[derive(Clone)]
pub struct NativeInstallHttp(Arc<Shared>);
impl NativeInstallHttp {
    pub fn new(
        auth: Arc<DaemonAuth>,
        installation: InstallationPaths,
        registry: HubRegistry,
        policy: NativeInstallPolicy,
        stop: CancellationToken,
    ) -> io::Result<Self> {
        let cancel = Cancellation::default();
        let deadline = Instant::now()
            .checked_add(policy.timeout)
            .filter(|_| !policy.timeout.is_zero())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid native installation deadline",
                )
            })?;
        RequestContext::new(policy.hub.content(), &cancel, deadline).map_err(io::Error::other)?;
        if installation.brand() != auth.installation().live.brand()
            || installation.root() != auth.installation().live.owner_root()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "native installation owner mismatch",
            ));
        }
        Ok(Self(Arc::new(Shared {
            auth,
            installation,
            registry,
            policy,
            stop,
            jobs: Mutex::new(JoinSet::new()),
        })))
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route(ROUTE, post(install))
            .layer(DefaultBodyLimit::max(
                self.0.policy.hub.content().max_input_bytes,
            ))
            .with_state(self.clone())
    }
    /// The JoinSet stays owned when a shutdown waiter is cancelled.
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        let mut jobs = self.0.jobs.lock().await;
        let mut failure = None;
        while let Some(result) = jobs.join_next().await {
            if let Err(error) = observe(result) {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}
fn observe(result: Result<io::Result<Completion>, tokio::task::JoinError>) -> io::Result<()> {
    let completion = result.map_err(io::Error::other)??;
    if completion.delivery.is_err() {
        // Fixed local diagnostics preserve known effects without dumping Auth or SDK data.
        eprintln!(
            "native installation delivery failed: payload={:?}, registry={:?}, operation_failure={:?}",
            completion.execution.outcome.payload,
            completion.execution.outcome.registry,
            completion
                .execution
                .outcome
                .failure
                .as_ref()
                .map(|failure| failure.category)
        );
        if completion.execution.cause.is_some() {
            eprintln!("native installation retained a causal SDK failure");
        }
    }
    completion.delivery
}
struct CancelOnDrop(Option<Cancellation>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(cancel) = &self.0 {
            cancel.cancel();
        }
    }
}
async fn install(
    State(server): State<NativeInstallHttp>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Capture these before the owned job mutex or blocking SDK queue.
    let cancel = Cancellation::default();
    let mut guard = CancelOnDrop(Some(cancel.clone()));
    let Some(deadline) = Instant::now().checked_add(server.0.policy.timeout) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if server.0.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if headers.get_all("x-graphit-auth-origin").iter().count() != 1
        || headers
            .get("x-graphit-auth-origin")
            .and_then(|v| v.to_str().ok())
            != Some("runtime")
        || headers.get_all(header::AUTHORIZATION).iter().count() != 1
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(raw) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| {
            !v.is_empty()
                && v.len() <= server.0.policy.hub.content().max_scalar_bytes
                && !v.bytes().any(|b| b.is_ascii_whitespace())
        })
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let credential = Zeroizing::new(raw.to_owned());
    let request = match serde_json::from_slice::<InstallRequest>(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let (send, receive) = mpsc::channel::<Result<Bytes, io::Error>>(1);
    let owner = Arc::clone(&server.0);
    let mut jobs = tokio::select! {
        biased;
        () = owner.stop.cancelled() => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
            return StatusCode::REQUEST_TIMEOUT.into_response();
        },
        jobs = owner.jobs.lock() => jobs,
    };
    while let Some(result) = jobs.try_join_next() {
        let _ = observe(result);
    }
    if owner.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if Instant::now() >= deadline {
        return StatusCode::REQUEST_TIMEOUT.into_response();
    }
    let state = Arc::clone(&owner);
    jobs.spawn(async move {
        let handle = Handle::current();
        let worker_owner = Arc::clone(&state);
        let worker_cancel = cancel.clone();
        let work = tokio::task::spawn_blocking(move || {
            let original = RequestContext::new(worker_owner.policy.hub.content(), &worker_cancel, deadline).map_err(io::Error::other)?;
            Ok::<_, io::Error>(handle.block_on(native_install::execute_install(InstallResources {
                auth: &worker_owner.auth, installation: &worker_owner.installation,
                registry: &worker_owner.registry, policy: &worker_owner.policy.hub,
                graph_policy: worker_owner.policy.graph,
            }, &credential, &request, &original)))
        });
        tokio::pin!(work);
        let mut watch_closed = true;
        let mut watch_stop = true;
        let mut watch_deadline = true;
        let result = loop {
            tokio::select! {
                result = &mut work => break result,
                () = send.closed(), if watch_closed => { cancel.cancel(); watch_closed = false; },
                () = state.stop.cancelled(), if watch_stop => { cancel.cancel(); watch_stop = false; },
                () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)), if watch_deadline => {
                    // Keep deadline causality while continuing to watch client/owner cancellation.
                    watch_deadline = false;
                },
            }
        };
        let execution = match result {
            Ok(Ok(execution)) => execution,
            Ok(Err(error)) => {
                eprintln!("native installation Original construction refused; no installation execution");
                let kind = error.kind();
                let _ = send.send(Err(io::Error::new(kind, "native installation Original construction failed"))).await;
                return Err(error);
            },
            Err(error) => {
                eprintln!("native installation SDK worker failed to join; effects unavailable");
                let _ = send.send(Err(io::Error::other("native installation SDK worker failed to join"))).await;
                return Err(io::Error::other(error));
            },
        };
        let encoded = serde_json::to_vec(&execution.outcome).map_err(io::Error::other)
            .and_then(|bytes| if bytes.len() <= state.policy.hub.content().max_output_bytes {
                Ok(Bytes::from(bytes))
            } else { Err(io::Error::new(io::ErrorKind::InvalidData, "native installation terminal exceeds output bound")) });
        let delivery = match encoded {
            Ok(bytes) => send.send(Ok(bytes)).await.map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error)),
            Err(error) => {
                let kind = error.kind();
                let _ = send.send(Err(io::Error::new(kind, "native installation terminal encoding failed"))).await;
                Err(error)
            },
        };
        Ok(Completion { execution, delivery })
    });
    drop(jobs);
    guard.0.take();
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(ReceiverStream::new(receive)),
    )
        .into_response()
}
