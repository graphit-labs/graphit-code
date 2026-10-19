//! Private birth and same-handle access checks. These primitives grant no path ownership.
use std::{fs::File, io, path::Path};

/// Exclusively create the final directory; never repair or adopt an existing entry.
/// A failure can leave a created directory. Namespace/cleanup authority is the caller's.
pub fn create_private_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        crate::windows_private_file::new_directory(path)
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

fn verify(file: &File, directory: bool) -> io::Result<()> {
    #[cfg(windows)]
    {
        crate::windows_private_file::verify(file, directory)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata()?;
        let kind = if directory {
            m.is_dir()
        } else {
            m.is_file() && m.nlink() == 1
        };
        if !kind
            || m.uid() != rustix::process::geteuid().as_raw()
            || m.mode() & 0o7777 != if directory { 0o700 } else { 0o600 }
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
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
    fn same_handle_checks_reject_changed_mode_kind_and_hardlinks() {
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
        assert!(verify_private_file(&file).is_err());
        symlink(&path, root.join("link")).unwrap();
        assert!(create_private_file(&root.join("link")).is_err());
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos", windows)))]
mod database_tests {
    use super::*;
    use crate::write_relative::open_private_database_file_relative;
    use std::{
        ffi::OsStr,
        fs,
        io::{Read, Write},
    };

    #[test]
    fn private_database_open_is_existing_readonly_private_and_cancellable() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("control");
        let parent = create_private_directory(&root)?;
        let path = root.join("database");
        let mut held = create_private_database_file(&path)?;
        held.write_all(b"retained body")?;
        verify_private_file(&held)?;
        assert!(create_private_database_file(&path).is_err());
        let mut reader =
            open_private_database_file_relative(&parent, OsStr::new("database"), 64, &mut || {
                Ok(())
            })?;
        let mut body = String::new();
        reader.read_to_string(&mut body)?;
        assert_eq!(body, "retained body");
        assert!(reader.write_all(b"must not write").is_err());
        assert_eq!(fs::read(&path)?, b"retained body");
        let missing =
            open_private_database_file_relative(&parent, OsStr::new("missing"), 64, &mut || Ok(()))
                .unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        assert!(!root.join("missing").exists());
        for name in ["", ".", "..", "nested/child", "nul\0byte"] {
            assert!(
                open_private_database_file_relative(&parent, OsStr::new(name), 64, &mut || Ok(()),)
                    .is_err()
            );
        }
        assert!(open_private_database_file_relative(
            &parent, OsStr::new("database"), 1, &mut || Ok(()),
        ).is_err());
        let cancelled =
            open_private_database_file_relative(&parent, OsStr::new("database"), 64, &mut || {
                Err(io::ErrorKind::Interrupted.into())
            })
            .unwrap_err();
        assert_eq!(cancelled.kind(), io::ErrorKind::Interrupted);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&path, root.join("link"))?;
            assert!(
                open_private_database_file_relative(
                    &parent,
                    OsStr::new("link"),
                    64,
                    &mut || Ok(()),
                )
                .is_err()
            );
        }
        drop(reader);
        drop(held);
        fs::hard_link(&path, root.join("alias"))?;
        assert!(
            open_private_database_file_relative(
                &parent,
                OsStr::new("database"),
                64,
                &mut || Ok(()),
            )
            .is_err()
        );
        assert_eq!(fs::read(&path)?, b"retained body");
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn database_handles_share_writes_without_delete_or_document_policy_changes() -> io::Result<()> {
        use crate::write_relative::open_private_file_relative;
        use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("control");
        let parent = create_private_directory(&root)?;
        let path = root.join("database");
        let held = create_private_database_file(&path)?;
        let mut driver = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&path)?;
        driver.write_all(b"driver body")?;
        let mut observer =
            open_private_database_file_relative(&parent, OsStr::new("database"), 64, &mut || {
                Ok(())
            })?;
        verify_private_file(&observer)?;
        let mut body = String::new();
        observer.read_to_string(&mut body)?;
        assert_eq!(body, "driver body");
        assert!(observer.write_all(b"denied").is_err());
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&path)?;
        assert!(
            open_private_file_relative(&parent, OsStr::new("database"), 64, &mut || Ok(()),)
                .is_err()
        );
        assert!(fs::rename(&path, root.join("renamed")).is_err());
        assert!(fs::remove_file(&path).is_err());
        let stage_path = root.join("stage");
        let stage = create_private_file(&stage_path)?;
        assert!(
            OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .open(&stage_path)
                .is_err()
        );
        drop(stage);
        drop(writer);
        drop(observer);
        drop(driver);
        drop(held);
        fs::remove_file(&path)?;
        assert!(!path.exists());
        Ok(())
    }
}
