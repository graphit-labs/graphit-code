//! Public consumer journeys: no private Core receipts or production fault callbacks.
use graphit_core::{
    artifact_store::{
        ArtifactChange, ArtifactKey, ArtifactLimits, ArtifactMode, ArtifactStore, CopyLimits,
        CopyPolicy,
    },
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    file_lock::Cancellation,
    installation_identity::InstallationId,
    native_path::NativePath,
    project_identity::{ProjectIdentity, ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_hub::{
    AgentMembership, ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, Projection,
    ProjectionLimits, ProjectionPlanner, ProjectionRequest, SourceRef,
    publication::{PublicationChange, PublicationFailure, PublicationWriter, ScopedCoordinator},
    registry::{DocumentLimits, HubLimits, HubRegistry, InstalledBackend},
    select_agent, select_agents,
};
use std::{fs, path::Path, time::Duration};

fn limits() -> HubLimits {
    HubLimits::new(
        DocumentLimits {
            bytes: 1024 * 1024,
            entries: 128,
            key_bytes: 8192,
            json_nodes: 50_000,
            json_string_bytes: 32 * 1024,
        },
        ProjectionLimits::new(4096, 128, 8192, 128).unwrap(),
        ArtifactLimits::new(
            1024 * 1024,
            8192,
            CopyLimits::new(16 * 1024 * 1024, 512).unwrap(),
        )
        .unwrap(),
        graphit_hub::materialization::MaterializationLimits {
            output: graphit_core::artifact_store::CopyLimits::new(512 * 1024 * 1024, 100_000)
                .unwrap(),
            snapshots: graphit_core::artifact_store::CopyLimits::new(16 * 1024 * 1024, 512)
                .unwrap(),
            lineage: graphit_hub::materialization::LineageLimits::new(4096, 8192, 32 * 1024)
                .unwrap(),
        },
    )
    .unwrap()
}
fn copy() -> ArtifactMode {
    ArtifactMode::Copy(CopyPolicy::RejectEntryLinks)
}

fn render_owned(
    source: &SourceRef,
    name: &str,
    output: &[u8],
) -> graphit_hub::materialization::DerivedArtifact {
    render_owned_schema(source, name, output, "host-schema-v1")
}
fn render_owned_schema(
    source: &SourceRef,
    name: &str,
    output: &[u8],
    schema: &str,
) -> graphit_hub::materialization::DerivedArtifact {
    use graphit_core::artifact_store::*;
    use std::time::Instant;
    let c = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    let template = match source.artifact_type() {
        ArtifactType::Skill => "SKILL.md",
        ArtifactType::Agent => "AGENT.md",
        _ => "COMMAND.md",
    };
    let retention = RetainedBytesLimits::new(256 * 1024, 10).unwrap();
    let copy = limits().materialization().snapshots;
    let snapshot = SourceSnapshot::capture(
        source.directory().as_path(),
        graphit_fs::NodeKind::Directory,
        &SnapshotPolicy::new(
            CopyPolicy::RejectEntryLinks,
            copy,
            RetainedFileSet::new([template.into()], retention).unwrap(),
            retention,
        )
        .unwrap(),
        &c,
    )
    .unwrap();
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let file_mode = PayloadMode::from_permissions(
        &fs::metadata(source.directory().as_path().join(template))
            .unwrap()
            .permissions(),
    )
    .unwrap();
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [PayloadOverlay::new(
            template.into(),
            "rendered.md".into(),
            output.to_vec(),
            file_mode,
        )
        .unwrap()],
        &PayloadPolicy::new(graphit_fs::NodeKind::Directory, root_mode, copy),
        &c,
    )
    .unwrap();
    let selection = if source.artifact_type() == ArtifactType::Skill {
        PayloadSelection::new("".into(), graphit_fs::NodeKind::Directory).unwrap()
    } else {
        PayloadSelection::new("rendered.md".into(), graphit_fs::NodeKind::File).unwrap()
    };
    graphit_hub::materialization::DerivedArtifact::new(
        source.clone(),
        plan,
        schema,
        name,
        (
            if source.artifact_type() == ArtifactType::Skill {
                PayloadSelection::new("".into(), graphit_fs::NodeKind::Directory).unwrap()
            } else {
                PayloadSelection::new(template.into(), graphit_fs::NodeKind::File).unwrap()
            },
            selection,
        ),
        limits().materialization().lineage,
    )
    .unwrap()
}

#[test]
fn public_derived_project_and_installation_copy_preserve_origin_and_assets() {
    use graphit_core::artifact_store::*;
    use graphit_hub::registry::InstalledPayload;
    for global in [false, true] {
        for mode in [copy()] {
            let f = Fixture::new();
            let source = f.seed(ArtifactType::Skill, "org/repo", "v1", 0x73);
            fs::write(
                source.directory().as_path().join("SKILL.md"),
                "original source A",
            )
            .unwrap();
            let (registry, store) = if global {
                fs::create_dir_all(f.installation.root()).unwrap();
                let id = InstallationId::new("derived-test-installation").unwrap();
                let scope = PublicationScope::installation(
                    &f.installation,
                    id.clone(),
                    [f.consumer.root().to_owned()],
                )
                .unwrap();
                (
                    HubRegistry::global(&f.installation, &id, limits()).unwrap(),
                    ArtifactStore::new(scope, limits().artifacts()),
                )
            } else {
                (f.registry(), f.store())
            };
            let coordinator = ScopedCoordinator::new(
                registry.clone(),
                store.clone(),
                NativePath::new(f.consumer.root().to_owned()).unwrap(),
            )
            .unwrap();
            let mut w = coordinator
                .writer(&request_context(
                    Duration::from_secs(30),
                    &Cancellation::default(),
                ))
                .unwrap();
            for binding in [NativeBinding::Codex, NativeBinding::Claude] {
                w.publish_derived(
                    request(&source, binding, Some("alias-B")),
                    mode,
                    render_owned(&source, "alias-B", b"rendered B"),
                    None,
                )
                .unwrap();
            }

            let installed = w
                .snapshot()
                .get_for_consumer(w.consumer(), ArtifactType::Skill, source.id().as_str())
                .unwrap();
            for installation in &installed.installations {
                assert_eq!(installation.original_source, source);
                let InstalledPayload::Derived(payload) = &installation.payload else {
                    panic!()
                };
                assert_eq!(
                    payload.original_selection.relative(),
                    std::path::Path::new("")
                );
                assert_eq!(
                    payload.original_selection.kind(),
                    graphit_fs::NodeKind::Directory
                );

                let InstalledBackend::Physical { destination, .. } = &installation.backend else {
                    panic!()
                };
                assert_eq!(
                    fs::read(destination.as_path().join("rendered.md")).unwrap(),
                    b"rendered B"
                );
                assert!(!destination.as_path().join("SKILL.md").exists());
                assert!(destination.as_path().join("files/empty").is_dir());
                assert_eq!(
                    fs::read(destination.as_path().join("files/00.bin")).unwrap(),
                    vec![0x73; 64 * 1024]
                );
                assert_mode(&destination.as_path().join("files/00.bin"), 0o640);
            }
            let derived_payload = installed.installations[0].payload.clone();
            fs::remove_dir_all(source.directory().as_path()).unwrap();
            drop(w);
            if global {
                let original_source = SourceRef::local_link(
                    source.directory().clone(),
                    source.producer(),
                    ArtifactType::Ast,
                    source.id().clone(),
                    source.version().clone(),
                    limits().projection(),
                )
                .unwrap();
                let mut metadata = registry
                    .writer(Duration::from_secs(30), &Cancellation::default())
                    .unwrap();
                let before = fs::read(registry.path()).unwrap();
                assert!(
                    metadata
                        .put_metadata(graphit_hub::registry::InstalledArtifact {
                            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                            artifact_type: ArtifactType::Ast,
                            id: source.id().clone(),
                            installations: vec![graphit_hub::registry::Installation {
                                target: graphit_hub::InstallationTarget::GlobalNative,
                                original_source,
                                alias: None,
                                entry_name: None,
                                display: None,
                                backend: InstalledBackend::Native,
                                payload: derived_payload,
                            }],
                        })
                        .is_err()
                );
                assert_eq!(fs::read(registry.path()).unwrap(), before);
            }

            let mut fresh = coordinator
                .writer(&request_context(
                    Duration::from_secs(30),
                    &Cancellation::default(),
                ))
                .unwrap();

            let first = ArtifactKey::new(
                NativeBinding::Codex.as_str(),
                ArtifactType::Skill.as_str(),
                source.id().as_str(),
            )
            .unwrap();
            let last = ArtifactKey::new(
                NativeBinding::Claude.as_str(),
                ArtifactType::Skill.as_str(),
                source.id().as_str(),
            )
            .unwrap();

            fresh.remove(&first).unwrap();
            fresh.remove(&last).unwrap();
        }
    }
}
fn mode(path: &Path, value: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(value)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = (path, value);
}
fn assert_mode(path: &Path, value: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            value
        );
    }
    #[cfg(not(unix))]
    let _ = (path, value);
}
struct Fixture {
    _temp: tempfile::TempDir,
    installation: InstallationPaths,
    producer: ProjectPaths,
    producer_identity: ProjectIdentity,
    consumer: ProjectPaths,
    consumer_identity: ProjectIdentity,
}
fn project(
    installation: &InstallationPaths,
    parent: &Path,
    name: &str,
) -> (ProjectPaths, ProjectIdentity) {
    let root = parent.join(name);
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join(installation.brand().directory())).unwrap();
    let paths = ProjectPaths::at(root, installation).unwrap();
    let identity = ProjectIdentityStore::at(&paths)
        .writer(Duration::from_secs(2), &Cancellation::default())
        .unwrap()
        .ensure(&ProjectMetadata::new(name).unwrap())
        .unwrap();
    (paths, identity)
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let (producer, producer_identity) = project(&installation, temp.path(), "producer");
        let (consumer, consumer_identity) = project(&installation, temp.path(), "consumer");
        assert_ne!(producer_identity.id(), consumer_identity.id());
        Self {
            _temp: temp,
            installation,
            producer,
            producer_identity,
            consumer,
            consumer_identity,
        }
    }
    fn registry(&self) -> HubRegistry {
        HubRegistry::project(&self.consumer, &self.consumer_identity, limits()).unwrap()
    }
    fn store(&self) -> ArtifactStore {
        ArtifactStore::new(
            graphit_core::artifact_store::PublicationScope::project(
                &self.consumer,
                self.consumer_identity.id(),
            )
            .unwrap(),
            limits().artifacts(),
        )
    }
    fn coordinator(&self) -> ScopedCoordinator {
        ScopedCoordinator::new(
            self.registry(),
            self.store(),
            NativePath::new(self.consumer.root().to_owned()).unwrap(),
        )
        .unwrap()
    }
    fn writer(&self) -> PublicationWriter {
        self.coordinator()
            .writer(&request_context(
                Duration::from_secs(2),
                &Cancellation::default(),
            ))
            .unwrap()
    }
    fn source(&self, kind: ArtifactType, id: &str, version: &str) -> SourceRef {
        SourceRef::resources(
            &self.producer,
            self.producer_identity.id(),
            kind,
            ArtifactId::new(id, limits().projection()).unwrap(),
            ConcreteVersion::new(version, limits().projection()).unwrap(),
            limits().projection(),
        )
        .unwrap()
    }
    fn seed(&self, kind: ArtifactType, id: &str, version: &str, byte: u8) -> SourceRef {
        let source = self.source(kind, id, version);
        let dir = source.directory().as_path();
        fs::create_dir_all(dir).unwrap();
        if kind == ArtifactType::Skill {
            fs::create_dir(dir.join("files")).unwrap();
            fs::create_dir(dir.join("files/empty")).unwrap();
            fs::create_dir(dir.join("empty")).unwrap();
            for n in 0..16 {
                let path = dir.join(format!("files/{n:02}.bin"));
                fs::write(&path, vec![byte; 64 * 1024]).unwrap();
                mode(&path, 0o640);
            }
            fs::write(dir.join("empty.bin"), []).unwrap();
            mode(&dir.join("empty.bin"), 0o640);
            for path in [
                dir.to_owned(),
                dir.join("files"),
                dir.join("files/empty"),
                dir.join("empty"),
            ] {
                mode(&path, 0o750);
            }
        } else {
            let filename = match kind {
                ArtifactType::Agent => "AGENT.md",
                ArtifactType::Rule => "RULE.md",
                _ => "COMMAND.md",
            };
            let path = dir.join(filename);
            fs::write(&path, vec![byte; 64 * 1024]).unwrap();
            mode(&path, 0o640);
        }
        source
    }
    fn planner(&self, mode: ArtifactMode) -> ProjectionPlanner {
        ProjectionPlanner::new(
            NativePath::new(self.consumer.root().to_owned()).unwrap(),
            limits().projection(),
            mode,
        )
    }
}
fn request<'a>(
    source: &'a SourceRef,
    binding: NativeBinding,
    alias: Option<&'a str>,
) -> ProjectionRequest<'a> {
    ProjectionRequest {
        binding,
        source,
        alias,
        entry_name: None,
    }
}
fn key(source: &SourceRef, binding: NativeBinding) -> ArtifactKey {
    graphit_hub::InstallationTarget::for_artifact(binding, source.artifact_type())
        .key(source.artifact_type(), source.id().as_str())
        .unwrap()
}
fn destination(
    f: &Fixture,
    source: &SourceRef,
    binding: NativeBinding,
    alias: Option<&str>,
) -> std::path::PathBuf {
    f.planner(copy())
        .preflight(&[request(source, binding, alias)])
        .unwrap()[0]
        .destination()
        .unwrap()
        .as_path()
        .to_owned()
}
fn assert_payload(source: &SourceRef, destination: &Path, byte: u8) {
    if source.artifact_type() == ArtifactType::Skill {
        assert!(destination.join("empty").is_dir());
        assert!(destination.join("files/empty").is_dir());
        assert_eq!(
            fs::read(destination.join("empty.bin")).unwrap(),
            Vec::<u8>::new()
        );
        assert_mode(destination, 0o750);
        for n in 0..16 {
            let path = destination.join(format!("files/{n:02}.bin"));
            assert_eq!(fs::read(&path).unwrap(), vec![byte; 64 * 1024]);
            assert_mode(&path, 0o640);
        }
    } else {
        assert_eq!(fs::read(destination).unwrap(), vec![byte; 64 * 1024]);
        assert_mode(destination, 0o640);
    }
}

#[test]
fn captured_selection_ten_bindings_real_create_noop_update_remove_and_sources() {
    let f = Fixture::new();
    let brand = Brand::default();
    let config = ConfigSnapshot::new(
        &brand,
        [ConfigDocument::new(
            &brand,
            ConfigSource::Environment,
            [(
                ConfigKey::new("agent").unwrap(),
                serde_json::json!("unknown"),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    let members = AgentMembership::new(NativeBinding::ALL);
    f.registry()
        .writer(Duration::from_secs(2), &Cancellation::default())
        .unwrap()
        .set_agents(members.as_slice())
        .unwrap();
    assert_eq!(
        select_agents(&config, None, &members).unwrap(),
        NativeBinding::ALL
    );
    assert!(select_agent(&config, None, &members).is_err());
    for binding in NativeBinding::ALL {
        let selected = select_agent(&config, Some(binding.as_str()), &members).unwrap();
        assert_eq!(selected.binding, binding);
        for kind in [
            ArtifactType::Command,
            ArtifactType::Workflow,
            ArtifactType::Skill,
            ArtifactType::Agent,
        ] {
            let id = format!("org/{binding}-{}", kind.as_str());
            let source = f.seed(kind, &id, "Feature/α-v1", b'a');
            // Each public operation has one request; fixture setup precedes its deadline.
            let change = f
                .writer()
                .publish(request(&source, selected.binding, None), copy(), None)
                .unwrap();
            if kind == ArtifactType::Agent
                || (matches!(binding, NativeBinding::Kimi | NativeBinding::KiroV3)
                    && matches!(kind, ArtifactType::Command | ArtifactType::Workflow))
            {
                assert_eq!(change.change, PublicationChange::Unsupported);

                continue;
            }
            assert_eq!(
                change.change,
                PublicationChange::Physical(ArtifactChange::Created)
            );
            let dest = destination(&f, &source, binding, None);
            assert_payload(&source, &dest, b'a');
            let metadata = fs::read(f.registry().path()).unwrap();
            let manifest = fs::read(f.store().manifest_path()).unwrap();
            let repeat = f
                .writer()
                .publish(request(&source, binding, None), copy(), None)
                .unwrap();
            assert_eq!(repeat.change, PublicationChange::VerifiedUnchanged);

            assert_eq!(fs::read(f.registry().path()).unwrap(), metadata);
            assert_eq!(fs::read(f.store().manifest_path()).unwrap(), manifest);
            let new = f.seed(kind, &id, "feature/β-V2", b'b');
            let mut writer = f.writer();
            writer
                .publish(request(&new, binding, Some("renamed")), copy(), None)
                .unwrap();
            let new_dest = destination(&f, &new, binding, Some("renamed"));
            assert!(!dest.exists());
            assert_payload(&new, &new_dest, b'b');
            let installed = &writer
                .snapshot()
                .get_for_consumer(writer.consumer(), kind, &id)
                .unwrap()
                .installations[0];
            assert_eq!(
                installed.original_source.producer(),
                f.producer_identity.id()
            );
            assert_eq!(installed.original_source.version().as_str(), "feature/β-V2");
            drop(writer);
            let remove_key = key(&new, binding);
            let mut writer = f.writer();
            writer.remove(&remove_key).unwrap();
            assert!(!new_dest.exists());
            assert!(
                writer
                    .snapshot()
                    .get_for_consumer(writer.consumer(), kind, &id)
                    .is_none()
            );
            let original_payload = if kind == ArtifactType::Skill {
                source.directory().as_path().to_owned()
            } else {
                source
                    .directory()
                    .as_path()
                    .join(if kind == ArtifactType::Agent {
                        "AGENT.md"
                    } else {
                        "COMMAND.md"
                    })
            };
            assert_payload(&source, &original_payload, b'a');
        }
    }
}

#[test]
fn type_and_basename_collisions_are_rejected_before_effects() {
    let f = Fixture::new();
    let command = f.source(ArtifactType::Command, "org/same", "v1");
    let workflow = f.source(ArtifactType::Workflow, "org/same", "v1");
    let err = f
        .planner(copy())
        .preflight(&[
            request(&command, NativeBinding::Codex, None),
            request(&workflow, NativeBinding::Codex, None),
        ])
        .unwrap_err();
    assert_eq!(err.stage(), graphit_hub::ProjectionStage::Collision);
    assert!(!f.consumer.root().join(".codex").exists());
    let command = f.seed(ArtifactType::Command, "org/same", "v1", b'a');
    let other = f.seed(ArtifactType::Command, "other/same", "v1", b'b');
    assert!(
        f.planner(copy())
            .preflight(&[
                request(&command, NativeBinding::Codex, None),
                request(&other, NativeBinding::Codex, None)
            ])
            .is_err()
    );
}

#[test]
fn source_codecs_global_scope_and_dynamic_context_do_not_imply_physical_authority() {
    let f = Fixture::new();
    let source = f.seed(ArtifactType::Rule, "org/rule", "Feature/α", b'r');
    let cache = SourceRef::hub_cache(
        &f.installation,
        f.producer_identity.id(),
        ArtifactType::Skill,
        ArtifactId::new("org/repo", limits().projection()).unwrap(),
        Some(ArtifactId::new("remote/repo", limits().projection()).unwrap()),
        ConcreteVersion::new("feature/α", limits().projection()).unwrap(),
        limits().projection(),
    )
    .unwrap();
    assert!(
        cache
            .directory()
            .as_path()
            .starts_with(f.installation.root().join("artifacts/modules"))
    );
    assert!(!cache.directory().as_path().exists());
    let record = serde_json::from_slice(&serde_json::to_vec(&cache).unwrap()).unwrap();
    assert_eq!(
        SourceRef::restore(record, limits().projection()).unwrap(),
        cache
    );
    let global = HubRegistry::global(
        &f.installation,
        &InstallationId::new("fixture-installation").unwrap(),
        limits(),
    )
    .unwrap();
    assert!(
        ScopedCoordinator::new(
            global.clone(),
            f.store(),
            NativePath::new(f.consumer.root().to_owned()).unwrap(),
        )
        .is_err()
    );
    let mut w = f.writer();
    let outcome = w
        .publish(request(&source, NativeBinding::Codex, None), copy(), None)
        .unwrap();
    assert_eq!(outcome.change, PublicationChange::MetadataOnly);

    assert!(!f.consumer.root().join(".codex").exists());
    assert!(w.physical_snapshot().unwrap().is_none());
    assert!(global.load().unwrap().entries().next().is_none());
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    w.remove(&key(&source, NativeBinding::Codex)).unwrap();
    assert!(w.snapshot().entries().next().is_none());
}

#[test]
fn retained_render_publishes_without_reopening_origin_and_preserves_user_edits() {
    let f = Fixture::new();
    let source = f.seed(ArtifactType::Command, "org/command", "v1", b'a');
    let rendered = render_owned(&source, "retained", b"rendered output");
    let repeated = render_owned(&source, "retained", b"rendered output");
    let updated = render_owned(&source, "retained", b"updated output");
    let destination = destination(&f, &source, NativeBinding::Codex, Some("retained"));
    fs::remove_dir_all(source.directory().as_path()).unwrap();
    let mut writer = f.writer();
    let projection = || request(&source, NativeBinding::Codex, Some("retained"));

    let published = writer
        .publish_derived(projection(), copy(), rendered, None)
        .unwrap();
    assert_eq!(
        published.change,
        PublicationChange::Physical(ArtifactChange::Created)
    );
    assert_eq!(fs::read(&destination).unwrap(), b"rendered output");
    let installed = writer
        .snapshot()
        .get_for_consumer(
            writer.consumer(),
            ArtifactType::Command,
            source.id().as_str(),
        )
        .unwrap();
    assert_eq!(installed.installations[0].original_source, source);
    let physical_before = fs::read(f.store().manifest_path()).unwrap();
    let metadata_before = fs::read(f.registry().path()).unwrap();
    let repeat = writer
        .publish_derived(projection(), copy(), repeated, None)
        .unwrap();
    assert_eq!(repeat.change, PublicationChange::VerifiedUnchanged);
    assert_eq!(
        fs::read(f.store().manifest_path()).unwrap(),
        physical_before
    );
    assert_eq!(fs::read(f.registry().path()).unwrap(), metadata_before);

    fs::write(&destination, b"user edit").unwrap();
    let error = writer
        .publish_derived(projection(), copy(), updated, None)
        .unwrap_err();
    assert!(matches!(error.failure, PublicationFailure::Physical(_)));
    assert_eq!(fs::read(&destination).unwrap(), b"user edit");
    assert_eq!(
        fs::read(f.store().manifest_path()).unwrap(),
        physical_before
    );
    assert_eq!(fs::read(f.registry().path()).unwrap(), metadata_before);
}

#[test]
fn directory_link_removal_preserves_source() {
    let f = Fixture::new();
    let source = f.seed(ArtifactType::Skill, "org/tree", "v1", b'a');
    let mut w = f.writer();
    w.publish(
        request(&source, NativeBinding::Codex, None),
        ArtifactMode::Link,
        None,
    )
    .unwrap();
    let dest = destination(&f, &source, NativeBinding::Codex, None);
    assert_eq!(
        fs::canonicalize(&dest).unwrap(),
        fs::canonicalize(source.directory().as_path()).unwrap()
    );

    let removed = w.remove(&key(&source, NativeBinding::Codex)).unwrap();
    assert_eq!(
        removed.change,
        PublicationChange::Physical(ArtifactChange::Removed)
    );
    assert_eq!(
        fs::symlink_metadata(&dest).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert_payload(&source, source.directory().as_path(), b'a');
}

#[test]
fn link_copy_transition_foreign_conflict_source_free_forget_and_metadata_provenance() {
    let f = Fixture::new();
    let source = f.seed(ArtifactType::Skill, "org/tree", "v1", b'a');
    let mut w = f.writer();
    w.publish(
        request(&source, NativeBinding::Codex, None),
        ArtifactMode::Link,
        None,
    )
    .unwrap();
    drop(w);
    let dest = destination(&f, &source, NativeBinding::Codex, None);
    assert_eq!(
        fs::canonicalize(&dest).unwrap(),
        fs::canonicalize(source.directory().as_path()).unwrap()
    );
    let physical_before = fs::read(f.store().manifest_path()).unwrap();
    let metadata_before = fs::read(f.registry().path()).unwrap();
    let link_before = fs::read_link(&dest).unwrap();
    let mut w = f.writer();
    let repeat = w
        .publish(
            request(&source, NativeBinding::Codex, None),
            ArtifactMode::Link,
            None,
        )
        .unwrap();
    drop(w);
    assert!(matches!(
        repeat.change,
        PublicationChange::VerifiedUnchanged
            | PublicationChange::Physical(ArtifactChange::Unchanged)
    ));
    assert_eq!(
        fs::read(f.store().manifest_path()).unwrap(),
        physical_before
    );
    assert_eq!(fs::read(f.registry().path()).unwrap(), metadata_before);
    assert_eq!(fs::read_link(&dest).unwrap(), link_before);
    assert_payload(&source, &dest, b'a');

    let mut w = f.writer();
    w.publish(request(&source, NativeBinding::Codex, None), copy(), None)
        .unwrap();
    drop(w);
    assert_payload(&source, source.directory().as_path(), b'a');
    assert!(
        !fs::symlink_metadata(&dest)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_payload(&source, &dest, b'a');
    let mut w = f.writer();
    w.publish(request(&source, NativeBinding::Cursor, None), copy(), None)
        .unwrap();
    let cursor = w
        .snapshot()
        .get_for_consumer(
            &f.store().publication_slot(),
            ArtifactType::Skill,
            "org/tree",
        )
        .unwrap()
        .installations[1]
        .clone();
    drop(w);
    let next = f.seed(ArtifactType::Skill, "org/tree", "v2", b'b');
    let mut w = f.writer();
    w.publish(request(&next, NativeBinding::Codex, None), copy(), None)
        .unwrap();
    assert_eq!(
        w.snapshot()
            .get_for_consumer(
                &f.store().publication_slot(),
                ArtifactType::Skill,
                "org/tree"
            )
            .unwrap()
            .installations[1],
        cursor
    );
    drop(w);
    fs::write(dest.join("foreign.txt"), b"preserve").unwrap();
    let mut w = f.writer();
    let err = w.remove(&key(&next, NativeBinding::Codex)).unwrap_err();
    drop(w);
    assert!(matches!(err.failure, PublicationFailure::Physical(_)));

    fs::remove_dir_all(next.directory().as_path()).unwrap();
    let mut w = f.writer();
    let forgotten = w.forget(&key(&next, NativeBinding::Codex)).unwrap();
    let installations = w
        .snapshot()
        .get_for_consumer(
            &f.store().publication_slot(),
            ArtifactType::Skill,
            "org/tree",
        )
        .unwrap()
        .installations
        .clone();
    drop(w);
    assert_eq!(forgotten.change, PublicationChange::Forgotten);
    assert!(forgotten.forgotten.is_some());
    assert_eq!(fs::read(dest.join("foreign.txt")).unwrap(), b"preserve");
    assert_payload(&source, source.directory().as_path(), b'a');
    assert_eq!(installations, [cursor]);
}

// Valid unfinished states use public Hub transitions and actual Core publication.
// Setup does not modify serialized Core state, or claim that journal data owns files.

#[test]
fn nested_worktrees_keep_independent_physical_installations() {
    use graphit_core::artifact_store::PublicationScope;

    for kind in [ArtifactType::Skill, ArtifactType::Agent] {
        let f = Fixture::new();
        let second_root = f.consumer.root().join("nested-worktree");
        fs::create_dir(&second_root).unwrap();
        fs::create_dir(second_root.join(f.installation.brand().directory())).unwrap();
        let second = ProjectPaths::at(&second_root, &f.installation).unwrap();
        let second_identity = ProjectIdentityStore::at(&second)
            .writer(Duration::from_secs(2), &Cancellation::default())
            .unwrap()
            .ensure_with_id(
                &ProjectMetadata::new("nested-worktree").unwrap(),
                f.consumer_identity.id(),
            )
            .unwrap();
        let second_registry = HubRegistry::project(&second, &second_identity, limits()).unwrap();
        let second_store = ArtifactStore::new(
            PublicationScope::project(&second, second_identity.id()).unwrap(),
            limits().artifacts(),
        );
        let second_coordinator = ScopedCoordinator::new(
            second_registry.clone(),
            second_store,
            NativePath::new(second.root().to_owned()).unwrap(),
        )
        .unwrap();
        assert_eq!(second_registry.path(), f.registry().path());
        let a = f.seed(kind, "org/coexist", "artifact-v1", b'a');
        let b = f.seed(kind, "org/coexist", "artifact-v2", b'b');
        let a_next = f.seed(kind, "org/coexist", "artifact-v3", b'c');
        if kind == ArtifactType::Skill {
            for source in [&a, &b, &a_next] {
                fs::write(
                    source.directory().as_path().join("SKILL.md"),
                    "skill source",
                )
                .unwrap();
            }
        }
        let binding = NativeBinding::Codex;
        let publish = |writer: &mut PublicationWriter, source: &SourceRef, bytes: &[u8]| {
            writer
                .publish_derived(
                    request(source, binding, None),
                    copy(),
                    render_owned_schema(source, "coexist", bytes, binding.schema()),
                    None,
                )
                .unwrap()
        };
        let (a_consumer, a_destination) = {
            let mut writer = f.writer();
            publish(&mut writer, &a, b"rendered A");
            (
                writer.consumer().clone(),
                destination(&f, &a, binding, None),
            )
        };
        let (b_consumer, b_destination, b_installed) = {
            let mut writer = second_coordinator
                .writer(&request_context(
                    Duration::from_secs(30),
                    &Cancellation::default(),
                ))
                .unwrap();
            let Projection::Physical(plan) =
                writer.preview(request(&b, binding, None), copy()).unwrap()
            else {
                panic!("physical installation expected")
            };
            let result = publish(&mut writer, &b, b"rendered B");
            assert_eq!(
                result.change,
                PublicationChange::Physical(ArtifactChange::Created)
            );
            let consumer = writer.consumer().clone();
            let installed = writer
                .snapshot()
                .get_for_consumer(&consumer, kind, "org/coexist")
                .unwrap()
                .clone();
            (consumer, plan.destination().as_path().to_owned(), installed)
        };
        assert_ne!(a_consumer, b_consumer);
        let content = |destination: &Path| {
            fs::read(if kind == ArtifactType::Skill {
                destination.join("rendered.md")
            } else {
                destination.to_owned()
            })
            .unwrap()
        };
        assert_eq!(content(&a_destination), b"rendered A");
        assert_eq!(content(&b_destination), b"rendered B");
        let document = f.registry().load().unwrap();
        assert_eq!(
            document
                .get_for_consumer(&a_consumer, kind, "org/coexist")
                .unwrap()
                .installations[0]
                .original_source
                .version()
                .as_str(),
            "artifact-v1"
        );
        assert_eq!(
            document.get_for_consumer(&b_consumer, kind, "org/coexist"),
            Some(&b_installed)
        );
        drop(document);
        {
            let mut writer = f.writer();
            publish(&mut writer, &a_next, b"rendered A updated");
            assert_eq!(content(&a_destination), b"rendered A updated");
            assert_eq!(content(&b_destination), b"rendered B");
            writer.remove(&key(&a_next, binding)).unwrap();
        }
        assert!(!a_destination.exists());
        assert_eq!(content(&b_destination), b"rendered B");
        let document = second_registry.load().unwrap();
        assert!(
            document
                .get_for_consumer(&a_consumer, kind, "org/coexist")
                .is_none()
        );
        assert_eq!(
            document.get_for_consumer(&b_consumer, kind, "org/coexist"),
            Some(&b_installed)
        );
        drop(document);
        // Reusing B's retained pin after A's update/removal exercises global reconciliation/GC.
        let mut writer = second_coordinator
            .writer(&request_context(
                Duration::from_secs(30),
                &Cancellation::default(),
            ))
            .unwrap();
        assert_eq!(
            publish(&mut writer, &b, b"rendered B").change,
            PublicationChange::VerifiedUnchanged
        );
        assert_eq!(
            writer
                .snapshot()
                .get_for_consumer(&b_consumer, kind, "org/coexist"),
            Some(&b_installed)
        );
    }
}

#[test]
fn cancellation_has_no_payload_and_open_snapshots_do_not_hold_publication_locks() {
    let f = Fixture::new();
    let source = f.seed(ArtifactType::Command, "org/cancel", "v1", b'a');
    let cancel = Cancellation::default();
    let mut writer = f
        .coordinator()
        .writer(&request_context(Duration::from_secs(2), &cancel))
        .unwrap();
    cancel.cancel();
    let err = writer
        .publish(request(&source, NativeBinding::Codex, None), copy(), None)
        .unwrap_err();
    assert_eq!(
        err.effect,
        graphit_core::artifact_store::ArtifactEffect::NotApplied
    );

    assert!(!destination(&f, &source, NativeBinding::Codex, None).exists());
    drop(writer);
    let held = f.writer();
    // Opening another snapshot does not retain a global publication lock.
    assert!(
        f.coordinator()
            .writer(&request_context(
                Duration::from_secs(1),
                &Cancellation::default()
            ))
            .is_ok()
    );
    drop(held);
    f.writer()
        .publish(request(&source, NativeBinding::Codex, None), copy(), None)
        .unwrap();
}

#[test]
fn remove_distinguishes_untracked_foreign_content_and_already_absent_owned_content() {
    let f = Fixture::new();
    let source = f.seed(ArtifactType::Command, "org/absent", "v1", b'a');
    let dest = destination(&f, &source, NativeBinding::Codex, None);
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(&dest, b"foreign").unwrap();
    let result = f
        .writer()
        .remove(&key(&source, NativeBinding::Codex))
        .unwrap();
    assert_eq!(
        result.change,
        PublicationChange::Physical(ArtifactChange::Unchanged)
    );
    assert_eq!(fs::read(&dest).unwrap(), b"foreign");
    for missing_parent in [false, true] {
        let f = Fixture::new();
        let source = f.seed(ArtifactType::Command, "org/owned", "v1", b'a');
        let mut writer = f.writer();
        writer
            .publish(request(&source, NativeBinding::Codex, None), copy(), None)
            .unwrap();
        let dest = destination(&f, &source, NativeBinding::Codex, None);
        if missing_parent {
            fs::remove_dir_all(dest.parent().unwrap()).unwrap();
        } else {
            fs::remove_file(&dest).unwrap();
        }
        let result = writer.remove(&key(&source, NativeBinding::Codex)).unwrap();
        assert_eq!(
            result.change,
            PublicationChange::Physical(ArtifactChange::Removed)
        );
        assert!(
            writer
                .snapshot()
                .get_for_consumer(
                    writer.consumer(),
                    ArtifactType::Command,
                    source.id().as_str()
                )
                .is_none()
        );
        assert!(
            writer
                .physical_snapshot()
                .unwrap()
                .unwrap()
                .entries()
                .iter()
                .all(|entry| entry.key() != &key(&source, NativeBinding::Codex))
        );
        assert!(!dest.exists());
    }
}

#[test]
fn public_kiro_v3_native_agents_require_renderer_and_survive_source_removal_and_reopen() {
    use graphit_hub::native::*;
    {
        let mode = copy();
        let f = Fixture::new();
        let source = f.seed(ArtifactType::Agent, "org/repo", "v1", 0x73);
        let input = b"---\nname: repo\ndescription: Inspect\nread_only: true\n---\nInspect public fixture.\n";
        fs::write(source.directory().as_path().join("AGENT.md"), input).unwrap();
        let mut writer = f.writer();
        let cancellation = Cancellation::default();
        let mut render = RenderContext::new(
            ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 16,
                max_nodes: 4096,
                max_scalar_bytes: 16384,
                max_collection_entries: 128,
            },
            &cancellation,
            std::time::Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
        let descriptor = AgentDescriptor::parse(input, &mut render).unwrap();
        let binding = NativeBinding::KiroV3;
        let raw = writer
            .publish(request(&source, binding, None), mode, None)
            .unwrap();
        assert_eq!(raw.change, PublicationChange::Unsupported);
        let grants = ToolGrants {
            binding,
            read_only: true,
            allows_shell: false,
            tools: vec![NativeToolGrant {
                name: "read".into(),
                access: ToolAccess::Read,
            }],
        };
        let output = render_agent(binding, &descriptor, None, &grants, &mut render).unwrap();
        writer
            .publish_derived(
                request(&source, binding, None),
                mode,
                render_owned_schema(&source, "repo", &output.bytes, binding.schema()),
                None,
            )
            .unwrap();
        let destination = f.consumer.root().join(&output.relative_entry);
        assert_eq!(fs::read(&destination).unwrap(), output.bytes);
        assert_eq!(
            writer
                .snapshot()
                .get_for_consumer(
                    &f.store().publication_slot(),
                    ArtifactType::Agent,
                    "org/repo"
                )
                .unwrap()
                .installations
                .len(),
            1
        );
        fs::remove_dir_all(source.directory().as_path()).unwrap();
        drop(writer);
        let mut writer = f.writer();
        assert_eq!(fs::read(&destination).unwrap(), output.bytes);
        writer.remove(&key(&source, NativeBinding::KiroV3)).unwrap();
        assert!(!destination.exists());
        assert!(
            writer
                .snapshot()
                .get_for_consumer(
                    &f.store().publication_slot(),
                    ArtifactType::Agent,
                    "org/repo"
                )
                .is_none()
        );
    }
}

fn request_context<'a>(
    timeout: Duration,
    cancellation: &'a Cancellation,
) -> graphit_hub::policy::RequestContext<'a> {
    graphit_hub::policy::RequestContext::new(
        graphit_hub::native::ContentLimits {
            max_input_bytes: 8 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
            max_depth: 32,
            max_nodes: 100_000,
            max_scalar_bytes: 1024 * 1024,
            max_collection_entries: 10_000,
        },
        cancellation,
        std::time::Instant::now().checked_add(timeout).unwrap(),
    )
    .unwrap()
}
