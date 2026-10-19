//! Installation-only offline composition with process-owned cleanup and real Native authority.
use crate::{
    native_install::{self, InstallExecution, InstallOutcome, InstallRequest, InstallResources},
    native_install_http::NativeInstallPolicy,
};
use graphit_auth::{
    AdminSessionPolicy, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits, Permission,
    ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::ProfileLimits,
};
use graphit_core::{
    artifact_store::{ScopeLimits, StoreOwner},
    auth_document::AuthDocumentLimits,
    file_lock::{Cancellation, FileLock},
    installation_bootstrap::BootstrapLimits,
    native_path::MAX_PATH_BYTES,
    project_paths::InstallationPaths,
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner, RuntimeKeyRecovery},
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupError, StartupPolicy, StartupResources};
use graphit_hub::{
    policy::{HubResourcePolicy, RequestContext, ResourceSetting},
    registry::HubRegistry,
};
use std::{
    fmt,
    io::{self, Write},
    sync::Arc,
    time::{Duration, Instant},
};

enum Resources {
    Auth(Box<DaemonAuth>),
    Runtime(Arc<RuntimeKeyOwner>, FileLock),
    Recovery(Box<(RuntimeKeyRecovery, FileLock)>),
}
enum Cause {
    Io(io::Error),
    Startup(StartupError),
}
pub struct InstallEntryFailure {
    cause: Cause,
    cleanup: Option<io::Error>,
    resources: Option<Resources>,
    pub execution: Option<Box<InstallExecution>>,
}
impl fmt::Debug for InstallEntryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl fmt::Display for InstallEntryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("offline native installation failed")?;
        if let Some(execution) = &self.execution {
            write!(
                f,
                " (payload={:?}, registry={:?})",
                execution.outcome.payload, execution.outcome.registry
            )?;
        }
        if self.resources.is_some() {
            f.write_str("; owned runtime cleanup remains pending")?;
        }
        Ok(())
    }
}
impl std::error::Error for InstallEntryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match &self.cause {
            Cause::Io(e) => e,
            Cause::Startup(StartupError::Io(e)) => e,
            Cause::Startup(StartupError::Bootstrap(e) | StartupError::Document(e)) => e,
            Cause::Startup(StartupError::Runtime(e)) => e,
            Cause::Startup(StartupError::Store(e)) => e,
            Cause::Startup(StartupError::Authority(e)) => e,
        })
    }
}
impl From<io::Error> for InstallEntryFailure {
    fn from(cause: io::Error) -> Self {
        Self {
            cause: Cause::Io(cause),
            cleanup: None,
            resources: None,
            execution: None,
        }
    }
}
impl InstallEntryFailure {
    pub fn has_resources(&self) -> bool {
        self.resources.is_some()
    }
    pub fn cleanup_failure(&self) -> Option<&io::Error> {
        self.cleanup.as_ref()
    }
    /// Retries only SDK cleanup of retained resources, never the installation operation.
    pub fn retry_cleanup(&mut self, policy: &HubResourcePolicy) -> Result<(), &io::Error> {
        if let Some(resources) = self.resources.take() {
            match cleanup(resources, policy) {
                Ok(()) => self.cleanup = None,
                Err((resources, cause)) => {
                    self.resources = resources;
                    self.cleanup = Some(cause);
                }
            }
        }
        match &self.cleanup {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
fn number(policy: &HubResourcePolicy, setting: ResourceSetting) -> io::Result<usize> {
    usize::try_from(policy.limit(setting).value).map_err(io::Error::other)
}
fn startup(policy: &HubResourcePolicy) -> io::Result<StartupPolicy> {
    let depth = number(policy, ResourceSetting::SnapshotDepth)?;
    let paths = number(policy, ResourceSetting::SnapshotPathBytes)?;
    let bytes = number(policy, ResourceSetting::ContentInputBytes)?;
    let scalar = number(policy, ResourceSetting::ContentScalarBytes)?;
    let entries = number(policy, ResourceSetting::ContentCollectionEntries)?;
    let source = || SourceAuthorityPolicy {
        ordinary: &[],
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(15 * 60),
            maximum: Duration::from_secs(60 * 60),
        },
    };
    let authority = IssuerAuthorityPolicy::new(
        source(),
        source(),
        IssuerLimits {
            max_permissions: entries,
            max_scope_bytes: paths,
            max_parent_bearer_bytes: scalar,
        },
        ServiceAuthorityInput {
            rules: &[],
            limits: ServiceAdmissionLimits {
                max_rules: entries,
                max_binding_bytes: paths,
            },
            native_refresh_margin: Duration::ZERO,
        },
    )
    .and_then(|policy| {
        policy.with_native_installation_permissions(&[
            Permission::Configure,
            Permission::ManageCache,
            Permission::External(ExternalEffect::PublishedFilesystem),
        ])
    })
    .map_err(io::Error::other)?;
    Ok(StartupPolicy {
        bootstrap: BootstrapLimits::new(MAX_PATH_BYTES, depth).map_err(io::Error::other)?,
        runtime: RuntimeKeyLimits::new(MAX_PATH_BYTES, depth, scalar).map_err(io::Error::other)?,
        scope: ScopeLimits::new(1, MAX_PATH_BYTES, depth)?,
        document: AuthDocumentLimits::new(bytes, MAX_PATH_BYTES, depth, scalar)
            .map_err(io::Error::other)?,
        profiles: ProfileLimits {
            bytes,
            nodes: number(policy, ResourceSetting::ContentNodes)?,
            scalar_bytes: scalar,
            collection_entries: entries,
            providers: entries,
            profiles: entries,
        },
        authority,
        ttl: TtlPolicy {
            ttl: Duration::from_secs(30 * 60),
            maximum: Duration::from_secs(24 * 60 * 60),
            refresh_margin: Duration::from_secs(60),
        },
    })
}
fn cleanup(
    resources: Resources,
    policy: &HubResourcePolicy,
) -> Result<(), (Option<Resources>, io::Error)> {
    let cancel = Cancellation::default();
    let original = match policy.request_context(Instant::now(), &cancel) {
        Ok(original) => original,
        Err(cause) => return Err((Some(resources), io::Error::other(cause))),
    };
    match resources {
        Resources::Auth(auth) => {
            let removed = auth.remove_runtime(original.snapshot());
            if let Some(cause) = removed.failure().or(removed.cleanup_failure()) {
                return Err((Some(Resources::Auth(auth)), io::Error::other(cause)));
            }
            Ok(())
        }
        Resources::Runtime(owner, singleton) => {
            let discarded = owner.discard_pending(original.snapshot());
            if let Some(cause) = discarded.failure().or(discarded.cleanup_failure()) {
                return Err((
                    Some(Resources::Runtime(owner, singleton)),
                    io::Error::other(cause),
                ));
            }
            let removed = owner.remove(original.snapshot());
            if let Some(cause) = removed.failure().or(removed.cleanup_failure()) {
                return Err((
                    Some(Resources::Runtime(owner, singleton)),
                    io::Error::other(cause),
                ));
            }
            Ok(())
        }
        Resources::Recovery(owned) => {
            let (recovery, singleton) = *owned;
            let discarded = recovery.discard(original.snapshot());
            if let Some(cause) = discarded.failure() {
                let resources = discarded
                    .into_recovery()
                    .map(|recovery| Resources::Recovery(Box::new((recovery, singleton))));
                return Err((resources, io::Error::other(cause)));
            }
            Ok(())
        }
    }
}
/// Call only after genuine discovery absence. A singleton race is an error, without replay.
pub async fn execute(
    installation: &InstallationPaths,
    request: &InstallRequest,
    policy: &NativeInstallPolicy,
    original: &RequestContext<'_>,
) -> Result<InstallExecution, InstallEntryFailure> {
    original.snapshot().check()?;
    let auth = match DaemonAuth::start(installation, &[], startup(&policy.hub)?, original) {
        Ok(auth) => auth,
        Err(failure) => {
            let (cause, resources) = failure.into_parts();
            let resources = resources.and_then(|resources| match resources {
                StartupResources::RuntimeOwner { owner, singleton } => {
                    Some(Resources::Runtime(owner, singleton))
                }
                StartupResources::RuntimeStart { outcome, singleton } => outcome
                    .into_recovery()
                    .map(|recovery| Resources::Recovery(Box::new((recovery, singleton)))),
            });
            let mut failure = InstallEntryFailure {
                cause: Cause::Startup(cause),
                cleanup: None,
                resources,
                execution: None,
            };
            let _ = failure.retry_cleanup(&policy.hub);
            return Err(failure);
        }
    };
    let result = async {
        let StoreOwner::Installation { installation_id } = auth.installation().live.owner() else {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        };
        let registry = HubRegistry::global(installation, installation_id, policy.hub.hub())?;
        let checked = auth
            .runtime()
            .check(original.snapshot())
            .map_err(io::Error::other)?;
        let credential = std::str::from_utf8(
            checked
                .secret(original.snapshot())
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
        Ok::<_, io::Error>(
            native_install::execute_install(
                InstallResources {
                    auth: &auth,
                    installation,
                    registry: &registry,
                    policy: &policy.hub,
                    graph_policy: policy.graph,
                },
                credential,
                request,
                original,
            )
            .await,
        )
    }
    .await;
    let cleaned = cleanup(Resources::Auth(Box::new(auth)), &policy.hub);
    match (result, cleaned) {
        (Ok(execution), Ok(())) => Ok(execution),
        (Err(cause), Ok(())) => Err(cause.into()),
        (result, Err((resources, cause))) => {
            let (cause, execution, cleanup) = match result {
                Ok(execution) => (Cause::Io(cause), Some(Box::new(execution)), None),
                Err(original) => (Cause::Io(original), None, Some(cause)),
            };
            Err(InstallEntryFailure {
                cause,
                cleanup,
                resources,
                execution,
            })
        }
    }
}

#[derive(Debug)]
pub struct InstallDeliveryFailure {
    cause: io::Error,
    payload: crate::native_install::PayloadEffect,
    registry: crate::native_install::RegistryEffect,
}
impl fmt::Display for InstallDeliveryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "native installation output failed (payload={:?}, registry={:?})",
            self.payload, self.registry
        )
    }
}
impl std::error::Error for InstallDeliveryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
/// A failed local output retains observed effects; it never rewrites the terminal or repeats work.
pub fn write_outcome(
    outcome: &InstallOutcome,
    maximum: usize,
    original: &RequestContext<'_>,
    output: &mut dyn Write,
) -> Result<(), InstallDeliveryFailure> {
    let report_check = || -> io::Result<()> {
        match original.snapshot().check() {
            Ok(()) => Ok(()),
            Err(error)
                if matches!(
                    (error.kind(), outcome.failure.as_ref().map(|e| e.category)),
                    (
                        io::ErrorKind::Interrupted,
                        Some(crate::native_install::FailureCategory::Cancelled)
                    ) | (
                        io::ErrorKind::TimedOut,
                        Some(crate::native_install::FailureCategory::Deadline)
                    )
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    };
    let result = (|| -> io::Result<()> {
        report_check()?;
        let mut bytes = serde_json::to_vec(outcome).map_err(io::Error::other)?;
        if bytes
            .len()
            .checked_add(1)
            .is_none_or(|length| length > maximum)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "native installation terminal exceeds output limit",
            ));
        }
        bytes.push(b'\n');
        report_check()?;
        output.write_all(&bytes)?;
        output.flush()?;
        Ok(())
    })();
    result.map_err(|cause| InstallDeliveryFailure {
        cause,
        payload: outcome.payload,
        registry: outcome.registry,
    })
}
