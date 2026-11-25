//! Private authentication document data at a configured conventional path.
//! Authorization and the profile schema belong to Auth.
#[cfg(feature = "auth-test-support")]
use crate::file_io::RequestIdentity;
use crate::{file_io::SnapshotContext, file_lock::FileLock, private_document};
use secrecy::zeroize::Zeroizing;
#[cfg(feature = "auth-test-support")]
use std::sync::Arc;
use std::{
    ffi::OsStr,
    fmt, io,
    path::{Component, Path, PathBuf},
};

pub mod publication;
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

#[derive(Clone, Copy)]
pub struct AuthDocumentLimits {
    bytes: usize,
}
impl AuthDocumentLimits {
    pub fn new(bytes: usize) -> Result<Self, AuthDocumentError> {
        private_document::validate_limit(bytes).map_err(|_| AuthDocumentError::InvalidPolicy)?;
        Ok(Self { bytes })
    }
}

pub struct AuthDocumentStore {
    root: PathBuf,
    limits: AuthDocumentLimits,
    #[cfg(feature = "auth-test-support")]
    identity: Arc<()>,
}

/// Current bytes and the short publisher lock; dropping performs no publication.
#[must_use]
pub struct AuthDocumentMutation<'a> {
    store: &'a AuthDocumentStore,
    original: &'a SnapshotContext,
    _lock: FileLock,
    snapshot: AuthDocumentSnapshot,
}

impl AuthDocumentStore {
    pub fn new(
        root: PathBuf,
        limits: AuthDocumentLimits,
        original: &SnapshotContext,
    ) -> Result<Self, AuthDocumentError> {
        let store = Self {
            root,
            limits,
            #[cfg(feature = "auth-test-support")]
            identity: Arc::new(()),
        };
        store.check(original)?;
        Ok(store)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn try_profile_refresh_lock(
        &self,
        lock_basename: &OsStr,
        original: &SnapshotContext,
    ) -> Result<Option<FileLock>, AuthDocumentError> {
        self.check(original)?;
        let mut components = Path::new(lock_basename).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(AuthDocumentError::InvalidPolicy);
        }
        private_document::try_lock(&self.root.join(lock_basename), original).map_err(Into::into)
    }
    pub fn read_locked(
        &self,
        original: &SnapshotContext,
    ) -> Result<(FileLock, AuthDocumentSnapshot), AuthDocumentError> {
        let lock = private_document::lock(&self.root.join(publication::LOCK), original, true)?;
        Ok((lock, self.read(original)?))
    }
    pub fn read(
        &self,
        original: &SnapshotContext,
    ) -> Result<AuthDocumentSnapshot, AuthDocumentError> {
        self.check(original)?;
        Ok(AuthDocumentSnapshot {
            bytes: private_document::read(
                &self.root.join("auth.json"),
                self.limits.bytes,
                original,
            )?,
        })
    }
    fn check(&self, original: &SnapshotContext) -> Result<(), AuthDocumentError> {
        original.check()?;
        private_document::private_directory(&self.root)?;
        Ok(())
    }
}

/// Secret bytes used as the expected content in a short compare-and-swap.
pub struct AuthDocumentSnapshot {
    bytes: Option<Zeroizing<Vec<u8>>>,
}
impl AuthDocumentSnapshot {
    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref().map(Vec::as_slice)
    }
}
#[cfg(test)]
mod tests;
