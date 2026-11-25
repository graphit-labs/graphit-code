//! Private birth and same-handle access checks. These primitives grant no path ownership.
use std::{fs::File, io, path::Path};

/// Exclusively create the final directory; never repair or adopt an existing entry.
/// A failure can leave a created directory. Namespace/cleanup authority is the caller's.
pub fn create_private_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        crate::windows_private_file::new_directory(path, false)
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos_private_file::new_directory(path)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        std::fs::DirBuilder::new().mode(0o700).create(path)?;
        let directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(
                (rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW).bits() as i32,
            )
            .open(path)?;
        verify_private_directory(&directory)?;
        Ok(directory)
    }
}

/// Create a protected Windows directory whose driver-created files inherit
/// access for the current user only.
#[cfg(windows)]
pub fn create_private_database_directory(path: &Path) -> io::Result<File> {
    crate::windows_private_file::new_directory(path, true)
}

/// Require private access and file inheritance before a database driver writes.
#[cfg(windows)]
pub fn verify_private_database_directory(file: &File) -> io::Result<()> {
    crate::windows_private_file::verify_database_directory(file)
}

/// CREATE_NEW/private mode or protected user-only DACL before any content bytes.
/// Verify the returned handle and retain it through staging; no path is reopened.
/// Failure after creation can leave an empty entry; it grants no cleanup authority.
pub fn create_private_file(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        crate::windows_private_file::new_file(path)
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos_private_file::new_file(path)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(path)?;
        verify_private_file(&file)?;
        Ok(file)
    }
}

/// CREATE_NEW with private access at birth and same-handle verification.
/// Never reopen, truncate or adopt an existing entry; retain the handle for identity checks.
/// Windows shares read/write access with the database driver, but excludes deletion.
/// Failure may leave an empty entry; this grants no namespace or cleanup authority.
pub fn create_private_database_file(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        crate::windows_private_file::new_database_file(path)
    }
    #[cfg(unix)]
    {
        create_private_file(path)
    }
}

/// Private CREATE_NEW for a permanent kernel lock; never replace or unlink it.
/// Windows permits read/write contenders, but excludes delete sharing.
pub fn create_private_lock_file(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        crate::windows_private_file::new_lock_file(path)
    }
    #[cfg(unix)]
    {
        create_private_file(path)
    }
}

/// Inspect privacy through the supplied handle; does not establish private birth or path history.
pub fn verify_private_file(file: &File) -> io::Result<()> {
    verify(file, false)
}
pub fn verify_private_directory(file: &File) -> io::Result<()> {
    verify(file, true)
}

/// Verify owner-only directory access through the configured path.
pub fn verify_private_directory_path(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        verify_private_database_directory_path(path)
    }
    #[cfg(windows)]
    {
        crate::windows_private_file::verify_private_directory_path(path)
    }
}

/// Verify current private access without opening the file. Unix callers may use
/// this while SQLite holds process-associated POSIX locks on the same inode.
/// Ordinary path resolution follows configured links; no binding is retained.
pub fn verify_private_file_path(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        verify_unix_metadata(&std::fs::metadata(path)?, false)?;
        #[cfg(target_os = "macos")]
        crate::macos_access::macos_access_snapshot_path(path)?.verify_no_aces()?;
        Ok(())
    }
    #[cfg(windows)]
    {
        crate::windows_private_file::verify_private_file_path(path)
    }
}

/// Verify a database parent's current private access, including the Windows
/// inheritance needed by SQLite sidecars. Does not open a database descriptor.
pub fn verify_private_database_directory_path(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        verify_unix_metadata(&std::fs::metadata(path)?, true)?;
        #[cfg(target_os = "macos")]
        crate::macos_access::macos_access_snapshot_path(path)?.verify_no_aces()?;
        Ok(())
    }
    #[cfg(windows)]
    {
        crate::windows_private_file::verify_private_database_directory_path(path)
    }
}

#[cfg(unix)]
fn verify_unix_metadata(metadata: &std::fs::Metadata, directory: bool) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let kind = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !kind
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}

fn verify(file: &File, directory: bool) -> io::Result<()> {
    #[cfg(windows)]
    {
        crate::windows_private_file::verify(file, directory)
    }
    #[cfg(unix)]
    {
        verify_unix_metadata(&file.metadata()?, directory)?;
        #[cfg(target_os = "macos")]
        crate::macos_access_snapshot(file, 8192)?.verify_no_aces()?;
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    };

    #[test]
    fn private_birth_and_exclusive_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("control");
        let directory = create_private_directory(&root).unwrap();
        verify_private_directory_path(&root).unwrap();
        assert_eq!(directory.metadata().unwrap().mode() & 0o7777, 0o700);
        let mut file = create_private_file(&root.join("stage")).unwrap();
        assert_eq!(file.metadata().unwrap().mode() & 0o7777, 0o600);
        file.write_all(b"fixture").unwrap();
        verify_private_file(&file).unwrap();
        assert!(create_private_directory(&root).is_err());
        assert!(create_private_file(&root.join("stage")).is_err());
        assert_eq!(fs::read(root.join("stage")).unwrap(), b"fixture");
    }

    #[test]
    fn private_files_work_under_a_configured_directory_link() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let alias = root.path().join("configured");
        symlink(&target, &alias).unwrap();
        let directory = create_private_directory(&alias.join("credentials")).unwrap();
        verify_private_directory(&directory).unwrap();
        verify_private_directory_path(&alias.join("credentials")).unwrap();
        let path = alias.join("credentials/token");
        let mut file = create_private_database_file(&path).unwrap();
        file.write_all(b"private value").unwrap();
        verify_private_file(&file).unwrap();
        assert_eq!(
            fs::read(target.join("credentials/token")).unwrap(),
            b"private value"
        );
        assert_eq!(file.metadata().unwrap().mode() & 0o7777, 0o600);
    }

    #[test]
    fn same_handle_checks_validate_private_access_and_accept_hardlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("control");
        let directory = create_private_directory(&root).unwrap();
        let path = root.join("stage");
        let file = create_private_file(&path).unwrap();
        assert!(verify_private_directory(&file).is_err());
        assert!(verify_private_file(&directory).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(verify_private_file(&file).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&path, root.join("alias")).unwrap();
        verify_private_file(&file).unwrap();
        symlink(&path, root.join("link")).unwrap();
        assert!(create_private_file(&root.join("link")).is_err());
    }
    #[test]
    fn owner_readonly_objects_remain_private() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("credentials");
        let directory = create_private_directory(&root).unwrap();
        let path = root.join("secret");
        let file = create_private_file(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        verify_private_file(&file).unwrap();
        verify_private_file_path(&path).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
        verify_private_directory(&directory).unwrap();
        verify_private_database_directory_path(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn path_checks_follow_links_and_reject_actual_access_or_type_errors() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("database-parent");
        drop(create_private_directory(&root).unwrap());
        let path = root.join("database");
        drop(create_private_database_file(&path).unwrap());
        verify_private_database_directory_path(&root).unwrap();
        verify_private_file_path(&path).unwrap();
        fs::hard_link(&path, root.join("hardlink")).unwrap();
        symlink(&path, root.join("symlink")).unwrap();
        symlink(&root, temp.path().join("configured-parent")).unwrap();
        for name in ["database", "hardlink", "symlink"] {
            verify_private_file_path(&temp.path().join("configured-parent").join(name)).unwrap();
        }
        assert_eq!(
            verify_private_file_path(&root).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            verify_private_database_directory_path(&path)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            verify_private_file_path(&root.join("absent"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(
            verify_private_file_path(&path).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        fs::set_permissions(&root, fs::Permissions::from_mode(0o750)).unwrap();
        assert_eq!(
            verify_private_database_directory_path(&root)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn path_privacy_checks_preserve_process_associated_database_locks() {
        use rustix::fs::{FlockOperation, fcntl_lock};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("database-parent");
        drop(create_private_directory(&root).unwrap());
        let path = root.join("database");
        let file = create_private_database_file(&path).unwrap();
        fs::hard_link(&path, root.join("hardlink")).unwrap();
        symlink(&path, root.join("symlink")).unwrap();
        fcntl_lock(&file, FlockOperation::NonBlockingLockExclusive).unwrap();
        let probe = |blocked: bool| {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "private_file::tests::private_path_lock_probe",
                    "--ignored",
                ])
                .env("GRAPHIT_PRIVATE_PATH_LOCK_FILE", &path)
                .env(
                    "GRAPHIT_PRIVATE_PATH_LOCK_BLOCKED",
                    if blocked { "yes" } else { "no" },
                )
                .status()
                .unwrap();
            assert!(status.success());
        };
        probe(true);
        verify_private_database_directory_path(&root).unwrap();
        for name in ["database", "hardlink", "symlink"] {
            verify_private_file_path(&root.join(name)).unwrap();
        }
        probe(true);
        drop(file);
        probe(false);
    }

    #[test]
    #[ignore = "child-process POSIX lock probe, launched by the database-lock test"]
    fn private_path_lock_probe() {
        use rustix::{
            fs::{FlockOperation, fcntl_lock},
            io::Errno,
        };
        let path = std::env::var_os("GRAPHIT_PRIVATE_PATH_LOCK_FILE")
            .expect("parent supplies database path");
        let blocked = std::env::var("GRAPHIT_PRIVATE_PATH_LOCK_BLOCKED")
            .expect("parent supplies expected lock state")
            == "yes";
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        let result = fcntl_lock(&file, FlockOperation::NonBlockingLockExclusive);
        if blocked {
            assert!(matches!(result, Err(Errno::AGAIN | Errno::ACCESS)));
        } else {
            result.unwrap();
        }
    }
}
