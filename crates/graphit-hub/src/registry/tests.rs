use super::*;
mod global_cache;
use crate::{ConcreteVersion, Projection, ProjectionPlanner, ProjectionRequest};
use graphit_core::{
    artifact_store::{CopyLimits, CopyPolicy},
    brand::Brand,
    project_identity::ProjectMetadata,
};
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Instant,
};

pub(super) fn limits() -> HubLimits {
    HubLimits::new(
        DocumentLimits {
            bytes: 256 * 1024,
            entries: 128,
            key_bytes: 8192,
            json_nodes: 50_000,
            json_string_bytes: 32 * 1024,
        },
        ProjectionLimits::new(4096, 32 * 1024, 128, 8192, 128).unwrap(),
        ArtifactLimits::new(
            256 * 1024,
            8192,
            CopyLimits::new(16 * 1024 * 1024, 512, 32 * 1024, 16).unwrap(),
        )
        .unwrap(),
        crate::materialization::MaterializationLimits {
            output: graphit_core::artifact_store::CopyLimits::new(
                512 * 1024 * 1024,
                100_000,
                8 * 1024 * 1024,
                128,
            )
            .unwrap(),
            snapshots: graphit_core::artifact_store::CopyLimits::new(
                16 * 1024 * 1024,
                512,
                32 * 1024,
                16,
            )
            .unwrap(),
            lineage: crate::materialization::LineageLimits::new(4096, 8192, 32 * 1024).unwrap(),
        },
    )
    .unwrap()
}
struct Fixture {
    temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: ProjectPaths,
    identity: ProjectIdentity,
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
        let root = temp.path().join("consumer");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(installation.brand().directory())).unwrap();
        let project = ProjectPaths::at(root, &installation).unwrap();
        let identity = ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(1), &Cancellation::default())
            .unwrap()
            .ensure(&ProjectMetadata::new("consumer").unwrap())
            .unwrap();
        Self {
            temp,
            installation,
            project,
            identity,
        }
    }
    fn consumer(&self) -> PublicationSlot {
        graphit_core::artifact_store::CapturedPublicationScope::project(
            &self.project,
            self.identity.id(),
            graphit_core::artifact_store::ScopeLimits::new(1, 16 * 1024, 1024).unwrap(),
            &std::sync::Arc::new(graphit_core::file_io::SnapshotContext::service(
                Cancellation::default(),
            )),
        )
        .unwrap()
        .publication_slot()
    }
    fn registry(&self) -> HubRegistry {
        HubRegistry::project(&self.project, &self.identity, limits()).unwrap()
    }
    fn global(&self) -> HubRegistry {
        HubRegistry::global(
            &self.installation,
            &InstallationId::new("installation-1").unwrap(),
            limits(),
        )
        .unwrap()
    }
    fn source(&self, kind: ArtifactType, id: &str, version: &str) -> SourceRef {
        SourceRef::local_link(
            NativePath::new(self.temp.path().join("producer")).unwrap(),
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            kind,
            ArtifactId::new(id, limits().projection).unwrap(),
            ConcreteVersion::new(version, limits().projection).unwrap(),
            limits().projection,
        )
        .unwrap()
    }
    fn native(&self, id: &str, agents: &[NativeBinding]) -> InstalledArtifact {
        let source = self.source(ArtifactType::Mcp, id, "feature/α");
        InstalledArtifact {
            consumer: PublicationSlot::Scope,
            artifact_type: ArtifactType::Mcp,
            id: source.id().clone(),
            installations: agents
                .iter()
                .map(|agent| Installation {
                    target: InstallationTarget::for_artifact(*agent, source.artifact_type()),
                    original_source: source.clone(),
                    alias: None,
                    entry_name: None,
                    display: None,
                    backend: InstalledBackend::Native,
                    payload: crate::registry::InstalledPayload::Original {
                        selection_binding: None,
                    },
                })
                .collect(),
        }
    }
    fn publish(&self) -> LogicalIntent {
        let source = self.source(ArtifactType::Command, "org/repo", "v1");
        fs::create_dir_all(source.directory().as_path()).unwrap();
        fs::write(source.directory().as_path().join("COMMAND.md"), "source").unwrap();
        fs::create_dir_all(self.project.root().join(".codex/commands")).unwrap();
        let planner = ProjectionPlanner::new(
            NativePath::new(self.project.root().to_owned()).unwrap(),
            NativePath::new(
                self.project
                    .project_runtime(self.identity.id(), &[])
                    .unwrap(),
            )
            .unwrap(),
            limits().projection,
            limits().artifacts,
            ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        );
        let Projection::Physical(plan) = planner
            .plan(ProjectionRequest {
                binding: NativeBinding::Codex,
                source: &source,
                alias: Some("local"),
                entry_name: None,
            })
            .unwrap()
        else {
            panic!()
        };
        let desired = InstalledArtifact {
            consumer: self.consumer(),
            artifact_type: source.artifact_type(),
            id: source.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::for_artifact(
                    NativeBinding::Codex,
                    source.artifact_type(),
                ),
                original_source: source,
                alias: Some("local".into()),
                entry_name: None,
                display: None,
                backend: InstalledBackend::Physical {
                    mode: plan.mode(),
                    node_kind: plan.selected().kind(),
                    destination: plan.destination().clone(),
                    name: plan.name().into(),
                },
                payload: crate::registry::InstalledPayload::Original {
                    selection_binding: Some(plan.selected().record()),
                },
            }],
        };
        LogicalIntent {
            consumer: self.consumer(),
            key: plan.key().clone(),
            phase: IntentPhase::IntentReady,
            goal: LogicalGoal::Publish {
                bindings: Box::new(
                    PublicationBindings::from_plan(
                        &plan,
                        &NativePath::new(self.project.root().to_owned()).unwrap(),
                    )
                    .unwrap(),
                ),
            },
            old: None,
            desired: Some(desired),
        }
    }
}
fn writer(registry: &HubRegistry) -> HubWriter {
    registry
        .writer(Duration::from_secs(1), &Cancellation::default())
        .unwrap()
}
fn finish(writer: &mut HubWriter) {
    writer.advance(IntentPhase::PhysicalStarted).unwrap();
    writer.advance(IntentPhase::PhysicalComplete).unwrap();
    writer.commit_logical().unwrap();
    writer.clear().unwrap();
}

#[test]
fn missing_scoped_documents_are_empty_without_creation() {
    let f = Fixture::new();
    let project = f.registry();
    let global = f.global();
    assert_eq!(
        project.load().unwrap().scope(),
        &HubScope::Project {
            project_id: f.identity.id()
        }
    );
    assert_eq!(
        global.load().unwrap().scope(),
        &HubScope::Global {
            installation_id: "installation-1".into()
        }
    );
    assert!(!project.path().exists());
    assert!(!global.path().exists());
    assert!(!global.lock_path().exists());
    assert!(!f.installation.root().exists());
    let mut tiny = project.clone();
    tiny.limits.document_bytes = 1;
    assert!(tiny.load().unwrap().entries().next().is_none());
}
#[test]
fn project_registry_is_global_per_id_and_shared_by_its_checkouts_only() {
    let f = Fixture::new();
    let registry = f.registry();
    assert_eq!(
        registry.path(),
        f.project
            .project_runtime(f.identity.id(), &["hub.json"])
            .unwrap()
    );
    assert_eq!(
        registry.lock_path(),
        f.project
            .project_runtime(f.identity.id(), &[".hub.json.lock"])
            .unwrap()
    );
    writer(&registry)
        .set_agents(&[NativeBinding::Codex])
        .unwrap();
    let checkout = f.temp.path().join("second-checkout");
    fs::create_dir(&checkout).unwrap();
    fs::create_dir(checkout.join(f.installation.brand().directory())).unwrap();
    let second = ProjectPaths::at(&checkout, &f.installation).unwrap();
    let identity = ProjectIdentityStore::at(&second)
        .writer(Duration::from_secs(1), &Cancellation::default())
        .unwrap()
        .ensure_with_id(
            &ProjectMetadata::new("second-checkout").unwrap(),
            f.identity.id(),
        )
        .unwrap();
    let shared = HubRegistry::project(&second, &identity, limits()).unwrap();
    assert_eq!(shared.path(), registry.path());
    assert_eq!(shared.load().unwrap().agents(), &[NativeBinding::Codex]);
    writer(&shared)
        .set_agents(&[NativeBinding::Claude])
        .unwrap();
    assert_eq!(registry.load().unwrap().agents(), &[NativeBinding::Claude]);
    assert!(!f.project.brand_directory().join("hub.json").exists());
    assert!(!second.brand_directory().join("hub.json").exists());
    assert!(!second.brand_directory().join(".hub.json.lock").exists());

    let other = Fixture::new();
    let other_checkout = ProjectPaths::at(other.project.root(), &f.installation).unwrap();
    let isolated = HubRegistry::project(&other_checkout, &other.identity, limits()).unwrap();
    assert_ne!(isolated.path(), registry.path());
    assert!(isolated.load().unwrap().agents().is_empty());
    writer(&isolated)
        .set_agents(&[NativeBinding::Cursor])
        .unwrap();
    assert_eq!(registry.load().unwrap().agents(), &[NativeBinding::Claude]);
    assert!(HubRegistry::project(&f.project, &other.identity, limits()).is_err());
    let mut wrong_parent = registry.clone();
    wrong_parent.parent = isolated.parent.clone();
    assert!(wrong_parent.load().is_err());

    let another_installation = ProjectPaths::at(f.project.root(), &other.installation).unwrap();
    let private = HubRegistry::project(&another_installation, &f.identity, limits()).unwrap();
    assert_ne!(private.path(), registry.path());
    assert!(private.load().unwrap().agents().is_empty());
    writer(&private)
        .set_agents(&[NativeBinding::OpenCode])
        .unwrap();
    assert_eq!(registry.load().unwrap().agents(), &[NativeBinding::Claude]);
}

#[cfg(unix)]
#[test]
fn global_project_registry_parent_alias_is_rejected_without_foreign_effects() {
    let f = Fixture::new();
    let registry = f.registry();
    let parent = registry.path().parent().unwrap().to_path_buf();
    fs::create_dir_all(parent.parent().unwrap()).unwrap();
    let foreign = f.temp.path().join("foreign-catalog");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("hub.json"), b"foreign catalog preserved").unwrap();
    std::os::unix::fs::symlink(&foreign, &parent).unwrap();
    assert!(registry.load().is_err());
    assert!(
        registry
            .writer(Duration::from_secs(1), &Cancellation::default())
            .is_err()
    );
    assert_eq!(
        fs::read(foreign.join("hub.json")).unwrap(),
        b"foreign catalog preserved"
    );
    assert!(!foreign.join(".hub.json.lock").exists());
}

#[test]
fn identity_changes_ephemeral_scope_brand_and_corrupt_are_errors() {
    let f = Fixture::new();
    let registry = f.registry();
    writer(&registry)
        .set_agents(&[NativeBinding::Codex])
        .unwrap();
    let original = fs::read(registry.path()).unwrap();
    for (key, value) in [
        ("format", json!(1)),
        ("brand", json!("another")),
        (
            "scope",
            json!({"kind":"Global","installation_id":"installation-1"}),
        ),
        ("agents", json!(["codex", "codex"])),
    ] {
        let mut raw: Value = serde_json::from_slice(&original).unwrap();
        raw[key] = value;
        fs::write(registry.path(), serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(registry.load().is_err());
    }
    fs::write(registry.path(), &original).unwrap();
    let mut metadata = ProjectMetadata::new("consumer").unwrap();
    metadata.ephemeral = true;
    let ephemeral = ProjectIdentityStore::at(&f.project)
        .writer(Duration::from_secs(1), &Cancellation::default())
        .unwrap()
        .update(f.identity.id(), &metadata)
        .unwrap();
    assert!(HubRegistry::project(&f.project, &ephemeral, limits()).is_err());
    assert!(registry.load().is_err());
    assert_eq!(fs::read(registry.path()).unwrap(), original);
}
#[test]
fn strict_json_rejects_duplicates_everywhere_null_unknown_and_trailing_data() {
    let f = Fixture::new();
    let registry = f.registry();
    writer(&registry)
        .put_metadata(f.native("org/repo", &[NativeBinding::Codex]))
        .unwrap();
    let original = fs::read(registry.path()).unwrap();
    let text = String::from_utf8(original.clone()).unwrap();
    let cases = [
        text.replacen("\"format\":4", "\"format\":4,\"format\":4", 1),
        text.replacen(
            "\"adapter\":\"codex\"",
            "\"adapter\":\"codex\",\"adapter\":\"codex\"",
            1,
        ),
        text.replacen("\"kind\":\"Native\"", "\"kind\":\"Native\",\"x\":true", 1),
        text.replacen("\"agents\":[]", "\"agents\":null", 1),
        format!("{text} false"),
        text.replacen(
            "\"kind\":\"Native\"",
            "\"kind\":\"Native\",\"x\":{\"k\":1,\"k\":2}",
            1,
        ),
    ];
    for bad in cases {
        assert_ne!(bad.as_bytes(), original);
        fs::write(registry.path(), bad).unwrap();
        assert!(registry.load().is_err());
    }
    fs::write(registry.path(), &original).unwrap();
    assert!(registry.load().is_ok());
}
#[test]
fn ordered_membership_and_all_capabilities_roundtrip_without_source_files() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    w.set_agents(&NativeBinding::ALL).unwrap();
    for kind in [
        ArtifactType::Mcp,
        ArtifactType::Ast,
        ArtifactType::Knowledge,
        ArtifactType::Language,
        ArtifactType::Power,
        ArtifactType::Rule,
    ] {
        let source = f.source(kind, "org/repo", "branch/α");
        let entry = InstalledArtifact {
            consumer: PublicationSlot::Scope,
            artifact_type: kind,
            id: source.id().clone(),
            installations: NativeBinding::ALL
                .iter()
                .map(|binding| Installation {
                    display: None,
                    target: InstallationTarget::for_artifact(*binding, kind),
                    original_source: source.clone(),
                    alias: None,
                    entry_name: Some(String::new()),
                    backend: if kind == ArtifactType::Rule {
                        InstalledBackend::DynamicContext
                    } else {
                        InstalledBackend::Native
                    },
                    payload: crate::registry::InstalledPayload::Original {
                        selection_binding: None,
                    },
                })
                .collect(),
        };
        w.put_metadata(entry).unwrap();
    }
    let expected = w.snapshot().clone();
    drop(w);
    assert_eq!(registry.load().unwrap(), expected);
    assert_eq!(expected.agents(), &NativeBinding::ALL);
    assert_eq!(expected.entries().count(), 6);
    assert!(!f.temp.path().join("producer").exists());
    let mut bad = f.native("other", &[NativeBinding::Codex]);
    bad.installations.push(bad.installations[0].clone());
    assert!(writer(&registry).put_metadata(bad).is_err());
}
#[test]
fn source_record_restore_keeps_new_cache_layout_and_rejects_mutations() {
    let f = Fixture::new();
    let policy = limits().projection;
    let producer = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
    let id = ArtifactId::new("org/repo", policy).unwrap();
    let version = ConcreteVersion::new("BRANCH/ação", policy).unwrap();
    let sources = [
        SourceRef::hub_cache(
            &f.installation,
            producer,
            ArtifactType::Skill,
            id.clone(),
            Some(ArtifactId::new("remote/name", policy).unwrap()),
            version.clone(),
            policy,
        )
        .unwrap(),
        SourceRef::resources(
            &f.project,
            producer,
            ArtifactType::Skill,
            id.clone(),
            version.clone(),
            policy,
        )
        .unwrap(),
        SourceRef::local_link(
            NativePath::new(f.temp.path().join("absent")).unwrap(),
            producer,
            ArtifactType::Skill,
            id,
            version,
            policy,
        )
        .unwrap(),
    ];
    for source in sources {
        let value = serde_json::to_value(&source).unwrap();
        let record: SourceRecord = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(SourceRef::restore(record, policy).unwrap(), source);
        assert!(value.get("directory").is_none());
        assert_eq!(
            value.get("physical_id").is_some(),
            source.physical_id() != source.id()
        );
        let mut invalid = value.clone();
        invalid["directory"] = json!(f.temp.path().join("different"));
        assert!(serde_json::from_value::<SourceRecord>(invalid).is_err());
        let record: SourceRecord = serde_json::from_value(value).unwrap();
        assert!(
            SourceRef::restore(
                record,
                ProjectionLimits::new(1, 32 * 1024, 128, 8192, 128).unwrap()
            )
            .is_err()
        );
    }
    assert!(!f.installation.root().exists());
}
#[test]
fn byte_node_string_key_entry_and_batch_limits_reject_before_replacements() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    w.put_metadata(f.native("one", &[NativeBinding::Codex]))
        .unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    let mut exact = registry.clone();
    exact.limits.document_bytes = bytes.len();
    assert!(exact.load().is_ok());
    exact.limits.document_bytes -= 1;
    assert!(exact.load().is_err());
    for (nodes, strings, keys) in [
        (1, 32 * 1024, 8192),
        (50_000, 1, 8192),
        (50_000, 32 * 1024, 1),
    ] {
        let mut bounded = registry.clone();
        bounded.limits.json_nodes = nodes;
        bounded.limits.json_string_bytes = strings;
        bounded.limits.key_bytes = keys;
        assert!(bounded.load().is_err());
    }
    w.registry.limits.entries = 1;
    assert!(
        w.put_metadata(f.native("two", &[NativeBinding::Codex]))
            .is_err()
    );
    assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    let mut bounded = registry.clone();
    bounded.limits.entries = 1;
    drop(w);
    let mut raw: Value = serde_json::from_slice(&bytes).unwrap();
    let mut another = raw["artifacts"]["mcp"]["one"].clone();
    another[0]["id"] = json!("two");
    another[0]["installations"][0]["source"]["id"] = json!("two");
    another[0]["installations"][0]["source"]["physical_id"] = json!("two");
    raw["artifacts"]["mcp"]["two"] = another;
    fs::write(registry.path(), serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(bounded.load().is_err());
}
#[test]
fn metadata_noop_preserves_exact_bytes_mtime_and_has_zero_replacements() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    let entry = f.native("org/repo", &[NativeBinding::Cursor, NativeBinding::Codex]);
    w.put_metadata(entry.clone()).unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    let modified = fs::metadata(registry.path()).unwrap().modified().unwrap();
    drop(w);
    let mut w = writer(&registry);
    assert!(!w.put_metadata(entry).unwrap());
    assert!(!w.set_agents(&[]).unwrap());

    assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    assert_eq!(
        fs::metadata(registry.path()).unwrap().modified().unwrap(),
        modified
    );
}
#[test]
fn intent_transitions_are_monotone_atomic_and_block_unrelated_mutations() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    let intent = f.publish();
    let dest = match &intent.goal {
        LogicalGoal::Publish { bindings } => bindings.destination.clone(),
        _ => panic!(),
    };
    w.record(intent.clone()).unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    assert!(w.set_agents(&[NativeBinding::Codex]).is_err());
    assert!(
        w.put_metadata(f.native("x", &[NativeBinding::Codex]))
            .is_err()
    );
    assert!(w.record(intent).is_err());
    assert!(w.advance(IntentPhase::PhysicalComplete).is_err());
    assert!(w.clear().is_err());
    assert!(!w.advance(IntentPhase::IntentReady).unwrap());
    assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    finish(&mut w);
    let expected = w.snapshot().clone();
    drop(w);
    assert_eq!(registry.load().unwrap(), expected);
    assert!(
        expected
            .get_for_consumer(&f.consumer(), ArtifactType::Command, "org/repo")
            .is_some()
    );
    assert!(expected.intent().is_none());
    assert!(
        !dest.as_path().exists(),
        "R records transitions only; Core effects belong to C"
    );
}
#[test]
fn forget_supersedes_same_key_preserves_provenance_and_never_nests() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    let intent = f.publish();
    let key = intent.key.clone();
    w.record(intent).unwrap();
    w.advance(IntentPhase::PhysicalStarted).unwrap();
    let other = ArtifactKey::new("cursor", "command", "org/repo").unwrap();
    assert!(
        w.forget(
            consumer_for_test_key(&f, &other),
            &other,
            ForgetPreview::default()
        )
        .is_err()
    );
    let preview = ForgetPreview {
        possible_parents: vec![NativePath::new(f.project.root().join(".codex")).unwrap()],
        ..ForgetPreview::default()
    };
    w.forget(consumer_for_test_key(&f, &key), &key, preview.clone())
        .unwrap();
    let expected = w.snapshot().clone();
    let bytes = fs::read(registry.path()).unwrap();
    assert!(
        !w.forget(
            consumer_for_test_key(&f, &key),
            &key,
            ForgetPreview::default()
        )
        .unwrap()
    );
    assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    drop(w);
    assert_eq!(registry.load().unwrap(), expected);
    let mut w = writer(&registry);
    match &w.snapshot().intent().unwrap().goal {
        LogicalGoal::Forget {
            preview: saved,
            previous: Some(previous),
        } => {
            assert_eq!(saved.as_ref(), &preview);
            assert_eq!(previous.phase, IntentPhase::PhysicalStarted);
            assert!(matches!(previous.goal, PreviousGoal::Publish { .. }));
        }
        _ => panic!(),
    }
    finish(&mut w);
    assert!(w.snapshot().entries().next().is_none());
    assert!(f.temp.path().join("producer/COMMAND.md").exists());
}
#[test]
fn forget_faults_reopen_with_old_or_new_complete_document() {
    for fault in [TestFault::BeforeReplace, TestFault::AfterReplace] {
        let f = Fixture::new();
        let registry = f.registry();
        let mut w = writer(&registry);
        let intent = f.publish();
        let key = intent.key.clone();
        w.record(intent).unwrap();
        w.fault = Some(fault);
        assert!(
            w.forget(
                consumer_for_test_key(&f, &key),
                &key,
                ForgetPreview::default()
            )
            .is_err()
        );
        drop(w);
        let loaded = registry.load().unwrap();
        assert_eq!(
            matches!(loaded.intent().unwrap().goal, LogicalGoal::Forget { .. }),
            matches!(fault, TestFault::AfterReplace)
        );
        let mut w = writer(&registry);
        w.forget(
            consumer_for_test_key(&f, &key),
            &key,
            ForgetPreview::default(),
        )
        .unwrap();
        finish(&mut w);
        assert!(registry.load().unwrap().intent().is_none());
    }
}
#[test]
fn removing_one_adapter_preserves_other_source_version_and_order() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    let mut entry = f.native(
        "org/repo",
        &[
            NativeBinding::Cursor,
            NativeBinding::Codex,
            NativeBinding::Qwen,
        ],
    );
    entry.installations[1].original_source = f.source(ArtifactType::Mcp, "org/repo", "different");
    w.put_metadata(entry.clone()).unwrap();
    let key = entry
        .key_for(InstallationTarget::for_artifact(
            NativeBinding::Codex,
            entry.artifact_type,
        ))
        .unwrap();
    w.record(LogicalIntent {
        consumer: PublicationSlot::Scope,
        key,
        phase: IntentPhase::IntentReady,
        goal: LogicalGoal::Remove,
        old: Some(entry.clone()),
        desired: None,
    })
    .unwrap();
    finish(&mut w);
    let saved = w.snapshot().get(ArtifactType::Mcp, "org/repo").unwrap();
    assert_eq!(
        saved.installations,
        vec![
            entry.installations[0].clone(),
            entry.installations[2].clone()
        ]
    );
}
#[test]
fn preparing_parents_is_bounded_and_retains_possible_residues_on_forget() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut intent = f.publish();
    intent.phase = IntentPhase::PreparingParents;
    let LogicalGoal::Publish { bindings } = &mut intent.goal else {
        panic!()
    };
    bindings.possible_parents = vec![
        NativePath::new(f.project.root().join(".codex")).unwrap(),
        NativePath::new(f.project.root().join(".codex/commands")).unwrap(),
    ];
    let possible = bindings.possible_parents.clone();
    let key = intent.key.clone();
    fs::remove_dir_all(f.project.root().join(".codex")).unwrap();
    let mut w = writer(&registry);
    w.record(intent).unwrap();
    drop(w);
    let mut w = writer(&registry);
    w.forget(
        consumer_for_test_key(&f, &key),
        &key,
        ForgetPreview {
            possible_parents: possible.clone(),
            ..ForgetPreview::default()
        },
    )
    .unwrap();
    match &w.snapshot().intent().unwrap().goal {
        LogicalGoal::Forget {
            previous: Some(previous),
            ..
        } => {
            assert_eq!(previous.phase, IntentPhase::PreparingParents);
            let PreviousGoal::Publish { bindings } = &previous.goal else {
                panic!()
            };
            assert_eq!(bindings.possible_parents, possible);
        }
        _ => panic!(),
    }
    finish(&mut w);
    assert!(!f.project.root().join(".codex").exists());
}
#[test]
fn cancellation_and_thread_contention_do_not_mutate_registry() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    w.set_agents(&[NativeBinding::Codex]).unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    let other = registry.clone();
    let result = thread::spawn(move || {
        other
            .writer(Duration::from_millis(30), &Cancellation::default())
            .err()
            .unwrap()
            .kind()
    })
    .join()
    .unwrap();
    assert_eq!(result, io::ErrorKind::TimedOut);
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        registry
            .writer(Duration::from_secs(1), &cancellation)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );
    w.cancellation.cancel();
    assert_eq!(
        w.set_agents(&[NativeBinding::Cursor]).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    drop(w);
    assert!(
        registry
            .writer(Duration::from_secs(1), &Cancellation::default())
            .is_ok()
    );
}
#[test]
fn process_writer_child() {
    let Some(root) = std::env::var_os("GRAPHIT_TEST_REGISTRY_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let installation = InstallationPaths::resolve(
        Brand::default(),
        root,
        None,
        Some(&root.join("installation")),
    )
    .unwrap();
    let registry = HubRegistry::global(
        &installation,
        &InstallationId::new("installation-1").unwrap(),
        limits(),
    )
    .unwrap();
    let mut w = writer(&registry);
    w.set_agents(&[NativeBinding::KiroV3]).unwrap();
    fs::write(root.join("ready"), "ready").unwrap();
    loop {
        thread::park();
    }
}
#[test]
fn process_death_releases_real_lock_and_complete_document_reopens() {
    let f = Fixture::new();
    let registry = f.global();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "registry::tests::process_writer_child",
            "--nocapture",
        ])
        .env("GRAPHIT_TEST_REGISTRY_ROOT", f.temp.path())
        .current_dir(f.project.root())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.temp.path().join("ready").exists() {
        if child.try_wait().unwrap().is_some() || Instant::now() > deadline {
            child.kill().ok();
            panic!("child did not hold lock");
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        registry
            .writer(Duration::from_millis(30), &Cancellation::default())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    child.kill().unwrap();
    child.wait().unwrap();
    let mut w = writer(&registry);
    assert_eq!(w.snapshot().agents(), &[NativeBinding::KiroV3]);
    w.set_agents(&[NativeBinding::Codex]).unwrap();
    assert_eq!(registry.load().unwrap().agents(), &[NativeBinding::Codex]);
}
#[cfg(unix)]
#[test]
fn private_creation_existing_modes_readonly_and_symlinks_are_preserved() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let registry = f.global();
    let mut w = writer(&registry);
    w.set_agents(&[NativeBinding::Codex]).unwrap();
    assert_eq!(
        fs::metadata(f.installation.root())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(registry.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(registry.path(), fs::Permissions::from_mode(0o640)).unwrap();
    w.set_agents(&[NativeBinding::Cursor]).unwrap();
    assert_eq!(
        fs::metadata(registry.path()).unwrap().permissions().mode() & 0o777,
        0o640
    );
    fs::set_permissions(registry.path(), fs::Permissions::from_mode(0o440)).unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    assert_eq!(
        w.set_agents(&[NativeBinding::KiroV3]).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    drop(w);
    fs::remove_file(registry.path()).unwrap();
    let target = f.temp.path().join("target");
    fs::write(&target, &bytes).unwrap();
    symlink(&target, registry.path()).unwrap();
    assert!(registry.load().is_err());
    assert!(
        registry
            .writer(Duration::from_secs(1), &Cancellation::default())
            .is_err()
    );
    assert_eq!(fs::read(&target).unwrap(), bytes);
    fs::remove_file(registry.path()).unwrap();
    fs::remove_file(registry.lock_path()).unwrap();
    symlink(&target, registry.lock_path()).unwrap();
    assert!(
        registry
            .writer(Duration::from_secs(1), &Cancellation::default())
            .is_err()
    );
}

#[test]
fn full_future_intent_and_forget_preview_are_preflighted_before_writes() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    let intent = f.publish();
    let key = intent.key.clone();
    w.record(intent).unwrap();
    let preview = ForgetPreview {
        owned: Some(PhysicalDescriptor {
            source: NativePath::new(f.temp.path().join("producer/COMMAND.md")).unwrap(),
            destination: NativePath::new(f.project.root().join(".codex/commands/local.md"))
                .unwrap(),
            node_kind: NodeKind::File,
            mode: ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        }),
        pending: Some(PendingDescriptor {
            phase: graphit_core::artifact_store::ArtifactPending::Cleanup,
            candidate: PhysicalDescriptor {
                source: NativePath::new(f.temp.path().join("producer/COMMAND.md")).unwrap(),
                destination: NativePath::new(f.project.root().join(".codex/commands/local.md"))
                    .unwrap(),
                node_kind: NodeKind::File,
                mode: ArtifactMode::Link,
            },
            staging_directory: Some(NativePath::new(f.temp.path().join("stage")).unwrap()),
            old_staging_directory: Some(NativePath::new(f.temp.path().join("old-stage")).unwrap()),
            old_destination: Some(NativePath::new(f.temp.path().join("old")).unwrap()),
            payload: Some(NativePath::new(f.temp.path().join("stage/payload")).unwrap()),
            backup: Some(NativePath::new(f.temp.path().join("stage/backup")).unwrap()),
            permissions_may_be_modified: true,
        }),
        possible_parents: vec![],
    };
    let original = fs::read(registry.path()).unwrap();
    w.registry.limits.document_bytes = original.len();
    assert!(
        w.forget(consumer_for_test_key(&f, &key), &key, preview.clone())
            .is_err()
    );
    assert_eq!(fs::read(registry.path()).unwrap(), original);
    w.registry.limits = limits();
    w.forget(consumer_for_test_key(&f, &key), &key, preview.clone())
        .unwrap();
    let saved = w.snapshot().clone();
    drop(w);
    assert_eq!(registry.load().unwrap(), saved);
    let LogicalGoal::Forget {
        preview: actual, ..
    } = &saved.intent().unwrap().goal
    else {
        panic!()
    };
    assert_eq!(actual.as_ref(), &preview);
    assert!(!f.temp.path().join("stage").exists());
}
#[test]
fn remove_to_forget_reopens_without_sources_and_preserves_other_adapters() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    let entry = f.native("org/repo", &[NativeBinding::Cursor, NativeBinding::Codex]);
    w.put_metadata(entry.clone()).unwrap();
    let key = entry
        .key_for(InstallationTarget::for_artifact(
            NativeBinding::Codex,
            entry.artifact_type,
        ))
        .unwrap();
    w.record(LogicalIntent {
        consumer: PublicationSlot::Scope,
        key: key.clone(),
        phase: IntentPhase::IntentReady,
        goal: LogicalGoal::Remove,
        old: Some(entry.clone()),
        desired: None,
    })
    .unwrap();
    w.advance(IntentPhase::PhysicalStarted).unwrap();
    w.forget(
        consumer_for_test_key(&f, &key),
        &key,
        ForgetPreview::default(),
    )
    .unwrap();
    drop(w);
    let mut w = writer(&registry);
    let LogicalGoal::Forget {
        previous: Some(previous),
        ..
    } = &w.snapshot().intent().unwrap().goal
    else {
        panic!()
    };
    assert_eq!(previous.phase, IntentPhase::PhysicalStarted);
    assert_eq!(previous.goal, PreviousGoal::Remove);
    finish(&mut w);
    assert_eq!(
        w.snapshot()
            .get(ArtifactType::Mcp, "org/repo")
            .unwrap()
            .installations,
        vec![entry.installations[0].clone()]
    );
    assert!(!f.temp.path().join("producer").exists());
}
#[test]
fn wrong_bindings_phase_key_old_snapshot_and_native_backend_are_rejected() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    let good = f.publish();
    let foreign_project = good.desired.as_ref().unwrap().installations[0]
        .original_source
        .producer();
    assert_ne!(foreign_project, f.identity.id());
    for mutation in 0..6 {
        let mut intent = good.clone();
        match mutation {
            0 => intent.key = ArtifactKey::new("cursor", "command", "org/repo").unwrap(),
            1 => intent.phase = IntentPhase::PhysicalComplete,
            2 => intent.old = intent.desired.clone(),
            3 => {
                let LogicalGoal::Publish { bindings } = &mut intent.goal else {
                    panic!()
                };
                bindings.runtime = NativePath::new(f.project.root().join("different")).unwrap();
            }
            4 => {
                intent.desired.as_mut().unwrap().installations[0].backend = InstalledBackend::Native
            }
            5 => {
                let LogicalGoal::Publish { bindings } = &mut intent.goal else {
                    panic!()
                };
                bindings.runtime =
                    NativePath::new(f.project.project_runtime(foreign_project, &[]).unwrap())
                        .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(w.record(intent).is_err());

        assert!(!registry.path().exists());
    }
    w.record(good).unwrap();
    let original = fs::read(registry.path()).unwrap();
    let mut raw: Value = serde_json::from_slice(&original).unwrap();
    raw["intent"]["phase"] = json!("Wrong");
    fs::write(registry.path(), serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(registry.load().is_err());
    fs::write(registry.path(), &original).unwrap();
}
#[cfg(unix)]
#[test]
fn missing_parent_under_alias_is_rejected_before_any_creation() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let real = f.temp.path().join("real");
    fs::create_dir(&real).unwrap();
    let alias = f.temp.path().join("alias");
    symlink(&real, &alias).unwrap();
    let installation = InstallationPaths::resolve(
        Brand::default(),
        f.temp.path(),
        None,
        Some(&alias.join("new")),
    )
    .unwrap();
    let registry = HubRegistry::global(
        &installation,
        &InstallationId::new("installation-1").unwrap(),
        limits(),
    )
    .unwrap();
    assert!(
        registry
            .writer(Duration::from_secs(1), &Cancellation::default())
            .is_err()
    );
    assert!(!real.join("new").exists());
}
#[cfg(unix)]
#[test]
fn unicode_sources_and_preview_paths_roundtrip_and_non_unicode_is_rejected() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    let mut entry = f.native("org/repo", &[NativeBinding::Codex]);
    let invalid = f.temp.path().join(OsString::from_vec(vec![b'n', 0xff]));
    assert!(NativePath::new(invalid).is_err());
    let path = f.temp.path().join("n-界");
    entry.installations[0].original_source = SourceRef::local_link(
        NativePath::new(path.clone()).unwrap(),
        entry.installations[0].original_source.producer(),
        ArtifactType::Mcp,
        entry.id.clone(),
        ConcreteVersion::new("v1", limits().projection).unwrap(),
        limits().projection,
    )
    .unwrap();
    w.put_metadata(entry.clone()).unwrap();
    let key = entry
        .key_for(InstallationTarget::for_artifact(
            NativeBinding::Codex,
            entry.artifact_type,
        ))
        .unwrap();
    w.forget(
        consumer_for_test_key(&f, &key),
        &key,
        ForgetPreview {
            possible_parents: vec![NativePath::new(path).unwrap()],
            ..ForgetPreview::default()
        },
    )
    .unwrap();
    let expected = w.snapshot().clone();
    drop(w);
    assert_eq!(registry.load().unwrap(), expected);
}

#[test]
fn same_key_forget_after_committed_publish_and_remove_survives_reopen() {
    for remove in [false, true] {
        let f = Fixture::new();
        let registry = f.registry();
        let mut w = writer(&registry);
        let publish = f.publish();
        let key = publish.key.clone();
        w.record(publish).unwrap();
        w.advance(IntentPhase::PhysicalStarted).unwrap();
        w.advance(IntentPhase::PhysicalComplete).unwrap();
        w.commit_logical().unwrap();
        if remove {
            w.clear().unwrap();
            let old = w
                .snapshot()
                .get_for_consumer(&f.consumer(), ArtifactType::Command, "org/repo")
                .cloned();
            w.record(LogicalIntent {
                consumer: f.consumer(),
                key: key.clone(),
                phase: IntentPhase::IntentReady,
                goal: LogicalGoal::Remove,
                old,
                desired: None,
            })
            .unwrap();
            w.advance(IntentPhase::PhysicalStarted).unwrap();
            w.advance(IntentPhase::PhysicalComplete).unwrap();
            w.commit_logical().unwrap();
        }
        drop(w);
        let mut w = writer(&registry);
        assert_eq!(
            w.snapshot().intent().unwrap().phase,
            IntentPhase::LogicalCommitted
        );
        w.forget(
            consumer_for_test_key(&f, &key),
            &key,
            ForgetPreview::default(),
        )
        .unwrap();
        drop(w);
        let mut w = writer(&registry);
        let LogicalGoal::Forget {
            previous: Some(previous),
            ..
        } = &w.snapshot().intent().unwrap().goal
        else {
            panic!()
        };
        assert_eq!(previous.phase, IntentPhase::LogicalCommitted);
        assert_eq!(matches!(previous.goal, PreviousGoal::Remove), remove);
        finish(&mut w);
        assert!(registry.load().unwrap().entries().next().is_none());
        assert!(!f.project.root().join(".codex/commands/local.md").exists());
    }
}
#[test]
fn impossible_nonlink_binding_is_rejected_without_filesystem_reads() {
    let f = Fixture::new();
    let policy = limits().projection;
    let input = NativePath::new(f.temp.path().join("absent/COMMAND.md")).unwrap();
    let record = SourceBindingRecord {
        input: input.clone(),
        canonical: input.clone(),
        kind: NodeKind::File,
        file_link: false,
    };
    assert!(SourceBinding::restore(record.clone(), policy).is_ok());
    let mut bad = record.clone();
    bad.canonical = NativePath::new(f.temp.path().join("another/COMMAND.md")).unwrap();
    assert!(SourceBinding::restore(bad, policy).is_err());
    let mut bad = record;
    bad.input = NativePath::new(f.temp.path().ancestors().last().unwrap().to_owned()).unwrap();
    assert!(SourceBinding::restore(bad, policy).is_err());
    assert!(!f.temp.path().join("absent").exists());
}

#[test]
fn kiro_native_mcp_metadata_and_strict_target_type_survive_roundtrip_without_inference() {
    let f = Fixture::new();
    let registry = f.registry();
    let mut w = writer(&registry);
    w.set_agents(&NativeBinding::ALL).unwrap();
    let entry = f.native("org/repo", &[NativeBinding::KiroV3]);
    assert_eq!(entry.installations.len(), 1);
    w.put_metadata(entry.clone()).unwrap();
    drop(w);
    assert_eq!(
        registry
            .load()
            .unwrap()
            .get(ArtifactType::Mcp, "org/repo")
            .unwrap(),
        &entry
    );
    let original = fs::read(registry.path()).unwrap();
    for target in [
        json!({"kind":"shared_adapter","adapter":"kiro"}),
        json!({"kind":"native_binding","binding":"kiro"}),
        json!({"kind":"native_binding","binding":"kiro-v3","adapter":"kiro"}),
        json!({"kind":"native_binding","binding":null}),
    ] {
        let mut raw: Value = serde_json::from_slice(&original).unwrap();
        raw["artifacts"]["mcp"]["org/repo"][0]["installations"][0]["target"] = target;
        fs::write(registry.path(), serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(registry.load().is_err());
    }
    fs::write(registry.path(), original).unwrap();
    assert!(registry.load().is_ok());
    for kind in [
        ArtifactType::Agent,
        ArtifactType::Mcp,
        ArtifactType::Power,
        ArtifactType::Command,
        ArtifactType::Workflow,
    ] {
        assert!(
            InstallationTarget::SharedAdapter {
                adapter: crate::AdapterId::Kiro
            }
            .key(kind, "org/repo")
            .is_err()
        );
        let target = InstallationTarget::NativeBinding {
            binding: NativeBinding::KiroV3,
        };
        assert_eq!(
            InstallationTarget::from_key(&target.key(kind, "org/repo").unwrap()).unwrap(),
            target
        );
    }
    assert!(
        InstallationTarget::NativeBinding {
            binding: NativeBinding::KiroV3
        }
        .key(ArtifactType::Skill, "org/repo")
        .is_err()
    );
}

#[test]
fn global_versions_upsert_lookup_remove_and_reopen_preserve_other_versions() {
    let f = Fixture::new();
    let registry = f.global();
    let mut one = f.native("org/repo", &[NativeBinding::Codex]);
    one.installations[0].original_source = f.source(ArtifactType::Mcp, "org/repo", "v1");
    let mut two = one.clone();
    two.installations[0].original_source = f.source(ArtifactType::Mcp, "org/repo", "v2");
    let v1 = ConcreteVersion::new("v1", limits().projection).unwrap();
    let v2 = ConcreteVersion::new("v2", limits().projection).unwrap();
    {
        let mut w = writer(&registry);
        assert!(w.put_metadata(one.clone()).unwrap());
        assert!(w.put_metadata(two.clone()).unwrap());
        assert!(!w.put_metadata(two.clone()).unwrap());
    }
    let doc = registry.load().unwrap();
    let mut bounded = registry.clone();
    bounded.limits.entries = 1;
    assert!(bounded.load().is_err());
    assert_eq!(doc.entries().count(), 2);
    assert!(doc.get(ArtifactType::Mcp, "org/repo").is_none());
    assert_eq!(
        doc.get_version(ArtifactType::Mcp, "org/repo", &v1),
        Some(&one)
    );
    assert_eq!(
        doc.get_version(ArtifactType::Mcp, "org/repo", &v2),
        Some(&two)
    );
    one.installations[0].alias = Some("renamed".into());
    let mut w = writer(&registry);
    assert!(w.put_metadata(one.clone()).unwrap());
    assert_eq!(
        w.snapshot().get_version(ArtifactType::Mcp, "org/repo", &v2),
        Some(&two)
    );
    let payload = f.temp.path().join("payload");
    fs::write(&payload, "retained").unwrap();
    assert!(
        w.remove_metadata_version(ArtifactType::Mcp, "org/repo", &v1)
            .unwrap()
    );
    assert!(
        !w.remove_metadata_version(ArtifactType::Mcp, "org/repo", &v1)
            .unwrap()
    );
    drop(w);
    let doc = registry.load().unwrap();
    assert_eq!(doc.entries().count(), 1);
    assert_eq!(doc.get(ArtifactType::Mcp, "org/repo"), Some(&two));
    assert_eq!(fs::read_to_string(payload).unwrap(), "retained");
}

#[test]
fn global_version_corruption_and_mixed_sources_are_rejected() {
    let f = Fixture::new();
    let registry = f.global();
    let one = f.native("org/repo", &[NativeBinding::Codex, NativeBinding::Cursor]);
    let mut mixed = one.clone();
    mixed.installations[1].original_source = f.source(ArtifactType::Mcp, "org/repo", "v2");
    assert!(writer(&registry).put_metadata(mixed).is_err());
    writer(&registry).put_metadata(one).unwrap();
    let original = fs::read(registry.path()).unwrap();
    let mut duplicate: Value = serde_json::from_slice(&original).unwrap();
    let entry = duplicate["artifacts"]["mcp"]["org/repo"][0].clone();
    duplicate["artifacts"]["mcp"]["org/repo"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    fs::write(registry.path(), serde_json::to_vec(&duplicate).unwrap()).unwrap();
    assert!(registry.load().is_err());
    let mut old: Value = serde_json::from_slice(&original).unwrap();
    old["format"] = json!(2);
    fs::write(registry.path(), serde_json::to_vec(&old).unwrap()).unwrap();
    assert!(registry.load().is_err());
}

#[test]
fn project_metadata_remains_singleton_and_version_removal_is_global_only() {
    let f = Fixture::new();
    let registry = f.registry();
    let one = f.native("org/repo", &[NativeBinding::Codex]);
    let mut two = one.clone();
    two.installations[0].original_source = f.source(ArtifactType::Mcp, "org/repo", "v2");
    let version = ConcreteVersion::new("v2", limits().projection).unwrap();
    let mut w = writer(&registry);
    w.put_metadata(one).unwrap();
    w.put_metadata(two.clone()).unwrap();
    assert_eq!(w.snapshot().entries().count(), 1);
    assert_eq!(w.snapshot().get(ArtifactType::Mcp, "org/repo"), Some(&two));
    assert!(
        w.remove_metadata_version(ArtifactType::Mcp, "org/repo", &version)
            .is_err()
    );
}

#[test]
fn global_ambiguous_intents_are_rejected_before_record_and_on_load() {
    let f = Fixture::new();
    let registry = f.global();
    let one = f.native("org/repo", &[NativeBinding::Codex]);
    let mut two = one.clone();
    two.installations[0].original_source = f.source(ArtifactType::Mcp, "org/repo", "v2");
    let intent = LogicalIntent {
        consumer: PublicationSlot::Scope,
        key: one
            .key_for(InstallationTarget::for_artifact(
                NativeBinding::Codex,
                ArtifactType::Mcp,
            ))
            .unwrap(),
        phase: IntentPhase::IntentReady,
        goal: LogicalGoal::Remove,
        old: Some(one.clone()),
        desired: None,
    };
    let mut w = writer(&registry);
    w.put_metadata(one).unwrap();
    w.put_metadata(two).unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    assert!(w.record(intent.clone()).is_err());
    assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    assert!(w.snapshot().intent().is_none());
    drop(w);
    let mut raw: Value = serde_json::from_slice(&bytes).unwrap();
    raw["intent"] = serde_json::to_value(intent).unwrap();
    fs::write(registry.path(), serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(registry.load().is_err());
}

fn global_corpus(f: &Fixture, kind: ArtifactType, version: &str) -> InstalledArtifact {
    let source = f.source(kind, "producer/corpus", version);
    InstalledArtifact {
        consumer: PublicationSlot::Scope,
        artifact_type: kind,
        id: source.id().clone(),
        installations: vec![Installation {
            target: InstallationTarget::GlobalNative,
            original_source: source,
            alias: None,
            entry_name: None,
            display: None,
            backend: InstalledBackend::Native,
            payload: InstalledPayload::Original {
                selection_binding: None,
            },
        }],
    }
}

fn remote_corpus(f: &Fixture, kind: ArtifactType, version: &str) -> InstalledArtifact {
    use crate::{
        native::ContentLimits, native_root::*, policy::RequestContext,
        remote_metadata::MetadataLimits,
    };
    use graphit_core::file_io::SnapshotContext;
    let source = SourceRef::hub_cache(
        &f.installation,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        kind,
        ArtifactId::new("producer/corpus", limits().projection()).unwrap(),
        None,
        ConcreteVersion::new(version, limits().projection()).unwrap(),
        limits().projection(),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    let context = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 32,
        },
        &cancel,
        deadline,
    )
    .unwrap();
    let component = if kind == ArtifactType::Ast {
        NativeComponent {
            role: NativeRole::Graph,
            path: "graph.icebug".into(),
            snapshot: NativeSnapshot::Icebug {
                manifest_path: "icebug.json".into(),
                generation: "actual-sdk-generation".into(),
                manifest_sha256: [9; 32],
            },
        }
    } else {
        NativeComponent {
            role: NativeRole::Wiki,
            path: "index.lance".into(),
            snapshot: NativeSnapshot::Lance {
                dataset_version: 7,
                manifest_path: "_versions/7.manifest".into(),
                manifest_sha256: [8; 32],
            },
        }
    };
    let policy = MetadataLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 32,
        versions: 8,
        dependencies: 8,
        projection: limits().projection(),
    };
    let bytes = NativeRoot::encode(
        &NativeRootInput {
            producer: source.producer(),
            artifact_type: kind,
            id: source.id().as_str(),
            version,
            components: &[component],
        },
        policy,
        &context,
    )
    .unwrap();
    let root = NativeRoot::decode(&bytes, policy, &context).unwrap();
    assert_eq!(root.id(), source.id());
    assert_eq!(root.version(), source.version());
    let original = SnapshotContext::new(deadline, cancel);
    let digest =
        crate::remote_storage::hash_reader(&mut std::io::Cursor::new(&bytes), &original).unwrap();
    fs::create_dir_all(source.directory().as_path()).unwrap();
    fs::write(source.directory().as_path().join("native.json"), bytes).unwrap();
    InstalledArtifact {
        consumer: PublicationSlot::Scope,
        artifact_type: kind,
        id: source.id().clone(),
        installations: vec![Installation {
            target: InstallationTarget::GlobalNative,
            original_source: source,
            alias: None,
            entry_name: None,
            display: None,
            backend: InstalledBackend::RemoteNative(
                crate::remote_native::RemoteNativeRecord::new(
                    "selected-provider",
                    "native-bucket",
                    "explicit-region",
                    Some("https://storage.example"),
                    "tenant",
                    digest,
                    limits().projection(),
                )
                .unwrap(),
            ),
            payload: InstalledPayload::Original {
                selection_binding: None,
            },
        }],
    }
}

#[test]
fn remote_native_writer_restores_two_versions_and_keeps_local_distinct() {
    for kind in [ArtifactType::Ast, ArtifactType::Knowledge] {
        let f = Fixture::new();
        let registry = f.global();
        let one = remote_corpus(&f, kind, "v1");
        let two = remote_corpus(&f, kind, "v2");
        let local = global_corpus(&f, kind, "local");
        let v1 = one.installations[0].original_source.version().clone();
        let v2 = two.installations[0].original_source.version().clone();
        let vl = local.installations[0].original_source.version().clone();
        let descriptor = one.installations[0]
            .original_source
            .directory()
            .as_path()
            .join("native.json");
        let descriptor_bytes = fs::read(&descriptor).unwrap();
        let mut w = writer(&registry);
        assert!(w.put_metadata(one.clone()).unwrap());
        assert!(w.put_metadata(two.clone()).unwrap());
        assert!(w.put_metadata(local.clone()).unwrap());
        assert!(!w.put_metadata(one.clone()).unwrap());
        drop(w);
        let document = registry.load().unwrap();
        assert!(document.get(kind, one.id.as_str()).is_none());
        assert_eq!(document.get_version(kind, one.id.as_str(), &v1), Some(&one));
        assert_eq!(document.get_version(kind, one.id.as_str(), &v2), Some(&two));
        assert_eq!(
            document.get_version(kind, one.id.as_str(), &vl),
            Some(&local)
        );
        let mut w = writer(&registry);
        assert!(
            w.remove_metadata_version(kind, one.id.as_str(), &v1)
                .unwrap()
        );
        drop(w);
        assert!(
            registry
                .load()
                .unwrap()
                .get_version(kind, one.id.as_str(), &v1)
                .is_none()
        );
        assert_eq!(fs::read(descriptor).unwrap(), descriptor_bytes);
    }
}

#[test]
fn remote_native_invalid_installations_never_replace_registry() {
    let f = Fixture::new();
    let registry = f.global();
    let valid = remote_corpus(&f, ArtifactType::Ast, "v1");
    let mut w = writer(&registry);
    w.put_metadata(valid.clone()).unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    let mut cases = Vec::new();
    let mut bad = valid.clone();
    bad.installations[0].alias = Some("alias".into());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].entry_name = Some("entry".into());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].payload = remote_mixed_derived_payload(&f);
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].payload = InstalledPayload::Original {
        selection_binding: match f.publish().desired.unwrap().installations[0]
            .payload
            .clone()
        {
            InstalledPayload::Original { selection_binding } => selection_binding,
            _ => unreachable!(),
        },
    };
    cases.push(bad);
    for binding in NativeBinding::ALL {
        let mut bad = valid.clone();
        bad.installations[0].target = InstallationTarget::for_artifact(binding, ArtifactType::Ast);
        cases.push(bad);
    }
    let mut bad = valid.clone();
    bad.installations.push(bad.installations[0].clone());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].original_source = f.source(ArtifactType::Ast, "producer/corpus", "v1");
    cases.push(bad);
    let mut bad = valid.clone();
    let source = &valid.installations[0].original_source;
    bad.installations[0].original_source = SourceRef::resources(
        &f.project,
        source.producer(),
        ArtifactType::Ast,
        source.id().clone(),
        source.version().clone(),
        limits().projection(),
    )
    .unwrap();
    cases.push(bad);
    let mut bad = valid.clone();
    let source = &valid.installations[0].original_source;
    bad.installations[0].original_source = SourceRef::hub_cache(
        &f.installation,
        source.producer(),
        ArtifactType::Ast,
        source.id().clone(),
        Some(ArtifactId::new("other/physical", limits().projection()).unwrap()),
        source.version().clone(),
        limits().projection(),
    )
    .unwrap();
    cases.push(bad);
    for kind in ArtifactType::ALL {
        if matches!(kind, ArtifactType::Ast | ArtifactType::Knowledge) {
            continue;
        }
        let mut bad = valid.clone();
        bad.artifact_type = kind;
        bad.installations[0].original_source = SourceRef::hub_cache(
            &f.installation,
            source.producer(),
            kind,
            source.id().clone(),
            None,
            source.version().clone(),
            limits().projection(),
        )
        .unwrap();
        cases.push(bad);
    }
    let mut bad = valid.clone();
    bad.installations[0].backend = InstalledBackend::RemoteNative(
        crate::remote_native::RemoteNativeRecord::new(
            "selected-provider",
            "native-bucket",
            "region",
            None,
            &"x".repeat(950),
            [1; 32],
            limits().projection(),
        )
        .unwrap(),
    );
    cases.push(bad);
    for bad in cases {
        assert!(w.put_metadata(bad).is_err());
        assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    }
    drop(w);
    let project = f.registry();
    let mut w = writer(&project);
    assert!(w.put_metadata(valid).is_err());
}

fn remote_mixed_derived_payload(f: &Fixture) -> InstalledPayload {
    use crate::{
        materialization::DerivedArtifact, native::ContentLimits, policy::RequestContext,
        publication::ScopedCoordinator,
    };
    use graphit_core::artifact_store::*;
    let source = f.publish().desired.unwrap().installations[0]
        .original_source
        .clone();
    let cancellation = Cancellation::default();
    let request = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 32,
        },
        &cancellation,
        Instant::now() + Duration::from_secs(10),
    )
    .unwrap();
    let policy = limits().materialization();
    let retention = RetainedBytesLimits::new(65536, 10, 65536).unwrap();
    let snapshot = SourceSnapshot::capture(
        source.directory().as_path(),
        NodeKind::Directory,
        &SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            policy.snapshots,
            RetainedFileSet::new(["COMMAND.md".into()], retention).unwrap(),
            retention,
        )
        .unwrap(),
        request.snapshot(),
    )
    .unwrap();
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        std::iter::empty::<PayloadOverlay>(),
        &PayloadPolicy::new(NodeKind::Directory, root_mode, policy.snapshots),
        request.snapshot(),
    )
    .unwrap();
    let derived = DerivedArtifact::new(
        source.clone(),
        plan,
        "codex-v1",
        "local",
        (
            PayloadSelection::new("COMMAND.md".into(), NodeKind::File).unwrap(),
            PayloadSelection::new("COMMAND.md".into(), NodeKind::File).unwrap(),
        ),
        policy.lineage,
    )
    .unwrap();
    let store = ArtifactStore::new(
        CapturedPublicationScope::project(
            &f.project,
            f.identity.id(),
            ScopeLimits::new(1, 16384, 1024).unwrap(),
            request.snapshot(),
        )
        .unwrap(),
        limits().artifacts(),
    );
    let coordinator = ScopedCoordinator::new(
        f.registry(),
        store,
        NativePath::new(f.project.root().to_owned()).unwrap(),
    )
    .unwrap();
    let mut w = coordinator.writer(&request).unwrap();
    w.publish_derived(
        ProjectionRequest {
            binding: NativeBinding::Codex,
            source: &source,
            alias: Some("local"),
            entry_name: None,
        },
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        derived,
        None,
    )
    .unwrap();
    let payload = w
        .snapshot()
        .get_for_consumer(&f.consumer(), ArtifactType::Command, source.id().as_str())
        .unwrap()
        .installations[0]
        .payload
        .clone();
    assert!(matches!(payload, InstalledPayload::Derived(_)));
    payload
}

#[test]
fn remote_native_final_object_key_boundary_is_exact() {
    let f = Fixture::new();
    let entry = remote_corpus(&f, ArtifactType::Ast, "v1");
    let source = &entry.installations[0].original_source;
    let digest = [3; 32];
    let release = crate::remote_storage::release_prefix(
        source.producer(),
        ArtifactType::Ast,
        source.id(),
        source.version(),
        &digest,
    )
    .unwrap();
    let capacity = 1024 - release.len() - "/native.json".len();
    for (length, accepted) in [(capacity, true), (capacity + 1, false)] {
        let record = crate::remote_native::RemoteNativeRecord::new(
            "selected-provider",
            "native-bucket",
            "region",
            None,
            &"x".repeat(length),
            digest,
            limits().projection(),
        )
        .unwrap();
        assert_eq!(
            record
                .validate_source(source, limits().projection())
                .is_ok(),
            accepted
        );
    }
}

#[test]
fn remote_native_registry_codec_rejects_unknown_null_and_wrong_type() {
    let f = Fixture::new();
    let registry = f.global();
    let mut w = writer(&registry);
    w.put_metadata(remote_corpus(&f, ArtifactType::Ast, "v1"))
        .unwrap();
    drop(w);
    let bytes = fs::read(registry.path()).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    for replacement in [
        "\"provider_name\":null",
        "\"provider_name\":false",
        "\"provider_name\":\"\"",
        "\"provider_name\":\"selected-provider\",\"credential\":\"secret\"",
        "\"provider_name\":\"selected-provider\",\"provider_name\":\"other\"",
    ] {
        let malformed = text.replace("\"provider_name\":\"selected-provider\"", replacement);
        assert_ne!(malformed, text);
        fs::write(registry.path(), malformed).unwrap();
        assert!(registry.load().is_err());
    }
    for replacement in [
        "\"endpoint\":null",
        "\"endpoint\":\"https://user:secret@other\"",
        "\"endpoint\":\"s3://bucket\"",
    ] {
        let malformed = text.replace("\"endpoint\":\"https://storage.example\"", replacement);
        assert_ne!(malformed, text);
        fs::write(registry.path(), malformed).unwrap();
        assert!(registry.load().is_err());
    }
    fs::write(registry.path(), bytes).unwrap();
    assert!(registry.load().is_ok());
}

#[test]
fn global_native_versions_roundtrip_exactly_and_remove_metadata_only() {
    for kind in [
        ArtifactType::Ast,
        ArtifactType::Knowledge,
        ArtifactType::Language,
    ] {
        let f = Fixture::new();
        let registry = f.global();
        let one = global_corpus(&f, kind, "v1");
        let two = global_corpus(&f, kind, "v2");
        let v1 = one.installations[0].original_source.version().clone();
        let v2 = two.installations[0].original_source.version().clone();
        fs::create_dir_all(one.installations[0].original_source.directory().as_path()).unwrap();
        let payload = one.installations[0]
            .original_source
            .directory()
            .as_path()
            .join("retained");
        fs::write(&payload, "actual payload").unwrap();
        let mut w = writer(&registry);
        assert!(w.put_metadata(one.clone()).unwrap());
        assert!(w.put_metadata(two.clone()).unwrap());
        assert!(!w.put_metadata(one.clone()).unwrap());
        drop(w);
        let document = registry.load().unwrap();
        assert!(document.get(kind, one.id.as_str()).is_none());
        assert_eq!(document.get_version(kind, one.id.as_str(), &v1), Some(&one));
        assert_eq!(document.get_version(kind, one.id.as_str(), &v2), Some(&two));
        let mut w = writer(&registry);
        assert!(
            w.remove_metadata_version(kind, one.id.as_str(), &v1)
                .unwrap()
        );
        drop(w);
        assert_eq!(
            registry.load().unwrap().get(kind, two.id.as_str()),
            Some(&two)
        );
        assert_eq!(fs::read_to_string(payload).unwrap(), "actual payload");
    }
}

#[test]
fn global_native_invalid_scope_kind_backend_and_targets_never_publish() {
    let f = Fixture::new();
    let registry = f.global();
    let valid = global_corpus(&f, ArtifactType::Ast, "v1");
    let mut w = writer(&registry);
    w.put_metadata(valid.clone()).unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    let mut cases = Vec::new();
    for kind in ArtifactType::ALL {
        if !matches!(
            kind,
            ArtifactType::Ast | ArtifactType::Knowledge | ArtifactType::Language
        ) {
            cases.push(global_corpus(&f, kind, "v1"));
        }
    }
    let mut bad = valid.clone();
    bad.installations[0].backend = InstalledBackend::DynamicContext;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].backend = f.publish().desired.unwrap().installations[0]
        .backend
        .clone();
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations.push(bad.installations[0].clone());
    cases.push(bad);
    for binding in NativeBinding::ALL {
        let mut bad = valid.clone();
        bad.installations[0].target = InstallationTarget::for_artifact(binding, ArtifactType::Ast);
        cases.push(bad.clone());
        bad.installations.push(valid.installations[0].clone());
        cases.push(bad);
    }
    let mut bad = valid.clone();
    bad.installations[0].alias = Some("host-alias".into());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].entry_name = Some("entry".into());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].payload = f.publish().desired.unwrap().installations[0]
        .payload
        .clone();
    cases.push(bad);
    for bad in cases {
        assert!(w.put_metadata(bad).is_err());
        assert_eq!(fs::read(registry.path()).unwrap(), bytes);
        assert_eq!(
            w.snapshot().get(ArtifactType::Ast, valid.id.as_str()),
            Some(&valid)
        );
    }
    let intent = LogicalIntent {
        consumer: PublicationSlot::Scope,
        key: valid.key_for(InstallationTarget::GlobalNative).unwrap(),
        phase: IntentPhase::IntentReady,
        goal: LogicalGoal::Remove,
        old: Some(valid.clone()),
        desired: None,
    };
    let mut intents = vec![intent];
    for goal in [
        LogicalGoal::Remove,
        LogicalGoal::Forget {
            preview: Box::default(),
            previous: None,
        },
    ] {
        intents.push(LogicalIntent {
            consumer: PublicationSlot::Scope,
            key: InstallationTarget::for_artifact(NativeBinding::Codex, ArtifactType::Ast)
                .key(ArtifactType::Ast, valid.id.as_str())
                .unwrap(),
            phase: IntentPhase::IntentReady,
            goal,
            old: Some(valid.clone()),
            desired: None,
        });
    }
    for intent in &intents {
        assert!(w.record(intent.clone()).is_err());
        assert_eq!(fs::read(registry.path()).unwrap(), bytes);
        assert!(w.snapshot().intent().is_none());
    }
    drop(w);
    for intent in intents {
        let mut raw: Value = serde_json::from_slice(&bytes).unwrap();
        raw["intent"] = serde_json::to_value(intent).unwrap();
        fs::write(registry.path(), serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(registry.load().is_err());
    }
    for bad_target in [
        json!({"kind":"shared_adapter","adapter":"codex"}),
        json!({"kind":"global_native","binding":"kiro-v3"}),
    ] {
        let mut raw: Value = serde_json::from_slice(&bytes).unwrap();
        raw["artifacts"]["ast"]["producer/corpus"][0]["installations"][0]["target"] = bad_target;
        fs::write(registry.path(), serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(registry.load().is_err());
    }
    let project = f.registry();
    let mut w = writer(&project);
    let before = project
        .path()
        .exists()
        .then(|| fs::read(project.path()).unwrap());
    assert!(w.put_metadata(valid).is_err());
    assert_eq!(
        project
            .path()
            .exists()
            .then(|| fs::read(project.path()).unwrap()),
        before
    );
}

#[test]
fn installed_display_roundtrip_updates_without_changing_source_and_rejects_invalid_data() {
    let f = Fixture::new();
    let registry = f.global();
    let mut entry = global_corpus(&f, ArtifactType::Knowledge, "v1");
    let version = entry.installations[0].original_source.version().clone();
    let id = entry.id.clone();
    entry.installations[0].display = Some(
        ArtifactDisplayMetadata::new(
            "Descrição é",
            Some("**Rich**\nartifact description"),
            &["rust".into(), "local".into()],
            registry.limits(),
        )
        .unwrap(),
    );
    writer(&registry).put_metadata(entry.clone()).unwrap();
    assert_eq!(
        registry
            .load()
            .unwrap()
            .get_version(ArtifactType::Knowledge, id.as_str(), &version),
        Some(&entry)
    );
    let previous = entry.installations[0].clone();
    entry.installations[0].display = Some(
        ArtifactDisplayMetadata::new(
            "Descrição é",
            Some("**Updated**"),
            &["new-tag".into()],
            registry.limits(),
        )
        .unwrap(),
    );
    assert_ne!(previous, entry.installations[0]);
    assert!(previous.same_source(&entry.installations[0]));
    assert!(writer(&registry).put_metadata(entry.clone()).unwrap());
    assert_eq!(
        registry
            .load()
            .unwrap()
            .get_version(ArtifactType::Knowledge, id.as_str(), &version),
        Some(&entry)
    );
    let mut changed = entry.installations[0].clone();
    changed.alias = Some("different".into());
    assert!(!previous.same_source(&changed));
    assert!(ArtifactDisplayMetadata::new("", None, &[], registry.limits()).is_err());
    assert!(
        ArtifactDisplayMetadata::new(
            &"x".repeat(registry.limits().document_bytes + 1),
            None,
            &[],
            registry.limits()
        )
        .is_err()
    );
    let bytes = fs::read(registry.path()).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("Descrição é"));
    fs::write(registry.path(), text.replace("Descrição é", "")).unwrap();
    assert!(registry.load().is_err());
    fs::write(registry.path(), bytes).unwrap();
}

fn consumer_for_test_key(f: &Fixture, key: &ArtifactKey) -> PublicationSlot {
    let kind = ArtifactType::ALL
        .into_iter()
        .find(|kind| kind.as_str() == key.artifact_type())
        .unwrap();
    consumer_for_artifact(f.consumer(), kind)
}
