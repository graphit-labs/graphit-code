use super::*;
use crate::{
    artifact_store::{CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy},
    file_lock::Cancellation,
};
use std::{
    fs,
    time::{Duration, Instant},
};
use tree::BuildTree;

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(10),
        Cancellation::default(),
    )
}
fn fixture(ctx: &SnapshotContext) -> (tempfile::TempDir, ValidatedPayloadPlan) {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("doc"), b"in").unwrap();
    fs::write(temp.path().join("asset"), b"12345678").unwrap();
    fs::create_dir(temp.path().join("empty")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temp.path().join("asset"), fs::Permissions::from_mode(0o750)).unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o750)).unwrap();
    }
    let copy = CopyLimits::new(16, 8, 128, 2).unwrap();
    let retention = RetainedBytesLimits::new(2, 2, 128).unwrap();
    let input = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([PathBuf::from("doc")], retention).unwrap(),
        retention,
    )
    .unwrap();
    let source = SourceSnapshot::capture(temp.path(), NodeKind::Directory, &input, ctx).unwrap();
    let root_mode = source.nodes().next().unwrap().mode();
    let file_mode = source
        .nodes()
        .find(|n| n.relative() == Path::new("doc"))
        .unwrap()
        .mode();
    let policy = PayloadPolicy::new(NodeKind::Directory, root_mode, copy);
    let overlay = PayloadOverlay::new(
        PathBuf::from("doc"),
        PathBuf::from("native"),
        b"out".to_vec(),
        file_mode,
    )
    .unwrap();
    let plan = ValidatedPayloadPlan::new(source, [overlay], &policy, ctx).unwrap();
    (temp, plan)
}
#[test]
fn private_builder_verifies_all_three_passes_and_applies_overlay_on_first_write() {
    let ctx = context();
    let (source, plan) = fixture(&ctx);
    plan.revalidate_content(&ctx).unwrap();
    // Capture retains only the two input bytes; replacement may have three.
    assert_eq!(
        plan.source_snapshot().unwrap().retained(Path::new("doc")),
        Some(b"in".as_slice())
    );
    let target = tempfile::tempdir().unwrap();
    let output = target.path().join("payload");
    let mut receipt = TreeReceipt { nodes: vec![] };
    plan.build_tree(
        &output,
        ctx.cancellation(),
        Some(&ctx),
        &mut receipt,
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(fs::read(output.join("native")).unwrap(), b"out");
    assert!(!output.join("doc").exists());
    assert_eq!(fs::read(output.join("asset")).unwrap(), b"12345678");
    assert!(output.join("empty").is_dir());
    assert_eq!(fs::read(source.path().join("doc")).unwrap(), b"in");

    assert_eq!(receipt.nodes.len(), 4);
    assert_eq!(
        tree::fingerprint(receipt.nodes.iter().map(|n| (
            n.relative.0.as_path(),
            n.mode,
            &n.content
        ))),
        plan.fingerprint()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(output.join("asset"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o750
        );
    }
}
#[test]
fn source_replacement_is_detected_before_effects() {
    let target = tempfile::tempdir().unwrap();
    let output = target.path().join("payload");
    let mut receipt = TreeReceipt { nodes: vec![] };
    let mut effects = 0;
    let ctx = context();
    let (source, plan) = fixture(&ctx);
    plan.revalidate_content(&ctx).unwrap();
    fs::rename(source.path().join("doc"), source.path().join("old")).unwrap();
    fs::write(source.path().join("doc"), b"in").unwrap();
    assert!(
        plan.build_tree(
            &output,
            ctx.cancellation(),
            Some(&ctx),
            &mut receipt,
            &mut || {
                effects += 1;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(effects, 0);
    assert!(!output.exists());
}
#[test]
fn source_mutation_after_first_effect_never_produces_a_verified_tree() {
    let ctx = context();
    let (source, plan) = fixture(&ctx);
    plan.revalidate_content(&ctx).unwrap();
    let target = tempfile::tempdir().unwrap();
    let output = target.path().join("payload");
    let mut receipt = TreeReceipt { nodes: vec![] };
    let mut effects = 0;
    let result = plan.build_tree(
        &output,
        ctx.cancellation(),
        Some(&ctx),
        &mut receipt,
        &mut || {
            effects += 1;
            if effects == 1 {
                crate::artifact_store::tests::overwrite_with_changed_mtime(
                    &source.path().join("asset"),
                    b"87654321",
                );
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(effects, 1);
    assert_eq!(receipt.nodes.len(), 1);
    assert_eq!(fs::read(source.path().join("doc")).unwrap(), b"in");
    assert!(!output.join("native").exists());
}
#[test]
fn canonical_generated_names_build_the_same_native_tree_and_receipt() {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o751)).unwrap();
    }
    let mode =
        PayloadMode::from_permissions(&fs::metadata(temp.path()).unwrap().permissions()).unwrap();
    let copy = CopyLimits::new(7, 4, 128, 2).unwrap();
    let policy = PayloadPolicy::new(NodeKind::Directory, mode, copy);
    let mut fingerprint = None;
    for (index, relative) in ["d/f", "d/./f", "d//f"].into_iter().enumerate() {
        let ctx = context();
        let plan = ValidatedPayloadPlan::generated(
            [
                GeneratedNode::directory(PathBuf::new(), mode).unwrap(),
                GeneratedNode::directory(PathBuf::from("d//"), mode).unwrap(),
                GeneratedNode::directory(PathBuf::from("empty/./"), mode).unwrap(),
                GeneratedNode::file(PathBuf::from(relative), b"payload".to_vec(), mode).unwrap(),
            ],
            &policy,
            &ctx,
        )
        .unwrap();
        let expected = *fingerprint.get_or_insert(plan.fingerprint());
        assert_eq!(plan.fingerprint(), expected);
        let output = temp.path().join(index.to_string());
        let mut receipt = TreeReceipt { nodes: vec![] };
        plan.build_tree(
            &output,
            ctx.cancellation(),
            Some(&ctx),
            &mut receipt,
            &mut || Ok(()),
        )
        .unwrap();
        receipt.validate(copy, NodeKind::Directory.into()).unwrap();
        assert_eq!(
            tree::fingerprint(receipt.nodes.iter().map(|n| (
                n.relative.0.as_path(),
                n.mode,
                &n.content
            ))),
            expected
        );
        assert_eq!(fs::read(output.join("d/f")).unwrap(), b"payload");
        assert!(output.join("empty").is_dir());
        assert!(receipt.nodes.iter().all(|node| node.mode == mode.0));
        assert_eq!(
            receipt.nodes[2].relative.0.as_os_str(),
            Path::new("d").join("f").as_os_str()
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(output.join("d/f"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o751
            );
        }
    }
}

#[test]
fn generated_build_has_no_source_capture_or_serialized_authority() {
    let temp = tempfile::tempdir().unwrap();
    let mode =
        PayloadMode::from_permissions(&fs::metadata(temp.path()).unwrap().permissions()).unwrap();
    let policy = PayloadPolicy::new(NodeKind::File, mode, CopyLimits::new(3, 1, 1, 1).unwrap());
    let ctx = context();
    let plan = ValidatedPayloadPlan::generated(
        [GeneratedNode::file(PathBuf::new(), b"out".to_vec(), mode).unwrap()],
        &policy,
        &ctx,
    )
    .unwrap();
    plan.revalidate_content(&ctx).unwrap();
    let root = temp.path().join("file");
    let mut receipt = TreeReceipt { nodes: vec![] };
    plan.build_tree(
        &root,
        ctx.cancellation(),
        Some(&ctx),
        &mut receipt,
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(fs::read(root).unwrap(), b"out");
}

#[cfg(unix)]
#[test]
fn final_shape_and_directory_mode_changes_reject_complete_receipts() {
    use std::os::unix::fs::PermissionsExt;
    for shape in [false, true] {
        let ctx = context();
        let (source, plan) = fixture(&ctx);
        plan.revalidate_content(&ctx).unwrap();
        let target = tempfile::tempdir().unwrap();
        let output = target.path().join("payload");
        let mut receipt = TreeReceipt { nodes: vec![] };
        let mut changed = false;
        let result = plan.build_tree(
            &output,
            ctx.cancellation(),
            Some(&ctx),
            &mut receipt,
            &mut || {
                if !changed && fs::metadata(&output)?.permissions().mode() & 0o777 == 0o750 {
                    changed = true;
                    if shape {
                        fs::create_dir(source.path().join("extra"))?;
                    } else {
                        fs::set_permissions(source.path(), fs::Permissions::from_mode(0o700))?;
                    }
                }
                Ok(())
            },
        );
        assert!(changed);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(receipt.nodes.len(), 4);
        assert_eq!(fs::read(output.join("native")).unwrap(), b"out");
    }
}

#[cfg(unix)]
#[test]
fn final_parent_alias_retarget_rejects_same_inode_without_touching_origin_bytes() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("file"), b"same").unwrap();
    fs::set_permissions(a.join("file"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::hard_link(a.join("file"), b.join("file")).unwrap();
    let alias = temp.path().join("alias");
    symlink(&a, &alias).unwrap();
    let ctx = context();
    let retain = RetainedBytesLimits::new(7, 2, 1).unwrap();
    let copy = CopyLimits::new(4, 1, 1, 1).unwrap();
    let capture = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([PathBuf::new()], retain).unwrap(),
        retain,
    )
    .unwrap();
    let snapshot =
        SourceSnapshot::capture(&alias.join("file"), NodeKind::File, &capture, &ctx).unwrap();
    let mode = snapshot.nodes().next().unwrap().mode();
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [PayloadOverlay::new(PathBuf::new(), PathBuf::new(), b"out".to_vec(), mode).unwrap()],
        &PayloadPolicy::new(NodeKind::File, mode, copy),
        &ctx,
    )
    .unwrap();
    plan.revalidate_content(&ctx).unwrap();
    let output = temp.path().join("output");
    let mut receipt = TreeReceipt { nodes: vec![] };
    let mut changed = false;
    let result = plan.build_tree(
        &output,
        ctx.cancellation(),
        Some(&ctx),
        &mut receipt,
        &mut || {
            if !changed && fs::metadata(&output)?.permissions().mode() & 0o777 == 0o644 {
                changed = true;
                fs::remove_file(&alias)?;
                symlink(&b, &alias)?;
            }
            Ok(())
        },
    );
    assert!(changed);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    assert_eq!(receipt.nodes.len(), 1);
    assert_eq!(fs::read(a.join("file")).unwrap(), b"same");
    assert_eq!(fs::read(b.join("file")).unwrap(), b"same");
}

#[test]
fn deadline_during_streaming_stops_before_the_next_block() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("asset");
    let bytes = vec![7; tree::BUFFER_BYTES * 2];
    fs::write(&path, &bytes).unwrap();
    let mut file = fs::File::open(&path).unwrap();
    let deadline = Instant::now() + Duration::from_millis(500);
    let ctx = SnapshotContext::new(deadline, Cancellation::default());
    ctx.check().unwrap();
    let mut buffer = vec![0; tree::BUFFER_BYTES];
    let result = ctx.consume_file(&mut file, bytes.len() as u64, &mut buffer, |_| {
        std::thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
        );
        Ok(())
    });
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);

    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn unselected_large_asset_is_streamed_with_selected_overlay() {
    const ASSET_BYTES: u64 = 8 * 1024 * 1024;
    const OUTPUT_BYTES: u64 = ASSET_BYTES + 3;
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join("doc"), b"in").unwrap();
    fs::File::create(source.path().join("asset"))
        .unwrap()
        .set_len(ASSET_BYTES)
        .unwrap();
    fs::create_dir(source.path().join("empty")).unwrap();
    let output_parent = tempfile::tempdir().unwrap();
    let output = output_parent.path().join("payload");
    let ctx = context();
    let copy = CopyLimits::new(OUTPUT_BYTES, 4, 128, 2).unwrap();
    let retention = RetainedBytesLimits::new(5, 2, 128).unwrap();
    let input = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([PathBuf::from("doc")], retention).unwrap(),
        retention,
    )
    .unwrap();
    let snapshot =
        SourceSnapshot::capture(source.path(), NodeKind::Directory, &input, &ctx).unwrap();
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let file_mode = snapshot
        .nodes()
        .find(|n| n.relative() == Path::new("doc"))
        .unwrap()
        .mode();
    let policy = PayloadPolicy::new(NodeKind::Directory, root_mode, copy);
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [PayloadOverlay::new(
            PathBuf::from("doc"),
            PathBuf::from("native"),
            b"out".to_vec(),
            file_mode,
        )
        .unwrap()],
        &policy,
        &ctx,
    )
    .unwrap();
    plan.revalidate_content(&ctx).unwrap();
    let mut receipt = TreeReceipt { nodes: vec![] };
    plan.build_tree(
        &output,
        ctx.cancellation(),
        Some(&ctx),
        &mut receipt,
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(receipt.nodes.len(), 4);

    assert_eq!(
        fs::metadata(output.join("asset")).unwrap().len(),
        ASSET_BYTES
    );
    assert_eq!(fs::read(output.join("native")).unwrap(), b"out");
    assert_eq!(fs::read(source.path().join("doc")).unwrap(), b"in");
    assert_eq!(
        tree::fingerprint(receipt.nodes.iter().map(|n| (
            n.relative.0.as_path(),
            n.mode,
            &n.content
        ))),
        plan.fingerprint()
    );
}

#[test]
fn selected_output_rebases_file_and_directory_and_preserves_origin_validation() {
    let ctx = context();
    let (source, plan) = fixture(&ctx);
    for (path, kind, expected) in [
        ("native", NodeKind::File, Some(b"out".as_slice())),
        ("asset", NodeKind::File, Some(b"12345678".as_slice())),
        ("empty", NodeKind::Directory, None),
    ] {
        let selection = super::super::PayloadSelection::new(PathBuf::from(path), kind).unwrap();
        let selected = plan.selected(&selection, &ctx).unwrap();
        assert_eq!(selected.kind(), kind);
        assert_eq!(selected.nodes().len(), 1);
        assert_eq!(selected.nodes().next().unwrap().relative(), Path::new(""));
        let target = tempfile::tempdir().unwrap();
        let destination = target.path().join("installed");
        let mut receipt = TreeReceipt { nodes: Vec::new() };
        selected
            .build_tree(
                &destination,
                ctx.cancellation(),
                Some(&ctx),
                &mut receipt,
                &mut || Ok(()),
            )
            .unwrap();
        if let Some(bytes) = expected {
            assert_eq!(fs::read(&destination).unwrap(), bytes);
        } else {
            assert!(destination.is_dir());
        }
    }
    assert!(
        plan.selected(
            &super::super::PayloadSelection::new(PathBuf::from("missing"), NodeKind::File).unwrap(),
            &ctx
        )
        .is_err()
    );
    assert!(
        plan.selected(
            &super::super::PayloadSelection::new(PathBuf::from("empty"), NodeKind::File).unwrap(),
            &ctx
        )
        .is_err()
    );
    let selected = plan
        .selected(
            &super::super::PayloadSelection::new(PathBuf::from("native"), NodeKind::File).unwrap(),
            &ctx,
        )
        .unwrap();
    fs::write(source.path().join("doc"), b"changed").unwrap();
    assert!(selected.revalidate_content(&ctx).is_err());
}
