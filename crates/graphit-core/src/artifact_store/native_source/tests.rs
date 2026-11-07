use super::*;
use crate::{
    artifact_store::{
        CapturedSourceReader, CopyLimits, RetainedBytesLimits, RetainedFileSet, ScopeLimits,
        SourceReadLimits,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
mod published_copy;

pub(super) fn request() -> std::sync::Arc<crate::file_io::SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}
pub(super) fn fixture() -> (
    tempfile::TempDir,
    ProjectPaths,
    std::sync::Arc<crate::file_io::SnapshotContext>,
    CapturedPublicationScope,
) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project");
    std::fs::create_dir_all(root.join(".graphit")).unwrap();
    std::fs::create_dir(root.join("Memory")).unwrap();
    std::fs::create_dir(root.join("Task")).unwrap();
    std::fs::write(root.join("Memory/data"), b"memory-data").unwrap();
    std::fs::write(root.join("Task/unselected"), b"UNSELECTED").unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        temporary.path(),
        None,
        Some(&temporary.path().join("global")),
    )
    .unwrap();
    let paths = ProjectPaths::at(&root, &installation).unwrap();
    let birth = request();
    let scope = CapturedPublicationScope::project(
        &paths,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        ScopeLimits::new(4, 4096, 256).unwrap(),
        &birth,
    )
    .unwrap();
    (temporary, paths, birth, scope)
}
fn policy(links: CopyPolicy) -> SnapshotPolicy {
    let retention = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
    SnapshotPolicy::new(
        links,
        CopyLimits::new(1024 * 1024, 128, 65536, 8).unwrap(),
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap()
}
pub(super) fn read_limits() -> SourceReadLimits {
    SourceReadLimits::new(65536, 4096, 1024 * 1024).unwrap()
}
fn capture(
    scope: &CapturedPublicationScope,
    path: &str,
    kind: NodeKind,
    request: &std::sync::Arc<crate::file_io::SnapshotContext>,
) -> io::Result<CapturedSourceReader> {
    CapturedSourceReader::capture_project(
        scope,
        Path::new(path),
        kind,
        &policy(CopyPolicy::RejectFileLinks),
        read_limits(),
        request,
    )
}
#[test]
fn actual_reader_ranges_and_metadata_use_native_selected_pipeline() {
    let (temporary, paths, birth, scope) = fixture();
    let current = request();
    let reader = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    assert_eq!(
        reader
            .nodes()
            .map(|n| n.relative().to_owned())
            .collect::<Vec<_>>(),
        [PathBuf::new(), PathBuf::from("data")]
    );
    // Only selected bytes were hashed. The Task sibling was never enumerated or
    // hashed by capture_project; its payload does not enter IO or manifest.

    let file = reader.open_file(Path::new("data")).unwrap();
    file.last_modified().unwrap();
    let bytes = file.read_range(1, 4).unwrap();
    assert_eq!(bytes.as_ref(), b"emor");

    drop(bytes);
    drop(file);
    drop(reader);
    drop(scope);
    drop(paths);
    drop(birth);
    drop(current);
    drop(temporary);
}
#[test]
fn source_file_edit_is_allowed_but_invalidates_retained_observations() {
    let (_temporary, paths, _birth, scope) = fixture();
    let current = request();
    let reader = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    let file = reader.open_file(Path::new("data")).unwrap();
    let prior = file.read_range(0, 6).unwrap();
    let path = paths.root().join("Memory").join("data");
    std::fs::write(path, b"new complete source").unwrap();
    assert_eq!(prior.as_ref(), b"memory");
    assert_eq!(
        file.last_modified().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        file.read_range(0, 6).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    let fresh = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    assert_eq!(
        fresh
            .open_file(Path::new("data"))
            .unwrap()
            .read_range(0, 19)
            .unwrap()
            .as_ref(),
        b"new complete source"
    );
}
#[test]
fn idle_source_file_replacement_and_removal_invalidate_old_readers() {
    for kind in [NodeKind::Directory, NodeKind::File] {
        let (_temporary, paths, _birth, scope) = fixture();
        let current = request();
        let selection = if kind == NodeKind::File {
            Path::new("Memory").join("data")
        } else {
            PathBuf::from("Memory")
        };
        let relative = if kind == NodeKind::File {
            Path::new("")
        } else {
            Path::new("data")
        };
        let capture = || {
            CapturedSourceReader::capture_project(
                &scope,
                &selection,
                kind,
                &policy(CopyPolicy::RejectFileLinks),
                read_limits(),
                &current,
            )
        };
        let reader = capture().unwrap();
        let file = reader.open_file(relative).unwrap();
        let prior = file.read_range(0, 11).unwrap();
        let path = paths.root().join("Memory").join("data");
        let staged = path.with_file_name("replacement");
        std::fs::write(&staged, b"replacement").unwrap();
        std::fs::rename(&staged, &path).unwrap();
        assert_eq!(prior.as_ref(), b"memory-data");
        assert!(file.last_modified().is_err());
        assert!(file.read_range(0, 11).is_err());
        let fresh = capture().unwrap();
        let fresh_file = fresh.open_file(relative).unwrap();
        assert_eq!(
            fresh_file.read_range(0, 11).unwrap().as_ref(),
            b"replacement"
        );
        std::fs::remove_file(&path).unwrap();
        assert!(fresh_file.last_modified().is_err());
        assert!(fresh_file.read_range(0, 11).is_err());
        if kind == NodeKind::File {
            assert!(capture().is_err());
        } else {
            assert!(capture().unwrap().open_file(relative).is_err());
        }
    }
}
#[cfg(windows)]
#[test]
fn same_size_source_writer_open_denies_range_stream_metadata_and_capture() {
    use std::io::{Read, Write};
    let (_temporary, paths, _birth, scope) = fixture();
    let current = request();
    let reader = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    let file = reader.open_file(Path::new("data")).unwrap();
    let mut stream = file.stream(0).unwrap();
    let prior = file.read_range(0, 11).unwrap();
    let modified = file.last_modified().unwrap();
    let path = paths.root().join("Memory").join("data");
    let mut writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    writer.write_all(b"edited-data").unwrap();
    writer.flush().unwrap();
    // Hold observation metadata equal: a strict read acquisition must reject
    // the live writer even before Windows publishes its last-write timestamp.
    writer.set_modified(modified).unwrap();
    assert_eq!(writer.metadata().unwrap().len(), 11);
    assert_eq!(writer.metadata().unwrap().modified().unwrap(), modified);
    assert_eq!(
        file.read_range(0, 11).err().unwrap().raw_os_error(),
        Some(32)
    );
    assert_eq!(file.last_modified().unwrap_err().raw_os_error(), Some(32));
    assert_eq!(
        stream.read(&mut [0; 11]).unwrap_err().raw_os_error(),
        Some(32)
    );
    assert!(capture(&scope, "Memory", NodeKind::Directory, &current).is_err());
    drop(writer);
    assert_eq!(prior.as_ref(), b"memory-data");
    assert_eq!(
        reader
            .validate_file_bytes(Path::new("data"), b"edited-data", &current)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let fresh = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    assert_eq!(
        fresh
            .open_file(Path::new("data"))
            .unwrap()
            .read_range(0, 11)
            .unwrap()
            .as_ref(),
        b"edited-data"
    );
}
#[cfg(unix)]
#[test]
fn swap_after_scope_birth_never_reads_replacement_corpus() {
    let (temporary, paths, birth, scope) = fixture();
    let original = paths.root().to_owned();
    let saved = temporary.path().join("saved");
    std::fs::rename(&original, &saved).unwrap();
    std::fs::create_dir_all(original.join("Memory")).unwrap();
    std::fs::write(original.join("Memory/data"), b"REPLACEMENT-B").unwrap();
    let current = request();
    assert!(capture(&scope, "Memory", NodeKind::Directory, &current).is_err());

    std::fs::remove_dir_all(&original).unwrap();
    std::fs::rename(&saved, &original).unwrap();
    let reader = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    assert_eq!(
        reader
            .open_file(Path::new("data"))
            .unwrap()
            .read_range(0, 6)
            .unwrap()
            .as_ref(),
        b"memory"
    );
    drop(reader);
    drop(scope);
    drop(paths);
    drop(birth);
    drop(current);
    drop(temporary);
}
#[test]
fn captured_file_empty_suffix_uses_same_range_engine_and_directory_does_not() {
    let (temporary, paths, birth, scope) = fixture();
    let current = request();
    let reader = CapturedSourceReader::capture_project(
        &scope,
        &Path::new("Memory").join("data"),
        NodeKind::File,
        &policy(CopyPolicy::RejectFileLinks),
        read_limits(),
        &current,
    )
    .unwrap();
    let file = reader.open_file(Path::new("")).unwrap();
    assert_eq!(file.read_range(0, 6).unwrap().as_ref(), b"memory");
    let directory = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    assert_eq!(
        directory.open_file(Path::new("")).err().unwrap().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        capture(&scope, "", NodeKind::Directory, &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    drop(file);
    drop(directory);
    drop(reader);
    drop(scope);
    drop(paths);
    drop(birth);
    drop(current);
    drop(temporary);
}
#[test]
fn invalid_link_policy_and_raw_selection_fail_without_native_payload() {
    let (temporary, paths, birth, scope) = fixture();
    let current = request();
    assert_eq!(
        CapturedSourceReader::capture_project(
            &scope,
            Path::new("Memory"),
            NodeKind::Directory,
            &policy(CopyPolicy::FollowRegularFiles),
            read_limits(),
            &current
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidInput
    );
    for invalid in [
        "",
        "./Memory",
        "Memory/./data",
        "Memory//data",
        "Memory/",
        "../Task",
    ] {
        assert!(capture(&scope, invalid, NodeKind::Directory, &current).is_err());
    }

    let retention = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
    assert!(RetainedFileSet::new([PathBuf::from("data/./other")], retention).is_err());
    drop(scope);
    drop(paths);
    drop(birth);
    drop(current);
    drop(temporary);
}
#[test]
fn captured_file_survives_reader_drop_and_owned_range_survives_file_drop() {
    let (temporary, paths, birth, scope) = fixture();
    let current = request();
    let reader = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    let file = reader.open_file(Path::new("data")).unwrap();
    let bytes = file.read_range(0, 6).unwrap();
    let anchor = Arc::clone(scope.native_anchor());
    drop(anchor);
    drop(scope);
    drop(paths);
    drop(reader);
    // Actual retained descriptor metadata and a subsequent range read still
    // work after all scope/reader wrappers were dropped.
    file.last_modified().unwrap();
    assert_eq!(file.read_range(0, 6).unwrap().as_ref(), b"memory");
    drop(file);
    assert_eq!(bytes.as_ref(), b"memory");
    drop(bytes);
    drop(birth);
    drop(current);
    drop(temporary);
}
#[cfg(windows)]
#[test]
fn windows_owned_range_does_not_pin_root_after_reader_drop() {
    let (temporary, paths, birth, scope) = fixture();
    let current = request();
    let reader = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    let file = reader.open_file(Path::new("data")).unwrap();
    let bytes = file.read_range(0, 6).unwrap();
    let root = paths.root().to_owned();
    let moved = temporary.path().join("moved");
    drop(file);
    drop(reader);
    drop(scope);
    drop(paths);
    std::fs::rename(&root, &moved).unwrap();
    assert_eq!(bytes.as_ref(), b"memory");
    assert_eq!(
        std::fs::read(moved.join("Memory").join("data")).unwrap(),
        b"memory-data"
    );
    drop(bytes);
    drop(birth);
    drop(current);
    drop(temporary);
}

#[test]
fn owned_range_survives_nested_reader_drop() {
    let (temporary, paths, birth, scope) = fixture();
    let nested = paths.root().join("Memory").join("nested").join("deep");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("file"), b"nested").unwrap();
    let current = request();
    let reader = capture(&scope, "Memory", NodeKind::Directory, &current).unwrap();
    let file = reader
        .open_file(&Path::new("nested").join("deep").join("file"))
        .unwrap();
    let range = file.read_range(0, 6).unwrap();
    drop(file);
    drop(reader);
    drop(scope);
    drop(paths);
    assert_eq!(range.as_ref(), b"nested");
    drop(range);
    drop(birth);
    drop(current);
    drop(temporary);
}

#[cfg(unix)]
#[test]
fn selected_symlink_and_hardlink_metadata_deny_without_payload() {
    use std::os::unix::fs::symlink;
    let (temporary, paths, birth, scope) = fixture();
    let memory = paths.root().join("Memory");
    symlink(memory.join("data"), memory.join("alias")).unwrap();
    let symlink_request = request();
    assert!(capture(&scope, "Memory/alias", NodeKind::File, &symlink_request).is_err());

    std::fs::remove_file(memory.join("alias")).unwrap();
    std::fs::hard_link(memory.join("data"), temporary.path().join("hardlinked")).unwrap();
    let linked_request = request();
    assert!(capture(&scope, "Memory", NodeKind::Directory, &linked_request).is_err());
    // The entire selected subtree is scanned without hashing before link denial.

    drop(scope);
    drop(paths);
    drop(birth);
    drop(symlink_request);
    drop(linked_request);
    drop(temporary);
}

#[cfg(target_os = "linux")]
#[test]
fn non_unicode_selected_name_is_rejected_without_rewriting_the_source() {
    use std::os::unix::ffi::OsStringExt;
    let (_temporary, paths, _birth, scope) = fixture();
    let name = std::ffi::OsString::from_vec(vec![b'r', 0xff, b'w']);
    std::fs::write(paths.root().join("Memory").join(&name), b"raw").unwrap();
    let current = request();
    assert!(capture(&scope, "Memory", NodeKind::Directory, &current).is_err());
    assert_eq!(
        std::fs::read(paths.root().join("Memory").join(name)).unwrap(),
        b"raw"
    );
}

#[test]
fn cancellation_after_native_request_creation_precedes_source_work_and_payload() {
    let (temporary, paths, birth, scope) = fixture();
    let current = request();
    current.cancellation().cancel();
    assert_eq!(
        capture(&scope, "Memory", NodeKind::Directory, &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );

    drop(scope);
    drop(paths);
    drop(birth);
    drop(current);
    drop(temporary);
}

struct InstallationFixture {
    scope: CapturedPublicationScope,
    birth: std::sync::Arc<crate::file_io::SnapshotContext>,
    paths: InstallationPaths,
    temporary: tempfile::TempDir,
}
impl InstallationFixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("installation");
        std::fs::create_dir_all(root.join("runtime/native/branch")).unwrap();
        std::fs::write(root.join("runtime/native/branch/data"), b"native-data").unwrap();
        std::fs::write(root.join("runtime/sibling"), b"UNSELECTED-SIBLING").unwrap();
        let paths =
            InstallationPaths::resolve(Brand::default(), temporary.path(), None, Some(&root))
                .unwrap();
        let birth = request();
        let scope = Self::scope(&paths, ScopeLimits::new(4, 4096, 256).unwrap(), &birth).unwrap();
        Self {
            scope,
            birth,
            paths,
            temporary,
        }
    }
    fn scope(
        paths: &InstallationPaths,
        limits: ScopeLimits,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<CapturedPublicationScope> {
        CapturedPublicationScope::installation(
            paths,
            crate::installation_identity::InstallationId::new("native-installation").unwrap(),
            [paths.root().to_owned()],
            limits,
            request,
        )
    }
    fn capture(
        &self,
        relative: &Path,
        kind: NodeKind,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<CapturedSourceReader> {
        CapturedSourceReader::capture_native_scope(
            &self.scope,
            relative,
            kind,
            &policy(CopyPolicy::RejectFileLinks),
            read_limits(),
            request,
        )
    }
    fn reader(
        &self,
        request: &std::sync::Arc<crate::file_io::SnapshotContext>,
    ) -> io::Result<CapturedSourceReader> {
        self.capture(
            &Path::new("runtime").join("native"),
            NodeKind::Directory,
            request,
        )
    }
}
struct CaptureHooks;
impl Drop for CaptureHooks {
    fn drop(&mut self) {
        CAPTURE_STEP.with(|slot| *slot.borrow_mut() = None);
    }
}
fn at_capture_step(step: CaptureStep, action: impl FnOnce() + 'static) -> CaptureHooks {
    CAPTURE_STEP.with(|slot| *slot.borrow_mut() = Some((step, Box::new(action))));
    CaptureHooks
}

#[test]
fn installation_public_project_factories_deny_before_admission_or_request_check() {
    let f = InstallationFixture::new();
    let current = request();
    current.cancellation().cancel();
    assert_eq!(
        capture(&f.scope, "runtime", NodeKind::Directory, &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let criteria = ProjectSelectionCriteria {
        subtree: Some(Path::new("runtime")),
        extras: &[],
        include_readme: false,
        extensions: &[".md"],
        ignore: ProjectIgnoreCriteria {
            protected_defaults: &[],
            custom_control: None,
            git_control: None,
        },
    };
    let limits = ProjectSelectionLimits {
        max_depth: 1,
        max_input_items: 1,
        max_input_bytes: 1,
        max_observed_path_bytes: 1,
        max_selected_file_bytes: 1,
        max_control_file_bytes: 1,
    };
    assert_eq!(
        CapturedSourceReader::capture_selected_project(
            &f.scope,
            criteria,
            limits,
            read_limits(),
            &current
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn installation_private_subtree_reader_hash_metadata_stream_batch_and_sibling_zero_io() {
    use crate::artifact_store::{AggregateReadLimits, SourceRangeRequest};
    let f = InstallationFixture::new();
    let current = request();
    let reader = f.reader(&current).unwrap();
    assert_eq!(
        reader
            .nodes()
            .map(|n| n.relative().to_owned())
            .collect::<Vec<_>>(),
        [
            PathBuf::new(),
            PathBuf::from("branch"),
            Path::new("branch").join("data")
        ]
    );

    assert_eq!(
        reader.open_file(Path::new("sibling")).err().unwrap().kind(),
        io::ErrorKind::NotFound
    );
    reader.check_request(&current).unwrap();
    let file = reader.open_file(&Path::new("branch").join("data")).unwrap();
    assert_eq!(file.len(), 11);
    file.last_modified().unwrap();
    let bytes = file.read_range(0, 6).unwrap();
    assert_eq!(bytes.as_ref(), b"native");
    let mut stream = file.stream(7).unwrap();
    let mut tail = Vec::new();
    stream.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, b"data");
    let batch = reader
        .read_ranges(
            &[
                SourceRangeRequest::new(&file, 0, 3).unwrap(),
                SourceRangeRequest::new(&file, 7, 4).unwrap(),
            ],
            AggregateReadLimits::new(2, 7).unwrap(),
        )
        .unwrap();
    assert_eq!(
        batch.iter().map(|b| b.as_ref()).collect::<Vec<_>>(),
        [b"nat".as_slice(), b"data".as_slice()]
    );
    let fingerprint = reader.fingerprint();
    reader.revalidate_content().unwrap();
    assert_eq!(reader.fingerprint(), fingerprint);
}

#[test]
fn installation_direct_file_uses_empty_suffix_and_original_request() {
    let f = InstallationFixture::new();
    let current = request();
    let reader = f
        .capture(
            &Path::new("runtime").join("sibling"),
            NodeKind::File,
            &current,
        )
        .unwrap();
    assert_eq!(reader.nodes().len(), 1);
    let file = reader.open_file(Path::new("")).unwrap();
    assert_eq!(file.read_range(0, 10).unwrap().as_ref(), b"UNSELECTED");

    assert!(reader.check_request(&request()).is_err());
}

#[cfg(unix)]
#[test]
fn installation_swap_after_birth_denies_attacker_and_restore_preserves_original() {
    let f = InstallationFixture::new();
    let root = f.paths.root();
    let saved = f.temporary.path().join("saved");
    std::fs::rename(root, &saved).unwrap();
    std::fs::create_dir_all(root.join("runtime/native")).unwrap();
    std::fs::write(root.join("runtime/native/data"), b"ATTACKER").unwrap();
    let current = request();
    assert!(f.reader(&current).is_err());

    assert_eq!(
        std::fs::read(root.join("runtime/native/data")).unwrap(),
        b"ATTACKER"
    );
    std::fs::remove_dir_all(root).unwrap();
    std::fs::rename(&saved, root).unwrap();
    let reader = f.reader(&current).unwrap();
    assert_eq!(
        reader
            .open_file(&Path::new("branch").join("data"))
            .unwrap()
            .read_range(0, 6)
            .unwrap()
            .as_ref(),
        b"native"
    );
}

#[cfg(unix)]
#[test]
fn installation_root_swap_after_first_admission_denies_before_payload_without_refund() {
    let f = InstallationFixture::new();
    let root = f.paths.root().to_owned();
    let saved = f.temporary.path().join("saved");
    let replacement = f.temporary.path().join("replacement");
    std::fs::create_dir_all(replacement.join("runtime/native")).unwrap();
    std::fs::write(replacement.join("runtime/native/data"), b"ATTACKER").unwrap();
    let current = request();
    let r = root.clone();
    let s = saved.clone();
    let b = replacement.clone();
    let _hook = at_capture_step(CaptureStep::AfterAdmission, move || {
        std::fs::rename(&r, &s).unwrap();
        std::fs::rename(&b, &r).unwrap();
    });
    assert!(f.reader(&current).is_err());

    assert_eq!(
        std::fs::read(root.join("runtime/native/data")).unwrap(),
        b"ATTACKER"
    );
    std::fs::rename(&root, &replacement).unwrap();
    std::fs::rename(&saved, &root).unwrap();
}

#[test]
fn installation_stream_keeps_root_until_drop_but_returned_bytes_do_not() {
    let f = InstallationFixture::new();
    let current = request();
    let reader = f.reader(&current).unwrap();
    let file = reader.open_file(&Path::new("branch").join("data")).unwrap();
    let bytes = file.read_range(0, 6).unwrap();
    let mut stream = file.stream(7).unwrap();
    let weak = Arc::downgrade(&f.scope.native_anchor().root.file);
    let _birth = f.birth.clone();
    drop(reader);
    drop(file);
    let InstallationFixture {
        scope,
        birth: fixture_birth,
        paths,
        temporary,
    } = f;
    drop(scope);
    drop(fixture_birth);
    drop(paths);
    assert!(weak.upgrade().is_some());
    let mut tail = Vec::new();
    stream.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, b"data");
    drop(stream);
    assert!(weak.upgrade().is_none());
    assert_eq!(bytes.as_ref(), b"native");
    drop(bytes);
    assert!(weak.upgrade().is_none());
    drop(temporary);
}

#[test]
fn installation_scope_input_limits_exact_and_one_less_are_physical_and_readonly() {
    let f = InstallationFixture::new();
    let baseline = f.scope.binding().unwrap();
    let value = serde_json::to_value(&baseline).unwrap();
    let path_bytes = |v: &serde_json::Value| {
        let text = v.as_str().unwrap();
        if cfg!(windows) {
            text.encode_utf16().count() * 2
        } else {
            text.len()
        }
    };
    let paths: Vec<_> = [&value["owner_root"], &value["runtime_root"]]
        .into_iter()
        .chain(value["allowed_roots"].as_array().unwrap().iter())
        .chain(
            value["directories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| &d["path"]),
        )
        .collect();
    let per_path = paths.iter().map(|p| path_bytes(p)).max().unwrap();
    // Find the existing captured-entry/depth input bound. No work counter is used.
    let mut observations = None;
    for count in 1..=256 {
        let current = request();
        let result = InstallationFixture::scope(
            &f.paths,
            ScopeLimits::new(1, per_path, count).unwrap(),
            &current,
        );
        match result {
            Ok(scope) => {
                drop(scope);
                observations = Some(count);
                break;
            }
            Err(error) => {
                assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
            }
        }
    }
    let observations = observations.expect("bounded fixture has an actual capture boundary");
    let current = request();
    let exact = InstallationFixture::scope(
        &f.paths,
        ScopeLimits::new(1, per_path, observations).unwrap(),
        &current,
    )
    .unwrap();
    assert!(exact.binding().unwrap() == baseline);
    drop(exact);
    for limits in [
        ScopeLimits::new(1, per_path - 1, observations).unwrap(),
        ScopeLimits::new(1, per_path, observations - 1).unwrap(),
    ] {
        let current = request();
        assert_eq!(
            InstallationFixture::scope(&f.paths, limits, &current)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
    let current = request();
    assert_eq!(
        CapturedPublicationScope::installation(
            &f.paths,
            crate::installation_identity::InstallationId::new("native-installation").unwrap(),
            [
                f.paths.root().to_owned(),
                f.paths.root().join("runtime/native")
            ],
            ScopeLimits::new(1, per_path, observations).unwrap(),
            &current
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::FileTooLarge
    );
}

#[test]
fn installation_cancel_and_expiry_after_real_payload_never_publish_partial_reader() {
    let f = InstallationFixture::new();
    for deadline in [false, true] {
        let current = Arc::new(SnapshotContext::new(
            Instant::now()
                + if deadline {
                    Duration::from_secs(1)
                } else {
                    Duration::from_secs(30)
                },
            Cancellation::default(),
        ));
        let cancel = current.cancellation().clone();
        let end = current.deadline().unwrap();
        let _hook = at_capture_step(CaptureStep::AfterPayloadRead, move || {
            if deadline {
                std::thread::sleep(end.saturating_duration_since(Instant::now()));
            } else {
                cancel.cancel();
            }
        });
        assert_eq!(
            f.reader(&current).err().unwrap().kind(),
            if deadline {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::Interrupted
            }
        );
    }
}

#[cfg(unix)]
#[test]
fn installation_growth_during_native_hash_fails_after_actual_read() {
    let f = InstallationFixture::new();
    let path = f.paths.root().join("runtime/native/branch/data");
    let _hook = at_capture_step(CaptureStep::AfterPayloadRead, move || {
        std::fs::write(path, b"native-data-GROWTH").unwrap();
    });
    let current = request();
    assert!(f.reader(&current).is_err());
}

#[test]
fn installation_poisoned_birth_root_denies_without_payload() {
    let f = InstallationFixture::new();
    let root = Arc::clone(&f.scope.native_anchor().root.file);
    let _ = std::thread::spawn(move || {
        let _guard = root.write().unwrap();
        panic!("fixture poison");
    })
    .join();
    let poisoned = request();
    assert_eq!(
        f.reader(&poisoned).err().unwrap().kind(),
        io::ErrorKind::Other
    );
}

#[cfg(unix)]
#[test]
fn installation_links_and_wrong_kind_fail_during_metadata_phase() {
    use std::os::unix::fs::symlink;
    let f = InstallationFixture::new();
    let data = f.paths.root().join("runtime/native/branch/data");
    let alias = f.paths.root().join("runtime/alias");
    symlink(&data, &alias).unwrap();
    for (path, kind) in [
        (Path::new("runtime").join("alias"), NodeKind::File),
        (Path::new("runtime").join("sibling"), NodeKind::Directory),
    ] {
        let current = request();
        assert!(f.capture(&path, kind, &current).is_err());
    }
    std::fs::remove_file(alias).unwrap();
    std::fs::hard_link(&data, f.temporary.path().join("hardlinked")).unwrap();
    let current = request();
    assert!(f.reader(&current).is_err());
}

#[cfg(unix)]
#[test]
fn installation_readonly_unicode_owner_and_leaf_are_preserved_without_repair() {
    use std::{ffi::OsString, os::unix::fs::PermissionsExt};
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("installation-界");
    std::fs::create_dir_all(root.join("runtime/native")).unwrap();
    let leaf = OsString::from("body-é");
    std::fs::write(root.join("runtime/native").join(&leaf), b"raw").unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).unwrap();
    let paths =
        InstallationPaths::resolve(Brand::default(), temporary.path(), None, Some(&root)).unwrap();
    let birth = request();
    let scope = InstallationFixture::scope(&paths, ScopeLimits::new(4, 4096, 256).unwrap(), &birth)
        .unwrap();
    let current = request();
    let reader = CapturedSourceReader::capture_native_scope(
        &scope,
        &Path::new("runtime").join("native"),
        NodeKind::Directory,
        &policy(CopyPolicy::RejectFileLinks),
        read_limits(),
        &current,
    )
    .unwrap();
    assert!(
        reader
            .nodes()
            .any(|n| n.relative().as_os_str() == leaf.as_os_str())
    );
    assert_eq!(
        reader
            .open_file(Path::new(&leaf))
            .unwrap()
            .read_range(0, 3)
            .unwrap()
            .as_ref(),
        b"raw"
    );
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o555
    );
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn installation_unreadable_body_denies_for_nonprivileged_host_and_never_repairs() {
    use std::os::unix::fs::PermissionsExt;
    let f = InstallationFixture::new();
    let path = f.paths.root().join("runtime/native/branch/data");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();
    let privileged_read = std::fs::File::open(&path).is_ok();
    let current = request();
    let result = f.reader(&current);
    if privileged_read {
        assert!(result.is_ok());
    } else {
        assert_eq!(
            result.err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o0
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(windows)]
#[test]
fn installation_windows_owned_range_does_not_pin_root_after_reader_drop() {
    let f = InstallationFixture::new();
    let current = request();
    let reader = f.reader(&current).unwrap();
    let bytes = reader
        .open_file(&Path::new("branch").join("data"))
        .unwrap()
        .read_range(0, 6)
        .unwrap();
    let root = f.paths.root().to_owned();
    let saved = f.temporary.path().join("saved");
    let InstallationFixture {
        scope,
        birth: fixture_birth,
        paths,
        temporary,
    } = f;
    drop(reader);
    drop(scope);
    drop(paths);
    std::fs::rename(&root, &saved).unwrap();
    assert_eq!(bytes.as_ref(), b"native");
    assert_eq!(
        std::fs::read(
            saved
                .join("runtime")
                .join("native")
                .join("branch")
                .join("data")
        )
        .unwrap(),
        b"native-data"
    );
    drop(bytes);
    // Keep the birth request alive while proving that returned bytes own no handles.
    drop(fixture_birth);
    drop(temporary);
}

#[test]
fn installation_missing_owner_capture_has_no_provisioning_or_live_resources() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("absent-owner");
    let paths =
        InstallationPaths::resolve(Brand::default(), temporary.path(), None, Some(&root)).unwrap();
    let current = request();
    assert_eq!(
        InstallationFixture::scope(&paths, ScopeLimits::new(4, 4096, 256).unwrap(), &current)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert!(!root.exists());
}

#[cfg(windows)]
#[test]
fn installation_windows_unicode_owner_and_leaf_keep_utf16_identity() {
    use std::{ffi::OsString, os::windows::ffi::OsStrExt};
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("installation-界");
    let leaf = OsString::from("body-😀");
    std::fs::create_dir_all(root.join("runtime/native")).unwrap();
    std::fs::write(root.join("runtime/native").join(&leaf), b"wide").unwrap();
    let paths =
        InstallationPaths::resolve(Brand::default(), temporary.path(), None, Some(&root)).unwrap();
    let birth = request();
    let scope = InstallationFixture::scope(&paths, ScopeLimits::new(4, 4096, 256).unwrap(), &birth)
        .unwrap();
    let current = request();
    let reader = CapturedSourceReader::capture_native_scope(
        &scope,
        &Path::new("runtime").join("native"),
        NodeKind::Directory,
        &policy(CopyPolicy::RejectFileLinks),
        read_limits(),
        &current,
    )
    .unwrap();
    let node = reader.nodes().find(|n| n.kind() == NodeKind::File).unwrap();
    assert_eq!(
        node.relative()
            .as_os_str()
            .encode_wide()
            .collect::<Vec<_>>(),
        leaf.encode_wide().collect::<Vec<_>>()
    );
    assert_eq!(
        reader
            .open_file(Path::new(&leaf))
            .unwrap()
            .read_range(0, 4)
            .unwrap()
            .as_ref(),
        b"wide"
    );
    assert_eq!(
        native_name_bytes(root.as_os_str()).unwrap(),
        root.as_os_str().encode_wide().count() * 2
    );
    drop(reader);
    drop(scope);
    drop(birth);
}

#[cfg(any(unix, windows))]
#[test]
fn missing_non_unicode_criteria_and_changes_are_errors_before_discovery() {
    let (_temporary, paths, _birth, scope) = fixture();
    let request = request();
    #[cfg(unix)]
    let missing = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(b"missing-\xff.md".to_vec()))
    };
    #[cfg(windows)]
    let missing = {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&[109, 0xd800, 46, 109, 100]))
    };
    assert!(!paths.root().join(&missing).exists());
    let limits = ProjectSelectionLimits {
        max_depth: 8,
        max_input_items: 16,
        max_input_bytes: 4096,
        max_observed_path_bytes: 4096,
        max_selected_file_bytes: 4096,
        max_control_file_bytes: 4096,
    };
    let extra = [missing.as_path()];
    for input in 0..3 {
        let criteria = ProjectSelectionCriteria {
            subtree: Some(if input == 0 {
                missing.as_path()
            } else {
                Path::new("Memory")
            }),
            extras: if input == 1 { &extra } else { &[] },
            include_readme: false,
            extensions: &[".md"],
            ignore: ProjectIgnoreCriteria {
                protected_defaults: &[],
                custom_control: None,
                git_control: None,
            },
        };
        let result = if input == 2 {
            selection::capture_changes_metadata(
                &scope,
                criteria,
                &std::collections::BTreeSet::from([missing.clone()]),
                limits,
                &request,
            )
        } else {
            selection::capture_metadata(&scope, criteria, limits, &request)
        };
        let error = result
            .err()
            .expect("invalid input cannot become an empty selection");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("Unicode"));
    }
    assert_eq!(
        std::fs::read(paths.root().join("Memory/data")).unwrap(),
        b"memory-data"
    );
}
