use super::*;
use crate::{ProjectionLimits, native::ContentLimits, remote_storage::hash_reader};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use serde_json::{Value, json};
use std::{
    io::Cursor,
    time::{Duration, Instant},
};

const PRODUCER: &str = "01KSH1CRFFG8Z74B5ZS78WW808";
fn limits() -> MetadataLimits {
    MetadataLimits {
        bytes: 64 * 1024,
        nodes: 4096,
        scalar_bytes: 1024,
        collection_entries: 32,
        versions: 8,
        dependencies: 8,
        projection: ProjectionLimits::new(512, 4096, 32, 255, 32).unwrap(),
    }
}
fn context(cancellation: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 64 * 1024,
            max_output_bytes: 64 * 1024,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 1024,
            max_collection_entries: 32,
        },
        cancellation,
        deadline,
    )
    .unwrap()
}
fn component(role: NativeRole) -> NativeComponent {
    NativeComponent {
        role,
        path: match role {
            NativeRole::Graph => "graph.icebug",
            NativeRole::Source => "source.lance",
            NativeRole::Wiki => "index.lance",
            NativeRole::EntitySearch => "corpus.lance",
        }
        .into(),
        snapshot: if role == NativeRole::Graph {
            NativeSnapshot::Icebug {
                manifest_path: "icebug.json".into(),
                generation: "opaque-generation/✓".into(),
                manifest_sha256: [0xab; 32],
            }
        } else {
            NativeSnapshot::Lance {
                dataset_version: 42,
                manifest_path: "_versions/42.manifest".into(),
                manifest_sha256: [0xcd; 32],
            }
        },
    }
}
fn input(kind: ArtifactType, components: &[NativeComponent]) -> NativeRootInput<'_> {
    NativeRootInput {
        producer: PRODUCER.parse().unwrap(),
        artifact_type: kind,
        id: "Org/ferramenta-✓",
        version: "branch/Feature/✓",
        components,
    }
}
fn fixture(
    components: &[NativeComponent],
    kind: ArtifactType,
    ctx: &RequestContext<'_>,
) -> Vec<u8> {
    NativeRoot::encode(&input(kind, components), limits(), ctx).unwrap()
}
fn invalid(value: &Value, ctx: &RequestContext<'_>) {
    assert_eq!(
        NativeRoot::decode(&serde_json::to_vec(value).unwrap(), limits(), ctx),
        Err(MetadataError::InvalidDocument)
    );
}

#[test]
fn typed_ast_knowledge_and_declared_optional_components_roundtrip() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(10));
    for roles in [
        vec![NativeRole::Graph],
        vec![NativeRole::Graph, NativeRole::Source],
        vec![NativeRole::Graph, NativeRole::EntitySearch],
        vec![
            NativeRole::Graph,
            NativeRole::Source,
            NativeRole::EntitySearch,
        ],
        vec![NativeRole::Wiki],
    ] {
        let components: Vec<_> = roles.iter().copied().map(component).collect();
        let kind = if roles[0] == NativeRole::Wiki {
            ArtifactType::Knowledge
        } else {
            ArtifactType::Ast
        };
        let bytes = fixture(&components, kind, &ctx);
        let root = NativeRoot::decode(&bytes, limits(), &ctx).unwrap();
        assert_eq!(root.producer().to_string(), PRODUCER);
        assert_eq!(root.artifact_type(), kind);
        assert_eq!(root.id().as_str(), "Org/ferramenta-✓");
        assert_eq!(root.version().as_str(), "branch/Feature/✓");
        assert_eq!(root.components(), components);
        assert!(
            root.components()
                .iter()
                .all(|c| !c.snapshot.manifest_path().is_empty()
                    && c.snapshot.manifest_sha256().len() == 32)
        );
    }
}

#[test]
fn strict_wire_rejects_unknown_duplicate_null_and_trailing_data() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(10));
    let bytes = fixture(&[component(NativeRole::Graph)], ArtifactType::Ast, &ctx);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for field in [
        "format",
        "v",
        "producer",
        "artifact_type",
        "id",
        "version",
        "components",
    ] {
        let mut null = value.clone();
        null[field] = Value::Null;
        invalid(&null, &ctx);
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        invalid(&missing, &ctx);
    }
    for patch in [
        json!({"format":"legacy-native-root"}),
        json!({"v":2}),
        json!({"v":1.0}),
        json!({"producer":"00000000000000000000000000"}),
        json!({"artifact_type":"language"}),
        json!({"version":"LATEST"}),
        json!({"id":"../bad"}),
        json!({"extra":true}),
    ] {
        let mut bad = value.clone();
        bad.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        invalid(&bad, &ctx);
    }
    for field in ["role", "path", "snapshot"] {
        let mut bad = value.clone();
        bad["components"][0][field] = Value::Null;
        invalid(&bad, &ctx);
    }
    for field in ["kind", "manifest_path", "generation", "manifest_sha256"] {
        let mut bad = value.clone();
        bad["components"][0]["snapshot"][field] = Value::Null;
        invalid(&bad, &ctx);
    }
    let text = String::from_utf8(bytes).unwrap();
    for bad in [
        text.replace("\"v\":1", "\"v\":1,\"v\":1"),
        text.replace("\"v\":1", "\"v\":1,\"\\u0076\":1"),
        text.replace(
            "\"kind\":\"icebug\"",
            "\"kind\":\"icebug\",\"kind\":\"icebug\"",
        ),
        format!("{text} {{}}"),
    ] {
        assert_eq!(
            NativeRoot::decode(bad.as_bytes(), limits(), &ctx),
            Err(MetadataError::InvalidDocument)
        );
    }
    let mut bad = value.clone();
    bad["components"][0]["unknown"] = json!(1);
    invalid(&bad, &ctx);
    let mut bad = value;
    bad["components"][0]["snapshot"]["unknown"] = json!(1);
    invalid(&bad, &ctx);
}

#[test]
fn required_roles_snapshot_kinds_versions_and_digests_are_closed() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(10));
    let ast: Value = serde_json::from_slice(&fixture(
        &[component(NativeRole::Graph), component(NativeRole::Source)],
        ArtifactType::Ast,
        &ctx,
    ))
    .unwrap();
    for role in ["wiki", "search", "other"] {
        let mut bad = ast.clone();
        bad["components"][0]["role"] = json!(role);
        invalid(&bad, &ctx);
    }
    let mut bad = ast.clone();
    bad["components"] = json!([]);
    invalid(&bad, &ctx);
    let mut bad = ast.clone();
    bad["components"] = json!([bad["components"][1].clone()]);
    invalid(&bad, &ctx);
    let mut bad = ast.clone();
    bad["components"][1] = bad["components"][0].clone();
    invalid(&bad, &ctx);
    let mut bad = ast.clone();
    bad["artifact_type"] = json!("knowledge");
    invalid(&bad, &ctx);
    let mut bad = ast.clone();
    bad["components"][0]["snapshot"] = bad["components"][1]["snapshot"].clone();
    invalid(&bad, &ctx);
    let mut bad = ast.clone();
    bad["components"][1]["snapshot"] = bad["components"][0]["snapshot"].clone();
    invalid(&bad, &ctx);
    for version in [json!(0), json!(-1), json!(1.0), json!("42")] {
        let mut bad = ast.clone();
        bad["components"][1]["snapshot"]["dataset_version"] = version;
        invalid(&bad, &ctx);
    }
    for digest in ["", &"AB".repeat(32), &"g0".repeat(32), &"0".repeat(63)] {
        let mut bad = ast.clone();
        bad["components"][0]["snapshot"]["manifest_sha256"] = json!(digest);
        invalid(&bad, &ctx);
    }
    let mut bad = ast.clone();
    bad["components"][0]["snapshot"]["generation"] = json!("");
    invalid(&bad, &ctx);
    let mut bad = ast.clone();
    bad["components"][0]["snapshot"]["manifest_path"] = json!("other.json");
    invalid(&bad, &ctx);
    let mut valid = ast;
    valid["components"][1]["snapshot"]["dataset_version"] = json!(u64::MAX);
    assert!(NativeRoot::decode(&serde_json::to_vec(&valid).unwrap(), limits(), &ctx).is_ok());
}

#[test]
fn relative_roots_and_manifests_cannot_escape_or_overlap() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(10));
    let base: Value = serde_json::from_slice(&fixture(
        &[component(NativeRole::Graph), component(NativeRole::Source)],
        ArtifactType::Ast,
        &ctx,
    ))
    .unwrap();
    for path in [
        "",
        "/root",
        "../x",
        "x/../y",
        "x//y",
        "x/./y",
        "x/",
        "x\\y",
        "C:/x",
        "s3://bucket/x",
        "x\0y",
        "x\ny",
    ] {
        let mut bad = base.clone();
        bad["components"][1]["path"] = json!(path);
        invalid(&bad, &ctx);
        let mut bad = base.clone();
        bad["components"][1]["snapshot"]["manifest_path"] = json!(path);
        invalid(&bad, &ctx);
    }
    for path in ["graph.icebug", "graph.icebug/child"] {
        let mut bad = base.clone();
        bad["components"][1]["path"] = json!(path);
        invalid(&bad, &ctx);
    }
    let mut valid = base.clone();
    valid["components"][0]["path"] = json!("graph");
    valid["components"][1]["path"] = json!("graph-extra/源%2f.lance");
    let root = NativeRoot::decode(&serde_json::to_vec(&valid).unwrap(), limits(), &ctx).unwrap();
    assert_eq!(root.components()[1].path, "graph-extra/源%2f.lance");
    valid["components"][1]["path"] = json!("graph/child");
    invalid(&valid, &ctx);
}

#[test]
fn exact_limits_and_original_cancellation_deadline_apply_to_both_directions() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(10));
    let components = [
        component(NativeRole::Graph),
        component(NativeRole::Source),
        component(NativeRole::EntitySearch),
    ];
    let input = input(ArtifactType::Ast, &components);
    let bytes = NativeRoot::encode(&input, limits(), &ctx).unwrap();
    let mut exact = limits();
    exact.bytes = bytes.len();
    exact.nodes = 60;
    exact.scalar_bytes = 64;
    exact.collection_entries = 7;
    assert_eq!(NativeRoot::encode(&input, exact, &ctx).unwrap(), bytes);
    assert!(NativeRoot::decode(&bytes, exact, &ctx).is_ok());
    for cap in [
        MetadataLimits {
            bytes: exact.bytes - 1,
            ..exact
        },
        MetadataLimits { nodes: 59, ..exact },
        MetadataLimits {
            scalar_bytes: 63,
            ..exact
        },
        MetadataLimits {
            collection_entries: 6,
            ..exact
        },
    ] {
        assert_eq!(
            NativeRoot::encode(&input, cap, &ctx),
            Err(MetadataError::Limit)
        );
        assert_eq!(
            NativeRoot::decode(&bytes, cap, &ctx),
            Err(MetadataError::Limit)
        );
    }
    let zero = MetadataLimits {
        dependencies: 0,
        ..exact
    };
    assert_eq!(
        NativeRoot::encode(&input, zero, &ctx),
        Err(MetadataError::InvalidPolicy)
    );
    assert_eq!(
        NativeRoot::decode(&bytes, zero, &ctx),
        Err(MetadataError::InvalidPolicy)
    );
    cancel.cancel();
    assert_eq!(
        NativeRoot::encode(&input, exact, &ctx),
        Err(MetadataError::Cancelled)
    );
    assert_eq!(
        NativeRoot::decode(b"bad", exact, &ctx),
        Err(MetadataError::Cancelled)
    );
    let fresh = Cancellation::default();
    let expired = context(&fresh, Instant::now() + Duration::from_millis(50));
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(
        NativeRoot::encode(&input, exact, &expired),
        Err(MetadataError::Deadline)
    );
    assert_eq!(
        NativeRoot::decode(b"bad", exact, &expired),
        Err(MetadataError::Deadline)
    );
}

#[test]
fn shared_hash_binds_exact_manifest_and_final_descriptor_bytes() {
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    let original = SnapshotContext::new(deadline, cancel.clone());
    let ctx = context(&cancel, deadline);
    let manifest = b"actual physical manifest bytes";
    let manifest_hash = hash_reader(&mut Cursor::new(manifest), &original).unwrap();
    let mut graph = component(NativeRole::Graph);
    if let NativeSnapshot::Icebug {
        manifest_sha256, ..
    } = &mut graph.snapshot
    {
        *manifest_sha256 = manifest_hash;
    }
    let bytes = fixture(&[graph], ArtifactType::Ast, &ctx);
    let root = NativeRoot::decode(&bytes, limits(), &ctx).unwrap();
    assert_eq!(
        root.components()[0].snapshot.manifest_sha256(),
        &manifest_hash
    );
    let root_hash = hash_reader(&mut Cursor::new(&bytes), &original).unwrap();
    let mut padded = bytes.clone();
    padded.push(b'\n');
    assert_eq!(NativeRoot::decode(&padded, limits(), &ctx).unwrap(), root);
    assert_ne!(
        hash_reader(&mut Cursor::new(padded), &original).unwrap(),
        root_hash
    );
    assert_ne!(manifest_hash, root_hash);
}

#[test]
fn knowledge_has_one_wiki_pin_and_no_separate_semantic_metadata() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(10));
    let components = [component(NativeRole::Wiki)];
    let input = input(ArtifactType::Knowledge, &components);
    let bytes = NativeRoot::encode(&input, limits(), &ctx).unwrap();
    let root = NativeRoot::decode(&bytes, limits(), &ctx).unwrap();
    assert_eq!(root.components(), components);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value.get("knowledge_pair").is_none());
    for pair in [Value::Null, json!({"source":{}})] {
        let mut bad = value.clone();
        bad["knowledge_pair"] = pair;
        invalid(&bad, &ctx);
    }
    for role in ["graph", "source", "search", "entity_search"] {
        let mut bad = value.clone();
        bad["components"][0]["role"] = json!(role);
        invalid(&bad, &ctx);
    }
    for role in [
        NativeRole::Graph,
        NativeRole::Source,
        NativeRole::EntitySearch,
        NativeRole::Wiki,
    ] {
        let mut bad = value.clone();
        bad["components"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(component(role)).unwrap());
        invalid(&bad, &ctx);
    }
    for path in ["wiki.lance", "embeddings.lance", "nested/index.lance"] {
        let mut bad = value.clone();
        bad["components"][0]["path"] = json!(path);
        invalid(&bad, &ctx);
    }
    let mut bad = value.clone();
    bad["components"][0]["snapshot"]["dataset_version"] = json!(0);
    invalid(&bad, &ctx);
    let mut low_nodes = limits();
    low_nodes.nodes = 29;
    assert_eq!(
        NativeRoot::encode(&input, low_nodes, &ctx),
        Err(MetadataError::Limit)
    );
    let mut low_entries = limits();
    low_entries.collection_entries = 6;
    assert_eq!(
        NativeRoot::decode(&bytes, low_entries, &ctx),
        Err(MetadataError::Limit)
    );
}

#[test]
fn ast_corpus_has_one_text_nn_lance_pin_and_is_not_a_knowledge_role() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel, Instant::now() + Duration::from_secs(10));
    let mut lexical = component(NativeRole::EntitySearch);
    lexical.snapshot = NativeSnapshot::Lance {
        dataset_version: 73,
        manifest_path: "_versions/00000073.manifest".into(),
        manifest_sha256: [0x73; 32],
    };
    let components = [
        component(NativeRole::Graph),
        component(NativeRole::Source),
        lexical.clone(),
    ];
    let bytes = fixture(&components, ArtifactType::Ast, &ctx);
    let parsed = NativeRoot::decode(&bytes, limits(), &ctx).unwrap();
    assert_eq!(parsed.components().last(), Some(&lexical));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for path in ["entity_search.lance", "search.lance", "native_nn.lance"] {
        let mut bad = value.clone();
        bad["components"][2]["path"] = json!(path);
        invalid(&bad, &ctx);
    }
    let knowledge_components = [component(NativeRole::Wiki), lexical];
    let knowledge = input(ArtifactType::Knowledge, &knowledge_components);
    assert!(NativeRoot::encode(&knowledge, limits(), &ctx).is_err());
}
