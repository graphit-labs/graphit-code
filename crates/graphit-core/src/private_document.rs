//! Conventional private credential I/O and atomic content replacement.
use crate::{file_io::SnapshotContext, file_lock::FileLock};
use secrecy::zeroize::Zeroizing;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

pub(crate) fn validate_limit(limit: usize) -> io::Result<()> {
    if limit == 0 || limit.checked_add(1).is_none() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

pub(crate) fn private_directory(path: &Path) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, READ_CONTROL};
        options
            .access_mode(READ_CONTROL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }
    graphit_fs::verify_private_directory(&options.open(path)?)
}

pub(crate) fn read(
    path: &Path,
    limit: usize,
    original: &SnapshotContext,
) -> io::Result<Option<Zeroizing<Vec<u8>>>> {
    original.check()?;
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    graphit_fs::verify_private_file(&file)?;
    let mut bytes = Zeroizing::new(Vec::new());
    let mut buffer = Zeroizing::new([0; 64 * 1024]);
    loop {
        original.check()?;
        let count = file.read(&mut *buffer)?;
        if count == 0 {
            break;
        }
        if count > limit.saturating_sub(bytes.len()) {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        bytes
            .try_reserve(count)
            .map_err(|_| io::ErrorKind::OutOfMemory)?;
        bytes.extend_from_slice(&buffer[..count]);
    }
    original.check()?;
    Ok(Some(bytes))
}

fn open_lock(path: &Path, original: &SnapshotContext) -> io::Result<File> {
    original.check()?;
    let file = match graphit_fs::create_private_lock_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            OpenOptions::new().read(true).write(true).open(path)?
        }
        Err(error) => return Err(error),
    };
    graphit_fs::verify_private_file(&file)?;
    original.check()?;
    Ok(file)
}

pub(crate) fn lock(path: &Path, original: &SnapshotContext, shared: bool) -> io::Result<FileLock> {
    let file = open_lock(path, original)?;
    if shared {
        FileLock::acquire_shared_owned_file(
            file,
            path.to_owned(),
            original.deadline(),
            original.cancellation(),
        )
    } else {
        FileLock::acquire_owned_file(
            file,
            path.to_owned(),
            original.deadline(),
            original.cancellation(),
        )
    }
}

pub(crate) fn try_lock(path: &Path, original: &SnapshotContext) -> io::Result<Option<FileLock>> {
    match FileLock::try_acquire_owned_file(open_lock(path, original)?, path.to_owned()) {
        Ok(lock) => {
            original.check()?;
            Ok(Some(lock))
        }
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
            original.check()?;
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// The caller holds its content lock; only bytes participate in the final CAS.
pub(crate) fn write_atomic(
    path: &Path,
    expected: Option<&[u8]>,
    body: &[u8],
    limit: usize,
    original: &SnapshotContext,
) -> io::Result<()> {
    original.check()?;
    if body.len() > limit {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    private_directory(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".graphit-private-")
        .make_in(parent, graphit_fs::create_private_file)?;
    for chunk in body.chunks(64 * 1024) {
        original.check()?;
        temporary.write_all(chunk)?;
    }
    temporary.as_file().sync_all()?;
    if read(path, limit, original)?.as_deref().map(Vec::as_slice) != expected {
        return Err(io::ErrorKind::InvalidData.into());
    }
    original.check()?;
    // Close the stage handle before the SDK atomically publishes the new file.
    let temporary = temporary.into_temp_path();
    temporary.persist(path).map_err(|error| error.error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_lock::Cancellation;
    use std::time::{Duration, Instant};

    #[test]
    fn private_lock_with_content_serializes_without_modifying_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owner.lock");
        let bytes = b"existing private lock content";
        let mut file = graphit_fs::create_private_lock_file(&path).unwrap();
        file.write_all(bytes).unwrap();
        drop(file);
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        );
        let first = try_lock(&path, &original).unwrap().unwrap();
        assert!(try_lock(&path, &original).unwrap().is_none());
        drop(first);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let successor = try_lock(&path, &original).unwrap().unwrap();
        assert!(try_lock(&path, &original).unwrap().is_none());
        drop(successor);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
