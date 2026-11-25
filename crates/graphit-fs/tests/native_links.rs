use graphit_fs::{LinkStage, NodeKind, create_link};
use std::{fs, io, path::Path};

#[cfg(unix)]
fn symlink(source: &Path, destination: &Path) {
    std::os::unix::fs::symlink(source, destination).unwrap();
}
#[cfg(windows)]
fn symlink(source: &Path, destination: &Path) {
    std::os::windows::fs::symlink_file(source, destination)
        .expect("file symlink tests require Windows Developer Mode or granted symlink privilege");
}

#[test]
fn file_and_directory_readthrough_preserve_sources() {
    let root = tempfile::tempdir().unwrap();
    for (kind, name) in [(NodeKind::File, "file 🚀"), (NodeKind::Directory, "dir 🚀")] {
        let source = root.path().join(name);
        if kind == NodeKind::Directory {
            fs::create_dir(&source).unwrap();
            fs::write(source.join("keep"), b"preserved").unwrap();
        } else {
            fs::write(&source, b"preserved").unwrap();
        }
        let destination = root.path().join(format!("link-{name}"));
        create_link(&source, &destination).expect(
            "file links require Windows symlink capability; directory links can use a junction",
        );
        let read = if kind == NodeKind::Directory {
            destination.join("keep")
        } else {
            destination.to_owned()
        };
        assert_eq!(fs::read(&read).unwrap(), b"preserved");
        assert_eq!(
            fs::canonicalize(fs::read_link(&destination).unwrap()).unwrap(),
            fs::canonicalize(&source).unwrap()
        );
        #[cfg(unix)]
        fs::remove_file(&destination).unwrap();
        #[cfg(windows)]
        match kind {
            NodeKind::File => fs::remove_file(&destination).unwrap(),
            NodeKind::Directory => fs::remove_dir(&destination).unwrap(),
        }
        let original = if kind == NodeKind::Directory {
            source.join("keep")
        } else {
            source
        };
        assert_eq!(fs::read(original).unwrap(), b"preserved");
    }
}

#[test]
fn every_existing_destination_is_preserved_by_the_native_create() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::write(&source, b"source").unwrap();
    for category in [
        "file",
        "empty-directory",
        "nonempty-directory",
        "broken-link",
    ] {
        let destination = root.path().join(category);
        match category {
            "file" => fs::write(&destination, b"user data").unwrap(),
            "empty-directory" => fs::create_dir(&destination).unwrap(),
            "nonempty-directory" => {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("keep"), b"user data").unwrap();
            }
            _ => symlink(&root.path().join("missing"), &destination),
        }
        let error = create_link(&source, &destination).unwrap_err();
        // Native collision classification varies by object type and platform.
        assert_eq!(
            (error.stage(), error.cleanup_kind()),
            (LinkStage::Symlink, None)
        );
        match category {
            "file" => assert_eq!(fs::read(&destination).unwrap(), b"user data"),
            "empty-directory" => assert_eq!(fs::read_dir(&destination).unwrap().count(), 0),
            "nonempty-directory" => {
                assert_eq!(fs::read(destination.join("keep")).unwrap(), b"user data")
            }
            _ => assert_eq!(
                fs::read_link(&destination).unwrap(),
                root.path().join("missing")
            ),
        }
    }
    assert_eq!(fs::read(source).unwrap(), b"source");
}

#[test]
fn missing_parent_keeps_native_error_without_effects() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("not-created");
    let missing = root.path().join("missing");
    fs::write(&source, b"source").unwrap();
    let error = create_link(&source, &missing.join("child")).unwrap_err();
    assert_eq!(
        (error.stage(), error.kind()),
        (LinkStage::Symlink, io::ErrorKind::NotFound)
    );
    assert!(!missing.exists());
    assert!(!target.exists());
    assert_eq!(fs::read(source).unwrap(), b"source");
}

#[test]
fn directory_link_inside_its_source_uses_normal_native_resolution() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("directory");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("keep"), b"preserved").unwrap();
    let target = source.join("link");
    create_link(&source, &target).unwrap();
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"preserved");
    assert_eq!(fs::read(source.join("keep")).unwrap(), b"preserved");
}

#[cfg(unix)]
#[test]
fn dangling_source_is_an_ordinary_native_symlink() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("missing");
    let target = root.path().join("link");
    create_link(&source, &target).unwrap();
    assert_eq!(fs::read_link(&target).unwrap(), source);
    assert!(!target.exists());
    assert!(
        fs::symlink_metadata(target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn configured_aliases_resolve_at_creation_and_native_paths_stay_lossless() {
    let root = tempfile::tempdir().unwrap();
    let source_root = root.path().join("source");
    let parent = root.path().join("parent");
    fs::create_dir(&source_root).unwrap();
    fs::create_dir(&parent).unwrap();
    #[cfg(target_os = "linux")]
    let source = {
        use std::os::unix::ffi::OsStringExt;
        source_root.join(std::ffi::OsString::from_vec(b"file-\xff".to_vec()))
    };
    #[cfg(not(target_os = "linux"))]
    let source = source_root.join("file-é 🚀");
    fs::write(&source, b"native bytes").unwrap();
    let source_alias = root.path().join("source-alias");
    let parent_alias = root.path().join("parent-alias");
    symlink(&source_root, &source_alias);
    symlink(&parent, &parent_alias);
    let selected_source = source_alias.join(source.file_name().unwrap());
    let destination = parent_alias.join("linked");
    // Configured aliases resolve when the native operation runs.
    fs::remove_file(&parent_alias).unwrap();
    symlink(&source_root, &parent_alias);
    create_link(&selected_source, &destination).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"native bytes");
    assert_eq!(
        fs::read(source_root.join("linked")).unwrap(),
        b"native bytes"
    );
    assert!(!parent.join("linked").exists());
}
