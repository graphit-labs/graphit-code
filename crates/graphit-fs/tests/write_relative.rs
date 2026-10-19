#![cfg(any(target_os = "linux", target_os = "macos", windows))]
use graphit_fs::{read_directory, write_relative::*};
use std::{
    ffi::OsStr,
    fs,
    io::{self, Read, Seek, Write},
};

#[test]
fn private_birth_is_exclusive_and_existing_reads_never_create() {
    let temp = tempfile::tempdir().unwrap();
    let parent = read_directory::open_root_readonly(temp.path(), &mut || Ok(())).unwrap();
    let directory =
        create_private_directory_relative(&parent, OsStr::new("control"), 4096, &mut || Ok(()))
            .unwrap();
    graphit_fs::verify_private_directory(&directory).unwrap();
    assert_eq!(
        create_private_directory_relative(&parent, OsStr::new("control"), 4096, &mut || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    let mut lock =
        open_private_lock_relative(&directory, OsStr::new("lock"), 4096, &mut || Ok(())).unwrap();
    graphit_fs::verify_private_file(&lock).unwrap();
    lock.write_all(b"same permanent file").unwrap();
    let mut contender =
        open_private_lock_relative(&directory, OsStr::new("lock"), 4096, &mut || Ok(())).unwrap();
    let mut bytes = Vec::new();
    contender.rewind().unwrap();
    contender.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"same permanent file");
    for missing in ["missing-lock", "missing-body"] {
        let result = if missing == "missing-lock" {
            open_existing_private_lock_relative(&directory, OsStr::new(missing), 4096, &mut || {
                Ok(())
            })
        } else {
            open_private_file_relative(&directory, OsStr::new(missing), 4096, &mut || Ok(()))
        };
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(!temp.path().join("control").join(missing).exists());
    }
    drop(contender);
    drop(lock);
    fs::hard_link(temp.path().join("control/lock"), temp.path().join("alias")).unwrap();
    assert_eq!(
        open_existing_private_lock_relative(&directory, OsStr::new("lock"), 4096, &mut || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}

#[test]
fn cancellation_invalid_names_and_postbirth_denial_preserve_real_effects() {
    let temp = tempfile::tempdir().unwrap();
    let parent = read_directory::open_root_readonly(temp.path(), &mut || Ok(())).unwrap();
    for name in ["", ".", "..", "a/b", "bad\0name"] {
        assert!(
            create_private_directory_relative(&parent, OsStr::new(name), 4096, &mut || Ok(()))
                .is_err()
        );
    }
    assert!(
        create_private_directory_relative(&parent, OsStr::new("too-long"), 1, &mut || Ok(()))
            .is_err()
    );
    assert!(
        create_private_directory_relative(&parent, OsStr::new("zero"), 0, &mut || Ok(())).is_err()
    );
    assert_eq!(
        create_private_directory_relative(&parent, OsStr::new("cancelled"), 4096, &mut || Err(
            io::ErrorKind::Interrupted.into()
        ))
        .unwrap_err()
        .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    let residue = temp.path().join("residue");
    assert_eq!(
        create_private_directory_relative(&parent, OsStr::new("residue"), 4096, &mut || if residue
            .exists()
        {
            Err(io::ErrorKind::Interrupted.into())
        } else {
            Ok(())
        })
        .unwrap_err()
        .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read_dir(&residue).unwrap().count(), 0);
    let observed =
        open_private_directory_relative(&parent, OsStr::new("residue"), 4096, &mut || Ok(()))
            .unwrap();
    graphit_fs::verify_private_directory(&observed).unwrap();
}

#[cfg(unix)]
#[test]
fn birth_uses_retained_parent_after_path_replacement_and_rejects_symlink() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("parent");
    fs::create_dir(&path).unwrap();
    let parent = read_directory::open_root_readonly(&path, &mut || Ok(())).unwrap();
    let retained = temp.path().join("retained");
    fs::rename(&path, &retained).unwrap();
    fs::create_dir(&path).unwrap();
    let child =
        create_private_directory_relative(&parent, OsStr::new("control"), 4096, &mut || Ok(()))
            .unwrap();
    assert!(retained.join("control").is_dir());
    assert!(!path.join("control").exists());
    drop(child);
    symlink(retained.join("control"), retained.join("alias")).unwrap();
    assert!(
        open_private_directory_relative(&parent, OsStr::new("alias"), 4096, &mut || Ok(()))
            .is_err()
    );
}

#[cfg(windows)]
#[test]
fn retained_windows_parent_blocks_path_replacement_and_stream_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("parent");
    fs::create_dir(&path).unwrap();
    let parent = read_directory::open_root_readonly(&path, &mut || Ok(())).unwrap();
    assert!(fs::rename(&path, temp.path().join("retained")).is_err());
    for child in ["a:b", "a\\b"] {
        assert!(
            create_private_directory_relative(&parent, OsStr::new(child), 4096, &mut || Ok(()))
                .is_err()
        );
    }
    create_private_directory_relative(&parent, OsStr::new("control"), 4096, &mut || Ok(()))
        .unwrap();
    assert!(path.join("control").is_dir());
}

#[cfg(target_os = "macos")]
#[test]
fn public_parent_without_inheritable_aces_allows_birth_but_inheritable_parent_is_denied() {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let parent = read_directory::open_root_readonly(temp.path(), &mut || Ok(())).unwrap();
    create_private_directory_relative(
        &parent,
        OsStr::new("public-parent-child"),
        4096,
        &mut || Ok(()),
    )
    .unwrap();
    assert!(
        Command::new("chmod")
            .args(["+a", "everyone allow read"])
            .arg(temp.path())
            .status()
            .unwrap()
            .success()
    );
    create_private_directory_relative(&parent, OsStr::new("non-inherited"), 4096, &mut || Ok(()))
        .unwrap();
    assert!(
        Command::new("chmod")
            .args([
                "+a",
                "everyone allow read,write,search,file_inherit,directory_inherit"
            ])
            .arg(temp.path())
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        create_private_directory_relative(
            &parent,
            OsStr::new("denied-directory"),
            4096,
            &mut || Ok(())
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        open_private_lock_relative(&parent, OsStr::new("denied-lock"), 4096, &mut || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(!temp.path().join("denied-directory").exists());
    assert!(!temp.path().join("denied-lock").exists());
    // The existing NO_INHERIT path producer remains available with its full ACL contract.
    let private = graphit_fs::create_private_directory(&temp.path().join("private")).unwrap();
    graphit_fs::verify_private_directory(&private).unwrap();
    open_private_lock_relative(&private, OsStr::new("lock"), 4096, &mut || Ok(())).unwrap();
}
