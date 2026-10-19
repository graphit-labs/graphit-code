use arrow_array::{RecordBatch, StringArray};
use graphit_ast::{
    BytePoint, SourceText,
    antlr_engine::{generic_model::ParsedGenericAntlrDocument, model::ParsedAntlrDocument},
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    indexed_source::{read_branch_source, source_schema},
    native_nn::NativeNnSource,
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    scip_syntax::{AstScipSyntaxError, AstScipSyntaxInput, produce_scip_syntax_document},
    semantic_graph::{
        AstGraphAntlrInput, AstGraphGenericInput, AstGraphJsonInput, AstGraphParts,
        AstGraphTsInput,
        antlr_generic::{AstGraphAntlrGenericInput, assemble_graph_antlr_generic},
        assemble_graph_antlr, assemble_graph_generic, assemble_graph_json, assemble_graph_ts,
    },
    semantic_model::ParsedJsonDocument,
    source_inspection::IndexedSource,
    symbols::{IdentityLimits, IdentityLimitsInput},
    tree_sitter_context::PreparedTreeSitterContexts,
    tree_sitter_engine::builtin,
    tree_sitter_generic_model::PreparedGenericTreeSitterModel,
    tree_sitter_model::ParsedTreeSitterDocument,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_data::{lance_branch_store::LanceBranchStore, lance_module_store::LanceModuleStore};
use protobuf::{Enum, Message};
use scip::{symbol, types};
use std::{collections::BTreeSet, error::Error, sync::Arc};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;
enum Adapter {
    Typed,
    Generic,
    Json,
    Antlr,
    AntlrGeneric,
}

fn original() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::service(Cancellation::default()))
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
fn catalog(bytes: &[u8], original: &SnapshotContext) -> CatalogDraft {
    let id = ProfileId::new("scip-syntax-fixture", 256).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    let profiles = [decoder
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
            original,
        )
        .unwrap()];
    CatalogDraft::build(
        &[ProfileLayer {
            kind: ProfileLayerKind::Runtime,
            profiles: &profiles,
        }],
        &CatalogLimits::new(CatalogLimitsInput {
            policy_entries: 4096,
            selector_bytes: 32768,
        })
        .unwrap(),
        original,
    )
    .unwrap()
}

/// Use the existing Raw reader, native parser/model and graph assemblers, not fabricated rows.
async fn graph(
    profile: &[u8],
    path: &str,
    body: &str,
    adapter: Adapter,
    original: &Arc<SnapshotContext>,
) -> Result<AstGraphParts, Box<dyn Error>> {
    let temp = TempDir::new()?;
    let uri = temp.path().join("source.lance");
    let batch = RecordBatch::try_new(
        source_schema(),
        vec![
            Arc::new(StringArray::from(vec![path])),
            Arc::new(StringArray::from(vec![body])),
        ],
    )?;
    LanceModuleStore::create(uri.to_str().unwrap(), source_schema(), vec![batch]).await?;
    let target = LanceBranchStore::open(uri.to_str().unwrap(), source_schema(), None)
        .await?
        .capture_target(Some("fixture"))
        .await?
        .materialize()
        .await?;
    let raw = read_branch_source(&target, path, original).await?;
    let IndexedSource::Available(text) = raw.source(original)? else {
        panic!("genuine stored Source");
    };
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
    let draft = catalog(profile, original);
    let profile = &draft.profiles()[0];
    let components = path.split('/').collect::<Vec<_>>();
    let graph = match adapter {
        Adapter::Typed => {
            let factory = PreparedTreeSitterContexts::prepare(
                profile,
                builtin(profile.grammar().value())?,
                original,
            )?;
            let parsed =
                ParsedTreeSitterDocument::collect(&factory, &input, &components, limits())?;
            let model = parsed.assemble()?;
            assemble_graph_ts(
                AstGraphTsInput {
                    document: &model,
                    source: NativeNnSource::Indexed(&raw),
                },
                original,
            )?
        }
        Adapter::Generic => {
            let contexts = PreparedTreeSitterContexts::prepare(
                profile,
                builtin(profile.grammar().value())?,
                original,
            )?;
            let factory = PreparedGenericTreeSitterModel::prepare(profile, contexts, original)?;
            let parsed = factory.collect(&input, &components, limits())?;
            let model = parsed.assemble()?;
            assemble_graph_generic(
                AstGraphGenericInput {
                    document: &model,
                    source: NativeNnSource::Indexed(&raw),
                },
                original,
            )?
        }
        Adapter::Json => {
            let factory = PreparedTreeSitterContexts::prepare(
                profile,
                builtin(profile.grammar().value())?,
                original,
            )?;
            let parsed = ParsedJsonDocument::collect(&factory, &input, &components, limits())?;
            let model = parsed.assemble()?;
            assemble_graph_json(
                AstGraphJsonInput {
                    document: &model,
                    source: NativeNnSource::Indexed(&raw),
                },
                original,
            )?
        }
        Adapter::AntlrGeneric => {
            let model =
                ParsedGenericAntlrDocument::collect(profile, &input, &components, limits())?;
            assemble_graph_antlr_generic(
                AstGraphAntlrGenericInput {
                    document: &model,
                    source: NativeNnSource::Indexed(&raw),
                },
                original,
            )?
        }
        Adapter::Antlr => {
            let parsed = ParsedAntlrDocument::collect(profile, &input, &components, limits())?;
            let model = parsed.assemble()?;
            assemble_graph_antlr(
                AstGraphAntlrInput {
                    document: &model,
                    source: NativeNnSource::Indexed(&raw),
                },
                original,
            )?
        }
    };
    Ok(graph.into_parts())
}

fn input<'a>(graph: &'a AstGraphParts, source: &'a str) -> AstScipSyntaxInput<'a> {
    AstScipSyntaxInput {
        graph,
        source,
        project_id: "logical-project",
        project_version: "branch/versão com `espaço`",
    }
}
fn fingerprint(graph: &AstGraphParts) -> Vec<u8> {
    serde_json::to_vec(&(
        &graph.file_observation,
        &graph.association,
        &graph.nodes,
        &graph.edges,
        &graph.outcomes,
        &graph.diagnostics_json,
        &graph.capabilities_json,
    ))
    .unwrap()
}
fn check_document(
    graph: &AstGraphParts,
    text: &str,
    original: &Arc<SnapshotContext>,
) -> Result<types::Document, Box<dyn Error>> {
    let before = fingerprint(graph);
    let document = produce_scip_syntax_document(input(graph, text), original)?;
    assert_eq!(
        before,
        fingerprint(graph),
        "all rich graph observations preserved"
    );
    assert_eq!(document.relative_path, graph.association.path());
    assert_eq!(
        document.position_encoding.enum_value().unwrap(),
        types::PositionEncoding::UTF8CodeUnitOffsetFromLineStart
    );
    assert_eq!(document.symbols.len(), graph.nodes.len());
    assert_eq!(
        document
            .symbols
            .iter()
            .map(|s| &s.symbol)
            .collect::<BTreeSet<_>>()
            .len(),
        graph.nodes.len()
    );
    assert!(
        document
            .occurrences
            .iter()
            .all(|o| o.diagnostics.is_empty())
    );
    let view = SourceText::new(text.as_bytes(), text.len().max(1))?;
    for info in &document.symbols {
        symbol::parse_symbol(&info.symbol).unwrap();
        assert!(
            !info
                .symbol
                .contains(std::env::temp_dir().to_string_lossy().as_ref())
        );
    }
    for occurrence in &document.occurrences {
        let range = &occurrence.range;
        assert!(range.len() == 3 || range.len() == 4);
        let start = BytePoint {
            row: range[0].try_into()?,
            column: range[1].try_into()?,
        };
        let end = if range.len() == 3 {
            BytePoint {
                row: start.row,
                column: range[2].try_into()?,
            }
        } else {
            BytePoint {
                row: range[2].try_into()?,
                column: range[3].try_into()?,
            }
        };
        view.range(view.byte_at(start)?, view.byte_at(end)?)?;
    }
    let encoded = document.write_to_bytes()?;
    assert_eq!(
        types::Document::parse_from_bytes(&encoded)?,
        document,
        "real official protobuf document roundtrip"
    );
    assert_eq!(
        produce_scip_syntax_document(input(graph, text), original)?,
        document
    );
    Ok(document)
}

#[tokio::test]
async fn typed_javascript_preserves_local_references_docs_signatures_and_equal_names() -> TestResult
{
    let original = original();
    let text = "/** café documentation */\nfunction café(x) { return x; }\nfunction caller() { return café(1); }\nfunction a() { function same() {} return same(); }\nfunction b() { function same() {} return same(); }\n";
    let parts = graph(
        include_bytes!("../profiles/queries/javascript.yaml"),
        "src/café.js",
        text,
        Adapter::Typed,
        &original,
    )
    .await?;
    let document = check_document(&parts, text, &original)?;
    let cafe = document
        .symbols
        .iter()
        .find(|s| s.display_name == "café")
        .unwrap();
    assert_eq!(
        cafe.kind.enum_value().unwrap(),
        types::symbol_information::Kind::Function
    );
    assert!(
        cafe.documentation
            .iter()
            .any(|d| d.contains("café documentation"))
    );
    assert!(
        cafe.signature_documentation
            .as_ref()
            .is_some_and(|s| s.text.contains("café"))
    );
    assert_eq!(
        document
            .symbols
            .iter()
            .filter(|s| s.display_name == "same")
            .count(),
        2
    );
    assert!(
        document
            .occurrences
            .iter()
            .any(|o| o.symbol == cafe.symbol && o.symbol_roles == 0)
    );
    assert!(document.occurrences.iter().any(|o| o.symbol == cafe.symbol
        && o.symbol_roles & types::SymbolRole::Definition.value() != 0
        && !o.enclosing_range.is_empty()));
    let mut other = input(&parts, text);
    other.project_version = "another-logical-version";
    let changed = produce_scip_syntax_document(other, &original)?;
    assert!(
        changed
            .symbols
            .iter()
            .all(|s| !document.symbols.iter().any(|old| old.symbol == s.symbol))
    );
    Ok(())
}

#[tokio::test]
async fn typed_python_preserves_unicode_docstring_signature_and_local_reference() -> TestResult {
    let original = original();
    let text = "def café(x):\n    \"\"\"Résumé documentation.\"\"\"\n    return x\n\ndef caller():\n    return café(1)\n";
    let parts = graph(
        include_bytes!("../profiles/queries/python.yaml"),
        "café.py",
        text,
        Adapter::Typed,
        &original,
    )
    .await?;
    let document = check_document(&parts, text, &original)?;
    let cafe = document
        .symbols
        .iter()
        .find(|s| s.display_name == "café")
        .unwrap();
    assert!(
        cafe.documentation
            .iter()
            .any(|d| d.contains("Résumé documentation"))
    );
    assert!(
        cafe.signature_documentation
            .as_ref()
            .is_some_and(|s| s.text.contains("café"))
    );
    assert!(
        document
            .occurrences
            .iter()
            .any(|o| o.symbol == cafe.symbol && o.symbol_roles == 0)
    );
    Ok(())
}

#[tokio::test]
async fn rust_typed_and_custom_json_generic_keep_all_entities_without_language_whitelist()
-> TestResult {
    let original = original();
    let rust = "struct Café { n: i32 }\nfn helper() {}\nfn caller() { helper(); }\n";
    let parts = graph(
        include_bytes!("../profiles/queries/rust.yaml"),
        "model.rs",
        rust,
        Adapter::Typed,
        &original,
    )
    .await?;
    assert!(!parts.nodes.is_empty());
    let document = check_document(&parts, rust, &original)?;
    assert!(
        document
            .symbols
            .iter()
            .any(|s| s.kind.enum_value().unwrap() == types::symbol_information::Kind::Struct)
    );
    let custom = br#"language: custom-json-syntax
grammar: tree-sitter-json
context_types: {pair: CustomNode}
context_name_queries: {pair: '(pair key: (_) @name) @context'}
queries:
  - graph_label: CustomNode
    data_key: custom_entities
    name_is_data: true
    pattern: '(pair key: (string) @name) @unit'
    span_capture: unit
"#;
    let text = "{\"é😀\":{\"same\":1},\"other\":{\"same\":1}}";
    let parts = graph(custom, "model.custom", text, Adapter::Generic, &original).await?;
    assert_eq!(parts.nodes.len(), 4);
    let document = check_document(&parts, text, &original)?;
    assert_eq!(document.language, "custom-json-syntax");
    assert!(
        document
            .symbols
            .iter()
            .all(|s| s.kind.enum_value().unwrap()
                == types::symbol_information::Kind::UnspecifiedKind)
    );
    Ok(())
}

#[tokio::test]
async fn json_borrowed_projection_keeps_values_fields_namespaces_and_duplicate_names() -> TestResult
{
    let original = original();
    let text = "{\"outer\":{\"same\":1},\"other\":{\"same\":1},\"é😀\":true}";
    let parts = graph(
        include_bytes!("fixtures/catalog/shipped/json.yaml"),
        "model.json",
        text,
        Adapter::Json,
        &original,
    )
    .await?;
    let document = check_document(&parts, text, &original)?;
    assert!(parts.nodes.iter().any(|node| {
        node.properties().get("name").is_some_and(|v| {
            v == &graphit_ast::semantic_graph::AstGraphValue::String("same".into())
        })
    }));
    assert_eq!(
        document
            .symbols
            .iter()
            .filter(|s| s.display_name == "same")
            .count(),
        2
    );
    assert!(
        document.symbols.iter().all(|s| s.documentation.is_empty()),
        "values and custom metadata are not documentation"
    );
    Ok(())
}

#[tokio::test]
async fn unavailable_generic_extent_preserves_original_definition_without_enclosing_range()
-> TestResult {
    let original = original();
    let profile = br#"language: custom-json-syntax
grammar: tree-sitter-json
queries:
  - graph_label: CustomNode
    data_key: custom_entities
    name_is_data: true
    pattern: '(pair key: (string) @name)'
"#;
    let text = "{\"café\":42,\"other\":true}";
    let parts = graph(profile, "model.custom", text, Adapter::Generic, &original).await?;
    assert_eq!(parts.nodes.len(), 2);
    assert!(parts.nodes.iter().all(|node| {
        node.properties().get("extent_status")
            == Some(&graphit_ast::semantic_graph::AstGraphValue::String(
                "Unavailable".into(),
            ))
    }));
    let document = check_document(&parts, text, &original)?;
    assert_eq!(document.symbols.len(), 2);
    assert!(document.occurrences.iter().all(|occurrence| {
        occurrence.symbol_roles == types::SymbolRole::Definition.value()
            && occurrence.enclosing_range.is_empty()
    }));
    Ok(())
}

#[tokio::test]
async fn antlr_sql_and_cobol_emit_real_documents_without_discarding_graph_observations()
-> TestResult {
    let original = original();
    for (profile, path, text, adapter) in [
        (
            include_bytes!("fixtures/catalog/shipped/plsql.yaml").as_slice(),
            "model.sql",
            "CREATE TABLE T(ID NUMBER); SELECT ID FROM T;",
            Adapter::Antlr,
        ),
        (
            include_bytes!("fixtures/catalog/shipped/db2.yaml").as_slice(),
            "model.db2",
            "CREATE TABLE T(C INTEGER); SELECT C FROM T;",
            Adapter::Antlr,
        ),
        (
            include_bytes!("fixtures/catalog/shipped/cobol85.yaml").as_slice(),
            "model.cbl",
            "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELLO.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 X PIC 9.\n       PROCEDURE DIVISION.\n       MAIN.\n           DISPLAY X\n           STOP RUN.\n",
            Adapter::AntlrGeneric,
        ),
    ] {
        let parts = graph(profile, path, text, adapter, &original).await?;
        assert!(
            !parts.nodes.is_empty(),
            "{path} has genuine native model entities"
        );
        check_document(&parts, text, &original)?;
    }
    Ok(())
}

#[tokio::test]
async fn empty_source_hash_unicode_ranges_and_cancellation_are_checked() -> TestResult {
    let original = original();
    let empty = graph(
        include_bytes!("../profiles/queries/javascript.yaml"),
        "empty.js",
        "",
        Adapter::Typed,
        &original,
    )
    .await?;
    let document = check_document(&empty, "", &original)?;
    assert!(document.symbols.is_empty() && document.occurrences.is_empty());
    assert!(matches!(
        produce_scip_syntax_document(input(&empty, "changed"), &original),
        Err(AstScipSyntaxError::SourceMismatch)
    ));
    let text = "function café() {}";
    let mut parts = graph(
        include_bytes!("../profiles/queries/javascript.yaml"),
        "unicode.js",
        text,
        Adapter::Typed,
        &original,
    )
    .await?;
    check_document(&parts, text, &original)?;
    let node = &parts.nodes[0];
    let mut invalid = serde_json::to_value(node)?;
    invalid["properties"]["name_end_byte"] = serde_json::json!({"Int64": 13});
    parts.nodes[0] = serde_json::from_value(invalid)?;
    assert!(
        produce_scip_syntax_document(input(&parts, text), &original).is_err(),
        "offset or advertised points cannot be normalized"
    );
    original.cancellation().cancel();
    assert!(
        matches!(produce_scip_syntax_document(input(&empty, ""), &original), Err(AstScipSyntaxError::Operation(e)) if e.kind() == std::io::ErrorKind::Interrupted)
    );
    Ok(())
}
