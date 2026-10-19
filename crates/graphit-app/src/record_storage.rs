//! Authoritative record tables use the selected provider and Broker-issued scopes.
use crate::native_storage_credentials::{StorageScope, broker_credentials_for};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::{
    AuthError, NativeProfileRevision, RecordStorageAdmission, RecordStorageScope,
    RemoteRecordStorage, S3CredentialView, ServiceBindingView, ServiceCredentialLease, ServiceMode,
    ServiceSelection, ServiceUseError,
    broker::{
        BrokerVerifier,
        oauth::{NativeOAuth, RemoteEffect},
    },
    profile_manager::{BrokerRefreshError, NativeProfileManager, ProfileManagerAuthority},
};
use graphit_daemon::broker_verifiers::BrokerVerifierCache;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    future::Future,
    io,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    provider: String,
    revision: u64,
    standalone: bool,
    identity: [u8; 32],
    scope: String,
}
struct SessionEntry {
    storage: RemoteRecordStorage,
    refresh_at: Option<Instant>,
}
impl SessionEntry {
    fn new(storage: RemoteRecordStorage, margin: Duration) -> Self {
        let now = Instant::now();
        let refresh_at = storage
            .expires_at
            .map(|expiry| expiry - margin.min(expiry.saturating_duration_since(now) / 10));
        Self {
            storage,
            refresh_at,
        }
    }
    fn reusable(&self) -> bool {
        self.storage.check().is_ok() && self.refresh_at.is_none_or(|at| Instant::now() < at)
    }
}
type Session = Arc<tokio::sync::Mutex<Option<SessionEntry>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct RefreshTarget {
    provider: String,
    provider_revision: u64,
    profile: String,
    issuer: String,
    subject: String,
    profile_revision: u64,
    token_revision: u64,
}
impl From<NativeProfileRevision<'_>> for RefreshTarget {
    fn from(value: NativeProfileRevision<'_>) -> Self {
        Self {
            provider: value.provider.into(),
            provider_revision: value.revisions.provider_revision,
            profile: value.profile.into(),
            issuer: value.issuer.into(),
            subject: value.subject.into(),
            profile_revision: value.revisions.profile_revision,
            token_revision: value.revisions.token_revision,
        }
    }
}
struct RefreshRequired {
    target: RefreshTarget,
    verifier: Arc<BrokerVerifier>,
}
impl std::fmt::Debug for RefreshRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Native Records refresh required")
    }
}
impl std::fmt::Display for RefreshRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Native Records refresh required")
    }
}
impl std::error::Error for RefreshRequired {}
#[derive(Default)]
struct NativeRefreshState {
    pending: Option<graphit_core::auth_document::publication::PendingCleanup>,
}

pub(crate) struct RecordStorageResolver {
    client: reqwest::Client,
    verifiers: Option<Arc<BrokerVerifierCache>>,
    allow_loopback_http: bool,
    request_timeout: Duration,
    sessions: Mutex<BTreeMap<Key, Session>>,
    auth: Weak<graphit_daemon::auth_startup::DaemonAuth>,
    content: graphit_hub::native::ContentLimits,
    refresh: tokio::sync::Mutex<NativeRefreshState>,
}
impl RecordStorageResolver {
    pub(crate) fn new(
        client: reqwest::Client,
        verifiers: Option<Arc<BrokerVerifierCache>>,
        allow_loopback_http: bool,
        request_timeout: Duration,
        auth: Weak<graphit_daemon::auth_startup::DaemonAuth>,
        content: graphit_hub::native::ContentLimits,
    ) -> Self {
        Self {
            client,
            verifiers,
            allow_loopback_http,
            request_timeout,
            sessions: Mutex::new(BTreeMap::new()),
            auth,
            content,
            refresh: tokio::sync::Mutex::new(NativeRefreshState::default()),
        }
    }
    async fn open(
        &self,
        admission: RecordStorageAdmission<'_>,
        deadline: Instant,
    ) -> io::Result<Option<RemoteRecordStorage>> {
        admission.recheck().map_err(io::Error::other)?;
        let Some(mut service) = admission.service().map_err(io::Error::other)? else {
            return Ok(None);
        };
        let mode = service.mode().map_err(io::Error::other)?;
        let verifier = if matches!(
            mode,
            ServiceMode::BrokerNative | ServiceMode::BrokerIncoming | ServiceMode::BrokerKey
        ) {
            Some(
                self.verifiers
                    .as_ref()
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotConnected,
                            "record storage Broker discovery is not configured",
                        )
                    })?
                    .get_for_service(&mut service)
                    .map_err(io::Error::other)?,
            )
        } else {
            None
        };
        let lease = if mode == ServiceMode::BrokerNative {
            match service
                .native_credential(verifier.as_deref().ok_or(io::ErrorKind::NotConnected)?)
                .await
            {
                Ok(lease) => lease,
                Err(ServiceUseError::NeedsRefresh) => {
                    return Err(io::Error::other(RefreshRequired {
                        target: service
                            .native_profile_revision()
                            .map_err(io::Error::other)?
                            .into(),
                        verifier: verifier.clone().ok_or(io::ErrorKind::NotConnected)?,
                    }));
                }
                Err(error) => return Err(io::Error::other(error)),
            }
        } else {
            service.credential().map_err(io::Error::other)?
        };
        lease.check().map_err(io::Error::other)?;
        let scope = admission.scope().map_err(io::Error::other)?;
        let (provider, revision) = match lease.binding().map_err(io::Error::other)? {
            ServiceBindingView::Broker {
                provider,
                provider_revision,
                ..
            }
            | ServiceBindingView::StandaloneS3 {
                provider,
                provider_revision,
                ..
            } => (provider.to_owned(), provider_revision),
            _ => return Err(io::ErrorKind::PermissionDenied.into()),
        };
        // A valid descriptor explicitly disabling S3 is the sole Broker-to-local path.
        if let Some(verifier) = &verifier {
            let metadata = verifier
                .metadata(graphit_auth::broker::BrokerRequestContext::new(
                    admission.request().map_err(io::Error::other)?.snapshot(),
                ))
                .await
                .map_err(io::Error::other)?;
            if !metadata.s3_enabled().map_err(io::Error::other)? {
                return Ok(None);
            }
        }
        let identity = identity(&lease, admission)?;
        let user_namespace = admission
            .user_identity()
            .map_err(io::Error::other)?
            .map(|user| user.namespace_key());
        let key = Key {
            provider,
            revision,
            standalone: matches!(
                lease.binding().map_err(io::Error::other)?,
                ServiceBindingView::StandaloneS3 { .. }
            ),
            identity,
            scope: scope_key(scope, user_namespace.as_deref())?,
        };
        let session = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| io::Error::other("record storage session cache poisoned"))?;
            if let Some(session) = sessions.get(&key) {
                Arc::clone(session)
            } else {
                // Only a new scope needs cache maintenance. Active callers keep
                // their Arc; no background polling or per-operation corpus work.
                sessions.retain(|observed, session| {
                    (observed.provider != key.provider
                        || (observed.revision == key.revision
                            && !(key.standalone
                                && observed.standalone
                                && observed.identity != key.identity)))
                        && session.try_lock().map_or(true, |entry| {
                            entry
                                .as_ref()
                                .is_none_or(|entry| entry.storage.check().is_ok())
                        })
                });
                let session = Arc::new(tokio::sync::Mutex::new(None));
                sessions.insert(key, Arc::clone(&session));
                session
            }
        };
        let operation = async {
            let mut cached = session.lock().await;
            lease.check().map_err(io::Error::other)?;
            admission.recheck().map_err(io::Error::other)?;
            if let Some(entry) = cached.as_ref().filter(|entry| entry.reusable()) {
                return Ok(Some(entry.storage.clone()));
            }
            let remote = match lease.binding().map_err(io::Error::other)? {
                ServiceBindingView::Broker { .. } => {
                    let request_scope = match scope {
                        RecordStorageScope::Project { project_id, module } => {
                            StorageScope::Project { project_id, module }
                        }
                        RecordStorageScope::User { installation_id } => StorageScope::User {
                            installation_id: installation_id.as_str(),
                        },
                    };
                    let storage = broker_credentials_for(
                        &lease,
                        request_scope,
                        &self.client,
                        self.allow_loopback_http,
                        deadline,
                        self.request_timeout,
                    )
                    .await
                    .map_err(io::Error::from)?;
                    RemoteRecordStorage {
                        corpus_key: corpus_key(
                            storage.endpoint.as_deref(),
                            &table_uri(&storage.bucket, &storage.prefix)?,
                        ),
                        uri: table_uri(&storage.bucket, &storage.prefix)?,
                        credentials: Arc::clone(storage.store.credentials()),
                        storage_options: storage_options(
                            &storage.region,
                            storage.endpoint.as_deref(),
                            &storage.copy_if_not_exists,
                            self.request_timeout,
                        ),
                        expires_at: Some(storage.expires_at()),
                    }
                }
                ServiceBindingView::StandaloneS3 { .. } => {
                    self.standalone(&lease, admission, scope, deadline).await?
                }
                _ => return Err(io::ErrorKind::PermissionDenied.into()),
            };
            lease.check().map_err(io::Error::other)?;
            admission.recheck().map_err(io::Error::other)?;
            remote.check()?;
            *cached = Some(SessionEntry::new(
                remote.clone(),
                lease.refresh_margin().map_err(io::Error::other)?,
            ));
            Ok(Some(remote))
        };
        operation.await
    }

    async fn renew(
        &self,
        required: &RefreshRequired,
        request: &graphit_hub::policy::RequestContext<'_>,
        deadline: Instant,
        check: impl Fn() -> io::Result<()>,
    ) -> io::Result<()> {
        request.check().map_err(io::Error::other)?;
        let deadline = request
            .snapshot()
            .deadline()
            .map_or(deadline, |outer| outer.min(deadline));
        // Corpus jobs remain continuous. OAuth is bounded by the storage
        // operation already selected by the resolver, using its cancellation.
        let refresh_request = graphit_hub::policy::RequestContext::new(
            self.content,
            request.cancellation(),
            deadline,
        )
        .map_err(io::Error::other)?;
        let request = &refresh_request;
        let mut state = tokio::select! {
            state = self.refresh.lock() => state,
            () = request.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
            () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
        };
        check()?;
        let auth = self.auth.upgrade().ok_or(io::ErrorKind::NotConnected)?;
        let key = auth
            .runtime()
            .check(request.snapshot())
            .map_err(io::Error::other)?;
        let raw = std::str::from_utf8(key.secret(request.snapshot()).map_err(io::Error::other)?)
            .map_err(io::Error::other)?;
        let caller = auth
            .issuer()
            .authenticate_runtime(raw, request)
            .map_err(io::Error::other)?;
        let session = auth
            .issuer()
            .bootstrap_admin(&caller)
            .map_err(io::Error::other)?;
        let parent = auth
            .issuer()
            .admin_parent(&caller, &session)
            .map_err(io::Error::other)?;
        let authority = || ProfileManagerAuthority::Admin(&parent);
        let manager = NativeProfileManager::new(auth.issuer());
        if let Some(proof) = state.pending.take() {
            let cleanup = match manager.cleanup_pending(authority(), proof) {
                Ok(cleanup) => cleanup,
                Err(error) => {
                    state.pending = Some(error.into_pending_cleanup());
                    return Err(io::Error::other("Native Records refresh cleanup denied"));
                }
            };
            let failure = cleanup.failure();
            state.pending = cleanup.into_pending_cleanup();
            if failure.is_some() || state.pending.is_some() {
                return Err(io::Error::other(
                    "Native Records refresh cleanup incomplete",
                ));
            }
        }
        let snapshot = manager.begin(authority()).map_err(io::Error::other)?;
        let active = snapshot
            .active_name()
            .ok_or(AuthError::Denied)
            .map_err(io::Error::other)?;
        let current = snapshot
            .profiles()
            .find(|profile| profile.name() == active)
            .ok_or(AuthError::Denied)
            .map_err(io::Error::other)?;
        let target = &required.target;
        if current.provider() != target.provider
            || current.name() != target.profile
            || current.provider_revision() != target.provider_revision
            || current.broker_identity() != Some((target.issuer.as_str(), target.subject.as_str()))
        {
            return Err(io::Error::other(AuthError::Denied));
        }
        if current.token_revision() != target.token_revision {
            // Another admitted operation already published a rotation. The caller
            // must obtain a fresh service/proof; never reuse the old selection.
            return Ok(());
        }
        if current.revision() != target.profile_revision {
            return Err(io::Error::other(AuthError::Denied));
        }
        let oauth = NativeOAuth::new(
            &required.verifier,
            crate::admin_auth::oauth_limits(self.content),
        )
        .map_err(io::Error::other)?;
        let now = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_secs(),
        )
        .map_err(io::Error::other)?;
        check()?;
        let publication = match manager.refresh_broker(snapshot, &oauth, now).await {
            Ok(publication) => publication,
            Err(error) => {
                // Another rotator may have completed while this manager waited
                // on the issuer gate. Recapture only the same identity and a new
                // token revision; the caller still obtains a fresh checked lease.
                if error.remote_effect() == RemoteEffect::NotSent {
                    let current = manager.begin(authority()).map_err(io::Error::other)?;
                    if current.active_name() == Some(target.profile.as_str())
                        && current.profiles().any(|profile| {
                            profile.name() == target.profile
                                && profile.provider() == target.provider
                                && profile.provider_revision() == target.provider_revision
                                && profile.broker_identity()
                                    == Some((target.issuer.as_str(), target.subject.as_str()))
                                && profile.token_revision() != target.token_revision
                        })
                    {
                        return Ok(());
                    }
                }
                return Err(refresh_error(error));
            }
        };
        let effect = publication.effect();
        let failure = publication.failure();
        let cleanup_failure = publication.cleanup_failure();
        state.pending = publication.into_pending_cleanup();
        if let Some(proof) = state.pending.take() {
            match manager.cleanup_pending(authority(), proof) {
                Ok(cleanup) => {
                    let failed = cleanup.failure().is_some();
                    state.pending = cleanup.into_pending_cleanup();
                    if failed || state.pending.is_some() {
                        return Err(io::Error::other(
                            "Native Records refresh committed with cleanup failure",
                        ));
                    }
                }
                Err(error) => {
                    state.pending = Some(error.into_pending_cleanup());
                    return Err(io::Error::other(
                        "Native Records refresh committed with cleanup denied",
                    ));
                }
            }
        }
        if effect != graphit_core::auth_document::publication::AuthPublicationEffect::Applied
            || failure.is_some()
            || cleanup_failure.is_some()
        {
            return Err(io::Error::other(
                "Native Records refresh publication failed",
            ));
        }
        request.check().map_err(io::Error::other)
    }

    async fn prepare_selection(
        &self,
        mut selection: ServiceSelection<'_>,
        request: &graphit_hub::policy::RequestContext<'_>,
    ) -> io::Result<()> {
        if !Arc::ptr_eq(
            selection.original().map_err(io::Error::other)?.snapshot(),
            request.snapshot(),
        ) {
            return Err(io::Error::other(AuthError::Request));
        }
        if selection.mode().map_err(io::Error::other)? != ServiceMode::BrokerNative {
            return Err(io::Error::other(ServiceUseError::WrongMode));
        }
        let verifier = self
            .verifiers
            .as_ref()
            .ok_or(io::ErrorKind::NotConnected)?
            .get_for_service(&mut selection)
            .map_err(io::Error::other)?;
        match selection.native_credential(&verifier).await {
            Ok(lease) => return lease.check().map_err(io::Error::other),
            Err(ServiceUseError::NeedsRefresh) => (),
            Err(error) => return Err(io::Error::other(error)),
        }
        let required = RefreshRequired {
            target: selection
                .native_profile_revision()
                .map_err(io::Error::other)?
                .into(),
            verifier,
        };
        self.renew(&required, request, self.deadline(request)?, || {
            request.check().map_err(io::Error::other)
        })
        .await
    }

    fn deadline(&self, request: &graphit_hub::policy::RequestContext<'_>) -> io::Result<Instant> {
        request.check().map_err(io::Error::other)?;
        let configured = Instant::now()
            .checked_add(self.request_timeout)
            .filter(|_| !self.request_timeout.is_zero())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid record storage request timeout",
                )
            })?;
        Ok(request
            .snapshot()
            .deadline()
            .map_or(configured, |outer| outer.min(configured)))
    }

    async fn resolve_storage(
        &self,
        admission: RecordStorageAdmission<'_>,
        deadline: Instant,
    ) -> io::Result<Option<RemoteRecordStorage>> {
        match self.open(admission, deadline).await {
            Err(error)
                if error
                    .get_ref()
                    .is_some_and(|cause| cause.is::<RefreshRequired>()) =>
            {
                let required = error
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<RefreshRequired>())
                    .ok_or(io::ErrorKind::Other)?;
                let request = admission.request().map_err(io::Error::other)?;
                self.renew(required, request, deadline, || {
                    admission.recheck().map_err(io::Error::other)
                })
                .await?;
                admission.recheck().map_err(io::Error::other)?;
                self.open(admission, deadline).await
            }
            result => result,
        }
    }

    async fn standalone(
        &self,
        lease: &ServiceCredentialLease<'_>,
        admission: RecordStorageAdmission<'_>,
        scope: RecordStorageScope<'_>,
        deadline: Instant,
    ) -> io::Result<RemoteRecordStorage> {
        let view = lease.standalone_s3().map_err(io::Error::other)?;
        if matches!(view.credential, S3CredentialView::Anonymous) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "record storage requires authenticated S3 credentials",
            ));
        }
        let bucket = view
            .location
            .bucket
            .as_deref()
            .ok_or(io::ErrorKind::InvalidInput)?
            .to_owned();
        let explicit_expiry = match view.credential {
            S3CredentialView::Local(credentials) => credentials
                .expires_at
                .map(|seconds| {
                    let seconds = u64::try_from(seconds).map_err(|_| io::ErrorKind::InvalidData)?;
                    let remaining = UNIX_EPOCH
                        .checked_add(Duration::from_secs(seconds))
                        .ok_or(io::ErrorKind::InvalidData)?
                        .duration_since(SystemTime::now())
                        .map_err(|_| io::ErrorKind::PermissionDenied)?;
                    Instant::now()
                        .checked_add(remaining)
                        .ok_or(io::ErrorKind::InvalidData)
                })
                .transpose()?,
            _ => None,
        };
        let storage = crate::remote_file_install::storage::open(
            lease,
            self.allow_loopback_http,
            deadline,
            self.request_timeout,
        )
        .await?;
        let suffix = match scope {
            RecordStorageScope::Project { project_id, module } => {
                format!("projects/{project_id}/{module}")
            }
            RecordStorageScope::User { installation_id } => {
                let user = admission
                    .user_identity()
                    .map_err(io::Error::other)?
                    .ok_or(io::ErrorKind::PermissionDenied)?;
                format!(
                    "users/{}/{}/memory",
                    installation_id.as_str(),
                    user.namespace_key()
                )
            }
        };
        let prefix = if storage.prefix.is_empty() {
            suffix
        } else {
            format!("{}/{suffix}", storage.prefix)
        };
        Ok(RemoteRecordStorage {
            corpus_key: corpus_key(
                view.location.endpoint.as_deref(),
                &table_uri(&bucket, &prefix)?,
            ),
            uri: table_uri(&bucket, &prefix)?,
            credentials: Arc::clone(storage.store.credentials()),
            storage_options: storage_options(
                &storage.region,
                view.location.endpoint.as_deref(),
                &object_store::aws::S3CopyIfNotExists::Multipart,
                self.request_timeout,
            ),
            expires_at: storage.expires_at.or(explicit_expiry),
        })
    }
}
fn refresh_error(error: BrokerRefreshError) -> io::Error {
    use graphit_auth::{
        broker::{BrokerError, oauth::NativeErrorKind},
        profile_manager::BrokerRefreshErrorKind,
    };
    let kind = match error.kind() {
        BrokerRefreshErrorKind::OAuth(NativeErrorKind::Broker(BrokerError::Cancelled)) => {
            io::ErrorKind::Interrupted
        }
        BrokerRefreshErrorKind::OAuth(NativeErrorKind::Broker(BrokerError::Deadline)) => {
            io::ErrorKind::TimedOut
        }
        _ => io::ErrorKind::PermissionDenied,
    };
    io::Error::new(kind, error)
}

impl graphit_auth::RecordStorageResolver for RecordStorageResolver {
    fn prepare_native<'a>(
        &'a self,
        selection: ServiceSelection<'a>,
        request: &'a graphit_hub::policy::RequestContext<'a>,
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>> {
        Box::pin(self.prepare_selection(selection, request))
    }
    fn resolve<'a>(
        &'a self,
        admission: RecordStorageAdmission<'a>,
    ) -> Pin<Box<dyn Future<Output = io::Result<Option<RemoteRecordStorage>>> + Send + 'a>> {
        Box::pin(async move {
            let request = admission.request().map_err(io::Error::other)?;
            let deadline = self.deadline(request)?;
            let operation = self.resolve_storage(admission, deadline);
            tokio::pin!(operation);
            tokio::select! {
                biased;
                ()=request.cancellation().cancelled()=> {
                    let _ = operation.await;
                    Err(io::Error::new(io::ErrorKind::Interrupted,"record storage request cancelled"))
                },
                ()=tokio::time::sleep_until(deadline.into())=> {
                    request.cancellation().cancel();
                    let _ = operation.await;
                    Err(io::Error::new(io::ErrorKind::TimedOut,"record storage request deadline expired"))
                },
                result=&mut operation=>result,
            }
        })
    }
}

fn scope_key(scope: RecordStorageScope<'_>, user_namespace: Option<&str>) -> io::Result<String> {
    Ok(match scope {
        RecordStorageScope::Project { project_id, module } => {
            format!("project/{project_id}/{module}")
        }
        RecordStorageScope::User { installation_id } => format!(
            "user/{}/{}/memory",
            installation_id.as_str(),
            user_namespace.ok_or(io::ErrorKind::PermissionDenied)?
        ),
    })
}

fn identity(
    lease: &ServiceCredentialLease<'_>,
    admission: RecordStorageAdmission<'_>,
) -> io::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    if let Some(subject) = lease.user_subject().map_err(io::Error::other)? {
        let issuer = subject.issuer().map_err(io::Error::other)?;
        let subject_id = subject.subject().map_err(io::Error::other)?;
        if let Some(user) = admission.user_identity().map_err(io::Error::other)?
            && (user.issuer() != issuer || user.subject() != subject_id)
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        for part in [issuer, subject_id] {
            hash.update((part.len() as u64).to_be_bytes());
            hash.update(part.as_bytes());
        }
        // One subject can hold tokens for different clients/resources/scopes.
        // Reuse the issued session only under that actual, revalidated bearer.
        let bearer = lease
            .credential()
            .map_err(io::Error::other)?
            .ok_or(io::ErrorKind::PermissionDenied)?;
        hash.update((bearer.len() as u64).to_be_bytes());
        hash.update(bearer.as_bytes());
    } else if let Ok(view) = lease.standalone_s3() {
        // USER identity is part of the scope key. This digest identifies the
        // standalone credential itself, so rotating a permanent key evicts its
        // obsolete clients across project and USER scopes without a timer.
        match view.credential {
            S3CredentialView::Local(credentials) => {
                for part in [
                    &credentials.access_key_id,
                    &credentials.secret_access_key,
                    &credentials.session_token,
                ] {
                    if let Some(part) = part {
                        hash.update((part.expose().len() as u64).to_be_bytes());
                        hash.update(part.expose().as_bytes());
                    } else {
                        hash.update(0_u64.to_be_bytes());
                    }
                }
                hash.update(credentials.expires_at.unwrap_or_default().to_be_bytes());
            }
            S3CredentialView::AwsChain { profile } => {
                if let Some(profile) = profile {
                    hash.update(profile.as_bytes());
                }
            }
            S3CredentialView::Anonymous => return Err(io::ErrorKind::PermissionDenied.into()),
        }
    } else {
        let credential = lease
            .credential()
            .map_err(io::Error::other)?
            .ok_or(io::ErrorKind::PermissionDenied)?;
        hash.update(credential.as_bytes());
    }
    Ok(hash.finalize().into())
}
fn storage_options(
    region: &str,
    endpoint: Option<&str>,
    copy: &object_store::aws::S3CopyIfNotExists,
    timeout: Duration,
) -> Arc<std::collections::HashMap<String, String>> {
    // Match the SDK's ordinary path-style AWS endpoint when none was announced.
    // Set both endpoint keys: its service-specific key takes precedence and must
    // not be inherited from the machine's AWS environment or profile.
    let endpoint = endpoint
        .map(str::to_owned)
        .unwrap_or_else(|| format!("https://s3.{region}.amazonaws.com"));
    let allow_http = endpoint.starts_with("http://").to_string();
    Arc::new(std::collections::HashMap::from([
        ("aws_region".into(), region.into()),
        ("aws_endpoint".into(), endpoint.clone()),
        ("aws_endpoint_url_s3".into(), endpoint),
        ("aws_allow_http".into(), allow_http.clone()),
        ("allow_http".into(), allow_http),
        ("aws_skip_signature".into(), "false".into()),
        ("aws_unsigned_payload".into(), "false".into()),
        ("aws_virtual_hosted_style_request".into(), "false".into()),
        ("aws_s3_express".into(), "false".into()),
        ("aws_conditional_put".into(), "etag".into()),
        ("aws_copy_if_not_exists".into(), copy.to_string()),
        ("aws_timeout".into(), format!("{}ms", timeout.as_millis())),
        ("client_retry_timeout".into(), timeout.as_secs().to_string()),
    ]))
}
fn corpus_key(endpoint: Option<&str>, uri: &str) -> String {
    let mut hash = Sha256::new();
    for value in [endpoint.unwrap_or(""), uri] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    URL_SAFE_NO_PAD.encode(hash.finalize())
}
fn table_uri(bucket: &str, prefix: &str) -> io::Result<String> {
    let mut uri =
        url::Url::parse(&format!("s3://{bucket}/")).map_err(|_| io::ErrorKind::InvalidData)?;
    if uri.host_str() != Some(bucket)
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.port().is_some()
        || uri.path() != "/"
        || uri.query().is_some()
        || uri.fragment().is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid S3 bucket",
        ));
    }
    uri.path_segments_mut()
        .map_err(|_| io::ErrorKind::InvalidData)?
        .clear()
        .extend(prefix.split('/').filter(|segment| !segment.is_empty()))
        .push("index.lance");
    Ok(uri.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdk_options_pin_authorized_transport_and_conditional_commits() {
        for (endpoint, expected) in [
            (None, "https://s3.eu-west-1.amazonaws.com"),
            (Some("http://127.0.0.1:9999"), "http://127.0.0.1:9999"),
        ] {
            let options = storage_options(
                "eu-west-1",
                endpoint,
                &object_store::aws::S3CopyIfNotExists::Multipart,
                Duration::from_secs(17),
            );
            assert_eq!(options["aws_endpoint"], expected);
            assert_eq!(options["aws_endpoint_url_s3"], expected);
            let mut effective = lance_io::object_store::StorageOptions(options.as_ref().clone());
            effective.with_env_s3();
            let effective = effective.as_s3_options();
            use object_store::aws::AmazonS3ConfigKey as Key;
            assert_eq!(effective[&Key::S3Endpoint], expected);
            assert_eq!(effective[&Key::SkipSignature], "false");
            assert_eq!(effective[&Key::ConditionalPut], "etag");
            assert_eq!(effective[&Key::Region], "eu-west-1");
        }
    }
    #[test]
    fn table_uri_preserves_literal_unicode_percent_and_segments() {
        let uri = table_uri("record-bucket", "tenant/a%2Fb/ação space/task").unwrap();
        assert_eq!(
            uri,
            "s3://record-bucket/tenant/a%252Fb/a%C3%A7%C3%A3o%20space/task/index.lance"
        );
        assert!(table_uri("other/path", "x").is_err());
        assert!(table_uri("user@other", "x").is_err());
        let table = "s3://record-bucket/module/index.lance";
        assert_eq!(
            corpus_key(Some("https://s3.example"), table),
            corpus_key(Some("https://s3.example"), table)
        );
        assert_ne!(
            corpus_key(Some("https://s3-a.example"), table),
            corpus_key(Some("https://s3-b.example"), table)
        );
        assert_ne!(
            corpus_key(None, table),
            corpus_key(None, "s3://record-bucket/other/index.lance")
        );
    }
    #[test]
    fn cached_session_refreshes_before_issued_expiry_without_inventing_ttl() {
        let expires = Instant::now() + Duration::from_secs(10);
        let storage = RemoteRecordStorage {
            corpus_key: corpus_key(None, "s3://bucket/module/index.lance"),
            uri: "s3://bucket/module/index.lance".into(),
            credentials: Arc::new(object_store::StaticCredentialProvider::new(
                object_store::aws::AwsCredential {
                    key_id: "test".into(),
                    secret_key: "test".into(),
                    token: None,
                },
            )),
            storage_options: storage_options(
                "us-east-1",
                None,
                &object_store::aws::S3CopyIfNotExists::Multipart,
                Duration::from_secs(30),
            ),
            expires_at: Some(expires),
        };
        let entry = SessionEntry::new(storage, Duration::from_secs(30));
        let refresh = entry.refresh_at.unwrap();
        assert!(refresh < expires);
        assert!(refresh >= expires - Duration::from_secs(1));
        assert!(entry.reusable());
        let permanent = SessionEntry::new(
            RemoteRecordStorage {
                expires_at: None,
                ..entry.storage.clone()
            },
            Duration::from_secs(30),
        );
        assert!(permanent.refresh_at.is_none());
        assert!(permanent.reusable());
        let expired = SessionEntry::new(
            RemoteRecordStorage {
                expires_at: Some(Instant::now() - Duration::from_secs(1)),
                ..entry.storage
            },
            Duration::from_secs(30),
        );
        assert!(!expired.reusable());
    }
}
