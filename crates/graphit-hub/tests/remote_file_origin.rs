use graphit_core::{brand::Brand, config::ConfigSnapshot, project_paths::InstallationPaths};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, RemoteFileOrigin, SourceRecord, SourceRef,
    policy::HubResourcePolicy,
};

#[test]
fn file_source_roundtrip_preserves_the_configured_authority_reference() {
    let directory = tempfile::tempdir().unwrap();
    let brand = Brand::default();
    let policy = HubResourcePolicy::resolve(
        &ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap(),
    )
    .unwrap();
    let installation =
        InstallationPaths::resolve(brand, directory.path(), None, Some(directory.path())).unwrap();
    for origin in [
        RemoteFileOrigin::Broker {
            provider: "broker".into(),
        },
        RemoteFileOrigin::Standalone {
            provider: "local".into(),
            profile: Some("transport".into()),
        },
        RemoteFileOrigin::Standalone {
            provider: "public".into(),
            profile: None,
        },
    ] {
        let source = SourceRef::hub_cache(
            &installation,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            ArtifactType::Skill,
            ArtifactId::new("org/skill", policy.projection()).unwrap(),
            None,
            ConcreteVersion::new("release/one", policy.projection()).unwrap(),
            policy.projection(),
        )
        .unwrap()
        .with_remote_file_origin(origin.clone(), [7; 32], policy.projection())
        .unwrap();
        let value = serde_json::to_value(&source).unwrap();
        assert!(value.get("directory").is_none());
        let record: SourceRecord = serde_json::from_value(value).unwrap();
        let restored = SourceRef::restore(record.clone(), policy.projection()).unwrap();
        assert_eq!(restored, source);
        assert_eq!(restored.remote_file_origin(), Some(&origin));
        assert_eq!(restored.archive_sha256(), Some(&[7; 32]));
        let other = source
            .clone()
            .with_remote_file_origin(origin.clone(), [8; 32], policy.projection())
            .unwrap();
        assert_eq!(source.id(), other.id());
        assert_eq!(source.physical_id(), other.physical_id());
        assert_eq!(source.version(), other.version());
        assert_ne!(source.directory(), other.directory());
        let other_origin = RemoteFileOrigin::Standalone {
            provider: origin.provider().into(),
            profile: Some("different-transport".into()),
        };
        let other_authority = source
            .clone()
            .with_remote_file_origin(other_origin.clone(), [7; 32], policy.projection())
            .unwrap();
        assert_ne!(source.directory(), other_authority.directory());
        let mut changed_origin = record.clone();
        changed_origin.remote_file_origin = Some(other_origin);
        assert_eq!(
            SourceRef::restore(changed_origin, policy.projection()).unwrap(),
            other_authority
        );
        let mut changed_digest = record.clone();
        changed_digest.archive_sha256 = Some([8; 32]);
        assert_eq!(
            SourceRef::restore(changed_digest, policy.projection()).unwrap(),
            other
        );
        let mut missing = record;
        missing.archive_sha256 = None;
        assert!(SourceRef::restore(missing, policy.projection()).is_err());
    }
}

#[test]
fn file_origin_rejects_native_root_and_invalid_provider_references() {
    let directory = tempfile::tempdir().unwrap();
    let brand = Brand::default();
    let policy = HubResourcePolicy::resolve(
        &ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap(),
    )
    .unwrap();
    let installation =
        InstallationPaths::resolve(brand, directory.path(), None, Some(directory.path())).unwrap();
    for kind in [
        ArtifactType::Ast,
        ArtifactType::Knowledge,
        ArtifactType::Skill,
    ] {
        let source = SourceRef::hub_cache(
            &installation,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            kind,
            ArtifactId::new("org/item", policy.projection()).unwrap(),
            None,
            ConcreteVersion::new("one", policy.projection()).unwrap(),
            policy.projection(),
        )
        .unwrap();
        let origin = RemoteFileOrigin::Broker {
            provider: if kind == ArtifactType::Skill {
                ""
            } else {
                "broker"
            }
            .into(),
        };
        assert!(
            source
                .with_remote_file_origin(origin, [7; 32], policy.projection())
                .is_err()
        );
    }
}
