#![deny(unsafe_code)]
#![forbid(unsafe_op_in_unsafe_fn)]
//! Native link creation and exclusive publication of prepared entries.
//!
//! This provider does not publish membership. Native
//! synchronous filesystem calls cannot be preempted by a deadline.
use std::{fmt, io, path::Path};

mod exclusive_rename;
pub use exclusive_rename::rename_exclusive;

mod native_principal;
pub use native_principal::{NativePrincipalError, NativePrincipalObservation};

mod private_file;
#[cfg(windows)]
pub use private_file::{create_private_database_directory, verify_private_database_directory};
pub use private_file::{
    create_private_database_file, create_private_directory, create_private_file,
    create_private_lock_file, verify_private_database_directory_path, verify_private_directory,
    verify_private_directory_path, verify_private_file, verify_private_file_path,
};
#[cfg(target_os = "macos")]
mod macos_private_file;
#[cfg(target_os = "macos")]
mod macos_private_sdk;
#[cfg(windows)]
mod windows_private_file;

#[cfg(target_os = "macos")]
mod macos_access;
#[cfg(any(target_os = "macos", test))]
mod macos_access_buffer;
#[cfg(target_os = "macos")]
pub(crate) use macos_access::macos_access_snapshot;

#[cfg(any(windows, test))]
mod junction_buffer;
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStage {
    Source,
    Symlink,
    JunctionBuffer,
    JunctionDirectory,
    JunctionOpen,
    JunctionSet,
}

/// Fixed diagnostics. Paths and native error payloads are retained by the caller's
/// context, never included in this error. Cleanup failure means a residual *may*
/// remain; it does not establish its existence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkFailure {
    stage: LinkStage,
    kind: io::ErrorKind,
    cleanup: Option<io::ErrorKind>,
}
impl LinkFailure {
    fn new(stage: LinkStage, kind: io::ErrorKind) -> Self {
        Self {
            stage,
            kind,
            cleanup: None,
        }
    }
    pub fn stage(self) -> LinkStage {
        self.stage
    }
    pub fn kind(self) -> io::ErrorKind {
        self.kind
    }
    pub fn cleanup_kind(self) -> Option<io::ErrorKind> {
        self.cleanup
    }
}
impl fmt::Display for LinkFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native link {:?}: {:?}", self.stage, self.kind)?;
        if let Some(kind) = self.cleanup {
            write!(f, "; staging cleanup failed: {kind:?}")?;
        }
        Ok(())
    }
}
impl std::error::Error for LinkFailure {}

/// Create a link through ordinary native path resolution without replacing an entry.
/// The current source type selects the Windows file/directory operation.
/// Paths and aliases are not captured or pinned; native errors propagate.
pub fn create_link(source: &Path, destination: &Path) -> Result<(), LinkFailure> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, destination).map_err(|e| failure(LinkStage::Symlink, e))
    }
    #[cfg(windows)]
    {
        windows::create(source, destination)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(LinkFailure::new(
            LinkStage::Symlink,
            io::ErrorKind::Unsupported,
        ))
    }
}

fn failure(stage: LinkStage, error: io::Error) -> LinkFailure {
    LinkFailure::new(stage, error.kind())
}
#[cfg(any(windows, test))]
fn directory_link_using(
    symlink: impl FnOnce() -> io::Result<()>,
    junction: impl FnOnce() -> Result<(), LinkFailure>,
) -> Result<(), LinkFailure> {
    match symlink() {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(1314) => junction(),
        Err(e) => Err(failure(LinkStage::Symlink, e)),
    }
}

#[cfg(any(windows, test))]
fn junction_using(
    create: impl FnOnce() -> io::Result<()>,
    setup: impl FnOnce() -> Result<(), LinkFailure>,
    cleanup: impl FnOnce() -> io::Result<()>,
) -> Result<(), LinkFailure> {
    create().map_err(|e| failure(LinkStage::JunctionDirectory, e))?;
    match setup() {
        Ok(()) => Ok(()),
        Err(mut primary) => {
            primary.cleanup = cleanup().err().map(|e| e.kind());
            Err(primary)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, fs};
    #[test]
    fn dispatcher_attempts_junction_only_for_1314() {
        for code in [None, Some(1314), Some(5), Some(123)] {
            let calls = Cell::new(0);
            let result = directory_link_using(
                || code.map_or(Ok(()), |c| Err(io::Error::from_raw_os_error(c))),
                || {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
            );
            assert_eq!(calls.get(), u32::from(code == Some(1314)));
            match code {
                None | Some(1314) => assert_eq!(result, Ok(())),
                _ => assert_eq!(result.unwrap_err().stage(), LinkStage::Symlink),
            }
        }
        let expected = LinkFailure::new(LinkStage::JunctionSet, io::ErrorKind::PermissionDenied);
        assert_eq!(
            directory_link_using(|| Err(io::Error::from_raw_os_error(1314)), || Err(expected)),
            Err(expected)
        );
    }
    #[test]
    fn cleanup_preserves_primary_and_secondary_failures() {
        for stage in [LinkStage::JunctionOpen, LinkStage::JunctionSet] {
            for cleanup_error in [None, Some(io::ErrorKind::PermissionDenied)] {
                let cleaned = Cell::new(0);
                let result = junction_using(
                    || Ok(()),
                    || Err(LinkFailure::new(stage, io::ErrorKind::Other)),
                    || {
                        cleaned.set(cleaned.get() + 1);
                        cleanup_error.map_or(Ok(()), |k| Err(io::Error::from(k)))
                    },
                );
                let error = result.unwrap_err();
                assert_eq!(
                    (error.stage(), error.kind(), error.cleanup_kind()),
                    (stage, io::ErrorKind::Other, cleanup_error)
                );
                assert_eq!(cleaned.get(), 1);
                assert!(!error.to_string().contains("/"));
            }
        }
        let setup = Cell::new(false);
        let cleanup = Cell::new(false);
        let error = junction_using(
            || Err(io::Error::from(io::ErrorKind::AlreadyExists)),
            || {
                setup.set(true);
                Ok(())
            },
            || {
                cleanup.set(true);
                Ok(())
            },
        )
        .unwrap_err();
        assert_eq!(error.stage(), LinkStage::JunctionDirectory);
        assert!(!setup.get() && !cleanup.get());
        junction_using(
            || Ok(()),
            || Ok(()),
            || panic!("successful setup must not clean up"),
        )
        .unwrap();
    }

    #[test]
    fn failed_junction_setup_cleans_only_the_created_empty_staging_directory() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let staging = root.path().join("staging");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"original").unwrap();
        let error = junction_using(
            || fs::create_dir(&staging),
            || {
                Err(LinkFailure::new(
                    LinkStage::JunctionSet,
                    io::ErrorKind::PermissionDenied,
                ))
            },
            || fs::remove_dir(&staging),
        )
        .unwrap_err();
        assert_eq!(error.cleanup_kind(), None);
        assert!(!staging.exists());
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"original");
        let error = junction_using(
            || fs::create_dir(&staging),
            || {
                fs::write(staging.join("unexpected"), b"preserve").unwrap();
                Err(LinkFailure::new(
                    LinkStage::JunctionOpen,
                    io::ErrorKind::Other,
                ))
            },
            || fs::remove_dir(&staging),
        )
        .unwrap_err();
        assert!(error.cleanup_kind().is_some());
        assert_eq!(fs::read(staging.join("unexpected")).unwrap(), b"preserve");
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"original");
    }
}
