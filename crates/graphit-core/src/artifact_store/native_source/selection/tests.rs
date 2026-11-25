use super::*;
use crate::{
    artifact_store::{CapturedSourceReader, SourceReadLimits},
    brand::Brand,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::time::{Duration, Instant};
fn fixture() -> (tempfile::TempDir, PublicationScope) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let global = temp.path().join("global");
    fs::create_dir(&global).unwrap();
    let installation =
        InstallationPaths::resolve(Brand::default(), temp.path(), None, Some(&global)).unwrap();
    let paths = ProjectPaths::at(&root, &installation).unwrap();
    let scope =
        PublicationScope::project(&paths, "00000000000000000000000001".parse().unwrap()).unwrap();
    (temp, scope)
}
fn request() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ))
}
fn limits() -> ProjectSelectionLimits {
    ProjectSelectionLimits {
        max_input_items: 64,
        max_input_bytes: 4096,
        max_selected_file_bytes: 65536,
        max_control_file_bytes: 4096,
    }
}
fn criteria<'a>(
    subtree: Option<&'a Path>,
    extras: &'a [&'a Path],
    extensions: &'a [&'a str],
) -> ProjectSelectionCriteria<'a> {
    ProjectSelectionCriteria {
        subtree,
        extras,
        include_readme: false,
        extensions,
        ignore: ProjectIgnoreCriteria {
            protected_defaults: &[],
            custom_control: Some(".wikiignore"),
            git_control: Some(".gitignore"),
        },
    }
}
fn reader(
    scope: &PublicationScope,
    input: ProjectSelectionCriteria<'_>,
    caps: ProjectSelectionLimits,
    ctx: &Arc<SnapshotContext>,
) -> io::Result<CapturedSourceReader> {
    CapturedSourceReader::capture_selected_project(
        scope,
        input,
        caps,
        SourceReadLimits::new(65536, 4096, 65536).unwrap(),
        ctx,
    )
}
fn documents(reader: &CapturedSourceReader) -> Vec<PathBuf> {
    reader
        .nodes()
        .filter(|node| node.kind() == graphit_fs::NodeKind::File)
        .map(|node| node.relative().to_owned())
        .collect()
}
#[test]
fn project_paths_union_dedup_and_selected_payload_only() {
    let (_temp, scope) = fixture();
    let root = scope.owner_root();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/a.md"), b"alpha").unwrap();
    fs::write(root.join("README.md"), b"readme").unwrap();
    fs::write(root.join("docs/asset.bin"), b"not selected").unwrap();
    let extras = [
        Path::new("docs/a.md"),
        Path::new("README.md"),
        Path::new("README.md"),
    ];
    let mut input = criteria(Some(Path::new("docs")), &extras, &[".md"]);
    input.include_readme = true;
    let ctx = request();
    let r = reader(&scope, input, limits(), &ctx).unwrap();
    assert_eq!(
        documents(&r),
        [PathBuf::from("README.md"), PathBuf::from("docs/a.md")]
    );
    assert_eq!(r.retained_bytes(), 11);
    assert_eq!(
        r.open_file(Path::new("docs/a.md"))
            .unwrap()
            .read_range(1, 3)
            .unwrap()
            .as_ref(),
        b"lph"
    );
}
#[test]
fn empty_criteria_does_not_read_controls() {
    let (_temp, scope) = fixture();
    fs::write(scope.owner_root().join(".wikiignore"), [0xff]).unwrap();
    let r = reader(&scope, criteria(None, &[], &[".md"]), limits(), &request()).unwrap();
    assert!(documents(&r).is_empty());
    assert_eq!(r.nodes().len(), 1);
}
#[test]
fn first_ignored_readme_has_no_fallback_and_directory_is_not_candidate() {
    let (_temp, scope) = fixture();
    let root = scope.owner_root();
    fs::write(root.join("README.md"), b"a").unwrap();
    fs::write(root.join("readme.mdx"), b"b").unwrap();
    fs::create_dir(root.join("readme.txt")).unwrap();
    fs::write(root.join(".wikiignore"), b"README.md\n").unwrap();
    let mut input = criteria(None, &[], &[".md", ".mdx", ".txt"]);
    input.include_readme = true;
    assert!(documents(&reader(&scope, input, limits(), &request()).unwrap()).is_empty());
}
#[test]
fn nested_ignore_rules_do_not_leak_to_sibling_readme() {
    let (_temp, scope) = fixture();
    let root = scope.owner_root();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/.wikiignore"), b"hidden.md\n").unwrap();
    fs::write(root.join("docs/hidden.md"), b"hidden").unwrap();
    fs::write(root.join("docs/open.MD"), b"visible").unwrap();
    fs::write(root.join("README.md"), b"root").unwrap();
    let mut input = criteria(Some(Path::new("docs")), &[], &[".md"]);
    input.include_readme = true;
    assert_eq!(
        documents(&reader(&scope, input, limits(), &request()).unwrap()),
        [PathBuf::from("README.md"), PathBuf::from("docs/open.MD")]
    );
}
#[test]
fn excluded_subtree_does_not_read_its_control() {
    let (_temp, scope) = fixture();
    let root = scope.owner_root();
    fs::create_dir_all(root.join("docs/build")).unwrap();
    fs::write(root.join(".wikiignore"), b"docs/build/\n").unwrap();
    fs::write(root.join("docs/build/.wikiignore"), [0xff]).unwrap();
    fs::write(root.join("docs/a.md"), b"a").unwrap();
    assert_eq!(
        documents(
            &reader(
                &scope,
                criteria(Some(Path::new("docs")), &[], &[".md"]),
                limits(),
                &request()
            )
            .unwrap()
        ),
        [PathBuf::from("docs/a.md")]
    );
}
#[test]
fn missing_is_optional_but_wrong_selected_type_is_error() {
    let (_temp, scope) = fixture();
    assert!(
        documents(
            &reader(
                &scope,
                criteria(
                    Some(Path::new("missing")),
                    &[Path::new("missing.md")],
                    &[".md"]
                ),
                limits(),
                &request()
            )
            .unwrap()
        )
        .is_empty()
    );
    fs::write(scope.owner_root().join("docs"), b"not a directory").unwrap();
    assert_eq!(
        reader(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".md"]),
            limits(),
            &request()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidData
    );
    fs::create_dir(scope.owner_root().join("extra.md")).unwrap();
    assert!(
        reader(
            &scope,
            criteria(None, &[Path::new("extra.md")], &[".md"]),
            limits(),
            &request()
        )
        .is_err()
    );
}
#[test]
fn selector_and_content_bounds_fail_without_partial_result() {
    let (_temp, scope) = fixture();
    fs::create_dir(scope.owner_root().join("docs")).unwrap();
    fs::write(scope.owner_root().join("docs/a.md"), b"alpha").unwrap();
    let mut caps = limits();
    caps.max_selected_file_bytes = 4;
    assert_eq!(
        reader(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".md"]),
            caps,
            &request()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::FileTooLarge
    );
    caps = limits();
    caps.max_input_items = 1;
    assert!(
        reader(
            &scope,
            criteria(None, &[Path::new("a"), Path::new("a")], &[".md"]),
            caps,
            &request()
        )
        .is_err()
    );
    assert!(
        reader(
            &scope,
            criteria(Some(Path::new("..")), &[], &[".md"]),
            limits(),
            &request()
        )
        .is_err()
    );
    assert!(
        reader(
            &scope,
            criteria(None, &[], &["../md"]),
            limits(),
            &request()
        )
        .is_err()
    );
}
#[test]
fn invalid_or_oversized_control_is_not_absence() {
    let (_temp, scope) = fixture();
    let root = scope.owner_root();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join(".wikiignore"), [0xff]).unwrap();
    assert!(
        reader(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".md"]),
            limits(),
            &request()
        )
        .is_err()
    );
    fs::write(root.join(".wikiignore"), b"long-rule").unwrap();
    let mut caps = limits();
    caps.max_control_file_bytes = 4;
    assert_eq!(
        reader(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".md"]),
            caps,
            &request()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::FileTooLarge
    );
}
#[test]
fn cancellation_and_deadline_are_operation_errors() {
    let (_temp, scope) = fixture();
    let ctx = request();
    ctx.cancellation().cancel();
    assert_eq!(
        reader(&scope, criteria(None, &[], &[".md"]), limits(), &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    assert_eq!(
        reader(&scope, criteria(None, &[], &[".md"]), limits(), &expired)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
}
#[test]
fn changed_directory_coverage_includes_missing_paths_without_hashing_siblings() {
    let (_temp, scope) = fixture();
    let root = scope.owner_root();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/a.md"), b"a").unwrap();
    fs::write(root.join("docs/b.md"), b"b").unwrap();
    let changes = BTreeSet::from([PathBuf::from("docs/a.md"), PathBuf::from("docs/gone.md")]);
    let ctx = request();
    let r = CapturedSourceReader::capture_selected_project_changes_metadata(
        &scope,
        criteria(Some(Path::new("docs")), &[], &[".md"]),
        &changes,
        limits(),
        SourceReadLimits::new(64, 8, 64).unwrap(),
        &ctx,
    )
    .unwrap();
    assert_eq!(documents(&r), [PathBuf::from("docs/a.md")]);
    assert!(r.covers_selected_path(Path::new("docs/gone.md")).unwrap());
    assert!(!r.covers_selected_path(Path::new("docs/b.md")).unwrap());
    assert_eq!(r.retained_bytes(), 0);
    assert!(
        r.nodes()
            .filter(|n| n.kind() == graphit_fs::NodeKind::File)
            .all(|n| n.digest().is_some())
    );
}
#[test]
fn changed_paths_share_selector_count_and_byte_limits() {
    let (_temp, scope) = fixture();
    let changes = BTreeSet::from([PathBuf::from("docs/a.md"), PathBuf::from("docs/b.md")]);
    for caps in [
        ProjectSelectionLimits {
            max_input_items: 5,
            ..limits()
        },
        ProjectSelectionLimits {
            max_input_bytes: 35,
            ..limits()
        },
    ] {
        assert_eq!(
            selected_project_changed_paths(
                &scope,
                criteria(Some(Path::new("docs")), &[], &[".md"]),
                &changes,
                caps,
                &request(),
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
}
#[test]
fn control_must_be_a_regular_file_before_it_is_opened() {
    let (_temp, scope) = fixture();
    fs::create_dir(scope.owner_root().join(".wikiignore")).unwrap();
    assert_eq!(
        reader(
            &scope,
            criteria(Some(Path::new(".")), &[], &[".md"]),
            limits(),
            &request()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidData
    );
}
#[cfg(unix)]
#[test]
fn control_fifo_is_rejected_without_opening_a_blocking_reader() {
    let (_temp, scope) = fixture();
    #[cfg(not(target_os = "macos"))]
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        scope.owner_root().join(".wikiignore"),
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::FileTypeExt;
        let path = scope.owner_root().join(".wikiignore");
        assert!(
            std::process::Command::new("/usr/bin/mkfifo")
                .args(["-m", "600"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
    }
    assert_eq!(
        reader(
            &scope,
            criteria(Some(Path::new(".")), &[], &[".md"]),
            limits(),
            &request()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidData
    );
}
#[cfg(unix)]
#[test]
fn linked_directories_and_hardlinked_documents_are_selected_normally() {
    use std::os::unix::fs::symlink;
    let (temp, scope) = fixture();
    let external = temp.path().join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("a.md"), b"linked").unwrap();
    fs::hard_link(external.join("a.md"), external.join("b.md")).unwrap();
    symlink(&external, scope.owner_root().join("docs")).unwrap();
    let r = reader(
        &scope,
        criteria(Some(Path::new("docs")), &[], &[".md"]),
        limits(),
        &request(),
    )
    .unwrap();
    assert_eq!(
        documents(&r),
        [PathBuf::from("docs/a.md"), PathBuf::from("docs/b.md")]
    );
    assert_eq!(r.retained_bytes(), 12);
}
#[cfg(unix)]
#[test]
fn linked_directory_cycle_is_reported_by_walker_without_recursion_loop() {
    use std::os::unix::fs::symlink;
    let (_temp, scope) = fixture();
    fs::create_dir(scope.owner_root().join("docs")).unwrap();
    symlink(
        scope.owner_root().join("docs"),
        scope.owner_root().join("docs/again"),
    )
    .unwrap();
    assert!(
        reader(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".md"]),
            limits(),
            &request()
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn dangling_optional_documents_and_unselected_links_do_not_block_real_documents() {
    use std::os::unix::fs::symlink;
    let (_temp, scope) = fixture();
    fs::create_dir(scope.owner_root().join("docs")).unwrap();
    fs::write(scope.owner_root().join("docs/guide.md"), b"guide").unwrap();
    symlink("absent", scope.owner_root().join("docs/missing.md")).unwrap();
    symlink("absent", scope.owner_root().join("unselected.bin")).unwrap();
    let selected = reader(
        &scope,
        criteria(
            Some(Path::new("docs")),
            &[Path::new("docs/missing.md")],
            &[".md"],
        ),
        limits(),
        &request(),
    )
    .unwrap();
    assert_eq!(documents(&selected), [PathBuf::from("docs/guide.md")]);
    assert_eq!(
        selected.retained(Path::new("docs/guide.md")),
        Some(b"guide".as_slice())
    );
}
