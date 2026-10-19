//! Private-file metadata inspection without opening a database or its sidecars.
use rustix::fs::{AtFlags, FileType, Stat, statat};
use std::{ffi::OsStr, fs::File, io};

/// Physical file identity. Equality grants no namespace or cleanup authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivateFileIdentity {
    device: rustix::fs::Dev,
    inode: u64,
}

fn identity(metadata: &Stat) -> io::Result<PrivateFileIdentity> {
    if FileType::from_raw_mode(metadata.st_mode) != FileType::RegularFile
        || metadata.st_nlink != 1
        || metadata.st_uid != rustix::process::geteuid().as_raw()
        || metadata.st_mode & 0o7777 != 0o600
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(PrivateFileIdentity {
        device: metadata.st_dev,
        inode: metadata.st_ino,
    })
}

/// Inspect one current private file beneath the caller's held private directory.
/// No leaf descriptor is opened or closed: doing so can release another SQLite
/// connection's process-associated POSIX locks. Absence remains NotFound.
/// Rejects symlinks, nonregular/multiply linked files and nonprivate access.
/// Cancellation checks share the caller's existing operation; this does not
/// create, repair, retain a file binding, or resist hostile namespace mutation.
pub fn inspect_private_file_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<PrivateFileIdentity> {
    crate::write_relative::name(child, component_capacity, before)?;
    before()?;
    crate::verify_private_directory(parent)?;
    before()?;
    let first = identity(&statat(parent, child, AtFlags::SYMLINK_NOFOLLOW)?)?;
    #[cfg(target_os = "macos")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let child = CString::new(child.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput)?;
        before()?;
        crate::macos_access::macos_access_snapshot_relative(parent, &child, 8192)?
            .verify_no_aces()?;
    }
    before()?;
    let last = identity(&statat(parent, child, AtFlags::SYMLINK_NOFOLLOW)?)?;
    if first != last {
        return Err(io::ErrorKind::InvalidData.into());
    }
    before()?;
    Ok(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    #[test]
    fn private_metadata_is_relative_current_and_preserves_absence() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("parent");
        let parent = crate::create_private_directory(&path)?;
        drop(crate::create_private_database_file(&path.join("database"))?);
        let inspect =
            || inspect_private_file_relative(&parent, OsStr::new("database"), 64, &mut || Ok(()));
        let first = inspect()?;
        assert_eq!(first, inspect()?);
        let retained = temp.path().join("retained");
        fs::rename(&path, &retained)?;
        fs::create_dir(&path)?;
        fs::write(path.join("database"), b"foreign")?;
        assert_eq!(first, inspect()?);
        fs::rename(retained.join("database"), retained.join("old"))?;
        drop(crate::create_private_database_file(
            &retained.join("database"),
        )?);
        assert_ne!(first, inspect()?);
        let error =
            inspect_private_file_relative(&parent, OsStr::new("missing"), 64, &mut || Ok(()))
                .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!retained.join("missing").exists());
        Ok(())
    }

    #[test]
    fn private_metadata_rejects_invalid_names_access_links_and_cancellation() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("parent");
        let parent = crate::create_private_directory(&path)?;
        let file_path = path.join("database");
        drop(crate::create_private_database_file(&file_path)?);
        for name in ["", ".", "..", "nested/child", "nul\0byte"] {
            assert!(
                inspect_private_file_relative(&parent, OsStr::new(name), 64, &mut || Ok(()))
                    .is_err()
            );
        }
        assert_eq!(
            inspect_private_file_relative(&parent, OsStr::new("database"), 1, &mut || Ok(()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
        assert_eq!(
            inspect_private_file_relative(&parent, OsStr::new("database"), 64, &mut || Err(
                io::ErrorKind::Interrupted.into()
            ))
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
        fs::set_permissions(&file_path, fs::Permissions::from_mode(0o640))?;
        assert!(
            inspect_private_file_relative(&parent, OsStr::new("database"), 64, &mut || Ok(()))
                .is_err()
        );
        fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600))?;
        symlink(&file_path, path.join("link"))?;
        assert!(
            inspect_private_file_relative(&parent, OsStr::new("link"), 64, &mut || Ok(())).is_err()
        );
        fs::create_dir(path.join("directory"))?;
        assert!(
            inspect_private_file_relative(&parent, OsStr::new("directory"), 64, &mut || Ok(()))
                .is_err()
        );
        fs::hard_link(&file_path, path.join("alias"))?;
        assert!(
            inspect_private_file_relative(&parent, OsStr::new("database"), 64, &mut || Ok(()))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn private_metadata_detects_replacement_between_observations() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("parent");
        let parent = crate::create_private_directory(&path)?;
        let database = path.join("database");
        drop(crate::create_private_database_file(&database)?);
        let mut checks = 0;
        let error = inspect_private_file_relative(&parent, OsStr::new("database"), 64, &mut || {
            checks += 1;
            if checks == 4 {
                fs::rename(&database, path.join("old"))?;
                drop(crate::create_private_database_file(&database)?);
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750))?;
        assert_eq!(
            inspect_private_file_relative(&parent, OsStr::new("database"), 64, &mut || Ok(()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn private_metadata_rejects_macos_aces() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("parent");
        let parent = crate::create_private_directory(&path)?;
        let file = path.join("database");
        drop(crate::create_private_database_file(&file)?);
        assert!(
            std::process::Command::new("chmod")
                .args(["+a", "everyone allow read"])
                .arg(file)
                .status()?
                .success()
        );
        assert_eq!(
            inspect_private_file_relative(&parent, OsStr::new("database"), 64, &mut || Ok(()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        Ok(())
    }
}
