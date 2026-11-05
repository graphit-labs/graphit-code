use super::*;
use crate::{LinkPlan, NodeKind};
use std::{
    fs,
    sync::{Arc, Barrier},
    thread,
};

fn prepare(path: &Path, directory: bool) {
    if directory {
        fs::create_dir(path).unwrap();
        fs::create_dir(path.join("empty")).unwrap();
        fs::write(path.join("content"), b"prepared bytes\0").unwrap();
    } else {
        fs::write(path, b"prepared bytes\0").unwrap();
    }
}

fn assert_prepared(path: &Path, directory: bool) {
    assert_eq!(fs::symlink_metadata(path).unwrap().is_dir(), directory);
    if directory {
        assert!(path.join("empty").is_dir());
        assert_eq!(fs::read(path.join("content")).unwrap(), b"prepared bytes\0");
    } else {
        assert_eq!(fs::read(path).unwrap(), b"prepared bytes\0");
    }
}

#[test]
fn moves_prepared_files_and_directories_with_native_identity() {
    for directory in [false, true] {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("prepared 🚀");
        let destination = fixture.path().join("published 🚀");
        prepare(&source, directory);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source, fs::Permissions::from_mode(0o750)).unwrap();
        }
        let before = fs::symlink_metadata(&source).unwrap();
        rename_exclusive(&source, &destination).unwrap();
        assert!(!source.exists());
        assert_prepared(&destination, directory);
        let after = fs::symlink_metadata(&destination).unwrap();
        assert_eq!(
            before.permissions().readonly(),
            after.permissions().readonly()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
            assert_eq!(before.permissions().mode(), after.permissions().mode());
        }
    }
}

#[test]
fn collisions_preserve_files_empty_and_nonempty_directories_and_links() {
    for directory in [false, true] {
        for collision in 0..6 {
            let fixture = tempfile::tempdir().unwrap();
            let source = fixture.path().join("prepared");
            let destination = fixture.path().join("foreign");
            prepare(&source, directory);
            match collision {
                0 => fs::write(&destination, b"user bytes").unwrap(),
                1 => fs::create_dir(&destination).unwrap(),
                2 => {
                    fs::create_dir(&destination).unwrap();
                    fs::write(destination.join("user"), b"user bytes").unwrap();
                }
                _ => {
                    let target = fixture.path().join("user target");
                    let target_directory = collision == 5;
                    prepare(&target, target_directory);
                    LinkPlan::capture(
                        &target,
                        &destination,
                        if target_directory {
                            NodeKind::Directory
                        } else {
                            NodeKind::File
                        },
                    )
                    .unwrap()
                    .create()
                    .unwrap();
                    if collision == 4 {
                        fs::remove_file(target).unwrap();
                    }
                }
            }
            let original = fs::symlink_metadata(&destination).unwrap();
            let target = if original.file_type().is_symlink() {
                Some(fs::read_link(&destination).unwrap())
            } else {
                None
            };
            assert!(rename_exclusive(&source, &destination).is_err());
            assert_prepared(&source, directory);
            let remaining = fs::symlink_metadata(&destination).unwrap();
            assert_eq!(original.file_type(), remaining.file_type());
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(
                    (original.dev(), original.ino()),
                    (remaining.dev(), remaining.ino())
                );
            }
            if let Some(target) = target {
                assert_eq!(fs::read_link(&destination).unwrap(), target);
            }
            match collision {
                0 => assert_eq!(fs::read(&destination).unwrap(), b"user bytes"),
                1 => assert_eq!(fs::read_dir(&destination).unwrap().count(), 0),
                2 => assert_eq!(fs::read(destination.join("user")).unwrap(), b"user bytes"),
                3 | 5 => assert_prepared(&fixture.path().join("user target"), collision == 5),
                4 => assert!(!fixture.path().join("user target").exists()),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn moves_link_entry_without_following_its_target() {
    let fixture = tempfile::tempdir().unwrap();
    let target = fixture.path().join("target");
    let source = fixture.path().join("link");
    let destination = fixture.path().join("published");
    prepare(&target, false);
    LinkPlan::capture(&target, &source, NodeKind::File)
        .unwrap()
        .create()
        .unwrap();
    rename_exclusive(&source, &destination).unwrap();
    assert!(
        fs::symlink_metadata(&destination)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(destination).unwrap(),
        fs::canonicalize(&target).unwrap()
    );
    assert_prepared(&target, false);
    assert!(fs::symlink_metadata(source).is_err());
}

#[test]
fn invalid_paths_and_absent_source_leave_existing_bytes_intact() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("prepared");
    let destination = fixture.path().join("foreign");
    prepare(&source, false);
    fs::write(&destination, b"user bytes").unwrap();
    for bad in [
        Path::new("relative"),
        fixture.path().join("..").as_path(),
        fixture.path().join("prepared/.").as_path(),
        fixture.path().join("intermediate/../prepared").as_path(),
        fixture.path().join("prepared/").as_path(),
        fixture
            .path()
            .join("x".repeat(crate::MAX_PATH_BYTES + 1))
            .as_path(),
        Path::new("/"),
        fixture.path().join("nul\0suffix").as_path(),
    ] {
        assert_eq!(
            rename_exclusive(bad, &destination).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            rename_exclusive(&source, bad).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(
        rename_exclusive(&source, &source).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        rename_exclusive(&fixture.path().join("absent"), &fixture.path().join("new"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert_prepared(&source, false);
    assert_eq!(fs::read(&destination).unwrap(), b"user bytes");
}

#[test]
fn concurrent_publishers_have_exactly_one_winner() {
    let fixture = tempfile::tempdir().unwrap();
    let destination = fixture.path().join("published");
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0u8..8)
        .map(|id| {
            let source = fixture.path().join(format!("prepared-{id}"));
            fs::write(&source, [id]).unwrap();
            let destination = destination.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                (id, source.clone(), rename_exclusive(&source, &destination))
            })
        })
        .collect();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|(_, _, result)| result.is_ok())
            .count(),
        1
    );
    let winner = results
        .iter()
        .find(|(_, _, result)| result.is_ok())
        .unwrap()
        .0;
    assert_eq!(fs::read(destination).unwrap(), [winner]);
    for (id, source, result) in results {
        if result.is_err() {
            assert_eq!(fs::read(source).unwrap(), [id]);
        } else {
            assert!(fs::symlink_metadata(source).is_err());
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_native_paths_are_not_converted() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture
        .path()
        .join(OsString::from_vec(b"prepared-\xff".to_vec()));
    let destination = fixture
        .path()
        .join(OsString::from_vec(b"published-\xfe".to_vec()));
    prepare(&source, false);
    rename_exclusive(&source, &destination).unwrap();
    assert_prepared(&destination, false);
}

#[cfg(windows)]
#[test]
fn windows_validation_keeps_unpaired_native_utf16_and_rejects_nul() {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    let native = Path::new("C:/").join(OsString::from_wide(&[0xd800, 0x61]));
    assert!(validate_path(&native).is_ok());
    let invalid = Path::new("C:/").join(OsString::from_wide(&[0x61, 0, 0x62]));
    assert_eq!(
        validate_path(&invalid).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
}
