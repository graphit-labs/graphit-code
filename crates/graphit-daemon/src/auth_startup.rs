//! One process-owned installation issuer; incoming requests retain their original context.
use graphit_auth::{
    AuthError, AuthenticatedCaller, IssuerAuthorityPolicy, RoleIssuer, ScopeCapture, TtlPolicy,
    profile::ProfileLimits,
    store::{AuthStore, AuthStoreError},
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeLimits},
    auth_document::{AuthDocumentError, AuthDocumentLimits, AuthDocumentStore},
    file_io::SnapshotContext,
    file_lock::FileLock,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{
        RuntimeKeyError, RuntimeKeyLimits, RuntimeKeyMutationOutcome, RuntimeKeyOwner,
        RuntimeKeyStartOutcome,
    },
};
use graphit_hub::policy::RequestContext;
use std::{fmt, io, sync::Arc};

/// Explicit operator policy. This is selected once by service startup, never by incoming data.
pub struct StartupPolicy {
    pub bootstrap: BootstrapLimits,
    pub runtime: RuntimeKeyLimits,
    pub scope: ScopeLimits,
    pub document: AuthDocumentLimits,
    pub profiles: ProfileLimits,
    pub authority: IssuerAuthorityPolicy,
    pub ttl: TtlPolicy,
}

/// Failure categories retain the SDK's actual error rather than interpreting absence as admission.
#[derive(Debug)]
pub enum StartupError {
    Io(io::Error),
    Bootstrap(AuthDocumentError),
    Runtime(RuntimeKeyError),
    Document(AuthDocumentError),
    Store(AuthStoreError),
    Authority(AuthError),
}

/// Resources from failed startup remain owned until the caller completes SDK cleanup.
/// A cancelled cleanup does not release the singleton or imply that the key was removed.
pub enum StartupResources {
    RuntimeStart {
        outcome: Box<RuntimeKeyStartOutcome>,
        singleton: FileLock,
    },
    RuntimeOwner {
        owner: Arc<RuntimeKeyOwner>,
        singleton: FileLock,
    },
}

pub struct AuthStartupFailure {
    cause: StartupError,
    resources: Option<StartupResources>,
}
impl AuthStartupFailure {
    pub fn cause(&self) -> &StartupError {
        &self.cause
    }
    pub fn resources(&self) -> Option<&StartupResources> {
        self.resources.as_ref()
    }
    /// Transfer the original typed cause and owned cleanup resources together.
    pub fn into_parts(self) -> (StartupError, Option<StartupResources>) {
        (self.cause, self.resources)
    }
    pub fn into_resources(self) -> Option<StartupResources> {
        self.resources
    }
    fn before_runtime(cause: StartupError) -> Self {
        Self {
            cause,
            resources: None,
        }
    }
}
impl fmt::Debug for AuthStartupFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthStartupFailure")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for AuthStartupFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("daemon authentication startup failed")
    }
}
impl std::error::Error for AuthStartupFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            StartupError::Io(value) => Some(value),
            StartupError::Bootstrap(value) | StartupError::Document(value) => Some(value),
            StartupError::Store(value) => Some(value),
            StartupError::Authority(value) => Some(value),
            StartupError::Runtime(value) => Some(value),
        }
    }
}

/// Owns service resources without retaining the startup request, deadline or cancellation token.
pub struct DaemonAuth {
    issuer: RoleIssuer,
    installation: CapturedPublicationScope,
    binding: ScopeBindingRecord,
    runtime: Arc<RuntimeKeyOwner>,
    _singleton: FileLock,
}
impl DaemonAuth {
    /// Trusted projects are selected by the service owner before accepting requests.
    /// Their roots expand the realm, while permissions still determine each admission.
    pub fn start(
        paths: &InstallationPaths,
        trusted_projects: &[ProjectPaths],
        policy: StartupPolicy,
        original: &RequestContext<'_>,
    ) -> Result<Self, AuthStartupFailure> {
        let context = original.snapshot();
        let startup_deadline = context
            .required_deadline()
            .map_err(|error| AuthStartupFailure::before_runtime(StartupError::Io(error)))?;
        let private = installation_bootstrap::prepare(paths, policy.bootstrap, context)
            .into_installation()
            .map_err(|error| AuthStartupFailure::before_runtime(StartupError::Bootstrap(error)))?;
        let lock_path = paths
            .global_path(&["daemon.pid"])
            .map_err(|error| AuthStartupFailure::before_runtime(StartupError::Io(error)))?;
        let singleton = FileLock::try_acquire(lock_path)
            .map_err(|error| AuthStartupFailure::before_runtime(StartupError::Io(error)))?;
        context
            .check()
            .map_err(|error| AuthStartupFailure::before_runtime(StartupError::Io(error)))?;
        let timeout = startup_deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or_else(|| {
                AuthStartupFailure::before_runtime(StartupError::Io(io::ErrorKind::TimedOut.into()))
            })?;
        let identity = InstallationIdentityStore::new(paths)
            .and_then(|store| store.ensure(timeout, context.cancellation()))
            .map_err(|error| AuthStartupFailure::before_runtime(StartupError::Io(error)))?;
        let outcome = RuntimeKeyOwner::start(private, policy.runtime, context);
        if outcome.failure().is_some() || outcome.cleanup_failure().is_some() {
            return Err(AuthStartupFailure {
                cause: StartupError::Runtime(
                    outcome
                        .failure()
                        .or(outcome.cleanup_failure())
                        .unwrap_or(RuntimeKeyError::Conflict),
                ),
                resources: Some(StartupResources::RuntimeStart {
                    outcome: Box::new(outcome),
                    singleton,
                }),
            });
        }
        let runtime =
            Arc::new(outcome.into_owner().map_err(|error| {
                AuthStartupFailure::before_runtime(StartupError::Runtime(error))
            })?);
        let configured = (|| {
            let installation = CapturedPublicationScope::installation(
                paths,
                identity.unit_id().clone(),
                std::iter::once(paths.root().to_owned()).chain(
                    trusted_projects
                        .iter()
                        .map(|project| project.root().to_owned()),
                ),
                policy.scope,
                context,
            )
            .map_err(StartupError::Io)?;
            let binding = installation.binding().map_err(StartupError::Io)?;
            let store_scope = CapturedPublicationScope::installation(
                paths,
                identity.unit_id().clone(),
                installation.allowed_roots().map(ToOwned::to_owned),
                policy.scope,
                context,
            )
            .map_err(StartupError::Io)?;
            let physical = AuthDocumentStore::capture(store_scope, policy.document, context)
                .map_err(StartupError::Document)?;
            let store = Arc::new(
                AuthStore::new(physical, policy.profiles, original).map_err(StartupError::Store)?,
            );
            let issuer = RoleIssuer::new(
                ScopeCapture {
                    live: &installation,
                    record: &binding,
                },
                Arc::clone(&runtime),
                store,
                policy.authority,
                policy.ttl,
                original,
            )
            .map_err(StartupError::Authority)?;
            Ok::<_, StartupError>((issuer, installation, binding))
        })();
        match configured {
            Ok((issuer, installation, binding)) => Ok(Self {
                issuer,
                installation,
                binding,
                runtime,
                _singleton: singleton,
            }),
            Err(cause) => Err(AuthStartupFailure {
                cause,
                resources: Some(StartupResources::RuntimeOwner {
                    owner: runtime,
                    singleton,
                }),
            }),
        }
    }

    /// Expand physical trust without replacing the process issuer or runtime owner.
    pub fn trust_project_root(
        &self,
        caller: &AuthenticatedCaller<'_>,
        original: &RequestContext<'_>,
        project: ScopeCapture<'_>,
    ) -> Result<(), AuthError> {
        self.issuer.trust_project_root(caller, original, project)
    }

    pub fn issuer(&self) -> &RoleIssuer {
        &self.issuer
    }
    pub fn installation(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.installation,
            record: &self.binding,
        }
    }
    pub fn runtime(&self) -> &Arc<RuntimeKeyOwner> {
        &self.runtime
    }
    pub fn maintain(&self, original: &SnapshotContext) -> RuntimeKeyMutationOutcome {
        self.runtime.maintain(original)
    }
    /// Call after admissions stop and all service consumers have joined. Drop alone only unlocks.
    pub fn remove_runtime(&self, original: &SnapshotContext) -> RuntimeKeyMutationOutcome {
        self.runtime.remove(original)
    }
}
