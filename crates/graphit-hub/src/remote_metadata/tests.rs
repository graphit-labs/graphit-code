use super::*;
use crate::{SourceRef, native::ContentLimits};
use graphit_core::{brand::Brand, file_lock::Cancellation, project_paths::InstallationPaths};
use serde_json::{Value, json};
use std::{
    fs,
    time::{Duration, Instant},
};

const PRODUCER: &str = "01KSH1CRFFG8Z74B5ZS78WW808";
const CONSUMER: &str = "01M1YEAE1WYMWG9KJKV1XQC4T4";
const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
pub(super) fn limits() -> MetadataLimits {
    MetadataLimits {
        bytes: 64 * 1024,
        nodes: 4096,
        scalar_bytes: 1024,
        collection_entries: 128,
        versions: 32,
        dependencies: 32,
        projection: ProjectionLimits::new(512, 4096, 128, 255, 32).unwrap(),
    }
}
pub(super) fn context(cancellation: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 64 * 1024,
            max_output_bytes: 64 * 1024,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 1024,
            max_collection_entries: 128,
        },
        cancellation,
        deadline,
    )
    .unwrap()
}
pub(super) fn input<'a>(
    kind: ArtifactType,
    versions: &'a [VersionInput<'a>],
    dependencies: &'a [DependencyInput<'a>],
) -> ManifestInput<'a> {
    ManifestInput {
        producer: PRODUCER.parse().unwrap(),
        artifact_type: kind,
        id: "org/tool",
        name: "tool",
        description: Some("Documentação 📘"),
        tags: &["analysis", "rust"],
        author: Some(AuthorInput {
            username: "publisher",
        }),
        latest: Some("1.2.3"),
        versions,
        dependencies,
    }
}
fn fixture(kind: ArtifactType, context: &RequestContext<'_>) -> Vec<u8> {
    let versions = [
        VersionInput {
            version: "branch/feature/query-✓",
            hash: HASH,
        },
        VersionInput {
            version: "1.2.3",
            hash: HASH,
        },
    ];
    let dependencies = [
        DependencyInput {
            artifact_type: ArtifactType::Command,
            id: "org/dep",
            selector: "1.2",
        },
        DependencyInput {
            artifact_type: ArtifactType::Agent,
            id: "org/agent",
            selector: "branch/main",
        },
    ];
    RemoteArtifactManifest::encode(&input(kind, &versions, &dependencies), limits(), context)
        .unwrap()
}
fn decode_error(
    bytes: &[u8],
    policy: MetadataLimits,
    context: &RequestContext<'_>,
) -> MetadataError {
    match RemoteArtifactManifest::decode(bytes, policy, context) {
        Ok(_) => panic!("unexpected metadata admission"),
        Err(error) => error,
    }
}
fn nodes(value: &Value) -> usize {
    1 + match value {
        Value::Array(values) => values.iter().map(nodes).sum(),
        Value::Object(values) => values.len() + values.values().map(nodes).sum::<usize>(),
        _ => 0,
    }
}

#[test]
fn all_artifact_types_roundtrip_preserves_metadata_and_selector_literals() {
    let cancellation = Cancellation::default();
    let context = context(&cancellation, Instant::now() + Duration::from_secs(10));
    for kind in ArtifactType::ALL {
        let bytes = fixture(kind, &context);
        let output = RemoteArtifactManifest::decode(&bytes, limits(), &context).unwrap();
        assert_eq!(output.producer().to_string(), PRODUCER);
        assert_eq!(output.artifact_type(), kind);
        assert_eq!(output.id().as_str(), "org/tool");
        assert_eq!(output.name(), "tool");
        assert_eq!(output.description(), Some("Documentação 📘"));
        assert_eq!(output.tags(), &["analysis", "rust"]);
        assert_eq!(output.author(), Some("publisher"));
        assert_eq!(output.latest().unwrap().as_str(), "1.2.3");
        assert_eq!(
            output.versions()[0].version().as_str(),
            "branch/feature/query-✓"
        );
        assert_eq!(output.versions()[0].hash()[..4], [0x01, 0x23, 0x45, 0x67]);
        assert_eq!(
            output.dependencies()[0].artifact_type(),
            ArtifactType::Command
        );
        assert_eq!(output.dependencies()[0].id().as_str(), "org/dep");
        assert_eq!(output.dependencies()[0].selector(), "1.2");
        assert_eq!(output.dependencies()[1].selector(), "branch/main");
        assert_eq!(bytes.len(), bytes.capacity());
    }
}

#[test]
fn branch_only_has_no_latest_and_optional_omission_differs_from_null() {
    let cancellation = Cancellation::default();
    let context = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let versions = [VersionInput {
        version: "branch/main",
        hash: HASH,
    }];
    let input = ManifestInput {
        producer: PRODUCER.parse().unwrap(),
        artifact_type: ArtifactType::Skill,
        id: "org/tool",
        name: "tool",
        description: None,
        tags: &[],
        author: None,
        latest: None,
        versions: &versions,
        dependencies: &[],
    };
    let bytes = RemoteArtifactManifest::encode(&input, limits(), &context).unwrap();
    let output = RemoteArtifactManifest::decode(&bytes, limits(), &context).unwrap();
    assert!(output.latest().is_none());
    assert!(output.description().is_none());
    assert!(output.author().is_none());
    assert!(output.tags().is_empty());
    assert!(output.dependencies().is_empty());
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for key in ["latest", "description", "tags", "author"] {
        assert!(value.get(key).is_none());
        let mut invalid = value.clone();
        invalid[key] = Value::Null;
        assert_eq!(
            decode_error(&serde_json::to_vec(&invalid).unwrap(), limits(), &context),
            MetadataError::InvalidDocument,
            "{key}"
        );
    }
    for selector in [
        "",
        "latest",
        "1",
        "1.2",
        "v1.2.3",
        "feature/query-✓",
        "tag/v2.0.0",
    ] {
        let deps = [DependencyInput {
            artifact_type: ArtifactType::Skill,
            id: "org/dep",
            selector,
        }];
        let input = ManifestInput {
            dependencies: &deps,
            author: None,
            ..input
        };
        let bytes = RemoteArtifactManifest::encode(&input, limits(), &context).unwrap();
        assert_eq!(
            RemoteArtifactManifest::decode(&bytes, limits(), &context)
                .unwrap()
                .dependencies()[0]
                .selector(),
            selector
        );
    }
}

#[test]
fn pure_source_composition_retains_publisher_and_creates_no_payload_or_project_state() {
    let fixture_root = tempfile::tempdir().unwrap();
    let global = fixture_root.path().join("captured-installation");
    let installation =
        InstallationPaths::resolve(Brand::default(), fixture_root.path(), None, Some(&global))
            .unwrap();
    let cancellation = Cancellation::default();
    let context = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let bytes = fixture(ArtifactType::Skill, &context);
    let manifest = RemoteArtifactManifest::decode(&bytes, limits(), &context).unwrap();
    let consumer: ProjectId = CONSUMER.parse().unwrap();
    assert_ne!(manifest.producer(), consumer);
    let source = SourceRef::hub_cache(
        &installation,
        manifest.producer(),
        manifest.artifact_type(),
        ArtifactId::new("org/local", limits().projection).unwrap(),
        Some(manifest.id().clone()),
        manifest.versions()[0].version().clone(),
        limits().projection,
    )
    .unwrap();
    assert_eq!(source.producer(), manifest.producer());
    assert_eq!(source.id().as_str(), "org/local");
    assert_eq!(source.physical_id().as_str(), "org/tool");
    assert_eq!(source.version().as_str(), "branch/feature/query-✓");
    assert!(
        source
            .directory()
            .as_path()
            .starts_with(global.join("artifacts").join("modules").join(PRODUCER))
    );
    assert!(!global.exists());
    assert_eq!(fs::read_dir(fixture_root.path()).unwrap().count(), 0);
}

#[test]
fn all_document_capacities_have_exact_and_one_under_boundaries_in_both_directions() {
    let cancellation = Cancellation::default();
    let context = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let versions = [
        VersionInput {
            version: "branch/main",
            hash: HASH,
        },
        VersionInput {
            version: "1.2.3",
            hash: HASH,
        },
    ];
    let deps = [
        DependencyInput {
            artifact_type: ArtifactType::Command,
            id: "org/dep",
            selector: "1.2",
        },
        DependencyInput {
            artifact_type: ArtifactType::Agent,
            id: "org/agent",
            selector: "branch/main",
        },
    ];
    let input = input(ArtifactType::Skill, &versions, &deps);
    let bytes = RemoteArtifactManifest::encode(&input, limits(), &context).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let exact = MetadataLimits {
        bytes: bytes.len(),
        nodes: nodes(&value),
        scalar_bytes: 64,
        collection_entries: 11,
        versions: 2,
        dependencies: 2,
        ..limits()
    };
    assert_eq!(
        RemoteArtifactManifest::encode(&input, exact, &context).unwrap(),
        bytes
    );
    assert!(RemoteArtifactManifest::decode(&bytes, exact, &context).is_ok());
    for field in 0..6 {
        let mut under = exact;
        match field {
            0 => under.bytes -= 1,
            1 => under.nodes -= 1,
            2 => under.scalar_bytes -= 1,
            3 => under.collection_entries -= 1,
            4 => under.versions -= 1,
            5 => under.dependencies -= 1,
            _ => unreachable!(),
        }
        assert_eq!(
            RemoteArtifactManifest::encode(&input, under, &context).unwrap_err(),
            MetadataError::Limit,
            "encode field {field}"
        );
        assert_eq!(
            decode_error(&bytes, under, &context),
            MetadataError::Limit,
            "decode field {field}"
        );
    }
    let too_short_identity = MetadataLimits {
        projection: ProjectionLimits::new(7, 4096, 128, 255, 32).unwrap(),
        ..limits()
    };
    assert_eq!(
        RemoteArtifactManifest::encode(&input, too_short_identity, &context).unwrap_err(),
        MetadataError::InvalidDocument
    );
    assert_eq!(
        decode_error(&bytes, too_short_identity, &context),
        MetadataError::InvalidDocument
    );
}

#[test]
fn invalid_policy_and_large_documents_fail_with_closed_errors() {
    let cancellation = Cancellation::default();
    let context = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let versions = [VersionInput {
        version: "branch/main",
        hash: HASH,
    }];
    let input = ManifestInput {
        latest: None,
        ..input(ArtifactType::Skill, &versions, &[])
    };
    let bytes = fixture(ArtifactType::Skill, &context);
    for field in 0..6 {
        let mut invalid = limits();
        match field {
            0 => invalid.bytes = 0,
            1 => invalid.nodes = 0,
            2 => invalid.scalar_bytes = 0,
            3 => invalid.collection_entries = 0,
            4 => invalid.versions = 0,
            5 => invalid.dependencies = 0,
            _ => unreachable!(),
        }
        assert_eq!(
            RemoteArtifactManifest::encode(&input, invalid, &context).unwrap_err(),
            MetadataError::InvalidPolicy
        );
        assert_eq!(
            decode_error(&bytes, invalid, &context),
            MetadataError::InvalidPolicy
        );
    }
    let huge = vec![b'x'; 1_048_576];
    assert_eq!(
        decode_error(
            &huge,
            MetadataLimits {
                bytes: 1,
                ..limits()
            },
            &context
        ),
        MetadataError::Limit
    );
    let error = decode_error(
        b"{\"unknown-secret-payload\":{\"bearer\":\"private-sentinel\"}}",
        limits(),
        &context,
    );
    assert_eq!(error, MetadataError::InvalidDocument);
    assert!(!format!("{error:?} {error}").contains("private-sentinel"));
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn wire_rejects_duplicate_escaped_fields_unknown_null_missing_and_trailing_data() {
    let cancellation = Cancellation::default();
    let context = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let bytes = fixture(ArtifactType::Skill, &context);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for field in [
        "v",
        "producer",
        "artifact_type",
        "id",
        "name",
        "description",
        "tags",
        "author",
        "latest",
        "versions",
        "dependencies",
    ] {
        let mut bad = value.clone();
        bad[field] = Value::Null;
        assert_eq!(
            decode_error(&serde_json::to_vec(&bad).unwrap(), limits(), &context),
            MetadataError::InvalidDocument,
            "null {field}"
        );
        let mut bad = value.clone();
        bad.as_object_mut().unwrap().remove(field);
        if [
            "v",
            "producer",
            "artifact_type",
            "id",
            "name",
            "versions",
            "dependencies",
        ]
        .contains(&field)
        {
            assert_eq!(
                decode_error(&serde_json::to_vec(&bad).unwrap(), limits(), &context),
                MetadataError::InvalidDocument,
                "missing {field}"
            );
        } else {
            assert!(
                RemoteArtifactManifest::decode(
                    &serde_json::to_vec(&bad).unwrap(),
                    limits(),
                    &context
                )
                .is_ok()
            );
        }
        let field_value = serde_json::to_string(&value[field]).unwrap();
        let mut duplicated = bytes[..bytes.len() - 1].to_vec();
        duplicated.extend_from_slice(format!(",{field:?}:{field_value}}}").as_bytes());
        assert_eq!(
            decode_error(&duplicated, limits(), &context),
            MetadataError::InvalidDocument,
            "duplicate {field}"
        );
    }
    for extra in [b"  false".as_slice(), b"{}", b"[]", b"\0"] {
        let mut bad = bytes.clone();
        bad.extend_from_slice(extra);
        assert_eq!(
            decode_error(&bad, limits(), &context),
            MetadataError::InvalidDocument
        );
    }
    let wire = String::from_utf8(bytes.clone()).unwrap();
    let duplicate = wire.replace(
        "\"name\":\"tool\"",
        "\"name\":\"tool\",\"na\\u006de\":\"tool\"",
    );
    assert_eq!(
        decode_error(duplicate.as_bytes(), limits(), &context),
        MetadataError::InvalidDocument
    );
    for (old, new) in [
        (
            "\"username\":\"publisher\"",
            "\"username\":\"publisher\",\"username\":\"publisher\"",
        ),
        (
            "\"version\":\"1.2.3\"",
            "\"version\":\"1.2.3\",\"version\":\"1.2.3\"",
        ),
        (
            "\"selector\":\"1.2\"",
            "\"selector\":\"1.2\",\"selector\":\"1.2\"",
        ),
    ] {
        assert_eq!(
            decode_error(wire.replace(old, new).as_bytes(), limits(), &context),
            MetadataError::InvalidDocument
        );
    }
    for target in [
        vec!["foreign"],
        vec!["author", "foreign"],
        vec!["versions", "0", "foreign"],
        vec!["dependencies", "0", "foreign"],
    ] {
        let mut bad = value.clone();
        let mut cursor = &mut bad;
        for part in &target[..target.len() - 1] {
            cursor = if *part == "0" {
                &mut cursor[0]
            } else {
                &mut cursor[*part]
            };
        }
        cursor[target[target.len() - 1]] = json!({"unexpected": ["private-sentinel"]});
        assert_eq!(
            decode_error(&serde_json::to_vec(&bad).unwrap(), limits(), &context),
            MetadataError::InvalidDocument
        );
    }
    let escaped = wire.replace("\"org/tool\"", "\"org\\u002ftool\"");
    assert_eq!(
        RemoteArtifactManifest::decode(escaped.as_bytes(), limits(), &context)
            .unwrap()
            .id()
            .as_str(),
        "org/tool"
    );
}

#[test]
fn identity_latest_version_and_hash_crosschecks_deny_inconsistent_publications() {
    let cancellation = Cancellation::default();
    let context = context(&cancellation, Instant::now() + Duration::from_secs(10));
    let bytes = fixture(ArtifactType::Skill, &context);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for (key, replacement) in [
        ("v", json!(2)),
        ("v", json!("1")),
        ("producer", json!(CONSUMER.to_lowercase())),
        ("producer", json!("00000000000000000000000000")),
        ("artifact_type", json!("Skill")),
        ("artifact_type", json!("unknown")),
        ("id", json!("../tool")),
        ("name", json!("a/b")),
        ("latest", json!("9.9.9")),
        ("latest", json!("latest")),
        ("versions", json!([])),
    ] {
        let mut bad = value.clone();
        bad[key] = replacement;
        assert_eq!(
            decode_error(&serde_json::to_vec(&bad).unwrap(), limits(), &context),
            MetadataError::InvalidDocument,
            "{key}"
        );
    }
    for digest in [
        "",
        "1234",
        "A123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde/",
    ] {
        let mut bad = value.clone();
        bad["versions"][0]["hash"] = json!(digest);
        assert_eq!(
            decode_error(&serde_json::to_vec(&bad).unwrap(), limits(), &context),
            MetadataError::InvalidDocument
        );
        let versions = [VersionInput {
            version: "1.2.3",
            hash: digest,
        }];
        assert_eq!(
            RemoteArtifactManifest::encode(
                &input(ArtifactType::Skill, &versions, &[]),
                limits(),
                &context
            )
            .unwrap_err(),
            MetadataError::InvalidDocument
        );
    }
    let mut duplicate = value.clone();
    duplicate["versions"][0]["version"] = json!("1.2.3");
    assert_eq!(
        decode_error(&serde_json::to_vec(&duplicate).unwrap(), limits(), &context),
        MetadataError::InvalidDocument
    );
    let versions = [
        VersionInput {
            version: "1.2.3",
            hash: HASH,
        },
        VersionInput {
            version: "1.2.3",
            hash: HASH,
        },
    ];
    assert_eq!(
        RemoteArtifactManifest::encode(
            &input(ArtifactType::Skill, &versions, &[]),
            limits(),
            &context
        )
        .unwrap_err(),
        MetadataError::InvalidDocument
    );
}

#[test]
fn existing_request_cancellation_and_deadline_are_not_replaced_or_reset() {
    let cancellation = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    let context = context(&cancellation, deadline);
    let bytes = fixture(ArtifactType::Skill, &context);
    let versions = [VersionInput {
        version: "1.2.3",
        hash: HASH,
    }];
    let input = input(ArtifactType::Skill, &versions, &[]);
    cancellation.cancel();
    assert_eq!(
        decode_error(&bytes, limits(), &context),
        MetadataError::Cancelled
    );
    assert_eq!(
        RemoteArtifactManifest::encode(&input, limits(), &context).unwrap_err(),
        MetadataError::Cancelled
    );
    assert_eq!(context.deadline().unwrap(), deadline);
    let fresh = Cancellation::default();
    let deadline = Instant::now() + Duration::from_millis(100);
    let expired = super::tests::context(&fresh, deadline);
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(
        decode_error(&bytes, limits(), &expired),
        MetadataError::Deadline
    );
    assert_eq!(
        RemoteArtifactManifest::encode(&input, limits(), &expired).unwrap_err(),
        MetadataError::Deadline
    );
    assert_eq!(expired.deadline().unwrap(), deadline);
}
