use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema},
    native_nn::NativeNnSource,
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_graph::{AstGraphJsonInput, assemble_graph_json},
    semantic_graph::{AstGraphTsInput, AstGraphValue, assemble_graph_ts},
    semantic_model::ParsedJsonDocument,
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::builtin,
    tree_sitter_model::{ParsedTreeSitterDocument, TreeSitterSemanticDocument},
};
use graphit_ast::{
    indexed_source::read_indexed_source,
    native_nn::NativeNnError,
    semantic_graph::{
        AstGraphError, AstGraphModelUnavailableReason, AstGraphUnavailableFileInput,
        observe_unavailable_graph_file,
    },
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use std::{collections::BTreeSet, error::Error, sync::Arc};
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
    action: impl FnOnce(&TreeSitterSemanticDocument<'_, '_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    with_source(text, original, |input| {
        let factory = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin(draft.profiles()[0].grammar().value())?,
            original,
        )?;
        let parsed = ParsedTreeSitterDocument::collect(&factory, input, path, limits())?;
        let document = parsed.assemble()?;
        action(&document)
    })
}
fn stock() -> CatalogDraft {
    catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/go.yaml"
    ))
}
async fn raw_snapshot(
    temp: &TempDir,
    text: &str,
    original: &Arc<SnapshotContext>,
    path: &str,
) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
    let uri = temp.path().join("raw.lance");
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec![path])),
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
    Ok(read_branch_source(&target, path, original).await?)
}
fn text_property<'a>(node: &'a graphit_ast::semantic_graph::AstGraphNode, key: &str) -> &'a str {
    let AstGraphValue::String(value) = node.properties().get(key).unwrap() else {
        panic!("string property {key}")
    };
    value
}

#[tokio::test]
async fn file_observation_preserves_all_native_relation_states_once_per_file() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let source = "package p\ntype I interface {Read() int}\ntype Box struct {A int}\nfunc G(){}\nfunc F(){}\nfunc F(){}\nfunc Caller(b *Box,i I){G();F();Missing();i.Read();_=b.A;b.A=1;func(){G()}()}\n";
    let raw = raw_snapshot(&temp, source, &original, "model.go").await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft = stock();
    with_model(text, &original, &draft, &["model.go"], |document| {
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        let observation = graph.file_observation();
        assert_eq!(observation.label(), "AstFileObservation");
        let key: serde_json::Value = serde_json::from_str(observation.key())?;
        assert_eq!(
            key,
            serde_json::json!(["file_observation", "model.go", ["go", "tree-sitter-go"]])
        );
        assert!(!observation.properties().contains_key("uid"));
        assert!(!observation.properties().contains_key("data_key"));
        assert!(!observation.properties().contains_key("start_byte"));
        assert_eq!(
            text_property(observation, "path"),
            graph.association().path()
        );
        assert_eq!(
            text_property(observation, "raw_version"),
            raw.version().to_string()
        );
        assert_eq!(
            text_property(observation, "diagnostics_json"),
            graph.diagnostics_json()
        );
        assert_eq!(
            text_property(observation, "capabilities_json"),
            graph.capabilities_json()
        );
        assert_eq!(
            observation.properties().get("source_stored"),
            Some(&AstGraphValue::Bool(true))
        );
        let relations: serde_json::Value =
            serde_json::from_str(text_property(observation, "resolution_outcomes_json"))?;
        assert_eq!(relations.as_array().unwrap().len(), graph.outcomes().len());
        let states: BTreeSet<_> = relations
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["target"]["state"].as_str().unwrap())
            .collect();
        for state in ["Resolved", "NotFound", "Ambiguous", "Unresolved"] {
            assert!(states.contains(state), "{state}: {relations}");
        }
        for outcome in relations.as_array().unwrap() {
            assert!(outcome["observation"]["captures"].is_array());
            assert!(outcome["observation"].get("target_rule").is_some());
            if outcome["target"]["state"] == "Resolved" {
                assert!(
                    graph
                        .nodes()
                        .iter()
                        .any(|n| Some(n.key()) == outcome["target"]["key"].as_str())
                );
            }
        }
        let parts = graph.into_parts();
        let records: Vec<_> = parts
            .nodes
            .into_iter()
            .chain(std::iter::once(parts.file_observation))
            .map(|n| n.into_parts())
            .collect();
        assert_eq!(
            records
                .iter()
                .filter(|(_, _, props)| props.get("is_file_observation")
                    == Some(&AstGraphValue::Bool(true)))
                .count(),
            1
        );
        for (_, key, properties) in records {
            let key: serde_json::Value = serde_json::from_str(&key)?;
            assert_eq!(
                key.as_array().unwrap().len(),
                if properties.get("is_file_observation") == Some(&AstGraphValue::Bool(true)) {
                    3
                } else {
                    2
                }
            );
        }
        Ok(())
    })
}
#[tokio::test]
async fn file_observation_survives_empty_source_zero_entities_and_json_diagnostics() -> TestResult {
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let raw = raw_snapshot(&temp, "", &original, "model.go").await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("stored empty")
    };
    with_model(text, &original, &stock(), &["model.go"], |document| {
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert!(graph.nodes().is_empty());
        assert!(graph.edges().is_empty());
        assert_eq!(
            graph.file_observation().properties().get("source_empty"),
            Some(&AstGraphValue::Bool(true))
        );
        assert_eq!(
            text_property(graph.file_observation(), "relative_components_json"),
            "[\"model.go\"]"
        );
        assert_eq!(
            text_property(graph.file_observation(), "physical_branch"),
            "fixture"
        );
        assert_eq!(
            text_property(graph.file_observation(), "source_sha256").len(),
            64
        );
        Ok(())
    })?;
    let temp = TempDir::new()?;
    let raw = raw_snapshot(&temp, r#"{"a":1}"#, &original, "model.json").await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft=catalog(b"language: json\ngrammar: tree-sitter-json\ncontext_types: {pair: Pair}\ncontext_name_queries: {pair: '(pair key: (_) @name) @context'}\nqueries:\n  - type: relation\n    data_key: calls\n    relation_type: CALLS\n    pattern: '(pair key:(string) @name)'\ntarget_rules:\n  CALLS:\n    labels: [Pair]\n    fallback: Value\n");
    with_source(text, &original, |input| {
        let factory = PreparedTreeSitterContexts::prepare(
            &draft.profiles()[0],
            builtin("tree-sitter-json")?,
            &original,
        )?;
        let parsed = ParsedJsonDocument::collect(&factory, input, &["model.json"], limits())?;
        let document = parsed.assemble()?;
        let graph = assemble_graph_json(
            AstGraphJsonInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert!(graph.nodes().is_empty());
        assert!(graph.edges().is_empty());
        let observation = graph.file_observation();
        assert!(text_property(observation, "diagnostics_json").contains("UnsupportedQueryKind"));
        let outcomes: serde_json::Value =
            serde_json::from_str(text_property(observation, "resolution_outcomes_json"))?;
        assert_eq!(outcomes.as_array().unwrap().len(), 1);
        assert_eq!(outcomes[0]["target"]["state"], "ObservedOnly");
        assert_eq!(
            outcomes[0]["observation"]["target_rule"]["labels"],
            serde_json::json!(["Pair"])
        );
        assert_eq!(
            outcomes[0]["observation"]["target_rule"]["fallback"],
            "Value"
        );
        assert!(
            serde_json::from_str::<serde_json::Value>(text_property(
                observation,
                "capabilities_json"
            ))?
            .as_array()
            .is_some()
        );
        let (label, key, properties) = graph.into_parts().file_observation.into_parts();
        assert_eq!(label, "AstFileObservation");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&key)?,
            serde_json::json!([
                "file_observation",
                "model.json",
                ["json", "tree-sitter-json"]
            ])
        );
        assert_eq!(
            properties.get("source_empty"),
            Some(&AstGraphValue::Bool(false))
        );
        let custom=catalog(b"language: json\ngrammar: tree-sitter-json\ncontext_types: {pair: Pair}\ncontext_name_queries: {pair: '(pair key: (_) @name) @context'}\nqueries:\n  - data_key: custom\n    graph_label: AstFileObservation\n    pattern: '(pair key:(string) @name)'\n");
        let factory = PreparedTreeSitterContexts::prepare(
            &custom.profiles()[0],
            builtin("tree-sitter-json")?,
            &original,
        )?;
        let parsed = ParsedJsonDocument::collect(&factory, input, &["model.json"], limits())?;
        let document = parsed.assemble()?;
        let graph = assemble_graph_json(
            AstGraphJsonInput {
                document: &document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), 1);
        let semantic = &graph.nodes()[0];
        assert_eq!(semantic.label(), "AstFileObservation");
        assert!(semantic.properties().contains_key("uid"));
        assert!(semantic.properties().contains_key("data_key"));
        assert!(!semantic.properties().contains_key("is_file_observation"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(semantic.key())?
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            graph
                .file_observation()
                .properties()
                .get("is_file_observation"),
            Some(&AstGraphValue::Bool(true))
        );
        Ok(())
    })
}

#[tokio::test]
async fn unavailable_file_observation_preserves_genuine_source_and_not_evaluated_capabilities()
-> TestResult {
    let original = Arc::new(context());
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/python.yaml"
    ));
    for source in ["def f():\n    return 1\n", ""] {
        let temp = TempDir::new()?;
        let raw = raw_snapshot(&temp, source, &original, "model.py").await?;
        let node = observe_unavailable_graph_file(
            AstGraphUnavailableFileInput {
                profile: &draft.profiles()[0],
                source: NativeNnSource::Indexed(&raw),
                reason: AstGraphModelUnavailableReason::NoNativeSemanticModel,
                identity_limits: limits(),
            },
            &original,
        )?;
        assert_eq!(
            node.properties().get("model_available"),
            Some(&AstGraphValue::Bool(false))
        );
        assert_eq!(
            node.properties().get("source_stored"),
            Some(&AstGraphValue::Bool(true))
        );
        assert_eq!(
            node.properties().get("source_empty"),
            Some(&AstGraphValue::Bool(source.is_empty()))
        );
        assert_eq!(
            text_property(&node, "model_unavailable_reason"),
            "NoNativeSemanticModel"
        );
        assert_eq!(text_property(&node, "resolution_outcomes_json"), "[]");
        assert!(text_property(&node, "diagnostics_json").contains("SemanticModelUnavailable"));
        let caps: serde_json::Value =
            serde_json::from_str(text_property(&node, "capabilities_json"))?;
        assert!(!caps.as_array().unwrap().is_empty());
        for query in caps.as_array().unwrap() {
            assert_eq!(query["state"], "NotEvaluated");
            assert_eq!(query["model_available"], false);
            assert!(query.get("matches").is_none());
        }
        assert!(!node.properties().contains_key("uid"));
        assert!(!node.properties().contains_key("start_byte"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(node.key())?,
            serde_json::json!([
                "file_observation",
                "model.py",
                ["python", "tree-sitter-python"]
            ])
        );
        let other = Arc::new(context());
        assert!(
            matches!(observe_unavailable_graph_file(AstGraphUnavailableFileInput{profile:&draft.profiles()[0],source:NativeNnSource::Indexed(&raw),reason:AstGraphModelUnavailableReason::NoNativeSemanticModel,identity_limits:limits()},&other),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::Source(_)))
        );
    }
    let temp = TempDir::new()?;
    let raw = raw_snapshot(&temp, "", &original, "model.go").await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    with_model(text, &original, &stock(), &["model.go"], |document| {
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert!(graph.nodes().is_empty());
        assert_eq!(
            graph.file_observation().properties().get("model_available"),
            Some(&AstGraphValue::Bool(true))
        );
        assert!(
            !graph
                .file_observation()
                .properties()
                .contains_key("model_unavailable_reason")
        );
        Ok(())
    })?;
    let uri = temp.path().join("null_raw.lance");
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["model.py"])),
            Arc::new(StringArray::from(vec![None::<&str>])),
        ],
    )?;
    let store =
        LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![batch]).await?;
    let raw = read_indexed_source(&store.snapshot(), "model.py", &original).await?;
    assert!(
        matches!(observe_unavailable_graph_file(AstGraphUnavailableFileInput{profile:&draft.profiles()[0],source:NativeNnSource::Indexed(&raw),reason:AstGraphModelUnavailableReason::NoNativeSemanticModel,identity_limits:limits()},&original),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::InvalidInput))
    );
    let missing = read_indexed_source(&store.snapshot(), "missing.py", &original).await?;
    assert!(
        matches!(observe_unavailable_graph_file(AstGraphUnavailableFileInput{profile:&draft.profiles()[0],source:NativeNnSource::Indexed(&missing),reason:AstGraphModelUnavailableReason::NoNativeSemanticModel,identity_limits:limits()},&original),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::InvalidInput))
    );
    original.cancellation().cancel();
    assert!(
        matches!(observe_unavailable_graph_file(AstGraphUnavailableFileInput{profile:&draft.profiles()[0],source:NativeNnSource::Indexed(&raw),reason:AstGraphModelUnavailableReason::NoNativeSemanticModel,identity_limits:limits()},&original),Err(AstGraphError::Operation(e)) if e.kind()==std::io::ErrorKind::Interrupted)
    );
    Ok(())
}

#[tokio::test]
async fn unavailable_file_observation_keeps_real_physical_null_source_authority() -> TestResult {
    use graphit_ast::{
        indexed_source::read_indexed_source,
        native_nn_physical_source::capture_native_nn_physical_source,
    };
    use graphit_auth::{ProjectCapture, ScopeCapture};
    use graphit_core::{
        artifact_store::{
            CapturedPublicationScope, CopyLimits, RetainedBytesLimits, ScopeLimits,
            SourceReadLimits,
        },
        brand::Brand,
        installation_identity::InstallationIdentityStore,
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::{Checkout, VersionContext},
    };
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let timeout = std::time::Duration::from_secs(5);
    let paths = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    InstallationIdentityStore::new(&paths)?.ensure(timeout, original.cancellation())?;
    std::fs::create_dir_all(paths.global_path(&["runtime"])?)?;
    let root = temp.path().join("project");
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("model.py"), "def f():\n    return 1\n")?;
    std::fs::write(root.join("empty.py"), "")?;
    let project = ProjectPaths::at(&root, &paths)?;
    let stored = ProjectIdentityStore::at(&project)
        .writer(timeout, original.cancellation())?
        .ensure(&ProjectMetadata::new("actual-go-graph-fixture")?)?;
    std::fs::create_dir_all(project.project_runtime(stored.id(), &[])?)?;
    let scope = CapturedPublicationScope::project(
        &project,
        stored.id(),
        ScopeLimits::new(16, 4096, 256)?,
        &original,
    )?;
    let record = scope.binding()?;
    let version = VersionContext::select(&project, Checkout::NonGit, "feature", None, "")?;
    let uri = temp.path().join("null_raw.lance");
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["model.py", "empty.py"])),
            Arc::new(StringArray::from(vec![None::<&str>, None])),
        ],
    )?;
    let store =
        LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![batch]).await?;

    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/python.yaml"
    ));
    for path in ["model.py", "empty.py"] {
        let raw = read_indexed_source(&store.snapshot(), path, &original).await?;
        let physical = capture_native_nn_physical_source(
            ProjectCapture {
                scope: ScopeCapture {
                    live: &scope,
                    record: &record,
                },
                version: &version,
            },
            &[path],
            CopyLimits::new(65536, 16, 4096, 8)?,
            RetainedBytesLimits::new(1, 1, 256)?,
            SourceReadLimits::new(65536, 1024, 65536)?,
            &original,
        )?;
        let observation = observe_unavailable_graph_file(
            AstGraphUnavailableFileInput {
                profile: &draft.profiles()[0],
                source: NativeNnSource::Physical {
                    capture: &physical,
                    raw: &raw,
                },
                reason: AstGraphModelUnavailableReason::NoNativeSemanticModel,
                identity_limits: limits(),
            },
            &original,
        )?;
        assert_eq!(
            observation.properties().get("model_available"),
            Some(&AstGraphValue::Bool(false))
        );
        assert_eq!(
            observation.properties().get("source_stored"),
            Some(&AstGraphValue::Bool(false))
        );
        assert_eq!(
            observation.properties().get("source_empty"),
            Some(&AstGraphValue::Bool(path == "empty.py"))
        );
        assert_eq!(text_property(&observation, "path"), path);
        let digest: String = physical
            .content_sha256()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(text_property(&observation, "source_sha256"), digest);
        if path == "model.py" {
            std::fs::write(root.join(path), "def changed():\n    return 2\n")?;
            assert!(
                matches!(observe_unavailable_graph_file(AstGraphUnavailableFileInput{profile:&draft.profiles()[0],source:NativeNnSource::Physical{capture:&physical,raw:&raw},reason:AstGraphModelUnavailableReason::NoNativeSemanticModel,identity_limits:limits()},&original),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::PhysicalSource(_)))
            );
        }
    }
    Ok(())
}
