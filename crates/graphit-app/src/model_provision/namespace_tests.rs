use super::*;
use graphit_core::{
    artifact_store::{GeneratedNode, PayloadMode, PayloadPolicy, StoreOwner, ValidatedPayloadPlan},
    brand::Brand,
    file_lock::Cancellation,
};
use std::os::unix::fs::PermissionsExt;

fn publish_local_document(
    store: &ArtifactStore,
    original: &Arc<SnapshotContext>,
    destination: &Path,
    name: &str,
) {
    let mode = PayloadMode::from_permissions(&fs::Permissions::from_mode(0o600)).unwrap();
    let plan = ValidatedPayloadPlan::generated(
        [GeneratedNode::file(PathBuf::new(), name.as_bytes().to_vec(), mode).unwrap()],
        &PayloadPolicy::new(
            graphit_fs::NodeKind::File,
            mode,
            store.limits().copy_limits(),
        ),
        original,
    )
    .unwrap();
    let mut writer = store.writer_with_context(original).unwrap();
    writer.recover().unwrap();
    writer
        .publish_generated(
            &ArtifactKey::new("local-fixture", "document", name).unwrap(),
            &plan,
            destination,
            graphit_fs::NodeKind::File,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        )
        .unwrap();
}

#[test]
fn model_file_cache_and_native_instruction_journals_coexist_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let paths = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let producer = ModelProvisioner::capture(
        &paths,
        original.clone(),
        ProvisionLimits::product().unwrap(),
    )
    .unwrap();
    let StoreOwner::Installation { installation_id } = producer.artifacts.owner() else {
        panic!("Installation owner")
    };
    let installation_id = installation_id.clone();
    let native = temp.path().join("home/.agents/skills");
    fs::create_dir_all(&native).unwrap();
    let scope_limits = producer.artifacts.scope().limits();
    let make_cache = || {
        ArtifactStore::new(
            CapturedPublicationScope::installation(
                &paths,
                installation_id.clone(),
                [paths.root().to_owned()],
                scope_limits,
                &original,
            )
            .unwrap(),
            producer.artifacts.limits(),
        )
    };
    let make_instructions = || {
        ArtifactStore::new(
            CapturedPublicationScope::installation_instructions(
                &paths,
                installation_id.clone(),
                [native.clone()],
                scope_limits,
                &original,
            )
            .unwrap(),
            producer.artifacts.limits(),
        )
    };
    let cache = make_cache();
    let instructions = make_instructions();
    assert_eq!(
        producer.artifacts.runtime_root(),
        paths.root().join("runtime/models")
    );
    assert_eq!(cache.runtime_root(), paths.root().join("runtime"));
    assert_eq!(
        instructions.runtime_root(),
        paths.root().join("runtime/native-instructions")
    );
    assert_eq!(producer.artifacts.owner(), cache.owner());
    assert_eq!(cache.owner(), instructions.owner());
    assert_eq!(
        producer
            .artifacts
            .scope()
            .allowed_roots()
            .collect::<Vec<_>>(),
        vec![paths.models_directory().as_path()]
    );
    assert_eq!(
        cache.scope().allowed_roots().collect::<Vec<_>>(),
        vec![paths.root()]
    );
    assert_eq!(
        instructions.scope().allowed_roots().collect::<Vec<_>>(),
        vec![native.as_path()]
    );
    let model_document = paths.models_directory().join("local-model-publication.txt");
    let cache_document = paths.root().join("local-file-cache.txt");
    let native_document = native.join("native-instruction.txt");
    // Tiny local documents exercise this producer's actual writer, not forged
    // model weights or inference. Digest/model validation is unchanged.
    publish_local_document(&producer.artifacts, &original, &model_document, "model");
    publish_local_document(&cache, &original, &cache_document, "cache");
    publish_local_document(&instructions, &original, &native_document, "instructions");
    let reopened = ModelProvisioner::capture(
        &paths,
        original.clone(),
        ProvisionLimits::product().unwrap(),
    )
    .unwrap();
    for store in [&reopened.artifacts, &make_cache(), &make_instructions()] {
        let mut writer = store.writer_with_context(&original).unwrap();
        writer.recover().unwrap();
        let snapshot = writer.load().unwrap().unwrap();
        assert_eq!(snapshot.entries().len(), 1);
        assert!(snapshot.pending().is_none());
    }
    assert_eq!(fs::read(model_document).unwrap(), b"model");
    assert_eq!(fs::read(cache_document).unwrap(), b"cache");
    assert_eq!(fs::read(native_document).unwrap(), b"instructions");
}
