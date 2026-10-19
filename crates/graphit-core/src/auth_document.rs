//! Private, fixed-name installation authentication document observations.
//! The body is data. Capturing it does not authenticate a caller or grant access.
use crate::{
    artifact_store::{CapturedPublicationScope, ScopeContext, StoreOwner},
    file_io::{RequestIdentity, SnapshotContext},
    native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    native_document_lock::NativeDocumentLock,
    private_document::{self, Access, CapturedFile},
};
use std::{ffi::OsStr, fmt, fs::File, io, sync::Arc};

pub mod publication;
pub mod recovery;
#[cfg(feature = "auth-test-support")]
pub mod test_support;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthDocumentError {
    InvalidPolicy,
    Denied,
    Conflict,
    Limit,
    Cancelled,
    Deadline,
    Unavailable,
    Resource,
    Io,
}
impl fmt::Display for AuthDocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPolicy => "invalid authentication document policy",
            Self::Denied => "authentication document access denied",
            Self::Conflict => "authentication document changed",
            Self::Limit => "authentication document limit exceeded",
            Self::Cancelled => "authentication document request cancelled",
            Self::Deadline => "authentication document deadline exceeded",
            Self::Unavailable => "authentication document parent unavailable",
            Self::Resource => "authentication document resources unavailable",
            Self::Io => "authentication document I/O failed",
        })
    }
}
impl std::error::Error for AuthDocumentError {}
impl From<io::Error> for AuthDocumentError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::Interrupted => Self::Cancelled,
            io::ErrorKind::TimedOut => Self::Deadline,
            io::ErrorKind::FileTooLarge => Self::Limit,
            io::ErrorKind::OutOfMemory => Self::Resource,
            io::ErrorKind::PermissionDenied => Self::Denied,
            io::ErrorKind::NotFound => Self::Unavailable,
            io::ErrorKind::InvalidData | io::ErrorKind::AlreadyExists => Self::Conflict,
            io::ErrorKind::InvalidInput => Self::InvalidPolicy,
            _ => Self::Io,
        }
    }
}

/// Positive limits, supplied by the captured application resource policy.
#[derive(Clone, Copy)]
pub struct AuthDocumentLimits {
    file: NativeDocumentFileLimits,
    bytes: usize,
    access_bytes: usize,
}
impl AuthDocumentLimits {
    pub fn new(
        bytes: usize,
        path_bytes: usize,
        ancestors: usize,
        access_bytes: usize,
    ) -> Result<Self, AuthDocumentError> {
        let file = NativeDocumentFileLimits::new(bytes, path_bytes, ancestors, access_bytes)
            .map_err(|_| AuthDocumentError::InvalidPolicy)?;
        Ok(Self {
            file,
            bytes,
            access_bytes,
        })
    }
}

/// A real private parent capture; there is no arbitrary filename or root DTO.
/// Capture never creates or repairs the parent. Bootstrap must qualify it first.
pub struct AuthDocumentStore {
    scope: CapturedPublicationScope,
    directory: CapturedDocumentDirectory,
    parent: File,
    limits: AuthDocumentLimits,
    identity: Arc<()>,
}
impl AuthDocumentStore {
    pub fn capture(
        scope: CapturedPublicationScope,
        limits: AuthDocumentLimits,
        context: &SnapshotContext,
    ) -> Result<Self, AuthDocumentError> {
        context.check()?;
        if !matches!(scope.owner(), StoreOwner::Installation { .. })
            || !scope.allowed_roots().any(|root| root == scope.owner_root())
        {
            return Err(AuthDocumentError::InvalidPolicy);
        }
        scope.revalidate_installation(&scope_context(context))?;
        let directory =
            CapturedDocumentDirectory::capture(scope.owner_root(), limits.file, context)?;
        if directory.canonical_path() != scope.owner_root() {
            return Err(AuthDocumentError::Conflict);
        }
        directory.path(OsStr::new("auth.json"))?;
        let parent = directory.private_handle(context)?;
        let store = Self {
            scope,
            directory,
            parent,
            limits,
            identity: Arc::new(()),
        };
        store.check(context)?;
        Ok(store)
    }

    /// Metadata-only binding to a real realm of this same captured installation.
    /// InstallationId compares selected captures, not independently persisted identity.
    /// It neither validates an auth.json snapshot nor authenticates a caller.
    pub fn recheck_installation(
        &self,
        scope: &CapturedPublicationScope,
        context: &SnapshotContext,
    ) -> Result<(), AuthDocumentError> {
        context.check()?;
        self.check(context)?;
        self.directory
            .verify_private_handle(&self.parent, context)?;
        if self.scope.owner() != scope.owner() || self.scope.brand().key() != scope.brand().key() {
            return Err(AuthDocumentError::Conflict);
        }
        #[cfg(test)]
        BEFORE_BINDING_CANDIDATE.with(|slot| {
            let hook = slot.borrow_mut().take();
            if let Some(hook) = hook {
                hook();
            }
        });
        scope.recheck_runtime_installation(
            self.directory.canonical_path(),
            &self.parent,
            self.scope.brand(),
            &scope_context(context),
        )?;
        self.check(context)?;
        self.directory
            .verify_private_handle(&self.parent, context)?;
        context.check()?;
        Ok(())
    }

    /// Share the existing publisher lock for a short credential inspection.
    /// Concurrent inspections coexist; publication still requires an exclusive lock.
    /// Missing document and lock remain absent; a present document requires its lock.
    /// The caller validates data and this guard before releasing it, before any await.
    pub fn read_locked<'a>(
        &'a self,
        context: &'a SnapshotContext,
    ) -> Result<(Option<NativeDocumentLock<'a>>, AuthDocumentSnapshot), AuthDocumentError> {
        self.check(context)?;
        match NativeDocumentLock::acquire_existing(
            &self.directory,
            OsStr::new(publication::LOCK),
            context,
        ) {
            Ok(lock) => {
                self.check(context)?;
                let observation = self.read(context)?;
                lock.revalidate()?;
                Ok((Some(lock), observation))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut absent = self.read(context)?;
                self.revalidate(&mut absent, context)?;
                if absent.bytes().is_some() {
                    return Err(AuthDocumentError::Conflict);
                }
                Ok((None, absent))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Missing auth.json creates nothing. The bytes remain protected and borrowed.
    pub fn read(
        &self,
        context: &SnapshotContext,
    ) -> Result<AuthDocumentSnapshot, AuthDocumentError> {
        self.read_inner(context, || {})
    }

    fn read_inner(
        &self,
        context: &SnapshotContext,
        after_read: impl FnOnce(),
    ) -> Result<AuthDocumentSnapshot, AuthDocumentError> {
        self.read_attempt(context, after_read, || {})
    }

    fn read_attempt(
        &self,
        context: &SnapshotContext,
        after_read: impl FnOnce(),
        after_attempt: impl FnOnce(),
    ) -> Result<AuthDocumentSnapshot, AuthDocumentError> {
        self.check(context)?;
        let path = self.directory.path(OsStr::new("auth.json"))?;
        // Namespace and directory privacy fence the complete read below. The
        // exact-handle reader checks file identity/privacy, bytes and EOF itself;
        // its inner checks need only the original cancellation and deadline.
        let check = || context.check();
        let access = self.access(context, &check);
        let result = CapturedFile::open(&path, &access, after_read);
        after_attempt();
        // A failed content read still needs the final scope/privacy/context check.
        self.check(context)?;
        Ok(AuthDocumentSnapshot {
            file: result?,
            store_identity: Arc::clone(&self.identity),
            request_identity: context.request_identity(),
        })
    }

    /// Rechecks a real observation from this store and the same original request.
    /// The original shared request is allowed; a newly created context is not.
    /// This is cooperative change detection, not hostile-filesystem atomic CAS.
    pub fn revalidate(
        &self,
        snapshot: &mut AuthDocumentSnapshot,
        context: &SnapshotContext,
    ) -> Result<(), AuthDocumentError> {
        context.check()?;
        if !Arc::ptr_eq(&self.identity, &snapshot.store_identity)
            || !snapshot.request_identity.matches(context)
        {
            return Err(AuthDocumentError::Conflict);
        }
        self.check(context)?;
        let path = self.directory.path(OsStr::new("auth.json"))?;
        // Keep the same namespace fences as read_attempt without repeating the
        // ancestor walk at every metadata probe and read of the retained file.
        let check = || context.check();
        let access = self.access(context, &check);
        let result = if let Some(file) = &mut snapshot.file {
            file.revalidate(&path, &access)
        } else {
            private_document::verify_closed(&path, None, &access)
        };
        self.check(context)?;
        result.map_err(Into::into)
    }

    fn access<'a>(
        &'a self,
        context: &'a SnapshotContext,
        check: &'a dyn Fn() -> io::Result<()>,
    ) -> Access<'a> {
        Access {
            limit: self.limits.bytes,
            access_limit: self.limits.access_bytes,
            context: Some(context),
            check,
        }
    }

    fn check(&self, context: &SnapshotContext) -> Result<(), AuthDocumentError> {
        self.check_io(context).map_err(Into::into)
    }
    fn check_io(&self, context: &SnapshotContext) -> io::Result<()> {
        context.check()?;
        self.scope
            .revalidate_installation(&scope_context(context))?;
        self.directory.revalidate(context)?;
        graphit_fs::verify_private_directory(&self.parent)?;
        context.check()
    }
}

fn scope_context(context: &SnapshotContext) -> ScopeContext {
    ScopeContext::from_snapshot(context)
}

/// Protected bytes and original physical handle. There is no deserializer or grant.
///
/// ```compile_fail
/// use graphit_core::auth_document::AuthDocumentSnapshot;
/// let _: AuthDocumentSnapshot = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::AuthDocumentSnapshot;
/// fn duplicate(snapshot: &AuthDocumentSnapshot) { let _ = snapshot.clone(); let _: AuthDocumentSnapshot = Clone::clone(snapshot); }
/// ```
/// ```compile_fail
/// use graphit_core::auth_document::AuthDocumentSnapshot;
/// fn expose(snapshot: &AuthDocumentSnapshot) { println!("{snapshot:?}"); }
/// ```
pub struct AuthDocumentSnapshot {
    file: Option<CapturedFile>,
    store_identity: Arc<()>,
    request_identity: RequestIdentity,
}
impl AuthDocumentSnapshot {
    pub fn bytes(&self) -> Option<&[u8]> {
        self.file.as_ref().map(|file| file.value.bytes.as_slice())
    }
}

#[cfg(test)]
thread_local! {
    static BEFORE_BINDING_CANDIDATE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
mod tests;
