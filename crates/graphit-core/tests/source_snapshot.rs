use graphit_core::{
    artifact_store::{
        CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotContext,
        SnapshotPolicy, SourceSnapshot,
    },
    file_lock::Cancellation,
};
use graphit_fs::NodeKind;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn policy(paths: &[&str], retained_bytes: usize) -> SnapshotPolicy {
    let retained = RetainedBytesLimits::new(retained_bytes, 8, 4096).unwrap();
    SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(8 * 1024 * 1024, 64, 4096, 8).unwrap(),
        RetainedFileSet::new(
            paths
                .iter()
                .map(|path| path.split('/').collect::<PathBuf>()),
            retained,
        )
        .unwrap(),
        retained,
    )
    .unwrap()
}

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}

#[test]
fn keeps_only_selected_bytes_and_streams_assets_with_shared_capture() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("document.md"), b"hello").unwrap();
    fs::write(temp.path().join("asset.bin"), vec![42; 1024 * 1024]).unwrap();
    fs::create_dir(temp.path().join("empty")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            temp.path().join("asset.bin"),
            fs::Permissions::from_mode(0o750),
        )
        .unwrap();
    }
    let io = context();
    let snapshot = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&["document.md"], 5),
        &io,
    )
    .unwrap();
    assert_eq!(snapshot.retained_bytes(), 5);
    assert_eq!(
        snapshot.retained(std::path::Path::new("document.md")),
        Some(&b"hello"[..])
    );
    assert!(
        snapshot
            .retained(std::path::Path::new("asset.bin"))
            .is_none()
    );
    assert_eq!(snapshot.nodes().len(), 4);
    let clone = snapshot.clone();
    assert_eq!(
        snapshot
            .retained(std::path::Path::new("document.md"))
            .unwrap()
            .as_ptr(),
        clone
            .retained(std::path::Path::new("document.md"))
            .unwrap()
            .as_ptr()
    );
    let empty = snapshot
        .nodes()
        .find(|n| n.relative() == std::path::Path::new("empty"))
        .unwrap();
    assert_eq!(empty.kind(), NodeKind::Directory);
    #[cfg(unix)]
    assert_eq!(
        snapshot
            .nodes()
            .find(|n| n.relative() == std::path::Path::new("asset.bin"))
            .unwrap()
            .mode()
            .unix_bits(),
        Some(0o750)
    );

    snapshot.revalidate_content(&io).unwrap();

    assert_eq!(fs::read(temp.path().join("document.md")).unwrap(), b"hello");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
}

#[test]
fn content_change_is_rejected_even_when_size_and_mtime_are_restored() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("document.md");
    fs::write(&file, b"first").unwrap();
    let snapshot =
        SourceSnapshot::capture(&file, NodeKind::File, &policy(&[""], 5), &context()).unwrap();
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    fs::write(&file, b"other").unwrap();
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert_eq!(
        snapshot.revalidate_content(&context()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        snapshot.retained(std::path::Path::new("")),
        Some(&b"first"[..])
    );
    assert_eq!(fs::read(&file).unwrap(), b"other");
}

#[test]
fn fingerprint_is_tree_content_and_modes_not_absolute_origin_or_inode() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    fs::write(a.path().join("file"), b"equal").unwrap();
    fs::write(b.path().join("file"), b"equal").unwrap();
    let options = policy(&["file"], 5);
    let first =
        SourceSnapshot::capture(a.path(), NodeKind::Directory, &options, &context()).unwrap();
    let second =
        SourceSnapshot::capture(b.path(), NodeKind::Directory, &options, &context()).unwrap();
    assert_eq!(first.fingerprint(), second.fingerprint());
    fs::create_dir(b.path().join("empty")).unwrap();
    let changed =
        SourceSnapshot::capture(b.path(), NodeKind::Directory, &options, &context()).unwrap();
    assert_ne!(first.fingerprint(), changed.fingerprint());
}

#[test]
fn retained_payload_limit_is_exact_and_revalidation_is_repeatable() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("file");
    fs::write(&file, b"12345").unwrap();
    let ctx = context();
    let snapshot = SourceSnapshot::capture(&file, NodeKind::File, &policy(&[""], 5), &ctx).unwrap();
    snapshot.revalidate_content(&ctx).unwrap();
    snapshot.revalidate_content(&ctx).unwrap();
    assert_eq!(
        SourceSnapshot::capture(&file, NodeKind::File, &policy(&[""], 4), &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert_eq!(fs::read(file).unwrap(), b"12345");
}

#[test]
fn input_selection_is_validated_before_retention() {
    let limits = RetainedBytesLimits::new(100, 4, 100).unwrap();
    assert!(RetainedFileSet::new([PathBuf::from("../outside")], limits).is_err());
    assert!(RetainedFileSet::new([PathBuf::from("same"), PathBuf::from("same")], limits).is_err());
    assert!(RetainedBytesLimits::new(0, 1, 1).is_err());
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("directory")).unwrap();
    assert_eq!(
        SourceSnapshot::capture(
            temp.path(),
            NodeKind::Directory,
            &policy(&["directory"], 100),
            &context()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        SourceSnapshot::capture(
            temp.path(),
            NodeKind::Directory,
            &policy(&["missing"], 100),
            &context()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::NotFound
    );
}

#[test]
fn cancellation_and_deadline_do_not_mutate_the_source() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("file"), b"source").unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(60), cancellation);
    assert_eq!(
        SourceSnapshot::capture(
            temp.path(),
            NodeKind::Directory,
            &policy(&["file"], 100),
            &cancelled
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert_eq!(
        SourceSnapshot::capture(
            temp.path(),
            NodeKind::Directory,
            &policy(&["file"], 100),
            &expired
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::TimedOut
    );

    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    assert_eq!(fs::read(temp.path().join("file")).unwrap(), b"source");
}

#[cfg(unix)]
#[test]
fn non_unicode_sources_fail_without_modifying_the_file() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let temp = tempfile::tempdir().unwrap();
    let name = PathBuf::from(OsString::from_vec(vec![b'f', 0xff]));
    fs::write(temp.path().join(&name), b"native").unwrap();
    assert!(
        RetainedFileSet::new([name.clone()], RetainedBytesLimits::new(6, 1, 64).unwrap()).is_err()
    );
    assert!(
        SourceSnapshot::capture(
            temp.path(),
            NodeKind::Directory,
            &policy(&[], 6),
            &context()
        )
        .is_err()
    );
    assert_eq!(fs::read(temp.path().join(name)).unwrap(), b"native");
}

#[test]
fn restoring_binding_is_bounded_data_and_does_not_recapture_or_authorize_a_source() {
    use graphit_core::artifact_store::SnapshotBindingRecord;
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("input");
    fs::write(&file, b"binding").unwrap();
    let snapshot =
        SourceSnapshot::capture(&file, NodeKind::File, &policy(&[""], 7), &context()).unwrap();
    let record = snapshot.binding().unwrap();
    let bytes = serde_json::to_vec(&record).unwrap();
    let limits = CopyLimits::new(7, 1, 1, 1).unwrap();
    let restored = SnapshotBindingRecord::pure_restore(&bytes, limits, bytes.len()).unwrap();
    assert!(record == restored);
    assert!(restored.matches_snapshot(&snapshot));
    assert_eq!(restored.source(), file);
    assert_eq!(restored.node_count(), 1);
    assert!(SnapshotBindingRecord::pure_restore(&bytes, limits, bytes.len() - 1).is_err());
    fs::remove_file(&file).unwrap();
    let detached = SnapshotBindingRecord::pure_restore(&bytes, limits, bytes.len()).unwrap();
    assert_eq!(detached.node_count(), 1);
    assert!(!detached.source().exists());
    assert!(SourceSnapshot::capture(&file, NodeKind::File, &policy(&[""], 7), &context()).is_err());
}

#[test]
fn binding_decoder_rejects_unknown_duplicate_null_wrong_os_and_inconsistent_nodes() {
    use graphit_core::artifact_store::SnapshotBindingRecord;
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("file"), b"x").unwrap();
    let record = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&["file"], 1),
        &context(),
    )
    .unwrap()
    .binding()
    .unwrap();
    let bytes = serde_json::to_vec(&record).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    let path_bytes = if cfg!(windows) { 8 } else { 4 };
    let limits = CopyLimits::new(10, 2, path_bytes, 1).unwrap();
    let decode = |bytes: &[u8]| SnapshotBindingRecord::pure_restore(bytes, limits, 32 * 1024);
    let mut malformed = vec![
        format!("{{\"unexpected\":0,{}", &text[1..]),
        text.replace(
            "\"kind\":\"Directory\"",
            "\"kind\":\"Directory\",\"k\\u0069nd\":\"Directory\"",
        ),
        text.replace("\"nodes\":[", "\"nodes\":null,\"ignored\":["),
        format!("{text} {{}}"),
    ];
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut non_string = original.clone();
    non_string["source"] = serde_json::json!({"os":"Unix","units":[47,112]});
    malformed.push(non_string.to_string());
    let mut inconsistent = original.clone();
    inconsistent["nodes"][1]["content"]["bytes"] = serde_json::json!(2);
    malformed.push(inconsistent.to_string());
    let mut invalid_time = original.clone();
    invalid_time["nodes"][0]["opened"]["modified"]["nanos"] = serde_json::json!(1_000_000_000u32);
    malformed.push(invalid_time.to_string());
    for bytes in malformed {
        assert!(decode(bytes.as_bytes()).is_err());
    }
    assert!(
        SnapshotBindingRecord::pure_restore(
            &bytes,
            CopyLimits::new(10, 1, path_bytes, 1).unwrap(),
            32 * 1024
        )
        .is_err()
    );
    assert!(
        decode(&bytes).unwrap().matches_snapshot(
            &SourceSnapshot::capture(
                temp.path(),
                NodeKind::Directory,
                &policy(&["file"], 1),
                &context()
            )
            .unwrap()
        )
    );
}

#[test]
fn capture_tree_limits_are_exact_and_reject_one_more_byte_node_depth_or_path_unit() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("d")).unwrap();
    fs::write(temp.path().join("d/f"), b"xy").unwrap();
    let paths = if cfg!(windows) { 8 } else { 4 };
    let retention = RetainedBytesLimits::new(1, 1, 1).unwrap();
    for (bytes, nodes, units, depth, accepted) in [
        (2, 3, paths, 2, true),
        (1, 3, paths, 2, false),
        (2, 2, paths, 2, false),
        (2, 3, paths - 1, 2, false),
        (2, 3, paths, 1, false),
    ] {
        let input = SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            CopyLimits::new(bytes, nodes, units, depth).unwrap(),
            RetainedFileSet::new([], retention).unwrap(),
            retention,
        )
        .unwrap();
        let result = SourceSnapshot::capture(temp.path(), NodeKind::Directory, &input, &context());
        if accepted {
            assert_eq!(result.unwrap().nodes().count(), 3);
        } else {
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::FileTooLarge);
        }
    }
    assert_eq!(fs::read(temp.path().join("d/f")).unwrap(), b"xy");
}

#[test]
fn restored_binding_rejects_aggregate_length_overflow_without_reading_files() {
    use graphit_core::artifact_store::SnapshotBindingRecord;
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a"), b"x").unwrap();
    fs::write(temp.path().join("b"), b"y").unwrap();
    let capture = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&[], 1),
        &context(),
    )
    .unwrap();
    let mut value = serde_json::to_value(capture.binding().unwrap()).unwrap();
    for node in value["nodes"].as_array_mut().unwrap().iter_mut().skip(1) {
        node["content"]["bytes"] = serde_json::json!(u64::MAX);
        node["opened"]["bytes"] = serde_json::json!(u64::MAX);
        node["original"]["bytes"] = serde_json::json!(u64::MAX);
    }
    let limits = CopyLimits::new(u64::MAX, 3, 8, 1).unwrap();
    let mut exact = value.clone();
    exact["nodes"].as_array_mut().unwrap().truncate(2);
    // A single MAX-length descriptor is valid data; the second overflows the aggregate.
    let exact_bytes = serde_json::to_vec(&exact).unwrap();
    assert!(SnapshotBindingRecord::pure_restore(&exact_bytes, limits, exact_bytes.len()).is_ok());
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::remove_file(temp.path().join("a")).unwrap();
    fs::remove_file(temp.path().join("b")).unwrap();
    assert!(
        SnapshotBindingRecord::pure_restore(
            &bytes,
            CopyLimits::new(u64::MAX, 3, 8, 1).unwrap(),
            bytes.len()
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn binding_preserves_times_before_the_epoch_and_capture_never_grants_permissions() {
    use graphit_core::artifact_store::SnapshotBindingRecord;
    use std::{os::unix::fs::PermissionsExt, time::UNIX_EPOCH};
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("old-file");
    fs::write(&file, b"readonly").unwrap();
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH - Duration::from_secs(100)))
        .unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    let snapshot =
        SourceSnapshot::capture(&file, NodeKind::File, &policy(&[""], 8), &context()).unwrap();
    snapshot.revalidate_content(&context()).unwrap();
    let record = snapshot.binding().unwrap();
    let bytes = serde_json::to_vec(&record).unwrap();
    assert!(
        SnapshotBindingRecord::pure_restore(
            &bytes,
            CopyLimits::new(8, 1, 1, 1).unwrap(),
            32 * 1024
        )
        .unwrap()
            == record
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o444
    );
    fs::set_permissions(&file, fs::Permissions::from_mode(0o204)).unwrap();
    // Run the permission failure on an ordinary Unix account; superuser
    // privileges legitimately bypass these mode bits.
    if fs::File::open(&file).is_err() {
        assert!(
            SourceSnapshot::capture(&file, NodeKind::File, &policy(&[""], 8), &context()).is_err()
        );
    }
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o204
    );
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(unix)]
#[test]
fn parent_alias_retarget_is_detected_even_for_same_hardlinked_file() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("file"), b"same").unwrap();
    fs::hard_link(a.join("file"), b.join("file")).unwrap();
    let alias = temp.path().join("alias");
    symlink(&a, &alias).unwrap();
    let source = alias.join("file");
    let policy = policy(&[""], 4);
    let snapshot = SourceSnapshot::capture(&source, NodeKind::File, &policy, &context()).unwrap();
    let binding = snapshot.binding().unwrap();
    fs::remove_file(&alias).unwrap();
    symlink(&b, &alias).unwrap();
    assert_eq!(
        snapshot.revalidate_content(&context()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let fresh = SourceSnapshot::capture(&source, NodeKind::File, &policy, &context()).unwrap();
    assert_eq!(snapshot.fingerprint(), fresh.fingerprint());
    assert!(!binding.matches_snapshot(&fresh));
    assert_eq!(fs::read(a.join("file")).unwrap(), b"same");
    assert_eq!(fs::read(b.join("file")).unwrap(), b"same");
}

#[test]
fn relative_path_budget_uses_exact_native_units() {
    let bytes = if cfg!(windows) { 8 } else { 4 };
    assert!(
        RetainedFileSet::new(
            [PathBuf::from("file")],
            RetainedBytesLimits::new(1, 1, bytes).unwrap()
        )
        .is_ok()
    );
    assert_eq!(
        RetainedFileSet::new(
            [PathBuf::from("file")],
            RetainedBytesLimits::new(1, 1, bytes - 1).unwrap()
        )
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::FileTooLarge
    );
}

#[cfg(windows)]
#[test]
fn windows_relative_budget_preserves_unicode_utf16_and_rejects_unpaired_surrogates() {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    for units in [&[0xd83d, 0xde00][..], &[0x20ac][..]] {
        let path = PathBuf::from(OsString::from_wide(units));
        let bytes = units.len() * 2;
        assert!(
            RetainedFileSet::new(
                [path.clone()],
                RetainedBytesLimits::new(1, 1, bytes).unwrap()
            )
            .is_ok()
        );
        assert_eq!(
            RetainedFileSet::new([path], RetainedBytesLimits::new(1, 1, bytes - 1).unwrap())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
    let invalid = PathBuf::from(OsString::from_wide(&[0xd800]));
    assert!(RetainedFileSet::new([invalid], RetainedBytesLimits::new(1, 1, 64).unwrap()).is_err());
}

fn output_policy(
    snapshot: &SourceSnapshot,
    kind: NodeKind,
) -> graphit_core::artifact_store::PayloadPolicy {
    graphit_core::artifact_store::PayloadPolicy::new(
        kind,
        snapshot.nodes().next().unwrap().mode(),
        CopyLimits::new(2 * 1024 * 1024, 16, 1024, 4).unwrap(),
    )
}

#[test]
fn output_plan_renames_document_and_preserves_streaming_assets_and_empty_directories() {
    use graphit_core::artifact_store::{PayloadOverlay, ValidatedPayloadPlan};
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("doc"), b"input").unwrap();
    fs::write(temp.path().join("asset"), vec![9; 1024 * 1024]).unwrap();
    fs::create_dir(temp.path().join("empty")).unwrap();
    let snapshot = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&["doc"], 5),
        &context(),
    )
    .unwrap();
    let document_mode = snapshot
        .nodes()
        .find(|n| n.relative() == Path::new("doc"))
        .unwrap()
        .mode();
    let policy = output_policy(&snapshot, NodeKind::Directory);
    let overlay = PayloadOverlay::new(
        PathBuf::from("doc"),
        PathBuf::from("native"),
        b"rendered".to_vec(),
        document_mode,
    )
    .unwrap();
    let plan = ValidatedPayloadPlan::new(snapshot.clone(), [overlay], &policy, &context()).unwrap();
    assert_eq!(plan.nodes().len(), 4);
    assert!(plan.nodes().all(|n| n.relative() != Path::new("doc")));
    let asset = plan
        .nodes()
        .find(|n| n.relative() == Path::new("asset"))
        .unwrap();
    let original = snapshot
        .nodes()
        .find(|n| n.relative() == Path::new("asset"))
        .unwrap();
    assert_eq!(asset.digest(), original.digest());
    assert!(asset.mode() == original.mode());
    assert_eq!(asset.bytes(), 1024 * 1024);
    assert_eq!(fs::read(temp.path().join("doc")).unwrap(), b"input");
    assert!(temp.path().join("empty").is_dir());
    assert!(!temp.path().join("native").exists());
}

#[test]
fn output_plans_reject_collisions_ancestors_missing_inputs_and_copy_overflow() {
    use graphit_core::artifact_store::{PayloadOverlay, ValidatedPayloadPlan};
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("doc"), b"input").unwrap();
    fs::write(temp.path().join("asset"), b"asset").unwrap();
    let snapshot = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&["doc"], 5),
        &context(),
    )
    .unwrap();
    let mode = snapshot.nodes().next().unwrap().mode();
    let policy = output_policy(&snapshot, NodeKind::Directory);
    for (original, output) in [
        ("missing", "native"),
        ("", "native"),
        ("doc", "asset"),
        ("doc", "asset/child"),
        ("doc", "missing/child"),
    ] {
        let overlay = PayloadOverlay::new(
            PathBuf::from(original),
            PathBuf::from(output),
            b"out".to_vec(),
            mode,
        )
        .unwrap();
        assert!(
            ValidatedPayloadPlan::new(snapshot.clone(), [overlay], &policy, &context()).is_err()
        );
    }
    let make = || {
        PayloadOverlay::new(
            PathBuf::from("doc"),
            PathBuf::from("native"),
            b"out".to_vec(),
            mode,
        )
        .unwrap()
    };
    assert!(
        ValidatedPayloadPlan::new(snapshot.clone(), [make(), make()], &policy, &context()).is_err()
    );
    let small = graphit_core::artifact_store::PayloadPolicy::new(
        NodeKind::Directory,
        mode,
        CopyLimits::new(7, 16, 1024, 4).unwrap(),
    );
    assert_eq!(
        ValidatedPayloadPlan::new(snapshot, [make()], &small, &context())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert!(
        PayloadOverlay::new(
            PathBuf::from("doc"),
            PathBuf::from("../escape"),
            vec![],
            mode
        )
        .is_err()
    );
    assert_eq!(fs::read(temp.path().join("doc")).unwrap(), b"input");
}

#[test]
fn explicit_root_conversions_preserve_files_and_reject_discarding_extra_nodes() {
    use graphit_core::artifact_store::{PayloadOverlay, ValidatedPayloadPlan};
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("doc"), b"in").unwrap();
    let file = SourceSnapshot::capture(
        &temp.path().join("doc"),
        NodeKind::File,
        &policy(&[""], 2),
        &context(),
    )
    .unwrap();
    let mode = file.nodes().next().unwrap().mode();
    let directory_policy = output_policy(&file, NodeKind::Directory);
    let overlay = PayloadOverlay::new(
        PathBuf::new(),
        PathBuf::from("native"),
        b"out".to_vec(),
        mode,
    )
    .unwrap();
    let dir = ValidatedPayloadPlan::new(file, [overlay], &directory_policy, &context()).unwrap();
    assert_eq!(dir.nodes().len(), 2);
    assert_eq!(dir.kind(), NodeKind::Directory);
    let snapshot = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&["doc"], 2),
        &context(),
    )
    .unwrap();
    let file_policy = graphit_core::artifact_store::PayloadPolicy::new(
        NodeKind::File,
        mode,
        CopyLimits::new(16, 4, 128, 2).unwrap(),
    );
    let overlay = || {
        PayloadOverlay::new(PathBuf::from("doc"), PathBuf::new(), b"out".to_vec(), mode).unwrap()
    };
    let file = ValidatedPayloadPlan::new(snapshot, [overlay()], &file_policy, &context()).unwrap();
    assert_eq!(file.nodes().len(), 1);
    assert_eq!(file.kind(), NodeKind::File);
    fs::create_dir(temp.path().join("empty")).unwrap();
    let snapshot = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&["doc"], 2),
        &context(),
    )
    .unwrap();
    assert!(ValidatedPayloadPlan::new(snapshot, [overlay()], &file_policy, &context()).is_err());
}

#[test]
fn relative_constructors_use_canonical_metadata_quotas_and_reject_alias_duplicates() {
    use graphit_core::artifact_store::{
        GeneratedNode, PayloadOverlay, PayloadPolicy, ValidatedPayloadPlan,
    };
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("d")).unwrap();
    fs::write(temp.path().join("d/f"), b"in").unwrap();
    let unit = if cfg!(windows) { 2 } else { 1 };
    let retention = RetainedBytesLimits::new(2, 1, 3 * unit).unwrap();
    assert!(
        RetainedFileSet::new(
            [PathBuf::from("d/./f")],
            RetainedBytesLimits::new(2, 1, 32).unwrap()
        )
        .is_err()
    );
    let selected = RetainedFileSet::new([PathBuf::from("d").join("f")], retention).unwrap();
    assert!(
        RetainedFileSet::new(
            [PathBuf::from("d//f")],
            RetainedBytesLimits::new(2, 1, 3 * unit - 1).unwrap()
        )
        .is_err()
    );
    assert!(
        RetainedFileSet::new(
            [PathBuf::from("d/f"), PathBuf::from("d/./f")],
            RetainedBytesLimits::new(4, 2, 32).unwrap()
        )
        .is_err()
    );
    let input = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(16, 3, 4 * unit, 2).unwrap(),
        selected,
        retention,
    )
    .unwrap();
    let snapshot =
        SourceSnapshot::capture(temp.path(), NodeKind::Directory, &input, &context()).unwrap();

    let mode = snapshot.nodes().next().unwrap().mode();
    let output = PayloadPolicy::new(
        NodeKind::Directory,
        mode,
        CopyLimits::new(16, 3, 4 * unit, 2).unwrap(),
    );
    let overlay = |original: &str, destination: &str| {
        PayloadOverlay::new(
            PathBuf::from(original),
            PathBuf::from(destination),
            b"out".to_vec(),
            mode,
        )
        .unwrap()
    };
    let canonical = ValidatedPayloadPlan::new(
        snapshot.clone(),
        [overlay("d/f", "d/g")],
        &output,
        &context(),
    )
    .unwrap();
    let normalized = ValidatedPayloadPlan::new(
        snapshot.clone(),
        [overlay("d/./f", "d//g")],
        &output,
        &context(),
    )
    .unwrap();
    assert_eq!(canonical.fingerprint(), normalized.fingerprint());
    assert!(
        normalized
            .nodes()
            .any(|n| n.relative().as_os_str() == Path::new("d").join("g").as_os_str())
    );
    let duplicates = PayloadPolicy::new(
        NodeKind::Directory,
        mode,
        CopyLimits::new(16, 4, 128, 2).unwrap(),
    );
    assert!(
        ValidatedPayloadPlan::new(
            snapshot,
            [overlay("d/f", "d/g"), overlay("d/./f", "d/h")],
            &duplicates,
            &context()
        )
        .is_err()
    );
    assert!(
        ValidatedPayloadPlan::generated(
            [
                GeneratedNode::directory(PathBuf::new(), mode).unwrap(),
                GeneratedNode::directory(PathBuf::from("d"), mode).unwrap(),
                GeneratedNode::file(PathBuf::from("d/f"), vec![1], mode).unwrap(),
                GeneratedNode::file(PathBuf::from("d/./f"), vec![2], mode).unwrap(),
            ],
            &duplicates,
            &context()
        )
        .is_err()
    );
}

#[test]
#[cfg(any(unix, windows))]
fn canonical_relative_names_preserve_unicode_and_reject_non_unicode() {
    use graphit_core::artifact_store::{
        GeneratedNode, PayloadMode, PayloadPolicy, ValidatedPayloadPlan,
    };
    let pairs = ["é", "😀"].into_iter().map(|name| {
        let canonical = PathBuf::from("d").join(name);
        let native_bytes = if cfg!(windows) {
            (2 + name.encode_utf16().count()) * 2
        } else {
            2 + name.len()
        };
        (
            PathBuf::from(format!("d/./{name}")),
            canonical,
            native_bytes,
        )
    });
    let temp = tempfile::tempdir().unwrap();
    let mode =
        PayloadMode::from_permissions(&fs::metadata(temp.path()).unwrap().permissions()).unwrap();
    for (alias, canonical, units) in pairs {
        assert!(
            RetainedFileSet::new(
                [alias.clone()],
                RetainedBytesLimits::new(1, 1, units + 16).unwrap()
            )
            .is_err()
        );
        assert!(
            RetainedFileSet::new(
                [canonical.clone()],
                RetainedBytesLimits::new(1, 1, units).unwrap()
            )
            .is_ok()
        );
        assert_eq!(
            RetainedFileSet::new(
                [canonical.clone()],
                RetainedBytesLimits::new(1, 1, units - 1).unwrap()
            )
            .err()
            .unwrap()
            .kind(),
            io::ErrorKind::FileTooLarge
        );
        let policy = PayloadPolicy::new(
            NodeKind::Directory,
            mode,
            CopyLimits::new(1, 3, units + if cfg!(windows) { 2 } else { 1 }, 2).unwrap(),
        );
        let plan = |path: PathBuf| {
            ValidatedPayloadPlan::generated(
                [
                    GeneratedNode::directory(PathBuf::new(), mode).unwrap(),
                    GeneratedNode::directory(PathBuf::from("d"), mode).unwrap(),
                    GeneratedNode::file(path, vec![42], mode).unwrap(),
                ],
                &policy,
                &context(),
            )
            .unwrap()
        };
        let normalized = plan(alias);
        assert_eq!(
            normalized.fingerprint(),
            plan(canonical.clone()).fingerprint()
        );
        assert_eq!(
            normalized.nodes().last().unwrap().relative().as_os_str(),
            canonical.as_os_str()
        );
    }
    #[cfg(unix)]
    let invalid = {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        PathBuf::from(OsString::from_vec(b"d/\xff".to_vec()))
    };
    #[cfg(windows)]
    let invalid = {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        PathBuf::from("d").join(OsString::from_wide(&[0xd800]))
    };
    assert!(GeneratedNode::file(invalid.clone(), vec![42], mode).is_err());
    assert!(RetainedFileSet::new([invalid], RetainedBytesLimits::new(1, 1, 64).unwrap()).is_err());
}

#[test]
fn relative_raw_native_cap_and_invalid_syntax_are_checked_before_normalization() {
    use graphit_core::artifact_store::{GeneratedNode, PayloadMode, PayloadOverlay};
    let temp = tempfile::tempdir().unwrap();
    let mode =
        PayloadMode::from_permissions(&fs::metadata(temp.path()).unwrap().permissions()).unwrap();
    let units = 64 * 1024 / if cfg!(windows) { 2 } else { 1 };
    let at_cap = format!("d/{}f/", "./".repeat((units - 4) / 2));
    assert_eq!(at_cap.len(), units);
    assert!(GeneratedNode::file(PathBuf::from(&at_cap), vec![], mode).is_ok());
    assert!(
        RetainedFileSet::new(
            [PathBuf::from(&at_cap)],
            RetainedBytesLimits::new(1, 1, 128 * 1024).unwrap()
        )
        .is_err()
    );
    for path in [
        format!("{at_cap}/"),
        "./d/f".into(),
        "../d/f".into(),
        "d/../f".into(),
        "/d/f".into(),
        "d/\0f".into(),
        ".".into(),
    ]
    .into_iter()
    .map(PathBuf::from)
    .chain(std::iter::once(temp.path().to_path_buf()))
    {
        assert!(GeneratedNode::file(path.clone(), vec![], mode).is_err());
        assert!(GeneratedNode::directory(path.clone(), mode).is_err());
        assert!(
            RetainedFileSet::new(
                [path.clone()],
                RetainedBytesLimits::new(1, 1, 128 * 1024).unwrap()
            )
            .is_err()
        );
        assert!(PayloadOverlay::new(path.clone(), PathBuf::from("f"), vec![], mode).is_err());
        assert!(PayloadOverlay::new(PathBuf::from("f"), path, vec![], mode).is_err());
    }
}

#[test]
fn restored_snapshot_relatives_must_have_canonical_native_spelling() {
    use graphit_core::artifact_store::SnapshotBindingRecord;
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("d")).unwrap();
    fs::write(temp.path().join("d/f"), b"in").unwrap();
    let snapshot = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&[], 1),
        &context(),
    )
    .unwrap();
    let original = serde_json::to_value(snapshot.binding().unwrap()).unwrap();
    let limits = CopyLimits::new(16, 3, 128, 2).unwrap();
    for spelling in ["d/./f", "d//f", "d/f/"] {
        let mut malformed = original.clone();
        malformed["nodes"][2]["relative"] = serde_json::json!(spelling);
        let bytes = serde_json::to_vec(&malformed).unwrap();
        assert_eq!(
            SnapshotBindingRecord::pure_restore(&bytes, limits, 32 * 1024)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(temp.path().join("d/f")).unwrap(), b"in");
    }
    let canonical = serde_json::to_vec(&original).unwrap();
    assert!(
        SnapshotBindingRecord::pure_restore(&canonical, limits, canonical.len())
            .unwrap()
            .matches_snapshot(&snapshot)
    );
}

#[test]
fn equivalent_generated_relative_spellings_have_one_logical_fingerprint() {
    use graphit_core::artifact_store::{
        GeneratedNode, PayloadMode, PayloadPolicy, ValidatedPayloadPlan,
    };
    let temp = tempfile::tempdir().unwrap();
    let mode =
        PayloadMode::from_permissions(&fs::metadata(temp.path()).unwrap().permissions()).unwrap();
    let policy = PayloadPolicy::new(
        NodeKind::Directory,
        mode,
        CopyLimits::new(16, 4, 128, 2).unwrap(),
    );
    let plan = |relative: &str| {
        ValidatedPayloadPlan::generated(
            [
                GeneratedNode::directory(PathBuf::new(), mode).unwrap(),
                GeneratedNode::directory(PathBuf::from("d"), mode).unwrap(),
                GeneratedNode::directory(PathBuf::from("empty"), mode).unwrap(),
                GeneratedNode::file(PathBuf::from(relative), b"payload".to_vec(), mode).unwrap(),
            ],
            &policy,
            &context(),
        )
        .unwrap()
    };
    let canonical = plan("d/f");
    for spelling in ["d/./f", "d//f", "d/././f"] {
        let equivalent = plan(spelling);
        assert_eq!(
            equivalent.fingerprint(),
            canonical.fingerprint(),
            "{spelling}"
        );
        assert_eq!(
            equivalent
                .nodes()
                .map(|n| n.relative().to_path_buf())
                .collect::<Vec<_>>(),
            canonical
                .nodes()
                .map(|n| n.relative().to_path_buf())
                .collect::<Vec<_>>(),
        );
    }
}

#[test]
fn generated_trees_are_closed_bounded_data_with_the_same_logical_fingerprint() {
    use graphit_core::artifact_store::{GeneratedNode, PayloadPolicy, ValidatedPayloadPlan};
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("file"), b"in").unwrap();
    let snapshot = SourceSnapshot::capture(
        temp.path(),
        NodeKind::Directory,
        &policy(&[], 1),
        &context(),
    )
    .unwrap();
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let file_mode = snapshot
        .nodes()
        .find(|n| n.relative() == Path::new("file"))
        .unwrap()
        .mode();
    let policy = PayloadPolicy::new(
        NodeKind::Directory,
        root_mode,
        CopyLimits::new(2, 2, if cfg!(windows) { 8 } else { 4 }, 1).unwrap(),
    );
    let nodes = || {
        [
            GeneratedNode::directory(PathBuf::new(), root_mode).unwrap(),
            GeneratedNode::file(PathBuf::from("file"), b"in".to_vec(), file_mode).unwrap(),
        ]
    };
    let plan = ValidatedPayloadPlan::generated(nodes(), &policy, &context()).unwrap();
    assert_eq!(plan.fingerprint(), snapshot.fingerprint());
    for limits in [
        CopyLimits::new(1, 2, 16, 1).unwrap(),
        CopyLimits::new(2, 1, 16, 1).unwrap(),
        CopyLimits::new(2, 2, 1, 1).unwrap(),
    ] {
        let p = PayloadPolicy::new(NodeKind::Directory, root_mode, limits);
        assert_eq!(
            ValidatedPayloadPlan::generated(nodes(), &p, &context())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(1), cancelled);
    assert_eq!(
        ValidatedPayloadPlan::generated(nodes(), &policy, &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert!(GeneratedNode::file(PathBuf::from("../outside"), vec![], file_mode).is_err());
}

#[test]
fn concurrent_captures_share_context_without_competing_for_quota() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("file");
    fs::write(&source, b"four").unwrap();
    let ctx = context();
    let policy = policy(&[], 1);
    let barrier = std::sync::Barrier::new(3);
    let results = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            SourceSnapshot::capture(&source, NodeKind::File, &policy, &ctx)
        });
        let b = scope.spawn(|| {
            barrier.wait();
            SourceSnapshot::capture(&source, NodeKind::File, &policy, &ctx)
        });
        barrier.wait();
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 2);
}

#[test]
fn source_replacement_and_tree_shape_changes_are_rejected_without_outputs() {
    for replacement in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("tree");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("doc"), b"same").unwrap();
        let snapshot = SourceSnapshot::capture(
            &source,
            NodeKind::Directory,
            &policy(&["doc"], 4),
            &context(),
        )
        .unwrap();
        if replacement {
            fs::rename(source.join("doc"), temp.path().join("old")).unwrap();
            fs::write(source.join("doc"), b"same").unwrap();
        } else {
            fs::create_dir(source.join("extra-empty")).unwrap();
        }
        assert_eq!(
            snapshot.revalidate_content(&context()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(source.join("doc")).unwrap(), b"same");
        assert!(
            fs::read_dir(temp.path()).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("journal"))
        );
    }
}

#[cfg(unix)]
#[test]
fn modes_and_followed_file_link_bindings_cannot_change_after_capture() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let tree = temp.path().join("tree");
    fs::create_dir(&tree).unwrap();
    fs::write(temp.path().join("a"), b"same").unwrap();
    fs::write(temp.path().join("b"), b"same").unwrap();
    symlink("../a", tree.join("doc")).unwrap();
    let retention = RetainedBytesLimits::new(4, 1, 16).unwrap();
    let capture_policy = SnapshotPolicy::new(
        CopyPolicy::FollowRegularFiles,
        CopyLimits::new(4, 2, 16, 1).unwrap(),
        RetainedFileSet::new([PathBuf::from("doc")], retention).unwrap(),
        retention,
    )
    .unwrap();
    let snapshot =
        SourceSnapshot::capture(&tree, NodeKind::Directory, &capture_policy, &context()).unwrap();
    fs::remove_file(tree.join("doc")).unwrap();
    symlink("../b", tree.join("doc")).unwrap();
    assert_eq!(
        snapshot.revalidate_content(&context()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    let file = temp.path().join("a");
    let snapshot =
        SourceSnapshot::capture(&file, NodeKind::File, &policy(&[], 1), &context()).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(
        snapshot.revalidate_content(&context()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o444
    );
}
