//! Finite physical route probes and reserved final reads with actual Core publication.
use graphit_core::{
    artifact_store::{
        ArtifactKey, ArtifactLimits, ArtifactStore, CapturedPublicationScope, CopyLimits,
        NativeDocumentOperationId, NativeDocumentOutcome, NativeDocumentPhase,
        NativeDocumentRecovery, NativeDocumentRecoveryRequest, ScopeLimits,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_identity::InstallationId,
    native_document_file::{
        CapturedDocumentDirectory, NativeDocumentFileLimits, NativeDocumentNameLimits,
        NativeDocumentRouteProbeLimits, NativeDocumentRouteRelation as Relation,
    },
    project_paths::InstallationPaths,
};
use std::{
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn store(root: &Path) -> ArtifactStore {
    let installation =
        InstallationPaths::resolve(Brand::default(), root, None, Some(&root.join("owner")))
            .unwrap();
    let scope = CapturedPublicationScope::installation(
        &installation,
        InstallationId::new("terminal-fixture").unwrap(),
        [root.to_owned()],
        ScopeLimits::new(4, 65536, 65536).unwrap(),
        &native_capture_request(),
    )
    .unwrap();
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 64).unwrap();
    ArtifactStore::new(scope, ArtifactLimits::new(1024 * 1024, 1024, copy).unwrap())
}
fn context(cancel: Cancellation) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancel,
    ))
}
fn limits() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(1024, 65536, 256, 65536).unwrap()
}
fn names() -> NativeDocumentNameLimits {
    NativeDocumentNameLimits::new(32, 65536).unwrap()
}
fn operation(root: &Path) -> NativeDocumentOperationId {
    let id = NativeDocumentOperationId::generate();
    let mut file = fs::File::create(root.join("caller-operation.json")).unwrap();
    write!(file, "{id}").unwrap();
    file.sync_all().unwrap();
    id
}

fn probes(n: usize) -> NativeDocumentRouteProbeLimits {
    NativeDocumentRouteProbeLimits::new(n.max(1), 65536, (2 * n + 2) * 65536).unwrap()
}

#[test]
fn receipt_identity_distinguishes_in_place_edit_from_equal_bytes_replacement() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("target");
    let distinct = tmp.path().join("distinct");
    fs::write(&target, b"{\"owned\":1,\"foreign\":1}").unwrap();
    let cancel = Cancellation::default();
    let ctx = context(cancel.clone());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();
    let original = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
    let original = original.observation().receipt().unwrap().clone();

    // Truncating this existing file preserves its native file identity.
    let edited_bytes = b"{\"owned\":1,\"foreign\":2}";
    fs::write(&target, edited_bytes).unwrap();
    let edited = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
    let edited = edited.observation().receipt().unwrap().clone();
    assert!(original != edited);
    assert!(original.same_file_as(&edited));

    // Keep both files alive, so their IDs cannot be immediately recycled.
    fs::write(&distinct, edited_bytes).unwrap();
    let other = parent
        .read_named("distinct".as_ref(), names(), &ctx)
        .unwrap();
    let other = other.observation().receipt().unwrap().clone();
    assert!(!edited.same_file_as(&other));
    fs::rename(&target, tmp.path().join("retained-original")).unwrap();
    fs::rename(&distinct, &target).unwrap();
    let replaced = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
    let replaced = replaced.observation().receipt().unwrap().clone();
    assert!(!original.same_file_as(&replaced));
    assert!(other.same_file_as(&replaced));
    assert_eq!(fs::read(&target).unwrap(), edited_bytes);
    assert_eq!(
        fs::read(tmp.path().join("retained-original")).unwrap(),
        edited_bytes
    );

    cancel.cancel();
    assert!(original.same_file_as(&edited));
    assert!(!edited.same_file_as(&replaced));
}

#[test]
fn ordered_duplicates_missing_and_distinct_preserve_bytes_without_body_io() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("target"), b"same").unwrap();
    fs::write(tmp.path().join("distinct"), b"same").unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();
    let routes = [
        "target".as_ref(),
        "missing".as_ref(),
        "distinct".as_ref(),
        "target".as_ref(),
    ];
    let batch = parent
        .prepare_route_probes("target".as_ref(), &routes, probes(4), &ctx)
        .unwrap();
    assert_eq!(batch.target_name().as_os_str(), "target");

    let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
    let result = batch.probe(&expected).unwrap();
    let mut iter = result.iter();
    assert_eq!(iter.len(), 4);
    for (name, relation) in [
        ("target", Relation::Same),
        ("missing", Relation::Missing),
        ("distinct", Relation::Distinct),
        ("target", Relation::Same),
    ] {
        let (actual_name, actual_relation) = iter.next().unwrap();
        assert_eq!(actual_name.as_os_str(), name);
        assert!(actual_relation == relation);
    }
    assert_eq!(iter.len(), 0);

    for name in ["target", "distinct"] {
        assert_eq!(fs::read(tmp.path().join(name)).unwrap(), b"same");
    }
}

#[test]
fn distinct_readonly_oversized_document_does_not_need_target_body_or_access_limits() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("target"), b"x").unwrap();
    let other = tmp.path().join("other");
    fs::write(&other, vec![42; 8192]).unwrap();
    let original_permissions = fs::metadata(&other).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&other, permissions).unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(
        tmp.path(),
        NativeDocumentFileLimits::new(1, 65536, 256, 65536).unwrap(),
        &ctx,
    )
    .unwrap();
    let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
    let result = parent
        .prepare_route_probes("target".as_ref(), &["other".as_ref()], probes(1), &ctx)
        .unwrap()
        .probe(&expected)
        .unwrap();
    assert!(result.iter().next().unwrap().1 == Relation::Distinct);

    assert_eq!(fs::read(&other).unwrap(), vec![42; 8192]);
    // Restore exactly the fixture's original permissions for native cleanup.
    fs::set_permissions(other, original_permissions).unwrap();
}

#[cfg(unix)]
#[test]
fn inaccessible_last_route_propagates_native_permission_denied_without_partial_results() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("target"), b"x").unwrap();
    let denied = tmp.path().join("denied");
    fs::write(&denied, b"inaccessible").unwrap();
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o0)).unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();
    let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
    let result = parent
        .prepare_route_probes(
            "target".as_ref(),
            &["target".as_ref(), "denied".as_ref()],
            probes(2),
            &ctx,
        )
        .unwrap()
        .probe(&expected);
    match fs::File::open(&denied) {
        Err(error) => {
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(
                result.err().unwrap().kind(),
                io::ErrorKind::PermissionDenied
            );
            println!("Native denied-route fixture: PermissionDenied enforced; no partial result");
        }
        Ok(_) => {
            // Privileged runners can access mode-000 files; that is not a denial.
            assert!(result.unwrap().iter().nth(1).unwrap().1 == Relation::Distinct);
            println!(
                "Native denied-route fixture: runner bypassed mode-000; native denial unqualified"
            );
        }
    }

    fs::set_permissions(&denied, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(fs::read(denied).unwrap(), b"inaccessible");
}

#[test]
fn exact_and_overflow_admission_is_readonly_and_preserves_raw_components() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();
    let unit = if cfg!(windows) { 2 } else { 1 };
    let exact = NativeDocumentRouteProbeLimits::new(1, 2 * unit, 4 * 65536).unwrap();
    let batch = parent
        .prepare_route_probes("t".as_ref(), &["r".as_ref()], exact, &ctx)
        .unwrap();
    assert_eq!(batch.target_name().as_os_str(), "t");
    drop(batch);
    for (routes, name_bytes, access_bytes) in
        [(1, 2 * unit - 1, 4 * 65536), (1, 2 * unit, 4 * 65536 - 1)]
    {
        let bound = NativeDocumentRouteProbeLimits::new(routes, name_bytes, access_bytes).unwrap();
        assert_eq!(
            parent
                .prepare_route_probes("t".as_ref(), &["r".as_ref()], bound, &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::FileTooLarge
        );
    }
    assert_eq!(
        parent
            .prepare_route_probes("t".as_ref(), &["r".as_ref(), "s".as_ref()], exact, &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    for invalid in ["", ".", "..", "a/b", "a\0b"] {
        assert_eq!(
            parent
                .prepare_route_probes("t".as_ref(), &[invalid.as_ref()], probes(1), &ctx)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    for (a, b, c) in [
        (0, 1, 1),
        (1, 0, 1),
        (1, 1, 0),
        (usize::MAX, 1, 1),
        (1, usize::MAX, 1),
        (1, 1, usize::MAX),
    ] {
        assert_eq!(
            NativeDocumentRouteProbeLimits::new(a, b, c)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
    let huge_parent = CapturedDocumentDirectory::capture(
        tmp.path(),
        NativeDocumentFileLimits::new(1, 65536, 256, usize::MAX).unwrap(),
        &ctx,
    )
    .unwrap();
    assert_eq!(
        huge_parent
            .prepare_route_probes(
                "t".as_ref(),
                &[],
                NativeDocumentRouteProbeLimits::new(1, 1, usize::MAX - 1).unwrap(),
                &ctx
            )
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );

    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
}

#[test]
fn empty_batch_still_qualifies_a_present_target_and_rejects_missing_or_wrong_bindings() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("target"), b"x").unwrap();
    fs::write(tmp.path().join("other"), b"x").unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();
    let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
    assert_eq!(
        parent
            .prepare_route_probes("target".as_ref(), &[], probes(0), &ctx)
            .unwrap()
            .probe(&expected)
            .unwrap()
            .iter()
            .len(),
        0
    );

    assert_eq!(
        parent
            .prepare_route_probes("other".as_ref(), &[], probes(0), &ctx)
            .unwrap()
            .probe(&expected)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let foreign_context = context(Cancellation::default());
    assert_eq!(
        parent
            .prepare_route_probes("target".as_ref(), &[], probes(0), &foreign_context)
            .unwrap()
            .probe(&expected)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let foreign_root = tempfile::tempdir().unwrap();
    fs::write(foreign_root.path().join("target"), b"x").unwrap();
    let foreign_parent =
        CapturedDocumentDirectory::capture(foreign_root.path(), limits(), &ctx).unwrap();
    assert_eq!(
        foreign_parent
            .prepare_route_probes("target".as_ref(), &[], probes(0), &ctx)
            .unwrap()
            .probe(&expected)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let missing = parent
        .read_named("missing".as_ref(), names(), &ctx)
        .unwrap();
    assert_eq!(
        parent
            .prepare_route_probes("missing".as_ref(), &[], probes(0), &ctx)
            .unwrap()
            .probe(&missing)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn named_revalidation_compares_modified_time_even_when_receipt_and_bytes_match() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("target");
    fs::write(&path, b"same").unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();

    let expected = parent
        .read_named_bounded("target".as_ref(), 4, names(), &ctx)
        .unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_modified(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(12345)),
        )
        .unwrap();
    let fresh = context(Cancellation::default());
    let same_receipt = parent
        .read_named("target".as_ref(), names(), &fresh)
        .unwrap();
    assert!(same_receipt.observation().receipt() == expected.observation().receipt());
    assert_eq!(
        parent
            .revalidate_named_observation(&expected, &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );

    assert_eq!(fs::read(path).unwrap(), b"same");
}

#[test]
fn named_observation_rejects_foreign_context_and_parent() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("target"), b"x").unwrap();
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();
    let expected = parent.read_named("target".as_ref(), names(), &ctx).unwrap();
    let foreign = context(Cancellation::default());
    assert_eq!(
        parent
            .revalidate_named_observation(&expected, &foreign)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let other = tempfile::tempdir().unwrap();
    let other_parent = CapturedDocumentDirectory::capture(other.path(), limits(), &ctx).unwrap();
    assert_eq!(
        other_parent
            .revalidate_named_observation(&expected, &ctx)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn named_revalidation_preserves_empty_missing_changed_bytes_and_foreign_identity() {
    for mode in ["empty", "missing", "changed", "foreign"] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("target");
        if mode != "missing" {
            fs::write(
                &path,
                if mode == "empty" {
                    b"".as_slice()
                } else {
                    b"same".as_slice()
                },
            )
            .unwrap();
        }
        let cap = if mode == "empty" || mode == "missing" {
            0
        } else {
            4
        };
        let ctx = context(Cancellation::default());
        let parent = CapturedDocumentDirectory::capture(tmp.path(), limits(), &ctx).unwrap();

        let expected = parent
            .read_named_bounded("target".as_ref(), cap, names(), &ctx)
            .unwrap();
        if mode == "changed" {
            fs::write(&path, b"edit").unwrap();
        }
        if mode == "foreign" {
            fs::rename(&path, tmp.path().join("historical")).unwrap();
            fs::write(&path, b"same").unwrap();
        }
        let result = parent.revalidate_named_observation(&expected, &ctx);
        if mode == "changed" || mode == "foreign" {
            assert_eq!(result.err().unwrap().kind(), io::ErrorKind::InvalidData);
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn route_probe_access_bound_fails_before_core_intent_or_file_creation() {
    let tmp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    fs::create_dir(root.join("owner")).unwrap();
    let s = store(&root);
    let ctx = context(Cancellation::default());
    let parent = CapturedDocumentDirectory::capture(&root, limits(), &ctx).unwrap();
    assert_eq!(
        parent
            .prepare_route_probes(
                "agent.json".as_ref(),
                &["agent.json".as_ref()],
                NativeDocumentRouteProbeLimits::new(1, 65536, 4 * 65536 - 1).unwrap(),
                &ctx
            )
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
    assert!(s.load().unwrap().is_none());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
}

#[test]
fn core_applied_routes_and_final_revalidation_gate_ack_and_retain_pending_on_failure() {
    for mode in [
        "success",
        "batch-cancel",
        "batch-invalid",
        "final-cancel",
        "final-change",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(tmp.path()).unwrap();
        fs::create_dir(root.join("owner")).unwrap();
        let s = store(&root);
        let cancel = Cancellation::default();
        let ctx = context(cancel.clone());
        let parent = CapturedDocumentDirectory::capture(&root, limits(), &ctx).unwrap();
        let old = parent.read("agent.json".as_ref(), &ctx).unwrap();
        let routes = [OsStr::new("agent.json"), OsStr::new("historical")];
        let batch = parent
            .prepare_route_probes("agent.json".as_ref(), &routes, probes(2), &ctx)
            .unwrap();

        let id = operation(&root);
        let key = ArtifactKey::new("native", "agent", "role").unwrap();
        let mut writer = s
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .with_io_context(ctx.clone())
            .unwrap();
        let resolution = writer
            .prepare_native_document(id, &key, &parent, &old, b"new")
            .unwrap_or_else(|f| panic!("{:?}", f.error()))
            .publish()
            .unwrap_or_else(|f| panic!("{:?}", f.error()));
        assert_eq!(resolution.outcome(), NativeDocumentOutcome::Applied);

        let expected = parent
            .read_named_bounded("agent.json".as_ref(), 3, names(), &ctx)
            .unwrap();
        assert!(expected.observation().receipt() == resolution.destination_receipt());
        if mode == "batch-cancel" {
            cancel.cancel();
        }
        if mode == "batch-invalid" {
            fs::create_dir(root.join("historical")).unwrap();
        }
        let result = batch.probe(&expected);
        if mode.starts_with("batch-") {
            assert_eq!(
                result.err().unwrap().kind(),
                if mode == "batch-cancel" {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::InvalidData
                }
            );
        } else {
            let result = result.unwrap();
            assert_eq!(result.iter().len(), 2);
            assert!(result.iter().next().unwrap().1 == Relation::Same);
            assert!(result.iter().nth(1).unwrap().1 == Relation::Missing);

            if mode == "final-cancel" {
                cancel.cancel();
            }
            if mode == "final-change" {
                fs::write(root.join("agent.json"), b"bad").unwrap();
            }
            let final_result = parent.revalidate_named_observation(&expected, &ctx);
            if mode == "success" {
                final_result.unwrap();

                // Minimal synced caller phase markers; not a CommonHub journal codec.
                let marker = root.join("caller-operation.json");
                fs::write(&marker, format!("{id}:logical-qualified")).unwrap();
                fs::File::options()
                    .write(true)
                    .open(&marker)
                    .unwrap()
                    .sync_all()
                    .unwrap();
                let completion = resolution.acknowledge().unwrap();
                fs::write(&marker, format!("{id}:released")).unwrap();
                fs::File::options()
                    .write(true)
                    .open(&marker)
                    .unwrap()
                    .sync_all()
                    .unwrap();
                completion.release().unwrap();
                assert!(s.load().unwrap().unwrap().pending().is_none());
                assert_eq!(fs::read(root.join("agent.json")).unwrap(), b"new");
                continue;
            }
            assert_eq!(
                final_result.err().unwrap().kind(),
                if mode == "final-cancel" {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::InvalidData
                }
            );
        }

        drop(resolution);
        drop(writer);
        assert!(
            matches!(s.load().unwrap().unwrap().pending(),Some(graphit_core::artifact_store::ArtifactIntent::NativeDocument(p)) if p.phase()==NativeDocumentPhase::Resolved)
        );
        assert!(root.join(format!(".graphit-doc-{id}")).exists());
        let actual_bytes = fs::read(root.join("agent.json")).unwrap();
        assert_eq!(
            actual_bytes,
            if mode == "final-change" {
                b"bad".as_slice()
            } else {
                b"new".as_slice()
            }
        );
        let fresh = context(Cancellation::default());
        let parent = CapturedDocumentDirectory::capture(&root, limits(), &fresh).unwrap();
        let identity = parent.directory_identity();
        let scope = s.scope().binding().unwrap();
        let destination = root.join("agent.json");
        let mut writer = s
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .with_io_context(fresh)
            .unwrap();
        let recovery = writer.recover_native_document(
            NativeDocumentRecoveryRequest {
                operation_id: id,
                key: &key,
                destination: &destination,
                parent_identity: &identity,
                previous: None,
                scope: &scope,
            },
            &parent,
        );
        if mode == "final-change" {
            let error = recovery.err().unwrap();
            assert_eq!(
                error.stage(),
                graphit_core::artifact_store::ArtifactStage::RecoveryConflict
            );
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        } else {
            let NativeDocumentRecovery::Resolution(recovered) = recovery.unwrap() else {
                panic!("expected retained resolution")
            };
            assert_eq!(recovered.outcome(), NativeDocumentOutcome::Applied);
            drop(recovered);
        }
        assert!(s.load().unwrap().unwrap().pending().is_some());
        assert_eq!(fs::read(destination).unwrap(), actual_bytes);
    }
}

fn native_capture_request() -> std::sync::Arc<graphit_core::file_io::SnapshotContext> {
    std::sync::Arc::new(graphit_core::file_io::SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        graphit_core::file_lock::Cancellation::default(),
    ))
}
