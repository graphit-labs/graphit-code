//! Owned Foreground jobs keep borrowed Now authority inside each running future.
mod native_refresh;
#[cfg(test)]
mod tests;

use futures_util::{StreamExt, stream::FuturesUnordered};
use graphit_auth::{
    ProjectCapture, ProjectRecordAuthority, ScopeCapture, ScopeInput, ServiceBindingView,
    ServicePurpose, ServiceUseError,
    broker::BrokerError,
    profile::{ProviderKind, ProviderRecord},
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_identity::ProjectId,
    project_paths::ProjectPaths,
    version_context::VersionContext,
};
use graphit_daemon::{
    auth_startup::DaemonAuth,
    broker_inventory::{
        InventoryAdmissionFailure, InventoryCredentialMode, OwnInventoryPublicationFailure,
        publish_maintained_inventory,
    },
    broker_verifiers::{BrokerVerifierCache, BrokerVerifierCacheError},
    now_publisher::{
        NowPublisherError, NowPublisherInput, NowPublisherLimits, start_now_publisher,
    },
    now_subscription::{NowRecoveryCause, NowRemoteCode},
    project_now::ProjectNowLimits,
};
use graphit_http::{
    client::{BoundedClient, ClientPolicy, Limits, NetworkError},
    websocket::{WebSocketClient, WebSocketError, WebSocketFailureKind, WebSocketLimits},
};
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
};
use std::{error::Error as _, io, num::NonZeroUsize, sync::Arc, time::Duration};
use tokio::{sync::oneshot, task::JoinSet};

type Projects = Vec<(ProjectPaths, ProjectId, VersionContext)>;
type Publishers =
    std::collections::HashMap<tokio::task::Id, (ProjectId, VersionContext, Cancellation)>;
struct Plan {
    auth: Arc<DaemonAuth>,
    provider: ProviderRecord,
    verifiers: Option<Arc<BrokerVerifierCache>>,
    policy: HubResourcePolicy,
    scope: ScopeLimits,
    binding: NativeBinding,
    heads: ProjectNowLimits,
    network: Limits,
    websocket: WebSocketLimits,
    wire: NowPublisherLimits,
    allow_loopback_http: bool,
}

pub(super) struct Settings {
    pub policy: HubResourcePolicy,
    pub scope: ScopeLimits,
    pub native: NativeDocumentFileLimits,
    pub binding: NativeBinding,
    pub network: Limits,
    pub allow_loopback_http: bool,
}

pub(super) struct ForegroundNow {
    plan: Arc<Plan>,
    initial_projects: Option<Arc<Projects>>,
    stop: Cancellation,
    jobs: JoinSet<io::Result<()>>,
    projects: Publishers,
    failure: Option<io::Error>,
    refresh_required: bool,
    refresh_failed: bool,
    refresh: Option<native_refresh::NativeRefreshJob>,
    pending_cleanup: Option<graphit_core::auth_document::publication::PendingCleanup>,
}
impl ForegroundNow {
    pub fn new(
        auth: Arc<DaemonAuth>,
        projects: Arc<Projects>,
        provider: ProviderRecord,
        verifiers: Option<Arc<BrokerVerifierCache>>,
        settings: Settings,
    ) -> io::Result<Self> {
        let Settings {
            policy,
            scope,
            native,
            binding,
            network,
            allow_loopback_http,
        } = settings;
        let task = crate::foreground_tasks::task_policy(&policy, scope, native, binding)?;
        let content = policy.content();
        let positive =
            |n| NonZeroUsize::new(n).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput));
        let transport = usize::try_from(policy.limit(ResourceSetting::TransportBytes).value)
            .map_err(io::Error::other)?;
        let heads = ProjectNowLimits {
            task: task.read,
            memory: graphit_records::memory_store::MemoryHeadReadLimits {
                batch_rows: task.read.batch_rows,
                native,
            },
            knowledge: graphit_data::knowledge_index::KnowledgeHeadReadLimits {
                max_slug_bytes: positive(content.max_scalar_bytes)?,
                batch_rows: task.read.batch_rows,
                native,
            },
        };
        Ok(Self {
            plan: Arc::new(Plan {
                auth,
                provider,
                verifiers,
                policy,
                scope,
                binding,
                heads,
                network,
                websocket: WebSocketLimits {
                    frame_bytes: positive(transport)?,
                    message_bytes: positive(transport)?,
                    send_bytes: positive(content.max_input_bytes)?,
                },
                wire: NowPublisherLimits {
                    command_bytes: positive(content.max_input_bytes)?,
                    control_bytes: positive(transport)?,
                },
                allow_loopback_http,
            }),
            initial_projects: Some(projects),
            stop: Cancellation::default(),
            jobs: JoinSet::new(),
            projects: std::collections::HashMap::new(),
            failure: None,
            refresh_required: false,
            refresh_failed: false,
            refresh: None,
            pending_cleanup: None,
        })
    }

    /// Start owned publishers; only their actual publish/welcome changes readiness.
    /// Source observes terminal results through next without blocking HTTP admission.
    pub fn start_jobs(&mut self) -> io::Result<()> {
        if !self.jobs.is_empty() || self.stop.is_cancelled() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let projects = self
            .initial_projects
            .take()
            .ok_or(io::ErrorKind::AlreadyExists)?;
        for selection in projects.iter() {
            drop(Self::spawn(
                &mut self.jobs,
                &mut self.projects,
                Arc::clone(&self.plan),
                selection,
            ));
        }
        Ok(())
    }
    fn spawn(
        jobs: &mut JoinSet<io::Result<()>>,
        projects: &mut Publishers,
        plan: Arc<Plan>,
        selection: &(ProjectPaths, ProjectId, VersionContext),
    ) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        let stop = Cancellation::default();
        let selected = selection.clone();
        let worker_stop = stop.clone();
        let handle = jobs.spawn(async move { run(plan, selected, worker_stop, sender).await });
        projects.insert(handle.id(), (selection.1, selection.2.clone(), stop));
        receiver
    }

    /// Preserve publishers of unchanged siblings and admit only actual new selections.
    pub async fn reconcile(
        &mut self,
        desired: Arc<Projects>,
        original: &RequestContext<'_>,
    ) -> io::Result<()> {
        original.check()?;
        let obsolete: Vec<_> = self
            .projects
            .values()
            .filter(|(id, version, _)| {
                !desired
                    .iter()
                    .any(|(_, desired_id, selected)| desired_id == id && selected == version)
            })
            .map(|(id, version, _)| (*id, version.clone()))
            .collect();
        for (id, version) in obsolete {
            self.stop_checkout(id, version.scope(), Some(&version))
                .await?;
            original.check()?;
        }
        let mut ready = FuturesUnordered::new();
        let mut admitted = Vec::new();
        for selection in desired.iter() {
            if !self
                .projects
                .values()
                .any(|(id, version, _)| *id == selection.1 && version == &selection.2)
            {
                original.check()?;
                ready.push(Self::spawn(
                    &mut self.jobs,
                    &mut self.projects,
                    Arc::clone(&self.plan),
                    selection,
                ));
                admitted.push((selection.1, selection.2.clone()));
            }
        }
        let deadline = original.snapshot().required_deadline()?;
        let result = async {
            while !ready.is_empty() {
                tokio::select! {
                    biased;
                    () = original.cancellation().cancelled() => return Err(io::ErrorKind::Interrupted.into()),
                    () = tokio::time::sleep_until(deadline.into()) => return Err(io::ErrorKind::TimedOut.into()),
                    result = ready.next() => match result {
                        Some(Ok(())) => (),
                        Some(Err(_)) => return self.next().await,
                        None => break,
                    },
                    result = self.next() => return result,
                }
            }
            original.check()
        }.await;
        if result.is_err() {
            // Cancel all new publishers before awaiting any one of them. Existing siblings remain live.
            for (id, version, stop) in self.projects.values() {
                if admitted
                    .iter()
                    .any(|(new_id, new_version)| new_id == id && new_version == version)
                {
                    stop.cancel();
                }
            }
            for (id, version) in admitted {
                if let Err(cleanup) = self
                    .stop_checkout(id, version.scope(), Some(&version))
                    .await
                {
                    self.failure.get_or_insert(cleanup);
                }
            }
        }
        result
    }
    pub fn cancel(&self) {
        self.stop.cancel();
        if let Some(refresh) = &self.refresh {
            refresh.cancel();
        }
        for (_, _, stop) in self.projects.values() {
            stop.cancel();
        }
    }
    pub async fn next(&mut self) -> io::Result<()> {
        if let Some(error) = self.failure.take() {
            self.refresh_required |= Self::needs_native_refresh(&error);
            return Err(error);
        }
        if self.refresh_required {
            return Err(io::Error::other(ServiceUseError::NeedsRefresh));
        }
        if self.jobs.is_empty() {
            self.stop.cancelled().await;
            return Ok(());
        }
        let result = self.jobs.join_next_with_id().await;
        if let Some(result) = &result {
            let task = match result {
                Ok((id, _)) => *id,
                Err(error) => error.id(),
            };
            self.projects.remove(&task);
        }
        let result = observed(
            result.map(|result| result.map(|(_, value)| value)),
            self.stop.is_cancelled(),
        );
        if let Err(error) = &result {
            self.refresh_required |= Self::needs_native_refresh(error);
        }
        result
    }
    /// Retire a publisher only after its owned socket and operation have joined.
    pub async fn stop_checkout(
        &mut self,
        expected: ProjectId,
        path: &std::path::Path,
        selected: Option<&VersionContext>,
    ) -> io::Result<()> {
        for (id, root, stop) in self.projects.values() {
            if *id == expected
                && root.scope() == path
                && selected.is_none_or(|selected| selected == root)
            {
                stop.cancel();
            }
        }
        let mut first = None;
        while self.projects.values().any(|(id, root, _)| {
            *id == expected
                && root.scope() == path
                && selected.is_none_or(|selected| selected == root)
        }) {
            let Some(result) = self.jobs.join_next_with_id().await else {
                return Err(io::Error::other(
                    "Now publisher owner unavailable before retirement",
                ));
            };
            let task = match &result {
                Ok((id, _)) => *id,
                Err(error) => error.id(),
            };
            let retired = self.projects.get(&task).is_some_and(|(id, root, _)| {
                *id == expected
                    && root.scope() == path
                    && selected.is_none_or(|selected| selected == root)
            });
            self.projects.remove(&task);
            let result = result
                .map_err(io::Error::other)
                .and_then(|(_, result)| result);
            if let Err(error) = &result {
                self.refresh_required |= Self::needs_native_refresh(error);
            }
            if retired {
                if let Err(error) = result {
                    first.get_or_insert(error);
                }
            } else {
                self.failure.get_or_insert_with(|| {
                    result.err().unwrap_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::ConnectionAborted,
                            "Now sibling publisher ended",
                        )
                    })
                });
            }
        }
        first.map_or(Ok(()), Err)
    }

    pub(super) fn needs_native_refresh(error: &io::Error) -> bool {
        let Some(cause) = error.get_ref() else {
            return false;
        };
        let admission = match cause.downcast_ref::<NowPublisherError>() {
            Some(NowPublisherError::Admission(admission)) => admission
                .source()
                .and_then(|cause| cause.downcast_ref::<ServiceUseError>()),
            _ => cause.downcast_ref::<ServiceUseError>(),
        };
        matches!(admission, Some(ServiceUseError::NeedsRefresh))
    }

    /// An inventory admission can request rotation only before any request was sent.
    pub(super) fn require_inventory_native_refresh(&mut self, error: &io::Error) -> bool {
        let required = inventory_needs_native_refresh(error);
        self.refresh_required |= required;
        required
    }

    /// Quiesce this provider's channels before one administrative credential rotation.
    /// The retained job survives a dropped waiter; callers must drain this owner.
    pub(super) async fn refresh_native(
        &mut self,
        desired: Arc<Projects>,
        cancellation: &Cancellation,
    ) -> io::Result<()> {
        let original = RequestContext::service(self.plan.policy.content(), cancellation)
            .map_err(io::Error::other)?;
        original.check()?;
        if self.stop.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.refresh_failed {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "native refresh previously failed",
            ));
        }
        if self.refresh_required && self.refresh.is_none() {
            for (_, _, stop) in self.projects.values() {
                stop.cancel();
            }
            let mut first = self
                .failure
                .take()
                .filter(|error| !Self::needs_native_refresh(error));
            while let Some(result) = self.jobs.join_next_with_id().await {
                let task = match &result {
                    Ok((id, _)) => *id,
                    Err(error) => error.id(),
                };
                self.projects.remove(&task);
                let result = result
                    .map_err(io::Error::other)
                    .and_then(|(_, result)| result);
                if let Err(error) = result
                    && !Self::needs_native_refresh(&error)
                {
                    first.get_or_insert(error);
                }
            }
            self.projects.clear();
            if let Some(error) = first {
                return Err(error);
            }
            original.check()?;
            if let Some(proof) = self.pending_cleanup.take()
                && let Err((error, pending)) = native_refresh::cleanup(&self.plan, proof)
            {
                self.pending_cleanup = pending;
                return Err(error);
            }
            self.refresh = Some(native_refresh::NativeRefreshJob::start(
                Arc::clone(&self.plan),
                cancellation.clone(),
            ));
        }
        if let Some(refresh) = self.refresh.as_mut() {
            let completion = refresh.join().await;
            self.refresh.take();
            // Any failed rotation is terminal, including an uncertain remote effect.
            self.refresh_failed = true;
            let completion = completion?;
            self.pending_cleanup = completion.pending;
            completion.result?;
            self.refresh_failed = false;
            self.refresh_required = false;
        }
        original.check()?;
        if self.initial_projects.is_some() {
            return Ok(());
        }
        // Like startup, credential rotation resumes owned background publishers.
        // Their actual publish/welcome establishes readiness; waiting here with a
        // request deadline would terminate HTTP while a channel is reconnecting.
        // Explicit PROJECT changes still use reconcile and its caller's deadline.
        for selection in desired.iter() {
            original.check()?;
            if !self
                .projects
                .values()
                .any(|(id, version, _)| *id == selection.1 && version == &selection.2)
            {
                drop(Self::spawn(
                    &mut self.jobs,
                    &mut self.projects,
                    Arc::clone(&self.plan),
                    selection,
                ));
            }
        }
        Ok(())
    }

    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.cancel();
        let mut first = None;
        while let Some(result) = self.jobs.join_next().await {
            let result = match result {
                Ok(result) => result,
                Err(error) => Err(io::Error::other(error)),
            };
            if let Err(error) = result
                && first.is_none()
            {
                first = Some(error);
            }
        }
        self.projects.clear();
        if let Some(refresh) = self.refresh.as_mut() {
            let completion = refresh.join().await;
            self.refresh.take();
            match completion {
                Ok(completion) => {
                    self.pending_cleanup = completion.pending;
                    if let Err(error) = completion.result {
                        first.get_or_insert(error);
                    }
                }
                Err(error) => {
                    first.get_or_insert(error);
                }
            }
        }
        if let Some(proof) = self.pending_cleanup.take()
            && let Err((error, pending)) = native_refresh::cleanup(&self.plan, proof)
        {
            self.pending_cleanup = pending;
            first.get_or_insert(error);
        }
        first.map_or(Ok(()), Err)
    }
}
impl Drop for ForegroundNow {
    fn drop(&mut self) {
        self.cancel();
        self.jobs.abort_all();
    }
}
fn inventory_needs_native_refresh(error: &io::Error) -> bool {
    matches!(
        error
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<OwnInventoryPublicationFailure>()),
        Some(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Auth(ServiceUseError::NeedsRefresh),
            effect: graphit_http::client::NetworkEffect::NotSent,
        })
    )
}

fn observed(
    result: Option<Result<io::Result<()>, tokio::task::JoinError>>,
    cancelled: bool,
) -> io::Result<()> {
    match result {
        Some(Ok(Err(error))) => Err(error),
        Some(Err(error)) => Err(io::Error::other(error)),
        Some(Ok(Ok(()))) if cancelled => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "Foreground Now job ended",
        )),
    }
}

async fn run(
    plan: Arc<Plan>,
    selection: (ProjectPaths, ProjectId, VersionContext),
    stop: Cancellation,
    ready: oneshot::Sender<()>,
) -> io::Result<()> {
    let mut ready = Some(ready);
    let mut delay = Duration::from_secs(1);
    loop {
        let operation = async {
            let original =
                RequestContext::service(plan.policy.content(), &stop).map_err(io::Error::other)?;
            let (project, id, version) = &selection;
            let captured =
                CapturedPublicationScope::project(project, *id, plan.scope, original.snapshot())?;
            let record = captured.binding()?;
            let caller = {
                let runtime = plan
                    .auth
                    .runtime()
                    .check(original.snapshot())
                    .map_err(io::Error::other)?;
                let raw = std::str::from_utf8(
                    runtime
                        .secret(original.snapshot())
                        .map_err(io::Error::other)?,
                )
                .map_err(io::Error::other)?;
                plan.auth
                    .issuer()
                    .authenticate_runtime(raw, &original)
                    .map_err(io::Error::other)?
            };
            let authority = ProjectRecordAuthority::new(plan.auth.issuer());
            let scope = ScopeInput {
                installation: plan.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &captured,
                        record: &record,
                    },
                    version,
                }),
                providers: &[],
                binding: plan.binding,
                global_version: "",
            };
            let mut selection = plan
                .auth
                .issuer()
                .capture_presence_publish_service(&caller, plan.provider.name())
                .map_err(io::Error::other)?;
            let verifier = if matches!(plan.provider.config().kind, ProviderKind::Broker(_)) {
                Some(
                    plan.verifiers
                        .as_ref()
                        .ok_or(io::ErrorKind::InvalidInput)?
                        .get(&mut selection)
                        .map_err(io::Error::other)?,
                )
            } else {
                None
            };
            let lease = match &verifier {
                Some(verifier) => selection
                    .native_credential(verifier)
                    .await
                    .map_err(io::Error::other)?,
                None => selection.credential().map_err(io::Error::other)?,
            };
            let ServiceBindingView::Broker {
                purpose: ServicePurpose::PresencePublish,
                endpoint,
                ..
            } = lease.binding().map_err(io::Error::other)?
            else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            if ready.is_none() {
                let client = BoundedClient::new(
                    ClientPolicy {
                        origin: endpoint,
                        allow_loopback_http: plan.allow_loopback_http,
                        proxy: None,
                        additional_roots: &[],
                    },
                    plan.network,
                )
                .map_err(io::Error::other)?;
                publish_maintained_inventory(
                    &client,
                    &plan.auth,
                    &caller,
                    if matches!(plan.provider.config().kind, ProviderKind::Broker(_)) {
                        InventoryCredentialMode::BrokerNative(
                            plan.verifiers
                                .as_deref()
                                .ok_or(io::ErrorKind::InvalidInput)?,
                        )
                    } else {
                        InventoryCredentialMode::LocalBrokerKey
                    },
                    &original,
                    usize::try_from(plan.policy.limit(ResourceSetting::ContentInputBytes).value)
                        .map_err(io::Error::other)?,
                )
                .await
                .map_err(|error| {
                    let error = io::Error::other(error);
                    if inventory_needs_native_refresh(&error) {
                        io::Error::other(ServiceUseError::NeedsRefresh)
                    } else {
                        error
                    }
                })?;
            }
            let client = WebSocketClient::new(
                ClientPolicy {
                    origin: endpoint,
                    allow_loopback_http: plan.allow_loopback_http,
                    proxy: None,
                    additional_roots: &[],
                },
                plan.network,
                plan.websocket,
            )
            .map_err(io::Error::other)?;
            let mut publisher = start_now_publisher(
                &client,
                &lease,
                &original,
                NowPublisherInput {
                    authority: &authority,
                    caller: &caller,
                    scope,
                    heads: plan.heads,
                },
                plan.wire,
            )
            .await
            .map_err(io::Error::other)?;
            delay = Duration::from_secs(1);
            if let Some(sender) = ready.take() {
                let _ = sender.send(());
            }
            loop {
                publisher.next().await.map_err(io::Error::other)?;
            }
        };
        let result = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(()),
            result = operation => result,
        };
        if stop.is_cancelled() {
            return Ok(());
        }
        match result {
            Err(error) if reconnectable(&error) => {
                // Readiness is an observation, not a transport-recovery gate. A
                // disconnect before the first welcome needs the same fresh admission.
                // This channel's socket, lease and serialized data have already dropped.
                // The next attempt admits and publishes current state, never replays it.
                eprintln!(
                    "Graphit Now publisher readmission: kind={:?}, publisher={:?}, admission={:?}",
                    error.kind(),
                    error
                        .get_ref()
                        .and_then(|cause| cause.downcast_ref::<NowPublisherError>()),
                    error
                        .get_ref()
                        .and_then(|cause| cause.downcast_ref::<ServiceUseError>())
                );
                tokio::select! {
                    biased;
                    () = stop.cancelled() => return Ok(()),
                    () = tokio::time::sleep(delay) => {},
                }
                delay = (delay * 2).min(Duration::from_secs(30));
            }
            result => return result,
        }
    }
}
fn reconnectable(error: &io::Error) -> bool {
    // A fresh PROJECT capture can hit the same descriptor pressure before
    // Auth has wrapped the physical failure. No authority was established.
    #[cfg(unix)]
    if error.raw_os_error() == Some(rustix::io::Errno::MFILE.raw_os_error()) {
        return true;
    }
    #[cfg(windows)]
    if error.raw_os_error()
        == i32::try_from(windows_sys::Win32::Foundation::ERROR_TOO_MANY_OPEN_FILES).ok()
    {
        return true;
    }
    let Some(cause) = error.get_ref() else {
        return false;
    };
    let mut cause: &(dyn std::error::Error + 'static) = cause;
    if let Some(error) = cause.downcast_ref::<NowPublisherError>() {
        if let NowPublisherError::Admission(admission) = error {
            let Some(admission) = admission.source() else {
                return false;
            };
            cause = admission;
        } else {
            return matches!(
                error,
                NowPublisherError::Closed(1000 | 1001 | 1012)
                    | NowPublisherError::Remote(NowRemoteCode::Closed | NowRemoteCode::Deadline)
                    | NowPublisherError::RecoveryRequired(NowRecoveryCause::Close(1013))
                    | NowPublisherError::Transport(
                        WebSocketError::Closed | WebSocketError::Network(NetworkError::Connection)
                    )
            ) || matches!(error, NowPublisherError::Transport(WebSocketError::Sdk(cause)) if cause.kind() == WebSocketFailureKind::Closed);
        }
    }
    if let Some(error) = cause.downcast_ref::<ServiceUseError>() {
        return matches!(
            error,
            ServiceUseError::Broker(BrokerError::Connection)
                | ServiceUseError::Auth(graphit_auth::AuthError::Io)
                | ServiceUseError::Store(graphit_auth::store::AuthStoreError::Core(
                    graphit_core::auth_document::AuthDocumentError::Io
                ))
        );
    }
    if matches!(
        cause.downcast_ref::<graphit_core::runtime_key::RuntimeKeyError>(),
        Some(graphit_core::runtime_key::RuntimeKeyError::Io)
    ) || matches!(
        cause.downcast_ref::<graphit_auth::AuthError>(),
        Some(graphit_auth::AuthError::Io)
    ) {
        return true;
    }
    matches!(
        cause.downcast_ref::<OwnInventoryPublicationFailure>(),
        Some(OwnInventoryPublicationFailure::Admission {
            source: InventoryAdmissionFailure::Auth(
                ServiceUseError::Auth(graphit_auth::AuthError::Io)
                    | ServiceUseError::Store(graphit_auth::store::AuthStoreError::Core(
                        graphit_core::auth_document::AuthDocumentError::Io
                    ))
            ),
            effect: graphit_http::client::NetworkEffect::NotSent,
        }) | Some(
            OwnInventoryPublicationFailure::Network {
                source: NetworkError::Connection,
                ..
            } | OwnInventoryPublicationFailure::Admission {
                source: InventoryAdmissionFailure::Auth(ServiceUseError::Broker(
                    BrokerError::Connection
                )) | InventoryAdmissionFailure::Verifier(BrokerVerifierCacheError::Broker(
                    BrokerError::Connection
                )),
                ..
            }
        )
    )
}
