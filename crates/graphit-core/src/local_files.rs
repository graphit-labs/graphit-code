//! Shared I/O for bounded local metadata, under the caller's lock and policy.
//!
//! These helpers convey no artifact ownership. Checks reject observed symlinks,
//! not hostile concurrent path replacement; atomic replacement syncs the file,
//! not the parent directory, and does not promise power-loss durability.

use crate::file_lock::Cancellation;
use secrecy::zeroize::Zeroizing;
use std::fs::{self, File, Metadata, Permissions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Resolve a directory, including aliases in the caller's captured input.
pub fn canonical_directory(path: &Path) -> io::Result<PathBuf> {
    crate::native_path::valid_native(path)?;
    let path = fs::canonicalize(path)?;
    crate::native_path::valid_native(&path)?;
    if !path.is_dir() {
        return Err(invalid_input("expected a directory"));
    }
    Ok(path)
}

/// Inspect a directory without accepting a symlink as the directory itself.
pub fn directory_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(invalid_input("expected a directory without symlinks")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if directory_exists(path)? {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "directory disappeared during creation",
                ))
            }
        }
        Err(error) => Err(error),
    }
}

/// Create a private directory at birth or qualify an existing directory by its handle.
/// This current access check conveys no artifact ownership or namespace history.
pub(crate) fn open_or_create_private_directory(path: &Path) -> io::Result<File> {
    match graphit_fs::create_private_directory(path) {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => open_private_directory(path),
        Err(error) => Err(error),
    }
}

/// Read-only current privacy qualification; never create or repair a directory.
pub(crate) fn open_private_directory(path: &Path) -> io::Result<File> {
    let file = crate::native_document_file::open_access_directory(path)?;
    graphit_fs::verify_private_directory(&file)?;
    Ok(file)
}

/// Inspect a regular file without following a symlink; absence is distinct from error.
pub fn regular_metadata(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata)),
        Ok(_) => Err(invalid_input("expected a regular file without symlinks")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Read at most `limit` bytes. An unrepresentable cap fails before filesystem I/O.
pub fn read_regular(
    path: &Path,
    limit: usize,
) -> io::Result<Option<(Zeroizing<Vec<u8>>, Permissions)>> {
    let read_limit = limit
        .checked_add(1)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| invalid_input("invalid file byte budget"))?;
    let Some(metadata) = regular_metadata(path)? else {
        return Ok(None);
    };
    if metadata.len() >= read_limit {
        return Err(invalid_data("file exceeds its byte budget"));
    }
    let mut bytes = Zeroizing::new(Vec::new());
    File::open(path)?.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(invalid_data("file exceeds its byte budget"));
    }
    Ok(Some((bytes, metadata.permissions())))
}

/// Replace a regular metadata file under the caller's lock.
///
/// Supplied permissions take precedence; otherwise existing permissions are
/// preserved. Callers creating private documents supply their private mode.
pub fn atomic_replace(
    path: &Path,
    bytes: &[u8],
    permissions: Option<Permissions>,
    cancellation: &Cancellation,
) -> io::Result<()> {
    atomic_replace_with(path, permissions, cancellation, |file| {
        file.write_all(bytes)
    })
}

/// Write a local file incrementally and publish it after the callback succeeds.
///
/// The callback must finish its encoder before returning. Its value is returned
/// only after file synchronization and atomic replacement; an error drops the
/// temporary file. The caller owns coordination and cancellation during writing.
/// This has the same filesystem guarantees as [`atomic_replace`].
pub fn atomic_replace_with<T>(
    path: &Path,
    permissions: Option<Permissions>,
    cancellation: &Cancellation,
    write: impl FnOnce(&mut File) -> io::Result<T>,
) -> io::Result<T> {
    check_cancelled(cancellation)?;
    let existing = writable_metadata(path)?;
    let permissions = permissions.or_else(|| existing.map(|metadata| metadata.permissions()));
    let directory = path
        .parent()
        .ok_or_else(|| invalid_input("file has no parent"))?;
    let mut temporary = temporary_file(directory, permissions.as_ref())?;
    let value = write(temporary.as_file_mut())?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.as_file().sync_all()?;
    persist_temporary(temporary, path, cancellation)?;
    Ok(value)
}

fn writable_metadata(path: &Path) -> io::Result<Option<Metadata>> {
    let metadata = regular_metadata(path)?;
    if metadata
        .as_ref()
        .is_some_and(|metadata| metadata.permissions().readonly())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "file is read-only",
        ));
    }
    Ok(metadata)
}

fn persist_temporary(
    temporary: tempfile::NamedTempFile,
    path: &Path,
    cancellation: &Cancellation,
) -> io::Result<()> {
    check_cancelled(cancellation)?;
    writable_metadata(path)?;
    check_cancelled(cancellation)?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn temporary_file(
    directory: &Path,
    permissions: Option<&Permissions>,
) -> io::Result<tempfile::NamedTempFile> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(".write-");
    if let Some(permissions) = permissions {
        builder.permissions(permissions.clone());
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(Permissions::from_mode(0o666));
        }
    }
    builder.tempfile_in(directory)
}

pub(crate) fn check_cancelled(cancellation: &Cancellation) -> io::Result<()> {
    if cancellation.is_cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "file operation cancelled",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn invalid_input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

pub(crate) fn invalid_data(
    error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn temporary_never_broadens_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        fs::write(&target, "user content").unwrap();
        fs::set_permissions(&target, Permissions::from_mode(0o600)).unwrap();
        let permissions = fs::metadata(target).unwrap().permissions();
        let mut temporary = temporary_file(root.path(), Some(&permissions)).unwrap();
        temporary.write_all(b"user content").unwrap();
        assert_eq!(
            temporary.as_file().metadata().unwrap().permissions().mode() & 0o077,
            0
        );
    }

    #[test]
    fn cancellation_before_write_and_after_sync_preserves_the_destination() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("target");
        fs::write(&destination, "user content").unwrap();
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let result = atomic_replace(&destination, b"replacement", None, &cancellation);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
        let result: io::Result<()> = atomic_replace_with(&destination, None, &cancellation, |_| {
            panic!("cancelled callback must not run")
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert_eq!(fs::read_to_string(&destination).unwrap(), "user content");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        let mut temporary = temporary_file(root.path(), None).unwrap();
        temporary.write_all(b"replacement").unwrap();
        temporary.as_file().sync_all().unwrap();
        assert_eq!(
            persist_temporary(temporary, &destination, &cancellation)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(fs::read_to_string(destination).unwrap(), "user content");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn nonregular_target_preserves_foreign_content() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("target");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("user"), "user content").unwrap();
        assert!(
            atomic_replace(&destination, b"replacement", None, &Cancellation::default()).is_err()
        );
        assert_eq!(
            fs::read_to_string(destination.join("user")).unwrap(),
            "user content"
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_rename_cleans_the_written_temporary() {
        let root = tempfile::tempdir().unwrap();
        let mut temporary = temporary_file(root.path(), None).unwrap();
        temporary.write_all(b"replacement").unwrap();
        temporary.as_file().sync_all().unwrap();
        assert!(
            persist_temporary(
                temporary,
                &root.path().join("missing/target"),
                &Cancellation::default()
            )
            .is_err()
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn bounded_reads_distinguish_missing_empty_exact_and_oversized() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("document");
        assert!(read_regular(&path, 0).unwrap().is_none());
        assert_eq!(
            read_regular(&path, usize::MAX).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        fs::write(&path, []).unwrap();
        assert!(read_regular(&path, 0).unwrap().unwrap().0.is_empty());
        fs::write(&path, b"1234").unwrap();
        assert_eq!(
            read_regular(&path, 4).unwrap().unwrap().0.as_slice(),
            b"1234"
        );
        assert_eq!(
            read_regular(&path, 3).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(128 * 1024 * 1024)
            .unwrap();
        assert_eq!(
            read_regular(&path, 32).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            read_regular(root.path(), 32).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_and_readonly_metadata_rejected_without_foreign_effects() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let link = root.path().join("link");
        fs::write(&target, b"foreign").unwrap();
        symlink(&target, &link).unwrap();
        assert_eq!(
            read_regular(&link, 64).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            atomic_replace(&link, b"replacement", None, &Cancellation::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(fs::read(&target).unwrap(), b"foreign");
        fs::set_permissions(&target, Permissions::from_mode(0o400)).unwrap();
        assert_eq!(
            atomic_replace(&target, b"replacement", None, &Cancellation::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(fs::read(&target).unwrap(), b"foreign");
        fs::set_permissions(&target, Permissions::from_mode(0o000)).unwrap();
        assert_eq!(
            read_regular(&target, 64).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        fs::set_permissions(&target, Permissions::from_mode(0o600)).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn creation_and_replacement_use_captured_or_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        atomic_replace(
            &target,
            b"private",
            Some(Permissions::from_mode(0o600)),
            &Cancellation::default(),
        )
        .unwrap();
        atomic_replace(&target, b"updated", None, &Cancellation::default()).unwrap();
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read(&target).unwrap(), b"updated");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    fn streaming_callback_finalizes_before_publication_and_returns_its_value() {
        use std::io::{Seek, SeekFrom};
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("target");
        fs::write(&destination, b"old").unwrap();
        let value = atomic_replace_with(&destination, None, &Cancellation::default(), |file| {
            file.write_all(b"0000")?;
            file.write_all(b"part")?;
            file.write_all(b"-tail")?;
            assert_eq!(fs::read(&destination).unwrap(), b"old");
            file.seek(SeekFrom::Start(0))?;
            file.write_all(b"0009")?;
            Ok("finalized")
        })
        .unwrap();
        assert_eq!(value, "finalized");
        assert_eq!(fs::read(&destination).unwrap(), b"0009part-tail");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn streaming_input_limit_failure_preserves_target_and_cleans_partial_file() {
        use std::io::Seek;
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("target");
        fs::write(&destination, b"old").unwrap();
        let limits = crate::artifact_store::CopyLimits::new(4, 2, 32, 2).unwrap();
        let error = atomic_replace_with(&destination, None, &Cancellation::default(), |file| {
            for chunk in [b"part".as_slice(), b"-tail".as_slice()] {
                if file.stream_position()? + chunk.len() as u64 > limits.total_file_bytes_limit() {
                    return Err(io::Error::new(
                        io::ErrorKind::FileTooLarge,
                        "expanded input exceeds policy",
                    ));
                }
                file.write_all(chunk)?;
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        assert_eq!(error.to_string(), "expanded input exceeds policy");
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn cancellation_after_streaming_preserves_target_and_cleans_temporary() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("target");
        fs::write(&destination, b"old").unwrap();
        let cancellation = Cancellation::default();
        let error = atomic_replace_with(&destination, None, &cancellation, |file| {
            file.write_all(b"replacement")?;
            cancellation.cancel();
            Ok("must not be returned")
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
