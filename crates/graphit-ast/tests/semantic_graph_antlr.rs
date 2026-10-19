use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    antlr_engine::model::{AntlrSemanticDocument, ParsedAntlrDocument},
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{IndexedSourceSnapshot, read_branch_source, source_schema},
    native_nn::{NativeNnError, NativeNnSource},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_graph::{
        AstGraphAntlrInput, AstGraphError, AstGraphRelationTarget, AstGraphValue,
        assemble_graph_antlr,
    },
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
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
    action: impl FnOnce(&AntlrSemanticDocument<'_, '_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    with_source(text, original, |input| {
        let parsed = ParsedAntlrDocument::collect(&draft.profiles()[0], input, path, limits())?;
        let model = parsed.assemble()?;
        action(&model)
    })
}
async fn raw(
    temp: &TempDir,
    name: &str,
    path: &str,
    text: Option<&str>,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, Box<dyn Error>> {
    let uri = temp.path().join(name);
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
        panic!("string property {key}");
    };
    value
}
fn metadata(node: &graphit_ast::semantic_graph::AstGraphNode) -> serde_json::Value {
    serde_json::from_str(text_property(node, "metadata_json")).unwrap()
}

#[tokio::test]
async fn genuine_db2_oracle_models_emit_owned_graph_and_observed_relations() -> TestResult {
    for (name, profile, body) in [
        (
            "db2",
            include_bytes!("fixtures/catalog/shipped/db2.yaml").as_slice(),
            "CREATE TYPE APP.TYPE_F AS(A_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_F INTEGER) RETURNS INTEGER LANGUAGE SQL; CREATE TABLE TABLE_F(C INTEGER); BEGIN ATOMIC DECLARE V_A,V_B INTEGER DEFAULT 1; END; CREATE ALIAS A_F FOR TABLE TABLE_F; SELECT C FROM TABLE_F;",
        ),
        (
            "oracle",
            include_bytes!("fixtures/catalog/shipped/plsql.yaml").as_slice(),
            "CREATE TABLE \"ACME\".\"PEDIDO\"(\"ID_PEDIDO\" NUMBER DEFAULT 123456789012345678901234567890,\"DT_PEDIDO\" DATE); SELECT ID_PEDIDO FROM PEDIDO;",
        ),
    ] {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let raw = raw(&temp, "source.lance", "model.sql", Some(body), &original).await?;
        let IndexedSource::Available(text) = raw.source(&original)? else {
            panic!("actual source");
        };
        let draft = catalog(profile);
        let mut first = None;
        with_model(text, &original, &draft, &["model.sql"], |model| {
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document: model,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            assert_eq!(graph.nodes().len(), model.entities().len());
            assert!(!graph.nodes().is_empty());
            assert_eq!(graph.association().path(), "model.sql");
            assert_eq!(graph.association().raw_version(), raw.version());
            assert_eq!(graph.association().physical_branch(), Some("fixture"));
            assert!(graph.edges().is_empty(), "no resolved ANTLR target API");
            assert_eq!(graph.outcomes().len(), model.unresolved_relations().len());
            assert!(
                graph
                    .outcomes()
                    .iter()
                    .all(|o| matches!(o.target(), AstGraphRelationTarget::ObservedOnly))
            );
            assert!(!graph.outcomes().is_empty(), "{name} selects retained");
            for (node, entity) in graph.nodes().iter().zip(model.entities()) {
                assert_eq!(text_property(node, "name"), entity.name());
                assert_eq!(text_property(node, "raw_name"), entity.name_raw().text());
                assert_eq!(
                    node.key(),
                    serde_json::to_string(&[entity.identity().uid().as_str(), entity.data_key()])?
                );
                assert_eq!(node.label(), entity.identity().label());
                let m = metadata(node);
                assert_eq!(
                    m["contributions"].as_array().unwrap().len(),
                    entity.contributions().len()
                );
                assert_eq!(
                    m["namespaces"].as_array().unwrap().len(),
                    entity.namespace().len()
                );
                assert!(m["contributions"][0]["occurrence"]["policy"]["provenance"].is_object());
                assert_eq!(
                    node.properties()["name_start_byte"],
                    AstGraphValue::Int64(i64::try_from(entity.name_raw().start())?)
                );
                assert_eq!(node.properties()["available"], AstGraphValue::Bool(true));
            }
            if name == "db2" {
                let typ = graph
                    .nodes()
                    .iter()
                    .find(|n| text_property(n, "name") == "TYPE_F")
                    .unwrap();
                assert_eq!(text_property(typ, "raw_name"), "APP.TYPE_F");
                assert_eq!(
                    metadata(typ)["qualified_name"]["components"]
                        .as_array()
                        .unwrap()
                        .len(),
                    2
                );
                let method = graph
                    .nodes()
                    .iter()
                    .find(|n| text_property(n, "name") == "METHOD_F")
                    .unwrap();
                assert_eq!(metadata(method)["namespaces"][0]["name"], "TYPE_F");
                assert!(
                    graph.nodes().iter().any(|n| n.label() == "Parameter"),
                    "outside embed policy still Graph"
                );
                assert!(!graph.nodes().iter().any(|n| n.label() == "Schema"));
                assert!(graph.nodes().iter().any(|n| n.label() == "Variable"));
                assert!(graph.nodes().iter().any(|n| n.label() == "Alias"));
                assert_eq!(
                    metadata(method)["contributions"][0]["type_roles"][0]["role"],
                    "Return"
                );
                assert!(
                    !metadata(method)["owner_links"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
            } else {
                let column = graph
                    .nodes()
                    .iter()
                    .find(|n| text_property(n, "name") == "ID_PEDIDO")
                    .unwrap();
                let values = metadata(column);
                assert!(
                    values["contributions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|c| c["declared_value"]["value"]["validated_number_lexeme"]
                            == "123456789012345678901234567890")
                );
            }
            first = Some(graph);
            Ok(())
        })?;
        with_model(text, &original, &draft, &["model.sql"], |model| {
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document: model,
                    source: NativeNnSource::Indexed(&raw),
                },
                &original,
            )?;
            assert_eq!(
                first.as_ref().unwrap(),
                &graph,
                "full rows equal, no normalization"
            );
            Ok(())
        })?;
        let graph = first.unwrap();
        drop(raw);
        drop(draft);
        std::thread::spawn(move || {
            let parts = graph.into_parts();
            assert!(!parts.nodes.is_empty());
            assert!(!parts.outcomes.is_empty());
        })
        .join()
        .unwrap();
    }
    Ok(())
}
#[tokio::test]
async fn genuine_antlr_graph_original_allocation_path_cancel_empty_and_null_guards() -> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let raw_source = raw(
        &temp,
        "source.lance",
        "model.sql",
        Some("CREATE TABLE TABLE_F(C INTEGER);"),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = raw_source.source(&original)? else {
        panic!("available");
    };
    with_model(text, &original, &draft, &["model.sql"], |model| {
        let foreign = Arc::new(context());
        assert!(
            matches!(assemble_graph_antlr(AstGraphAntlrInput{document:model,source:NativeNnSource::Indexed(&raw_source)},&foreign),Err(AstGraphError::Source(e)) if matches!(e.as_ref(),NativeNnError::RequestMismatch))
        );
        Ok(())
    })?;
    {
        let candidate = text.to_owned();
        let path = ["model.sql"];
        with_model(&candidate, &original, &draft, &path, |model| {
            assert!(
                matches!(assemble_graph_antlr(AstGraphAntlrInput{document:model,source:NativeNnSource::Indexed(&raw_source)},&original),Err(AstGraphError::Source(e)) if matches!(e.as_ref(),NativeNnError::InvalidInput))
            );
            Ok(())
        })?;
    }
    {
        let path = ["wrong.sql"];
        with_model(text, &original, &draft, &path, |model| {
            assert!(
                matches!(assemble_graph_antlr(AstGraphAntlrInput{document:model,source:NativeNnSource::Indexed(&raw_source)},&original),Err(AstGraphError::Source(e)) if matches!(e.as_ref(),NativeNnError::InvalidInput))
            );
            Ok(())
        })?;
    }
    let empty = raw(&temp, "empty.lance", "empty.sql", Some(""), &original).await?;
    let null = raw(&temp, "null.lance", "empty.sql", None, &original).await?;
    let IndexedSource::Available(empty_text) = empty.source(&original)? else {
        panic!("stored empty");
    };
    with_model(empty_text, &original, &draft, &["empty.sql"], |model| {
        let graph = assemble_graph_antlr(
            AstGraphAntlrInput {
                document: model,
                source: NativeNnSource::Indexed(&empty),
            },
            &original,
        )?;
        assert!(graph.nodes().is_empty());
        assert_eq!(
            graph.association().source_observation(),
            graphit_ast::semantic_graph::AstGraphSourceObservation::IndexedStored { empty: true }
        );
        assert_eq!(graph.file_observation().label(), "AstFileObservation");
        assert!(
            matches!(assemble_graph_antlr(AstGraphAntlrInput{document:model,source:NativeNnSource::Indexed(&null)},&original),Err(AstGraphError::Source(e)) if matches!(e.as_ref(),NativeNnError::InvalidInput))
        );
        Ok(())
    })?;
    with_model(text, &original, &draft, &["model.sql"], |model| {
        original.cancellation().cancel();
        assert!(
            matches!(assemble_graph_antlr(AstGraphAntlrInput{document:model,source:NativeNnSource::Indexed(&raw_source)},&original),Err(AstGraphError::Operation(e)) if e.kind()==std::io::ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    Ok(())
}

fn custom(queries: &str) -> CatalogDraft {
    catalog(format!("language: db2\nparser: antlr4\ngrammar: antlr-db2\nembed_labels: [Table]\nantlr_contexts: [{{pattern: '//create_table_statement', label: Table, name: [{{xpath: '/table_name'}}]}}]\nqueries:\n{queries}").as_bytes())
}
#[tokio::test]
async fn genuine_antlr_graph_custom_tuples_duplicates_group_policy_and_conflicts() -> TestResult {
    let normal = "  - {data_key: tables, graph_label: Table, type: entity, pattern: '//create_table_statement/table_name'}\n";
    let bad = "  - {data_key: tables, graph_label: Table, type: entity, pattern: '//create_table_statement', captures: {name: [{xpath: '/table_name'}], span: [{nearest_descendant: 'column_name'}]}}\n";
    let distinct = "  - {data_key: custom, graph_label: Custom, type: entity, pattern: '//create_table_statement/table_name'}\n";
    let normal_group = "  - {data_key: vars, graph_label: Custom, type: entity, pattern: '//declare_variable_statement/variable_name'}\n";
    let missing_group = "  - {data_key: vars, graph_label: Custom, type: entity, pattern: '//declare_variable_statement', captures: {name: [{xpath: '/alias_name'}]}}\n";
    let invalid_group = "  - {data_key: vars, graph_label: Custom, type: entity, pattern: '//declare_variable_statement', captures: {name: [{xpath: '/data_type'}]}}\n";
    let local_policy = "  - {data_key: vars, graph_label: Custom, type: entity, pattern: '//declare_variable_statement/variable_name', name_reject: '^V_A$'}\n";
    for (queries, body, expected_count, unavailable, contributors) in [
        (
            format!("{normal}{normal}"),
            "CREATE TABLE TABLE_F(C INTEGER);",
            1,
            false,
            2,
        ),
        (
            format!("{normal}{distinct}"),
            "CREATE TABLE TABLE_F(C INTEGER);",
            2,
            false,
            1,
        ),
        (
            format!("{normal}{bad}"),
            "CREATE TABLE TABLE_F(C INTEGER);",
            1,
            true,
            2,
        ),
        (
            format!("{bad}{normal}"),
            "CREATE TABLE TABLE_F(C INTEGER);",
            1,
            true,
            2,
        ),
        (
            format!("{normal_group}{missing_group}"),
            "BEGIN ATOMIC DECLARE V_A,V_B INTEGER; END;",
            2,
            true,
            1,
        ),
        (
            format!("{missing_group}{normal_group}"),
            "BEGIN ATOMIC DECLARE V_A,V_B INTEGER; END;",
            2,
            true,
            1,
        ),
        (
            format!("{normal_group}{invalid_group}"),
            "BEGIN ATOMIC DECLARE V_A,V_B INTEGER; END;",
            2,
            true,
            1,
        ),
        (
            format!("{invalid_group}{normal_group}"),
            "BEGIN ATOMIC DECLARE V_A,V_B INTEGER; END;",
            2,
            true,
            1,
        ),
        (
            local_policy.to_owned(),
            "BEGIN ATOMIC DECLARE V_A,V_B INTEGER; END;",
            1,
            false,
            1,
        ),
    ] {
        let temp = TempDir::new()?;
        let original = Arc::new(context());
        let source = raw(&temp, "source.lance", "policy.db2", Some(body), &original).await?;
        let IndexedSource::Available(text) = source.source(&original)? else {
            panic!("available");
        };
        let draft = custom(&queries);
        with_model(text, &original, &draft, &["policy.db2"], |model| {
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document: model,
                    source: NativeNnSource::Indexed(&source),
                },
                &original,
            )?;
            assert_eq!(graph.nodes().len(), expected_count);
            let mut keys = std::collections::BTreeSet::new();
            for node in graph.nodes() {
                assert!(keys.insert(node.key()));
                assert_eq!(
                    node.properties()["available"],
                    AstGraphValue::Bool(!unavailable)
                );
                let m = metadata(node);
                assert_eq!(m["contributions"].as_array().unwrap().len(), contributors);
                assert_eq!(
                    m["contributions"][0]["occurrence"]["name"]["status"],
                    "Present"
                );
                if unavailable {
                    assert!(
                        !m["diagnostic_indices"].as_array().unwrap().is_empty()
                            || !m["conflicts"].as_array().unwrap().is_empty()
                    );
                }
            }
            if queries.contains("name_reject") {
                assert_eq!(text_property(&graph.nodes()[0], "name"), "V_B");
                assert_eq!(graph.nodes()[0].label(), "Custom");
                assert_eq!(text_property(&graph.nodes()[0], "data_key"), "vars");
            }
            if expected_count == 2 && body.starts_with("CREATE TABLE") {
                assert_eq!(graph.nodes()[1].label(), "Custom");
                assert_eq!(text_property(&graph.nodes()[1], "data_key"), "custom");
            }
            let caps: serde_json::Value = serde_json::from_str(graph.capabilities_json())?;
            assert_eq!(
                caps.as_array().unwrap().len(),
                draft.profiles()[0].queries().value().len()
            );
            assert!(caps[0]["policy"]["provenance"]["pattern"].is_string());
            Ok(())
        })?;
    }
    Ok(())
}
#[tokio::test]
async fn genuine_antlr_graph_recovery_and_nonexact_original_keep_unavailable_observations()
-> TestResult {
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let source = raw(
        &temp,
        "recovery.lance",
        "recovery.db2",
        Some("CREATE TABLE T(C INTEGER"),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = source.source(&original)? else {
        panic!("available");
    };
    with_model(text, &original, &draft, &["recovery.db2"], |model| {
        let graph = assemble_graph_antlr(
            AstGraphAntlrInput {
                document: model,
                source: NativeNnSource::Indexed(&source),
            },
            &original,
        )?;
        assert!(!graph.nodes().is_empty());
        assert!(
            graph
                .nodes()
                .iter()
                .all(|node| node.properties()["available"] == AstGraphValue::Bool(false))
        );
        let diagnostics: serde_json::Value = serde_json::from_str(graph.diagnostics_json())?;
        assert!(
            diagnostics
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["source"] == "NativeSyntax")
        );
        assert!(
            diagnostics
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["issue"].as_str().is_some_and(|s| s.contains("Recovery")))
        );
        Ok(())
    })?;
    let source = raw(
        &temp,
        "nonexact.lance",
        "nonexact.db2",
        Some("CREATE TABLE TABLE_F(C INTEGER);"),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = source.source(&original)? else {
        panic!("available");
    };
    let start = text.find("TABLE_F").unwrap();
    let end = start + "TABLE_F".len();
    let pieces = [
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: 0,
                end: start as u64,
            },
            reason: graphit_ast::prepared_input::SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: start as u64,
                end: end as u64,
            },
            original: ByteSpan {
                start: start as u64,
                end: end as u64,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: end as u64,
                end: text.len() as u64,
            },
            reason: graphit_ast::prepared_input::SyntheticReason::Wrapper,
        },
    ];
    let removed = [
        ByteSpan {
            start: 0,
            end: start as u64,
        },
        ByteSpan {
            start: end as u64,
            end: text.len() as u64,
        },
    ];
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &pieces,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let draft = custom(
        "  - {data_key: tables, graph_label: Table, type: entity, pattern: '//create_table_statement/table_name'}\n",
    );
    let parsed =
        ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["nonexact.db2"], limits())?;
    let model = parsed.assemble()?;
    let graph = assemble_graph_antlr(
        AstGraphAntlrInput {
            document: &model,
            source: NativeNnSource::Indexed(&source),
        },
        &original,
    )?;
    assert_eq!(graph.nodes().len(), 1);
    let node = &graph.nodes()[0];
    assert_eq!(text_property(node, "extent_status"), "NonExact");
    assert_eq!(node.properties()["available"], AstGraphValue::Bool(false));
    assert!(!node.properties().contains_key("start_byte"));
    assert_eq!(text_property(node, "raw_name"), "TABLE_F");
    assert_eq!(
        metadata(node)["contributions"][0]["extent"]["status"],
        "NonExact"
    );
    Ok(())
}
#[tokio::test]
async fn genuine_antlr_graph_actual_deadline_after_collection_is_typed() -> TestResult {
    use std::time::{Duration, Instant};
    let temp = TempDir::new()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let original = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let source = raw(
        &temp,
        "deadline.lance",
        "deadline.db2",
        Some("CREATE TABLE TABLE_F(C INTEGER);"),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = source.source(&original)? else {
        panic!("available");
    };
    with_model(text, &original, &draft, &["deadline.db2"], |model| {
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        assert!(
            matches!(assemble_graph_antlr(AstGraphAntlrInput { document: model, source: NativeNnSource::Indexed(&source) }, &original), Err(AstGraphError::Operation(e)) if e.kind() == std::io::ErrorKind::TimedOut)
        );
        Ok(())
    })?;
    Ok(())
}

#[tokio::test]
async fn db2_module_type_variable_genuine_raw_nn_graph() -> TestResult {
    use graphit_ast::{
        antlr_engine::model::{AntlrEntityExtent, AntlrProjection},
        native_nn::{
            NativeNnAntlrInput, NativeNnModel, encode_native_nn_antlr, validate_native_nn,
        },
    };
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let body = "-- α\r\nALTER MODULE APP.M_A ADD TYPE T_F AS INTEGER ARRAY[];\r\nALTER MODULE APP.M_B PUBLISH TYPE T_F AS ROW ANCHOR ROW OF APP.TABLE_F;\r\nALTER MODULE APP.M_A ADD VARIABLE V_F INTEGER;\r\nALTER MODULE APP.M_B PUBLISH VARIABLE V_F INTEGER DEFAULT NULL;\r\nALTER MODULE \"café\".\"MODULE\" ADD VARIABLE \"variável.pão\" CURSOR DEFAULT (CURSOR FOR SELECT C FROM APP.TABLE_F);";
    let source = raw(&temp, "modules.lance", "modules.db2", Some(body), &original).await?;
    let IndexedSource::Available(text) = source.source(&original)? else {
        panic!("genuine indexed Source");
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let mut first_graph = None;
    with_model(text, &original, &draft, &["modules.db2"], |model| {
        assert!(
            model.diagnostics().is_empty(),
            "{:?}",
            model
                .diagnostics()
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        assert_eq!(model.entities().len(), 5);
        let indices = model
            .code_projections()?
            .iter()
            .map(|p| match p {
                AntlrProjection::Available(p) => p.entity_index,
                _ => panic!("clean exact native projection"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            indices.len(),
            2,
            "stock embeds Types, keeps Variables graph-visible"
        );
        let encoder = NativeNnModel {
            vector_id: [83; 32],
            dimensions: 2,
            model_id: "actual-db2-module-fixture",
            revision: "one",
        };
        let vectors = vec![vec![1., 0.]; indices.len()];
        let batch = encode_native_nn_antlr(
            &encoder,
            &[NativeNnAntlrInput {
                document: model,
                source: NativeNnSource::Indexed(&source),
                entity_indices: &indices,
                vectors: &vectors,
                model_generation: Some(7),
            }],
            &original,
        )?;
        validate_native_nn(&batch, &encoder, &original)?;
        assert_eq!(batch.num_rows(), 2);
        let names = batch
            .column_by_name("name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let uids = batch
            .column_by_name("uid")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for (row, &index) in indices.iter().enumerate() {
            assert_eq!(names.value(row), "T_F");
            assert_eq!(
                uids.value(row),
                model.entities()[index].identity().uid().as_str()
            );
        }
        assert_ne!(uids.value(0), uids.value(1));
        let graph = assemble_graph_antlr(
            AstGraphAntlrInput {
                document: model,
                source: NativeNnSource::Indexed(&source),
            },
            &original,
        )?;
        assert_eq!(graph.nodes().len(), 5);
        assert_eq!(graph.association().raw_version(), source.version());
        assert_eq!(graph.association().path(), "modules.db2");
        assert_eq!(graph.association().physical_branch(), Some("fixture"));
        assert!(graph.edges().is_empty());
        assert!(
            !graph.outcomes().is_empty(),
            "cursor SELECT is a real observed relation"
        );
        assert!(
            graph
                .outcomes()
                .iter()
                .all(|o| matches!(o.target(), AstGraphRelationTarget::ObservedOnly))
        );
        let mut keys = std::collections::BTreeSet::new();
        for (node, entity) in graph.nodes().iter().zip(model.entities()) {
            assert!(keys.insert(node.key()));
            assert_eq!(
                node.key(),
                serde_json::to_string(&[entity.identity().uid().as_str(), entity.data_key()])?
            );
            assert_eq!(text_property(node, "name"), entity.name());
            assert_eq!(text_property(node, "raw_name"), entity.name_raw().text());
            assert_eq!(
                node.properties()["name_start_byte"],
                AstGraphValue::Int64(entity.name_raw().start() as i64)
            );
            assert_eq!(node.properties()["available"], AstGraphValue::Bool(true));
            let AntlrEntityExtent::Exact(extent) = entity.extent() else {
                panic!("exact helper");
            };
            assert!(
                extent.range().text().starts_with("TYPE ")
                    || extent.range().text().starts_with("VARIABLE ")
            );
            assert!(!extent.range().text().starts_with("ALTER"));
            let m = metadata(node);
            assert_eq!(m["namespaces"].as_array().unwrap().len(), 1);
            assert_eq!(m["namespaces"][0]["name"], entity.namespace()[0].name());
            assert_eq!(
                m["qualified_name"]["components"].as_array().unwrap().len(),
                1
            );
        }
        assert!(
            !graph
                .nodes()
                .iter()
                .any(|n| n.label() == "Module" || text_property(n, "name") == "TABLE_F")
        );
        first_graph = Some(graph);
        Ok(())
    })?;
    with_model(text, &original, &draft, &["modules.db2"], |model| {
        let graph = assemble_graph_antlr(
            AstGraphAntlrInput {
                document: model,
                source: NativeNnSource::Indexed(&source),
            },
            &original,
        )?;
        assert_eq!(
            &graph,
            first_graph.as_ref().unwrap(),
            "deterministic exact owned rows"
        );
        Ok(())
    })?;
    drop(source);
    drop(draft);
    std::thread::spawn(move || {
        let parts = first_graph.unwrap().into_parts();
        assert_eq!(parts.nodes.len(), 5);
        assert!(!parts.outcomes.is_empty());
    })
    .join()
    .unwrap();
    Ok(())
}

#[tokio::test]
async fn oracle_callable_qnames_flow_through_genuine_graph_and_nn() -> TestResult {
    use graphit_ast::native_nn::{
        NativeNnAntlrInput, NativeNnModel, encode_native_nn_antlr, validate_native_nn,
    };
    let source = r#"CREATE PACKAGE BODY "Ω"."PKG" AS
 FUNCTION "F""Q" RETURN NUMBER IS v NUMBER := 1; BEGIN RETURN v; END;
 PROCEDURE p IS BEGIN NULL; END;
END "PKG";
/"#;
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let snapshot = raw(
        &temp,
        "callable-source.lance",
        "callables.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("genuine source")
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let mut wires = Vec::new();
    for _ in 0..2 {
        with_model(text, &original, &draft, &["callables.sql"], |model| {
            assert!(
                model.parsed().observations().syntax.diagnostics.is_empty(),
                "{:?}",
                model.parsed().observations().syntax.diagnostics
            );
            assert!(model.diagnostics().iter().all(|d| matches!(
                d.issue,
                graphit_ast::antlr_engine::model::AntlrIssue::DeclarationReference { .. }
            )));
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document: model,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(graph.nodes().len(), 4);
            assert_eq!(graph.association().raw_version(), snapshot.version());
            assert_eq!(graph.association().path(), "callables.sql");
            assert!(graph.edges().is_empty());
            let package = graph
                .nodes()
                .iter()
                .find(|n| n.label() == "Package")
                .unwrap();
            assert_eq!(text_property(package, "raw_name"), "\"PKG\"");
            let parts = metadata(package)["qualified_name"]["components"]
                .as_array()
                .unwrap()
                .clone();
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0]["decoded"], "Ω");
            assert_eq!(parts[1]["decoded"], "PKG");
            let function = graph
                .nodes()
                .iter()
                .find(|n| n.label() == "Function")
                .unwrap();
            assert_eq!(text_property(function, "name"), "F\"Q");
            assert_eq!(text_property(function, "raw_name"), "\"F\"\"Q\"");
            let owners = metadata(function)["namespaces"].as_array().unwrap().clone();
            assert_eq!(owners.len(), 1);
            assert_eq!(
                owners[0]["qualified_name"]["components"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            let nn_model = NativeNnModel {
                vector_id: [83; 32],
                dimensions: 2,
                model_id: "oracle-callable-codec-fixture",
                revision: "one",
            };
            let indices: Vec<_> = (0..model.entities().len()).collect();
            let vectors = vec![vec![1., 0.]; indices.len()];
            let batch = encode_native_nn_antlr(
                &nn_model,
                &[NativeNnAntlrInput {
                    document: model,
                    source: NativeNnSource::Indexed(&snapshot),
                    entity_indices: &indices,
                    vectors: &vectors,
                    model_generation: Some(7),
                }],
                &original,
            )?;
            validate_native_nn(&batch, &nn_model, &original)?;
            assert_eq!(
                batch.num_rows(),
                4,
                "fixture vectors test codec, not model inference"
            );
            wires.push(
                graph
                    .nodes()
                    .iter()
                    .map(|n| (n.key().to_owned(), n.properties().clone()))
                    .collect::<Vec<_>>(),
            );
            Ok(())
        })?;
    }
    assert_eq!(
        wires[0], wires[1],
        "all owned Graph properties stable across reparse"
    );
    Ok(())
}

#[tokio::test]
async fn oracle_typed_declarations_flow_through_genuine_graph_and_nn() -> TestResult {
    use graphit_ast::native_nn::{
        NativeNnAntlrInput, NativeNnModel, encode_native_nn_antlr, validate_native_nn,
    };
    let source = "CREATE PROCEDURE p(a IN NUMBER,b OUT NOCOPY NUMBER,c IN OUT NUMBER,d NUMBER DEFAULT fn(1,2)) IS TYPE r_t IS RECORD(id NUMBER NOT NULL); TYPE t_t IS TABLE OF NUMBER; TYPE a_t IS TABLE OF NUMBER INDEX BY VARCHAR2(10); TYPE v_t IS VARRAY(10+1) OF NUMBER; TYPE c_t IS REF CURSOR; comum NUMBER; BEGIN NULL; END; /";
    let mut profile =
        String::from_utf8(include_bytes!("fixtures/catalog/shipped/plsql.yaml").to_vec())?;
    assert!(profile.contains("embed_labels:\n"));
    profile = profile.replace("embed_labels:\n", "embed_labels:\n  - Parameter\n");
    let draft = catalog(profile.as_bytes());
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let snapshot = raw(
        &temp,
        "pt-source.lance",
        "typed.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("real source")
    };
    let mut graphs = Vec::new();
    for _ in 0..2 {
        with_model(text, &original, &draft, &["typed.sql"], |document| {
            assert!(
                document
                    .parsed()
                    .observations()
                    .syntax
                    .diagnostics
                    .is_empty(),
                "{:?}",
                document.parsed().observations().syntax.diagnostics
            );
            assert!(
                document.diagnostics().is_empty(),
                "{:?}",
                document
                    .diagnostics()
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>()
            );
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(graph.nodes().len(), 11);
            assert_eq!(graph.association().raw_version(), snapshot.version());
            assert_eq!(graph.association().path(), "typed.sql");
            assert!(graph.edges().is_empty());
            let mut type_kinds = std::collections::BTreeSet::new();
            for node in graph.nodes() {
                let m = metadata(node);
                match node.label() {
                    "Parameter" => {
                        assert_eq!(m["oracle_declaration"]["kind"], "Parameter");
                        if text_property(node, "name") == "d" {
                            assert_eq!(
                                m["oracle_declaration"]["default_value"]["native"]["literal_source"],
                                "fn(1,2)"
                            );
                            assert_eq!(
                                m["contributions"][0]["declared_value"]["status"],
                                "NotConfigured"
                            );
                        }
                    }
                    "Type" => {
                        assert_eq!(m["oracle_declaration"]["kind"], "Type");
                        type_kinds.insert(
                            m["oracle_declaration"]["type_kind"]
                                .as_str()
                                .unwrap()
                                .to_owned(),
                        );
                        if text_property(node, "name") == "v_t" {
                            assert_eq!(
                                m["oracle_declaration"]["bound"]["native"]["literal_source"],
                                "10+1"
                            );
                        }
                    }
                    _ => assert!(
                        m.get("oracle_declaration").is_none(),
                        "old metadata unchanged"
                    ),
                }
            }
            assert_eq!(type_kinds.len(), 5);
            let nn_model = NativeNnModel {
                vector_id: [83; 32],
                dimensions: 2,
                model_id: "oracle-pt-codec-fixture",
                revision: "one",
            };
            let indices: Vec<_> = (0..document.entities().len()).collect();
            let vectors = vec![vec![1., 0.]; indices.len()];
            let batch = encode_native_nn_antlr(
                &nn_model,
                &[NativeNnAntlrInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                    entity_indices: &indices,
                    vectors: &vectors,
                    model_generation: Some(7),
                }],
                &original,
            )?;
            validate_native_nn(&batch, &nn_model, &original)?;
            assert_eq!(batch.num_rows(), 11, "fixture vectors only");
            graphs.push(graph);
            Ok(())
        })?;
    }
    let wires: Vec<_> = graphs
        .iter()
        .map(|g| {
            g.nodes()
                .iter()
                .map(|n| (n.key().to_owned(), n.properties().clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(wires[0], wires[1]);
    let graph = graphs.pop().unwrap();
    assert_eq!(
        std::thread::spawn(move || graph.nodes().len())
            .join()
            .unwrap(),
        11
    );
    Ok(())
}

#[tokio::test]
async fn oracle_applications_reach_genuine_graph_as_owned_observations() -> TestResult {
    let source = "CREATE PROCEDURE p(a NUMBER DEFAULT sig()) IS v NUMBER := init_value(); BEGIN v := f(1,nome=>g(2))(3); v := obj.EXISTS(1)(2).WHEN(3); first_call(1).second_call(2); bare_call; EXECUTE IMMEDIATE 'BEGIN target; END;'; END; / SELECT CHR(65 USING NCHAR_CS),top_call() FROM dual;";
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let snapshot = raw(
        &temp,
        "apps-source.lance",
        "applications.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("actual Raw Source")
    };
    let mut graphs = Vec::new();
    for _ in 0..2 {
        with_model(text, &original, &draft, &["applications.sql"], |document| {
            assert!(
                document
                    .parsed()
                    .observations()
                    .syntax
                    .diagnostics
                    .is_empty(),
                "{:?}",
                document.parsed().observations().syntax.diagnostics
            );
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(graph.association().raw_version(), snapshot.version());
            assert!(graph.edges().is_empty());
            assert_eq!(
                graph.outcomes().len(),
                document.unresolved_relations().len()
            );
            let mut applications = 0;
            for outcome in graph.outcomes() {
                let value: serde_json::Value = serde_json::from_str(outcome.observation_json())?;
                if let Some(call) = document.oracle_call(outcome.relation_index()) {
                    assert!(matches!(
                        outcome.target(),
                        AstGraphRelationTarget::NotFound
                            | AstGraphRelationTarget::Unresolved { .. }
                    ));
                    assert_eq!(outcome.source_index(), matches!(call.source, graphit_ast::antlr_engine::model::AntlrOracleCallSourceState::Bound { .. }).then_some(call.call_index));
                    assert!(value["oracle_call"].is_object());
                } else {
                    assert!(matches!(
                        outcome.target(),
                        AstGraphRelationTarget::ObservedOnly
                    ));
                    assert!(outcome.source_index().is_none());
                    assert_eq!(value["source_state"], "ObservedAntlrRelation");
                }
                if let Some(application) = value.get("oracle_application") {
                    applications += 1;
                    for boundary in application["boundaries"].as_array().unwrap() {
                        assert!(
                            boundary.get("literal_source").is_none(),
                            "no body corpus per relation"
                        );
                    }
                    assert_eq!(
                        application["occurrence_index"],
                        value["occurrence"]["occurrence_index"]
                    );
                    assert!(!application["head"]["original"].is_null());
                    if application["head"]["literal_source"] == "f(1,nome=>g(2))(3)" {
                        assert_eq!(application["groups"].as_array().unwrap().len(), 2);
                        assert_eq!(
                            application["groups"][0]["arguments"]
                                .as_array()
                                .unwrap()
                                .len(),
                            2
                        );
                        assert_eq!(
                            application["groups"][0]["arguments"][1]["name"]["literal_source"],
                            "nome"
                        );
                        assert_eq!(application["boundaries"][0]["kind"], "body");
                    }
                } else {
                    assert!(
                        document
                            .oracle_application(
                                value["occurrence"]["occurrence_index"].as_u64().unwrap() as usize
                            )
                            .is_none()
                    );
                }
            }
            assert_eq!(applications, document.oracle_applications().len());
            assert!(applications >= 10);
            graphs.push(graph);
            Ok(())
        })?;
    }
    let wire = |g: &graphit_ast::semantic_graph::AstSemanticGraphBatch| -> TestResult {
        assert!(!g.file_observation().properties().is_empty());
        Ok(())
    };
    wire(&graphs[0])?;
    assert_eq!(
        graphs[0]
            .nodes()
            .iter()
            .map(|n| (n.key(), n.properties()))
            .collect::<Vec<_>>(),
        graphs[1]
            .nodes()
            .iter()
            .map(|n| (n.key(), n.properties()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        graphs[0].file_observation().properties(),
        graphs[1].file_observation().properties()
    );
    assert_eq!(
        graphs[0]
            .outcomes()
            .iter()
            .map(|o| o.observation_json())
            .collect::<Vec<_>>(),
        graphs[1]
            .outcomes()
            .iter()
            .map(|o| o.observation_json())
            .collect::<Vec<_>>()
    );
    let graph = graphs.pop().unwrap();
    let count = graph.outcomes().len();
    assert_eq!(
        std::thread::spawn(move || graph.outcomes().len())
            .join()
            .unwrap(),
        count
    );
    Ok(())
}

#[tokio::test]
async fn oracle_collection_assignment_facts_reach_genuine_owned_graph() -> TestResult {
    let source = "CREATE PROCEDURE p(items IN NUMBER) IS BEGIN items.DELETE(1,idx(2)); items.EXTEND; items.TRIM(2); items(1) := fn(2); items(idx(1)) := value_fn(2); END; /";
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let snapshot = raw(
        &temp,
        "collection-source.lance",
        "collections.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("genuine source")
    };
    let mut graphs = Vec::new();
    for _ in 0..2 {
        with_model(text, &original, &draft, &["collections.sql"], |document| {
            assert!(
                document
                    .parsed()
                    .observations()
                    .syntax
                    .diagnostics
                    .is_empty(),
                "{:?}",
                document.parsed().observations().syntax.diagnostics
            );
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(graph.association().raw_version(), snapshot.version());
            assert!(
                graph.edges().is_empty(),
                "native NUMBER parameter is not a proven collection binding"
            );
            let mut collections = 0;
            let mut targets = 0;
            let mut values = 0;
            for outcome in graph.outcomes() {
                let value: serde_json::Value = serde_json::from_str(outcome.observation_json())?;
                if let Some(call) = document.oracle_call(outcome.relation_index()) {
                    assert!(matches!(
                        outcome.target(),
                        AstGraphRelationTarget::NotFound
                            | AstGraphRelationTarget::Unresolved { .. }
                    ));
                    assert_eq!(outcome.source_index(), matches!(call.source, graphit_ast::antlr_engine::model::AntlrOracleCallSourceState::Bound { .. }).then_some(call.call_index));
                    assert!(value["oracle_call"].is_object());
                } else {
                    assert!(matches!(
                        outcome.target(),
                        AstGraphRelationTarget::ObservedOnly
                    ));
                    assert!(outcome.source_index().is_none());
                    assert_eq!(value["source_state"], "ObservedAntlrRelation");
                }
                let Some(app) = value.get("oracle_application") else {
                    continue;
                };
                if let Some(collection) = app.get("collection_statement") {
                    collections += 1;
                    assert_eq!(app["kind"], "CollectionMethodStatement");
                    assert!(app["groups"].as_array().unwrap().is_empty());
                    assert!(collection["syntax"].get("literal_source").is_none());
                    assert_eq!(collection["receiver"]["native"]["literal_source"], "items");
                    for delimiter in collection["delimiters"].as_array().unwrap() {
                        assert!(delimiter.get("literal_source").is_none());
                    }
                }
                if let Some(assignment) = app.get("assignment") {
                    assert!(assignment["statement"].get("literal_source").is_none());
                    assert!(assignment["branch"].get("literal_source").is_none());
                    for expr in assignment["expressions"].as_array().unwrap() {
                        assert!(expr.get("literal_source").is_none());
                    }
                    if assignment["side"] == "Target" {
                        targets += 1;
                    } else {
                        assert_eq!(assignment["side"], "Value");
                        values += 1;
                    }
                    if app["head"]["literal_source"] == "idx(1)" {
                        assert_eq!(assignment["side"], "Target");
                        assert!(
                            !assignment["expressions"].as_array().unwrap().is_empty(),
                            "index evaluation isn't inferred write"
                        );
                    }
                }
            }
            assert_eq!(collections, 3);
            assert_eq!(targets, 3);
            assert_eq!(values, 2);
            graphs.push(graph);
            Ok(())
        })?;
    }
    assert_eq!(
        graphs[0]
            .nodes()
            .iter()
            .map(|n| (n.key(), n.properties()))
            .collect::<Vec<_>>(),
        graphs[1]
            .nodes()
            .iter()
            .map(|n| (n.key(), n.properties()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        graphs[0].file_observation().properties(),
        graphs[1].file_observation().properties()
    );
    assert_eq!(
        graphs[0]
            .outcomes()
            .iter()
            .map(|o| o.observation_json())
            .collect::<Vec<_>>(),
        graphs[1]
            .outcomes()
            .iter()
            .map(|o| o.observation_json())
            .collect::<Vec<_>>()
    );
    let graph = graphs.pop().unwrap();
    assert!(
        std::thread::spawn(move || !graph.outcomes().is_empty())
            .join()
            .unwrap()
    );
    Ok(())
}

#[tokio::test]
async fn oracle_local_binding_proofs_reach_genuine_owned_graph() -> TestResult {
    let source = "DECLARE TYPE numbers_t IS TABLE OF NUMBER; xs numbers_t; BEGIN xs(1):=2; xs.DELETE(1); sink:=numbers_t(1); END; /";
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let snapshot = raw(
        &temp,
        "binding-source.lance",
        "bindings.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("genuine Raw Source");
    };
    let mut graphs = Vec::new();
    for _ in 0..2 {
        with_model(text, &original, &draft, &["bindings.sql"], |document| {
            assert!(
                document
                    .parsed()
                    .observations()
                    .syntax
                    .diagnostics
                    .is_empty()
            );
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(graph.association().raw_version(), snapshot.version());
            assert!(graph.edges().is_empty());
            let mut classified = std::collections::BTreeSet::new();
            for outcome in graph.outcomes() {
                assert!(outcome.source_index().is_none());
                assert!(matches!(
                    outcome.target(),
                    AstGraphRelationTarget::ObservedOnly
                ));
                let value: serde_json::Value = serde_json::from_str(outcome.observation_json())?;
                let binding = &value["oracle_binding"];
                assert_eq!(binding["receiver"]["state"], "Resolved");
                classified.insert(binding["classification"].as_str().unwrap().to_owned());
                assert!(
                    binding["receiver"]["scope_continuity"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|c| c["verified"] == true)
                );
                assert!(!binding["type_evidence"].as_array().unwrap().is_empty());
                for field in binding["type_evidence"].as_array().unwrap() {
                    assert!(field.get("literal_source").is_none());
                }
                let candidates = binding["receiver"]["candidates"].as_array().unwrap();
                assert_eq!(candidates.len(), 1);
                for candidate in candidates {
                    assert!(!candidate["entity_indices"].as_array().unwrap().is_empty());
                    assert!(candidate["declaration"].get("literal_source").is_none());
                    assert!(candidate["scope"].get("literal_source").is_none());
                    for marker in candidate["scope_evidence"].as_array().unwrap() {
                        assert!(marker.get("literal_source").is_none());
                    }
                }
                assert!(
                    binding["receiver"]["reference"]
                        .get("literal_source")
                        .is_none()
                );
                if binding["receiver"]["candidates"][0]["kind"] == "Value" {
                    assert_eq!(binding["declared_type"]["state"], "Resolved");
                    assert_eq!(binding["declared_type"]["candidates"][0]["kind"], "Type");
                }
            }
            assert_eq!(
                classified,
                std::collections::BTreeSet::from([
                    "CollectionIndex".to_owned(),
                    "CollectionMethod".to_owned(),
                    "CollectionConstructor".to_owned()
                ])
            );
            graphs.push(graph);
            Ok(())
        })?;
    }
    assert_eq!(
        graphs[0]
            .nodes()
            .iter()
            .map(|n| (n.key(), n.properties()))
            .collect::<Vec<_>>(),
        graphs[1]
            .nodes()
            .iter()
            .map(|n| (n.key(), n.properties()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        graphs[0].file_observation().properties(),
        graphs[1].file_observation().properties()
    );
    assert_eq!(
        graphs[0]
            .outcomes()
            .iter()
            .map(|o| o.observation_json())
            .collect::<Vec<_>>(),
        graphs[1]
            .outcomes()
            .iter()
            .map(|o| o.observation_json())
            .collect::<Vec<_>>()
    );
    let graph = graphs.pop().unwrap();
    assert!(
        std::thread::spawn(move || !graph.outcomes().is_empty())
            .join()
            .unwrap()
    );
    Ok(())
}

#[tokio::test]
async fn oracle_collection_members_reach_owned_graph_with_real_continuity() -> TestResult {
    fn structural(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                assert!(!fields.contains_key("literal_source"));
                for value in fields.values() {
                    structural(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    structural(value);
                }
            }
            _ => {}
        }
    }
    let source = "DECLARE TYPE values_t IS TABLE OF NUMBER; xs values_t; n NUMBER; b BOOLEAN; BEGIN n:=xs.COUNT; n:=xs.FIRST; n:=xs.LAST; n:=xs.LIMIT; n:=xs.NEXT(1); b:=xs.EXISTS(1); END; /";
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let snapshot = raw(
        &temp,
        "member-source.lance",
        "members.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("genuine Raw Source")
    };
    let mut graphs = Vec::new();
    with_model(text, &original, &draft, &["members.sql"], |first| {
        with_model(text, &original, &draft, &["members.sql"], |second| {
            for document in [first, second] {
                assert!(
                    document
                        .parsed()
                        .observations()
                        .syntax
                        .diagnostics
                        .is_empty()
                );
                let graph = assemble_graph_antlr(
                    AstGraphAntlrInput {
                        document,
                        source: NativeNnSource::Indexed(&snapshot),
                    },
                    &original,
                )?;
                assert_eq!(graph.association().raw_version(), snapshot.version());
                assert!(graph.edges().is_empty());
                let mut methods = std::collections::BTreeSet::new();
                for outcome in graph.outcomes() {
                    assert!(outcome.source_index().is_none());
                    assert!(matches!(
                        outcome.target(),
                        AstGraphRelationTarget::ObservedOnly
                    ));
                    let value: serde_json::Value =
                        serde_json::from_str(outcome.observation_json())?;
                    let member = &value["oracle_application"]["member"];
                    assert!(member.is_object());
                    structural(member);
                    assert_eq!(member["receiver"]["kind"], "general_element");
                    assert_eq!(member["separator"]["kind"], "PERIOD");
                    let binding = &value["oracle_binding"];
                    assert_eq!(binding["receiver"]["state"], "Resolved");
                    assert_eq!(binding["classification"], "CollectionMethod");
                    assert_eq!(binding["declared_type"]["state"], "Resolved");
                    assert_eq!(binding["member_continuity"]["verified"], true);
                    assert_eq!(binding["member_continuity"]["syntax"], member["syntax"]);
                    structural(&binding["member_continuity"]);
                    assert!(!binding["type_evidence"].as_array().unwrap().is_empty());
                    methods.insert(binding["collection_method"].as_str().unwrap().to_owned());
                }
                assert_eq!(
                    methods,
                    ["Count", "First", "Last", "Limit", "Next", "Exists"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect()
                );
                graphs.push(graph);
            }
            Ok(())
        })
    })?;
    assert_eq!(
        graphs[0]
            .nodes()
            .iter()
            .map(|node| (node.key(), node.properties()))
            .collect::<Vec<_>>(),
        graphs[1]
            .nodes()
            .iter()
            .map(|node| (node.key(), node.properties()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        graphs[0].file_observation().properties(),
        graphs[1].file_observation().properties()
    );
    assert_eq!(
        graphs[0]
            .outcomes()
            .iter()
            .map(|outcome| outcome.observation_json())
            .collect::<Vec<_>>(),
        graphs[1]
            .outcomes()
            .iter()
            .map(|outcome| outcome.observation_json())
            .collect::<Vec<_>>()
    );
    let graph = graphs.pop().unwrap();
    assert!(
        std::thread::spawn(move || !graph.outcomes().is_empty())
            .join()
            .unwrap()
    );
    Ok(())
}

#[tokio::test]
async fn oracle_local_calls_emit_real_owned_edges_and_source_ordinals() -> TestResult {
    use graphit_ast::antlr_engine::model::AntlrOracleCallSourceState;
    let source = "DECLARE FUNCTION helper(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE sink(v NUMBER) IS BEGIN NULL; END; PROCEDURE caller IS x NUMBER; BEGIN x := helper(1); x := helper(2); sink(3); END; BEGIN caller; END; /";
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let snapshot = raw(
        &temp,
        "oracle-call-source.lance",
        "calls.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("real Raw Source");
    };
    let mut graphs = Vec::new();
    for _ in 0..2 {
        with_model(text, &original, &draft, &["calls.sql"], |document| {
            assert!(
                document
                    .parsed()
                    .observations()
                    .syntax
                    .diagnostics
                    .is_empty()
            );
            let graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(
                graph.edges().len(),
                3,
                "two separate helper captures and one sink; root caller has no source"
            );
            let caller = graph
                .nodes()
                .iter()
                .find(|n| text_property(n, "name") == "caller")
                .unwrap();
            let mut edge_ids = std::collections::BTreeSet::new();
            for edge in graph.edges() {
                assert_eq!(edge.kind(), "CALLS");
                assert_eq!(edge.source_key(), caller.key());
                let target = graph
                    .nodes()
                    .iter()
                    .find(|n| n.key() == edge.target_key())
                    .unwrap();
                assert!(matches!(text_property(target, "name"), "helper" | "sink"));
                assert!(matches!(target.label(), "Function" | "Procedure"));
                assert!(edge_ids.insert(edge.uid()));
                let uid: serde_json::Value = serde_json::from_str(edge.uid())?;
                assert_eq!(uid[0], "CALLS");
                assert_eq!(uid[1], edge.source_key());
                assert_eq!(uid[2], edge.target_key());
                assert_eq!(uid[3], "calls");
                assert!(uid[4].as_u64().unwrap() < uid[5].as_u64().unwrap());
            }
            let mut ordinal_differs = false;
            for call in document.oracle_calls() {
                let outcome = graph
                    .outcomes()
                    .iter()
                    .find(|o| o.relation_index() == call.occurrence_index)
                    .unwrap();
                assert!(
                    outcome.capture_index().is_none(),
                    "no invented native capture ordinal"
                );
                let value: serde_json::Value = serde_json::from_str(outcome.observation_json())?;
                assert_eq!(value["oracle_call"]["call_index"], call.call_index);
                fn structural(v: &serde_json::Value) {
                    match v {
                        serde_json::Value::Object(o) => {
                            assert!(!o.contains_key("literal_source"));
                            assert!(!o.contains_key("text"));
                            for child in o.values() {
                                structural(child);
                            }
                        }
                        serde_json::Value::Array(a) => {
                            for child in a {
                                structural(child);
                            }
                        }
                        _ => {}
                    }
                }
                structural(&value["oracle_call"]);
                match call.source {
                    AntlrOracleCallSourceState::Bound { entity_index } => {
                        assert_eq!(outcome.source_index(), Some(call.call_index));
                        ordinal_differs |= entity_index != call.call_index;
                        assert_eq!(value["source_state"], "Bound");
                        assert!(matches!(
                            outcome.target(),
                            AstGraphRelationTarget::Resolved { .. }
                        ));
                    }
                    _ => assert!(outcome.source_index().is_none()),
                }
                if value["oracle_application"]["head"]["literal_source"] == "sink" {
                    assert_eq!(
                        value["oracle_binding"]["receiver"]["state"],
                        "Indeterminate"
                    );
                    assert_eq!(value["oracle_call"]["call_lookup"]["state"], "Resolved");
                    assert_eq!(
                        value["oracle_call"]["call_lookup"]["candidates"][0]["kind"],
                        "Other"
                    );
                }
            }
            assert!(
                ordinal_differs,
                "entity_index cannot substitute for call ordinal"
            );
            graphs.push(graph);
            Ok(())
        })?;
    }
    assert_eq!(
        graphs[0], graphs[1],
        "entire owned batch independent of native owner"
    );
    with_model(text, &original, &draft, &["calls.sql"], |document| {
        original.cancellation().cancel();
        assert!(
            matches!(assemble_graph_antlr(AstGraphAntlrInput { document, source: NativeNnSource::Indexed(&snapshot) }, &original),
            Err(AstGraphError::Operation(e)) if e.kind() == std::io::ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    drop(snapshot);
    drop(original);
    drop(temp);
    let owned = graphs.pop().unwrap();
    let owned = std::thread::spawn(move || {
        let wire = serde_json::to_vec(&owned).unwrap();
        let restored: graphit_ast::semantic_graph::AstSemanticGraphBatch =
            serde_json::from_slice(&wire).unwrap();
        assert_eq!(owned, restored);
        owned
    })
    .join()
    .unwrap();
    assert_eq!(owned.edges().len(), 3);
    Ok(())
}

#[tokio::test]
async fn oracle_call_target_cause_does_not_borrow_source_failure_node() -> TestResult {
    use graphit_ast::{
        antlr_engine::model::{
            AntlrOracleCallReason as Reason, AntlrOracleCallTargetState as Target,
        },
        prepared_input::SyntheticReason,
        tree_sitter_model::TsLocalUnresolvedReason,
    };
    let source = "DECLARE FUNCTION helper(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE caller IS x NUMBER; BEGIN x := helper(1); END; BEGIN caller; END; /";
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let snapshot = raw(
        &temp,
        "call-cause.lance",
        "call-cause.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("actual Raw Source");
    };
    let start = text.find("BEGIN x").unwrap() as u64;
    let end = start + 5;
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
            prepared: ByteSpan { start, end },
            reason: SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: end,
                end: text.len() as u64,
            },
            original: ByteSpan {
                start: end,
                end: text.len() as u64,
            },
        },
    ];
    let removed = [ByteSpan { start, end }];
    let prepared = text.to_owned();
    let input = PreparedInput::new(
        text.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed =
        ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["call-cause.sql"], limits())?;
    assert!(parsed.observations().syntax.diagnostics.is_empty());
    let model = parsed.assemble()?;
    let call = model
        .oracle_calls()
        .iter()
        .find(|c| {
            model
                .oracle_application(c.occurrence_index)
                .unwrap()
                .head
                .node
                .span
                .as_ref()
                .is_some_and(|span| prepared.get(span.start..span.end) == Some("helper(1)"))
        })
        .unwrap();
    assert_eq!(
        call.target,
        Target::Unresolved {
            reason: Reason::NonExactOrigin
        }
    );
    let matching = call
        .issues
        .iter()
        .filter(|i| i.reason == Reason::NonExactOrigin)
        .collect::<Vec<_>>();
    assert!(
        matching.iter().any(|i| i.node_index.is_some()),
        "source has its own native failing field"
    );
    assert!(
        matching.last().unwrap().node_index.is_none(),
        "B1 target did not supply a causal node"
    );
    let graph = assemble_graph_antlr(
        AstGraphAntlrInput {
            document: &model,
            source: NativeNnSource::Indexed(&snapshot),
        },
        &original,
    )?;
    assert!(graph.edges().is_empty());
    let outcome = graph
        .outcomes()
        .iter()
        .find(|o| o.relation_index() == call.occurrence_index)
        .unwrap();
    let head_index = model
        .oracle_application(call.occurrence_index)
        .unwrap()
        .head
        .node_index;
    assert_eq!(
        outcome.target(),
        &AstGraphRelationTarget::Unresolved {
            reason: TsLocalUnresolvedReason::UnsupportedSyntax {
                node_index: head_index
            },
        }
    );
    let metadata: serde_json::Value = serde_json::from_str(outcome.observation_json())?;
    assert!(
        metadata["oracle_call"]["issues"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["node_index"]
            .is_null()
    );
    assert_eq!(
        metadata["oracle_binding"]["receiver"]["issues"][0],
        "NonExactOrigin"
    );
    Ok(())
}

#[tokio::test]
async fn oracle_calls_duplicate_queries_keep_capture_uid_and_simultaneous_owners() -> TestResult {
    use graphit_ast::antlr_engine::{
        model::AntlrOracleCallSourceState as Source, semantic::CaptureBinding,
    };
    use graphit_ast::prepared_input::OriginalProjection;
    let stock = std::str::from_utf8(include_bytes!("fixtures/catalog/shipped/plsql.yaml"))?;
    let duplicate_queries = "queries:\n  - {data_key: calls, graph_label: '', pattern: '//general_element/general_element_part', child_rule: 'function_argument', type: relation, relation_type: CALLS, captures: {name: [{xpath: '/id_expression'}]}}\n  - {data_key: alternate_calls, graph_label: '', pattern: '//general_element_part', child_rule: 'function_argument', type: relation, relation_type: CALLS, captures: {name: [{xpath: '/id_expression'}]}}\n";
    let source = "DECLARE FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE p IS x NUMBER; BEGIN x := f(1); END; BEGIN p; END; /";
    let original = Arc::new(context());
    let temp = TempDir::new()?;
    let snapshot = raw(
        &temp,
        "duplicate-call-source.lance",
        "duplicate-calls.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("real Raw Source");
    };
    let mut owned_pairs = Vec::new();
    for duplicate_caller in [false, true] {
        let mut profile = stock.replace("queries:\n", duplicate_queries);
        if duplicate_caller {
            profile = profile.replace("queries:\n", "queries:\n  - {data_key: alternate_procedures, graph_label: Procedure, pattern: '//procedure_body/identifier', type: entity}\n");
        }
        let draft = catalog(profile.as_bytes());
        with_source(text, &original, |input| {
            let first = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["duplicate-calls.sql"],
                limits(),
            )?;
            let second = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["duplicate-calls.sql"],
                limits(),
            )?;
            assert!(first.observations().syntax.diagnostics.is_empty());
            assert!(second.observations().syntax.diagnostics.is_empty());
            assert!(
                !std::ptr::eq(
                    first.observations().syntax.nodes.as_ptr(),
                    second.observations().syntax.nodes.as_ptr()
                ),
                "two simultaneously live native owners"
            );
            let first_model = first.assemble()?;
            let second_model = second.assemble()?;
            let first_graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document: &first_model,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            let second_graph = assemble_graph_antlr(
                AstGraphAntlrInput {
                    document: &second_model,
                    source: NativeNnSource::Indexed(&snapshot),
                },
                &original,
            )?;
            assert_eq!(
                first_graph, second_graph,
                "full equality while both native owners remain alive"
            );
            assert_eq!(
                first_graph.outcomes().len(),
                first_model.unresolved_relations().len()
            );
            let mut matching = 0;
            let mut keys = std::collections::BTreeMap::<String, usize>::new();
            for call in first_model.oracle_calls() {
                let app = first_model
                    .oracle_application(call.occurrence_index)
                    .unwrap();
                if !matches!(app.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(1)")
                {
                    continue;
                }
                matching += 1;
                let outcome = first_graph
                    .outcomes()
                    .iter()
                    .find(|o| o.relation_index() == call.occurrence_index)
                    .unwrap();
                let occurrence = &first.observations().occurrences[call.occurrence_index];
                if duplicate_caller {
                    assert!(
                        matches!(&call.source, Source::Ambiguous { entity_indices } if entity_indices.len() == 2)
                    );
                    assert!(outcome.source_index().is_none());
                    assert!(matches!(
                        outcome.target(),
                        AstGraphRelationTarget::Resolved { .. }
                    ));
                    continue;
                }
                let Source::Bound { entity_index } = call.source else {
                    panic!("{:?}", call.source);
                };
                let AstGraphRelationTarget::Resolved { key: target_key } = outcome.target() else {
                    panic!("{:?}", outcome.target());
                };
                let CaptureBinding::Present(capture) = &occurrence.name else {
                    panic!("real capture");
                };
                let Some(OriginalProjection::Exact(anchor)) = &capture.original else {
                    panic!("exact Original capture");
                };
                assert_eq!(anchor.text(), "f");
                assert_eq!(text.get(anchor.start()..anchor.end()), Some(anchor.text()));
                let source_key = first_graph.nodes()[entity_index].key();
                let expected_uid = serde_json::to_string(&(
                    occurrence.query.relation_type().value(),
                    source_key,
                    target_key,
                    occurrence.query.data_key().value(),
                    anchor.start(),
                    anchor.end(),
                ))?;
                assert!(first_graph.edges().iter().any(|e| e.uid() == expected_uid));
                *keys
                    .entry(occurrence.query.data_key().value().clone())
                    .or_default() += 1;
            }
            assert_eq!(
                matching, 3,
                "every concordant/different-key occurrence retained"
            );
            if duplicate_caller {
                assert!(
                    first_graph.edges().is_empty(),
                    "ambiguous source cannot choose a row"
                );
            } else {
                assert_eq!(keys.get("calls"), Some(&2));
                assert_eq!(keys.get("alternate_calls"), Some(&1));
                assert_eq!(
                    first_graph.edges().len(),
                    2,
                    "same-key concordant calls coalesce; different keys remain distinct"
                );
                assert_ne!(first_graph.edges()[0].uid(), first_graph.edges()[1].uid());
            }
            owned_pairs.push((first_graph, second_graph));
            Ok(())
        })?;
    }
    drop(snapshot);
    drop(original);
    drop(temp);
    let owned_pairs = std::thread::spawn(move || owned_pairs).join().unwrap();
    assert_eq!(owned_pairs.len(), 2);
    for (first, second) in owned_pairs {
        assert_eq!(
            first, second,
            "owned after both native owners and Source drop"
        );
    }
    Ok(())
}

async fn oracle_target_fixture(
    temp: &TempDir,
    storage: &str,
    path: &str,
    source: &str,
    profile: &[u8],
) -> Result<
    (
        graphit_ast::semantic_graph::oracle::AstOracleGraphRows,
        graphit_ast::semantic_graph::AstSemanticGraphBatch,
    ),
    Box<dyn Error>,
> {
    use graphit_ast::semantic_graph::oracle::collect_oracle_graph_rows;
    let original = Arc::new(context());
    let snapshot = raw(temp, storage, path, Some(source), &original).await?;
    let draft = catalog(profile);
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("Source")
    };
    let mut result = None;
    with_model(text, &original, &draft, &[path], |document| {
        assert!(
            document
                .parsed()
                .observations()
                .syntax
                .diagnostics
                .is_empty()
        );
        let batch = assemble_graph_antlr(
            AstGraphAntlrInput {
                document,
                source: NativeNnSource::Indexed(&snapshot),
            },
            &original,
        )?;
        let rows = collect_oracle_graph_rows(document, &batch, &original)?;
        result = Some((rows, batch));
        Ok(())
    })?;
    Ok(result.unwrap())
}

#[tokio::test]
async fn oracle_target_rules_resolve_unique_real_cross_file_tables_and_file_outcomes() -> TestResult
{
    use graphit_ast::semantic_graph::oracle::{
        AstOracleGraphRows, apply_oracle_graph_resolution, resolve_oracle_graph_reference,
    };
    let temp = TempDir::new()?;
    let profile = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    let (target, _) = oracle_target_fixture(
        &temp,
        "target.lance",
        "target.sql",
        "CREATE TABLE target_tab(id NUMBER);",
        profile,
    )
    .await?;
    let source =
        "CREATE PROCEDURE caller AS v NUMBER; BEGIN SELECT id INTO v FROM target_tab; END; /";
    let (rows, batch) =
        oracle_target_fixture(&temp, "caller.lance", "caller.sql", source, profile).await?;
    assert_eq!(target.endpoints().len(), 1);
    assert!(!rows.references().is_empty());
    let encoded = serde_json::to_vec(&rows)?;
    let rows: AstOracleGraphRows =
        std::thread::spawn(move || serde_json::from_slice(&encoded).unwrap())
            .join()
            .unwrap();
    let original = Arc::new(context());
    let caller = batch
        .nodes()
        .iter()
        .find(|n| n.label() == "Procedure")
        .unwrap();
    let mut file = batch.file_observation().clone();
    let mut unique_edges = std::collections::BTreeSet::new();
    for reference in rows.references() {
        let resolution = resolve_oracle_graph_reference(reference, target.endpoints(), &original)?;
        assert!(matches!(
            resolution.outcome().target(),
            AstGraphRelationTarget::Resolved { .. }
        ));
        assert_eq!(resolution.edges().len(), 1);
        let edge = &resolution.edges()[0];
        assert_eq!(edge.kind(), "SELECTS");
        assert_eq!(edge.source_key(), caller.key());
        assert_eq!(edge.target_key(), target.endpoints()[0].key());
        let uid: serde_json::Value = serde_json::from_str(edge.uid())?;
        let start = uid[4].as_u64().unwrap() as usize;
        let end = uid[5].as_u64().unwrap() as usize;
        assert_eq!(&source[start..end], "target_tab");
        unique_edges.insert(edge.uid().to_owned());
        apply_oracle_graph_resolution(&mut file, &resolution, &original)?;
        let before = file.clone();
        assert!(apply_oracle_graph_resolution(&mut file, &resolution, &original).is_err());
        assert_eq!(file, before);
    }
    assert_eq!(
        unique_edges.len(),
        1,
        "duplicate native queries retain one occurrence edge identity"
    );
    let outcomes: serde_json::Value =
        serde_json::from_str(text_property(&file, "resolution_outcomes_json"))?;
    assert!(
        outcomes
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["target"]["state"] == "Resolved")
    );
    let (repeat, _) =
        oracle_target_fixture(&temp, "repeat.lance", "caller.sql", source, profile).await?;
    assert_eq!(
        rows, repeat,
        "native owners and SDK allocations do not enter persisted rows"
    );
    Ok(())
}

#[tokio::test]
async fn oracle_target_rules_keep_zero_ambiguity_and_label_policy_without_edges() -> TestResult {
    use graphit_ast::semantic_graph::oracle::resolve_oracle_graph_reference;
    let temp = TempDir::new()?;
    let profile = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    let (a, _) = oracle_target_fixture(
        &temp,
        "a.lance",
        "a.sql",
        "CREATE TABLE target_tab(id NUMBER);",
        profile,
    )
    .await?;
    let (b, _) = oracle_target_fixture(
        &temp,
        "b.lance",
        "b.sql",
        "CREATE TABLE target_tab(id NUMBER);",
        profile,
    )
    .await?;
    let source =
        "CREATE PROCEDURE caller AS v NUMBER; BEGIN SELECT id INTO v FROM target_tab; END; /";
    let (rows, _) = oracle_target_fixture(&temp, "r.lance", "r.sql", source, profile).await?;
    let original = Arc::new(context());
    let candidates = a
        .endpoints()
        .iter()
        .chain(b.endpoints())
        .cloned()
        .collect::<Vec<_>>();
    for reference in rows.references() {
        let zero = resolve_oracle_graph_reference(reference, &[], &original)?;
        assert!(matches!(
            zero.outcome().target(),
            AstGraphRelationTarget::NotFound
        ));
        assert!(zero.edges().is_empty());
        let ambiguous = resolve_oracle_graph_reference(reference, &candidates, &original)?;
        assert!(
            matches!(ambiguous.outcome().target(),AstGraphRelationTarget::Ambiguous{keys} if keys.len()==2)
        );
        assert!(ambiguous.edges().is_empty());
    }
    let profile=String::from_utf8(profile.to_vec())?.replace(
        "SELECTS: { labels: [Table, View, MaterializedView, Synonym, Sequence, Package, Procedure, Function, Trigger, Type, Index, DBLink, Tablespace], fallback: Table }",
        "SELECTS: { labels: [View] }"
    );
    let (restricted, _) = oracle_target_fixture(
        &temp,
        "restricted.lance",
        "restricted.sql",
        source,
        profile.as_bytes(),
    )
    .await?;
    assert!(!restricted.references().is_empty());
    for reference in restricted.references() {
        let result = resolve_oracle_graph_reference(reference, a.endpoints(), &original)?;
        assert!(matches!(
            result.outcome().target(),
            AstGraphRelationTarget::NotFound
        ));
        assert!(result.edges().is_empty());
    }

    let union_profile = profile.replace(
        "relation_type: SELECTS",
        "relation_type: SELECTS\n    target_labels: [Table]",
    );
    let (union_rows, _) = oracle_target_fixture(
        &temp,
        "union.lance",
        "union.sql",
        source,
        union_profile.as_bytes(),
    )
    .await?;
    assert!(!union_rows.references().is_empty());
    for reference in union_rows.references() {
        let result = resolve_oracle_graph_reference(reference, a.endpoints(), &original)?;
        assert_eq!(
            result.edges().len(),
            1,
            "query target_labels union with language rule"
        );
    }
    Ok(())
}

#[tokio::test]
async fn oracle_target_rules_preserve_qualified_quotes_source_association_and_cancellation()
-> TestResult {
    use graphit_ast::semantic_graph::oracle::{
        apply_oracle_graph_resolution, resolve_oracle_graph_reference,
    };
    let temp = TempDir::new()?;
    let profile = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    let (target, _) = oracle_target_fixture(
        &temp,
        "q.lance",
        "q.sql",
        "CREATE TABLE \"ACME\".\"Álvo\"(id NUMBER);",
        profile,
    )
    .await?;
    let (rows,batch)=oracle_target_fixture(&temp,"qc.lance","qc.sql",
        "CREATE PROCEDURE caller AS v NUMBER; BEGIN SELECT id INTO v FROM \"ACME\".\"Álvo\"; END; /",profile).await?;
    assert_eq!(target.endpoints().len(), 1);
    assert!(!rows.references().is_empty());
    let original = Arc::new(context());
    for reference in rows.references() {
        let resolution = resolve_oracle_graph_reference(reference, target.endpoints(), &original)?;
        assert_eq!(resolution.edges().len(), 1);

        for (field, value) in [
            ("path", serde_json::json!({"String":"foreign.sql"})),
            ("raw_version", serde_json::json!({"String":"99999999"})),
            (
                "source_sha256",
                serde_json::json!({"String":"foreign-hash"}),
            ),
            (
                "physical_branch",
                serde_json::json!({"String":"foreign-branch"}),
            ),
            (
                "relative_components_json",
                serde_json::json!({"String":"[\"foreign.sql\"]"}),
            ),
        ] {
            let mut wire = serde_json::to_value(batch.file_observation())?;
            wire["properties"][field] = value;
            let mut file: graphit_ast::semantic_graph::AstGraphNode = serde_json::from_value(wire)?;
            let prior = file.clone();
            assert!(apply_oracle_graph_resolution(&mut file, &resolution, &original).is_err());
            assert_eq!(file, prior);
        }
    }
    original.cancellation().cancel();
    assert!(
        matches!(resolve_oracle_graph_reference(&rows.references()[0],target.endpoints(),&original),
        Err(AstGraphError::Operation(e)) if e.kind()==std::io::ErrorKind::Interrupted)
    );
    Ok(())
}

#[tokio::test]
async fn oracle_target_rules_keep_unavailable_native_target_and_nonexact_reference_explicit()
-> TestResult {
    use graphit_ast::prepared_input::SyntheticReason;
    use graphit_ast::semantic_graph::oracle::{
        collect_oracle_graph_rows, resolve_oracle_graph_reference,
    };
    let temp = TempDir::new()?;
    let original = Arc::new(context());
    let profile = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    let draft = catalog(profile);
    let broken = "CREATE TABLE target_tab(id NUMBER, );";
    let snapshot = raw(
        &temp,
        "unavailable.lance",
        "unavailable.sql",
        Some(broken),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("Source")
    };
    let mut unavailable = None;
    with_model(text, &original, &draft, &["unavailable.sql"], |document| {
        assert!(
            !document
                .parsed()
                .observations()
                .syntax
                .diagnostics
                .is_empty()
        );
        let batch = assemble_graph_antlr(
            AstGraphAntlrInput {
                document,
                source: NativeNnSource::Indexed(&snapshot),
            },
            &original,
        )?;
        let rows = collect_oracle_graph_rows(document, &batch, &original)?;
        assert_eq!(
            rows.endpoints().len(),
            1,
            "real recovered declaration retains its exact native name"
        );
        unavailable = Some(rows);
        Ok(())
    })?;
    let source =
        "CREATE PROCEDURE caller AS v NUMBER; BEGIN SELECT id INTO v FROM target_tab; END; /";
    let (rows, _) = oracle_target_fixture(
        &temp,
        "unavailable-call.lance",
        "unavailable-call.sql",
        source,
        profile,
    )
    .await?;
    for reference in rows.references() {
        let result = resolve_oracle_graph_reference(
            reference,
            unavailable.as_ref().unwrap().endpoints(),
            &original,
        )?;
        assert!(result.edges().is_empty());
        assert!(matches!(result.outcome().target(),
            AstGraphRelationTarget::GenericUnresolved{reason} if reason=="UnavailableTarget"));
    }
    let snapshot = raw(
        &temp,
        "nonexact.lance",
        "nonexact.sql",
        Some(source),
        &original,
    )
    .await?;
    let IndexedSource::Available(text) = snapshot.source(&original)? else {
        panic!("Source")
    };
    let start = text.find("target_tab").unwrap() as u64;
    let end = start + "target_tab".len() as u64;
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
            prepared: ByteSpan { start, end },
            reason: SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: end,
                end: text.len() as u64,
            },
            original: ByteSpan {
                start: end,
                end: text.len() as u64,
            },
        },
    ];
    let removed = [ByteSpan { start, end }];
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
    let parsed =
        ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["nonexact.sql"], limits())?;
    let document = parsed.assemble()?;
    let batch = assemble_graph_antlr(
        AstGraphAntlrInput {
            document: &document,
            source: NativeNnSource::Indexed(&snapshot),
        },
        &original,
    )?;
    let rows = collect_oracle_graph_rows(&document, &batch, &original)?;
    assert!(!rows.references().is_empty());
    for reference in rows.references() {
        let resolution = resolve_oracle_graph_reference(reference, &[], &original)?;
        assert!(resolution.edges().is_empty());
        assert!(
            matches!(resolution.outcome().target(),AstGraphRelationTarget::GenericUnresolved{reason}
            if reason=="NonExactOrigin")
        );
    }
    let expired = Arc::new(SnapshotContext::new(
        std::time::Instant::now(),
        Cancellation::default(),
    ));
    assert!(
        matches!(resolve_oracle_graph_reference(&rows.references()[0],&[],&expired),
        Err(AstGraphError::Operation(e)) if e.kind()==std::io::ErrorKind::TimedOut)
    );
    Ok(())
}

#[tokio::test]
async fn oracle_target_rules_do_not_capture_a_homonymous_cte_as_global_table() -> TestResult {
    use graphit_ast::semantic_graph::oracle::resolve_oracle_graph_reference;
    let temp = TempDir::new()?;
    let profile = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    let (target, _) = oracle_target_fixture(
        &temp,
        "cte-target.lance",
        "cte-target.sql",
        "CREATE TABLE target_tab(id NUMBER);",
        profile,
    )
    .await?;
    let source = "CREATE PROCEDURE caller AS v NUMBER; BEGIN WITH target_tab AS (SELECT 1 AS id FROM dual) SELECT id INTO v FROM target_tab; END; /";
    let (rows, _) =
        oracle_target_fixture(&temp, "cte-caller.lance", "cte-caller.sql", source, profile).await?;
    assert_eq!(target.endpoints().len(), 1);
    assert!(!rows.references().is_empty());
    let original = Arc::new(context());
    for reference in rows.references() {
        let result = resolve_oracle_graph_reference(reference, target.endpoints(), &original)?;
        assert!(
            result.edges().is_empty(),
            "native WITH binding is a scope barrier"
        );
        assert!(matches!(result.outcome().target(),
            AstGraphRelationTarget::GenericUnresolved{reason} if reason=="UnsupportedScope"));
    }
    Ok(())
}
