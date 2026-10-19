//! Held-parent private control births. Primitive inputs carry no ownership grant.
//! A failed operation may leave a newly created entry; never delete it on Drop.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use crate::private_file_metadata::inspect_private_file_relative;

#[cfg(target_os = "linux")]
use rustix::fs::{Mode, OFlags, ResolveFlags, openat2};
#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(unix)]
use std::{ffi::OsStr, io};

#[cfg(unix)]
pub(crate) fn name(
    value: &OsStr,
    capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    if capacity == 0 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let bytes = value.as_encoded_bytes();
    if bytes.len() > capacity {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    if matches!(bytes, b"" | b"." | b"..") {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    for chunk in bytes.chunks(256) {
        before()?;
        if chunk.iter().any(|byte| matches!(*byte, 0 | b'/')) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn strict(
    parent: &File,
    name: &OsStr,
    flags: OFlags,
    mode: Mode,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    before()?;
    openat2(
        parent,
        name,
        flags | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        mode,
        ResolveFlags::BENEATH
            | ResolveFlags::NO_SYMLINKS
            | ResolveFlags::NO_MAGICLINKS
            | ResolveFlags::NO_XDEV,
    )
    .map(File::from)
    .map_err(|error| {
        if matches!(
            error,
            rustix::io::Errno::NOSYS | rustix::io::Errno::OPNOTSUPP
        ) {
            io::ErrorKind::Unsupported.into()
        } else {
            io::Error::from(error)
        }
    })
}

#[cfg(target_os = "linux")]
fn private(
    file: &File,
    directory: bool,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    before()?;
    if directory {
        crate::verify_private_directory(file)
    } else {
        crate::verify_private_file(file)
    }
}

/// Exclusively birth a private directory relative to the held parent.
/// No existing entry is repaired or adopted. Failure after mkdir may leave it.
/// Caller admits its descriptor, bounded name conversion and frames beforehand.
#[cfg(target_os = "linux")]
pub fn create_private_directory_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    name(child, component_capacity, before)?;
    before()?;
    rustix::fs::mkdirat(parent, child, Mode::RWXU)?;
    let file = strict(
        parent,
        child,
        OFlags::RDONLY | OFlags::DIRECTORY,
        Mode::empty(),
        before,
    )?;
    private(&file, true, before)?;
    Ok(file)
}

/// Open an existing private directory; no create, repair or ownership inference.
#[cfg(target_os = "linux")]
pub fn open_private_directory_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    name(child, component_capacity, before)?;
    let file = strict(
        parent,
        child,
        OFlags::RDONLY | OFlags::DIRECTORY,
        Mode::empty(),
        before,
    )?;
    private(&file, true, before)?;
    Ok(file)
}

/// Open the permanent private lock, exclusively creating it only on absence.
/// Retain this exact file through kernel lock acquisition and binding checks.
/// Never replace/unlink the lock. An error can leave an exclusively born lock.
#[cfg(target_os = "linux")]
pub fn open_private_lock_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    name(child, component_capacity, before)?;
    let file = match strict(
        parent,
        child,
        OFlags::RDWR | OFlags::CREATE | OFlags::EXCL,
        Mode::RUSR | Mode::WUSR,
        before,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            strict(parent, child, OFlags::RDWR, Mode::empty(), before)?
        }
        Err(error) => return Err(error),
    };
    private(&file, false, before)?;
    Ok(file)
}

/// Open a current private regular file relative to a held private parent.
/// No creation, repair or path-derived authority; absence remains NotFound.
#[cfg(target_os = "linux")]
pub fn open_private_file_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    name(child, component_capacity, before)?;
    let file = strict(parent, child, OFlags::RDONLY, Mode::empty(), before)?;
    private(&file, false, before)?;
    before()?;
    Ok(file)
}

/// Open an existing private database file relative to its held parent, read-only.
/// Windows permits concurrent driver writes but excludes deletion. This does not
/// create, repair or grant path ownership; sidecar observers must close before cleanup.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn open_private_database_file_relative(
    parent: &std::fs::File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<std::fs::File> {
    open_private_file_relative(parent, child, component_capacity, before)
}

/// Inspect current private-directory access on the exact caller-owned handle.
/// This grants neither birth history nor ownership of its namespace.
#[cfg(target_os = "linux")]
pub fn inspect_private_directory_current(
    file: &File,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    private(file, true, before)
}

/// Reopen the existing lock for exact binding comparison; absence never creates.
#[cfg(target_os = "linux")]
pub fn open_existing_private_lock_relative(
    parent: &File,
    child: &OsStr,
    component_capacity: usize,
    before: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<File> {
    name(child, component_capacity, before)?;
    let file = strict(parent, child, OFlags::RDWR, Mode::empty(), before)?;
    private(&file, false, before)?;
    Ok(file)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    };

    #[test]
    fn current_private_read_is_relative_and_never_creates_or_repairs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("parent");
        let parent = crate::create_private_directory(&path).unwrap();
        let file = crate::create_private_file(&path.join("body")).unwrap();
        drop(file);
        let retained = tmp.path().join("retained");
        fs::rename(&path, &retained).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("body"), b"foreign").unwrap();
        let body =
            open_private_file_relative(&parent, OsStr::new("body"), 32, &mut || Ok(())).unwrap();
        assert_eq!(body.metadata().unwrap().len(), 0);
        let error = open_private_file_relative(&parent, OsStr::new("missing"), 32, &mut || Ok(()))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!retained.join("missing").exists());
        let error = open_private_file_relative(&parent, OsStr::new("body"), 32, &mut || {
            Err(io::ErrorKind::Interrupted.into())
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        fs::hard_link(retained.join("body"), retained.join("alias")).unwrap();
        assert!(
            open_private_file_relative(&parent, OsStr::new("body"), 32, &mut || Ok(())).is_err()
        );
    }

    #[test]
    fn held_parent_birth_does_not_follow_replaced_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("parent");
        fs::create_dir(&path).unwrap();
        let parent = crate::read_directory::open_root_readonly(&path, &mut || Ok(())).unwrap();
        let retained = tmp.path().join("retained");
        fs::rename(&path, &retained).unwrap();
        fs::create_dir(&path).unwrap();
        let directory =
            create_private_directory_relative(&parent, OsStr::new("control"), 32, &mut || Ok(()))
                .unwrap();
        assert!(retained.join("control").is_dir());
        assert!(!path.join("control").exists());
        let lock =
            open_private_lock_relative(&directory, OsStr::new("lock"), 32, &mut || Ok(())).unwrap();
        assert_eq!(lock.metadata().unwrap().mode() & 0o7777, 0o600);
        assert!(
            create_private_directory_relative(&parent, OsStr::new("control"), 32, &mut || Ok(()))
                .is_err()
        );
        assert!(
            open_private_lock_relative(&directory, OsStr::new("lock"), 32, &mut || Ok(())).is_ok()
        );
    }

    #[test]
    fn directory_multiple_links_are_valid_lock_multiple_links_deny() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = crate::read_directory::open_root_readonly(tmp.path(), &mut || Ok(())).unwrap();
        let directory =
            create_private_directory_relative(&parent, OsStr::new("control"), 32, &mut || Ok(()))
                .unwrap();
        fs::create_dir(tmp.path().join("control/subdir")).unwrap();
        assert!(directory.metadata().unwrap().nlink() > 1);
        assert!(
            open_private_directory_relative(&parent, OsStr::new("control"), 32, &mut || Ok(()))
                .is_ok()
        );
        let lock =
            open_private_lock_relative(&directory, OsStr::new("lock"), 32, &mut || Ok(())).unwrap();
        fs::hard_link(
            tmp.path().join("control/lock"),
            tmp.path().join("control/alias"),
        )
        .unwrap();
        assert!(
            open_private_lock_relative(&directory, OsStr::new("lock"), 32, &mut || Ok(())).is_err()
        );
        drop(lock);
    }

    #[test]
    fn admission_and_invalid_names_precede_birth_but_later_denial_retains_residue() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = crate::read_directory::open_root_readonly(tmp.path(), &mut || Ok(())).unwrap();
        for value in ["", ".", "..", "a/b", "nul\0byte"] {
            assert!(
                create_private_directory_relative(&parent, OsStr::new(value), 64, &mut || Ok(()))
                    .is_err()
            );
        }
        assert!(
            create_private_directory_relative(&parent, OsStr::new("oversize"), 7, &mut || Ok(()))
                .is_err()
        );
        assert!(!tmp.path().join("oversize").exists());
        let failure =
            create_private_directory_relative(&parent, OsStr::new("denied"), 32, &mut || {
                Err(io::ErrorKind::Interrupted.into())
            })
            .unwrap_err();
        assert_eq!(failure.kind(), io::ErrorKind::Interrupted);
        assert!(!tmp.path().join("denied").exists());
        let mut calls = 0;
        let failure =
            create_private_directory_relative(&parent, OsStr::new("residue"), 32, &mut || {
                calls += 1;
                if calls == 3 {
                    Err(io::ErrorKind::TimedOut.into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
        assert!(tmp.path().join("residue").is_dir());
        assert!(
            fs::read_dir(tmp.path().join("residue"))
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn current_privacy_and_symlink_checks_do_not_repair() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = crate::read_directory::open_root_readonly(tmp.path(), &mut || Ok(())).unwrap();
        let directory =
            create_private_directory_relative(&parent, OsStr::new("control"), 32, &mut || Ok(()))
                .unwrap();
        fs::set_permissions(
            tmp.path().join("control"),
            fs::Permissions::from_mode(0o750),
        )
        .unwrap();
        assert!(
            open_private_directory_relative(&parent, OsStr::new("control"), 32, &mut || Ok(()))
                .is_err()
        );
        assert_eq!(directory.metadata().unwrap().mode() & 0o7777, 0o750);
        symlink("control", tmp.path().join("alias")).unwrap();
        assert!(
            open_private_directory_relative(&parent, OsStr::new("alias"), 32, &mut || Ok(()))
                .is_err()
        );
        assert!(
            open_private_lock_relative(&parent, OsStr::new("alias"), 32, &mut || Ok(())).is_err()
        );
    }
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
