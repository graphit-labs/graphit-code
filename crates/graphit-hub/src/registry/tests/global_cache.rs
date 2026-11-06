use super::*;
use crate::RemoteFileOrigin;

fn cached(f: &Fixture, kind: ArtifactType, version: &str) -> InstalledArtifact {
    let source = SourceRef::hub_cache(
        &f.installation,
        "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        kind,
        ArtifactId::new("producer/module", limits().projection).unwrap(),
        None,
        ConcreteVersion::new(version, limits().projection).unwrap(),
        limits().projection,
    )
    .unwrap()
    .with_remote_file_origin(
        RemoteFileOrigin::Broker {
            provider: "selected".into(),
        },
        [7; 32],
        limits().projection,
    )
    .unwrap();
    InstalledArtifact {
        consumer: PublicationSlot::Scope,
        artifact_type: kind,
        id: source.id().clone(),
        installations: vec![Installation {
            target: InstallationTarget::GlobalCache,
            original_source: source,
            display: None,
            alias: None,
            entry_name: None,
            backend: InstalledBackend::GlobalCache,
            payload: InstalledPayload::Original {
                selection_binding: None,
            },
        }],
    }
}

#[test]
fn captured_registry_keeps_logical_cache_roots_and_rejects_replaced_parent() {
    use graphit_core::{
        file_io::SnapshotContext,
        native_document_file::{CapturedDocumentDirectory, NativeDocumentFileLimits},
    };
    let f = Fixture::new();
    let registry = f.global();
    let one = cached(&f, ArtifactType::Skill, "captured-one");
    writer(&registry).put_metadata(one.clone()).unwrap();
    let parent = registry.path().parent().unwrap().to_owned();
    let original = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    // Hub's document limit remains independent of the directory capture bound.
    let native = NativeDocumentFileLimits::new(1, 32768, 128, 65536).unwrap();
    let captured = CapturedDocumentDirectory::capture(&parent, native, &original).unwrap();
    let before = fs::read(registry.path()).unwrap();
    assert!(before.len() > 1);
    assert_eq!(
        one.installations[0].original_source.root().as_path(),
        parent
    );
    let document = registry.load_captured(&captured, &original).unwrap();
    assert_eq!(
        document.get_version(
            ArtifactType::Skill,
            one.id.as_str(),
            one.installations[0].original_source.version(),
        ),
        Some(&one)
    );
    assert_eq!(fs::read(registry.path()).unwrap(), before);
    let cancelled = SnapshotContext::service(Cancellation::default());
    cancelled.cancellation().cancel();
    let error = match registry.load_captured(&captured, &cancelled) {
        Err(error) => error,
        Ok(_) => panic!("cancelled registry read must fail"),
    };
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    let error = match registry.load_captured(&captured, &expired) {
        Err(error) => error,
        Ok(_) => panic!("expired registry read must fail"),
    };
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);

    let other = Fixture::new();
    let other_registry = other.global();
    writer(&other_registry)
        .put_metadata(cached(&other, ArtifactType::Skill, "other"))
        .unwrap();
    let foreign = CapturedDocumentDirectory::capture(
        other_registry.path().parent().unwrap(),
        native,
        &original,
    )
    .unwrap();
    assert!(registry.load_captured(&foreign, &original).is_err());

    let moved = f.temp.path().join("held-installation");
    #[cfg(unix)]
    {
        fs::rename(&parent, &moved).unwrap();
        let two = cached(&f, ArtifactType::Skill, "replacement-two");
        writer(&registry).put_metadata(two).unwrap();
        // The replacement is a valid registry with the same logical scope and brand.
        assert_eq!(registry.load().unwrap().entries().count(), 1);
        assert!(registry.load_captured(&captured, &original).is_err());
    }
    #[cfg(windows)]
    {
        let identity = captured.directory_identity();
        // The retained directory handle deliberately denies FILE_SHARE_DELETE.
        let error = fs::rename(&parent, &moved).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(32));
        assert!(!moved.exists());
        assert!(parent.is_dir());
        captured.revalidate(&original).unwrap();
        assert!(captured.directory_identity() == identity);
        assert_eq!(fs::read(registry.path()).unwrap(), before);
        for document in [
            registry.load().unwrap(),
            registry.load_captured(&captured, &original).unwrap(),
        ] {
            assert_eq!(
                document.get_version(
                    ArtifactType::Skill,
                    one.id.as_str(),
                    one.installations[0].original_source.version(),
                ),
                Some(&one)
            );
        }
    }
    original.check().unwrap();
}

#[test]
fn global_cache_versions_roundtrip_and_removal_preserves_payload() {
    for kind in ArtifactType::ALL {
        if matches!(kind, ArtifactType::Ast | ArtifactType::Knowledge) {
            continue;
        }
        let f = Fixture::new();
        let registry = f.global();
        let one = cached(&f, kind, "v1");
        let two = cached(&f, kind, "v2");
        let source = &one.installations[0].original_source;
        fs::create_dir_all(source.directory().as_path()).unwrap();
        let payload = source.directory().as_path().join("retained");
        fs::write(&payload, "cached bytes").unwrap();
        let mut w = writer(&registry);
        assert!(w.put_metadata(one.clone()).unwrap());
        assert!(w.put_metadata(two.clone()).unwrap());
        assert!(!w.put_metadata(one.clone()).unwrap());
        assert!(w.snapshot().agents().is_empty());
        assert!(w.snapshot().intent().is_none());
        drop(w);
        let document = registry.load().unwrap();
        assert_eq!(document.entries().count(), 2);
        assert!(document.get(kind, one.id.as_str()).is_none());
        assert_eq!(
            document.get_version(kind, one.id.as_str(), source.version()),
            Some(&one)
        );
        assert_eq!(
            document.get_version(
                kind,
                two.id.as_str(),
                two.installations[0].original_source.version()
            ),
            Some(&two)
        );
        let mut w = writer(&registry);
        assert!(
            w.remove_metadata_version(kind, one.id.as_str(), source.version())
                .unwrap()
        );
        assert!(
            !w.remove_metadata_version(kind, one.id.as_str(), source.version())
                .unwrap()
        );
        drop(w);
        assert_eq!(
            registry.load().unwrap().get(kind, two.id.as_str()),
            Some(&two)
        );
        assert_eq!(fs::read_to_string(&payload).unwrap(), "cached bytes");
        assert!(!f.installation.root().join(".codex").exists());
    }
}

#[test]
fn metadata_update_replaces_only_expected_version_and_rejects_stale_selection() {
    let f = Fixture::new();
    let registry = f.global();
    let one = cached(&f, ArtifactType::Skill, "v1");
    let two = cached(&f, ArtifactType::Skill, "v2");
    let three = cached(&f, ArtifactType::Skill, "v3");
    let mut w = writer(&registry);
    w.put_metadata(one.clone()).unwrap();
    w.put_metadata(two.clone()).unwrap();
    assert!(
        w.replace_metadata_version(&one.installations[0], three.clone())
            .unwrap()
    );
    assert_eq!(w.snapshot().entries().count(), 2);
    assert_eq!(
        w.snapshot().get_version(
            ArtifactType::Skill,
            two.id.as_str(),
            two.installations[0].original_source.version()
        ),
        Some(&two)
    );
    assert_eq!(
        w.snapshot().get_version(
            ArtifactType::Skill,
            three.id.as_str(),
            three.installations[0].original_source.version()
        ),
        Some(&three)
    );
    let bytes = fs::read(registry.path()).unwrap();
    assert_eq!(
        w.replace_metadata_version(&one.installations[0], two.clone())
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(registry.path()).unwrap(), bytes);
    let mut refreshed = three.clone();
    refreshed.installations[0].display =
        Some(ArtifactDisplayMetadata::new("Current display", None, &[], limits()).unwrap());
    assert!(
        w.replace_metadata_version(&two.installations[0], refreshed.clone())
            .unwrap()
    );
    assert_eq!(
        w.snapshot().get_version(
            ArtifactType::Skill,
            three.id.as_str(),
            three.installations[0].original_source.version()
        ),
        Some(&refreshed)
    );
    assert_eq!(w.snapshot().entries().count(), 1);
    assert!(
        !w.replace_metadata_version(&three.installations[0], refreshed.clone())
            .unwrap()
    );
    let mut foreign = cached(&f, ArtifactType::Skill, "v4");
    foreign.installations[0].original_source = foreign.installations[0]
        .original_source
        .clone()
        .with_remote_file_origin(
            RemoteFileOrigin::Broker {
                provider: "foreign".into(),
            },
            [7; 32],
            limits().projection,
        )
        .unwrap();
    assert!(
        w.replace_metadata_version(&three.installations[0], foreign)
            .is_err()
    );
    assert_eq!(w.snapshot().entries().count(), 1);
}

#[test]
fn global_cache_rejects_scope_provenance_backend_and_native_journal() {
    let f = Fixture::new();
    let registry = f.global();
    let valid = cached(&f, ArtifactType::Agent, "v1");
    let mut w = writer(&registry);
    w.put_metadata(valid.clone()).unwrap();
    let bytes = fs::read(registry.path()).unwrap();
    let mut cases = Vec::new();
    for backend in [InstalledBackend::Native, InstalledBackend::DynamicContext] {
        let mut bad = valid.clone();
        bad.installations[0].backend = backend;
        cases.push(bad);
    }
    for target in [
        InstallationTarget::GlobalNative,
        InstallationTarget::for_artifact(NativeBinding::Codex, ArtifactType::Agent),
    ] {
        let mut bad = valid.clone();
        bad.installations[0].target = target;
        cases.push(bad);
    }
    let mut bad = valid.clone();
    bad.installations[0].alias = Some("alias".into());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].entry_name = Some("entry".into());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations.push(bad.installations[0].clone());
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].original_source = f.source(ArtifactType::Agent, "producer/module", "v1");
    cases.push(bad);
    let other = Fixture::new();
    cases.push(cached(&other, ArtifactType::Agent, "v1"));
    let mut record = serde_json::from_value::<SourceRecord>(
        serde_json::to_value(&valid.installations[0].original_source).unwrap(),
    )
    .unwrap();
    record.remote_file_origin = None;
    assert!(SourceRef::restore(record, limits().projection).is_err());
    let mut bad = valid.clone();
    bad.installations[0].original_source = SourceRef::hub_cache(
        &f.installation,
        valid.installations[0].original_source.producer(),
        ArtifactType::Agent,
        valid.id.clone(),
        None,
        valid.installations[0].original_source.version().clone(),
        limits().projection,
    )
    .unwrap();
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].original_source = SourceRef::hub_cache(
        &f.installation,
        valid.installations[0].original_source.producer(),
        ArtifactType::Agent,
        valid.id.clone(),
        Some(ArtifactId::new("another/module", limits().projection).unwrap()),
        valid.installations[0].original_source.version().clone(),
        limits().projection,
    )
    .unwrap()
    .with_remote_file_origin(
        RemoteFileOrigin::Broker {
            provider: "selected".into(),
        },
        [7; 32],
        limits().projection,
    )
    .unwrap();
    cases.push(bad);
    let mut bad = valid.clone();
    bad.installations[0].payload = f.publish().desired.unwrap().installations[0]
        .payload
        .clone();
    cases.push(bad);
    let mut bad = cached(&f, ArtifactType::Skill, "v1");
    bad.installations[0].backend = f.publish().desired.unwrap().installations[0]
        .backend
        .clone();
    cases.push(bad);
    for bad in cases {
        assert!(w.put_metadata(bad).is_err());
        assert_eq!(fs::read(registry.path()).unwrap(), bytes);
        assert_eq!(
            w.snapshot().get(ArtifactType::Agent, valid.id.as_str()),
            Some(&valid)
        );
    }
    for key in [
        valid.key_for(InstallationTarget::GlobalCache).unwrap(),
        InstallationTarget::for_artifact(NativeBinding::Codex, ArtifactType::Agent)
            .key(ArtifactType::Agent, valid.id.as_str())
            .unwrap(),
    ] {
        for goal in [
            LogicalGoal::Remove,
            LogicalGoal::Forget {
                preview: Box::default(),
                previous: None,
            },
        ] {
            assert!(
                w.record(LogicalIntent {
                    consumer: PublicationSlot::Scope,
                    key: key.clone(),
                    phase: IntentPhase::IntentReady,
                    goal,
                    old: Some(valid.clone()),
                    desired: None
                })
                .is_err()
            );
            assert_eq!(fs::read(registry.path()).unwrap(), bytes);
            assert!(w.snapshot().intent().is_none());
        }
    }
    drop(w);
    assert!(writer(&f.registry()).put_metadata(valid).is_err());
    for (field, value) in [
        ("backend", json!({"kind":"Native"})),
        (
            "backend",
            json!({"kind":"GlobalCache","data":{"binding":"codex"}}),
        ),
        ("target", json!({"kind":"global_cache","binding":"codex"})),
        ("alias", json!("alias")),
    ] {
        let mut raw: Value = serde_json::from_slice(&bytes).unwrap();
        raw["artifacts"]["agent"]["producer/module"][0]["installations"][0][field] = value;
        fs::write(registry.path(), serde_json::to_vec(&raw).unwrap()).unwrap();
        assert!(registry.load().is_err());
    }
    fs::write(registry.path(), bytes).unwrap();
    assert!(registry.load().is_ok());
}
