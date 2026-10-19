//! Runtime-only profile administration on the daemon's existing listener and issuer.
use crate::auth_cli::{self, Command, CommandResult};
use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use graphit_auth::{
    broker::{
        BrokerPolicy, BrokerRequestContext, BrokerVerifier, JsonLimits, VerifierLimits,
        oauth::{NativeOAuth, OAuthLimits},
    },
    profile::ProviderKind,
    profile_manager::{
        InteractiveLoginConfig, InteractiveLoginLimits, NativeProfileManager,
        ProfileManagerAuthority,
    },
};
use graphit_core::{
    auth_document::publication::{AuthDocumentPublication, PendingCleanup},
    file_lock::Cancellation,
};
use graphit_daemon::auth_startup::DaemonAuth;
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use serde::{Deserialize, Serialize};
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{Mutex, mpsc, oneshot},
    task::JoinSet,
};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub const ROUTE: &str = "/admin/auth";
/// Only checked URLs and redacted observations cross the response boundary.
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    DeviceAuthorization {
        verification_uri: String,
        verification_uri_complete: Option<String>,
        user_code: String,
    },
    AuthorizationUrl {
        url: String,
    },
    Read {
        value: serde_json::Value,
    },
    Publication {
        effect: String,
        failure: Option<String>,
        cleanup_effect: String,
        cleanup_failure: Option<String>,
        pending: bool,
    },
    Failure {
        message: String,
        remote_effect: Option<String>,
    },
}
struct Shared {
    auth: Arc<DaemonAuth>,
    content: ContentLimits,
    timeout: Duration,
    stop: CancellationToken,
    jobs: Mutex<JoinSet<()>>,
    // Actual failed SDK cleanup ownership, never predicted capacity or a flow registry.
    pending: Mutex<Vec<PendingCleanup>>,
}
#[derive(Clone)]
pub struct AdminAuth(Arc<Shared>);
impl AdminAuth {
    pub fn new(
        auth: Arc<DaemonAuth>,
        content: ContentLimits,
        timeout: Duration,
        stop: CancellationToken,
    ) -> Self {
        Self(Arc::new(Shared {
            auth,
            content,
            timeout,
            stop,
            jobs: Mutex::new(JoinSet::new()),
            pending: Mutex::new(Vec::new()),
        }))
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route(ROUTE, post(command))
            .layer(DefaultBodyLimit::max(self.0.content.max_input_bytes))
            .with_state(self.clone())
    }
    /// Drain actual jobs and native publication cleanup before removing the runtime owner.
    pub async fn shutdown(&self) -> io::Result<()> {
        self.0.stop.cancel();
        let mut jobs = self.0.jobs.lock().await;
        let mut failed = false;
        while let Some(result) = jobs.join_next().await {
            failed |= result.is_err();
        }
        drop(jobs);
        let mut pending = self.0.pending.lock().await;
        while let Some(proof) = pending.pop() {
            if let Err((error, proof)) = cleanup(&self.0, proof) {
                if let Some(proof) = proof {
                    pending.push(proof);
                }
                return Err(error);
            }
        }
        if failed {
            Err(io::Error::other(
                "Auth administrative operation failed to join",
            ))
        } else {
            Ok(())
        }
    }
}
fn now() -> io::Result<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)
        .and_then(|d| i64::try_from(d.as_secs()).map_err(io::Error::other))
}
fn cleanup(
    state: &Shared,
    proof: PendingCleanup,
) -> Result<(), (io::Error, Option<PendingCleanup>)> {
    let cancellation = Cancellation::default();
    let original =
        match RequestContext::new(state.content, &cancellation, Instant::now() + state.timeout) {
            Ok(original) => original,
            Err(error) => return Err((io::Error::other(error), Some(proof))),
        };
    let authority = (|| {
        let key = state
            .auth
            .runtime()
            .check(original.snapshot())
            .map_err(io::Error::other)?;
        let raw = std::str::from_utf8(key.secret(original.snapshot()).map_err(io::Error::other)?)
            .map_err(io::Error::other)?;
        let caller = state
            .auth
            .issuer()
            .authenticate_runtime(raw, &original)
            .map_err(io::Error::other)?;
        let session = state
            .auth
            .issuer()
            .bootstrap_admin(&caller)
            .map_err(io::Error::other)?;
        // Execute cleanup in the same genuine caller/parent lifetime.
        Ok((caller, session))
    })();
    let (caller, session) = match authority {
        Ok(v) => v,
        Err(error) => return Err((error, Some(proof))),
    };
    let parent = match state.auth.issuer().admin_parent(&caller, &session) {
        Ok(parent) => parent,
        Err(error) => return Err((io::Error::other(error), Some(proof))),
    };
    let manager = NativeProfileManager::new(state.auth.issuer());
    match manager.cleanup_pending(ProfileManagerAuthority::Admin(&parent), proof) {
        Ok(outcome) => {
            if let Some(error) = outcome.failure() {
                let error = io::Error::other(error);
                return Err((error, outcome.into_pending_cleanup()));
            }
            Ok(())
        }
        Err(error) => Err((
            io::Error::other(error.error()),
            Some(error.into_pending_cleanup()),
        )),
    }
}
async fn publication(state: &Shared, outcome: AuthDocumentPublication) -> Event {
    let effect = format!("{:?}", outcome.effect());
    let failure = outcome.failure().map(|e| format!("{e:?}"));
    let cleanup_effect = format!("{:?}", outcome.cleanup_effect());
    let cleanup_failure = outcome.cleanup_failure().map(|e| format!("{e:?}"));
    let pending = outcome.into_pending_cleanup();
    let event = Event::Publication {
        effect,
        failure,
        cleanup_effect,
        cleanup_failure,
        pending: pending.is_some(),
    };
    if let Some(proof) = pending
        && let Err((_, Some(proof))) = cleanup(state, proof)
    {
        state.pending.lock().await.push(proof);
    }
    event
}
pub(crate) fn oauth_limits(content: ContentLimits) -> OAuthLimits {
    OAuthLimits {
        authorization_url_bytes: content.max_scalar_bytes,
        callback_bytes: content.max_input_bytes,
        callback_query_entries: content.max_collection_entries,
        form_body_bytes: content.max_input_bytes,
        token_bytes: content.max_scalar_bytes,
        response_nodes: content.max_nodes,
        response_scalar_bytes: content.max_scalar_bytes,
    }
}
pub(crate) fn verifier_limits(content: ContentLimits) -> VerifierLimits {
    VerifierLimits {
        token_bytes: content.max_scalar_bytes,
        json: JsonLimits {
            bytes: content.max_input_bytes,
            nodes: content.max_nodes,
            depth: content.max_depth,
            scalar_bytes: content.max_scalar_bytes,
            collection_entries: content.max_collection_entries,
        },
        max_keys: content.max_collection_entries,
        metadata_ttl: Duration::from_secs(300),
        key_ttl: Duration::from_secs(300),
        unknown_key_cooldown: Duration::from_secs(1),
    }
}
pub(crate) fn verifier(
    content: ContentLimits,
    service: &graphit_auth::profile::BrokerService,
    revision: u64,
) -> io::Result<BrokerVerifier> {
    BrokerVerifier::new(
        BrokerPolicy {
            endpoint: &service.endpoint,
            mcp_resource: service.mcp_resource.as_deref(),
            provider_revision: revision,
            network: ClientPolicy {
                origin: &service.endpoint,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
        NetworkLimits {
            request_body_bytes: content.max_input_bytes,
            response_body_bytes: content.max_input_bytes,
            url_bytes: content.max_scalar_bytes,
            header_bytes: content.max_input_bytes,
            header_count: content.max_collection_entries,
            max_in_flight: 1,
            max_idle_per_host: 1,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 1,
            max_certificate_bytes: content.max_input_bytes,
        },
        verifier_limits(content),
    )
    .map_err(io::Error::other)
}
async fn execute(
    state: &Shared,
    command: Command,
    raw: &str,
    original: &RequestContext<'_>,
    send: &mpsc::Sender<Event>,
    accepted: oneshot::Sender<StatusCode>,
) -> io::Result<Event> {
    let caller = match state.auth.issuer().authenticate_runtime(raw, original) {
        Ok(caller) => caller,
        Err(error) => {
            let _ = accepted.send(StatusCode::UNAUTHORIZED);
            return Err(io::Error::other(error));
        }
    };
    let session = match state.auth.issuer().bootstrap_admin(&caller) {
        Ok(session) => session,
        Err(error) => {
            let _ = accepted.send(StatusCode::FORBIDDEN);
            return Err(io::Error::other(error));
        }
    };
    let parent = match state.auth.issuer().admin_parent(&caller, &session) {
        Ok(parent) => parent,
        Err(error) => {
            let _ = accepted.send(StatusCode::FORBIDDEN);
            return Err(io::Error::other(error));
        }
    };
    let _ = accepted.send(StatusCode::OK);
    let manager = NativeProfileManager::new(state.auth.issuer());
    let authority = || ProfileManagerAuthority::Admin(&parent);
    let read = match &command {
        Command::Provider(input) => matches!(
            &input.command,
            auth_cli::ProviderCommand::List | auth_cli::ProviderCommand::Show { .. }
        ),
        Command::Account(input) => matches!(
            &input.command,
            auth_cli::AccountCommand::List | auth_cli::AccountCommand::Show { .. }
        ),
        _ => false,
    };
    if !read {
        let recovered = manager
            .recover_stage(authority())
            .map_err(io::Error::other)?;
        if let Some(error) = recovered.failure() {
            let effect = recovered.effect();
            if let Some(proof) = recovered.into_pending_cleanup() {
                state.pending.lock().await.push(proof);
            }
            return Ok(Event::Failure {
                message: format!("authentication recovery: effect={effect:?}; {error}"),
                remote_effect: None,
            });
        }
    }
    let selected = match &command {
        Command::Login(input) => Some(input.provider.clone()),
        Command::Refresh(_) => {
            let snapshot = manager.begin(authority()).map_err(io::Error::other)?;
            let active = snapshot
                .active_name()
                .ok_or_else(|| io::Error::other("no active profile"))?;
            Some(
                snapshot
                    .profiles()
                    .find(|p| p.name() == active)
                    .ok_or_else(|| io::Error::other("active profile not found"))?
                    .provider()
                    .to_owned(),
            )
        }
        _ => None,
    };
    let broker = if let Some(name) = selected {
        let snapshot = manager.begin(authority()).map_err(io::Error::other)?;
        let provider = snapshot
            .providers()
            .find(|p| p.name() == name)
            .ok_or_else(|| io::Error::other("provider not found"))?;
        match &provider.config().kind {
            ProviderKind::Broker(service) => Some((service.clone(), provider.revision())),
            _ => None,
        }
    } else {
        None
    };
    if let Some((service, revision)) = broker {
        let verifier = verifier(state.content, &service, revision)?;
        let content = state.content;
        let limits = oauth_limits(content);
        let oauth = NativeOAuth::new(&verifier, limits).map_err(io::Error::other)?;
        let result = match command {
            Command::Login(input) if input.device => {
                let announcement = send.clone();
                auth_cli::device::login(
                    &input,
                    &manager,
                    authority(),
                    &oauth,
                    move |prompt| {
                        announcement
                            .try_send(Event::DeviceAuthorization {
                                verification_uri: prompt.verification_uri().to_owned(),
                                verification_uri_complete: prompt
                                    .verification_uri_complete()
                                    .map(str::to_owned),
                                user_code: prompt.user_code().to_owned(),
                            })
                            .map_err(io::Error::other)
                    },
                    now,
                )
                .await
            }
            Command::Login(input) => {
                let metadata = verifier
                    .metadata(BrokerRequestContext::new(original.snapshot()))
                    .await
                    .map_err(io::Error::other)?;
                auth_cli::broker::login(
                    &input,
                    &manager,
                    authority(),
                    &oauth,
                    InteractiveLoginConfig {
                        bind: "127.0.0.1:0".parse().map_err(io::Error::other)?,
                        callback_path: metadata.redirect_path(),
                        limits: InteractiveLoginLimits {
                            head_bytes: content.max_input_bytes,
                            header_entries: content.max_collection_entries,
                            target_bytes: content.max_scalar_bytes,
                            callback_bytes: content.max_input_bytes,
                        },
                    },
                    |url| {
                        send.try_send(Event::AuthorizationUrl {
                            url: url.to_owned(),
                        })
                        .map_err(io::Error::other)
                    },
                    now,
                )
                .await
            }
            Command::Refresh(input) => {
                auth_cli::broker::refresh(&input, &manager, authority(), &oauth, now()?).await
            }
            _ => unreachable!("only login and refresh select a Broker"),
        };
        return match result {
            Ok(p) => Ok(publication(state, p).await),
            Err(e) => Ok(Event::Failure {
                message: e.to_string(),
                remote_effect: e.remote_effect().map(|v| format!("{v:?}")),
            }),
        };
    }
    match auth_cli::execute(command, &manager, authority(), now()?) {
        Ok(CommandResult::Read(value)) => {
            original.snapshot().check()?;
            Ok(Event::Read { value })
        }
        Ok(CommandResult::Publication(p)) => Ok(publication(state, p).await),
        Err(error) => Ok(Event::Failure {
            message: error.to_string(),
            remote_effect: None,
        }),
    }
}
async fn command(
    State(server): State<AdminAuth>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if server.0.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if headers
        .get("x-graphit-auth-origin")
        .and_then(|v| v.to_str().ok())
        != Some("runtime")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(raw) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let raw = Zeroizing::new(raw.to_owned());
    let body = Zeroizing::new(body.to_vec());
    let command = match serde_json::from_slice::<Command>(&body) {
        Ok(command) => command,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let (send, receive) = mpsc::channel(2);
    let (accepted, admission) = oneshot::channel();
    let owner = server.0.clone();
    let mut jobs = owner.jobs.lock().await;
    while jobs.try_join_next().is_some() {}
    if owner.stop.is_cancelled() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let state = owner.clone();
    jobs.spawn(async move {
        let cancel = Cancellation::default();
        let original =
            match RequestContext::new(state.content, &cancel, Instant::now() + state.timeout) {
                Ok(original) => original,
                Err(_) => {
                    let _ = accepted.send(StatusCode::INTERNAL_SERVER_ERROR);
                    return;
                }
            };
        let work = execute(&state, command, &raw, &original, &send, accepted);
        tokio::pin!(work);
        let result = tokio::select! {
            result = &mut work => result,
            () = send.closed() => { cancel.cancel(); work.await },
            () = state.stop.cancelled() => { cancel.cancel(); work.await },
        };
        let event = result.unwrap_or_else(|e| Event::Failure {
            message: e.to_string(),
            remote_effect: None,
        });
        let _ = send.send(event).await;
    });
    drop(jobs);
    let status = admission.await.unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if status != StatusCode::OK {
        return status.into_response();
    }
    let max = server.0.content.max_output_bytes;
    let stream = ReceiverStream::new(receive).map(move |event| {
        let mut bytes = serde_json::to_vec(&event).map_err(io::Error::other)?;
        if bytes.len() > max {
            return Err(io::Error::other("Auth response exceeds output limit"));
        }
        bytes.push(b'\n');
        Ok::<_, io::Error>(bytes)
    });
    let mut response = Body::from_stream(stream).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/x-ndjson".parse().unwrap(),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
