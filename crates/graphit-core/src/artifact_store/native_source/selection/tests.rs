use super::super::tests::{fixture, read_limits, request};
use super::*;
use crate::artifact_store::{CapturedSourceReader, SourceReadLimits};
#[cfg(unix)]
use std::sync::Mutex;

fn limits() -> ProjectSelectionLimits {
    ProjectSelectionLimits {
        max_depth: 16,
        max_input_items: 64,
        max_input_bytes: 4096,
        max_observed_path_bytes: 4096,
        max_selected_file_bytes: 65536,
        max_control_file_bytes: 4096,
    }
}
fn criteria<'a>(
    tree: Option<&'a Path>,
    extras: &'a [&'a Path],
    extensions: &'a [&'a str],
) -> ProjectSelectionCriteria<'a> {
    ProjectSelectionCriteria {
        subtree: tree,
        extras,
        include_readme: false,
        extensions,
        ignore: ProjectIgnoreCriteria {
            protected_defaults: &[],
            custom_control: None,
            git_control: None,
        },
    }
}
fn capture_with(
    scope: &CapturedPublicationScope,
    input: ProjectSelectionCriteria<'_>,
    caps: ProjectSelectionLimits,
    request: &std::sync::Arc<crate::file_io::SnapshotContext>,
) -> io::Result<CapturedSourceReader> {
    CapturedSourceReader::capture_selected_project(scope, input, caps, read_limits(), request)
}
fn documents(reader: &CapturedSourceReader) -> Vec<PathBuf> {
    reader
        .nodes()
        .filter(|node| node.kind() == NodeKind::File)
        .map(|node| node.relative().to_owned())
        .collect()
}
#[test]
fn project_paths_union_dedup_and_selected_payload_only() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("docs")).unwrap();
    std::fs::write(paths.root().join("docs/a.md"), b"alpha").unwrap();
    std::fs::write(paths.root().join("README.md"), b"readme").unwrap();
    let original = request();
    let doc = Path::new("docs").join("a.md");
    let extras = [
        doc.as_path(),
        Path::new("README.md"),
        Path::new("README.md"),
    ];
    let mut input = criteria(Some(Path::new("docs")), &extras, &[".md"]);
    input.include_readme = true;
    let reader = capture_with(&scope, input, limits(), &original).unwrap();
    assert_eq!(
        documents(&reader),
        [PathBuf::from("README.md"), Path::new("docs").join("a.md")]
    );

    assert_eq!(reader.retained_bytes(), 11);
    let file = reader.open_file(&Path::new("docs").join("a.md")).unwrap();
    assert_eq!(file.read_range(1, 3).unwrap().as_ref(), b"lph");
    reader.check_request(&original).unwrap();
}
#[test]
fn empty_criteria_does_not_enumerate_or_read_controls() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join(".wikiignore"), [0xff]).unwrap();
    let original = request();
    let mut input = criteria(None, &[], &[".md"]);
    input.ignore.custom_control = Some(".wikiignore");
    let caps = limits();
    let reader = capture_with(&scope, input, caps, &original).unwrap();
    assert!(documents(&reader).is_empty());
    assert_eq!(reader.nodes().count(), 1);
}
#[test]
fn last_extension_ascii_fold_and_dotfiles_keep_observed_spelling() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("docs")).unwrap();
    for name in ["a.MD", "a.part.md", ".md", ".hidden.md", "without", "a.md."] {
        std::fs::write(paths.root().join("docs").join(name), b"x").unwrap();
    }
    let original = request();
    let reader = capture_with(
        &scope,
        criteria(Some(Path::new("docs")), &[], &[".md"]),
        limits(),
        &original,
    )
    .unwrap();
    assert_eq!(
        documents(&reader),
        [
            Path::new("docs").join(".hidden.md"),
            Path::new("docs").join("a.MD"),
            Path::new("docs").join("a.part.md")
        ]
    );
}
#[cfg(unix)]
#[test]
fn unicode_extensions_are_exact_and_invalid_names_are_rejected_without_loss() {
    use std::os::unix::ffi::OsStringExt;
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("docs")).unwrap();
    let bad = std::ffi::OsString::from_vec(b"bad\xff.md".to_vec());
    let invalid_ext = std::ffi::OsString::from_vec(b"normal.\xff".to_vec());
    for name in [
        std::ffi::OsString::from("a.é"),
        std::ffi::OsString::from("b.É"),
        std::ffi::OsString::from("c.e\u{301}"),
    ] {
        std::fs::write(paths.root().join("docs").join(name), b"x").unwrap();
    }
    let original = request();
    let reader = capture_with(
        &scope,
        criteria(Some(Path::new("docs")), &[], &[".é", ".md"]),
        limits(),
        &original,
    )
    .unwrap();
    assert_eq!(documents(&reader), [Path::new("docs").join("a.é")]);
    for name in [bad, invalid_ext] {
        let path = paths.root().join("docs").join(name);
        std::fs::write(&path, b"x").unwrap();
        let error = capture_with(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".é", ".md"]),
            limits(),
            &original,
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("Unicode"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), b"x");
        std::fs::remove_file(path).unwrap();
    }
}
#[test]
fn first_ignored_readme_has_no_fallback_and_directory_is_not_candidate() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("README.md"), b"a").unwrap();
    std::fs::write(paths.root().join("readme.mdx"), b"b").unwrap();
    std::fs::create_dir(paths.root().join("readme.txt")).unwrap();
    std::fs::write(paths.root().join(".wikiignore"), b"README.md\n").unwrap();
    let original = request();
    let mut input = criteria(None, &[], &[".md", ".mdx", ".txt"]);
    input.include_readme = true;
    input.ignore.custom_control = Some(".wikiignore");
    let reader = capture_with(&scope, input, limits(), &original).unwrap();
    assert!(documents(&reader).is_empty());
}
#[test]
fn ignored_first_readme_membership_invalidates_only_when_readme_selection_is_enabled() {
    for include_readme in [false, true] {
        for create in [false, true] {
            let (_temp, paths, _birth, scope) = fixture();
            std::fs::create_dir(paths.root().join("docs")).unwrap();
            std::fs::write(paths.root().join("docs/main.md"), b"main").unwrap();
            std::fs::write(paths.root().join("readme.mdx"), b"visible").unwrap();
            std::fs::write(paths.root().join(".wikiignore"), b"README.md\n").unwrap();
            let ignored_first = paths.root().join("README.md");
            if !create {
                std::fs::write(&ignored_first, b"ignored").unwrap();
            }
            let original = request();
            let extras = [Path::new("docs/main.md")];
            let input = || {
                let mut input = criteria(None, &extras, &[".md", ".mdx"]);
                input.include_readme = include_readme;
                input.ignore.custom_control = Some(".wikiignore");
                input
            };
            let reader = CapturedSourceReader::capture_selected_project_metadata(
                &scope,
                input(),
                limits(),
                read_limits(),
                &original,
            )
            .unwrap();
            assert_eq!(
                documents(&reader).contains(&PathBuf::from("readme.mdx")),
                include_readme && create
            );
            if create {
                std::fs::write(&ignored_first, b"ignored").unwrap();
            } else {
                std::fs::remove_file(&ignored_first).unwrap();
            }
            if include_readme {
                assert!(reader.revalidate_bindings().is_err());
                assert!(reader.revalidate_content().is_err());
            } else {
                reader.revalidate_bindings().unwrap();
                reader.revalidate_content().unwrap();
            }
            let fresh = capture_with(&scope, input(), limits(), &original).unwrap();
            assert_eq!(
                documents(&fresh).contains(&PathBuf::from("readme.mdx")),
                include_readme && !create
            );
            assert_eq!(
                std::fs::read(paths.root().join("docs/main.md")).unwrap(),
                b"main"
            );
        }
    }
}
#[test]
fn ignored_subtree_never_reads_child_control() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("docs")).unwrap();
    std::fs::write(paths.root().join("docs/.wikiignore"), [0xff]).unwrap();
    std::fs::write(paths.root().join("docs/a.md"), b"a").unwrap();
    let original = request();
    let mut input = criteria(Some(Path::new("docs")), &[], &[".md"]);
    input.ignore.protected_defaults = &["docs/"];
    input.ignore.custom_control = Some(".wikiignore");
    let reader = capture_with(&scope, input, limits(), &original).unwrap();
    assert!(documents(&reader).is_empty());
}
#[test]
fn ignored_build_churn_with_unrelated_negation_preserves_selected_metadata() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.rs"), b"fn original() {}").unwrap();
    std::fs::write(paths.root().join(".gitignore"), b"target/\n!.env.example\n").unwrap();
    std::fs::create_dir(paths.root().join("target")).unwrap();
    std::fs::write(paths.root().join("target/.wikiignore"), [0xff]).unwrap();
    std::fs::write(paths.root().join("target/hidden.rs"), b"not selected").unwrap();
    let original = request();
    let capture = |tree, extras| {
        let mut input = criteria(tree, extras, &[".rs"]);
        input.ignore.git_control = Some(".gitignore");
        input.ignore.custom_control = Some(".wikiignore");
        CapturedSourceReader::capture_selected_project_metadata(
            &scope,
            input,
            limits(),
            read_limits(),
            &original,
        )
        .unwrap()
    };
    let reader = capture(Some(Path::new(".")), &[]);
    assert_eq!(documents(&reader), [PathBuf::from("a.rs")]);
    assert_eq!(reader.retained_bytes(), 0);
    assert!(documents(&capture(Some(Path::new("target")), &[])).is_empty());
    assert!(documents(&capture(None, &[Path::new("target/hidden.rs")])).is_empty());
    std::fs::write(paths.root().join("target/new.rs"), b"build churn").unwrap();
    reader.revalidate_content().unwrap();
    std::fs::write(paths.root().join("a.rs"), b"fn changed() {}").unwrap();
    assert_eq!(
        reader.revalidate_content().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn custom_deep_rules_win_without_leaking_to_readme() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("docs")).unwrap();
    std::fs::write(paths.root().join(".wikiignore"), b"*.md\n").unwrap();
    std::fs::write(paths.root().join("docs/.wikiignore"), b"!a.md\n").unwrap();
    std::fs::write(paths.root().join("docs/a.md"), b"a").unwrap();
    std::fs::write(paths.root().join("README.md"), b"r").unwrap();
    let original = request();
    let mut input = criteria(Some(Path::new("docs")), &[], &[".md"]);
    input.include_readme = true;
    input.ignore.custom_control = Some(".wikiignore");
    let reader = capture_with(&scope, input, limits(), &original).unwrap();
    assert_eq!(documents(&reader), [Path::new("docs").join("a.md")]);
}
#[test]
fn equal_control_names_read_once_but_raw_inputs_count_twice() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join(".gitignore"), b"*.md\n").unwrap();
    std::fs::write(paths.root().join("README.md"), b"r").unwrap();
    let original = request();
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.custom_control = Some(".gitignore");
    input.ignore.git_control = Some(".gitignore");
    let reader = capture_with(&scope, input, limits(), &original).unwrap();
    assert!(documents(&reader).is_empty());

    let other = request();
    let mut caps = limits();
    caps.max_input_items = 3;
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.custom_control = Some(".gitignore");
    input.ignore.git_control = Some(".gitignore");
    assert_eq!(
        capture_with(&scope, input, caps, &other)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
}
#[test]
fn missing_is_optional_but_wrong_type_is_integral_error() {
    let (_temp, paths, _birth, scope) = fixture();
    let original = request();
    assert!(
        documents(
            &capture_with(
                &scope,
                criteria(
                    Some(Path::new("missing")),
                    &[Path::new("missing-extra.md")],
                    &[".md"]
                ),
                limits(),
                &original
            )
            .unwrap()
        )
        .is_empty()
    );
    std::fs::write(paths.root().join("docs"), b"not directory").unwrap();
    let other = request();
    assert_eq!(
        capture_with(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".md"]),
            limits(),
            &other
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        capture_with(
            &scope,
            criteria(None, &[Path::new("Memory")], &[".md"]),
            limits(),
            &other
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidData
    );
}
#[test]
fn malformed_criteria_and_duplicate_raw_caps_fail_before_io() {
    let (_temp, _paths, _birth, scope) = fixture();
    for suffix in [
        "",
        ".",
        "..",
        "..md",
        ".x.y",
        ".x/y",
        ".a b",
        "../Dockerfile",
        "a\\b",
        "a:b",
        "bad\0name",
    ] {
        let original = request();
        assert_eq!(
            capture_with(
                &scope,
                criteria(Some(Path::new(".")), &[], &[suffix]),
                limits(),
                &original
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    let original = request();
    let extras = [Path::new("a.md"), Path::new("a.md")];
    let mut caps = limits();
    caps.max_input_items = 2;
    assert_eq!(
        capture_with(&scope, criteria(None, &extras, &[".md"]), caps, &original)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
}
#[test]
fn selected_file_limit_admits_complete_union_before_payload() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.md"), b"abcd").unwrap();
    std::fs::write(paths.root().join("b.md"), b"xyz").unwrap();
    for bytes in [3, 4] {
        let original = request();
        let mut caps = limits();
        caps.max_selected_file_bytes = bytes;
        let result = capture_with(
            &scope,
            criteria(Some(Path::new(".")), &[], &[".md"]),
            caps,
            &original,
        );
        if bytes == 3 {
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::FileTooLarge);
        } else {
            assert_eq!(result.unwrap().retained_bytes(), 7);
        }
    }
}

#[test]
fn individual_path_limit_includes_unsupported_entries() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("long-unsupported.bin"), b"x").unwrap();
    let original = request();
    let mut caps = limits();
    caps.max_observed_path_bytes = 3;
    assert_eq!(
        capture_with(
            &scope,
            criteria(Some(Path::new(".")), &[], &[".md"]),
            caps,
            &original
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::FileTooLarge
    );
}

#[test]
fn control_caps_and_parse_errors_do_not_become_absence() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join(".wikiignore"), b"abcd\n").unwrap();
    for cap in [4, 5] {
        let original = request();
        let mut caps = limits();
        caps.max_control_file_bytes = cap;
        let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
        input.ignore.custom_control = Some(".wikiignore");
        let result = capture_with(&scope, input, caps, &original);
        if cap == 4 {
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::FileTooLarge);
        } else {
            result.unwrap();
        }
    }
    std::fs::write(paths.root().join(".wikiignore"), [0xff]).unwrap();
    let original = request();
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.custom_control = Some(".wikiignore");
    assert!(capture_with(&scope, input, limits(), &original).is_err());
}
#[test]
fn negative_control_and_new_document_invalidate_same_snapshot() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.md"), b"a").unwrap();
    let original = request();
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.custom_control = Some(".wikiignore");
    let reader = capture_with(&scope, input, limits(), &original).unwrap();
    std::fs::write(paths.root().join(".wikiignore"), b"*.md\n").unwrap();
    assert_eq!(
        reader.open_file(Path::new("a.md")).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
}
#[test]
fn explicit_revalidation_reads_only_selected_payload() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.md"), b"abcd").unwrap();
    let huge = std::fs::File::create(paths.root().join("huge.bin")).unwrap();
    huge.set_len(100_000_000).unwrap();
    drop(huge);
    let original = request();
    let reader = capture_with(
        &scope,
        criteria(Some(Path::new(".")), &[], &[".md"]),
        limits(),
        &original,
    )
    .unwrap();
    reader.revalidate_content().unwrap();
}
#[test]
fn original_cancellation_stops_capture_and_existing_ranges() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.md"), b"abcd").unwrap();
    let original = request();
    original.cancellation().cancel();
    assert_eq!(
        capture_with(
            &scope,
            criteria(Some(Path::new(".")), &[], &[".md"]),
            limits(),
            &original
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::Interrupted
    );
}
#[cfg(unix)]
#[test]
fn excluded_permission_denied_payload_and_symlink_stay_unopened() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let (_temp, paths, _birth, scope) = fixture();
    let file = paths.root().join("blocked.bin");
    std::fs::write(&file, b"secret").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o0)).unwrap();
    symlink("blocked.bin", paths.root().join("link.md")).unwrap();
    let original = request();
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.protected_defaults = &["link.md"];
    let reader = capture_with(&scope, input, limits(), &original).unwrap();
    assert!(documents(&reader).is_empty());

    let other = request();
    assert_eq!(
        capture_with(
            &scope,
            criteria(Some(Path::new(".")), &[], &[".md"]),
            limits(),
            &other
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidData
    );
}
#[test]
fn cancellation_after_capture_keeps_original_request_and_last_owner() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.md"), b"abcd").unwrap();
    let original = request();
    let reader = capture_with(
        &scope,
        criteria(Some(Path::new(".")), &[], &[".md"]),
        limits(),
        &original,
    )
    .unwrap();
    let file = reader.open_file(Path::new("a.md")).unwrap();
    let range = file.read_range(0, 2).unwrap();
    drop(reader);
    drop(file);
    assert_eq!(range.as_ref(), b"ab");
    drop(range);
    let other = request();
    let reader = capture_with(
        &scope,
        criteria(Some(Path::new(".")), &[], &[".md"]),
        limits(),
        &other,
    )
    .unwrap();
    other.cancellation().cancel();
    assert_eq!(
        reader.revalidate_content().unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
}

#[test]
fn checked_overflow_and_zero_cap_are_input_errors_without_payload() {
    let parent = Path::new("docs");
    let child = parent.join("a.md");
    assert_eq!(
        native_name_bytes(child.as_os_str()).unwrap(),
        native_name_bytes(parent.as_os_str()).unwrap()
            + separator_bytes(parent)
            + native_name_bytes(OsStr::new("a.md")).unwrap()
    );
    assert_eq!(separator_bytes(Path::new("")), 0);
    let (_temp, _paths, _birth, scope) = fixture();
    let original = request();
    let mut cap = limits();
    cap.max_selected_file_bytes = 0;
    assert_eq!(
        capture_with(&scope, criteria(None, &[], &[".md"]), cap, &original)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        add(usize::MAX, 1, usize::MAX).unwrap_err().kind(),
        io::ErrorKind::FileTooLarge
    );
}
#[cfg(unix)]
#[test]
fn selected_replacement_after_capture_never_returns_new_payload() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.md"), b"abcd").unwrap();
    let original = request();
    let reader = capture_with(
        &scope,
        criteria(Some(Path::new(".")), &[], &[".md"]),
        limits(),
        &original,
    )
    .unwrap();

    std::fs::rename(paths.root().join("a.md"), paths.root().join("old.bin")).unwrap();
    std::fs::write(paths.root().join("a.md"), b"WXYZ").unwrap();
    assert_eq!(
        reader.open_file(Path::new("a.md")).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );

    assert_eq!(reader.retained(Path::new("a.md")).unwrap(), b"abcd");
}
#[cfg(windows)]
#[test]
fn unpaired_native_basename_with_valid_extension_is_rejected_without_loss() {
    use std::os::windows::ffi::OsStringExt;
    let (_temp, paths, _birth, scope) = fixture();
    let name = std::ffi::OsString::from_wide(&[0xd800, 46, 109, 100]);
    std::fs::write(paths.root().join(&name), b"x").unwrap();
    let original = request();
    let error = capture_with(
        &scope,
        criteria(Some(Path::new(".")), &[], &[".md"]),
        limits(),
        &original,
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("Unicode"), "{error}");
    assert_eq!(std::fs::read(paths.root().join(name)).unwrap(), b"x");
}
#[test]
fn expired_original_request_cannot_start_native_selection() {
    use crate::{file_io::SnapshotContext, file_lock::Cancellation};
    use std::time::{Duration, Instant};
    let context = Arc::new(SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    assert_eq!(
        context.check().err().unwrap().kind(),
        io::ErrorKind::TimedOut
    );
}
#[test]
fn depth_limit_denies_before_child_acquisition() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir_all(paths.root().join("docs/deep")).unwrap();
    std::fs::write(paths.root().join("docs/deep/a.md"), b"a").unwrap();
    let original = request();
    let mut caps = limits();
    caps.max_depth = 1;
    assert_eq!(
        capture_with(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".md"]),
            caps,
            &original
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::FileTooLarge
    );
    caps.max_depth = 2;
    assert_eq!(
        documents(
            &capture_with(
                &scope,
                criteria(Some(Path::new("docs")), &[], &[".md"]),
                caps,
                &original
            )
            .unwrap()
        ),
        [PathBuf::from("docs/deep/a.md")]
    );
}

#[test]
fn many_small_documents_and_controls_have_no_count_quota() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("docs")).unwrap();
    std::fs::write(paths.root().join(".wikiignore"), b"#a\n").unwrap();
    let mut expected = Vec::new();
    for index in 0..48 {
        let directory = PathBuf::from(format!("docs/d{index:02}"));
        std::fs::create_dir(paths.root().join(&directory)).unwrap();
        std::fs::write(paths.root().join(&directory).join(".wikiignore"), b"#b\n").unwrap();
        for name in ["a.md", "b.md"] {
            let path = directory.join(name);
            std::fs::write(paths.root().join(&path), b"x").unwrap();
            expected.push(path);
        }
    }
    let original = request();
    let mut caps = limits();
    caps.max_selected_file_bytes = 1;
    caps.max_control_file_bytes = 3;
    let mut input = criteria(Some(Path::new("docs")), &[], &[".md"]);
    input.ignore.custom_control = Some(".wikiignore");
    let reader = capture_with(&scope, input, caps, &original).unwrap();
    assert_eq!(documents(&reader), expected);
    assert_eq!(reader.retained_bytes(), 96);
    reader.revalidate_content().unwrap();
}

#[test]
fn input_and_individual_native_path_boundaries_include_unsupported_entries() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.md"), b"a").unwrap();
    let unit = if cfg!(windows) { 2 } else { 1 };
    for field in 0..4 {
        let original = request();
        let mut caps = limits();
        caps.max_input_items = 5;
        caps.max_input_bytes = 24 + unit;
        caps.max_observed_path_bytes = 8 * unit;
        let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
        input.ignore.protected_defaults = &[".graphit/", "Memory/", "Task/"];
        match field {
            1 => caps.max_input_items -= 1,
            2 => caps.max_input_bytes -= 1,
            3 => caps.max_observed_path_bytes -= 1,
            _ => {}
        }
        let result = capture_with(&scope, input, caps, &original);
        if field == 0 {
            assert_eq!(result.unwrap().retained_bytes(), 1);
        } else {
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::FileTooLarge);
        }
    }
}

#[test]
fn control_exact_empty_and_document_domains_are_separate() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("docs")).unwrap();
    std::fs::write(paths.root().join(".wikiignore"), b"#a\n").unwrap();
    std::fs::write(paths.root().join("docs").join(".wikiignore"), b"#b\n").unwrap();
    let original = request();
    let mut caps = limits();
    caps.max_control_file_bytes = 3;
    let mut input = criteria(Some(Path::new("docs")), &[], &[".md"]);
    input.ignore.custom_control = Some(".wikiignore");
    let reader = capture_with(&scope, input, caps, &original).unwrap();
    assert!(documents(&reader).is_empty());

    drop(reader);
    std::fs::write(paths.root().join("rules.md"), []).unwrap();
    let empty = request();
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.custom_control = Some("rules.md");
    let reader = capture_with(&scope, input, limits(), &empty).unwrap();
    assert_eq!(documents(&reader), [PathBuf::from("rules.md")]);

    drop(reader);
    std::fs::write(paths.root().join("rules.md"), b"#c\n").unwrap();
    let both = request();
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.custom_control = Some("rules.md");
    let reader = capture_with(&scope, input, limits(), &both).unwrap();

    reader.revalidate_content().unwrap();
}

#[test]
fn actual_document_addition_removal_and_optional_presence_invalidate() {
    for change in 0..3 {
        let (_temp, paths, _birth, scope) = fixture();
        std::fs::write(paths.root().join("a.md"), b"old").unwrap();
        let original = request();
        let explicit_names = [Path::new("a.md")];
        let input = if change == 2 {
            criteria(Some(Path::new("missing")), &explicit_names, &[".md"])
        } else {
            criteria(Some(Path::new(".")), &[], &[".md"])
        };
        let reader = capture_with(&scope, input, limits(), &original).unwrap();

        match change {
            0 => std::fs::write(paths.root().join("new.md"), b"new").unwrap(),
            1 => std::fs::remove_file(paths.root().join("a.md")).unwrap(),
            _ => std::fs::create_dir(paths.root().join("missing")).unwrap(),
        }
        if change == 1 {
            assert_eq!(
                reader.open_file(Path::new("a.md")).err().unwrap().kind(),
                io::ErrorKind::NotFound
            );
        } else {
            let file = reader.open_file(Path::new("a.md")).unwrap();
            assert_eq!(file.read_range(0, 3).unwrap().as_ref(), b"old");
        }
        // Membership changes invalidate the whole selection, while a stable
        // FILE remains readable after unrelated names appear beside it.
        assert_eq!(
            reader.revalidate_bindings().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            reader.revalidate_content().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(reader.retained(Path::new("a.md")).unwrap(), b"old");
    }
}

#[cfg(unix)]
struct SelectionHookGuard;
#[cfg(unix)]
impl Drop for SelectionHookGuard {
    fn drop(&mut self) {
        SELECTED_STEP.with(|slot| *slot.borrow_mut() = None);
    }
}
#[cfg(unix)]
fn selection_hook(
    step: SelectionStep,
    at: &Path,
    action: impl FnOnce(&SnapshotContext) + 'static,
) -> SelectionHookGuard {
    SELECTED_STEP.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(SelectionHook {
            step,
            at: at.to_owned(),
            action: Box::new(action),
        });
    });
    SelectionHookGuard
}
#[cfg(unix)]
fn multi_chunk_limits(
    bytes: usize,
) -> (
    ProjectSelectionLimits,
    crate::artifact_store::SourceReadLimits,
) {
    let mut caps = limits();
    caps.max_selected_file_bytes = bytes as u64;
    let reads = crate::artifact_store::SourceReadLimits::new(bytes, 4096, 1024 * 1024).unwrap();
    (caps, reads)
}

#[cfg(unix)]
#[test]
fn selected_body_growth_after_actual_chunk_does_not_read_growth_or_return_partial() {
    let (_temp, paths, _birth, scope) = fixture();
    let file = paths.root().join("a.md");
    let size = tree::BUFFER_BYTES * 2;
    std::fs::write(&file, vec![b'A'; size]).unwrap();
    let original = request();
    let same = original.clone();
    let observed = Arc::new(Mutex::new(None));
    let step = Arc::clone(&observed);
    let _guard = selection_hook(
        SelectionStep::ReadChunk,
        Path::new("a.md"),
        move |current| {
            assert!(current.same_request(&same));

            *step.lock().unwrap() = Some(());
            // Real append after the first consumed chunk exceeds both admitted caps.
            std::fs::OpenOptions::new()
                .write(true)
                .open(&file)
                .unwrap()
                .set_len((size + tree::BUFFER_BYTES) as u64)
                .unwrap();
        },
    );
    let (caps, reads) = multi_chunk_limits(size);
    let outcome = CapturedSourceReader::capture_selected_project(
        &scope,
        criteria(Some(Path::new(".")), &[], &[".md"]),
        caps,
        reads,
        &original,
    );
    assert_eq!(outcome.err().unwrap().kind(), io::ErrorKind::InvalidData);

    assert_eq!(
        std::fs::metadata(paths.root().join("a.md")).unwrap().len(),
        (size + tree::BUFFER_BYTES) as u64
    );
}

#[cfg(unix)]
#[test]
fn root_replacement_after_selected_chunk_denies_b_and_preserves_spent_request() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, paths, _birth, scope) = fixture();
    let root = paths.root().to_owned();
    let saved = temp.path().join("saved-original");
    let size = tree::BUFFER_BYTES * 2;
    std::fs::write(root.join("a.md"), vec![b'A'; size]).unwrap();
    let original = request();
    let same = original.clone();
    let observed = Arc::new(Mutex::new(None));
    let step = Arc::clone(&observed);
    let root_to_swap = root.clone();
    let saved_to_swap = saved.clone();
    let _guard = selection_hook(
        SelectionStep::ReadChunk,
        Path::new("a.md"),
        move |current| {
            assert!(current.same_request(&same));

            *step.lock().unwrap() = Some(());
            std::fs::rename(&root_to_swap, &saved_to_swap).unwrap();
            std::fs::create_dir_all(root_to_swap.join(".graphit")).unwrap();
            let b = root_to_swap.join("a.md");
            std::fs::write(&b, vec![b'B'; size * 2]).unwrap();
            std::fs::set_permissions(&b, std::fs::Permissions::from_mode(0o0)).unwrap();
        },
    );
    let (caps, reads) = multi_chunk_limits(size);
    assert!(
        CapturedSourceReader::capture_selected_project(
            &scope,
            criteria(Some(Path::new(".")), &[], &[".md"]),
            caps,
            reads,
            &original,
        )
        .is_err()
    );

    // Restore A physically; a retry is the same already-spent request, never B.
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::rename(&saved, &root).unwrap();
    let (caps, reads) = multi_chunk_limits(size);
    let restored = CapturedSourceReader::capture_selected_project(
        &scope,
        criteria(Some(Path::new(".")), &[], &[".md"]),
        caps,
        reads,
        &original,
    )
    .unwrap();
    assert!(
        restored
            .retained(Path::new("a.md"))
            .unwrap()
            .iter()
            .all(|byte| *byte == b'A')
    );

    drop(restored);
}

#[cfg(unix)]
#[test]
fn existing_control_changed_after_inventory_is_checked_before_document_delivery() {
    let (_temp, paths, _birth, scope) = fixture();
    let control = paths.root().join(".wikiignore");
    std::fs::write(&control, b"#a\n").unwrap();
    std::fs::write(paths.root().join("a.md"), b"old").unwrap();
    let original = request();
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.custom_control = Some(".wikiignore");
    let reader = capture_with(&scope, input, limits(), &original).unwrap();

    let observed = Arc::new(Mutex::new(None));
    let step = Arc::clone(&observed);
    let _guard = selection_hook(
        SelectionStep::ControlBinding,
        Path::new(".wikiignore"),
        move |_| {
            // Same name, inode and length; this is existing content, not added control.
            *step.lock().unwrap() = Some(());
            std::fs::write(&control, b"#b\n").unwrap();
        },
    );
    assert_eq!(
        reader.open_file(Path::new("a.md")).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );

    assert_eq!(reader.retained(Path::new("a.md")).unwrap(), b"old");
    drop(reader);
}

#[cfg(unix)]
#[test]
fn ancestor_replacement_after_root_inventory_denies_fresh_directory_binding() {
    let (temp, paths, _birth, scope) = fixture();
    let docs = paths.root().join("docs");
    std::fs::create_dir(&docs).unwrap();
    std::fs::write(docs.join("a.md"), b"old").unwrap();
    let original = request();
    let reader = capture_with(
        &scope,
        criteria(Some(Path::new("docs")), &[], &[".md"]),
        limits(),
        &original,
    )
    .unwrap();

    let saved = temp.path().join("saved-docs");
    let observed = Arc::new(Mutex::new(None));
    let step = Arc::clone(&observed);
    let _guard = selection_hook(
        SelectionStep::DirectoryBinding,
        Path::new("docs"),
        move |_| {
            // Root inventory already passed; the next native open must detect new docs.
            *step.lock().unwrap() = Some(());
            std::fs::rename(&docs, &saved).unwrap();
            std::fs::create_dir(&docs).unwrap();
            std::fs::write(docs.join("a.md"), b"replacement-B").unwrap();
        },
    );
    assert_eq!(
        reader
            .open_file(Path::new("docs/a.md"))
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );

    assert_eq!(reader.retained(Path::new("docs/a.md")).unwrap(), b"old");
    drop(reader);
}

#[test]
fn selected_snapshot_binding_and_payload_build_keep_selection_guards() {
    use crate::artifact_store::tree::{BuildTree, TreeReceipt};
    use crate::artifact_store::{CopyLimits, PayloadPolicy, ValidatedPayloadPlan};
    let (temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("docs")).unwrap();
    std::fs::write(paths.root().join("docs/a.md"), b"selected").unwrap();
    std::fs::write(paths.root().join("docs/ignored.bin"), b"unselected").unwrap();
    std::fs::write(paths.root().join("docs/.wikiignore"), b"*.bin\n").unwrap();
    let original = request();
    let mut input = criteria(Some(Path::new("docs")), &[], &[".md"]);
    input.ignore.custom_control = Some(".wikiignore");
    let snapshot = capture(&scope, input, limits(), &original).unwrap();
    assert_eq!(snapshot.link_policy(), CopyPolicy::RejectFileLinks);
    let binding = snapshot.binding().unwrap();
    assert!(binding.matches_snapshot(&snapshot));
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let policy = PayloadPolicy::new(
        NodeKind::Directory,
        root_mode,
        CopyLimits::new(65536, 16, 4096, 16).unwrap(),
    );
    let payload = ValidatedPayloadPlan::new(snapshot, [], &policy, &original).unwrap();
    // Unselected content changes preserve this selected source; new inventory entries do not.
    std::fs::write(
        paths.root().join("docs/ignored.bin"),
        b"changed outside selection",
    )
    .unwrap();
    payload.revalidate_content(&original).unwrap();
    let mut receipt = TreeReceipt { nodes: Vec::new() };
    let output = temp.path().join("selected-output");
    payload
        .build_tree(
            &output,
            original.cancellation(),
            Some(&original),
            &mut receipt,
            &mut || Ok(()),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(output.join("docs/a.md")).unwrap(),
        b"selected"
    );
    assert!(!output.join("docs/ignored.bin").exists());
    assert!(!output.join("docs/.wikiignore").exists());
    std::fs::write(paths.root().join("docs/.wikiignore"), b"*.md\n").unwrap();
    let rejected = temp.path().join("rejected-output");
    let mut receipt = TreeReceipt { nodes: Vec::new() };
    assert!(
        payload
            .build_tree(
                &rejected,
                original.cancellation(),
                Some(&original),
                &mut receipt,
                &mut || Ok(())
            )
            .is_err()
    );
    assert!(!rejected.exists());
}

#[test]
fn profile_basename_routes_and_metadata_only_reads_keep_native_authority() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::create_dir(paths.root().join("nested")).unwrap();
    for name in ["Dockerfile", "MAKEFILE", "Build.Config", "other", "file.RS"] {
        std::fs::write(paths.root().join("nested").join(name), name.as_bytes()).unwrap();
    }
    let original = request();
    let reader = CapturedSourceReader::capture_selected_project_metadata(
        &scope,
        criteria(
            Some(Path::new(".")),
            &[],
            &["dockerfile", "Makefile", "build.config", ".rs"],
        ),
        limits(),
        read_limits(),
        &original,
    )
    .unwrap();
    assert_eq!(reader.retained_bytes(), 0);
    assert_eq!(
        documents(&reader),
        [
            "nested/Build.Config",
            "nested/Dockerfile",
            "nested/MAKEFILE",
            "nested/file.RS"
        ]
        .map(PathBuf::from)
    );
    for node in reader.nodes().filter(|n| n.kind() == NodeKind::File) {
        assert!(reader.retained(node.relative()).is_none());
        let file = reader.open_file(node.relative()).unwrap();
        let body = file.read_range(0, node.bytes() as usize).unwrap();
        reader
            .validate_file_bytes(node.relative(), body.as_ref(), &original)
            .unwrap();
        let mut wrong = body.as_ref().to_vec();
        wrong[0] ^= 1;
        assert_eq!(
            reader
                .validate_file_bytes(node.relative(), &wrong, &original)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            reader
                .validate_file_bytes(node.relative(), &[], &original)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            reader
                .validate_file_bytes(node.relative(), body.as_ref(), &request())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(
        reader
            .validate_file_bytes(Path::new("nested"), &[], &original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        reader
            .validate_file_bytes(Path::new("not-selected"), &[], &original)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    reader.revalidate_content().unwrap();
    std::fs::write(paths.root().join("nested/Dockerfile"), b"changed").unwrap();
    assert!(reader.revalidate_content().is_err());
}

#[cfg(unix)]
#[test]
fn wide_metadata_capture_revalidates_below_the_kernel_file_limit() {
    const CHILD: &str = "GRAPHIT_TEST_SELECTED_SOURCE_LOW_NOFILE";
    if std::env::var_os(CHILD).is_none() {
        let name = concat!(
            module_path!(),
            "::wide_metadata_capture_revalidates_below_the_kernel_file_limit"
        );
        let (_, selector) = name.split_once("::").unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", selector, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "isolated descriptor fixture failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use rustix::process::{Resource, getrlimit, setrlimit};
    let mut limit = getrlimit(Resource::Nofile);
    limit.current = Some(1024.min(limit.current.unwrap_or(1024)));
    setrlimit(Resource::Nofile, limit).unwrap();
    let (_temp, paths, _birth, scope) = fixture();
    for index in 0..1100 {
        let directory = paths.root().join(format!("dir-{index:04}"));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join(".wikiignore"), b"*.skip\n").unwrap();
        if index == 1099 {
            std::fs::write(directory.join("note.md"), b"selected\n").unwrap();
        }
    }
    let original = request();
    let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
    input.ignore.custom_control = Some(".wikiignore");
    let reader = CapturedSourceReader::capture_selected_project_metadata(
        &scope,
        input,
        limits(),
        read_limits(),
        &original,
    )
    .unwrap();
    assert_eq!(documents(&reader).len(), 1);
    assert_eq!(reader.retained_bytes(), 0);
    reader.revalidate_content().unwrap();
    let file = reader.open_file(Path::new("dir-1099/note.md")).unwrap();
    drop(reader);
    assert_eq!(file.read_range(0, 9).unwrap().as_ref(), b"selected\n");
    drop(file);
}

#[cfg(windows)]
#[test]
fn selected_file_keeps_no_delete_ancestors_only_while_io_is_owned() {
    let (_temp, paths, _birth, scope) = fixture();
    let docs = paths.root().join("docs");
    let saved = paths.root().join("saved");
    std::fs::create_dir(&docs).unwrap();
    std::fs::write(docs.join("a.md"), b"selected").unwrap();
    let original = request();
    let reader = CapturedSourceReader::capture_selected_project_metadata(
        &scope,
        criteria(Some(Path::new("docs")), &[], &[".md"]),
        limits(),
        read_limits(),
        &original,
    )
    .unwrap();
    let file = reader.open_file(Path::new("docs/a.md")).unwrap();
    assert!(
        std::fs::rename(&docs, &saved).is_err(),
        "active IO must retain no-delete ancestors"
    );
    assert_eq!(file.read_range(0, 8).unwrap().as_ref(), b"selected");
    drop(file);
    std::fs::rename(&docs, &saved).unwrap();
    assert!(
        reader.revalidate_content().is_err(),
        "an inactive selection keeps receipts, not a permanent directory pin"
    );
}

#[cfg(windows)]
#[test]
fn metadata_selection_detects_replaced_directory_between_reads() {
    let (_temp, paths, _birth, scope) = fixture();
    let docs = paths.root().join("docs");
    let saved = paths.root().join("saved");
    std::fs::create_dir(&docs).unwrap();
    std::fs::write(docs.join("a.md"), b"selected").unwrap();
    let original = request();
    let reader = CapturedSourceReader::capture_selected_project_metadata(
        &scope,
        criteria(Some(Path::new("docs")), &[], &[".md"]),
        limits(),
        read_limits(),
        &original,
    )
    .unwrap();
    std::fs::rename(&docs, &saved).unwrap();
    std::fs::create_dir(&docs).unwrap();
    std::fs::write(docs.join("a.md"), b"selected").unwrap();
    assert!(
        reader.open_file(Path::new("docs/a.md")).is_err(),
        "matching names and bytes cannot replace captured physical identity"
    );
}

#[test]
fn repository_generated_parsers_are_not_selected_and_authored_file_limit_stays_strict() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(
        paths.root().join(".astignore"),
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.astignore")),
    )
    .unwrap();
    std::fs::write(
        paths.root().join(".gitignore"),
        b"/target/\n/.build/\n!vendor/fixture.rs\n",
    )
    .unwrap();
    let maximum = 8 * 1024 * 1024;
    for name in [
        "crates/graphit-ast/grammars/tree_sitter/scala/src/parser.c",
        "crates/graphit-ast/src/antlr_engine/cpp/generated/plsql/PlSqlParser.cpp",
        "target/build/output.rs",
        ".build/output.rs",
    ] {
        let path = paths.root().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::File::create(path)
            .unwrap()
            .set_len(maximum + 1)
            .unwrap();
    }
    for name in [
        "crates/graphit-app/assets/ui/index.html",
        "crates/graphit-app/assets/ui/logo.svg",
        "crates/graphit-app/assets/ui/favicon.svg",
    ] {
        let path = paths.root().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"generated bundle\n").unwrap();
    }
    let authored = [
        "ui/src/components/system/WorkspaceNow.tsx",
        "ui/src/api/now.ts",
        "ui/index.html",
        "ui/public/logo.svg",
        "crates/graphit-ast/src/antlr_engine/cpp/bridge.cpp",
        "crates/graphit-ast/src/antlr_engine/cpp/actions/PlSqlParserBase.cpp",
        "crates/graphit-ast/grammars/antlr/PlSqlParser.g4",
        "crates/graphit-ast/grammars/tree_sitter/scala/src/scanner.c",
        "src/lib.rs",
    ];
    for name in authored {
        let path = paths.root().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"source\n").unwrap();
    }
    let original = request();
    let mut input = criteria(
        Some(Path::new(".")),
        &[],
        &[".c", ".cpp", ".rs", ".g4", ".ts", ".tsx", ".html", ".svg"],
    );
    input.ignore.custom_control = Some(".astignore");
    input.ignore.git_control = Some(".gitignore");
    let mut caps = limits();
    caps.max_selected_file_bytes = maximum;
    let reads = SourceReadLimits::new(maximum as usize, 64 * 1024, maximum).unwrap();
    let reader = CapturedSourceReader::capture_selected_project_metadata(
        &scope, input, caps, reads, &original,
    )
    .unwrap();
    assert_eq!(reader.retained_bytes(), 0);
    let actual: std::collections::BTreeSet<_> = documents(&reader).into_iter().collect();
    assert_eq!(actual, authored.into_iter().map(PathBuf::from).collect());
    reader.revalidate_content().unwrap();
    drop(reader);

    std::fs::File::create(paths.root().join("src/oversized.rs"))
        .unwrap()
        .set_len(maximum + 1)
        .unwrap();
    let mut input = criteria(
        Some(Path::new(".")),
        &[],
        &[".c", ".cpp", ".rs", ".g4", ".ts", ".tsx", ".html", ".svg"],
    );
    input.ignore.custom_control = Some(".astignore");
    input.ignore.git_control = Some(".gitignore");
    assert_eq!(
        CapturedSourceReader::capture_selected_project_metadata(
            &scope, input, caps, reads, &original,
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::FileTooLarge
    );
}

#[test]
fn indexed_selection_preserves_native_order_dedup_and_component_ancestors() {
    let (_temp, paths, _birth, scope) = fixture();
    for directory in ["a", "a-else", "parent/nested", "parent-empty"] {
        std::fs::create_dir_all(paths.root().join(directory)).unwrap();
    }
    for name in ["z.rs", "a-else/code.rs", "parent/nested/code.rs"] {
        std::fs::write(paths.root().join(name), name.as_bytes()).unwrap();
    }
    std::fs::write(paths.root().join("a/unselected.txt"), b"other").unwrap();
    let original = request();
    let reader = CapturedSourceReader::capture_selected_project_metadata(
        &scope,
        criteria(
            Some(Path::new(".")),
            &[
                Path::new("z.rs"),
                Path::new("parent/nested/code.rs"),
                Path::new("a-else/code.rs"),
                Path::new("z.rs"),
            ],
            &[".rs"],
        ),
        limits(),
        read_limits(),
        &original,
    )
    .unwrap();
    let expected: Vec<_> = [
        "",
        "a-else",
        "a-else/code.rs",
        "parent",
        "parent/nested",
        "parent/nested/code.rs",
        "z.rs",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    assert_eq!(
        reader
            .nodes()
            .map(|node| node.relative().to_owned())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(reader.retained_bytes(), 0);
    reader.revalidate_content().unwrap();
    assert_eq!(
        reader
            .open_file(Path::new("parent/nested/code.rs"))
            .unwrap()
            .read_range(0, 6)
            .unwrap()
            .as_ref(),
        b"parent"
    );
}

#[test]
fn local_file_read_defers_other_selected_changes_to_global_validation_and_publication() {
    use crate::artifact_store::tree::{BuildTree, TreeReceipt};
    use crate::artifact_store::{CopyLimits, PayloadPolicy, ValidatedPayloadPlan};
    for change in [
        "body",
        "control",
        "root-create",
        "root-remove",
        "parent-create",
        "parent-remove",
        "root-create-dir",
        "root-remove-dir",
        "parent-create-dir",
        "parent-remove-dir",
    ] {
        let (temp, paths, _birth, scope) = fixture();
        std::fs::create_dir(paths.root().join("a")).unwrap();
        std::fs::create_dir(paths.root().join("b")).unwrap();
        std::fs::write(paths.root().join("a/file.md"), b"alpha").unwrap();
        std::fs::write(paths.root().join("b/file.md"), b"beta").unwrap();
        std::fs::write(paths.root().join("b/.wikiignore"), b"#b\n").unwrap();
        let sibling = if change.starts_with("parent-") {
            paths.root().join("a/unrelated.md")
        } else {
            paths.root().join("unrelated.md")
        };
        if change.contains("-remove") {
            if change.ends_with("-dir") {
                std::fs::create_dir(&sibling).unwrap();
            } else {
                std::fs::write(&sibling, b"unrelated namespace").unwrap();
            }
        }
        let original = request();
        let input = || {
            let mut input = criteria(Some(Path::new(".")), &[], &[".md"]);
            input.ignore.custom_control = Some(".wikiignore");
            input
        };
        let reader = capture_with(&scope, input(), limits(), &original).unwrap();
        let metadata = CapturedSourceReader::capture_selected_project_metadata(
            &scope,
            input(),
            limits(),
            read_limits(),
            &original,
        )
        .unwrap();
        let snapshot = capture(&scope, input(), limits(), &original).unwrap();
        let root_mode = snapshot.nodes().next().unwrap().mode();
        let policy = PayloadPolicy::new(
            NodeKind::Directory,
            root_mode,
            CopyLimits::new(65536, 16, 4096, 16).unwrap(),
        );
        let payload = ValidatedPayloadPlan::new(snapshot, [], &policy, &original).unwrap();
        let a = reader.open_file(Path::new("a/file.md")).unwrap();
        let b = reader.open_file(Path::new("b/file.md")).unwrap();
        let mut stream = a.stream(0).unwrap();
        let mut bytes = [0; 2];
        assert_eq!(stream.read(&mut bytes).unwrap(), 2);
        assert_eq!(&bytes, b"al");

        let leaf = paths.root().join("a/file.md");
        let before_file =
            tree::Observation::from_file(&std::fs::File::open(&leaf).unwrap()).unwrap();
        let parent = sibling.parent().unwrap();
        let directory = NativeHandle::open_root(parent, &original).unwrap();
        let before_directory = directory.observe(&original).unwrap();
        match change {
            "body" => std::fs::write(paths.root().join("b/file.md"), b"changed sibling\n").unwrap(),
            "control" => {
                std::fs::write(paths.root().join("b/.wikiignore"), b"changed sibling\n").unwrap()
            }
            _ if change.contains("-create") => {
                if change.ends_with("-dir") {
                    std::fs::create_dir(&sibling).unwrap();
                } else {
                    std::fs::write(&sibling, b"unrelated namespace").unwrap();
                }
            }
            _ if change.ends_with("-dir") => std::fs::remove_dir(&sibling).unwrap(),
            _ => std::fs::remove_file(&sibling).unwrap(),
        }
        assert!(
            tree::Observation::from_file(&std::fs::File::open(&leaf).unwrap()).unwrap()
                == before_file
        );
        if change.contains('-') {
            assert!(
                directory.observe(&original).unwrap() != before_directory,
                "namespace mutation must change the ancestor observation"
            );
        }
        assert_eq!(stream.read(&mut bytes).unwrap(), 2);
        assert_eq!(&bytes, b"ph");
        let mut tail = Vec::new();
        stream.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, b"a");
        assert_eq!(stream.read(&mut []).unwrap(), 0);
        assert_eq!(stream.read(&mut bytes).unwrap(), 0);
        assert_eq!(a.read_range(0, 5).unwrap().as_ref(), b"alpha");
        assert_eq!(a.read_range(5, 0).unwrap().as_ref(), b"");
        assert_eq!(reader.open_file(Path::new("a/file.md")).unwrap().len(), 5);
        let fresh = metadata.open_file(Path::new("a/file.md")).unwrap();
        assert_eq!(fresh.read_range(0, 5).unwrap().as_ref(), b"alpha");
        assert_eq!(fresh.read_range(5, 0).unwrap().as_ref(), b"");
        assert!(metadata.revalidate_bindings().is_err());

        // A batch rejects a changed member, but stable FILES survive unrelated namespace changes.
        let plan = [
            crate::artifact_store::SourceRangeRequest::new(&a, 0, 2).unwrap(),
            crate::artifact_store::SourceRangeRequest::new(&b, 0, 2).unwrap(),
        ];
        let batch = reader.read_ranges(
            &plan,
            crate::artifact_store::AggregateReadLimits::new(2, 4).unwrap(),
        );
        if change == "body" || change == "control" {
            assert!(batch.is_err());
        } else {
            let batch = batch.unwrap();
            assert_eq!(batch[0].as_ref(), b"al");
            assert_eq!(batch[1].as_ref(), b"be");
        }
        assert!(reader.revalidate_bindings().is_err());
        assert!(reader.revalidate_content().is_err());

        // The real publication path checks the whole selected backing before effects.
        let output = temp.path().join("rejected-sibling-publication");
        let mut receipt = TreeReceipt { nodes: Vec::new() };
        assert!(
            payload
                .build_tree(
                    &output,
                    original.cancellation(),
                    Some(&original),
                    &mut receipt,
                    &mut || Ok(()),
                )
                .is_err()
        );
        assert!(!output.exists());
    }
}

#[cfg(unix)]
#[test]
fn local_file_read_rejects_replaced_symlinked_and_permission_changed_ancestors() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for change in ["replacement", "symlink", "permissions"] {
        let (temp, paths, _birth, scope) = fixture();
        let docs = paths.root().join("docs");
        std::fs::create_dir_all(docs.join("nested")).unwrap();
        let relative = Path::new("docs/nested/a.md");
        let leaf = paths.root().join(relative);
        std::fs::write(&leaf, b"unchanged FILE").unwrap();
        let original = request();
        let reader = CapturedSourceReader::capture_selected_project_metadata(
            &scope,
            criteria(Some(Path::new("docs")), &[], &[".md"]),
            limits(),
            read_limits(),
            &original,
        )
        .unwrap();
        let file = reader.open_file(relative).unwrap();
        let mut stream = file.stream(0).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"unchanged FILE");
        let before_file =
            tree::Observation::from_file(&std::fs::File::open(&leaf).unwrap()).unwrap();
        let saved = temp.path().join("saved-docs");
        match change {
            "replacement" => {
                std::fs::rename(&docs, &saved).unwrap();
                std::fs::create_dir(&docs).unwrap();
                // Keep the actual leaf and its immediate parent; only its ancestor changes.
                std::fs::rename(saved.join("nested"), docs.join("nested")).unwrap();
            }
            "symlink" => {
                std::fs::rename(&docs, &saved).unwrap();
                symlink(&saved, &docs).unwrap();
            }
            _ => {
                let mode = std::fs::metadata(&docs).unwrap().permissions().mode();
                std::fs::set_permissions(&docs, std::fs::Permissions::from_mode(mode ^ 0o020))
                    .unwrap();
            }
        }
        assert!(
            tree::Observation::from_file(&std::fs::File::open(&leaf).unwrap()).unwrap()
                == before_file
        );
        assert!(reader.open_file(relative).is_err(), "{change}");
        assert!(file.read_range(0, 2).is_err(), "{change}");
        assert!(stream.read(&mut []).is_err(), "{change}");
        assert!(stream.read(&mut [0; 2]).is_err(), "{change}");
        assert!(reader.revalidate_bindings().is_err(), "{change}");
    }
}

#[test]
fn local_stream_rechecks_present_and_absent_controls_of_every_applicable_scope() {
    for directory in ["", "outer", "outer/docs"] {
        for (git_name, changed_name) in [
            (".wikiignore", ".wikiignore"),
            (".gitignore", ".wikiignore"),
            (".gitignore", ".gitignore"),
        ] {
            for change in 0..5 {
                let (temp, paths, _birth, scope) = fixture();
                std::fs::create_dir_all(paths.root().join("outer/docs")).unwrap();
                std::fs::write(paths.root().join("outer/docs/a.md"), b"abcdef").unwrap();
                let control = paths.root().join(directory).join(changed_name);
                for name in [git_name, ".wikiignore"] {
                    if name != changed_name {
                        std::fs::write(paths.root().join(directory).join(name), b"#other\n")
                            .unwrap();
                    }
                }
                if change != 0 {
                    std::fs::write(&control, b"#a\n").unwrap();
                }
                let original = request();
                let mut input = criteria(Some(Path::new("outer/docs")), &[], &[".md"]);
                // Equal names identify one physical control; distinct names use both receipts.
                input.ignore.custom_control = Some(".wikiignore");
                input.ignore.git_control = Some(git_name);
                let reader = capture_with(&scope, input, limits(), &original).unwrap();
                let file = reader.open_file(Path::new("outer/docs/a.md")).unwrap();
                let mut stream = file.stream(0).unwrap();
                let mut bytes = [0; 2];
                assert_eq!(stream.read(&mut bytes).unwrap(), 2);
                assert_eq!(&bytes, b"ab");
                match change {
                    0 => std::fs::write(&control, b"*.md\n").unwrap(),
                    1 => std::fs::write(&control, b"#changed existing control\n").unwrap(),
                    2 => std::fs::remove_file(&control).unwrap(),
                    3 => {
                        std::fs::rename(&control, temp.path().join("saved-control")).unwrap();
                        std::fs::write(&control, b"#a\n").unwrap();
                    }
                    _ => {
                        std::fs::remove_file(&control).unwrap();
                        std::fs::create_dir(&control).unwrap();
                    }
                }
                assert!(
                    stream.read(&mut bytes).is_err(),
                    "{directory}, change {change}"
                );
                assert!(stream.read(&mut []).is_err());
                assert!(file.read_range(0, 2).is_err());
                assert!(reader.revalidate_bindings().is_err());
            }
        }
    }
}

#[test]
fn local_eof_and_empty_reads_keep_leaf_and_original_checks() {
    for cancel in [false, true] {
        let (_temp, paths, _birth, scope) = fixture();
        let path = paths.root().join("a.md");
        std::fs::write(&path, b"abc").unwrap();
        let original = request();
        let reader = capture_with(
            &scope,
            criteria(Some(Path::new(".")), &[], &[".md"]),
            limits(),
            &original,
        )
        .unwrap();
        let file = reader.open_file(Path::new("a.md")).unwrap();
        let mut stream = file.stream(0).unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"abc");
        if cancel {
            original.cancellation().cancel();
        } else {
            std::fs::write(&path, b"changed leaf").unwrap();
        }
        let expected = if cancel {
            io::ErrorKind::Interrupted
        } else {
            io::ErrorKind::InvalidData
        };
        assert_eq!(stream.read(&mut []).err().unwrap().kind(), expected);
        assert_eq!(stream.read(&mut [0; 2]).err().unwrap().kind(), expected);
        assert_eq!(file.read_range(3, 0).err().unwrap().kind(), expected);
    }
}

#[test]
fn ignored_build_entries_do_not_invalidate_selected_file_or_eof() {
    for mutation in [
        "create-ignored",
        "remove-ignored",
        "create-selected",
        "remove-selected",
        "change-selected",
        "change-control",
    ] {
        let (_temp, paths, _birth, scope) = fixture();
        std::fs::create_dir(paths.root().join("ui")).unwrap();
        std::fs::write(paths.root().join("ui/main.rs"), b"fn original() {}\n").unwrap();
        std::fs::write(
            paths.root().join(".gitignore"),
            b"ui/vite.config.ts.timestamp-*.mjs\n",
        )
        .unwrap();
        let ignored = paths.root().join("ui/vite.config.ts.timestamp-1.mjs");
        let selected = paths.root().join("ui/extra.rs");
        if mutation == "remove-ignored" {
            std::fs::write(&ignored, b"compiled config").unwrap();
        }
        if mutation == "remove-selected" {
            std::fs::write(&selected, b"fn extra() {}\n").unwrap();
        }
        let original = request();
        let mut input = criteria(Some(Path::new("ui")), &[], &[".rs", ".mjs"]);
        input.ignore.git_control = Some(".gitignore");
        let reader = CapturedSourceReader::capture_selected_project_metadata(
            &scope,
            input,
            limits(),
            read_limits(),
            &original,
        )
        .unwrap();
        match mutation {
            "create-ignored" => std::fs::write(&ignored, b"compiled config").unwrap(),
            "remove-ignored" => std::fs::remove_file(&ignored).unwrap(),
            "create-selected" => std::fs::write(&selected, b"fn extra() {}\n").unwrap(),
            "remove-selected" => std::fs::remove_file(&selected).unwrap(),
            "change-selected" => {
                std::fs::write(paths.root().join("ui/main.rs"), b"fn changed() {}\n").unwrap()
            }
            "change-control" => {
                std::fs::write(paths.root().join(".gitignore"), b"ui/extra.rs\n").unwrap()
            }
            _ => unreachable!(),
        }
        if mutation.ends_with("ignored") {
            let file = reader.open_file(Path::new("ui/main.rs")).unwrap();
            let bytes = file.read_range(0, file.len() as usize).unwrap();
            reader
                .validate_file_bytes(Path::new("ui/main.rs"), bytes.as_ref(), &original)
                .unwrap();
            assert_eq!(bytes.as_ref(), b"fn original() {}\n");
            reader.revalidate_bindings().unwrap();
            reader.revalidate_content().unwrap();
        } else {
            assert!(reader.revalidate_bindings().is_err(), "{mutation}");
            assert!(reader.revalidate_content().is_err(), "{mutation}");
        }
    }
}

#[cfg(unix)]
#[test]
fn selected_paths_discovery_does_not_hash_payloads() {
    let (_temp, paths, _birth, scope) = fixture();
    std::fs::write(paths.root().join("a.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::write(paths.root().join("z.rs"), "pub fn zeta() {}\n").unwrap();
    let original = request();
    let _guard = selection_hook(SelectionStep::ReadChunk, Path::new("a.rs"), |_| {
        panic!("path discovery must not consume a FILE payload");
    });
    let selected = selected_project_paths(
        &scope,
        criteria(Some(Path::new(".")), &[], &[".rs"]),
        limits(),
        &original,
    )
    .unwrap();
    assert_eq!(selected, [PathBuf::from("a.rs"), PathBuf::from("z.rs")]);
}
