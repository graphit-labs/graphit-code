use graphit_fs::{LinkCreation, LinkPlan, LinkStage, NodeKind};
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
        let plan =
            LinkPlan::capture(&source, &root.path().join(format!("link-{name}")), kind).unwrap();
        assert_eq!(plan.source(), fs::canonicalize(&source).unwrap());
        let outcome = plan.create().expect(
            "file links require Windows symlink capability; directory links can use a junction",
        );
        assert!(matches!(
            (kind, outcome),
            (NodeKind::File, LinkCreation::FileSymlink)
                | (
                    NodeKind::Directory,
                    LinkCreation::DirectorySymlink | LinkCreation::DirectoryJunction
                )
        ));
        let read = if kind == NodeKind::Directory {
            plan.destination().join("keep")
        } else {
            plan.destination().to_owned()
        };
        assert_eq!(fs::read(&read).unwrap(), b"preserved");
        assert_eq!(
            fs::canonicalize(fs::read_link(plan.destination()).unwrap()).unwrap(),
            plan.source()
        );
        #[cfg(unix)]
        fs::remove_file(plan.destination()).unwrap();
        #[cfg(windows)]
        match kind {
            NodeKind::File => fs::remove_file(plan.destination()).unwrap(),
            NodeKind::Directory => fs::remove_dir(plan.destination()).unwrap(),
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
fn every_existing_destination_is_preserved_including_after_capture() {
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
        let plan = LinkPlan::capture(&source, &destination, NodeKind::File).unwrap();
        match category {
            "file" => fs::write(&destination, b"user data").unwrap(),
            "empty-directory" => fs::create_dir(&destination).unwrap(),
            "nonempty-directory" => {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("keep"), b"user data").unwrap();
            }
            _ => symlink(&root.path().join("missing"), &destination),
        }
        for error in [
            plan.create().unwrap_err(),
            LinkPlan::capture(&source, &destination, NodeKind::File).unwrap_err(),
        ] {
            assert_eq!(
                (error.stage(), error.kind(), error.cleanup_kind()),
                (LinkStage::Destination, io::ErrorKind::AlreadyExists, None)
            );
        }
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
fn invalid_context_source_type_and_overlap_have_no_creation_effect() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let directory = root.path().join("directory");
    fs::write(&source, b"source").unwrap();
    fs::create_dir(&directory).unwrap();
    let target = root.path().join("not-created");
    for (src, dst, kind) in [
        (Path::new("relative"), target.as_path(), NodeKind::File),
        (source.as_path(), Path::new("relative"), NodeKind::File),
        (source.as_path(), target.as_path(), NodeKind::Directory),
        (directory.as_path(), target.as_path(), NodeKind::File),
        (source.as_path(), source.as_path(), NodeKind::File),
        (
            directory.as_path(),
            directory.join("link").as_path(),
            NodeKind::Directory,
        ),
        (
            source.as_path(),
            root.path().join("..").as_path(),
            NodeKind::File,
        ),
    ] {
        assert!(LinkPlan::capture(src, dst, kind).is_err());
    }
    assert!(!target.exists());
    for destination in [
        directory.join("."),
        directory.join(".."),
        directory.join("").join("."),
    ] {
        assert_eq!(
            LinkPlan::capture(&source, &destination, NodeKind::File)
                .unwrap_err()
                .stage(),
            LinkStage::Validate
        );
    }
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
    let missing = root.path().join("missing");
    assert_eq!(
        LinkPlan::capture(&missing, &target, NodeKind::File)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        LinkPlan::capture(&source, &missing.join("child"), NodeKind::File)
            .unwrap_err()
            .stage(),
        LinkStage::Parent
    );
    assert!(!missing.exists());
    let oversized = root.path().join("a".repeat(65537));
    assert_eq!(
        LinkPlan::capture(&source, &oversized, NodeKind::File)
            .unwrap_err()
            .stage(),
        LinkStage::Validate
    );
    let plan = LinkPlan::capture(&source, &target, NodeKind::File).unwrap();
    fs::remove_file(&source).unwrap();
    fs::create_dir(&source).unwrap();
    assert_eq!(plan.create().unwrap_err().stage(), LinkStage::Source);
    assert!(!target.exists());
}

#[cfg(unix)]
#[test]
fn aliases_are_captured_and_native_paths_stay_lossless() {
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
    let plan = LinkPlan::capture(
        &source_alias.join(source.file_name().unwrap()),
        &parent_alias.join("linked"),
        NodeKind::File,
    )
    .unwrap();
    assert_eq!(plan.source(), fs::canonicalize(&source).unwrap());
    assert_eq!(
        plan.destination(),
        fs::canonicalize(&parent).unwrap().join("linked")
    );
    // Changing an alias after capture must not change the actual destination.
    fs::remove_file(&parent_alias).unwrap();
    symlink(&source_root, &parent_alias);
    plan.create().unwrap();
    assert_eq!(fs::read(plan.destination()).unwrap(), b"native bytes");
    assert!(!source_root.join("linked").exists());
    assert!(
        LinkPlan::capture(
            &source_root,
            &parent_alias.join("cycle"),
            NodeKind::Directory
        )
        .is_err()
    );
}
