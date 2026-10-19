//! Kernel file locks with bounded, cancellable waits and ownership-based release.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// A permanent cancellation signal shared by clones; cancellation wakes waiters.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<CancelState>);

#[derive(Debug, Default)]
struct CancelState {
    cancelled: Mutex<bool>,
    wake: Condvar,
    #[cfg(feature = "async-cancel")]
    async_wake: tokio::sync::Notify,
}

impl Cancellation {
    pub fn cancel(&self) {
        *self.0.cancelled.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.0.wake.notify_all();
        #[cfg(feature = "async-cancel")]
        self.0.async_wake.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.cancelled.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Await the same permanent signal used by synchronous file-lock waiters.
    #[cfg(feature = "async-cancel")]
    pub async fn cancelled(&self) {
        // notify_waiters also wakes Notified futures created before their first poll.
        let notified = self.0.async_wake.notified();
        if !self.is_cancelled() {
            notified.await;
        }
    }

    pub(crate) fn wait(&self, duration: Duration) -> bool {
        let state = self.0.cancelled.lock().unwrap_or_else(|e| e.into_inner());
        let (state, _) = self
            .0
            .wake
            .wait_timeout_while(state, duration, |cancelled| !*cancelled)
            .unwrap_or_else(|e| e.into_inner());
        *state
    }
}

/// Ownership of one kernel file lock. Drop unlocks before closing the handle.
///
/// Do not unlink or replace a lock file: contenders must open the same inode.
#[derive(Debug)]
#[must_use = "dropping the guard releases the lock"]
pub struct FileLock {
    file: File,
    path: PathBuf,
    locked: bool,
}

impl FileLock {
    pub fn try_acquire(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let file = open(path, true)?;
        try_lock(&file)?;
        Ok(Self {
            file,
            path: path.to_owned(),
            locked: true,
        })
    }

    /// Attempts immediately, then waits at most `timeout` for contention.
    /// Cancelled waits return `Interrupted`; expired waits return `TimedOut`.
    pub fn acquire(
        path: impl AsRef<Path>,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<Self> {
        Self::acquire_with_parent(path.as_ref(), timeout, cancellation, true)
    }

    /// A qualified control root must never be recreated by lock acquisition.
    pub(crate) fn acquire_in_existing_parent(
        path: &Path,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> io::Result<Self> {
        Self::acquire_with_parent(path, timeout, cancellation, false)
    }

    /// Wait against the original deadline without recreating the lock parent.
    pub fn acquire_in_existing_parent_until(
        path: &Path,
        deadline: Option<Instant>,
        cancellation: &Cancellation,
    ) -> io::Result<Self> {
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let file = open(path, false)?;
        Self::acquire_owned_file(file, path.to_owned(), deadline, cancellation)
    }

    fn acquire_with_parent(
        path: &Path,
        timeout: Duration,
        cancellation: &Cancellation,
        create_parent: bool,
    ) -> io::Result<Self> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "timeout is too large"))?;
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let file = open(path, create_parent)?;
        Self::acquire_owned_file(file, path.to_owned(), Some(deadline), cancellation)
    }

    /// Internal callers qualify the exact handle before handing over ownership.
    pub(crate) fn acquire_owned_file(
        file: File,
        path: PathBuf,
        deadline: Option<Instant>,
        cancellation: &Cancellation,
    ) -> io::Result<Self> {
        Self::acquire_owned_file_checked(file, path, deadline, cancellation, &mut || Ok(()))
    }

    /// One physical acquisition loop; native callers admit every attempt and wait
    /// against the original request before either action. No deadline is renewed.
    pub(crate) fn acquire_owned_file_checked(
        file: File,
        path: PathBuf,
        deadline: Option<Instant>,
        cancellation: &Cancellation,
        before: &mut impl FnMut() -> io::Result<()>,
    ) -> io::Result<Self> {
        Self::acquire_owned_file_with(file, path, deadline, cancellation, before, try_lock)
    }

    /// Readers share the publisher's lock; exclusive publication still excludes them.
    pub(crate) fn acquire_shared_owned_file(
        file: File,
        path: PathBuf,
        deadline: Option<Instant>,
        cancellation: &Cancellation,
    ) -> io::Result<Self> {
        Self::acquire_owned_file_with(file, path, deadline, cancellation, &mut || Ok(()), |file| {
            file.try_lock_shared().map_err(lock_error)
        })
    }

    fn acquire_owned_file_with(
        file: File,
        path: PathBuf,
        deadline: Option<Instant>,
        cancellation: &Cancellation,
        before: &mut impl FnMut() -> io::Result<()>,
        try_lock: impl Fn(&File) -> io::Result<()>,
    ) -> io::Result<Self> {
        loop {
            before()?;
            if cancellation.is_cancelled() {
                return Err(cancelled());
            }
            match try_lock(&file) {
                Ok(()) => {
                    return Ok(Self {
                        file,
                        path,
                        locked: true,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            #[cfg(test)]
            BEFORE_WAIT.with(|hook| {
                if let Some(hook) = hook.borrow_mut().take() {
                    hook();
                }
            });
            let remaining =
                deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
            if remaining.is_some_and(|remaining| remaining.is_zero()) {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "lock wait expired"));
            }
            before()?;
            if cancellation.wait(remaining.map_or(Duration::from_millis(50), |remaining| {
                remaining.min(Duration::from_millis(50))
            })) {
                return Err(cancelled());
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "lock wait expired"));
            }
        }
    }

    pub(crate) fn file(&self) -> &File {
        &self.file
    }

    /// Probes without creating a file, truncating it, or changing its contents.
    pub fn is_locked(path: impl AsRef<Path>) -> io::Result<bool> {
        let file = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        match try_lock(&file) {
            Ok(()) => {
                file.unlock()?;
                Ok(false)
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(true),
            Err(error) => Err(error),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Releases ownership now and reports an unlock error, if any.
    pub fn release(mut self) -> io::Result<()> {
        self.file.unlock()?;
        self.locked = false;
        Ok(())
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        if self.locked {
            // Fork/dup can retain the open file description beyond this owner.
            let _ = self.file.unlock();
        }
    }
}

fn open(path: &Path, create_parent: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Err(error) if create_parent && error.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
                options.open(path)
            } else {
                Err(error)
            }
        }
        result => result,
    }
}

fn try_lock(file: &File) -> io::Result<()> {
    file.try_lock().map_err(lock_error)
}

fn lock_error(error: TryLockError) -> io::Error {
    match error {
        TryLockError::WouldBlock => io::Error::new(io::ErrorKind::WouldBlock, "lock is held"),
        TryLockError::Error(error) => error,
    }
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "lock wait cancelled")
}

#[cfg(test)]
thread_local! {
    pub(crate) static BEFORE_WAIT: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_parent_acquisition_never_recreates_a_missing_control_directory() {
        let fixture = tempfile::tempdir().unwrap();
        let parent = fixture.path().join("missing");
        let path = parent.join("lock");
        let error = FileLock::acquire_in_existing_parent(
            &path,
            Duration::from_secs(1),
            &Cancellation::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!parent.exists());
        let lock =
            FileLock::acquire(&path, Duration::from_secs(1), &Cancellation::default()).unwrap();
        assert!(parent.is_dir());
        lock.release().unwrap();
    }

    #[test]
    fn retained_descriptor_cannot_extend_owned_lock_lifetime() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().join("lock");
        let guard = FileLock::try_acquire(&path).unwrap();
        // Models an inherited open file description; this handle stays private.
        let retained = guard.file.try_clone().unwrap();
        drop(guard);
        let next = FileLock::try_acquire(&path).unwrap();
        drop(retained);
        drop(next);
    }
}

#[cfg(test)]
mod native_original_acquisition_tests {
    use super::*;
    use crate::file_io::SnapshotContext;

    fn request(cancellation: Cancellation) -> std::sync::Arc<crate::file_io::SnapshotContext> {
        Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            cancellation,
        ))
    }

    #[test]
    fn original_cancel_denies_before_first_kernel_attempt() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("fixture.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let cancellation = Cancellation::default();
        let original = request(cancellation.clone());
        cancellation.cancel();
        let result = FileLock::acquire_owned_file_checked(
            file,
            path.clone(),
            original.deadline(),
            original.cancellation(),
            &mut || original.check(),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
        let guard = FileLock::try_acquire(&path).unwrap();
        drop(guard);
    }

    #[test]
    fn service_contention_waits_without_deadline_and_cancellation_wakes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("service.lock");
        let held = FileLock::try_acquire(&path).unwrap();
        let original = Arc::new(SnapshotContext::service(Cancellation::default()));
        let signal = original.cancellation().clone();
        let (ready, waiting) = std::sync::mpsc::channel();
        let worker_path = path.clone();
        let worker = std::thread::spawn(move || {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&worker_path)
                .unwrap();
            BEFORE_WAIT.with(|hook| {
                *hook.borrow_mut() = Some(Box::new(move || {
                    ready.send(()).unwrap();
                }))
            });
            FileLock::acquire_owned_file_checked(
                file,
                worker_path,
                original.deadline(),
                original.cancellation(),
                &mut || original.check(),
            )
        });
        waiting.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!worker.is_finished());
        signal.cancel();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(FileLock::is_locked(&path).unwrap());
        drop(held);
        assert!(!FileLock::is_locked(&path).unwrap());
    }
    #[test]
    fn actual_contention_observes_deadline_and_preserves_held_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("fixture.lock");
        let held = FileLock::try_acquire(&path).unwrap();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(20);
        let original = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
        let result = FileLock::acquire_owned_file_checked(
            file,
            path.clone(),
            Some(deadline),
            original.cancellation(),
            &mut || original.check(),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert_eq!(original.deadline(), Some(deadline));
        assert!(FileLock::is_locked(&path).unwrap());
        drop(held);
        assert!(!FileLock::is_locked(&path).unwrap());
    }
}
