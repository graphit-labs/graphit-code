use super::*;
use crate::file_lock::Cancellation;
use std::fs;
fn context() -> SnapshotContext {
    SnapshotContext::new(
        std::time::Instant::now() + std::time::Duration::from_secs(10),
        Cancellation::default(),
    )
}
fn fixture(ctx: &SnapshotContext) -> (tempfile::TempDir, ValidatedPayloadPlan) {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("doc"), b"in").unwrap();
    fs::write(source.path().join("asset"), b"12345678").unwrap();
    fs::create_dir(source.path().join("empty")).unwrap();
    let copy = CopyLimits::new(32, 16).unwrap();
    let retention = super::super::RetainedBytesLimits::new(2, 2).unwrap();
    let selected = super::super::RetainedFileSet::new([PathBuf::from("doc")], retention).unwrap();
    let input = super::super::SnapshotPolicy::new(
        super::super::CopyPolicy::FollowLinks,
        copy,
        selected,
        retention,
    )
    .unwrap();
    let snapshot =
        SourceSnapshot::capture(source.path(), NodeKind::Directory, &input, ctx).unwrap();
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let mode = snapshot
        .nodes()
        .find(|n| n.relative() == Path::new("doc"))
        .unwrap()
        .mode();
    let policy = PayloadPolicy::new(NodeKind::Directory, root_mode, copy);
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [PayloadOverlay::new("doc".into(), "native".into(), b"out".to_vec(), mode).unwrap()],
        &policy,
        ctx,
    )
    .unwrap();
    (source, plan)
}
#[test]
fn writes_overlays_assets_and_empty_directories_without_changing_source() {
    let ctx = context();
    let (source, plan) = fixture(&ctx);
    let output = tempfile::tempdir().unwrap();
    let target = output.path().join("payload");
    plan.write_to(&target, &ctx).unwrap();
    assert_eq!(fs::read(target.join("native")).unwrap(), b"out");
    assert!(!target.join("doc").exists());
    assert_eq!(fs::read(target.join("asset")).unwrap(), b"12345678");
    assert!(target.join("empty").is_dir());
    assert_eq!(fs::read(source.path().join("doc")).unwrap(), b"in");
    assert_eq!(
        plan.source_snapshot().unwrap().retained(Path::new("doc")),
        Some(b"in".as_slice())
    );
}
#[test]
fn changed_asset_content_is_rejected_during_actual_copy() {
    let ctx = context();
    let (source, plan) = fixture(&ctx);
    fs::write(source.path().join("asset"), b"modified").unwrap();
    let output = tempfile::tempdir().unwrap();
    assert_eq!(
        plan.write_to(&output.path().join("payload"), &ctx)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(source.path().join("asset")).unwrap(), b"modified");
}
#[test]
fn unrelated_source_changes_and_directory_metadata_do_not_invalidate_output() {
    let ctx = context();
    let (source, plan) = fixture(&ctx);
    fs::write(source.path().join("unrelated"), b"new").unwrap();
    fs::write(source.path().join("doc"), b"new original").unwrap();
    let output = tempfile::tempdir().unwrap();
    let target = output.path().join("payload");
    plan.write_to(&target, &ctx).unwrap();
    assert_eq!(fs::read(target.join("native")).unwrap(), b"out");
    assert!(!target.join("unrelated").exists());
}
#[test]
fn generated_paths_are_validated_at_the_external_entry_boundary() {
    for path in ["../outside", "/absolute", "nested/../../outside"] {
        assert!(
            GeneratedNode::file(
                PathBuf::from(path),
                b"x".to_vec(),
                PayloadMode::private_file()
            )
            .is_err()
        );
    }
    #[cfg(windows)]
    for path in [
        r"C:\outside",
        r"C:outside",
        r"\rooted",
        r"\\server\share\file",
    ] {
        assert!(
            GeneratedNode::file(path.into(), b"x".to_vec(), PayloadMode::private_file()).is_err()
        );
    }
    #[cfg(unix)]
    {
        let literal = GeneratedNode::file(
            r"literal\name.md".into(),
            b"x".to_vec(),
            PayloadMode::private_file(),
        )
        .unwrap();
        assert_eq!(literal.relative.as_os_str(), r"literal\name.md");
    }
    let ctx = context();
    let limits = CopyLimits::new(64, 16).unwrap();
    let policy = PayloadPolicy::new(
        NodeKind::Directory,
        PayloadMode::private_directory(),
        limits,
    );
    let plan = ValidatedPayloadPlan::generated(
        [
            GeneratedNode::directory(PathBuf::new(), PayloadMode::private_directory()).unwrap(),
            GeneratedNode::file(
                "ação.txt".into(),
                b"unicode name".to_vec(),
                PayloadMode::private_file(),
            )
            .unwrap(),
        ],
        &policy,
        &ctx,
    )
    .unwrap();
    let output = tempfile::tempdir().unwrap();
    plan.write_to(&output.path().join("payload"), &ctx).unwrap();
    assert_eq!(
        fs::read(output.path().join("payload/ação.txt")).unwrap(),
        b"unicode name"
    );
}
#[test]
fn generated_tree_rejects_missing_parents_duplicates_and_kind_mismatch() {
    let ctx = context();
    let limits = CopyLimits::new(64, 16).unwrap();
    let policy = PayloadPolicy::new(
        NodeKind::Directory,
        PayloadMode::private_directory(),
        limits,
    );
    assert!(
        ValidatedPayloadPlan::generated(
            [GeneratedNode::file(
                "no-parent/file".into(),
                b"x".to_vec(),
                PayloadMode::private_file()
            )
            .unwrap()],
            &policy,
            &ctx
        )
        .is_err()
    );
    let nodes = [
        GeneratedNode::directory(PathBuf::new(), PayloadMode::private_directory()).unwrap(),
        GeneratedNode::file("same".into(), b"x".to_vec(), PayloadMode::private_file()).unwrap(),
        GeneratedNode::file("same".into(), b"y".to_vec(), PayloadMode::private_file()).unwrap(),
    ];
    assert!(ValidatedPayloadPlan::generated(nodes, &policy, &ctx).is_err());
}
#[test]
fn cancellation_stops_materialization_without_modifying_existing_output() {
    let ctx = context();
    let (_source, plan) = fixture(&ctx);
    ctx.cancellation().cancel();
    let output = tempfile::tempdir().unwrap();
    let target = output.path().join("payload");
    assert_eq!(
        plan.write_to(&target, &ctx).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!target.exists());
}
#[test]
fn selected_output_rebases_file_without_extra_copies() {
    let ctx = context();
    let (_source, plan) = fixture(&ctx);
    let selection = super::super::PayloadSelection::new("native".into(), NodeKind::File).unwrap();
    let selected = plan.selected(&selection, &ctx).unwrap();
    let output = tempfile::tempdir().unwrap();
    selected
        .write_to(&output.path().join("single"), &ctx)
        .unwrap();
    assert_eq!(fs::read(output.path().join("single")).unwrap(), b"out");
}
