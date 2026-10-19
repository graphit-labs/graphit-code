//! Process-owned private runtime key. A physical receipt grants no application authority.
use crate::{
    artifact_store::{CapturedPublicationScope, ScopeContext},
    auth_document::AuthDocumentError,
    brand::Brand,
    file_io::{RequestIdentity, SnapshotContext},
    file_lock::FileLock,
    installation_bootstrap::PrivateInstallation,
    local_files::regular_metadata,
    native_document_file::{CapturedDocumentDirectory, FileObservation, NativeDocumentFileLimits},
    native_document_lock,
    native_file_identity::physical_id,
    private_document::{self, Access, CapturedFile, CreatedScratch, ExpectedEffect, Observed},
};
use secrecy::zeroize::Zeroizing;
use std::{
    fs::{self, File},
    io::{self, Seek, Write},
    path::{Path, PathBuf},
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

pub const RUNTIME_PRIVATE_NAMES: [&str; 3] = ["mcp.key", "mcp.owner.lock", ".mcp.key.next"];
const KEY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeKeyError {
    Invalid,
    Limit,
    Conflict,
    Busy,
    Cancelled,
    Deadline,
    Io,
}
impl std::fmt::Display for RuntimeKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid runtime key policy or private entry",
            Self::Limit => "runtime key limit exceeded",
            Self::Conflict => "runtime key observation changed",
            Self::Busy => "runtime key owner busy",
            Self::Cancelled => "runtime key operation cancelled",
            Self::Deadline => "runtime key deadline exceeded",
            Self::Io => "runtime key I/O failed",
        })
    }
}
impl std::error::Error for RuntimeKeyError {}
impl From<io::Error> for RuntimeKeyError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::Interrupted => Self::Cancelled,
            io::ErrorKind::TimedOut => Self::Deadline,
            io::ErrorKind::FileTooLarge => Self::Limit,
            io::ErrorKind::WouldBlock => Self::Busy,
            io::ErrorKind::InvalidData | io::ErrorKind::AlreadyExists | io::ErrorKind::NotFound => {
                Self::Conflict
            }
            io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput => Self::Invalid,
            _ => Self::Io,
        }
    }
}
impl From<AuthDocumentError> for RuntimeKeyError {
    fn from(e: AuthDocumentError) -> Self {
        match e {
            AuthDocumentError::Cancelled => Self::Cancelled,
            AuthDocumentError::Deadline => Self::Deadline,
            AuthDocumentError::Limit => Self::Limit,
            AuthDocumentError::Conflict | AuthDocumentError::Unavailable => Self::Conflict,
            AuthDocumentError::InvalidPolicy | AuthDocumentError::Denied => Self::Invalid,
            _ => Self::Io,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RuntimeKeyEffect {
    #[default]
    NotApplied,
    Applied,
    Unknown,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeNamespaceEffects {
    pub directory: RuntimeKeyEffect,
    pub owner_lock: RuntimeKeyEffect,
    pub stage: RuntimeKeyEffect,
    pub discard: RuntimeKeyEffect,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeKeyEffects {
    pub key: RuntimeKeyEffect,
    pub namespace: RuntimeNamespaceEffects,
}

#[derive(Clone, Copy)]
pub struct RuntimeKeyLimits {
    native: NativeDocumentFileLimits,
    access: usize,
}
impl RuntimeKeyLimits {
    pub fn new(
        path_bytes: usize,
        ancestors: usize,
        access_bytes: usize,
    ) -> Result<Self, RuntimeKeyError> {
        if access_bytes < 12
            || [path_bytes, ancestors, access_bytes]
                .iter()
                .any(|n| *n > isize::MAX as usize)
        {
            return Err(RuntimeKeyError::Invalid);
        }
        Ok(Self {
            native: NativeDocumentFileLimits::new(KEY, path_bytes, ancestors, access_bytes)
                .map_err(|_| RuntimeKeyError::Invalid)?,
            access: access_bytes,
        })
    }
}

struct Namespace {
    brand: Brand,
    root: CapturedDocumentDirectory,
    root_handle: File,
    directory: CapturedDocumentDirectory,
    handle: File,
    lease: FileLock,
    lock: FileObservation,
    key_path: PathBuf,
    stage_path: PathBuf,
    limits: RuntimeKeyLimits,
}
impl Namespace {
    fn check(&self, ctx: &SnapshotContext) -> io::Result<()> {
        ctx.check()?;
        self.root.verify_private_handle(&self.root_handle, ctx)?;
        self.directory.verify_private_handle(&self.handle, ctx)?;
        let access = Access {
            limit: KEY,
            access_limit: self.limits.access,
            context: Some(ctx),
            check: &|| ctx.check(),
        };
        if private_document::observe(self.lease.path(), self.lease.file(), &access)? != self.lock {
            return Err(private_document::conflict());
        }
        self.directory.verify_private_handle(&self.handle, ctx)?;
        ctx.check()
    }
    fn access<'a>(
        &'a self,
        ctx: &'a SnapshotContext,
        check: &'a dyn Fn() -> io::Result<()>,
    ) -> Access<'a> {
        Access {
            limit: KEY,
            access_limit: self.limits.access,
            context: Some(ctx),
            check,
        }
    }
    fn sync(&self) -> io::Result<()> {
        #[cfg(test)]
        if let Some(after) = SYNC_FAULT.with(|v| v.borrow_mut().take()) {
            if after {
                self.handle.sync_all()?;
            }
            return Err(io::ErrorKind::Other.into());
        }
        self.handle.sync_all()
    }
}
enum OwnerState {
    Active {
        generation: u64,
        receipt: FileObservation,
        pending: Option<CreatedScratch>,
    },
    Removed {
        generation: u64,
        pending: Option<CreatedScratch>,
    },
    Indeterminate {
        generation: u64,
        pending: Option<CreatedScratch>,
    },
}
impl OwnerState {
    fn pending(&self) -> Option<&CreatedScratch> {
        match self {
            Self::Active { pending, .. }
            | Self::Removed { pending, .. }
            | Self::Indeterminate { pending, .. } => pending.as_ref(),
        }
    }
    fn take_pending(&mut self) -> Option<CreatedScratch> {
        match self {
            Self::Active { pending, .. }
            | Self::Removed { pending, .. }
            | Self::Indeterminate { pending, .. } => pending.take(),
        }
    }
    fn set_pending(&mut self, value: Option<CreatedScratch>) {
        match self {
            Self::Active { pending, .. }
            | Self::Removed { pending, .. }
            | Self::Indeterminate { pending, .. } => *pending = value,
        }
    }
    fn generation(&self) -> u64 {
        match self {
            Self::Active { generation, .. }
            | Self::Removed { generation, .. }
            | Self::Indeterminate { generation, .. } => *generation,
        }
    }
}

/// Owns the lease and immutable process secret; dropping never deletes filesystem entries.
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyOwner;
/// fn clone<T: Clone>() {} clone::<RuntimeKeyOwner>();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyOwner;
/// fn debug<T: std::fmt::Debug>() {} debug::<RuntimeKeyOwner>();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyOwner;
/// let _: RuntimeKeyOwner = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyOwner;
/// fn serial<T: serde::Serialize>() {} serial::<RuntimeKeyOwner>();
/// ```
pub struct RuntimeKeyOwner {
    namespace: Namespace,
    secret: Zeroizing<[u8; KEY]>,
    state: RwLock<OwnerState>,
}
/// Checked data tied to one original request and owner generation; no retained lock guard.
/// ```compile_fail
/// use graphit_core::runtime_key::CapturedRuntimeKeyBorrow;
/// fn clone<T: Clone>() {} clone::<CapturedRuntimeKeyBorrow<'static>>();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::CapturedRuntimeKeyBorrow;
/// fn debug<T: std::fmt::Debug>() {} debug::<CapturedRuntimeKeyBorrow<'static>>();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::CapturedRuntimeKeyBorrow;
/// let _: CapturedRuntimeKeyBorrow<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::CapturedRuntimeKeyBorrow;
/// fn restore(bytes: &[u8]) -> CapturedRuntimeKeyBorrow<'static> { bytes.into() }
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::CapturedRuntimeKeyBorrow;
/// fn serial<T: serde::Serialize>() {} serial::<CapturedRuntimeKeyBorrow<'static>>();
/// ```
pub struct CapturedRuntimeKeyBorrow<'a> {
    owner: &'a RuntimeKeyOwner,
    generation: u64,
    request: RequestIdentity,
}
impl CapturedRuntimeKeyBorrow<'_> {
    /// Recheck the current key and its real installation realm, under this original request.
    /// Scope inspection has its own ScopeLimits; it performs no content reads.
    pub fn recheck_installation(
        &self,
        scope: &CapturedPublicationScope,
        ctx: &SnapshotContext,
    ) -> Result<(), RuntimeKeyError> {
        ctx.check()?;
        if !self.request.matches(ctx) {
            return Err(RuntimeKeyError::Conflict);
        }
        let state = self.owner.read_state(ctx)?;
        if !matches!(&*state,OwnerState::Active{generation,..} if *generation==self.generation) {
            return Err(RuntimeKeyError::Conflict);
        }
        let OwnerState::Active { receipt, .. } = &*state else {
            return Err(RuntimeKeyError::Conflict);
        };
        self.owner.namespace.check(ctx)?;
        let compared =
            compare_key_retained(&self.owner.namespace, receipt, &self.owner.secret, ctx)?;
        scope.recheck_runtime_installation(
            self.owner.namespace.root.canonical_path(),
            &self.owner.namespace.root_handle,
            &self.owner.namespace.brand,
            &ScopeContext::from_snapshot(ctx),
        )?;
        compared.recheck(&self.owner.namespace, ctx)?;
        self.owner.namespace.check(ctx)?;
        ctx.check()?;
        Ok(())
    }
    pub fn recheck(&self, ctx: &SnapshotContext) -> Result<(), RuntimeKeyError> {
        ctx.check()?;
        if !self.request.matches(ctx) {
            return Err(RuntimeKeyError::Conflict);
        }
        let state = self.owner.read_state(ctx)?;
        if !matches!(&*state,OwnerState::Active{generation,..} if *generation==self.generation) {
            return Err(RuntimeKeyError::Conflict);
        }
        self.owner.verify_active(&state, ctx)?;
        ctx.check()?;
        Ok(())
    }
    pub fn secret(&self, ctx: &SnapshotContext) -> Result<&[u8; KEY], RuntimeKeyError> {
        self.recheck(ctx)?;
        Ok(&self.owner.secret)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[must_use]
pub struct RuntimeKeyStartOutcome {
    owner: Option<RuntimeKeyOwner>,
    recovery: Option<RuntimeKeyRecovery>,
    effects: RuntimeKeyEffects,
    failure: Option<RuntimeKeyError>,
    cleanup_failure: Option<RuntimeKeyError>,
}
impl RuntimeKeyStartOutcome {
    pub fn effects(&self) -> RuntimeKeyEffects {
        self.effects
    }
    pub fn failure(&self) -> Option<RuntimeKeyError> {
        self.failure
    }
    pub fn cleanup_failure(&self) -> Option<RuntimeKeyError> {
        self.cleanup_failure
    }
    pub fn into_owner(self) -> Result<RuntimeKeyOwner, RuntimeKeyError> {
        self.owner
            .ok_or(self.failure.unwrap_or(RuntimeKeyError::Conflict))
    }
    pub fn into_recovery(self) -> Option<RuntimeKeyRecovery> {
        self.recovery
    }
}
#[must_use]
pub struct RuntimeKeyMutationOutcome {
    effects: RuntimeKeyEffects,
    failure: Option<RuntimeKeyError>,
    cleanup_failure: Option<RuntimeKeyError>,
    generation: u64,
}
impl RuntimeKeyMutationOutcome {
    pub fn effects(&self) -> RuntimeKeyEffects {
        self.effects
    }
    pub fn failure(&self) -> Option<RuntimeKeyError> {
        self.failure
    }
    pub fn cleanup_failure(&self) -> Option<RuntimeKeyError> {
        self.cleanup_failure
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
enum StageExpected {
    Observed(FileObservation),
    Origin(CreatedScratch),
}
/// Owns namespace/lease and one bounded observed stage, not its content or historical authority.
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyRecovery;
/// fn clone<T: Clone>() {} clone::<RuntimeKeyRecovery>();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyRecovery;
/// fn debug<T: std::fmt::Debug>() {} debug::<RuntimeKeyRecovery>();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyRecovery;
/// let _: RuntimeKeyRecovery = serde_json::from_str("{}").unwrap();
/// ```
///
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyRecovery;
/// fn serial<T: serde::Serialize>() {} serial::<RuntimeKeyRecovery>();
/// ```
pub struct RuntimeKeyRecovery {
    namespace: Namespace,
    stage: StageExpected,
}
/// Consumptive continuation after successful explicit discard; retains the same real lease.
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyBootstrapReady;
/// fn clone<T: Clone>() {} clone::<RuntimeKeyBootstrapReady>();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyBootstrapReady;
/// fn debug<T: std::fmt::Debug>() {} debug::<RuntimeKeyBootstrapReady>();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyBootstrapReady;
/// let _: RuntimeKeyBootstrapReady = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::runtime_key::RuntimeKeyBootstrapReady;
/// fn serial<T: serde::Serialize>() {} serial::<RuntimeKeyBootstrapReady>();
/// ```
pub struct RuntimeKeyBootstrapReady {
    namespace: Namespace,
}
#[must_use]
pub struct RuntimeKeyRecoveryOutcome {
    ready: Option<RuntimeKeyBootstrapReady>,
    recovery: Option<RuntimeKeyRecovery>,
    effects: RuntimeKeyEffects,
    failure: Option<RuntimeKeyError>,
}
impl RuntimeKeyRecoveryOutcome {
    pub fn effects(&self) -> RuntimeKeyEffects {
        self.effects
    }
    pub fn failure(&self) -> Option<RuntimeKeyError> {
        self.failure
    }
    pub fn into_ready(self) -> Result<RuntimeKeyBootstrapReady, RuntimeKeyError> {
        self.ready
            .ok_or(self.failure.unwrap_or(RuntimeKeyError::Conflict))
    }
    pub fn into_recovery(self) -> Option<RuntimeKeyRecovery> {
        self.recovery
    }
}

impl RuntimeKeyOwner {
    pub fn start(
        installation: PrivateInstallation,
        limits: RuntimeKeyLimits,
        ctx: &SnapshotContext,
    ) -> RuntimeKeyStartOutcome {
        let mut effects = RuntimeKeyEffects::default();
        let prepared = (|| -> Result<Namespace, RuntimeKeyError> {
            ctx.check()?;
            installation.revalidate(ctx)?;
            let root = installation.runtime_directory().canonical_path();
            CapturedDocumentDirectory::check_child_path(root, "daemon", limits.native)?;
            let paths = planned_paths(root, limits)?;
            let (root, root_handle, brand) = installation.into_runtime_parts(ctx)?;
            root.verify_private_handle(&root_handle, ctx)?;
            let daemon_handle = match graphit_fs::create_private_directory(&paths[0]) {
                Ok(file) => {
                    effects.namespace.directory = RuntimeKeyEffect::Applied;
                    Some(file)
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => None,
                Err(e) => {
                    effects.namespace.directory = RuntimeKeyEffect::Unknown;
                    return Err(e.into());
                }
            };
            phase("directory_created");
            root.verify_private_handle(&root_handle, ctx)?;
            let directory = CapturedDocumentDirectory::capture(&paths[0], limits.native, ctx)?;
            let handle = match daemon_handle {
                Some(file) => {
                    directory.verify_private_handle(&file, ctx)?;
                    file
                }
                None => directory.private_handle(ctx)?,
            };
            let file = match graphit_fs::create_private_lock_file(&paths[3]) {
                Ok(file) => {
                    effects.namespace.owner_lock = RuntimeKeyEffect::Applied;
                    file
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    native_document_lock::open_existing(&paths[3])?
                }
                Err(e) => {
                    effects.namespace.owner_lock = RuntimeKeyEffect::Unknown;
                    return Err(e.into());
                }
            };
            let check = || {
                root.verify_private_handle(&root_handle, ctx)?;
                directory.verify_private_handle(&handle, ctx)
            };
            let access = Access {
                limit: KEY,
                access_limit: limits.access,
                context: Some(ctx),
                check: &check,
            };
            let lock = private_document::observe(&paths[3], &file, &access)?;
            if lock.length != 0 {
                return Err(RuntimeKeyError::Invalid);
            }
            let lease = FileLock::acquire_owned_file(
                file,
                paths[3].clone(),
                ctx.deadline(),
                ctx.cancellation(),
            )?;
            let mut iter = paths.into_iter();
            let _ = iter.next();
            let namespace = Namespace {
                brand,
                root,
                root_handle,
                directory,
                handle,
                lease,
                lock,
                key_path: iter.next().ok_or(RuntimeKeyError::Invalid)?,
                stage_path: iter.next().ok_or(RuntimeKeyError::Invalid)?,
                limits,
            };
            namespace.check(ctx)?;
            Ok(namespace)
        })();
        match prepared {
            Ok(namespace) => start_namespace(namespace, effects, ctx),
            Err(e) => RuntimeKeyStartOutcome {
                owner: None,
                recovery: None,
                effects,
                failure: Some(e),
                cleanup_failure: None,
            },
        }
    }
    fn read_state<'a>(
        &'a self,
        ctx: &SnapshotContext,
    ) -> Result<RwLockReadGuard<'a, OwnerState>, RuntimeKeyError> {
        ctx.check()?;
        let guard = self.state.read().map_err(|_| RuntimeKeyError::Io)?;
        ctx.check()?;
        Ok(guard)
    }
    fn write_state<'a>(
        &'a self,
        ctx: &SnapshotContext,
    ) -> Result<RwLockWriteGuard<'a, OwnerState>, RuntimeKeyError> {
        ctx.check()?;
        let guard = self.state.write().map_err(|_| RuntimeKeyError::Io)?;
        ctx.check()?;
        Ok(guard)
    }
    fn verify_active(
        &self,
        state: &OwnerState,
        ctx: &SnapshotContext,
    ) -> Result<(), RuntimeKeyError> {
        let OwnerState::Active { receipt, .. } = state else {
            return Err(RuntimeKeyError::Conflict);
        };
        self.namespace.check(ctx)?;
        compare_key(&self.namespace, receipt, &self.secret, ctx)?;
        self.namespace.check(ctx)?;
        Ok(())
    }
    pub fn check(
        &self,
        ctx: &SnapshotContext,
    ) -> Result<CapturedRuntimeKeyBorrow<'_>, RuntimeKeyError> {
        let state = self.read_state(ctx)?;
        self.verify_active(&state, ctx)?;
        Ok(CapturedRuntimeKeyBorrow {
            owner: self,
            generation: state.generation(),
            request: ctx.request_identity(),
        })
    }
    pub fn maintain(&self, ctx: &SnapshotContext) -> RuntimeKeyMutationOutcome {
        let mut effects = RuntimeKeyEffects::default();
        let mut generation = 0;
        let mut cleanup_failure = None;
        let result = (|| -> Result<(), RuntimeKeyError> {
            let mut state = self.write_state(ctx)?;
            generation = state.generation();
            if !matches!(&*state, OwnerState::Active { .. }) {
                return Err(RuntimeKeyError::Conflict);
            }
            if state.pending().is_some() {
                return Err(RuntimeKeyError::Conflict);
            }
            self.namespace.check(ctx)?;
            if regular_metadata(&self.namespace.key_path)?.is_some() {
                return self.verify_active(&state, ctx);
            }
            let next = generation.checked_add(1).ok_or(RuntimeKeyError::Limit)?;
            let publication = publish(&self.namespace, &self.secret, None, ctx);
            effects = publication.effects;
            cleanup_failure = publication.cleanup_failure;
            state.set_pending(publication.pending_origin);
            match effects.key {
                RuntimeKeyEffect::Applied => {
                    generation = next;
                    *state = match publication.receipt {
                        Some(receipt)
                            if publication.failure.is_none()
                                && publication.cleanup_failure.is_none() =>
                        {
                            OwnerState::Active {
                                generation,
                                receipt,
                                pending: state.take_pending(),
                            }
                        }
                        _ => OwnerState::Indeterminate {
                            generation,
                            pending: state.take_pending(),
                        },
                    };
                }
                RuntimeKeyEffect::Unknown => {
                    generation = next;
                    *state = OwnerState::Indeterminate {
                        generation,
                        pending: state.take_pending(),
                    };
                }
                RuntimeKeyEffect::NotApplied => {}
            }
            if let Some(e) = publication.failure {
                return Err(e);
            }
            Ok(())
        })();
        RuntimeKeyMutationOutcome {
            effects,
            failure: result.err(),
            cleanup_failure,
            generation,
        }
    }
    pub fn remove(&self, ctx: &SnapshotContext) -> RuntimeKeyMutationOutcome {
        let mut effects = RuntimeKeyEffects::default();
        let mut generation = 0;
        let result = (|| -> Result<(), RuntimeKeyError> {
            let mut state = self.write_state(ctx)?;
            generation = state.generation();
            self.verify_active(&state, ctx)?;
            let next = generation.checked_add(1).ok_or(RuntimeKeyError::Limit)?;
            phase("before_remove");
            self.namespace.check(ctx)?;
            let OwnerState::Active { receipt, .. } = &*state else {
                return Err(RuntimeKeyError::Conflict);
            };
            let expected = &receipt.identity;
            let check = || self.namespace.check(ctx);
            let access = self.namespace.access(ctx, &check);
            let Some((file, latest)) =
                private_document::capture(&self.namespace.key_path, &access)?
            else {
                return Err(RuntimeKeyError::Conflict);
            };
            if latest != *receipt {
                return Err(RuntimeKeyError::Conflict);
            }
            drop(file);
            let native = native_remove(&self.namespace.key_path);
            if native.is_ok() {
                effects.key = RuntimeKeyEffect::Applied;
            }
            phase("after_remove");
            let observation =
                (|| -> io::Result<Option<crate::native_file_identity::PhysicalId>> {
                    self.namespace.check(ctx)?;
                    regular_metadata(&self.namespace.key_path)?
                        .map(|m| physical_id(&self.namespace.key_path, &m))
                        .transpose()
                })();
            if native.is_err() {
                effects.key = match &observation {
                    Ok(None) => RuntimeKeyEffect::Applied,
                    Ok(Some(id)) if id == expected => RuntimeKeyEffect::NotApplied,
                    _ => RuntimeKeyEffect::Unknown,
                };
            }
            match effects.key {
                RuntimeKeyEffect::Applied => {
                    generation = next;
                    *state = OwnerState::Removed {
                        generation,
                        pending: state.take_pending(),
                    };
                }
                RuntimeKeyEffect::Unknown => {
                    generation = next;
                    *state = OwnerState::Indeterminate {
                        generation,
                        pending: state.take_pending(),
                    };
                }
                RuntimeKeyEffect::NotApplied => {}
            }
            native?;
            observation?;
            self.namespace.sync()?;
            self.namespace.check(ctx)?;
            Ok(())
        })();
        RuntimeKeyMutationOutcome {
            effects,
            failure: result.err(),
            cleanup_failure: None,
            generation,
        }
    }
}

impl RuntimeKeyBootstrapReady {
    pub fn start(self, ctx: &SnapshotContext) -> RuntimeKeyStartOutcome {
        let effects = RuntimeKeyEffects::default();
        if let Err(error) = ctx.check() {
            return RuntimeKeyStartOutcome {
                owner: None,
                recovery: None,
                effects,
                failure: Some(error.into()),
                cleanup_failure: None,
            };
        }
        start_namespace(self.namespace, effects, ctx)
    }
}
impl RuntimeKeyRecovery {
    pub fn discard(self, ctx: &SnapshotContext) -> RuntimeKeyRecoveryOutcome {
        let discarded = discard_stage(&self.namespace, &self.stage, ctx);
        let mut effects = RuntimeKeyEffects::default();
        effects.namespace.discard = discarded.effect;
        match discarded.failure {
            None => RuntimeKeyRecoveryOutcome {
                ready: Some(RuntimeKeyBootstrapReady {
                    namespace: self.namespace,
                }),
                recovery: None,
                effects,
                failure: None,
            },
            Some(error) => RuntimeKeyRecoveryOutcome {
                ready: None,
                recovery: Some(self),
                effects,
                failure: Some(error),
            },
        }
    }
}
struct DiscardResult {
    effect: RuntimeKeyEffect,
    failure: Option<RuntimeKeyError>,
}
fn discard_stage(
    namespace: &Namespace,
    expected: &StageExpected,
    ctx: &SnapshotContext,
) -> DiscardResult {
    let mut effect = RuntimeKeyEffect::NotApplied;
    let result = (|| -> Result<(), RuntimeKeyError> {
        namespace.check(ctx)?;
        let check = || namespace.check(ctx);
        let access = namespace.access(ctx, &check);
        let current = private_document::capture(&namespace.stage_path, &access)?;
        let Some((file, observed)) = current else {
            // Absence is a current managed-slot fact, not a restored origin.
            namespace.check(ctx)?;
            if regular_metadata(&namespace.stage_path)?.is_some() {
                return Err(RuntimeKeyError::Conflict);
            }
            namespace.sync()?;
            namespace.check(ctx)?;
            return Ok(());
        };
        match expected {
            StageExpected::Observed(old) if *old != observed => {
                return Err(RuntimeKeyError::Conflict);
            }
            StageExpected::Origin(origin) => origin.verify(&file, &check)?,
            StageExpected::Observed(_) => {}
        }
        drop(file);
        phase("before_discard");
        let Some((file, latest)) = private_document::capture(&namespace.stage_path, &access)?
        else {
            return Err(RuntimeKeyError::Conflict);
        };
        if latest != observed {
            return Err(RuntimeKeyError::Conflict);
        }
        if let StageExpected::Origin(origin) = expected {
            origin.verify(&file, &check)?;
        }
        drop(file);
        namespace.check(ctx)?;
        let native = native_remove(&namespace.stage_path);
        if native.is_ok() {
            effect = RuntimeKeyEffect::Applied;
        }
        phase("after_discard");
        let after = (|| -> io::Result<Option<crate::native_file_identity::PhysicalId>> {
            namespace.check(ctx)?;
            regular_metadata(&namespace.stage_path)?
                .map(|m| physical_id(&namespace.stage_path, &m))
                .transpose()
        })();
        if native.is_err() {
            effect = match &after {
                Ok(None) => RuntimeKeyEffect::Applied,
                Ok(Some(id)) if *id == observed.identity => RuntimeKeyEffect::NotApplied,
                _ => RuntimeKeyEffect::Unknown,
            };
        }
        native?;
        if after?.is_some() {
            return Err(RuntimeKeyError::Conflict);
        }
        namespace.sync()?;
        namespace.check(ctx)?;
        Ok(())
    })();
    DiscardResult {
        effect,
        failure: result.err(),
    }
}
impl RuntimeKeyOwner {
    pub fn discard_pending(&self, ctx: &SnapshotContext) -> RuntimeKeyMutationOutcome {
        let mut effects = RuntimeKeyEffects::default();
        let mut generation = 0;
        let result = (|| -> Result<(), RuntimeKeyError> {
            let mut state = self.write_state(ctx)?;
            generation = state.generation();
            let Some(origin) = state.take_pending() else {
                self.namespace.check(ctx)?;
                return Ok(());
            };
            let expected = StageExpected::Origin(origin);
            let discarded = discard_stage(&self.namespace, &expected, ctx);
            effects.namespace.discard = discarded.effect;
            if discarded.failure.is_some() {
                let StageExpected::Origin(origin) = expected else {
                    return Err(RuntimeKeyError::Conflict);
                };
                state.set_pending(Some(origin));
            }
            if let Some(error) = discarded.failure {
                return Err(error);
            }
            Ok(())
        })();
        RuntimeKeyMutationOutcome {
            effects,
            failure: result.err(),
            cleanup_failure: None,
            generation,
        }
    }
}

fn planned_paths(root: &Path, limits: RuntimeKeyLimits) -> Result<[PathBuf; 4], RuntimeKeyError> {
    limits.native.check_path(root)?;
    let names = [
        "daemon",
        "daemon/mcp.key",
        "daemon/.mcp.key.next",
        "daemon/mcp.owner.lock",
    ];
    let build = |name: &str| -> Result<PathBuf, RuntimeKeyError> {
        let cap = limits.native.child_path_cost(root, name)?;
        let mut path = PathBuf::new();
        path.try_reserve_exact(cap)
            .map_err(|_| RuntimeKeyError::Io)?;
        path.push(root);
        path.push(name);
        Ok(path)
    };
    let paths = [
        build(names[0])?,
        build(names[1])?,
        build(names[2])?,
        build(names[3])?,
    ];
    for path in &paths {
        limits.native.check_path(path)?;
    }
    Ok(paths)
}
fn entropy() -> Result<Zeroizing<[u8; KEY]>, RuntimeKeyError> {
    let mut random = Zeroizing::new([0u8; 32]);
    #[cfg(test)]
    if ENTROPY_FAIL.with(|v| v.replace(false)) {
        return Err(RuntimeKeyError::Io);
    }
    getrandom::fill(&mut *random).map_err(|_| RuntimeKeyError::Io)?;
    let mut key = Zeroizing::new([0; KEY]);
    let hex = b"0123456789abcdef";
    for (i, b) in random.iter().enumerate() {
        key[2 * i] = hex[usize::from(b >> 4)];
        key[2 * i + 1] = hex[usize::from(b & 15)];
    }
    Ok(key)
}
struct PreparedKey {
    secret: Zeroizing<[u8; KEY]>,
    old: Option<Observed>,
}

fn start_namespace(
    namespace: Namespace,
    mut effects: RuntimeKeyEffects,
    ctx: &SnapshotContext,
) -> RuntimeKeyStartOutcome {
    let check = || namespace.check(ctx);
    let access = namespace.access(ctx, &check);
    let stage = private_document::capture(&namespace.stage_path, &access);
    match stage {
        Ok(Some((file, stage))) => {
            drop(file);
            return RuntimeKeyStartOutcome {
                owner: None,
                recovery: Some(RuntimeKeyRecovery {
                    namespace,
                    stage: StageExpected::Observed(stage),
                }),
                effects,
                failure: Some(RuntimeKeyError::Conflict),
                cleanup_failure: None,
            };
        }
        Err(e) => {
            return RuntimeKeyStartOutcome {
                owner: None,
                recovery: None,
                effects,
                failure: Some(e.into()),
                cleanup_failure: None,
            };
        }
        Ok(None) => {}
    }
    let prepared = (|| -> Result<PreparedKey, RuntimeKeyError> {
        let secret = entropy()?;
        let old = CapturedFile::open(&namespace.key_path, &namespace.access(ctx, &check), || {})?
            .map(|c| {
                drop(c.file);
                c.value
            });
        if let Some(old) = &old
            && (old.bytes.len() != KEY
                || !old
                    .bytes
                    .iter()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)))
        {
            return Err(RuntimeKeyError::Invalid);
        }
        Ok(PreparedKey { secret, old })
    })();
    let PreparedKey { secret, old } = match prepared {
        Ok(p) => p,
        Err(e) => {
            return RuntimeKeyStartOutcome {
                owner: None,
                recovery: None,
                effects,
                failure: Some(e),
                cleanup_failure: None,
            };
        }
    };
    let publication = publish(&namespace, &secret, old.as_ref(), ctx);
    effects.key = publication.effects.key;
    effects.namespace.stage = publication.effects.namespace.stage;
    effects.namespace.discard = publication.effects.namespace.discard;
    let (owner, recovery) = match publication.receipt {
        Some(receipt) if publication.failure.is_none() && publication.cleanup_failure.is_none() => {
            (
                Some(RuntimeKeyOwner {
                    namespace,
                    secret,
                    state: RwLock::new(OwnerState::Active {
                        generation: 1,
                        receipt,
                        pending: None,
                    }),
                }),
                None,
            )
        }
        _ => (
            None,
            publication.pending_origin.map(|origin| RuntimeKeyRecovery {
                namespace,
                stage: StageExpected::Origin(origin),
            }),
        ),
    };
    RuntimeKeyStartOutcome {
        owner,
        recovery,
        effects,
        failure: publication.failure,
        cleanup_failure: publication.cleanup_failure,
    }
}

struct CheckedKeyComparison {
    file: File,
    observed: FileObservation,
}
impl CheckedKeyComparison {
    fn recheck(&self, namespace: &Namespace, ctx: &SnapshotContext) -> io::Result<()> {
        // The caller fences the entire comparison with namespace checks. Keep
        // the original request live within the exact-handle file observation.
        let check = || ctx.check();
        let access = namespace.access(ctx, &check);
        if private_document::observe(&namespace.key_path, &self.file, &access)? != self.observed {
            return Err(private_document::conflict());
        }
        access.check()
    }
}
fn compare_key(
    namespace: &Namespace,
    receipt: &FileObservation,
    secret: &[u8; KEY],
    ctx: &SnapshotContext,
) -> io::Result<()> {
    compare_key_retained(namespace, receipt, secret, ctx).map(drop)
}
fn compare_key_retained(
    namespace: &Namespace,
    receipt: &FileObservation,
    secret: &[u8; KEY],
    ctx: &SnapshotContext,
) -> io::Result<CheckedKeyComparison> {
    // Both callers check namespace identity/privacy before and after this read;
    // do not repeat the ancestor walk at every metadata, byte and EOF probe.
    let check = || ctx.check();
    let access = namespace.access(ctx, &check);
    let Some((mut file, observed)) = private_document::capture(&namespace.key_path, &access)?
    else {
        return Err(private_document::conflict());
    };
    if observed != *receipt || observed.length != KEY as u64 {
        return Err(private_document::conflict());
    }
    private_document::compare_bytes(&mut file, secret, &access)?;
    if private_document::observe(&namespace.key_path, &file, &access)? != observed {
        return Err(private_document::conflict());
    }
    Ok(CheckedKeyComparison { file, observed })
}
struct Publication {
    pending_origin: Option<CreatedScratch>,
    receipt: Option<FileObservation>,
    effects: RuntimeKeyEffects,
    failure: Option<RuntimeKeyError>,
    cleanup_failure: Option<RuntimeKeyError>,
}
fn publish(
    namespace: &Namespace,
    body: &[u8; KEY],
    old: Option<&Observed>,
    ctx: &SnapshotContext,
) -> Publication {
    let mut output = Publication {
        pending_origin: None,
        receipt: None,
        effects: RuntimeKeyEffects::default(),
        failure: None,
        cleanup_failure: None,
    };
    let check = || namespace.check(ctx);
    let access = namespace.access(ctx, &check);
    let prepared = (|| -> io::Result<(CreatedScratch, File)> {
        access.check()?;
        if regular_metadata(&namespace.stage_path)?.is_some() {
            return Err(private_document::conflict());
        }
        ctx.check()?;
        match CreatedScratch::create(
            namespace.stage_path.clone(),
            &check,
            private_document::create_file,
        ) {
            Ok(v) => {
                output.effects.namespace.stage = RuntimeKeyEffect::Applied;
                Ok(v)
            }
            Err(e) => {
                output.effects.namespace.stage = RuntimeKeyEffect::Unknown;
                Err(e)
            }
        }
    })();
    let (scratch, mut file) = match prepared {
        Ok(v) => v,
        Err(e) => {
            output.failure = Some(e.into());
            return output;
        }
    };
    phase("stage_created");
    let written = (|| -> io::Result<()> {
        let mut remaining = &body[..];
        while !remaining.is_empty() {
            access.check()?;
            scratch.verify(&file, &check)?;
            #[cfg(test)]
            let wanted = WRITE_MAX.with(|v| v.get()).min(remaining.len());
            #[cfg(not(test))]
            let wanted = remaining.len();
            let n = file.write(&remaining[..wanted])?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            remaining = &remaining[n..];
            phase("written");
        }
        access.check()?;
        scratch.verify(&file, &check)?;
        file.sync_all()?;
        phase("stage_synced");
        file.rewind()?;
        private_document::compare_bytes(&mut file, body, &access)?;
        let metadata = private_document::observe(scratch.path(), &file, &access)?;
        if metadata.length != KEY as u64 {
            return Err(private_document::conflict());
        }
        Ok(())
    })();
    drop(file);
    let publication = (|| -> io::Result<FileObservation> {
        written?;
        let Some((mut file, observed)) = private_document::capture(scratch.path(), &access)? else {
            return Err(private_document::conflict());
        };
        if observed.identity != *scratch.identity() {
            return Err(private_document::conflict());
        }
        private_document::compare_bytes(&mut file, body, &access)?;
        if private_document::observe(scratch.path(), &file, &access)? != observed {
            return Err(private_document::conflict());
        }
        drop(file);
        private_document::verify_closed(&namespace.key_path, old, &access)?;
        phase("before_replace");
        let Some((file, latest)) = private_document::capture(scratch.path(), &access)? else {
            return Err(private_document::conflict());
        };
        if latest != observed || latest.identity != *scratch.identity() {
            return Err(private_document::conflict());
        }
        drop(file);
        access.check()?;
        let identity = private_document::replacement_identity(
            scratch.identity(),
            old.map(|v| &v.metadata.identity),
        )?;
        let native = native_replace(scratch.path(), &namespace.key_path, old.is_some());
        if native.is_ok() {
            output.effects.key = RuntimeKeyEffect::Applied;
        }
        phase("after_replace");
        let observation = private_document::observe_effect(
            &namespace.key_path,
            ExpectedEffect {
                identity: &identity,
                bytes: body,
                old,
                native_failed: native.is_err(),
                retain: false,
            },
            &access,
        );
        if native.is_err() {
            output.effects.key = match &observation {
                Ok(Some(v)) if v.metadata.identity == identity => RuntimeKeyEffect::Applied,
                Ok(None) => RuntimeKeyEffect::NotApplied,
                Ok(Some(v)) if old.is_some_and(|o| o.metadata == v.metadata) => {
                    RuntimeKeyEffect::NotApplied
                }
                _ => RuntimeKeyEffect::Unknown,
            };
        }
        native?;
        let observed = observation?.ok_or_else(private_document::conflict)?;
        namespace.sync()?;
        phase("after_sync");
        access.check()?;
        Ok(observed.metadata)
    })();
    match publication {
        Ok(receipt) => output.receipt = Some(receipt),
        Err(e) => output.failure = Some(e.into()),
    }
    phase("before_cleanup");
    // An origin must survive errors, foreign replacement and late cancellation.
    // Never turn a failed metadata read into an absent/successful cleanup.
    match regular_metadata(scratch.path()) {
        Ok(None) => {
            if let Err(error) = namespace.check(ctx) {
                output.cleanup_failure = Some(error.into());
                output.pending_origin = Some(scratch);
            }
        }
        Ok(Some(_)) => {
            let expected = StageExpected::Origin(scratch);
            let discarded = discard_stage(namespace, &expected, ctx);
            output.effects.namespace.discard = discarded.effect;
            if let Some(error) = discarded.failure {
                output.cleanup_failure = Some(error);
                if let StageExpected::Origin(origin) = expected {
                    output.pending_origin = Some(origin);
                }
            }
        }
        Err(error) => {
            output.cleanup_failure = Some(error.into());
            output.pending_origin = Some(scratch);
        }
    }

    output
}
fn native_replace(stage: &Path, destination: &Path, exists: bool) -> io::Result<()> {
    #[cfg(test)]
    if let Some(after) = REPLACE_FAULT.with(|v| v.borrow_mut().take()) {
        if after {
            private_document::replace(stage, destination, exists)?;
        }
        return Err(io::ErrorKind::Other.into());
    }
    private_document::replace(stage, destination, exists)
}
fn native_remove(path: &Path) -> io::Result<()> {
    #[cfg(test)]
    if let Some(after) = REMOVE_FAULT.with(|v| v.borrow_mut().take()) {
        if after {
            fs::remove_file(path)?;
        }
        return Err(io::ErrorKind::Other.into());
    }
    fs::remove_file(path)
}
fn phase(name: &'static str) {
    #[cfg(test)]
    HOOK.with(|v| {
        if let Some(hook) = v.borrow_mut().as_mut() {
            hook(name);
        }
    });
    #[cfg(not(test))]
    let _ = name;
}
#[cfg(test)]
type Hook = Box<dyn FnMut(&'static str)>;
#[cfg(test)]
thread_local! {
    static HOOK: std::cell::RefCell<Option<Hook>> = const { std::cell::RefCell::new(None) };
    static REPLACE_FAULT: std::cell::RefCell<Option<bool>> = const { std::cell::RefCell::new(None) };
    static REMOVE_FAULT: std::cell::RefCell<Option<bool>> = const { std::cell::RefCell::new(None) };
    static ENTROPY_FAIL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static WRITE_MAX: std::cell::Cell<usize> = const { std::cell::Cell::new(usize::MAX) };
    static SYNC_FAULT: std::cell::RefCell<Option<bool>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
mod tests;
