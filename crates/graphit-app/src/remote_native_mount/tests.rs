use super::*;
use graphit_auth::Permission;
use graphit_core::{
    artifact_store::PublicationSlot, brand::Brand, file_lock::Cancellation,
    module_settings::Module, project_paths::InstallationPaths,
};
use graphit_hub::{NativeBinding, RemoteFileOrigin, SourceRef, registry::InstalledArtifact};
use std::{
    fs,
    time::{Duration, Instant},
};

fn fixture() -> crate::source_server::test_fixture::Fixture {
    crate::source_server::test_fixture::fixture(
        &[
            Permission::Read(Module::Ast),
            Permission::Read(Module::Knowledge),
        ],
        Brand::default(),
        NativeBinding::KiroV3,
    )
}

fn remote_entry(
    fixture: &crate::source_server::test_fixture::Fixture,
    kind: ArtifactType,
    digest: [u8; 32],
) -> InstalledArtifact {
    let registry = &fixture.server.policy().registry;
    let projection = registry.limits().projection();
    let source = SourceRef::hub_cache(
        &fixture.paths,
        fixture.projects[0].1,
        kind,
        ArtifactId::new("fixture/remote", projection).unwrap(),
        None,
        ConcreteVersion::new("v1", projection).unwrap(),
        projection,
    )
    .unwrap();
    InstalledArtifact {
        consumer: PublicationSlot::Scope,
        artifact_type: kind,
        id: source.id().clone(),
        installations: vec![Installation {
            target: InstallationTarget::GlobalNative,
            original_source: source,
            display: None,
            alias: None,
            entry_name: None,
            backend: InstalledBackend::RemoteNative(
                RemoteNativeRecord::new(
                    "selected",
                    "captured-native",
                    "us-east-1",
                    None,
                    "fixture",
                    digest,
                    projection,
                )
                .unwrap(),
            ),
            payload: InstalledPayload::Original,
        }],
    }
}

fn cache_entry(fixture: &crate::source_server::test_fixture::Fixture) -> InstalledArtifact {
    let projection = fixture.server.policy().registry.limits().projection();
    let source = SourceRef::hub_cache(
        &fixture.paths,
        fixture.projects[0].1,
        ArtifactType::Skill,
        ArtifactId::new("fixture/skill", projection).unwrap(),
        None,
        ConcreteVersion::new("v1", projection).unwrap(),
        projection,
    )
    .unwrap()
    .with_remote_file_origin(
        RemoteFileOrigin::Broker {
            provider: "selected".into(),
        },
        Sha256::digest(b"cached skill metadata").into(),
        projection,
    )
    .unwrap();
    InstalledArtifact {
        consumer: PublicationSlot::Scope,
        artifact_type: ArtifactType::Skill,
        id: source.id().clone(),
        installations: vec![Installation {
            target: InstallationTarget::GlobalCache,
            original_source: source,
            display: None,
            alias: None,
            entry_name: None,
            backend: InstalledBackend::GlobalCache,
            payload: InstalledPayload::Original,
        }],
    }
}

#[tokio::test]
async fn captured_registry_selection_preserves_logical_sources_and_cache_metadata() {
    let fixture = fixture();
    let StoreOwner::Installation { installation_id } = fixture.auth.installation().owner() else {
        panic!("installation scope expected")
    };
    let policy = fixture.server.policy();
    let logical_registry =
        HubRegistry::global(&fixture.paths, installation_id, policy.registry.limits()).unwrap();
    let registry = &logical_registry;
    assert_eq!(
        registry.path().parent().unwrap(),
        fixture.auth.installation().owner_root()
    );
    let cached = cache_entry(&fixture);
    let ast_entry = remote_entry(
        &fixture,
        ArtifactType::Ast,
        Sha256::digest(b"AST metadata").into(),
    );
    let knowledge_entry = remote_entry(
        &fixture,
        ArtifactType::Knowledge,
        Sha256::digest(b"Knowledge metadata").into(),
    );
    {
        let mut writer = registry
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap();
        for entry in [&cached, &ast_entry, &knowledge_entry] {
            writer.put_metadata(entry.clone()).unwrap();
        }
    }
    let before = fs::read(registry.path()).unwrap();
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let issuer = fixture.auth.issuer();
    let caller = issuer
        .authenticate_runtime(&fixture.raw, &original)
        .unwrap();
    let ast = issuer
        .admit_authenticated_imported_ast_read(&caller, fixture.auth.installation())
        .unwrap();
    let knowledge = issuer
        .admit_authenticated_imported_knowledge_read(&caller, fixture.auth.installation())
        .unwrap();
    for (read, entry) in [
        (ReadAdmission::Ast(&ast), &ast_entry),
        (ReadAdmission::Knowledge(&knowledge), &knowledge_entry),
    ] {
        let source = &entry.installations[0].original_source;
        let selection = Selection::capture(read, registry, source.id(), source.version())
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(
            selection.request().snapshot(),
            original.snapshot()
        ));
        assert_eq!(selection.source(), source);
        assert_eq!(
            selection.source().root().as_path(),
            registry.path().parent().unwrap()
        );
        selection.recheck().unwrap();
        let document = registry.load().unwrap();
        assert_eq!(
            document.get_version(
                ArtifactType::Skill,
                cached.id.as_str(),
                cached.installations[0].original_source.version()
            ),
            Some(&cached)
        );
        assert_eq!(fs::read(registry.path()).unwrap(), before);
    }
    let selection = Selection::capture(
        ReadAdmission::Knowledge(&knowledge),
        registry,
        knowledge_entry.installations[0].original_source.id(),
        knowledge_entry.installations[0].original_source.version(),
    )
    .unwrap()
    .unwrap();
    let replacement = remote_entry(
        &fixture,
        ArtifactType::Knowledge,
        Sha256::digest(b"replacement metadata").into(),
    );
    registry
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .put_metadata(replacement)
        .unwrap();
    assert!(matches!(
        selection.recheck(),
        Err(ErrorKind::SelectionChanged)
    ));
    fixture.server.shutdown().await.unwrap();
}

#[tokio::test]
async fn captured_registry_selection_rejects_foreign_root_and_preserves_cancel_cause() {
    let fixture = fixture();
    let policy = fixture.server.policy();
    let registry = &policy.registry;
    let entry = remote_entry(
        &fixture,
        ArtifactType::Knowledge,
        Sha256::digest(b"Knowledge metadata").into(),
    );
    let cancel = Cancellation::default();
    registry
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .put_metadata(entry.clone())
        .unwrap();
    let before = fs::read(registry.path()).unwrap();
    let original = RequestContext::new(
        policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let issuer = fixture.auth.issuer();
    let caller = issuer
        .authenticate_runtime(&fixture.raw, &original)
        .unwrap();
    let read = issuer
        .admit_authenticated_imported_knowledge_read(&caller, fixture.auth.installation())
        .unwrap();
    let source = &entry.installations[0].original_source;
    let selection = Selection::capture(
        ReadAdmission::Knowledge(&read),
        registry,
        source.id(),
        source.version(),
    )
    .unwrap()
    .unwrap();
    let foreign_root = fixture.temp.path().join("foreign-installation");
    fs::create_dir(&foreign_root).unwrap();
    let paths = InstallationPaths::resolve(
        fixture.paths.brand().clone(),
        fixture.paths.start_directory(),
        fixture.paths.home_directory(),
        Some(&foreign_root),
    )
    .unwrap();
    let StoreOwner::Installation { installation_id } = fixture.auth.installation().owner() else {
        panic!("installation owner")
    };
    let foreign = HubRegistry::global(&paths, installation_id, registry.limits()).unwrap();
    assert!(matches!(
        Selection::capture(
            ReadAdmission::Knowledge(&read),
            &foreign,
            source.id(),
            source.version(),
        ),
        Err(ErrorKind::InvalidInstallation)
    ));
    cancel.cancel();
    assert!(
        matches!(selection.recheck(), Err(ErrorKind::Operation(error)) if error.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(fs::read(registry.path()).unwrap(), before);
    fixture.server.shutdown().await.unwrap();
}
