//! Server-owned Web OAuth sessions; PROJECT authorization is a separate admission.
#![doc = include_str!("../../../docs/specs/rust_browser_auth.md")]
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::broker::{
    BrokerError, BrokerRequestContext, BrokerVerifier,
    oauth::{
        NativeError, NativeErrorKind, OAuthLimits, RemoteEffect,
        web::{RegisteredWebClient, WebCredentials, WebOAuth},
    },
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::HashMap,
    io,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{Mutex, oneshot},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;
use url::Url;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod ast;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod daemon;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod hub_authored_resources;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod hub_catalog;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod hub_file_mutations;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod hub_file_publication;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod hub_file_reads;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod hub_remote_catalog;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod hub_unpublish;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
mod live;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod live_catalog;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod live_execute;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) mod live_mcp;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod live_page;
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
mod now;
mod session;
mod task;

/// Existing protocol byte limits and actual flow/session expiration policy.
#[derive(Clone, Copy)]
pub struct BrowserPolicy {
    pub oauth: OAuthLimits,
    pub request_timeout: Duration,
    pub login_timeout: Duration,
    pub session_lifetime: Duration,
    pub refresh_margin: Duration,
    pub allow_loopback_http: bool,
}
struct Callback {
    url: zeroize::Zeroizing<String>,
    reply: oneshot::Sender<Result<String, Failure>>,
}
struct Session {
    provider: usize,
    registered: RegisteredWebClient,
    credentials: Mutex<WebCredentials>,
    expires: Instant,
    cancelled: CancellationToken,
    _expiry: tokio_util::sync::DropGuard,
}
struct Shared {
    providers: Vec<(String, Arc<BrokerVerifier>)>,
    origin: String,
    callback: String,
    secure: bool,
    policy: BrowserPolicy,
    stop: CancellationToken,
    flows: Mutex<HashMap<String, oneshot::Sender<Callback>>>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    jobs: Mutex<JoinSet<()>>,
}
/// Trusted configured providers stay fixed until this owner drains and replaces the service.
#[derive(Clone)]
pub struct BrowserAuth(Arc<Shared>);
impl BrowserAuth {
    pub fn new(
        providers: Vec<(String, Arc<BrokerVerifier>)>,
        canonical_origin: &str,
        policy: BrowserPolicy,
        stop: CancellationToken,
    ) -> io::Result<Self> {
        let url = Url::parse(canonical_origin).map_err(|_| invalid_policy())?;
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"));
        if url.cannot_be_a_base()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || (url.scheme() != "https"
                && !(policy.allow_loopback_http && url.scheme() == "http" && loopback))
            || providers.is_empty()
            || providers.iter().enumerate().any(|(i, (name, _))| {
                name.trim().is_empty() || providers[..i].iter().any(|(other, _)| name == other)
            })
        {
            return Err(invalid_policy());
        }
        for duration in [
            policy.request_timeout,
            policy.login_timeout,
            policy.session_lifetime,
        ] {
            if duration.is_zero()
                || Instant::now().checked_add(duration).is_none()
                || duration.as_secs() > i64::MAX as u64
            {
                return Err(invalid_policy());
            }
        }
        let origin = url.origin().ascii_serialization();
        let callback = format!("{origin}/auth/callback");
        for (_, verifier) in &providers {
            WebOAuth::new(
                verifier,
                policy.oauth,
                &callback,
                policy.allow_loopback_http,
            )
            .map_err(|_| invalid_policy())?;
        }
        Ok(Self(Arc::new(Shared {
            providers,
            origin,
            callback,
            secure: url.scheme() == "https",
            policy,
            stop,
            flows: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            jobs: Mutex::new(JoinSet::new()),
        })))
    }
    /// Only authentication routes. This router never authorizes a PROJECT operation.
    pub fn router(&self) -> Router {
        Router::new()
            .route("/auth/providers", get(providers))
            .route("/auth/login", post(login))
            .route("/auth/callback", get(callback))
            .route("/auth/session", get(session::identity))
            .route("/auth/logout", post(session::logout))
            .layer(DefaultBodyLimit::max(self.0.policy.oauth.callback_bytes))
            .with_state(self.clone())
            .layer(axum::middleware::map_response(private_response_async))
    }
    /// Adds cookie-authenticated Task reads with a fresh Broker PROJECT decision per operation.
    pub fn task_router(
        &self,
        server: crate::task_server::TaskServer,
        backend: Arc<graphit_auth::broker::project_access::BrokerProjectAccess>,
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))] source: Option<
            crate::source_server::SourceServer,
        >,
    ) -> Router {
        let router =
            self.router()
                .merge(task::router(self.clone(), server.clone(), backend.clone()));
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        let router = if let Some(source) = source.as_ref() {
            router
                .merge(hub_file_reads::router(
                    self.clone(),
                    server.clone(),
                    source.clone(),
                    backend.clone(),
                ))
                .merge(hub_file_mutations::router(
                    self.clone(),
                    server.clone(),
                    source.clone(),
                    backend.clone(),
                ))
        } else {
            router
                .merge(unavailable_hub_file_mutations(
                    StatusCode::SERVICE_UNAVAILABLE,
                ))
                .merge(unavailable_hub_file_reads(StatusCode::SERVICE_UNAVAILABLE))
        };
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        let router = if let Some(source) = source.as_ref() {
            router
                .merge(hub_unpublish::router(
                    self.clone(),
                    server.clone(),
                    source.clone(),
                ))
                .merge(hub_file_publication::router(
                    self.clone(),
                    server.clone(),
                    source.clone(),
                    backend.clone(),
                ))
                .merge(hub_authored_resources::router(
                    self.clone(),
                    server.clone(),
                    source.clone(),
                    backend.clone(),
                ))
        } else {
            router
                .merge(unavailable_hub_unpublish(StatusCode::SERVICE_UNAVAILABLE))
                .merge(unavailable_hub_file_publication(
                    StatusCode::SERVICE_UNAVAILABLE,
                ))
                .merge(unavailable_hub_authored_resources(
                    StatusCode::SERVICE_UNAVAILABLE,
                ))
        };
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        let router = router
            .merge(unavailable_hub_unpublish(StatusCode::NOT_IMPLEMENTED))
            .merge(unavailable_hub_file_publication(
                StatusCode::NOT_IMPLEMENTED,
            ))
            .merge(unavailable_hub_authored_resources(
                StatusCode::NOT_IMPLEMENTED,
            ));
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        let router = if let Some(source) = source.as_ref() {
            router
                .merge(hub_catalog::router(
                    self.clone(),
                    server.clone(),
                    source.clone(),
                ))
                .merge(hub_remote_catalog::router(
                    self.clone(),
                    server.clone(),
                    source.clone(),
                ))
        } else {
            router
                .route(
                    "/api/hub/installed/native",
                    post(|| async {
                        (
                            StatusCode::SERVICE_UNAVAILABLE,
                            Json(json!({"error":{"code":"native_hub_unavailable"}})),
                        )
                    }),
                )
                .route(
                    "/api/hub/remote/projects",
                    post(|| async {
                        (
                            StatusCode::SERVICE_UNAVAILABLE,
                            Json(json!({"error":{"code":"hub_catalog_unavailable"}})),
                        )
                    }),
                )
                .route(
                    "/api/hub/remote/search",
                    post(|| async {
                        (
                            StatusCode::SERVICE_UNAVAILABLE,
                            Json(json!({"error":{"code":"hub_catalog_unavailable"}})),
                        )
                    }),
                )
        };
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        let router = router
            .route(
                "/api/hub/installed/native",
                post(|| async {
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        Json(json!({"error":{"code":"native_hub_unavailable"}})),
                    )
                }),
            )
            .route(
                "/api/hub/remote/projects",
                post(|| async {
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        Json(json!({"error":{"code":"hub_catalog_unavailable"}})),
                    )
                }),
            )
            .route(
                "/api/hub/remote/search",
                post(|| async {
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        Json(json!({"error":{"code":"hub_catalog_unavailable"}})),
                    )
                }),
            );
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        let router = router
            .merge(unavailable_hub_file_mutations(StatusCode::NOT_IMPLEMENTED))
            .merge(unavailable_hub_file_reads(StatusCode::NOT_IMPLEMENTED));
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        let router = router
            .merge(live::router(self.clone(), server.clone()))
            .merge(live_catalog::router(
                self.clone(),
                server.clone(),
                backend.clone(),
                source,
            ))
            .merge(live_page::router(self.clone(), server, backend));
        router
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn live_execution_router(
        &self,
        server: crate::task_server::TaskServer,
        backend: Arc<graphit_auth::broker::project_access::BrokerProjectAccess>,
        address: std::net::SocketAddr,
        executable: std::path::PathBuf,
        source: Option<crate::source_server::SourceServer>,
    ) -> Router {
        live_execute::router(self.clone(), server, backend, address, executable, source)
    }
    /// Now retains the same Web session; the Broker separately admits PresenceRead.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn now_router(&self, server: crate::source_server::SourceServer) -> Router {
        match server.now_service() {
            Some(now) => now::router(self.clone(), now),
            None => Router::new(),
        }
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn daemon_router(&self, source: crate::source_server::SourceServer) -> Router {
        daemon::router(self.clone(), source)
    }

    /// AST uses the same Web sessions with its own ReadAst PROJECT admission.
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    pub(crate) fn ast_router(
        &self,
        server: crate::source_server::SourceServer,
        backend: Arc<graphit_auth::broker::project_access::BrokerProjectAccess>,
    ) -> Router {
        ast::router(self.clone(), server, backend)
    }
    /// Stops new work, joins owned login/read jobs and destroys all in-memory credentials.
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        self.0.flows.lock().await.clear();
        let mut jobs = self.0.jobs.lock().await;
        let mut failed = false;
        while let Some(result) = jobs.join_next().await {
            failed |= result.is_err();
        }
        let mut sessions = self.0.sessions.lock().await;
        for session in sessions.values() {
            session.cancelled.cancel();
        }
        sessions.clear();
        if failed {
            Err(io::Error::other("browser login job failed to join"))
        } else {
            Ok(())
        }
    }
}
fn unavailable_hub_file_reads(status: StatusCode) -> Router {
    let mut router = Router::new();
    for path in ["/api/hub/files/content", "/api/hub/installed/files"] {
        router = router.route(
            path,
            post(move || async move {
                (
                    status,
                    Json(json!({"error":{"code":"file_read_unavailable"}})),
                )
            }),
        );
    }
    router
}
fn unavailable_hub_unpublish(status: StatusCode) -> Router {
    Router::new().route(
        "/api/hub/unpublish",
        post(move || async move {
            (
                status,
                Json(json!({"error":{"code":"hub_unpublish_unavailable"}})),
            )
        }),
    )
}
fn unavailable_hub_authored_resources(status: StatusCode) -> Router {
    let mut router = Router::new();
    for path in [
        "/api/hub/authored/list",
        "/api/hub/authored/review",
        "/api/hub/authored/content",
    ] {
        router = router.route(
            path,
            post(move || async move {
                (
                    status,
                    Json(json!({"error":{"code":"authored_unavailable"}})),
                )
            }),
        );
    }
    router
}
fn unavailable_hub_file_publication(status: StatusCode) -> Router {
    Router::new().route(
        "/api/hub/files/publish",
        post(move || async move {
            (
                status,
                Json(json!({"error":{"code":"file_publication_unavailable"}})),
            )
        }),
    )
}
fn unavailable_hub_file_mutations(status: StatusCode) -> Router {
    let mut router = Router::new();
    for path in [
        "/api/hub/files/install",
        "/api/hub/authored/install",
        "/api/hub/files/update",
        "/api/hub/files/remove",
        "/api/hub/files/recover",
        "/api/hub/files/forget",
    ] {
        router = router.route(
            path,
            post(move || async move {
                (
                    status,
                    Json(json!({"error":{"code":"file_mutation_unavailable"}})),
                )
            }),
        );
    }
    router
}
fn invalid_policy() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "invalid browser OAuth configuration",
    )
}
#[derive(Clone, Copy)]
enum LoginPhase {
    Register,
    Authorize,
}
impl LoginPhase {
    fn name(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Authorize => "authorize",
        }
    }
}
#[derive(Clone, Copy)]
enum Failure {
    Unauthorized,
    Forbidden,
    Invalid,
    Unavailable,
    Native(NativeError),
    Broker(BrokerError),
    Login {
        phase: LoginPhase,
        cause: &'static str,
        error: Option<NativeError>,
    },
}
impl Failure {
    fn login(phase: LoginPhase, error: NativeError, original: &SnapshotContext) -> Self {
        let cause = match original.check() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => "original_cancelled",
            Err(error) if error.kind() == io::ErrorKind::TimedOut => "original_deadline",
            Err(_) => "original_failed",
            Ok(()) => match error.kind() {
                NativeErrorKind::Broker(error) => match error {
                    BrokerError::InvalidPolicy => "broker_invalid_policy",
                    BrokerError::InvalidMetadata => "broker_invalid_metadata",
                    BrokerError::InvalidDocument => "broker_invalid_document",
                    BrokerError::InvalidKeys => "broker_invalid_keys",
                    BrokerError::InvalidToken => "broker_invalid_token",
                    BrokerError::Limit => "broker_limit",
                    BrokerError::Busy => "broker_busy",
                    BrokerError::Cancelled => "broker_cancelled",
                    BrokerError::Deadline => "broker_deadline",
                    BrokerError::Connection => "broker_connection",
                    BrokerError::Transport => "broker_transport",
                },
                NativeErrorKind::InvalidPolicy => "invalid_policy",
                NativeErrorKind::InvalidRedirect => "invalid_redirect",
                NativeErrorKind::InvalidCallback => "invalid_callback",
                NativeErrorKind::AuthorizationRejected => "authorization_rejected",
                NativeErrorKind::InvalidResponse => "invalid_response",
                NativeErrorKind::InvalidRotation => "invalid_rotation",
                NativeErrorKind::DeviceExpired => "device_expired",
                NativeErrorKind::Unsupported => "unsupported",
                NativeErrorKind::Entropy => "entropy",
                NativeErrorKind::Limit => "limit",
                NativeErrorKind::Busy => "busy",
                NativeErrorKind::HttpRejected => "http_rejected",
            },
        };
        Self::login_diagnostic(phase, cause, Some(error))
    }
    fn login_diagnostic(
        phase: LoginPhase,
        cause: &'static str,
        error: Option<NativeError>,
    ) -> Self {
        // Only finite labels enter the journal, even if the HTTP caller has left.
        // Never format the SDK error, request, provider response or authorization URL.
        eprintln!(
            "browser OAuth start failed: phase={} cause={cause}",
            phase.name()
        );
        Self::Login {
            phase,
            cause,
            error,
        }
    }
}
impl From<NativeError> for Failure {
    fn from(error: NativeError) -> Self {
        Self::Native(error)
    }
}
impl From<BrokerError> for Failure {
    fn from(error: BrokerError) -> Self {
        Self::Broker(error)
    }
}
impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let diagnostic = match self {
            Self::Login { phase, cause, .. } => Some((phase.name(), cause)),
            _ => None,
        };
        let (status, category) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "session unavailable"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "origin denied"),
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid authentication request"),
            Self::Unavailable | Self::Login { error: None, .. } => (
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication interrupted",
            ),
            Self::Broker(error) => broker_failure(error),
            Self::Native(error)
            | Self::Login {
                error: Some(error), ..
            } => match error.kind() {
                NativeErrorKind::InvalidCallback => {
                    (StatusCode::BAD_REQUEST, "invalid OAuth callback")
                }
                NativeErrorKind::AuthorizationRejected => {
                    (StatusCode::UNAUTHORIZED, "login denied")
                }
                NativeErrorKind::Broker(error) => broker_failure(error),
                NativeErrorKind::Unsupported => {
                    (StatusCode::BAD_GATEWAY, "provider operation unsupported")
                }
                _ => (StatusCode::BAD_GATEWAY, "OAuth provider operation failed"),
            },
        };
        let mut body = serde_json::json!({"error":category});
        if let Some((phase, cause)) = diagnostic {
            body["phase"] = phase.into();
            body["cause"] = cause.into();
        }
        private_response((status, Json(body)).into_response())
    }
}
fn broker_failure(error: BrokerError) -> (StatusCode, &'static str) {
    match error {
        BrokerError::InvalidToken | BrokerError::InvalidPolicy => {
            (StatusCode::UNAUTHORIZED, "authentication failed")
        }
        BrokerError::Deadline => (StatusCode::GATEWAY_TIMEOUT, "provider deadline exceeded"),
        BrokerError::Cancelled | BrokerError::Busy => (
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication unavailable",
        ),
        _ => (StatusCode::BAD_GATEWAY, "provider verification failed"),
    }
}
async fn private_response_async(response: Response) -> Response {
    private_response(response)
}
fn private_response(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, header::HeaderValue::from_static("no-cache"));
    response
}
impl Shared {
    fn flow(&self, provider: usize) -> Result<WebOAuth<'_>, Failure> {
        Ok(WebOAuth::new(
            &self.providers[provider].1,
            self.policy.oauth,
            &self.callback,
            self.policy.allow_loopback_http,
        )?)
    }
    fn context(&self) -> Result<SnapshotContext, Failure> {
        if self.stop.is_cancelled() {
            return Err(Failure::Unavailable);
        }
        Ok(SnapshotContext::new(
            Instant::now() + self.policy.request_timeout,
            Cancellation::default(),
        ))
    }
    fn origin(&self, headers: &HeaderMap) -> Result<(), Failure> {
        if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(self.origin.as_str()) {
            return Err(Failure::Forbidden);
        }
        if self.stop.is_cancelled() {
            return Err(Failure::Unavailable);
        }
        Ok(())
    }
    fn cookie_name(&self, login: bool) -> &'static str {
        match (self.secure, login) {
            (true, true) => "__Host-graphit_login",
            (true, false) => "__Host-graphit_session",
            (false, true) => "graphit_login",
            (false, false) => "graphit_session",
        }
    }
    fn cookie(&self, login: bool, id: String) -> Cookie<'static> {
        Cookie::build((self.cookie_name(login), id))
            .http_only(true)
            .secure(self.secure)
            .path("/")
            .same_site(if login {
                SameSite::Lax
            } else {
                SameSite::Strict
            })
            .max_age(cookie::time::Duration::seconds(if login {
                self.policy.login_timeout.as_secs() as i64
            } else {
                self.policy.session_lifetime.as_secs() as i64
            }))
            .build()
    }
    fn clear(&self, jar: CookieJar, login: bool) -> CookieJar {
        jar.remove(self.cookie(login, String::new()))
    }
    fn handle(&self, jar: &CookieJar, login: bool) -> Result<String, Failure> {
        let raw = jar
            .get(self.cookie_name(login))
            .ok_or(Failure::Unauthorized)?
            .value();
        if raw.len() != 43
            || URL_SAFE_NO_PAD
                .decode(raw)
                .map_or(true, |bytes| bytes.len() != 32)
        {
            return Err(Failure::Unauthorized);
        }
        Ok(raw.to_owned())
    }
}
fn opaque_handle() -> Result<String, Failure> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| Failure::Unavailable)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    provider: String,
}
async fn login(
    State(service): State<BrowserAuth>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(request): Json<Login>,
) -> Response {
    let shared = &service.0;
    if let Err(error) = shared.origin(&headers) {
        return error.into_response();
    }
    let Some(provider) = shared
        .providers
        .iter()
        .position(|(name, _)| name == &request.provider)
    else {
        return Failure::Invalid.into_response();
    };
    let id = match opaque_handle() {
        Ok(id) => id,
        Err(error) => return error.into_response(),
    };
    let (send, receive) = oneshot::channel();
    if let Ok(old) = shared.handle(&jar, true) {
        shared.flows.lock().await.remove(&old);
    }
    shared.flows.lock().await.insert(id.clone(), send);
    let (started, ready) = oneshot::channel();
    let worker = service.clone();
    let worker_id = id.clone();
    let mut jobs = shared.jobs.lock().await;
    while jobs.try_join_next().is_some() {}
    if shared.stop.is_cancelled() {
        shared.flows.lock().await.remove(&id);
        return Failure::Unavailable.into_response();
    }
    jobs.spawn(async move {
        login_worker(worker, provider, worker_id, receive, started).await;
    });
    drop(jobs);
    match ready.await {
        Ok(Ok(url)) => private_response(
            (
                jar.add(shared.cookie(true, id)),
                Json(serde_json::json!({"authorization_url": url})),
            )
                .into_response(),
        ),
        Ok(Err(error)) => error.into_response(),
        Err(_) => Failure::Unavailable.into_response(),
    }
}
async fn login_worker(
    service: BrowserAuth,
    provider: usize,
    id: String,
    receive: oneshot::Receiver<Callback>,
    started: oneshot::Sender<Result<String, Failure>>,
) {
    let shared = &service.0;
    let cancel = Cancellation::default();
    let deadline = Instant::now() + shared.policy.login_timeout;
    let original = SnapshotContext::new(deadline, cancel.clone());
    let mut started = Some(started);
    let mut phase = LoginPhase::Register;
    let run = async {
        let flow = match shared.flow(provider) {
            Ok(flow) => flow,
            Err(error) => {
                if let Some(started) = started.take() {
                    let _ = started.send(Err(error));
                }
                return;
            }
        };
        let ctx = BrokerRequestContext::new(&original);
        let registered = match flow.register(ctx).await {
            Ok(client) => client,
            Err(error) => {
                let error = Failure::login(LoginPhase::Register, error, &original);
                if let Some(started) = started.take() {
                    let _ = started.send(Err(error));
                }
                return;
            }
        };
        phase = LoginPhase::Authorize;
        let pending = match flow.begin(&registered, ctx).await {
            Ok(pending) => pending,
            Err(error) => {
                let error = Failure::login(LoginPhase::Authorize, error, &original);
                if let Some(started) = started.take() {
                    let _ = started.send(Err(error));
                }
                return;
            }
        };
        let url = match pending.authorization_url(ctx) {
            Ok(url) => url.to_owned(),
            Err(error) => {
                let error = Failure::login(LoginPhase::Authorize, error, &original);
                if let Some(started) = started.take() {
                    let _ = started.send(Err(error));
                }
                return;
            }
        };
        let Some(started) = started.take() else {
            return;
        };
        if started.send(Ok(url)).is_err() {
            return;
        }
        let Ok(callback) = receive.await else {
            return;
        };
        let credentials = async {
            let credentials = flow.exchange(pending, &callback.url, ctx).await?;
            let caller = flow
                .verify_access(&registered, credentials.access_token(), ctx)
                .await?;
            caller.check(&shared.providers[provider].1, ctx)?;
            Ok::<_, Failure>(credentials)
        }
        .await;
        let result = async {
            let credentials = credentials?;
            let session_id = opaque_handle()?;
            let expires = Instant::now() + shared.policy.session_lifetime;
            let expiry = CancellationToken::new();
            let entry = Arc::new(Session { provider, registered, credentials: Mutex::new(credentials), expires, cancelled: expiry.clone(), _expiry: expiry.clone().drop_guard() });
            let mut jobs = shared.jobs.lock().await;
            let mut sessions = shared.sessions.lock().await;
            if shared.stop.is_cancelled() { return Err(Failure::Unavailable); }
            sessions.insert(session_id.clone(), entry);
            let timer_owner = service.clone();
            let timer_id = session_id.clone();
            jobs.spawn(async move {
                tokio::select! {
                    _ = timer_owner.0.stop.cancelled() => {},
                    _ = expiry.cancelled() => {},
                    _ = tokio::time::sleep_until(expires.into()) => { expiry.cancel(); timer_owner.0.sessions.lock().await.remove(&timer_id); },
                }
            });
            Ok(session_id)
        }.await;
        if let Err(Ok(session_id)) = callback.reply.send(result) {
            shared.sessions.lock().await.remove(&session_id);
        }
    };
    let interrupted = tokio::select! {
        _ = shared.stop.cancelled() => { cancel.cancel(); Some("original_cancelled") },
        _ = tokio::time::sleep_until(deadline.into()) => { cancel.cancel(); Some("original_deadline") },
        _ = run => None,
    };
    if let Some(cause) = interrupted
        && let Some(started) = started.take()
    {
        let error = Failure::login_diagnostic(phase, cause, None);
        let _ = started.send(Err(error));
    }
    shared.flows.lock().await.remove(&id);
}

async fn callback(State(service): State<BrowserAuth>, jar: CookieJar, uri: Uri) -> Response {
    let shared = &service.0;
    let id = match shared.handle(&jar, true) {
        Ok(id) => id,
        Err(error) => return error.into_response(),
    };
    let sender = shared.flows.lock().await.remove(&id);
    let jar = shared.clear(jar, true);
    let Some(sender) = sender else {
        return private_response((jar, Failure::Unauthorized).into_response());
    };
    let url = match uri.query() {
        Some(query) if uri.path() == "/auth/callback" => format!("{}?{}", shared.callback, query),
        _ => return private_response((jar, Failure::Invalid).into_response()),
    };
    if url.len() > shared.policy.oauth.callback_bytes {
        return private_response((jar, Failure::Invalid).into_response());
    }
    let (reply, receive) = oneshot::channel();
    if sender
        .send(Callback {
            url: zeroize::Zeroizing::new(url),
            reply,
        })
        .is_err()
    {
        return private_response((jar, Failure::Unauthorized).into_response());
    }
    match receive.await {
        Ok(Ok(session)) => private_response(
            (jar.add(shared.cookie(false, session)), Redirect::to("/")).into_response(),
        ),
        Ok(Err(error)) => private_response((jar, error).into_response()),
        Err(_) => private_response((jar, Failure::Unavailable).into_response()),
    }
}

#[cfg(test)]
pub(crate) mod tests;

async fn providers(State(service): State<BrowserAuth>) -> Response {
    if service.0.stop.is_cancelled() {
        return Failure::Unavailable.into_response();
    }
    let mut providers: Vec<&str> = service
        .0
        .providers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    providers.sort_unstable();
    private_response(Json(serde_json::json!({"providers":providers})).into_response())
}
