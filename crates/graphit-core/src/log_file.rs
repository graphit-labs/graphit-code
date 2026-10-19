//! Cooperating, bounded UTF-8 line sinks. No persistent descriptor or worker.
//! Budgets and durability are caller policy; append is not a crash transaction.

use crate::{
    brand::portable_segment,
    file_lock::{Cancellation, FileLock},
    local_files::{directory_exists, regular_metadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Directories,
    Lock,
    Open,
    Read,
    Recover,
    Append,
    Compact,
    Sync,
    Unlock,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogError {
    InvalidOptions,
    InvalidPath,
    RecordLimit,
    InvalidRecord,
    Corrupt,
    Allocation,
    Io {
        stage: Stage,
        kind: io::ErrorKind,
        os_code: Option<i32>,
        rollback_failed: bool,
    },
}
impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "log file {self:?}")
    }
}
impl std::error::Error for LogError {}
impl LogError {
    pub(crate) fn from_io(stage: Stage, error: io::Error) -> Self {
        Self::Io {
            stage,
            kind: error.kind(),
            os_code: error.raw_os_error(),
            rollback_failed: false,
        }
    }
    fn into_io(self) -> io::Error {
        let kind = match self {
            Self::Io { kind, .. } => kind,
            Self::Corrupt => io::ErrorKind::InvalidData,
            Self::Allocation => io::ErrorKind::OutOfMemory,
            _ => io::ErrorKind::InvalidInput,
        };
        io::Error::new(kind, self)
    }
}

/// Managed locations create only their explicitly selected namespace directories.
#[derive(Clone)]
pub struct LogPaths {
    directory: PathBuf,
    filename: OsString,
    create: Vec<PathBuf>,
    runtime: Option<PathBuf>,
    digest: String,
}
impl fmt::Debug for LogPaths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LogPaths { explicit destination }")
    }
}
fn channel_name(channel: &str) -> Result<OsString, LogError> {
    if channel.is_empty()
        || channel.len() > 64
        || !channel
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        return Err(LogError::InvalidPath);
    }
    portable_segment(channel).map_err(|_| LogError::InvalidPath)?;
    Ok(format!("{channel}.log").into())
}
impl LogPaths {
    fn new(
        directory: PathBuf,
        filename: OsString,
        create: Vec<PathBuf>,
        runtime: Option<PathBuf>,
    ) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut digest = String::with_capacity(64);
        for byte in Sha256::digest(filename.as_encoded_bytes()) {
            digest.push(char::from(HEX[usize::from(byte >> 4)]));
            digest.push(char::from(HEX[usize::from(byte & 15)]));
        }
        Self {
            directory,
            filename,
            create,
            runtime,
            digest,
        }
    }
    pub fn project(
        paths: &ProjectPaths,
        project_id: crate::project_identity::ProjectId,
        channel: &str,
    ) -> Result<Self, LogError> {
        let filename = channel_name(channel)?;
        let runtime = paths
            .project_runtime(project_id, &[])
            .map_err(|e| LogError::from_io(Stage::Directories, e))?;
        let directory = runtime.join("logs");
        Ok(Self::new(
            directory.clone(),
            filename,
            vec![
                paths.global_root().to_owned(),
                paths.global_root().join("projects"),
                runtime.clone(),
                directory,
            ],
            Some(runtime),
        ))
    }
    pub fn installation(paths: &InstallationPaths, channel: &str) -> Result<Self, LogError> {
        let filename = channel_name(channel)?;
        let directory = paths.root().join("logs");
        Ok(Self::new(
            directory.clone(),
            filename,
            vec![paths.root().to_owned(), directory],
            None,
        ))
    }
    pub fn explicit(path: impl AsRef<Path>) -> Result<Self, LogError> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(LogError::InvalidPath);
        }
        let filename = path.file_name().ok_or(LogError::InvalidPath)?.to_owned();
        if filename
            .as_encoded_bytes()
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b".log-"))
        {
            return Err(LogError::InvalidPath);
        }
        let directory = fs::canonicalize(path.parent().ok_or(LogError::InvalidPath)?)
            .map_err(|e| LogError::from_io(Stage::Directories, e))?;
        if !directory.is_dir() {
            return Err(LogError::InvalidPath);
        }
        Ok(Self::new(directory, filename, Vec::new(), None))
    }
    pub fn path(&self) -> PathBuf {
        self.directory.join(&self.filename)
    }
    pub fn lock_path(&self) -> PathBuf {
        self.directory.join(format!(".log-{}.lock", self.digest))
    }
    fn temporary_prefix(&self) -> String {
        format!(".log-{}.compact-", self.digest)
    }
    fn prepare(&mut self) -> Result<(), LogError> {
        let mut runtime_directory = None;
        for (index, directory) in self.create.iter().enumerate() {
            if self.runtime.as_ref() == Some(directory) {
                runtime_directory = Some(
                    crate::local_files::open_or_create_private_directory(directory)
                        .map_err(|e| LogError::from_io(Stage::Directories, e))?,
                );
                continue;
            }
            if directory_exists(directory).map_err(|e| LogError::from_io(Stage::Directories, e))? {
                continue;
            }
            // A configured installation root may have missing parent directories.
            if index == 0
                && let Some(parent) = directory.parent()
            {
                fs::create_dir_all(parent).map_err(|e| LogError::from_io(Stage::Directories, e))?;
            }
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let mut builder = builder;
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(directory) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if !directory_exists(directory)
                        .map_err(|e| LogError::from_io(Stage::Directories, e))?
                    {
                        return Err(LogError::from_io(
                            Stage::Directories,
                            io::ErrorKind::NotFound.into(),
                        ));
                    }
                }
                Err(e) => return Err(LogError::from_io(Stage::Directories, e)),
            }
        }
        // A clone of prepared paths still has to qualify its project runtime.
        if runtime_directory.is_none()
            && let Some(runtime) = &self.runtime
        {
            runtime_directory = Some(
                crate::local_files::open_private_directory(runtime)
                    .map_err(|e| LogError::from_io(Stage::Directories, e))?,
            );
        }
        self.directory = fs::canonicalize(&self.directory)
            .map_err(|e| LogError::from_io(Stage::Directories, e))?;
        if !self.directory.is_dir() {
            return Err(LogError::InvalidPath);
        }
        self.create.clear();
        drop(runtime_directory);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_file_bytes: usize,
    pub max_record_bytes: usize,
    pub max_batch_bytes: usize,
    pub retain_bytes: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Durability {
    Buffered,
    SyncData,
}
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub limits: Limits,
    pub durability: Durability,
    pub lock_timeout: Duration,
}
impl Options {
    fn validate(self) -> Result<(), LogError> {
        let b = self.limits;
        if b.max_record_bytes == 0
            || b.max_batch_bytes < b.max_record_bytes
            || b.max_file_bytes < b.max_batch_bytes
            || b.retain_bytes > b.max_file_bytes - b.max_batch_bytes
            || b.retain_bytes.checked_add(1).is_none()
            || b.max_record_bytes.checked_add(1).is_none()
            || u64::try_from(b.max_file_bytes).is_err()
            || Instant::now().checked_add(self.lock_timeout).is_none()
        {
            return Err(LogError::InvalidOptions);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AppendOutcome {
    pub records: usize,
    pub compacted: bool,
    pub recovered_bytes: u64,
    pub discarded_bytes: u64,
}

#[derive(Clone)]
pub struct LogFile {
    paths: LogPaths,
    options: Options,
    cancellation: Cancellation,
}
impl fmt::Debug for LogFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogFile")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}
impl LogFile {
    pub fn open(
        mut paths: LogPaths,
        options: Options,
        cancellation: Cancellation,
    ) -> Result<Self, LogError> {
        options.validate()?;
        cancelled(&cancellation)?;
        paths.prepare()?;
        let log = Self {
            paths,
            options,
            cancellation,
        };
        log.with_lock(|| {
            let _ = log.open_active()?;
            log.cleanup_temporaries()
        })?;
        Ok(log)
    }
    pub fn paths(&self) -> &LogPaths {
        &self.paths
    }
    fn with_lock<T>(&self, operation: impl FnOnce() -> Result<T, LogError>) -> Result<T, LogError> {
        cancelled(&self.cancellation)?;
        if !directory_exists(&self.paths.directory)
            .map_err(|e| LogError::from_io(Stage::Directories, e))?
        {
            return Err(LogError::InvalidPath);
        }
        let lock_path = self.paths.lock_path();
        regular_metadata(&lock_path).map_err(|e| LogError::from_io(Stage::Lock, e))?;
        #[cfg(test)]
        tests::before_lock();
        let guard = FileLock::acquire_in_existing_parent(
            &lock_path,
            self.options.lock_timeout,
            &self.cancellation,
        )
        .map_err(|e| LogError::from_io(Stage::Lock, e))?;
        let result = operation();
        let unlock = guard
            .release()
            .map_err(|e| LogError::from_io(Stage::Unlock, e));
        match result {
            Err(e) => Err(e),
            Ok(value) => {
                unlock?;
                Ok(value)
            }
        }
    }
    fn open_active(&self) -> Result<File, LogError> {
        let path = self.paths.path();
        if regular_metadata(&path)
            .map_err(|e| LogError::from_io(Stage::Open, e))?
            .is_some_and(|m| m.permissions().readonly())
        {
            return Err(LogError::from_io(
                Stage::Open,
                io::ErrorKind::PermissionDenied.into(),
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true).append(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|e| LogError::from_io(Stage::Open, e))?;
        let metadata = file
            .metadata()
            .map_err(|e| LogError::from_io(Stage::Open, e))?;
        if !metadata.is_file() {
            return Err(LogError::InvalidPath);
        }
        if metadata.permissions().readonly() {
            return Err(LogError::from_io(
                Stage::Open,
                io::ErrorKind::PermissionDenied.into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o7777 != 0o600 {
                file.set_permissions(fs::Permissions::from_mode(0o600))
                    .map_err(|e| LogError::from_io(Stage::Open, e))?;
            }
        }
        Ok(file)
    }
    fn cleanup_temporaries(&self) -> Result<(), LogError> {
        let prefix = self.paths.temporary_prefix();
        for entry in
            fs::read_dir(&self.paths.directory).map_err(|e| LogError::from_io(Stage::Compact, e))?
        {
            let entry = entry.map_err(|e| LogError::from_io(Stage::Compact, e))?;
            let name = entry.file_name();
            let Some(suffix) = name.to_str().and_then(|s| s.strip_prefix(&prefix)) else {
                continue;
            };
            if suffix.len() == 16
                && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
                && entry
                    .file_type()
                    .map_err(|e| LogError::from_io(Stage::Compact, e))?
                    .is_file()
            {
                fs::remove_file(entry.path()).map_err(|e| LogError::from_io(Stage::Compact, e))?;
            }
        }
        Ok(())
    }
    fn temporary(&self) -> Result<tempfile::NamedTempFile, LogError> {
        let mut builder = tempfile::Builder::new();
        let prefix = self.paths.temporary_prefix();
        builder.prefix(&prefix).rand_bytes(16);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o600));
        }
        builder
            .tempfile_in(&self.paths.directory)
            .map_err(|e| LogError::from_io(Stage::Compact, e))
    }
    pub fn append(&self, bytes: &[u8]) -> Result<AppendOutcome, LogError> {
        if bytes.is_empty() {
            return Ok(AppendOutcome::default());
        }
        let limits = self.options.limits;
        if bytes.len() > limits.max_batch_bytes {
            return Err(LogError::RecordLimit);
        }
        if !bytes.ends_with(b"\n") || std::str::from_utf8(bytes).is_err() {
            return Err(LogError::InvalidRecord);
        }
        let mut records = 0;
        for line in bytes.split_inclusive(|b| *b == b'\n') {
            if line.len() > limits.max_record_bytes {
                return Err(LogError::RecordLimit);
            }
            records += 1;
        }
        self.with_lock(|| {
            let mut file = self.open_active()?;
            let recovered_bytes = recover(&mut file, limits.max_record_bytes)?;
            let length = file
                .metadata()
                .map_err(|e| LogError::from_io(Stage::Read, e))?
                .len();
            let fits = length
                .checked_add(bytes.len() as u64)
                .is_some_and(|n| n <= limits.max_file_bytes as u64);
            let mut outcome = AppendOutcome {
                records,
                recovered_bytes,
                ..AppendOutcome::default()
            };
            if fits {
                cancelled(&self.cancellation)?;
                append_to(
                    &mut file,
                    bytes,
                    length,
                    self.options.durability,
                    |file, length| file.set_len(length),
                    |file| file.sync_data(),
                )?;
            } else {
                self.cleanup_temporaries()?;
                let suffix = retained_suffix(&mut file, length, limits.retain_bytes)?;
                let mut temporary = self.temporary()?;
                temporary
                    .write_all(&suffix)
                    .and_then(|_| temporary.write_all(bytes))
                    .map_err(|e| LogError::from_io(Stage::Compact, e))?;
                if self.options.durability == Durability::SyncData {
                    temporary
                        .as_file()
                        .sync_data()
                        .map_err(|e| LogError::from_io(Stage::Sync, e))?;
                }
                cancelled(&self.cancellation)?;
                drop(file);
                persist(temporary, &self.paths)?;
                outcome.compacted = true;
                outcome.discarded_bytes = length - suffix.len() as u64;
            }
            Ok(outcome)
        })
    }
    pub fn sync(&self) -> Result<(), LogError> {
        self.with_lock(|| {
            self.open_active()?
                .sync_all()
                .map_err(|e| LogError::from_io(Stage::Sync, e))
        })
    }
}
impl Write for LogFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.append(bytes)
            .map(|_| bytes.len())
            .map_err(LogError::into_io)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.sync().map_err(LogError::into_io)
    }
}
fn cancelled(cancellation: &Cancellation) -> Result<(), LogError> {
    if cancellation.is_cancelled() {
        Err(LogError::from_io(
            Stage::Lock,
            io::ErrorKind::Interrupted.into(),
        ))
    } else {
        Ok(())
    }
}
fn append_to<W: Write>(
    writer: &mut W,
    bytes: &[u8],
    length: u64,
    durability: Durability,
    rollback: impl FnOnce(&mut W, u64) -> io::Result<()>,
    sync: impl FnOnce(&mut W) -> io::Result<()>,
) -> Result<(), LogError> {
    if let Err(error) = writer.write_all(bytes) {
        let rollback_failed = rollback(writer, length).is_err();
        return Err(LogError::Io {
            stage: Stage::Append,
            kind: error.kind(),
            os_code: error.raw_os_error(),
            rollback_failed,
        });
    }
    if durability == Durability::SyncData {
        sync(writer).map_err(|e| LogError::from_io(Stage::Sync, e))?;
    }
    Ok(())
}
fn persist(temporary: tempfile::NamedTempFile, paths: &LogPaths) -> Result<(), LogError> {
    let metadata = regular_metadata(&paths.path())
        .map_err(|e| LogError::from_io(Stage::Compact, e))?
        .ok_or(LogError::InvalidPath)?;
    if metadata.permissions().readonly() {
        return Err(LogError::from_io(
            Stage::Compact,
            io::ErrorKind::PermissionDenied.into(),
        ));
    }
    temporary
        .persist(paths.path())
        .map_err(|e| LogError::from_io(Stage::Compact, e.error))?;
    Ok(())
}
fn read_window(
    file: &mut (impl Read + Seek),
    start: u64,
    size: usize,
) -> Result<Vec<u8>, LogError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| LogError::Allocation)?;
    bytes.resize(size, 0);
    file.seek(SeekFrom::Start(start))
        .and_then(|_| file.read_exact(&mut bytes))
        .map_err(|e| LogError::from_io(Stage::Read, e))?;
    Ok(bytes)
}
fn recover(file: &mut File, max_record: usize) -> Result<u64, LogError> {
    let length = file
        .metadata()
        .map_err(|e| LogError::from_io(Stage::Read, e))?
        .len();
    if length == 0 {
        return Ok(0);
    }
    let mut last = [0];
    file.seek(SeekFrom::Start(length - 1))
        .and_then(|_| file.read_exact(&mut last))
        .map_err(|e| LogError::from_io(Stage::Read, e))?;
    if last == *b"\n" {
        return Ok(0);
    }
    let size = length.min((max_record + 1) as u64) as usize;
    let bytes = read_window(file, length - size as u64, size)?;
    let boundary = match bytes.iter().rposition(|b| *b == b'\n') {
        Some(i) => length - size as u64 + i as u64 + 1,
        None if length <= max_record as u64 => 0,
        None => return Err(LogError::Corrupt),
    };
    let recovered = length - boundary;
    if recovered > max_record as u64 {
        return Err(LogError::Corrupt);
    }
    file.set_len(boundary)
        .map_err(|e| LogError::from_io(Stage::Recover, e))?;
    Ok(recovered)
}
fn retained_suffix(
    file: &mut (impl Read + Seek),
    length: u64,
    budget: usize,
) -> Result<Vec<u8>, LogError> {
    if budget == 0 || length == 0 {
        return Ok(Vec::new());
    }
    let size = length.min((budget + 1) as u64) as usize;
    let mut bytes = read_window(file, length - size as u64, size)?;
    if length > budget as u64 {
        let start = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| n + 1)
            .unwrap_or(bytes.len());
        bytes.drain(..start);
    }
    if !bytes.is_empty() && (!bytes.ends_with(b"\n") || std::str::from_utf8(&bytes).is_err()) {
        return Err(LogError::Corrupt);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    thread_local! {
        static BEFORE_LOCK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const {
            std::cell::RefCell::new(None)
        };
    }

    pub(super) fn before_lock() {
        if let Some(hook) = BEFORE_LOCK.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }

    #[test]
    fn lock_rejects_disappeared_parent_without_recreating_runtime() {
        let fixture = tempfile::tempdir().unwrap();
        let runtime = fixture.path().join("runtime");
        let directory = runtime.join("logs");
        fs::create_dir_all(&directory).unwrap();
        let log = LogFile::open(
            LogPaths::explicit(directory.join("test.log")).unwrap(),
            Options {
                limits: Limits {
                    max_file_bytes: 1024,
                    max_record_bytes: 128,
                    max_batch_bytes: 256,
                    retain_bytes: 512,
                },
                durability: Durability::Buffered,
                lock_timeout: Duration::ZERO,
            },
            Cancellation::default(),
        )
        .unwrap();
        log.append(b"preserve\n").unwrap();
        let moved = fixture.path().join("moved-runtime");
        let original = runtime.clone();
        let destination = moved.clone();
        BEFORE_LOCK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || fs::rename(original, destination).unwrap()));
        });
        assert!(matches!(
            log.append(b"new\n"),
            Err(LogError::Io {
                stage: Stage::Lock,
                kind: io::ErrorKind::NotFound,
                ..
            })
        ));
        assert!(!runtime.exists());
        assert_eq!(
            fs::read(moved.join("logs/test.log")).unwrap(),
            b"preserve\n"
        );
    }

    struct LimitedWriter {
        bytes: Vec<u8>,
        chunk: usize,
        remaining: usize,
        zero: bool,
        calls: usize,
        rollbacks: usize,
        syncs: usize,
    }
    impl LimitedWriter {
        fn new(chunk: usize, remaining: usize, zero: bool) -> Self {
            Self {
                bytes: b"old\n".to_vec(),
                chunk,
                remaining,
                zero,
                calls: 0,
                rollbacks: 0,
                syncs: 0,
            }
        }
    }
    impl Write for LimitedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.remaining == 0 {
                return if self.zero {
                    Ok(0)
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "private-payload/path",
                    ))
                };
            }
            let count = bytes.len().min(self.chunk).min(self.remaining);
            self.bytes.extend_from_slice(&bytes[..count]);
            self.remaining -= count;
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    fn rollback(writer: &mut LimitedWriter, length: u64) -> io::Result<()> {
        writer.rollbacks += 1;
        writer.bytes.truncate(length as usize);
        Ok(())
    }
    #[test]
    fn short_zero_and_partial_writes_roll_back_once_without_batch_retry() {
        let mut short = LimitedWriter::new(2, 100, false);
        append_to(
            &mut short,
            b"new record\n",
            4,
            Durability::Buffered,
            rollback,
            |_| panic!("Buffered must not sync"),
        )
        .unwrap();
        assert_eq!(short.bytes, b"old\nnew record\n");
        assert_eq!(short.calls, 6);
        assert_eq!(short.rollbacks, 0);
        for zero in [true, false] {
            let mut writer = LimitedWriter::new(2, if zero { 0 } else { 5 }, zero);
            let error = append_to(
                &mut writer,
                b"new record\n",
                4,
                Durability::SyncData,
                rollback,
                |_| panic!("failed write must not sync"),
            )
            .unwrap_err();
            assert!(matches!(error, LogError::Io { stage: Stage::Append,
                kind, rollback_failed: false, .. } if kind == if zero { io::ErrorKind::WriteZero } else { io::ErrorKind::BrokenPipe }));
            assert_eq!(writer.bytes, b"old\n");
            assert_eq!(writer.rollbacks, 1);
            assert_eq!(writer.calls, if zero { 1 } else { 4 });
            assert!(!format!("{error:?}").contains("private-payload"));
        }
        let mut broken = LimitedWriter::new(2, 5, false);
        let error = append_to(
            &mut broken,
            b"new record\n",
            4,
            Durability::Buffered,
            |writer, _| {
                writer.rollbacks += 1;
                Err(io::ErrorKind::PermissionDenied.into())
            },
            |_| panic!("not reached"),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            LogError::Io {
                stage: Stage::Append,
                rollback_failed: true,
                ..
            }
        ));
        assert_eq!(broken.bytes, b"old\nnew r");
        assert_eq!(broken.calls, 4);
        assert_eq!(broken.rollbacks, 1);
    }
    #[test]
    fn sync_error_preserves_completed_bytes_and_is_not_retried_or_rolled_back() {
        let mut writer = LimitedWriter::new(2, 100, false);
        let error = append_to(
            &mut writer,
            b"new\n",
            4,
            Durability::SyncData,
            rollback,
            |writer| {
                writer.syncs += 1;
                Err(io::Error::other("private sync message"))
            },
        )
        .unwrap_err();
        assert_eq!(writer.bytes, b"old\nnew\n");
        assert_eq!(writer.rollbacks, 0);
        assert_eq!(writer.calls, 2);
        assert_eq!(writer.syncs, 1);
        assert!(matches!(
            error,
            LogError::Io {
                stage: Stage::Sync,
                rollback_failed: false,
                ..
            }
        ));
        assert!(!error.to_string().contains("private sync"));
    }
    struct CountedRead {
        source: Cursor<Vec<u8>>,
        total: usize,
        largest: usize,
        seeks: usize,
    }
    impl Read for CountedRead {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.largest = self.largest.max(bytes.len());
            let count = self.source.read(bytes)?;
            self.total += count;
            Ok(count)
        }
    }
    impl Seek for CountedRead {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.seeks += 1;
            self.source.seek(pos)
        }
    }
    #[test]
    fn retained_reader_never_requests_the_whole_source_and_allocation_fails_before_read() {
        let mut source = vec![b'x'; 1024 * 1024];
        let length = source.len();
        source[length - 6..].copy_from_slice(b"\nlast\n");
        let mut counted = CountedRead {
            source: Cursor::new(source),
            total: 0,
            largest: 0,
            seeks: 0,
        };
        assert_eq!(
            retained_suffix(&mut counted, length as u64, 8).unwrap(),
            b"last\n"
        );
        assert_eq!(counted.total, 9);
        assert_eq!(counted.largest, 9);
        assert_eq!(counted.seeks, 1);
        assert_eq!(
            read_window(&mut counted, 0, usize::MAX).unwrap_err(),
            LogError::Allocation
        );
        assert_eq!(counted.total, 9);
        assert_eq!(counted.seeks, 1);
        assert_eq!(
            retained_suffix(&mut counted, length as u64, 0).unwrap(),
            b""
        );
        assert_eq!(counted.total, 9);
    }
    #[test]
    fn temporary_is_private_before_payload_and_failed_replace_preserves_the_active_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("log");
        let log = LogFile::open(
            LogPaths::explicit(&path).unwrap(),
            Options {
                limits: Limits {
                    max_file_bytes: 12,
                    max_record_bytes: 4,
                    max_batch_bytes: 4,
                    retain_bytes: 8,
                },
                durability: Durability::Buffered,
                lock_timeout: Duration::from_secs(1),
            },
            Cancellation::default(),
        )
        .unwrap();
        log.append(b"old\n").unwrap();
        let mut temporary = log.temporary().unwrap();
        let temporary_path = temporary.path().to_owned();
        assert_eq!(temporary.as_file().metadata().unwrap().len(), 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                temporary.as_file().metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        temporary.write_all(b"new\n").unwrap();
        let original = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let result = persist(temporary, &log.paths);
        fs::set_permissions(&path, original).unwrap();
        assert!(matches!(
            result,
            Err(LogError::Io {
                stage: Stage::Compact,
                kind: io::ErrorKind::PermissionDenied,
                ..
            })
        ));
        assert_eq!(fs::read(&path).unwrap(), b"old\n");
        assert!(!temporary_path.exists());
    }
    #[cfg(unix)]
    #[test]
    fn real_rename_permission_failure_reports_compact_and_factory_cleans_orphan() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("log");
        let log = LogFile::open(
            LogPaths::explicit(&path).unwrap(),
            Options {
                limits: Limits {
                    max_file_bytes: 12,
                    max_record_bytes: 4,
                    max_batch_bytes: 4,
                    retain_bytes: 8,
                },
                durability: Durability::Buffered,
                lock_timeout: Duration::from_secs(1),
            },
            Cancellation::default(),
        )
        .unwrap();
        log.append(b"old\n").unwrap();
        let mut temporary = log.temporary().unwrap();
        let temporary_path = temporary.path().to_owned();
        temporary.write_all(b"new\n").unwrap();
        let original = fs::metadata(directory.path()).unwrap().permissions();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o500)).unwrap();
        // Privileged runners can override directory modes; probe this actual fixture.
        let blocked = fs::write(directory.path().join("permission-probe"), b"").is_err();
        eprintln!("real rename permission-denied branch exercised={blocked}");
        let result = persist(temporary, &log.paths);
        fs::set_permissions(directory.path(), original).unwrap();
        if blocked {
            assert!(matches!(
                result,
                Err(LogError::Io {
                    stage: Stage::Compact,
                    kind: io::ErrorKind::PermissionDenied,
                    ..
                })
            ));
            assert_eq!(fs::read(&path).unwrap(), b"old\n");
            // Tempfile Drop could not remove its entry while the directory was read-only.
            assert!(temporary_path.exists());
            LogFile::open(log.paths.clone(), log.options, Cancellation::default()).unwrap();
            assert!(!temporary_path.exists());
        } else {
            assert!(result.is_ok());
            assert_eq!(fs::read(&path).unwrap(), b"new\n");
        }
    }
}
