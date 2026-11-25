use super::*;
use crate::create_link;
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
fn moves_prepared_files_and_directories_preserving_content() {
    for directory in [false, true] {
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("prepared 🚀");
        let destination = fixture.path().join("published 🚀");
        prepare(&source, directory);
        rename_exclusive(&source, &destination).unwrap();
        assert!(!source.exists());
        assert_prepared(&destination, directory);
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
                    create_link(&target, &destination).unwrap();
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
    create_link(&target, &source).unwrap();
    let stored_target = fs::read_link(&source).unwrap().into_os_string();
    rename_exclusive(&source, &destination).unwrap();
    assert!(
        fs::symlink_metadata(&destination)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(destination).unwrap().into_os_string(),
        stored_target
    );
    assert_prepared(&target, false);
    assert!(fs::symlink_metadata(source).is_err());
}

#[test]
fn nul_paths_and_absent_source_leave_existing_bytes_intact() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("prepared");
    let destination = fixture.path().join("foreign");
    prepare(&source, false);
    fs::write(&destination, b"user bytes").unwrap();
    let bad = fixture.path().join("nul\0suffix");
    assert_eq!(
        rename_exclusive(&bad, &destination).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        rename_exclusive(&source, &bad).unwrap_err().kind(),
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
fn windows_long_native_paths_move_without_clobbering() {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
    };

    for (directory, absolute) in [(false, false), (false, true), (true, false), (true, true)] {
        let working_directory = std::env::current_dir().unwrap();
        let fixture = if absolute {
            tempfile::tempdir().unwrap()
        } else {
            tempfile::tempdir_in(&working_directory).unwrap()
        };
        let mut parent = if absolute {
            std::path::absolute(fixture.path()).unwrap()
        } else {
            fixture
                .path()
                .strip_prefix(&working_directory)
                .unwrap()
                .to_owned()
        };
        assert_eq!(parent.is_absolute(), absolute);
        // Keep conventional Win32 paths at the public API boundary. std::fs
        // prepares the actual entries beyond MAX_PATH without shortening names.
        assert!(!matches!(
            parent.components().next(),
            Some(std::path::Component::Prefix(prefix)) if prefix.kind().is_verbatim()
        ));
        while parent.as_os_str().encode_wide().count() < 320 {
            parent.push("native-long-path-🚀-segment");
        }
        fs::create_dir_all(&parent).unwrap();
        let mut source_name = OsString::from("prepared 🚀");
        source_name.push(OsString::from_wide(&[0xd800]));
        let source = parent.join(source_name);
        let destination = parent.join("published 🚀");
        prepare(&source, directory);
        rename_exclusive(&source, &destination).unwrap();
        assert!(!source.exists());
        assert_prepared(&destination, directory);

        prepare(&source, directory);
        assert!(rename_exclusive(&source, &destination).is_err());
        assert_prepared(&source, directory);
        assert_prepared(&destination, directory);
    }
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

#[cfg(all(windows, target_pointer_width = "64"))]
#[test]
fn windows_long_unc_adaptation_is_lexical_and_preserves_unicode() {
    use std::{os::windows::ffi::OsStrExt, path::PathBuf};

    let mut input = PathBuf::from(r"\\graphit.invalid\share");
    let mut expected = PathBuf::from(r"\\?\UNC\graphit.invalid\share");
    for _ in 0..4 {
        let segment = "segment-🚀-".repeat(8);
        input.push(&segment);
        expected.push(&segment);
    }
    assert!(input.as_os_str().encode_wide().count() > 260);
    // This calls only lexical path APIs. No share is opened or resolved, and
    // this assertion does not qualify an actual rename on a remote filesystem.
    assert_eq!(windows_rename_path(&input).unwrap(), expected);
}

#[cfg(unix)]
#[test]
fn linked_parents_and_native_parent_components_resolve_normally() {
    let fixture = tempfile::tempdir().unwrap();
    let real = fixture.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::create_dir(real.join("child")).unwrap();
    let configured = fixture.path().join("configured");
    std::os::unix::fs::symlink(&real, &configured).unwrap();
    fs::write(real.join("source"), b"published bytes").unwrap();
    let source = configured.join("child/../source");
    let destination = configured.join("destination");
    rename_exclusive(&source, &destination).unwrap();
    assert_eq!(
        fs::read(real.join("destination")).unwrap(),
        b"published bytes"
    );
    assert!(!real.join("source").exists());
}
