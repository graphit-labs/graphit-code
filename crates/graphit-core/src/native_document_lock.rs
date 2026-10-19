//! Permanent private locks in an already captured directory.
//! Namespace selection and document ownership belong to the caller.

use crate::{
    file_io::SnapshotContext,
    file_lock::FileLock,
    native_document_file::CapturedDocumentDirectory,
    native_file_identity::{PhysicalId, handle_id, physical_id},
};
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io,
    path::Path,
};

/// Owns the exact kernel lock and borrows its captured directory/request.
/// Drop unlocks and closes handles; it never deletes or replaces an entry.
/// Revalidate before the protected operation. This guard grants no Stage authority.
///
/// ```compile_fail
/// use graphit_core::native_document_lock::NativeDocumentLock;
/// let _: NativeDocumentLock<'static> = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use graphit_core::native_document_lock::NativeDocumentLock;
/// fn duplicate(lock: &NativeDocumentLock<'_>) { let _ = Clone::clone(lock); }
/// ```
/// ```compile_fail
/// use graphit_core::native_document_lock::NativeDocumentLock;
/// fn expose(lock: &NativeDocumentLock<'_>) { println!("{lock:?}"); }
/// ```
#[must_use = "dropping the guard releases the kernel lock"]
pub struct NativeDocumentLock<'a> {
    guard: FileLock,
    directory: &'a CapturedDocumentDirectory,
    context: &'a SnapshotContext,
    parent: File,
    parent_identity: PhysicalId,
    file_identity: PhysicalId,
}

impl<'a> NativeDocumentLock<'a> {
    /// A new entry is private before content; existing entries must be empty/private.
    /// Names and paths use the captured directory's admission limits.
    pub fn acquire(
        directory: &'a CapturedDocumentDirectory,
        name: &OsStr,
        context: &'a SnapshotContext,
    ) -> io::Result<Self> {
        Self::acquire_checked(directory, name, context, true).map_err(static_error)
    }

    /// Inspect under a shared publisher lock without creating an entry.
    /// This guard exposes only physical revalidation, never write authority.
    pub(crate) fn acquire_existing(
        directory: &'a CapturedDocumentDirectory,
        name: &OsStr,
        context: &'a SnapshotContext,
    ) -> io::Result<Self> {
        Self::acquire_checked(directory, name, context, false).map_err(static_error)
    }

    fn acquire_checked(
        directory: &'a CapturedDocumentDirectory,
        name: &OsStr,
        context: &'a SnapshotContext,
        create: bool,
    ) -> io::Result<Self> {
        let path = directory.path(name)?;
        let parent = directory.private_handle(context)?;
        let parent_identity = handle_id(&parent)?;
        context.check()?;
        let file = if create {
            match graphit_fs::create_private_lock_file(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => open_existing(&path)?,
                Err(error) => return Err(error),
            }
        } else {
            open_existing(&path)?
        };
        directory.revalidate(context)?;
        graphit_fs::verify_private_directory(&parent)?;
        let file_identity = verify_file(&file, &path)?;
        context.check()?;
        let guard = if !create {
            FileLock::acquire_shared_owned_file(
                file,
                path,
                context.deadline(),
                context.cancellation(),
            )?
        } else {
            FileLock::acquire_owned_file(file, path, context.deadline(), context.cancellation())?
        };
        let lock = Self {
            guard,
            directory,
            context,
            parent,
            parent_identity,
            file_identity,
        };
        lock.revalidate_checked()?;
        Ok(lock)
    }

    /// Detects observed path, identity, privacy, cancellation and deadline changes.
    /// Cooperative checks are not a filesystem CAS or syscall preemption.
    pub fn revalidate(&self) -> io::Result<()> {
        self.revalidate_checked().map_err(static_error)
    }

    fn revalidate_checked(&self) -> io::Result<()> {
        self.directory.revalidate(self.context)?;
        if handle_id(&self.parent)? != self.parent_identity {
            return Err(conflict());
        }
        graphit_fs::verify_private_directory(&self.parent)?;
        if verify_file(self.guard.file(), self.guard.path())? != self.file_identity {
            return Err(conflict());
        }
        self.directory.revalidate(self.context)
    }
}

fn verify_file(file: &File, path: &Path) -> io::Result<PhysicalId> {
    graphit_fs::verify_private_file(file)?;
    let identity = handle_id(file)?;
    identity.validate()?;
    let current = fs::symlink_metadata(path)?;
    if !current.is_file()
        || current.file_type().is_symlink()
        || current.len() != 0
        || file.metadata()?.len() != 0
        || physical_id(path, &current)? != identity
    {
        return Err(conflict());
    }
    graphit_fs::verify_private_file(file)?;
    Ok(identity)
}

pub(crate) fn open_existing(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

fn conflict() -> io::Error {
    io::ErrorKind::InvalidData.into()
}
fn static_error(error: io::Error) -> io::Error {
    error.kind().into()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{file_lock::Cancellation, native_document_file::NativeDocumentFileLimits};
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn shared_inspections_coexist_and_exclude_publication() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("private");
        drop(graphit_fs::create_private_directory(&root).unwrap());
        let context = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let directory = CapturedDocumentDirectory::capture(
            &root,
            NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
            &context,
        )
        .unwrap();
        let name = OsStr::new("document.lock");
        assert_eq!(
            NativeDocumentLock::acquire_existing(&directory, name, &context)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!root.join(name).exists());
        drop(NativeDocumentLock::acquire(&directory, name, &context).unwrap());
        let first = NativeDocumentLock::acquire_existing(&directory, name, &context).unwrap();
        let second = NativeDocumentLock::acquire_existing(&directory, name, &context).unwrap();
        first.revalidate().unwrap();
        second.revalidate().unwrap();
        assert_eq!(
            FileLock::try_acquire(root.join(name)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(first);
        assert_eq!(
            FileLock::try_acquire(root.join(name)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(second);
        drop(FileLock::try_acquire(root.join(name)).unwrap());
    }

    #[test]
    fn existing_lock_never_creates_and_rejects_replacement_during_contention() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("private");
        drop(graphit_fs::create_private_directory(&root).unwrap());
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let directory = CapturedDocumentDirectory::capture(
            &root,
            NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
            &ctx,
        )
        .unwrap();
        let name = OsStr::new("document.lock");
        assert_eq!(
            NativeDocumentLock::acquire_existing(&directory, name, &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!root.join(name).exists());
        let held = NativeDocumentLock::acquire(&directory, name, &ctx).unwrap();
        let (send, receive) = mpsc::channel();
        thread::scope(|scope| {
            let waiting = scope.spawn(|| {
                crate::file_lock::BEFORE_WAIT.with(|hook| {
                    *hook.borrow_mut() = Some(Box::new(move || send.send(()).unwrap()));
                });
                NativeDocumentLock::acquire_existing(&directory, name, &ctx)
                    .err()
                    .unwrap()
                    .kind()
            });
            receive.recv_timeout(Duration::from_secs(10)).unwrap();
            fs::rename(root.join(name), root.join("original.lock")).unwrap();
            drop(graphit_fs::create_private_lock_file(&root.join(name)).unwrap());
            drop(held);
            assert_eq!(waiting.join().unwrap(), io::ErrorKind::InvalidData);
        });
        drop(NativeDocumentLock::acquire_existing(&directory, name, &ctx).unwrap());
    }

    #[test]
    fn existing_lock_contention_preserves_original_cancellation_and_deadline() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("private");
        drop(graphit_fs::create_private_directory(&root).unwrap());
        let holder = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let directory = CapturedDocumentDirectory::capture(
            &root,
            NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
            &holder,
        )
        .unwrap();
        let name = OsStr::new("document.lock");
        let held = NativeDocumentLock::acquire(&directory, name, &holder).unwrap();
        for cancel in [true, false] {
            let cancellation = Cancellation::default();
            let deadline = Instant::now()
                + if cancel {
                    Duration::from_secs(10)
                } else {
                    Duration::from_millis(100)
                };
            let original = SnapshotContext::new(deadline, cancellation.clone());
            let (send, receive) = mpsc::channel();
            thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    crate::file_lock::BEFORE_WAIT.with(|hook| {
                        *hook.borrow_mut() = Some(Box::new(move || send.send(()).unwrap()));
                    });
                    NativeDocumentLock::acquire_existing(&directory, name, &original)
                        .err()
                        .unwrap()
                        .kind()
                });
                receive.recv_timeout(Duration::from_secs(5)).unwrap();
                if cancel {
                    cancellation.cancel();
                }
                assert_eq!(
                    worker.join().unwrap(),
                    if cancel {
                        io::ErrorKind::Interrupted
                    } else {
                        io::ErrorKind::TimedOut
                    }
                );
            });
            assert_eq!(original.deadline(), Some(deadline));
            held.revalidate().unwrap();
        }
    }

    #[test]
    fn replacement_during_actual_kernel_contention_cannot_return_a_guard() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("private");
        drop(graphit_fs::create_private_directory(&root).unwrap());
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let directory = CapturedDocumentDirectory::capture(
            &root,
            NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
            &ctx,
        )
        .unwrap();
        let name = OsStr::new("document.lock");
        let held = NativeDocumentLock::acquire(&directory, name, &ctx).unwrap();
        let (send, receive) = mpsc::channel();
        thread::scope(|scope| {
            let waiting = scope.spawn(|| {
                crate::file_lock::BEFORE_WAIT.with(|hook| {
                    *hook.borrow_mut() = Some(Box::new(move || send.send(()).unwrap()));
                });
                NativeDocumentLock::acquire(&directory, name, &ctx)
                    .err()
                    .unwrap()
                    .kind()
            });
            receive.recv_timeout(Duration::from_secs(10)).unwrap();
            fs::rename(root.join(name), root.join("original.lock")).unwrap();
            drop(graphit_fs::create_private_lock_file(&root.join(name)).unwrap());
            drop(held);
            assert_eq!(waiting.join().unwrap(), io::ErrorKind::InvalidData);
        });
        assert!(root.join("original.lock").exists());
        drop(NativeDocumentLock::acquire(&directory, name, &ctx).unwrap());
    }

    #[test]
    fn cancellation_after_actual_contention_wakes_the_waiter_and_preserves_holder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("private");
        drop(graphit_fs::create_private_directory(&root).unwrap());
        let token = Cancellation::default();
        let ctx = SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        );
        let waiter = SnapshotContext::new(Instant::now() + Duration::from_secs(30), token.clone());
        let directory = CapturedDocumentDirectory::capture(
            &root,
            NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
            &ctx,
        )
        .unwrap();
        let name = OsStr::new("document.lock");
        let held = NativeDocumentLock::acquire(&directory, name, &ctx).unwrap();
        let (send, receive) = mpsc::channel();
        thread::scope(|scope| {
            let waiting = scope.spawn(|| {
                crate::file_lock::BEFORE_WAIT.with(|hook| {
                    *hook.borrow_mut() = Some(Box::new(move || send.send(()).unwrap()));
                });
                NativeDocumentLock::acquire(&directory, name, &waiter)
                    .err()
                    .unwrap()
                    .kind()
            });
            receive.recv_timeout(Duration::from_secs(10)).unwrap();
            token.cancel();
            assert_eq!(waiting.join().unwrap(), io::ErrorKind::Interrupted);
        });
        held.revalidate().unwrap();
    }
}
