#[path = "support/corpus_rows.rs"]
mod corpus_rows;
use arrow_array::{Array, RecordBatch, StringArray};
use graphit_ast::{
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema},
    native_nn::{NativeNnError, NativeNnSource},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput, SyntheticReason},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_graph::{
        AstGraphError, AstGraphRelationTarget, AstGraphTsInput, AstGraphValue,
        AstSemanticGraphBatch, assemble_graph_ts,
    },
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::builtin,
    tree_sitter_model::{ParsedTreeSitterDocument, TreeSitterSemanticDocument},
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
async fn raw(
    temp: &TempDir,
    text: &str,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
    let uri = temp.path().join("raw.lance");
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec!["model.go"])),
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
    Ok(read_branch_source(&target, "model.go", original).await?)
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
async fn native_go_graph_preserves_families_ranges_metadata_and_local_edges() -> TestResult {
    fn assert_send<T: Send>() {}
    assert_send::<AstSemanticGraphBatch>();
    assert_send::<AstGraphError>();
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let source = "package sample\nimport alias \"example/pkg\"\ntype I interface {Read() int}\ntype Box struct {A,B int}\ntype Count int\nconst C,D=1,2\nvar V int=3\nfunc F() int {return 1}\nfunc (b *Box) Read() int {return b.A}\nfunc Caller(b *Box,i I) {_=F();_=b.Read();_=b.A;b.A=V;b.A+=1;i.Read();alias.F();Missing();func(){F()}()}\nfunc Shadow(F func()) {F()}\n";
    let raw = raw(&temp, source, &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("actual stored Source")
    };
    let draft = stock();
    let mut owned = None;
    with_model(text, &original, &draft, &["model.go"], |document| {
        owned = Some(assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?);
        Ok(())
    })?;
    let graph = owned.unwrap();
    assert_eq!(graph.association().path(), "model.go");
    assert_eq!(graph.association().raw_version(), raw.version());
    assert_eq!(graph.association().physical_branch(), Some("fixture"));
    let families: BTreeSet<_> = graph.nodes().iter().map(|n| n.label()).collect();
    for label in [
        "Function",
        "Method",
        "Struct",
        "Field",
        "Interface",
        "Type",
        "Constant",
        "Variable",
        "Import",
        "Parameter",
    ] {
        assert!(families.contains(label), "{label}");
    }
    for node in graph.nodes() {
        let pair: Vec<String> = serde_json::from_str(node.key())?;
        assert_eq!(
            pair,
            [text_property(node, "uid"), text_property(node, "data_key")]
        );
        assert_eq!(
            text_property(node, "raw_version"),
            raw.version().to_string()
        );
        assert_eq!(text_property(node, "lang"), "go");
        assert_eq!(text_property(node, "grammar"), "tree-sitter-go");
        assert_eq!(text_property(node, "path"), "model.go");
        let start = usize::try_from(int_property(node, "name_start_byte"))?;
        let end = usize::try_from(int_property(node, "name_end_byte"))?;
        assert_eq!(&source[start..end], text_property(node, "raw_name"));
        assert_eq!(
            int_property(node, "line_number"),
            i64::try_from(source[..start].bytes().filter(|&b| b == b'\n').count() + 1)?
        );
        assert!(int_property(node, "start_byte") <= int_property(node, "name_start_byte"));
        assert!(int_property(node, "end_byte") >= int_property(node, "name_end_byte"));
        let metadata: serde_json::Value =
            serde_json::from_str(text_property(node, "metadata_json"))?;
        assert!(!metadata["contributions"].as_array().unwrap().is_empty());
        assert!(metadata["contributions"][0]["query_policy"].is_object());
        assert!(metadata["contributions"][0]["name_capture_index"].is_number());
    }
    let import = graph
        .nodes()
        .iter()
        .find(|n| n.label() == "Import")
        .unwrap();
    assert_eq!(text_property(import, "name"), "\"example/pkg\"");
    assert_eq!(
        text_property(import, "name_representation"),
        "LiteralSource"
    );
    let constant = graph
        .nodes()
        .iter()
        .find(|n| text_property(n, "name") == "D")
        .unwrap();
    let metadata: serde_json::Value =
        serde_json::from_str(text_property(constant, "metadata_json"))?;
    assert!(
        metadata["contributions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["declared_value"]["subject_ordinal"] == 1)
    );
    assert_eq!(
        graph.edges().iter().filter(|e| e.kind() == "CALLS").count(),
        2
    );
    assert_eq!(
        graph
            .edges()
            .iter()
            .filter(|e| e.kind() == "READS_FIELD")
            .count(),
        3
    );
    assert_eq!(
        graph
            .edges()
            .iter()
            .filter(|e| e.kind() == "WRITES_FIELD")
            .count(),
        2
    );
    for edge in graph.edges() {
        assert!(graph.nodes().iter().any(|n| n.key() == edge.source_key()));
        assert!(graph.nodes().iter().any(|n| n.key() == edge.target_key()));
    }
    assert!(
        graph
            .outcomes()
            .iter()
            .any(|o| matches!(o.target(), AstGraphRelationTarget::NotFound))
    );
    assert!(graph.outcomes().iter().any(|o| matches!(
        o.target(),
        AstGraphRelationTarget::Unresolved {
            reason: graphit_ast::tree_sitter_model::TsLocalUnresolvedReason::DynamicReceiver { .. }
        }
    )));
    assert!(graph.outcomes().iter().any(|o| matches!(
        o.target(),
        AstGraphRelationTarget::Unresolved {
            reason: graphit_ast::tree_sitter_model::TsLocalUnresolvedReason::AnonymousSource { .. }
        }
    )));
    for outcome in graph.outcomes() {
        let v: serde_json::Value = serde_json::from_str(outcome.observation_json())?;
        assert!(v["captures"].is_array());
        assert!(v["policy"].is_object());
        assert!(v["target_rule"].is_object());
    }
    let graph = std::thread::spawn(move || graph).join().unwrap();
    assert!(!graph.nodes().is_empty());
    Ok(())
}
#[tokio::test]
async fn native_go_graph_preserves_tuple_candidates_and_query_order() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let raw = raw(
        &temp,
        "package sample\nfunc F(){}\nfunc Caller(){F()}\n",
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let queries = [
        "  - {data_key: functions, graph_label: Function, pattern: '(function_declaration name: (identifier) @name)'}\n",
        "  - {data_key: calls, type: relation, relation_type: CALLS, pattern: '(call_expression function: (identifier) @name)'}\n",
    ];
    let mut snapshots = Vec::new();
    for reverse in [false, true] {
        let query = if reverse {
            format!("{}{}", queries[1], queries[0])
        } else {
            queries.concat()
        };
        let bytes = format!(
            "language: go\ngrammar: tree-sitter-go\ncontext_types: {{function_declaration: Function}}\nqueries:\n{query}\ntarget_rules:\n  CALLS:\n    labels: [Function]\n"
        );
        let draft = catalog(bytes.as_bytes());
        with_model(text, &original, &draft, &["model.go"], |document| {
            let graph = assemble_graph_ts(
                AstGraphTsInput {
                    document,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            snapshots.push((
                graph
                    .nodes()
                    .iter()
                    .map(|n| n.key().to_owned())
                    .collect::<BTreeSet<_>>(),
                graph
                    .edges()
                    .iter()
                    .map(|e| e.uid().to_owned())
                    .collect::<BTreeSet<_>>(),
            ));
            assert_eq!(graph.edges().len(), 1);
            Ok(())
        })?;
    }
    assert_eq!(snapshots[0], snapshots[1]);
    let stock_bytes = String::from_utf8(
        include_bytes!("../../../crates/graphit-ast/profiles/queries/go.yaml").to_vec(),
    )?;
    let duplicated=stock_bytes.replace("queries:\n","queries:\n  - {data_key: companion_functions, graph_label: Function, pattern: '(function_declaration name: (identifier) @name)'}\n");
    let draft = catalog(duplicated.as_bytes());
    with_model(text, &original, &draft, &["model.go"], |document| {
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), 4);
        let f: Vec<_> = graph
            .nodes()
            .iter()
            .filter(|n| text_property(n, "name") == "F")
            .collect();
        assert_eq!(text_property(f[0], "uid"), text_property(f[1], "uid"));
        assert_ne!(f[0].key(), f[1].key());
        assert!(graph.edges().is_empty());
        assert!(graph.outcomes().iter().any(
            |o| matches!(o.target(),AstGraphRelationTarget::Ambiguous{keys} if keys.len()==2)
        ));
        Ok(())
    })
}
#[tokio::test]
async fn native_graph_rejects_wrong_original_source_allocation_path_and_cancellation() -> TestResult
{
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let raw = raw(&temp, "package sample\nfunc F(){}\n", &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft = stock();
    with_model(text, &original, &draft, &["model.go"], |document| {
        let other = Arc::new(context());
        assert!(
            matches!(assemble_graph_ts(AstGraphTsInput{document,source:NativeNnSource::Indexed(&raw)},&other),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::RequestMismatch))
        );
        Ok(())
    })?;
    let copy = text.to_owned();
    for (source, path) in [(copy.as_str(), vec!["model.go"]), (text, vec!["other.go"])] {
        with_model(source, &original, &draft, &path, |document| {
            assert!(
                matches!(assemble_graph_ts(AstGraphTsInput{document,source:NativeNnSource::Indexed(&raw)},&original),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::InvalidInput))
            );
            Ok(())
        })?;
    }
    with_model(text, &original, &draft, &["model.go"], |document| {
        original.cancellation().cancel();
        assert!(
            matches!(assemble_graph_ts(AstGraphTsInput{document,source:NativeNnSource::Indexed(&raw)},&original),Err(AstGraphError::Operation(e)) if e.kind()==std::io::ErrorKind::Interrupted)
        );
        Ok(())
    })
}
#[tokio::test]
async fn native_graph_empty_source_and_nonexact_extent_remain_distinct() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let empty = raw(&temp, "", &original).await?;
    let IndexedSource::Available(text) = empty.source(&original)? else {
        panic!("stored empty is available")
    };
    let draft = stock();
    with_model(text, &original, &draft, &["model.go"], |document| {
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&empty),
            },
            &original,
        )?;
        assert!(graph.nodes().is_empty());
        assert!(graph.edges().is_empty());
        assert_eq!(graph.association().raw_version(), empty.version());
        assert_eq!(
            graph.association().source_observation(),
            graphit_ast::semantic_graph::AstGraphSourceObservation::IndexedStored { empty: true }
        );
        Ok(())
    })?;
    let temp = TempDir::new()?;
    let raw = raw(
        &temp,
        "package sample\ntype Box struct {A int}\n",
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let start = u64::try_from(text.find("int").unwrap())?;
    let end = u64::try_from(text.len())?;
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: start,
            },
            original: ByteSpan {
                start: 0,
                end: start,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start,
                end: start + 3,
            },
            reason: SyntheticReason::InsertedByTransform,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: start + 3,
                end,
            },
            original: ByteSpan {
                start: start + 3,
                end,
            },
        },
    ];
    let removed = [ByteSpan {
        start,
        end: start + 3,
    }];
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let factory = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        builtin("tree-sitter-go")?,
        &original,
    )?;
    let parsed = ParsedTreeSitterDocument::collect(&factory, &input, &["model.go"], limits())?;
    let document = parsed.assemble()?;
    let graph = assemble_graph_ts(
        AstGraphTsInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
        },
        &original,
    )?;
    let field = graph.nodes().iter().find(|n| n.label() == "Field").unwrap();
    assert_eq!(text_property(field, "extent_status"), "NonExact");
    assert_eq!(
        field.properties().get("available"),
        Some(&AstGraphValue::Bool(false))
    );
    assert!(!field.properties().contains_key("start_byte"));
    let diagnostics: serde_json::Value = serde_json::from_str(graph.diagnostics_json())?;
    assert!(!diagnostics.as_array().unwrap().is_empty());
    let metadata: serde_json::Value = serde_json::from_str(text_property(field, "metadata_json"))?;
    assert_eq!(
        metadata["contributions"][0]["extent"]["origin"]["status"],
        "Compound"
    );
    Ok(())
}

#[tokio::test]
async fn native_graph_physical_null_source_preserves_real_owner_and_empty_status() -> TestResult {
    use graphit_ast::{
        indexed_source::read_indexed_source,
        native_nn_physical_source::capture_native_nn_physical_source,
        semantic_graph::AstGraphSourceObservation,
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
    std::fs::write(root.join("model.go"), "package sample\nfunc F(){}\n")?;
    std::fs::write(root.join("empty.go"), "")?;
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
            Arc::new(StringArray::from(vec!["model.go", "empty.go"])),
            Arc::new(StringArray::from(vec![None::<&str>, None])),
        ],
    )?;
    let store =
        LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![batch]).await?;
    let draft = stock();
    for path in ["model.go", "empty.go"] {
        let raw = read_indexed_source(&store.snapshot(), path, &original).await?;
        assert!(matches!(raw.source(&original)?, IndexedSource::Missing));
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
        let text = physical.source(&original)?;
        with_model(text, &original, &draft, &[path], |document| {
            assert!(
                matches!(assemble_graph_ts(AstGraphTsInput{document,source:NativeNnSource::Indexed(&raw)},&original),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::InvalidInput))
            );
            let graph = assemble_graph_ts(
                AstGraphTsInput {
                    document,
                    source: NativeNnSource::Physical {
                        capture: &physical,
                        raw: &raw,
                    },
                },
                &original,
            )?;
            assert_eq!(
                graph.association().source_observation(),
                AstGraphSourceObservation::PhysicalTextNotStored {
                    empty: text.is_empty()
                }
            );
            let observation = graph.file_observation();
            assert_eq!(observation.label(), "AstFileObservation");
            assert_eq!(
                observation.properties().get("source_stored"),
                Some(&AstGraphValue::Bool(false))
            );
            assert_eq!(
                observation.properties().get("source_empty"),
                Some(&AstGraphValue::Bool(text.is_empty()))
            );
            assert_eq!(text_property(observation, "path"), path);
            let key: serde_json::Value = serde_json::from_str(observation.key())?;
            assert_eq!(
                key,
                serde_json::json!(["file_observation", path, ["go", "tree-sitter-go"]])
            );
            assert_eq!(graph.association().path(), path);
            assert_eq!(
                graph.association().source_sha256(),
                &physical.content_sha256()
            );
            if path == "empty.go" {
                assert!(graph.nodes().is_empty());
            } else {
                assert_eq!(graph.nodes().len(), 1);
                assert_eq!(
                    graph.nodes()[0].properties().get("source_stored"),
                    Some(&AstGraphValue::Bool(false))
                );
                assert_eq!(
                    graph.nodes()[0].properties().get("source_empty"),
                    Some(&AstGraphValue::Bool(false))
                );
            }
            Ok(())
        })?;
        if path == "model.go" {
            std::fs::write(root.join(path), "package sample\nfunc Changed(){}\n")?;
            with_model(text, &original, &draft, &[path], |document| {
                assert!(
                    matches!(assemble_graph_ts(AstGraphTsInput{document,source:NativeNnSource::Physical{capture:&physical,raw:&raw}},&original),Err(AstGraphError::Source(e)) if matches!(*e,NativeNnError::PhysicalSource(_)))
                );
                Ok(())
            })?;
        }
    }
    Ok(())
}
#[tokio::test]
async fn native_rust_graph_keeps_raw_relation_observations_without_a_go_target_guess() -> TestResult
{
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let raw = raw(&temp, "pub fn f() {}\npub fn caller() {f();}\n", &original).await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft = catalog(include_bytes!(
        "../../../crates/graphit-ast/profiles/queries/rust.yaml"
    ));
    with_model(text, &original, &draft, &["model.go"], |document| {
        let graph = assemble_graph_ts(
            AstGraphTsInput {
                document,
                source: NativeNnSource::Indexed(&raw),
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), 2);
        assert!(
            graph
                .nodes()
                .iter()
                .all(|n| text_property(n, "lang") == "rust"
                    && text_property(n, "grammar") == "tree-sitter-rust")
        );
        assert!(graph.edges().is_empty());
        assert!(
            graph
                .outcomes()
                .iter()
                .any(|o| matches!(o.target(), AstGraphRelationTarget::ObservedOnly))
        );
        let capabilities: serde_json::Value = serde_json::from_str(graph.capabilities_json())?;
        assert!(!capabilities.as_array().unwrap().is_empty());
        Ok(())
    })
}

#[tokio::test]
async fn native_go_graph_full_wire_is_stable_across_independent_reparse() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let raw = raw(
        &temp,
        "package sample\nfunc F(){}\nfunc Caller(){F()}\n",
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source")
    };
    let draft = catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {function_declaration: Function}\nqueries:\n  - {data_key: functions, graph_label: Function, pattern: '(function_declaration name: (identifier) @name)'}\n  - {data_key: calls, type: relation, relation_type: CALLS, pattern: '(call_expression function: (identifier) @name)'}\ntarget_rules:\n  CALLS: {labels: [Function]}\n");
    with_model(text, &original, &draft, &["model.go"], |left| {
        with_model(text, &original, &draft, &["model.go"], |right| {
            assert_ne!(
                left.entities()?[0].contributions()[0]
                    .name_capture
                    .native
                    .node_id,
                right.entities()?[0].contributions()[0]
                    .name_capture
                    .native
                    .node_id
            );
            let first = assemble_graph_ts(
                AstGraphTsInput {
                    document: left,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            let second = assemble_graph_ts(
                AstGraphTsInput {
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
            assert_eq!(first.edges().len(), 1);
            assert!(!first.outcomes().is_empty());
            for node in first.nodes() {
                let metadata: serde_json::Value =
                    serde_json::from_str(text_property(node, "metadata_json"))?;
                for contribution in metadata["contributions"].as_array().unwrap() {
                    assert!(contribution.get("sdk_name_address").is_none());
                }
            }
            for outcome in first.outcomes() {
                let observation: serde_json::Value =
                    serde_json::from_str(outcome.observation_json())?;
                for capture in observation["captures"].as_array().unwrap() {
                    assert!(capture.get("sdk_node_id").is_none());
                }
            }
            Ok(())
        })
    })
}

#[tokio::test]
async fn configured_value_label_graph_and_nn_preserve_literal_boundary() -> TestResult {
    use graphit_ast::native_nn::{NativeNnModel, NativeNnTsInput, encode_native_nn_ts};
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let raw = raw(
        &temp,
        "package p\nvar a=1\nvar b=\"\"\nvar c=outside\nfunc F(){var inner=1;_ = inner}\n",
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = raw.source(&original)? else {
        panic!("Source");
    };
    let draft = catalog(b"language: go\ngrammar: tree-sitter-go\ncontext_types: {function_declaration: Function}\nembed_labels: [Variable, Value]\nqueries:\n  - data_key: variables\n    graph_label: Variable\n    pattern: '(var_spec name: (identifier) @name value: (expression_list (_) @value))'\n    value_capture: value\n    value_label: Value\n");
    let mut mixed_batch = None;
    let mut corpus_lexical = None;
    with_model(text, &original, &draft, &["model.go"], |left| {
        with_model(text, &original, &draft, &["model.go"], |right| {
            assert_ne!(
                left.entities()?[0].contributions()[0]
                    .name_capture
                    .native
                    .node_id,
                right.entities()?[0].contributions()[0]
                    .name_capture
                    .native
                    .node_id
            );
            let first = assemble_graph_ts(
                AstGraphTsInput {
                    document: left,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            let second = assemble_graph_ts(
                AstGraphTsInput {
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
            assert_eq!(first.nodes().len(), 8);
            assert!(
                first.edges().is_empty(),
                "value provenance does not invent an edge"
            );
            let rows: Vec<_> = first
                .nodes()
                .iter()
                .filter(|n| n.label() == "Value")
                .collect();
            assert_eq!(rows.len(), 4);
            assert_eq!(first.association().raw_version(), raw.version());
            assert_eq!(first.association().path(), "model.go");
            assert_eq!(first.file_observation().label(), "AstFileObservation");
            for node in rows {
                assert_eq!(text_property(node, "data_key"), "variables");
                let metadata: serde_json::Value =
                    serde_json::from_str(text_property(node, "metadata_json"))?;
                let contribution = &metadata["contributions"][0];
                assert!(contribution.get("sdk_name_address").is_none());
                let parent_uid = contribution["derived_value"]["owner_uid"].as_str().unwrap();
                let parent = first
                    .nodes()
                    .iter()
                    .find(|n| text_property(n, "uid") == parent_uid)
                    .unwrap();
                assert_eq!(parent.label(), "Variable");
                let begin = usize::try_from(int_property(node, "name_start_byte"))?;
                let end = usize::try_from(int_property(node, "name_end_byte"))?;
                assert_eq!(&text[begin..end], text_property(node, "raw_name"));
                assert_eq!(text_property(node, "name"), text_property(node, "raw_name"));
                let namespace = contribution["namespaces"]
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap();
                assert_eq!(namespace["name"], text_property(parent, "name"));
                assert_eq!(namespace["selection"], "DerivedDeclarationIdentifier");
            }
            let nn_model = NativeNnModel {
                vector_id: [7; 32],
                dimensions: 2,
                model_id: "controlled-fixture",
                revision: "value-row",
            };
            let indices: Vec<_> = left
                .entities()?
                .iter()
                .enumerate()
                .filter(|(_, e)| e.identity().label() == "Variable")
                .map(|(i, _)| i)
                .collect();
            let vectors = vec![vec![1.0, 0.0]; indices.len()];
            let batch = encode_native_nn_ts(
                &nn_model,
                &[NativeNnTsInput {
                    document: left,
                    source: NativeNnSource::Indexed(&raw),
                    entity_indices: &indices,
                    vectors: &vectors,
                    model_generation: Some(9),
                }],
                &original,
            )?;
            assert_eq!(batch.num_rows(), 4);

            let mixed_indices: Vec<_> = left
                .code_projections()?
                .iter()
                .map(|p| match p {
                    graphit_ast::tree_sitter_model::TsProjection::Available(p) => p.entity_index,
                    _ => panic!("all configured mixed projections are available"),
                })
                .collect();
            assert_eq!(mixed_indices.len(), 8);
            let mixed_vectors = vec![vec![1.0, 0.0]; mixed_indices.len()];
            let mixed = encode_native_nn_ts(
                &nn_model,
                &[NativeNnTsInput {
                    document: left,
                    source: NativeNnSource::Indexed(&raw),
                    entity_indices: &mixed_indices,
                    vectors: &mixed_vectors,
                    model_generation: Some(9),
                }],
                &original,
            )?;
            assert_eq!(mixed.num_rows(), 8);
            let name_kinds = mixed
                .column_by_name("name_kind")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let names = mixed
                .column_by_name("name")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            for (row, &index) in mixed_indices.iter().enumerate() {
                let entity = &left.entities()?[index];
                assert_eq!(names.value(row), entity.name());
                assert_eq!(
                    name_kinds.value(row),
                    if entity.identity().label() == "Value" {
                        "LiteralSource"
                    } else {
                        "Identifier"
                    }
                );
            }
            corpus_lexical = Some(graphit_ast::entity_search::encode_file(
                &first.into_parts(),
                text,
                &original,
            )?);
            mixed_batch = Some(mixed);
            let foreign = Arc::new(context());
            assert!(
                assemble_graph_ts(
                    AstGraphTsInput {
                        document: left,
                        source: NativeNnSource::Indexed(&raw)
                    },
                    &foreign
                )
                .is_err()
            );
            Ok(())
        })
    })?;

    let mixed = mixed_batch.unwrap();
    use graphit_ast::{
        native_nn::validate_native_nn,
        native_nn_query::{NativeNnMetric, query_corpus_nn},
    };
    let nn_model = NativeNnModel {
        vector_id: [7; 32],
        dimensions: 2,
        model_id: "controlled-fixture",
        revision: "value-row",
    };
    validate_native_nn(&mixed, &nn_model, &original)?;
    let replace = |column: &str, array: arrow_array::ArrayRef| {
        let mut columns = mixed.columns().to_vec();
        columns[mixed.schema().index_of(column).unwrap()] = array;
        RecordBatch::try_new(mixed.schema(), columns)
    };
    for bad in ["Unknown", "JsonName", "RustType"] {
        assert!(matches!(
            validate_native_nn(
                &replace("name_kind", Arc::new(StringArray::from(vec![bad; 8]))).unwrap(),
                &nn_model,
                &original
            ),
            Err(NativeNnError::InvalidRow)
        ));
    }
    assert!(
        replace(
            "name_kind",
            Arc::new(StringArray::from(vec![None::<&str>; 8]))
        )
        .is_err()
    );
    let namespaces = mixed
        .column_by_name("namespace")
        .unwrap()
        .as_any()
        .downcast_ref::<arrow_array::ListArray>()
        .unwrap();
    let frames = namespaces
        .values()
        .as_any()
        .downcast_ref::<arrow_array::StructArray>()
        .unwrap();
    for (field_name, bad) in [("name_kind", "Unknown"), ("raw_name", "different-owner")] {
        let mut fields = frames.columns().to_vec();
        let index = frames
            .fields()
            .iter()
            .position(|f| f.name() == field_name)
            .unwrap();
        fields[index] = Arc::new(StringArray::from(vec![bad; frames.len()]));
        let corrupt_frames =
            arrow_array::StructArray::try_new(frames.fields().clone(), fields, None)?;
        let corrupt_namespaces = arrow_array::ListArray::try_new(
            Arc::new(arrow_schema::Field::new(
                "item",
                arrow_schema::DataType::Struct(frames.fields().clone()),
                true,
            )),
            namespaces.offsets().clone(),
            Arc::new(corrupt_frames),
            None,
        )?;
        let corrupt = replace("namespace", Arc::new(corrupt_namespaces))?;
        assert!(matches!(
            validate_native_nn(&corrupt, &nn_model, &original),
            Err(NativeNnError::InvalidRow)
        ));
    }
    let mut fields = mixed.schema().fields().to_vec();
    fields.pop();
    let mut columns = mixed.columns().to_vec();
    columns.pop();
    let old = RecordBatch::try_new(Arc::new(arrow_schema::Schema::new(fields)), columns)?;
    assert!(matches!(
        validate_native_nn(&old, &nn_model, &original),
        Err(NativeNnError::SchemaMismatch)
    ));
    let published = corpus_rows::persist(
        &temp.path().join("corpus.lance"),
        corpus_lexical.ok_or("genuine lexical rows missing")?,
        &mixed,
        &nn_model,
        None,
        &original,
    )
    .await?;
    let queried = query_corpus_nn(
        &published,
        &nn_model,
        &[1.0, 0.0],
        8,
        NativeNnMetric::L2,
        &original,
    )
    .await?;
    let mut count = 0;
    for batch in queried.rows(&original)? {
        validate_native_nn(batch, &nn_model, &original)?;
        let strings = |name: &str| {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
        };
        let source_strings = |name: &str| {
            mixed
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
        };
        for row in 0..batch.num_rows() {
            let index = (0..8)
                .find(|&i| {
                    source_strings("uid").value(i) == strings("uid").value(row)
                        && source_strings("data_key").value(i) == strings("data_key").value(row)
                })
                .unwrap();
            for field in mixed.schema().fields() {
                assert_eq!(
                    batch
                        .column_by_name(field.name())
                        .unwrap()
                        .slice(row, 1)
                        .to_data(),
                    mixed
                        .column_by_name(field.name())
                        .unwrap()
                        .slice(index, 1)
                        .to_data()
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 8);
    Ok(())
}
