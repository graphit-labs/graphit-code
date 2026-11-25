//! One process-owned installation issuer; incoming requests retain their original context.
use graphit_auth::{
    AuthError, AuthenticatedCaller, IssuerAuthorityPolicy, RoleIssuer, TtlPolicy,
    profile::ProfileLimits,
    store::{AuthStore, AuthStoreError},
};
use graphit_core::{
    artifact_store::PublicationScope,
    auth_document::{AuthDocumentError, AuthDocumentLimits, AuthDocumentStore},
    file_io::SnapshotContext,
    file_lock::FileLock,
    installation_identity::InstallationIdentityStore,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyEffect, RuntimeKeyError, RuntimeKeyOwner},
};
use graphit_hub::policy::RequestContext;
use std::{fmt, io, sync::Arc};

/// Explicit operator policy. This is selected once by service startup, never by incoming data.
pub struct StartupPolicy {
    pub document: AuthDocumentLimits,
    pub profiles: ProfileLimits,
    pub authority: IssuerAuthorityPolicy,
    pub ttl: TtlPolicy,
}

/// Failure categories retain the SDK's actual error rather than interpreting absence as admission.
#[derive(Debug)]
pub enum StartupError {
    Io(io::Error),
    Runtime(RuntimeKeyError),
    Document(AuthDocumentError),
    Store(AuthStoreError),
    Authority(AuthError),
}

pub struct AuthStartupFailure {
    cause: StartupError,
}
impl AuthStartupFailure {
    pub fn cause(&self) -> &StartupError {
        &self.cause
    }
    fn new(cause: StartupError) -> Self {
        Self { cause }
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
            StartupError::Document(value) => Some(value),
            StartupError::Store(value) => Some(value),
            StartupError::Authority(value) => Some(value),
            StartupError::Runtime(value) => Some(value),
        }
    }
}

/// Owns service resources without retaining the startup request, deadline or cancellation token.
pub struct DaemonAuth {
    issuer: RoleIssuer,
    installation: PublicationScope,
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
            .map_err(|error| AuthStartupFailure::new(StartupError::Io(error)))?;
        if let Some(parent) = paths.root().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| AuthStartupFailure::new(StartupError::Io(error)))?;
        }
        match graphit_fs::create_private_directory(paths.root()) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(AuthStartupFailure::new(StartupError::Io(error))),
        }
        let lock_path = paths
            .global_path(&["daemon.pid"])
            .map_err(|error| AuthStartupFailure::new(StartupError::Io(error)))?;
        let singleton = FileLock::try_acquire(lock_path)
            .map_err(|error| AuthStartupFailure::new(StartupError::Io(error)))?;
        context
            .check()
            .map_err(|error| AuthStartupFailure::new(StartupError::Io(error)))?;
        let timeout = startup_deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or_else(|| {
                AuthStartupFailure::new(StartupError::Io(io::ErrorKind::TimedOut.into()))
            })?;
        let identity = InstallationIdentityStore::new(paths)
            .and_then(|store| store.ensure(timeout, context.cancellation()))
            .map_err(|error| AuthStartupFailure::new(StartupError::Io(error)))?;
        let runtime = Arc::new(
            RuntimeKeyOwner::start(paths.root().to_owned(), context)
                .map_err(|error| AuthStartupFailure::new(StartupError::Runtime(error)))?,
        );
        let configured = (|| {
            let installation = PublicationScope::installation(
                paths,
                identity.unit_id().clone(),
                std::iter::once(paths.root().to_owned()).chain(
                    trusted_projects
                        .iter()
                        .map(|project| project.root().to_owned()),
                ),
            )
            .map_err(StartupError::Io)?;
            let physical =
                AuthDocumentStore::new(paths.root().to_owned(), policy.document, context)
                    .map_err(StartupError::Document)?;
            let store = Arc::new(
                AuthStore::new(physical, policy.profiles, original).map_err(StartupError::Store)?,
            );
            let issuer = RoleIssuer::new(
                &installation,
                Arc::clone(&runtime),
                store,
                policy.authority,
                policy.ttl,
                original,
            )
            .map_err(StartupError::Authority)?;
            Ok::<_, StartupError>((issuer, installation))
        })();
        match configured {
            Ok((issuer, installation)) => Ok(Self {
                issuer,
                installation,
                runtime,
                _singleton: singleton,
            }),
            Err(cause) => Err(AuthStartupFailure::new(cause)),
        }
    }

    /// Expand authorized project roots without replacing the process issuer or runtime owner.
    pub fn trust_project_root(
        &self,
        caller: &AuthenticatedCaller<'_>,
        original: &RequestContext<'_>,
        project: &PublicationScope,
    ) -> Result<(), AuthError> {
        self.issuer.trust_project_root(caller, original, project)
    }

    pub fn issuer(&self) -> &RoleIssuer {
        &self.issuer
    }
    pub fn installation(&self) -> &PublicationScope {
        &self.installation
    }
    pub fn runtime(&self) -> &Arc<RuntimeKeyOwner> {
        &self.runtime
    }
    pub fn maintain(
        &self,
        original: &SnapshotContext,
    ) -> Result<RuntimeKeyEffect, RuntimeKeyError> {
        self.runtime.maintain(original)
    }
    /// Call after admissions stop and all service consumers have joined. Drop alone only unlocks.
    pub fn remove_runtime(
        &self,
        original: &SnapshotContext,
    ) -> Result<RuntimeKeyEffect, RuntimeKeyError> {
        self.runtime.remove(original)
    }
}
