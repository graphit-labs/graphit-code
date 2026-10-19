use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema},
    native_nn::{NativeNnError, NativeNnSource},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_graph::{
        AstGraphError, AstGraphJsonInput, AstGraphRelationTarget, AstGraphValue,
        AstSemanticGraphBatch, assemble_graph_json,
    },
    semantic_model::{JsonSemanticDocument, ParsedJsonDocument},
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::builtin,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::{error::Error, sync::Arc};
use tempfile::TempDir;
type TestResult = Result<(), Box<dyn Error>>;
fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}
fn limits() -> IdentityLimits {
    IdentityLimits::new(IdentityLimitsInput {
        source_bytes: 1 << 20,
        path_components: 16,
        path_component_bytes: 1024,
        identity_text_bytes: 32768,
        owner_depth: 64,
        uid_bytes: 1 << 20,
    })
    .unwrap()
}
fn catalog(bytes: &[u8]) -> CatalogDraft {
    let original = context();
    let id = ProfileId::new("model-fixture", 256).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    let decoded = [decoder
        .decode(
            ProfileInput { id: &id, bytes },
            &ProfileLimits::new(ProfileLimitsInput {
                input_bytes: 1 << 20,
                nodes: 32768,
                depth: 64,
                text_bytes: 1 << 20,
                scalar_bytes: 32768,
                collection_entries: 4096,
            })
            .unwrap(),
            &original,
        )
        .unwrap()];
    CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &decoded,
        }],
        &CatalogLimits::new(CatalogLimitsInput {
            policy_entries: 4096,
            selector_bytes: 32768,
        })
        .unwrap(),
        &original,
    )
    .unwrap()
}
fn with_source(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let span = ByteSpan {
        start: 0,
        end: u64::try_from(text.len())?,
    };
    let segments = if text.is_empty() {
        vec![]
    } else {
        vec![OriginSegment::Direct {
            prepared: span,
            original: span,
        }]
    };
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        original,
    )?;
    action(&input)
}

fn with_model(
    text: &str,
    original: &SnapshotContext,
    draft: &CatalogDraft,
    path: &[&str],
    action: impl FnOnce(&JsonSemanticDocument<'_, '_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    with_source(text, original, |input| {
        let factory = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin(draft.profiles()[0].grammar().value())?,
            original,
        )?;
        let parsed = ParsedJsonDocument::collect(&factory, input, path, limits())?;
        let document = parsed.assemble()?;
        action(&document)
    })
}
fn custom(queries: &str) -> CatalogDraft {
    catalog(format!("language: json\ngrammar: tree-sitter-json\ncontext_types: {{pair: Pair}}\ncontext_name_queries: {{pair: '(pair key: (_) @name) @context'}}\nembed_labels: [Pair, Value, Custom]\nqueries:\n{queries}").as_bytes())
}
async fn raw(
    temp: &TempDir,
    text: &str,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
    let uri = temp.path().join("raw.lance");
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["model.json"])),
            Arc::new(StringArray::from(vec![text])),
        ],
    )?;
    LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![batch]).await?;
    let target = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
        .await?
        .capture_target(Some("fixture"))
        .await?
        .materialize()
        .await?;
    Ok(read_branch_source(&target, "model.json", original).await?)
}
fn text_property<'a>(node: &'a graphit_ast::semantic_graph::AstGraphNode, key: &str) -> &'a str {
    let AstGraphValue::String(value) = node.properties().get(key).unwrap() else {
        panic!("string property {key}")
    };
    value
}
fn int_property(node: &graphit_ast::semantic_graph::AstGraphNode, key: &str) -> i64 {
    let AstGraphValue::Int64(value) = node.properties().get(key).unwrap() else {
        panic!("integer property {key}")
    };
    *value
}

#[tokio::test]
async fn json_graph_preserves_stock_entities_original_ranges_and_owned_parts() -> TestResult {
    fn assert_send<T: Send>() {}
    assert_send::<AstSemanticGraphBatch>();
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let source = "{\r\n\"outer\":{\"a\":1,\"b\":1},\"s\":\"x\",\"flag\":false,\"items\":[1,\"x\",true,null]}\r\n";
    let raw = raw(&temp, source, &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/json.yaml"));
    let mut owned = None;
    with_model(text, &original, &draft, &["model.json"], |document| {
        let graph = assemble_graph_json(
            AstGraphJsonInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), 14);
        assert_eq!(graph.nodes().len(), document.entities().len());
        assert!(graph.edges().is_empty());
        for (node, entity) in graph.nodes().iter().zip(document.entities()) {
            assert_eq!(node.label(), entity.identity().label());
            assert_eq!(text_property(node, "name"), entity.name());
            assert_eq!(text_property(node, "raw_name"), entity.name_raw().text());
            assert_eq!(text_property(node, "lang"), "json");
            assert_eq!(text_property(node, "name_representation"), "JsonName");
            assert_eq!(int_property(node, "line_number"), 2);
            let pair: Vec<String> = serde_json::from_str(node.key())?;
            assert_eq!(pair, [entity.identity().uid().as_str(), entity.data_key()]);
            let metadata: serde_json::Value =
                serde_json::from_str(text_property(node, "metadata_json"))?;
            assert_eq!(
                metadata["contributions"].as_array().unwrap().len(),
                entity.contributions().len()
            );
            assert!(metadata["contributions"][0]["name_slot"].is_number());
            assert!(metadata["contributions"][0]["query_capture_names"].is_array());
            let start = usize::try_from(int_property(node, "name_start_byte"))?;
            let end = usize::try_from(int_property(node, "name_end_byte"))?;
            assert_eq!(&source[start..end], entity.name_raw().text());
            if node.label() == "Pair" && entity.name() == "a" {
                assert_eq!(metadata["namespaces"][0]["name"], "outer");
                assert!(metadata["contributions"].as_array().unwrap().iter().any(
                    |c| c["declared_value"]["literal"]["value"]["validated_number_lexeme"] == "1"
                ));
            }
        }
        owned = Some(graph);
        Ok(())
    })?;
    let parts = std::thread::spawn(move || owned.unwrap().into_parts())
        .join()
        .unwrap();
    assert_eq!(parts.association.raw_version(), raw.version());
    assert_eq!(parts.association.path(), "model.json");
    assert_eq!(parts.nodes.len(), 14);
    for node in parts.nodes {
        let (label, key, properties) = node.into_parts();
        assert!(!label.is_empty());
        assert_eq!(
            properties.get("lang"),
            Some(&AstGraphValue::String("json".into()))
        );
        assert_eq!(serde_json::from_str::<Vec<String>>(&key)?.len(), 2);
    }
    Ok(())
}
#[tokio::test]
async fn json_graph_preserves_empty_nul_escaped_names_number_lexemes_and_raw_relations()
-> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let source = r#"{"":"x\n","\u0000":1e9999,"a\u0062":123456789012345678901234567890,"flag":false,"nil":null}"#;
    let raw = raw(&temp, source, &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft = custom(
        "  - data_key: one\n    graph_label: Custom\n    pattern: '(pair key:(string) @name value:[(string) (number) (false) (null)] @value)'\n    value_capture: value\n    value_label: Value\n  - data_key: two\n    graph_label: Custom\n    pattern: '(pair key:(string) @name value:[(string) (number) (false) (null)] @value)'\n    value_capture: value\n    value_label: Value\n  - type: relation\n    data_key: raw\n    relation_type: CALLS\n    pattern: '(pair key:(string) @name)'\n  - type: relation\n    data_key: reference\n    relation_type: REFERENCES\n    pattern: '(number) @name'\ntarget_rules:\n  CALLS:\n    labels: [Custom]\n    fallback: Symbol\n",
    );
    with_model(text, &original, &draft, &["model.json"], |document| {
        let graph = assemble_graph_json(
            AstGraphJsonInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), 20);
        assert!(graph.edges().is_empty());
        assert_eq!(graph.outcomes().len(), 7);
        for outcome in graph.outcomes() {
            let raw: serde_json::Value = serde_json::from_str(outcome.observation_json())?;
            if raw["relation_type"] == "CALLS" {
                assert_eq!(raw["target_rule"]["labels"], serde_json::json!(["Custom"]));
                assert_eq!(raw["target_rule"]["fallback"], "Symbol");
            } else {
                assert_eq!(raw["relation_type"], "REFERENCES");
                assert!(raw["target_rule"].is_null());
            }
        }
        assert!(
            graph
                .outcomes()
                .iter()
                .all(|o| matches!(o.target(), AstGraphRelationTarget::ObservedOnly))
        );
        assert!(graph.diagnostics_json().contains("UnsupportedQueryKind"));
        for name in ["", "\0", "ab", "flag", "nil"] {
            let nodes: Vec<_> = graph
                .nodes()
                .iter()
                .filter(|n| n.label() == "Custom" && text_property(n, "name") == name)
                .collect();
            assert_eq!(nodes.len(), 2);
            assert_eq!(
                text_property(nodes[0], "uid"),
                text_property(nodes[1], "uid")
            );
            assert_ne!(nodes[0].key(), nodes[1].key());
            let meta: serde_json::Value =
                serde_json::from_str(text_property(nodes[0], "metadata_json"))?;
            let literal = &meta["contributions"][0]["declared_value"]["literal"];
            match name {
                "" => assert_eq!(literal["value"]["decoded_value"], "x\n"),
                "\0" => assert_eq!(literal["value"]["validated_number_lexeme"], "1e9999"),
                "ab" => assert_eq!(
                    literal["value"]["validated_number_lexeme"],
                    "123456789012345678901234567890"
                ),
                "flag" => assert_eq!(literal["value"]["decoded_value"], false),
                "nil" => assert!(literal["value"]["decoded_value"].is_null()),
                _ => unreachable!(),
            }
            assert!(literal["literal_source"].is_string());
        }
        Ok(())
    })
}
#[tokio::test]
async fn json_graph_preserves_conflicts_and_rejects_foreign_source_and_cancellation() -> TestResult
{
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let source = r#"{"k":[1,1.0]}"#;
    let raw = raw(&temp, source, &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft = custom(
        "  - data_key: pair\n    graph_label: Pair\n    pattern: '(pair key:(string) @name value:(array (number) @value))'\n    value_capture: value\n    value_label: Value\n",
    );
    with_model(text, &original, &draft, &["model.json"], |document| {
        let graph = assemble_graph_json(
            AstGraphJsonInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), 3);
        let node = graph.nodes().iter().find(|n| n.label() == "Pair").unwrap();
        assert_eq!(
            node.properties().get("available"),
            Some(&AstGraphValue::Bool(false))
        );
        let meta: serde_json::Value = serde_json::from_str(text_property(node, "metadata_json"))?;
        assert_eq!(meta["contributions"].as_array().unwrap().len(), 2);
        assert!(meta["conflicts"].as_str().unwrap().contains("Value"));
        let other = Arc::new(context());
        assert!(
            matches!(assemble_graph_json(AstGraphJsonInput{document,source:NativeNnSource::Indexed(&raw)},&other),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::RequestMismatch))
        );
        Ok(())
    })?;
    let copy = text.to_owned();
    for (source, path) in [
        (copy.as_str(), vec!["model.json"]),
        (text, vec!["other.json"]),
    ] {
        with_model(source, &original, &draft, &path, |document| {
            assert!(
                matches!(assemble_graph_json(AstGraphJsonInput{document,source:NativeNnSource::Indexed(&raw)},&original),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::InvalidInput))
            );
            Ok(())
        })?;
    }
    with_model(text, &original, &draft, &["model.json"], |document| {
        original.cancellation().cancel();
        assert!(
            matches!(assemble_graph_json(AstGraphJsonInput{document,source:NativeNnSource::Indexed(&raw)},&original),Err(AstGraphError::Operation(e)) if e.kind()==std::io::ErrorKind::Interrupted)
        );
        Ok(())
    })
}

#[tokio::test]
async fn native_json_graph_full_wire_is_stable_across_independent_reparse() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let raw = raw(&temp, r#"{"a":1}"#, &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft = custom(
        "  - data_key: pairs\n    graph_label: Pair\n    pattern: '(pair key:(string) @name value:(number) @value)'\n    value_capture: value\n    value_label: Value\n  - type: relation\n    data_key: raw\n    relation_type: CALLS\n    pattern: '(pair key:(string) @name)'\n",
    );
    with_model(text, &original, &draft, &["model.json"], |left| {
        with_model(text, &original, &draft, &["model.json"], |right| {
            assert_ne!(
                left.entities()[0].contributions()[0]
                    .name_capture
                    .native
                    .node_id,
                right.entities()[0].contributions()[0]
                    .name_capture
                    .native
                    .node_id
            );
            let first = assemble_graph_json(
                AstGraphJsonInput {
                    document: left,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            let second = assemble_graph_json(
                AstGraphJsonInput {
                    document: right,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            assert_eq!(first.nodes(), second.nodes());
            assert_eq!(first.edges(), second.edges());
            assert_eq!(first.file_observation(), second.file_observation());
            assert_eq!(first.outcomes(), second.outcomes());
            assert_eq!(first.association(), second.association());
            assert_eq!(first.diagnostics_json(), second.diagnostics_json());
            assert_eq!(first.capabilities_json(), second.capabilities_json());
            assert!(first.edges().is_empty());
            assert_eq!(first.outcomes().len(), 1);
            assert!(matches!(
                first.outcomes()[0].target(),
                AstGraphRelationTarget::ObservedOnly
            ));
            for node in first.nodes() {
                let metadata: serde_json::Value =
                    serde_json::from_str(text_property(node, "metadata_json"))?;
                for contribution in metadata["contributions"].as_array().unwrap() {
                    assert!(contribution.get("sdk_name_address").is_none());
                }
            }
            let observation: serde_json::Value =
                serde_json::from_str(first.outcomes()[0].observation_json())?;
            for capture in observation["captures"].as_array().unwrap() {
                assert!(capture.get("sdk_node_id").is_none());
            }
            Ok(())
        })
    })
}
