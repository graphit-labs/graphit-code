use super::*;

fn plan(store: &ArtifactStore, content: &[u8], request: &SnapshotContext) -> ValidatedPayloadPlan {
    let mode = if cfg!(unix) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            PayloadMode::from_permissions(&fs::Permissions::from_mode(0o644)).unwrap()
        }
        #[cfg(not(unix))]
        unreachable!()
    } else {
        PayloadMode::from_permissions(
            &fs::metadata(std::env::current_exe().unwrap())
                .unwrap()
                .permissions(),
        )
        .unwrap()
    };
    ValidatedPayloadPlan::generated(
        [GeneratedNode::file(PathBuf::new(), content.to_vec(), mode).unwrap()],
        &PayloadPolicy::new(NodeKind::File, mode, store.limits.copy),
        request,
    )
    .unwrap()
}

#[test]
fn generated_publication_installs_once_records_origin_hash_and_has_no_second_copy() {
    let f = Fixture::new();
    let request = native_capture_request();
    let plan = plan(&f.store, b"package skill", &request);
    let mut writer = f.store.writer_with_context(&request).unwrap();
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    assert_eq!(
        writer
            .publish_generated(&f.key(), &plan, &f.destination, NodeKind::File, mode)
            .unwrap(),
        ArtifactChange::Created
    );
    assert_eq!(fs::read(&f.destination).unwrap(), b"package skill");
    assert_eq!(
        writer
            .check_generated_publication(&f.key(), &plan, &f.destination, NodeKind::File, mode)
            .unwrap(),
        PublicationCheck::VerifiedUnchanged
    );
    let before = fs::read(f.store.manifest_path()).unwrap();
    assert_eq!(
        writer
            .publish_generated(&f.key(), &plan, &f.destination, NodeKind::File, mode)
            .unwrap(),
        ArtifactChange::Unchanged
    );
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
    let state = writer.load().unwrap().unwrap();
    assert_eq!(
        state.entries()[0].content_fingerprint(),
        Some(plan.fingerprint())
    );
    let origin = state.entries()[0].generated_origin().unwrap();
    assert_eq!(origin.package, "graphit");
    assert_eq!(origin.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        state.entries()[0].source(),
        std::env::current_exe().unwrap()
    );
    let json: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let doc = json.as_object().unwrap();
    assert!(doc.keys().all(|key| !matches!(
        key.as_str(),
        "pins" | "payloads" | "payload_root" | "abandoned_materializations"
    )));
    assert!(!f.store.runtime_root().join("owned-payloads").exists());
    assert!(
        fs::read_dir(f.destination.parent().unwrap())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".graphit-artifact-"))
    );
    drop(writer);
    let mut reopened = f.fresh_store().writer_with_context(&request).unwrap();
    assert_eq!(
        reopened
            .check_generated_receipt(
                &f.key(),
                plan.fingerprint(),
                &f.destination,
                NodeKind::File,
                mode
            )
            .unwrap(),
        PublicationCheck::VerifiedUnchanged
    );
    reopened.remove(&f.key()).unwrap();
    assert!(!f.destination.exists());
}

#[test]
fn generated_content_hash_preserves_user_changes_but_accepts_identical_atomic_save() {
    let f = Fixture::new();
    let request = native_capture_request();
    let old = plan(&f.store, b"old content", &request);
    let new = plan(&f.store, b"new content", &request);
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    let mut writer = f.store.writer_with_context(&request).unwrap();
    writer
        .publish_generated(&f.key(), &old, &f.destination, NodeKind::File, mode)
        .unwrap();
    let replacement = f.destination.with_extension("atomic");
    fs::copy(&f.destination, &replacement).unwrap();
    fs::rename(&replacement, &f.destination).unwrap();
    assert_eq!(
        writer
            .check_generated_publication(&f.key(), &old, &f.destination, NodeKind::File, mode)
            .unwrap(),
        PublicationCheck::VerifiedUnchanged
    );
    assert_eq!(
        writer
            .publish_generated(&f.key(), &new, &f.destination, NodeKind::File, mode)
            .unwrap(),
        ArtifactChange::Updated
    );
    fs::write(&f.destination, b"user customization").unwrap();
    let before = fs::read(f.store.manifest_path()).unwrap();
    assert!(
        writer
            .publish_generated(&f.key(), &old, &f.destination, NodeKind::File, mode)
            .is_err()
    );
    assert!(writer.remove(&f.key()).is_err());
    assert_eq!(fs::read(&f.destination).unwrap(), b"user customization");
    assert_eq!(fs::read(f.store.manifest_path()).unwrap(), before);
}

#[test]
fn generated_publication_rejects_link_and_recovers_interrupted_update_without_origin_copy() {
    let f = Fixture::new();
    let request = native_capture_request();
    let old = plan(&f.store, b"old", &request);
    let new = plan(&f.store, b"new", &request);
    let mut writer = f.store.writer_with_context(&request).unwrap();
    assert_eq!(
        writer
            .publish_generated(
                &f.key(),
                &old,
                &f.destination,
                NodeKind::File,
                ArtifactMode::Link
            )
            .unwrap_err()
            .stage(),
        ArtifactStage::InvalidPolicy
    );
    assert!(!f.store.manifest_path().exists());
    let mode = ArtifactMode::Copy(CopyPolicy::RejectFileLinks);
    writer
        .publish_generated(&f.key(), &old, &f.destination, NodeKind::File, mode)
        .unwrap();
    writer.fail_at.set(Some(ArtifactStage::Publish));
    assert!(
        writer
            .publish_generated(&f.key(), &new, &f.destination, NodeKind::File, mode)
            .is_err()
    );
    drop(writer);
    let mut reopened = f.fresh_store().writer_with_context(&request).unwrap();
    assert_eq!(
        reopened.recover().unwrap(),
        ArtifactRecovery::CompletedUpdate
    );
    assert_eq!(fs::read(&f.destination).unwrap(), b"new");
    assert_eq!(
        reopened
            .check_generated_receipt(
                &f.key(),
                new.fingerprint(),
                &f.destination,
                NodeKind::File,
                mode
            )
            .unwrap(),
        PublicationCheck::VerifiedUnchanged
    );
    assert!(!f.store.runtime_root().join("owned-payloads").exists());
}

#[test]
fn generated_publication_cancellation_and_file_bounds_leave_no_installed_copy() {
    let f = Fixture::new();
    let request = native_capture_request();
    let generated = plan(&f.store, b"large enough", &request);
    let mut writer = f.store.writer_with_context(&request).unwrap();
    writer.store.limits.copy = CopyLimits::new(1, 1, 1, 1).unwrap();
    assert!(
        writer
            .publish_generated(
                &f.key(),
                &generated,
                &f.destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
            )
            .is_err()
    );
    assert!(!f.store.manifest_path().exists());
    assert!(!f.destination.exists());
    request.cancellation().cancel();
    assert_eq!(
        writer
            .publish_generated(
                &f.key(),
                &generated,
                &f.destination,
                NodeKind::File,
                ArtifactMode::Copy(CopyPolicy::RejectFileLinks)
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
}
