use graphit_ast::{
    antlr_engine::{
        AntlrEngineError, CaptureStep, Dialect, NodeClass, ProfileQuery, SpanStatus,
        cobol_copy::{CopyProgramScope, parse_cobol_semantic_occurrences},
        parse_profile_queries,
        preprocessing::{CobolFormat, prepare_cobol},
        semantic::{
            CaptureBinding, CapturedNode, SemanticDocument, SemanticError, SemanticQueryOrigin,
            parse_semantic_occurrences, postgresql::parse_postgresql_semantic_occurrences,
        },
    },
    catalog::{
        CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind,
        ProvenanceView,
    },
    prepared_input::{
        ByteSpan, OriginPiece, OriginPlan, OriginSegment, OriginalProjection, PreparedInput,
        SyntheticReason,
    },
    profiles::{
        ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput, QueryKind,
        QuerySource,
    },
    scalar_policy::{RegexLimits, ScalarPolicy},
    tree_sitter_engine::ParseControl,
    tree_sitter_profile::{CaptureBinding as BodyCaptureBinding, PreparedTreeSitterProfile},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    error::Error,
    io::ErrorKind,
    time::{Duration, Instant},
};

fn capture_steps(
    steps: &graphit_ast::catalog::EffectiveList<
        graphit_ast::catalog::Effective<graphit_ast::catalog::CaptureStep>,
    >,
) -> Vec<CaptureStep> {
    steps
        .iter()
        .map(|value| {
            let value = value.value();
            if !value.xpath().value().is_empty() {
                CaptureStep::XPath(value.xpath().value().clone())
            } else if !value.nearest_descendant().value().is_empty() {
                CaptureStep::NearestDescendant(value.nearest_descendant().value().clone())
            } else {
                CaptureStep::NearestAncestor(value.nearest_ancestor().value().clone())
            }
        })
        .collect()
}
fn configured_query(query: &graphit_ast::catalog::Query) -> ProfileQuery {
    ProfileQuery {
        pattern: query.pattern().value().clone(),
        select: query.select().value().clone(),
        keyword: query.keyword().value().clone(),
        child_rule: query.child_rule().value().clone(),
        negate: *query.negate().value(),
        captures: query
            .captures()
            .value()
            .iter()
            .map(|(_, steps)| capture_steps(steps.value()))
            .collect(),
    }
}
fn configured_context(value: &graphit_ast::catalog::AntlrContext) -> ProfileQuery {
    ProfileQuery {
        pattern: value.pattern().value().clone(),
        keyword: value.keyword().value().clone(),
        child_rule: value.child_rule().value().clone(),
        negate: *value.negate().value(),
        captures: vec![capture_steps(value.name().value())],
        ..ProfileQuery::default()
    }
}

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}
fn catalog(bytes: &[u8]) -> CatalogDraft {
    let operation = context();
    let id = ProfileId::new("semantic-fixture", 256).unwrap();
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
            &operation,
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
        &operation,
    )
    .unwrap()
}
fn profile(grammar: &str, body: &str) -> CatalogDraft {
    catalog(format!("language: fixture\nparser: antlr4\ngrammar: {grammar}\n{body}").as_bytes())
}
fn with_identity(
    source: &str,
    operation: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>),
) {
    let bound = ByteSpan {
        start: 0,
        end: u64::try_from(source.len()).unwrap(),
    };
    let segments = if source.is_empty() {
        vec![]
    } else {
        vec![OriginSegment::Direct {
            prepared: bound,
            original: bound,
        }]
    };
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        operation,
    )
    .unwrap();
    action(&input);
}
fn present<'a, 'o, 'v>(binding: &'a CaptureBinding<'o, 'v>) -> &'a CapturedNode<'o, 'v> {
    match binding {
        CaptureBinding::Present(node) => node,
        _ => panic!("expected present capture"),
    }
}
fn text<'v>(binding: &CaptureBinding<'_, 'v>) -> &'v str {
    present(binding).prepared.as_ref().unwrap().text()
}
fn no_diagnostics(document: &SemanticDocument<'_, '_, '_>) {
    assert!(
        document.syntax.diagnostics.is_empty(),
        "{:?}",
        document.syntax.diagnostics
    );
}

#[test]
fn child_rule_context_guards_keep_disjoint_native_declarations() {
    let catalog = profile(
        "antlr-plsql",
        r#"
queries:
- data_key: types
  graph_label: Type
  pattern: //type_declaration/identifier
  type: entity
antlr_contexts:
- pattern: //type_declaration
  child_rule: record_type_def
  label: Type
  name:
  - xpath: /identifier
- pattern: //type_declaration
  child_rule: record_type_def
  negate: true
  label: Type
  name:
  - xpath: /identifier
"#,
    );
    let profile = &catalog.profiles()[0];
    let source =
        "DECLARE TYPE r_t IS RECORD(id NUMBER); TYPE c_t IS REF CURSOR; BEGIN NULL; END; /";
    with_identity(source, &context(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        no_diagnostics(&document);
        assert_eq!(document.occurrences.len(), 2);
        assert_eq!(document.contexts.len(), 2);
        for (occurrence, (name, selector)) in document
            .occurrences
            .iter()
            .zip([("r_t", "type_declaration"), ("c_t", "type_declaration")])
        {
            assert_eq!(text(&occurrence.name), name);
            assert!(std::ptr::eq(
                occurrence.query,
                profile.queries().value().as_slice()[0].value()
            ));
            assert_eq!(occurrence.contexts.len(), 1);
            let scope = &document.contexts[occurrence.contexts[0]];
            assert_eq!(scope.selector, selector);
            assert_eq!(scope.label, Some("Type"));
            assert_eq!(text(&scope.name), name);
            let native = &document.syntax.nodes[scope.matched.node_index];
            assert_eq!(native.class, NodeClass::Rule);
            assert_eq!(native.kind, "type_declaration");
            assert_eq!(
                document.syntax.nodes[occurrence.matched.node_index].parent,
                Some(scope.matched.node_index)
            );
        }
    });
}

#[test]
fn oracle_table_columns_retain_raw_names_query_provenance_and_context() {
    let catalog = profile(
        "antlr-plsql",
        r#"
queries:
- data_key: tables
  graph_label: Table
  pattern: //create_table/table_name
  type: entity
- data_key: columns
  graph_label: Column
  pattern: //column_definition/column_name
  type: entity
antlr_contexts:
- pattern: //create_table
  label: Table
  name:
  - xpath: /table_name
"#,
    );
    let source =
        r#"CREATE TABLE "ACME"."PEDIDO" ("ID_PEDIDO" NUMBER,"DT_PEDIDO" DATE,"VL_TOTAL" NUMBER);"#;
    with_identity(source, &context(), |input| {
        let profile = &catalog.profiles()[0];
        let result = parse_semantic_occurrences(profile, input).unwrap();
        no_diagnostics(&result);
        assert_eq!(result.occurrences.len(), 4);
        assert!(std::ptr::eq(result.profile, profile));
        assert_eq!(result.contexts.len(), 1);
        assert_eq!(text(&result.contexts[0].name), r#""PEDIDO""#);
        assert_eq!(result.contexts[0].label, Some("Table"));
        let names: Vec<_> = result.occurrences.iter().map(|o| text(&o.name)).collect();
        assert_eq!(
            names,
            [
                r#""PEDIDO""#,
                r#""ID_PEDIDO""#,
                r#""DT_PEDIDO""#,
                r#""VL_TOTAL""#
            ]
        );
        for occurrence in &result.occurrences {
            assert_eq!(*occurrence.query.r#type().value(), QueryKind::Entity);
            assert!(std::ptr::eq(
                occurrence.query,
                profile.queries().value().as_slice()[occurrence.query_index].value()
            ));
            assert!(matches!(
                occurrence.query.graph_label().provenance(),
                ProvenanceView::Declared(_)
            ));
            assert_eq!(occurrence.contexts, [0]);
            assert!(matches!(occurrence.span, CaptureBinding::NotConfigured));
            assert!(matches!(
                occurrence.matched.original,
                Some(OriginalProjection::Exact(_))
            ));
            assert!(matches!(
                occurrence.qualifier,
                CaptureBinding::NotConfigured
            ));
        }
        assert_eq!(result.occurrences[1].query.data_key().value(), "columns");
    });
}

#[test]
fn postgres_slots_preserve_missing_values_duplicates_and_parent_capture() {
    let catalog = profile(
        "antlr-postgresql",
        r#"
queries:
- data_key: columns
  graph_label: Column
  pattern: //columnDef
  value_label: Text
  parent_label: Scope
  type: entity
  captures:
    name:
    - xpath: /colid
    value:
    - xpath: /colquallist
    - nearest_descendant: colconstraintelem
    - xpath: /b_expr
    parent:
    - xpath: /colquallist
- data_key: other
  graph_label: Field
  pattern: //columnDef
  type: entity
  captures:
    name:
    - xpath: /colid
"#,
    );
    with_identity(
        "CREATE TABLE pedido(status TEXT DEFAULT 'ABERTO',id INT NOT NULL,vl INT CHECK(vl>0));",
        &context(),
        |input| {
            let result = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
            no_diagnostics(&result);
            assert_eq!(result.occurrences.len(), 6);
            let columns: Vec<_> = result
                .occurrences
                .iter()
                .filter(|o| o.query_index == 0)
                .collect();
            assert_eq!(text(&columns[0].value), "'ABERTO'");
            assert!(matches!(columns[1].value, CaptureBinding::Missing));
            assert!(matches!(columns[2].value, CaptureBinding::Missing));
            assert!(
                columns
                    .iter()
                    .all(|o| matches!(o.parent, CaptureBinding::Present(_)))
            );
            for index in 0..3 {
                assert_eq!(
                    result.occurrences[index].matched.node_index,
                    result.occurrences[index + 3].matched.node_index
                );
                assert_ne!(
                    result.occurrences[index].query.graph_label().value(),
                    result.occurrences[index + 3].query.graph_label().value()
                );
            }
        },
    );
}

#[test]
fn tsql_terminal_relation_capture_keeps_all_metadata() {
    let catalog = profile(
        "antlr-tsql",
        r#"
queries:
- data_key: refs
  pattern: //declare_statement
  relation_type: READS_FIELD
  target_labels:
  - Variable
  target_fallback: Field
  name_reject: NEVER
  name_is_data: true
  value_label: Payload
  parent_label: Scope
  type: relation
  captures:
    name:
    - xpath: /LOCAL_ID
    value:
    - xpath: /LOCAL_ID
    parent:
    - xpath: /LOCAL_ID
"#,
    );
    with_identity("DECLARE @x INT;", &context(), |input| {
        let result = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
        no_diagnostics(&result);
        assert_eq!(result.occurrences.len(), 1);
        let occurrence = &result.occurrences[0];
        assert_eq!(text(&occurrence.name), "@x");
        assert_eq!(
            result.syntax.nodes[present(&occurrence.name).node_index].class,
            NodeClass::Terminal
        );
        let query = occurrence.query;
        assert_eq!(*query.r#type().value(), QueryKind::Relation);
        assert_eq!(query.relation_type().value(), "READS_FIELD");
        assert_eq!(
            query.target_labels().value().as_slice()[0].value(),
            "Variable"
        );
        assert_eq!(query.target_fallback().value(), "Field");
        assert_eq!(query.name_reject().value(), "NEVER");
        assert!(*query.name_is_data().value());
        assert_eq!(query.value_label().value(), "Payload");
        assert_eq!(query.parent_label().value(), "Scope");
    });
}

#[test]
fn qualifier_uses_actual_parent_rule_and_missing_keeps_occurrence() {
    let catalog = profile(
        "antlr-plsql",
        r#"
queries:
- data_key: writes
  relation_type: WRITES_COLUMN
  pattern: //column_based_update_set_clause/column_name
  type: relation
  captures:
    qualifier:
    - nearest_ancestor: update_statement
    - xpath: /general_table_ref
- data_key: columns
  graph_label: Column
  pattern: //column_definition/column_name
  type: entity
  captures:
    qualifier:
    - nearest_ancestor: update_statement
    - xpath: /general_table_ref
"#,
    );
    with_identity(
        r#"UPDATE "ACME"."PEDIDO" SET "ST_PROC"=1; CREATE TABLE other(id NUMBER);"#,
        &context(),
        |input| {
            let result = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
            no_diagnostics(&result);
            assert_eq!(result.occurrences.len(), 2);
            assert_eq!(text(&result.occurrences[0].qualifier), r#""ACME"."PEDIDO""#);
            assert!(matches!(
                result.occurrences[1].qualifier,
                CaptureBinding::Missing
            ));
        },
    );
}

#[test]
fn context_name_is_literal_and_equal_text_nodes_keep_distinct_identity() {
    let catalog = profile(
        "antlr-postgresql",
        r#"
queries:
- data_key: script
  graph_label: File
  pattern: //root
  type: entity
- data_key: statements
  graph_label: Statement
  pattern: //stmt
  type: entity
antlr_contexts:
- pattern: //root
  label: File
  name:
  - xpath: /name
- pattern: //stmt
  label: Statement
"#,
    );
    with_identity("SELECT 1; SELECT 1;", &context(), |input| {
        let result = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
        no_diagnostics(&result);
        assert_eq!(result.contexts.len(), 3);
        assert!(matches!(result.contexts[0].name, CaptureBinding::Missing));
        assert_eq!(text(&result.occurrences[0].name), "SELECT 1; SELECT 1;");
        assert!(result.occurrences[0].contexts.is_empty());
        let contexts: Vec<_> = result
            .contexts
            .iter()
            .filter(|c| c.selector == "stmt")
            .collect();
        assert_eq!(text(&contexts[0].name), text(&contexts[1].name));
        assert_ne!(
            contexts[0].matched.node_index,
            contexts[1].matched.node_index
        );
        assert!(!contexts[0].name_is_explicit);
    });
}

#[test]
fn compound_cobol_and_synthetic_origins_remain_explicit() {
    let catalog = profile(
        "antlr-cobol85",
        "queries:\n- data_key: values\n  graph_label: Value\n  pattern: //literal\n  type: entity\n",
    );
    let source = "000100 IDENTIFICATION DIVISION.\n000200 PROGRAM-ID. HELLO.\n000300 PROCEDURE DIVISION.\n000400     DISPLAY 'HEL\n000500-    'LO'.\n000600     STOP RUN.\n";
    let operation = context();
    let prepared = prepare_cobol(source, CobolFormat::Fixed, &operation).unwrap();
    prepared.with_origin_plan(|plan| {
        let input = PreparedInput::new(source.as_bytes(), prepared.text().as_bytes(), plan, &operation).unwrap();
        let result = parse_semantic_occurrences(&catalog.profiles()[0], &input).unwrap();
        no_diagnostics(&result);
        let literal = result.occurrences.iter().find(|o| text(&o.name) == "'HELLO'").unwrap();
        assert!(matches!(&present(&literal.name).original, Some(OriginalProjection::Compound(pieces)) if pieces.len() >= 2 && pieces.iter().any(|p| matches!(p, OriginPiece::Original(_)))));
    });
    let catalog = profile(
        "antlr-postgresql",
        "queries:\n- data_key: script\n  graph_label: File\n  pattern: //root\n  type: entity\n",
    );
    let prepared = "SELECT 1;";
    let segments = [OriginSegment::Synthetic {
        prepared: ByteSpan { start: 0, end: 9 },
        reason: SyntheticReason::Wrapper,
    }];
    let input = PreparedInput::new(
        b"",
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &operation,
    )
    .unwrap();
    let result = parse_semantic_occurrences(&catalog.profiles()[0], &input).unwrap();
    assert!(matches!(
        result.occurrences[0].matched.original,
        Some(OriginalProjection::Synthetic(SyntheticReason::Wrapper))
    ));
}

fn plpgsql_fixture() -> graphit_ast::tree_sitter_engine::GrammarLanguage {
    graphit_ast::tree_sitter_engine::builtin("tree-sitter-plpgsql").unwrap()
}

fn assert_shipped_observations(
    language: &str,
    profile: &graphit_ast::catalog::EffectiveProfile,
    body_profile: &PreparedTreeSitterProfile<'_>,
    input: &PreparedInput<'_, '_, '_, '_>,
) {
    let main = match language {
        "cobol85" => {
            let result = parse_cobol_semantic_occurrences(profile, input).unwrap();
            let sidecar = &result.preprocessor;
            assert_eq!(sidecar.grammar(), "Cobol85Preprocessor.g4");
            assert!(sidecar.syntax.diagnostics.is_empty());
            assert!(
                !sidecar.occurrences.is_empty(),
                "actual stock COPY observation"
            );
            for item in &sidecar.occurrences {
                let occurrence = &item.occurrence;
                assert_eq!(
                    *occurrence.query.source().value(),
                    QuerySource::CobolPreprocessor
                );
                assert!(std::ptr::eq(
                    occurrence.query,
                    profile.queries().value().as_slice()[occurrence.query_index].value()
                ));
                assert_eq!(text(&occurrence.name), "BOOK");
                assert_eq!(
                    sidecar.syntax.nodes[occurrence.matched.node_index].kind,
                    "copySource"
                );
                assert!(
                    matches!(&present(&occurrence.name).original, Some(OriginalProjection::Exact(span)) if span.text() == "BOOK")
                );
                let CopyProgramScope::Linked {
                    main_anchor_node_index,
                    program_node_index,
                    context_indices,
                } = &item.scope
                else {
                    panic!("COPY scope: {:?}", item.scope);
                };
                assert_eq!(
                    result.main.syntax.nodes[*main_anchor_node_index].kind,
                    "COPY"
                );
                assert_eq!(
                    result.main.syntax.nodes[*program_node_index].kind,
                    "programUnit"
                );
                let mut actual_contexts = Vec::new();
                let mut parent = result.main.syntax.nodes[*main_anchor_node_index].parent;
                let mut found_program = false;
                while let Some(index) = parent {
                    found_program |= index == *program_node_index;
                    if let Some(context) = result
                        .main
                        .contexts
                        .iter()
                        .position(|context| context.matched.node_index == index)
                    {
                        actual_contexts.push(context);
                    }
                    parent = result.main.syntax.nodes[index].parent;
                }
                assert!(found_program);
                assert_eq!(*context_indices, actual_contexts);
                assert!(
                    context_indices
                        .iter()
                        .any(|index| result.main.contexts[*index].matched.node_index
                            == *program_node_index)
                );
            }
            result.main
        }
        "postgresql" => {
            let result = parse_postgresql_semantic_occurrences(
                profile,
                body_profile,
                input,
                ParseControl::default(),
            )
            .unwrap();
            let mut labels = std::collections::BTreeSet::new();
            assert!(!result.bodies.is_empty(), "actual PL/pgSQL bodies");
            for body in &result.bodies {
                assert!(!body.host_contexts.is_empty());
                let mut parent = Some(body.observations.body.host_parent);
                let mut actual_contexts = Vec::new();
                while let Some(index) = parent {
                    if let Some(context) = result
                        .host
                        .contexts
                        .iter()
                        .position(|context| context.matched.node_index == index)
                    {
                        actual_contexts.push(context);
                    }
                    parent = result.host.syntax.nodes[index].parent;
                }
                assert_eq!(body.host_contexts, actual_contexts);
                for index in &body.host_contexts {
                    let context = &result.host.contexts[*index];
                    if let Some(label) = context.label {
                        labels.insert(label);
                    }
                    assert_eq!(text(&context.name), "same");
                }
                assert!(!body.matches.is_empty(), "stock body variable capture");
                for address in &body.matches {
                    let group = &body.observations.groups[address.query_index];
                    assert!(std::ptr::eq(
                        group.query,
                        &body.observations.profile.queries().value().as_slice()
                            [address.query_index]
                    ));
                    assert_eq!(group.query.value().graph_label().value(), "Variable");
                    let found = &group.matches[address.match_index];
                    let BodyCaptureBinding::Present(slots) = &found.bindings.name else {
                        panic!("stock body name capture");
                    };
                    for slot in slots {
                        let capture = &found.captures[*slot];
                        assert_eq!(capture.prepared.text(), "v");
                        assert!(
                            matches!(&capture.original, OriginalProjection::Exact(span) if span.text() == "v")
                        );
                        assert_eq!(
                            body.observations.nodes[address.capture_nodes[*slot]].node_id,
                            capture.native.node_id
                        );
                    }
                }
                assert!(
                    !body.decisions.is_empty(),
                    "actual configured IF observation"
                );
                for decision in &body.decisions {
                    assert_eq!(body.observations.nodes[decision.node_index].kind, "stmt_if");
                    assert!(
                        matches!(&decision.original, OriginalProjection::Exact(span) if span.text() == decision.prepared.text())
                    );
                }
            }
            assert!(labels.contains("Function"));
            assert!(labels.contains("Procedure"));
            result.host
        }
        _ => parse_semantic_occurrences(profile, input)
            .unwrap_or_else(|error| panic!("{language}: {error}")),
    };
    no_diagnostics(&main);
    assert!(!main.occurrences.is_empty(), "{language}");
    for occurrence in &main.occurrences {
        assert!(std::ptr::eq(
            occurrence.query,
            profile.queries().value().as_slice()[occurrence.query_index].value()
        ));
        assert_eq!(*occurrence.query.source().value(), QuerySource::Primary);
        assert!(occurrence.matched.node_index < main.syntax.nodes.len());
        assert!(
            occurrence
                .contexts
                .iter()
                .all(|index| *index < main.contexts.len())
        );
    }
    if language == "tsql" {
        let revokes: Vec<_> = main
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.query.data_key().value() == "revokes")
            .collect();
        assert_eq!(
            revokes.len(),
            1,
            "the actual REVOKE target, excluding CREATE/SELECT"
        );
        let occurrence = revokes[0];
        assert_eq!(*occurrence.query.r#type().value(), QueryKind::Relation);
        assert_eq!(occurrence.query.graph_label().value(), "");
        assert_eq!(occurrence.query.relation_type().value(), "REFERENCES");
        assert_eq!(occurrence.query.name_capture().value(), "");
        assert_eq!(
            occurrence.query.pattern().value(),
            "//revoke_permission//table_name"
        );
        let name = present(&occurrence.name);
        assert_eq!(name.node_index, occurrence.matched.node_index);
        assert_eq!(text(&occurrence.name), "dbo.pedido");
        let node = &main.syntax.nodes[name.node_index];
        assert_eq!(node.class, NodeClass::Rule);
        assert_eq!(node.kind, "table_name");
        assert_eq!(
            main.syntax.nodes[node.parent.unwrap()].kind,
            "revoke_permission"
        );
        assert!(
            matches!(&name.original, Some(OriginalProjection::Exact(span)) if span.text() == "dbo.pedido")
        );
    }
    let capability = if language == "cobol85" {
        "Program"
    } else {
        "Table"
    };
    assert!(
        main.occurrences
            .iter()
            .any(
                |occurrence| occurrence.query.graph_label().value() == capability
                    && matches!(occurrence.name, CaptureBinding::Present(_))
            ),
        "{language}: {capability} capability"
    );
}

const SHIPPED: [(&str, &[u8], &str); 5] = [
    (
        "plsql",
        include_bytes!("fixtures/catalog/shipped/plsql.yaml"),
        "CREATE TABLE pedido(id NUMBER); SELECT id FROM pedido;",
    ),
    (
        "postgresql",
        include_bytes!("fixtures/catalog/shipped/postgresql.yaml"),
        "CREATE TABLE pedido(id INT); SELECT id FROM pedido; CREATE FUNCTION same() RETURNS void LANGUAGE plpgsql AS $$DECLARE v text; BEGIN IF true THEN NULL; END IF; END;$$; CREATE PROCEDURE same() LANGUAGE plpgsql AS $MiXeD$DECLARE v text; BEGIN IF true THEN NULL; END IF; END;$MiXeD$;",
    ),
    (
        "tsql",
        include_bytes!("fixtures/catalog/shipped/tsql.yaml"),
        "CREATE TABLE pedido(id INT); SELECT id FROM pedido; REVOKE SELECT ON OBJECT::dbo.pedido FROM leitor;",
    ),
    (
        "db2",
        include_bytes!("fixtures/catalog/shipped/db2.yaml"),
        "CREATE TABLE pedido LIKE modelo COMPRESS NO",
    ),
    (
        "cobol85",
        include_bytes!("fixtures/catalog/shipped/cobol85.yaml"),
        "       IDENTIFICATION DIVISION.\n       PROGRAM-ID. HELLO.\n       DATA DIVISION.\n       WORKING-STORAGE SECTION.\n       01 FIELD-A PIC 9.\n           COPY BOOK.\n       PROCEDURE DIVISION.\n           DISPLAY FIELD-A.\n           STOP RUN.\n",
    ),
];

#[test]
fn plsql_type_body_profile_follows_sdk_declaration_branches() {
    let yaml = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/plsql.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/plsql.yaml")
    );
    let catalog = catalog(yaml);
    let profile = &catalog.profiles()[0];
    let queries = profile.queries().value().as_slice();
    let type_queries = [&queries[8], &queries[9]];
    assert_eq!(
        type_queries[0].value().pattern().value(),
        "//create_type/type_definition/type_name"
    );
    assert_eq!(
        type_queries[1].value().pattern().value(),
        "//create_type/type_body/type_name"
    );
    let native_queries: Vec<_> = type_queries
        .iter()
        .map(|query| {
            let query = query.value();
            assert_eq!(*query.r#type().value(), QueryKind::Entity);
            assert_eq!(query.data_key().value(), "types");
            assert_eq!(query.graph_label().value(), "Type");
            assert_eq!(query.name_capture().value(), "");
            ProfileQuery {
                pattern: query.pattern().value().clone(),
                captures: vec![Vec::new()],
                ..ProfileQuery::default()
            }
        })
        .collect();
    let contexts: Vec<_> = profile
        .antlr_contexts()
        .value()
        .iter()
        .map(|value| value.value())
        .filter(|value| {
            matches!(
                value.pattern().value().as_str(),
                "//create_type" | "//create_type_body" | "//type_body"
            )
        })
        .map(|value| {
            (
                value.pattern().value().trim_start_matches('/'),
                value.label().value().as_str(),
            )
        })
        .collect();
    assert_eq!(contexts, [("create_type", "Type")]);
    assert_eq!(
        profile
            .declaration_types()
            .value()
            .iter()
            .filter(|rule| rule.value().as_str() == "create_type")
            .count(),
        1
    );
    assert!(
        !profile
            .declaration_types()
            .value()
            .iter()
            .any(|rule| matches!(rule.value().as_str(), "create_type_body" | "type_body"))
    );
    for (source, expected) in [
        (
            "CREATE OR REPLACE TYPE BODY pedido_t AS MEMBER PROCEDURE executar IS BEGIN NULL; END; END; /",
            Some(1),
        ),
        ("CREATE TYPE pedido_t AS OBJECT (id NUMBER);", Some(0)),
        ("CREATE TABLE pedido(id NUMBER);", None),
    ] {
        let parsed =
            parse_profile_queries(Dialect::PlSql, source, &native_queries, &context()).unwrap();
        assert!(
            parsed.syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.syntax.diagnostics
        );
        assert_eq!(parsed.matches.len(), usize::from(expected.is_some()));
        if let Some(query_index) = expected {
            let matched = &parsed.matches[0];
            assert_eq!(matched.query_index, query_index);
            assert_eq!(matched.captures, [Some(matched.node_index)]);
            let name = &parsed.syntax.nodes[matched.node_index];
            assert_eq!(name.class, NodeClass::Rule);
            assert_eq!(name.kind, "type_name");
            assert_eq!(&source[name.span.clone().unwrap()], "pedido_t");
            let branch = &parsed.syntax.nodes[name.parent.unwrap()];
            assert_eq!(
                branch.kind,
                if query_index == 0 {
                    "type_definition"
                } else {
                    "type_body"
                }
            );
            let declaration = &parsed.syntax.nodes[branch.parent.unwrap()];
            assert_eq!(declaration.class, NodeClass::Rule);
            assert_eq!(declaration.kind, "create_type");
        }
    }
}

#[test]
fn postgresql_function_procedure_profiles_use_native_context_guards() {
    let yaml = include_bytes!("fixtures/catalog/shipped/postgresql.yaml");
    assert_eq!(
        yaml,
        include_bytes!("fixtures/profiles/shipped/postgresql.yaml")
    );
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/postgresql.yaml")
    );
    let mut fixture: serde_json::Value = serde_saphyr::from_str_with_options(
        std::str::from_utf8(yaml).unwrap(),
        serde_saphyr::options! {
            duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
            merge_keys: serde_saphyr::MergeKeyPolicy::Error,
            strict_booleans: true,
        },
    )
    .unwrap();
    fixture["queries"] =
        serde_json::json!([fixture["queries"][2].clone(), fixture["queries"][3].clone()]);
    fixture["antlr_contexts"]
        .as_array_mut()
        .unwrap()
        .retain(|value| value["pattern"] == "//createfunctionstmt");
    let bounded = catalog(&serde_json::to_vec(&fixture).unwrap());
    let profile = &bounded.profiles()[0];
    for (source, expected) in [
        (
            "CREATE FUNCTION f() RETURNS integer AS 'SELECT 1' LANGUAGE sql;",
            vec![("Function", "f")],
        ),
        (
            "CREATE PROCEDURE p() AS 'SELECT 1' LANGUAGE sql;",
            vec![("Procedure", "p")],
        ),
        (
            "CREATE FUNCTION f() RETURNS integer AS 'SELECT 1' LANGUAGE sql; CREATE PROCEDURE p() AS 'SELECT 1' LANGUAGE sql;",
            vec![("Function", "f"), ("Procedure", "p")],
        ),
        (
            "create function f() returns integer as 'PROCEDURE' language sql;",
            vec![("Function", "f")],
        ),
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            no_diagnostics(&document);
            let actual: Vec<_> = document
                .occurrences
                .iter()
                .map(|occurrence| {
                    assert!(std::ptr::eq(
                        occurrence.query,
                        profile.queries().value().as_slice()[occurrence.query_index].value()
                    ));
                    assert_eq!(
                        document.syntax.nodes[occurrence.matched.node_index].class,
                        NodeClass::Rule
                    );
                    assert_eq!(occurrence.contexts.len(), 1);
                    let scope = &document.contexts[occurrence.contexts[0]];
                    let label = occurrence.query.graph_label().value().as_str();
                    assert_eq!(scope.label, Some(label));
                    assert_eq!(text(&scope.name), text(&occurrence.name));
                    assert_eq!(
                        document.syntax.nodes[scope.matched.node_index].kind,
                        "createfunctionstmt"
                    );
                    assert_eq!(scope.selector, "createfunctionstmt");
                    (label, text(&occurrence.name))
                })
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(document.contexts.len(), expected.len());
        });
    }
}

#[test]
fn context_selectors_preserve_sdk_errors_and_reject_overlapping_native_nodes() {
    for selector in ["unknownRule", "root["] {
        let catalog = profile(
            "antlr-postgresql",
            &format!("antlr_contexts: [{{pattern: '//{selector}', label: File}}]\n"),
        );
        with_identity("SELECT 1;", &context(), |input| {
            let error = parse_semantic_occurrences(&catalog.profiles()[0], input)
                .err()
                .unwrap();
            assert!(matches!(&error, SemanticError::Engine {
                query_origin: Some(SemanticQueryOrigin::Context { selector: actual }),
                cause: AntlrEngineError::QueryFailure { .. },
            } if actual == selector));
            assert!(error.source().is_some());
        });
    }
    let catalog = profile(
        "antlr-postgresql",
        "antlr_contexts:\n- pattern: //root\n  label: File\n- pattern: //root\n  keyword: SELECT\n  negate: true\n  label: Alternate\n",
    );
    with_identity("SELECT 1;", &context(), |input| {
        let error = parse_semantic_occurrences(&catalog.profiles()[0], input)
            .err()
            .unwrap();
        assert!(
            matches!(error, SemanticError::OverlappingContextSelectors { first_selector, second_selector, .. }
            if first_selector == "root" && second_selector == "root")
        );
    });
    let catalog = profile(
        "antlr-postgresql",
        "antlr_contexts:\n- pattern: //root\n  keyword: NOT_A_TOKEN\n  label: File\n",
    );
    with_identity("SELECT 1;", &context(), |input| {
        let document = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
        no_diagnostics(&document);
        assert!(document.contexts.is_empty());
    });
}

#[test]
fn postgresql_type_profile_captures_native_declaration_variants() {
    let yaml = include_bytes!("fixtures/catalog/shipped/postgresql.yaml");
    assert_eq!(
        yaml,
        include_bytes!("fixtures/profiles/shipped/postgresql.yaml")
    );
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/postgresql.yaml")
    );
    let mut fixture: serde_json::Value = serde_saphyr::from_str_with_options(
        std::str::from_utf8(yaml).unwrap(),
        serde_saphyr::options! {
            duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
            merge_keys: serde_saphyr::MergeKeyPolicy::Error,
            strict_booleans: true,
        },
    )
    .unwrap();
    let types: Vec<_> = fixture["queries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|query| {
            query["pattern"] == "//definestmt"
                && query["keyword"] == "TYPE"
                && query["select"] == "/any_name"
        })
        .cloned()
        .collect();
    assert_eq!(types.len(), 1);
    fixture["queries"] = serde_json::Value::Array(types);
    let bounded = catalog(&serde_json::to_vec(&fixture).unwrap());
    let profile = &bounded.profiles()[0];
    for (source, expected) in [
        ("CREATE TYPE pedido_t AS(id integer);", vec!["pedido_t"]),
        (
            "CREATE TYPE estado_t AS ENUM('aberto','fechado');",
            vec!["estado_t"],
        ),
        (
            "CREATE TYPE faixa_t AS RANGE(subtype=integer);",
            vec!["faixa_t"],
        ),
        ("CREATE TYPE shell_t;", vec!["shell_t"]),
        (
            "CREATE TYPE base_t(INPUT=int4in, OUTPUT=int4out);",
            vec!["base_t"],
        ),
        ("create type min_t as enum('a');", vec!["min_t"]),
        (
            "CREATE TYPE pedido_t AS(id integer); CREATE TYPE estado_t AS ENUM('aberto','fechado'); CREATE TYPE faixa_t AS RANGE(subtype=integer);",
            vec!["pedido_t", "estado_t", "faixa_t"],
        ),
        (
            "CREATE OPERATOR + (FUNCTION=int4pl,LEFTARG=integer,RIGHTARG=integer);",
            vec![],
        ),
        ("CREATE COLLATION locale_t (locale='en_US');", vec![]),
        ("CREATE DOMAIN dominio_t AS integer;", vec![]),
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            no_diagnostics(&document);
            let names: Vec<_> = document
                .occurrences
                .iter()
                .map(|occurrence| {
                    assert!(std::ptr::eq(
                        occurrence.query,
                        profile.queries().value().as_slice()[0].value()
                    ));
                    let query = occurrence.query;
                    assert_eq!(*query.r#type().value(), QueryKind::Entity);
                    assert_eq!(query.data_key().value(), "types");
                    assert_eq!(query.graph_label().value(), "Type");
                    assert_eq!(query.name_capture().value(), "");
                    let node = &document.syntax.nodes[occurrence.matched.node_index];
                    assert_eq!(node.class, NodeClass::Rule);
                    assert_eq!(node.kind, "any_name");
                    assert_eq!(&source[node.span.clone().unwrap()], text(&occurrence.name));
                    match present(&occurrence.name).original.as_ref().unwrap() {
                        OriginalProjection::Exact(range) => {
                            assert_eq!(range.text(), text(&occurrence.name))
                        }
                        _ => panic!("expected exact original type name"),
                    }
                    text(&occurrence.name)
                })
                .collect();
            assert_eq!(names, expected);
            let scopes: Vec<_> = document
                .contexts
                .iter()
                .filter(|scope| scope.selector == "definestmt")
                .collect();
            let scope_names: Vec<_> = scopes
                .iter()
                .map(|scope| {
                    assert_eq!(scope.label, Some("Type"));
                    assert!(scope.name_is_explicit);
                    let index = scope.matched.node_index;
                    assert_eq!(document.syntax.nodes[index].class, NodeClass::Rule);
                    assert_eq!(document.syntax.nodes[index].kind, "definestmt");
                    assert_eq!(
                        document.syntax.nodes[present(&scope.name).node_index].parent,
                        Some(index)
                    );
                    text(&scope.name)
                })
                .collect();
            assert_eq!(scope_names, expected);
        });
    }
}

#[test]
fn plsql_declarations_preserve_native_types_and_tablespaces() {
    let yaml = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/plsql.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/plsql.yaml")
    );
    let mut fixture: serde_json::Value = serde_saphyr::from_str_with_options(
        std::str::from_utf8(yaml).unwrap(),
        serde_saphyr::options! {
            duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
            merge_keys: serde_saphyr::MergeKeyPolicy::Error,
            strict_booleans: true,
        },
    )
    .unwrap();
    let declarations: Vec<_> = fixture["queries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|query| {
            query["data_key"] == "tablespaces"
                || query["pattern"] == "//type_declaration/identifier"
        })
        .cloned()
        .collect();
    assert_eq!(declarations.len(), 2);
    fixture["queries"] = serde_json::Value::Array(declarations);
    let bounded = catalog(&serde_json::to_vec(&fixture).unwrap());
    let profile = &bounded.profiles()[0];
    for (source, expected, local_types) in [
        ("CREATE TABLESPACE dados;", vec!["dados"], false),
        (
            "CREATE TEMPORARY TABLESPACE temporario;",
            vec!["temporario"],
            false,
        ),
        (
            "CREATE UNDO TABLESPACE undo_dados;",
            vec!["undo_dados"],
            false,
        ),
        ("CREATE TABLE pedido(id NUMBER);", vec![], false),
        ("ALTER TABLESPACE dados READ ONLY;", vec![], false),
        (
            "DECLARE TYPE r_t IS RECORD(id NUMBER); TYPE t_t IS TABLE OF NUMBER; TYPE v_t IS VARRAY(10) OF NUMBER; TYPE c_t IS REF CURSOR; comum NUMBER; BEGIN NULL; END; /",
            vec!["r_t", "t_t", "v_t", "c_t"],
            true,
        ),
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            no_diagnostics(&document);
            let actual: Vec<_> = document
                .occurrences
                .iter()
                .map(|occurrence| {
                    let query = occurrence.query;
                    assert!(std::ptr::eq(
                        query,
                        profile.queries().value().as_slice()[occurrence.query_index].value()
                    ));
                    assert_eq!(*query.r#type().value(), QueryKind::Entity);
                    assert_eq!(
                        query.data_key().value(),
                        if local_types { "types" } else { "tablespaces" }
                    );
                    assert_eq!(
                        query.graph_label().value(),
                        if local_types { "Type" } else { "Tablespace" }
                    );
                    let name = present(&occurrence.name);
                    let node = &document.syntax.nodes[name.node_index];
                    assert_eq!(node.class, NodeClass::Rule);
                    assert_eq!(
                        node.kind,
                        if local_types {
                            "identifier"
                        } else {
                            "id_expression"
                        }
                    );
                    assert_eq!(&source[node.span.clone().unwrap()], text(&occurrence.name));
                    match name.original.as_ref().unwrap() {
                        OriginalProjection::Exact(range) => {
                            assert_eq!(range.text(), text(&occurrence.name))
                        }
                        _ => panic!("expected exact original declaration name"),
                    }
                    text(&occurrence.name)
                })
                .collect();
            assert_eq!(actual, expected);
            let scopes: Vec<_> = document
                .contexts
                .iter()
                .filter(|scope| scope.selector == "type_declaration")
                .collect();
            assert_eq!(scopes.len(), if local_types { 4 } else { 0 });
            if local_types {
                let mut ordinals = std::collections::BTreeSet::new();
                let names: Vec<_> = scopes
                    .iter()
                    .map(|scope| {
                        assert_eq!(scope.label, Some("Type"));
                        assert!(scope.name_is_explicit);
                        let index = scope.matched.node_index;
                        assert!(ordinals.insert(index));
                        assert_eq!(document.syntax.nodes[index].kind, "type_declaration");
                        assert_eq!(document.syntax.nodes[index].class, NodeClass::Rule);
                        assert_eq!(
                            document.syntax.nodes[present(&scope.name).node_index].parent,
                            Some(index)
                        );
                        text(&scope.name)
                    })
                    .collect();
                assert_eq!(names, expected);
            }
        });
    }
}

#[test]
fn tsql_grant_profile_captures_native_permission_targets() {
    let yaml = include_bytes!("fixtures/catalog/shipped/tsql.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/tsql.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/tsql.yaml")
    );
    let mut fixture: serde_json::Value = serde_saphyr::from_str_with_options(
        std::str::from_utf8(yaml).unwrap(),
        serde_saphyr::options! {
            duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
            merge_keys: serde_saphyr::MergeKeyPolicy::Error,
            strict_booleans: true,
        },
    )
    .unwrap();
    let grants: Vec<_> = fixture["queries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|query| query["data_key"] == "grants")
        .cloned()
        .collect();
    assert_eq!(grants.len(), 1);
    fixture["queries"] = serde_json::Value::Array(grants);
    let bounded = catalog(&serde_json::to_vec(&fixture).unwrap());
    let profile = &bounded.profiles()[0];
    for (source, expected) in [
        (
            "GRANT SELECT ON OBJECT::dbo.pedido TO leitor WITH GRANT OPTION;",
            vec!["dbo.pedido"],
        ),
        ("GRANT CONNECT TO leitor;", vec![]),
        ("REVERT;", vec![]),
        (
            "GRANT SELECT ON OBJECT::dbo.pedido TO leitor; GRANT CONNECT TO leitor; REVERT; SELECT 'GRANT SELECT ON dbo.falso';",
            vec!["dbo.pedido"],
        ),
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            no_diagnostics(&document);
            let actual: Vec<_> = document
                .occurrences
                .iter()
                .map(|occurrence| {
                    let query = occurrence.query;
                    assert!(std::ptr::eq(
                        query,
                        profile.queries().value().as_slice()[0].value()
                    ));
                    assert_eq!(*query.r#type().value(), QueryKind::Relation);
                    assert_eq!(query.data_key().value(), "grants");
                    assert_eq!(query.graph_label().value(), "");
                    assert_eq!(query.relation_type().value(), "REFERENCES");
                    let node = &document.syntax.nodes[occurrence.matched.node_index];
                    assert_eq!(node.class, NodeClass::Rule);
                    assert_eq!(node.kind, "table_name");
                    assert_eq!(&source[node.span.clone().unwrap()], text(&occurrence.name));
                    assert_eq!(
                        text(&occurrence.name),
                        match present(&occurrence.name).original.as_ref().unwrap() {
                            OriginalProjection::Exact(range) => range.text(),
                            _ => panic!("expected exact original target"),
                        }
                    );
                    text(&occurrence.name)
                })
                .collect();
            assert_eq!(actual, expected);
        });
    }
}

#[test]
fn all_shipped_antlr_profiles_compile_and_produce_occurrences() {
    for (language, yaml, source) in SHIPPED {
        let catalog = catalog(yaml);
        let profile = &catalog.profiles()[0];
        let dialect = match language {
            "plsql" => Dialect::PlSql,
            "postgresql" => Dialect::PostgreSql,
            "tsql" => Dialect::TSql,
            "db2" => Dialect::Db2,
            "cobol85" => Dialect::Cobol85,
            _ => unreachable!(),
        };
        for source_tree in [QuerySource::Primary, QuerySource::CobolPreprocessor] {
            let mut queries: Vec<_> = profile
                .queries()
                .value()
                .iter()
                .filter(|query| *query.value().source().value() == source_tree)
                .map(|query| configured_query(query.value()))
                .collect();
            if source_tree == QuerySource::Primary {
                queries.extend(
                    profile
                        .antlr_contexts()
                        .value()
                        .iter()
                        .map(|value| configured_context(value.value())),
                );
            }
            if queries.is_empty() {
                continue;
            }
            let operation = context();
            let prepared = graphit_ast::antlr_engine::PreparedProfileQueries::new(
                dialect,
                source_tree == QuerySource::CobolPreprocessor,
                &queries,
                &operation,
            )
            .unwrap();
            prepared.parse(source, &operation).unwrap();
        }
    }
    let body_catalog = catalog(include_bytes!("fixtures/catalog/shipped/plpgsql.yaml"));
    let language = plpgsql_fixture();
    let factory =
        PreparedTreeSitterProfile::prepare(&body_catalog.profiles()[0], language, &context())
            .unwrap();
    for (language, yaml, source) in SHIPPED {
        let catalog = catalog(yaml);
        with_identity(source, &context(), |input| {
            assert_shipped_observations(language, &catalog.profiles()[0], &factory, input);
        });
    }
}

#[test]
fn errors_keep_profile_or_context_origin_and_native_cause() {
    for (body, expected) in [
        (
            "queries:\n- data_key: a\n  graph_label: File\n  pattern: //root\n  type: entity\n- data_key: b\n  graph_label: File\n  pattern: //unknownRule\n  type: entity\n",
            SemanticQueryOrigin::Profile { query_index: 1 },
        ),
        (
            "queries:\n- data_key: a\n  graph_label: File\n  pattern: //root\n  type: entity\n  captures:\n    name:\n    - xpath: /name/\n",
            SemanticQueryOrigin::Profile { query_index: 0 },
        ),
        (
            "antlr_contexts:\n- pattern: //root\n  name:\n  - xpath: /unknownRule\n",
            SemanticQueryOrigin::Context {
                selector: "root".into(),
            },
        ),
    ] {
        let catalog = profile("antlr-postgresql", body);
        with_identity("SELECT 1;", &context(), |input| {
            let error = parse_semantic_occurrences(&catalog.profiles()[0], input)
                .err()
                .unwrap();
            assert!(error.source().is_some());
            assert!(
                matches!(error, SemanticError::Engine { query_origin: Some(ref origin), cause: AntlrEngineError::QueryFailure { .. } } if *origin == expected)
            );
        });
    }
    for (yaml, kind) in [
        ("language: fixture\nparser: tree-sitter\ngrammar: sql\n", 0),
        (
            "language: fixture\nparser: antlr4\ngrammar: antlr-unknown\n",
            1,
        ),
        (
            "language: fixture\nparser: antlr4\ngrammar: antlr-postgresql\nstart_rule: ''\n",
            2,
        ),
        (
            "language: fixture\nparser: antlr4\ngrammar: antlr-postgresql\nantlr_contexts:\n- pattern: //root[\n  label: File\n",
            3,
        ),
        (
            "language: fixture\nparser: antlr4\ngrammar: antlr-postgresql\nantlr_contexts:\n- pattern: //SELECT\n  label: Token\n",
            4,
        ),
    ] {
        let catalog = catalog(yaml.as_bytes());
        with_identity("SELECT 1;", &context(), |input| {
            let error = parse_semantic_occurrences(&catalog.profiles()[0], input)
                .err()
                .unwrap();
            assert!(matches!(
                (kind, error),
                (0, SemanticError::UnsupportedParser(_))
                    | (1, SemanticError::UnsupportedGrammar(_))
                    | (2, SemanticError::UnsupportedStartRule { .. })
                    | (
                        3,
                        SemanticError::Engine {
                            query_origin: Some(SemanticQueryOrigin::Context { .. }),
                            cause: AntlrEngineError::QueryFailure { .. }
                        }
                    )
                    | (4, SemanticError::ContextMustBeRule { .. })
            ));
        });
    }
}

#[test]
fn original_context_borrow_controls_preflight_deadline_and_mid_operation() {
    let catalog = profile(
        "antlr-postgresql",
        "queries:\n- data_key: script\n  graph_label: File\n  pattern: //root\n  type: entity\n",
    );
    let operation = context();
    with_identity("SELECT 1;", &operation, |input| {
        assert!(std::ptr::eq(input.original_context(), &operation));
        operation.cancellation().cancel();
        assert!(
            matches!(parse_semantic_occurrences(&catalog.profiles()[0], input), Err(SemanticError::Operation(e)) if e.kind() == ErrorKind::Interrupted)
        );
    });
    let operation = SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    );
    with_identity("SELECT 1;", &operation, |input| {
        std::thread::sleep(Duration::from_millis(25));
        assert!(
            matches!(parse_semantic_occurrences(&catalog.profiles()[0], input), Err(SemanticError::Operation(e)) if e.kind() == ErrorKind::TimedOut)
        );
    });
    let operation = context();
    let source = "SELECT 1;\n".repeat(100_000);
    with_identity(&source, &operation, |input| {
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(5));
                operation.cancellation().cancel();
            });
            let error = parse_semantic_occurrences(&catalog.profiles()[0], input)
                .err()
                .unwrap();
            assert!(
                matches!(error, SemanticError::Operation(e) | SemanticError::Engine { cause: AntlrEngineError::Operation(e), .. } if e.kind() == ErrorKind::Interrupted)
            );
        });
    });
}

#[test]
fn unicode_bom_crlf_eof_recovery_and_empty_profiles_keep_sdk_classification() {
    let catalog = profile(
        "antlr-tsql",
        "queries:\n- data_key: all\n  graph_label: File\n  pattern: //tsql_file\n  type: entity\n- data_key: end\n  graph_label: End\n  pattern: //EOF\n  type: entity\n",
    );
    let source = "\u{feff}SELECT '🙂α' AS value;\r\n";
    with_identity(source, &context(), |input| {
        let result = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
        no_diagnostics(&result);
        assert_eq!(text(&result.occurrences[0].name), &source[3..]);
        assert!(std::ptr::eq(
            text(&result.occurrences[0].name).as_ptr(),
            source[3..].as_ptr()
        ));
        let eof = &result.occurrences[1];
        assert_eq!(text(&eof.name), "");
        assert_eq!(
            result.syntax.nodes[eof.matched.node_index].status,
            SpanStatus::Eof
        );
        assert_eq!(
            result.syntax.nodes[eof.matched.node_index].class,
            NodeClass::Terminal
        );
    });
    with_identity("SELECT FROM;", &context(), |input| {
        let result = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
        assert!(!result.syntax.diagnostics.is_empty());
        assert!(
            result
                .syntax
                .nodes
                .iter()
                .any(|node| node.error || node.missing)
        );
    });
    let empty = profile(
        "antlr-postgresql",
        "antlr_contexts:\n- pattern: //root\n  label: File\n",
    );
    with_identity("", &context(), |input| {
        let result = parse_semantic_occurrences(&empty.profiles()[0], input).unwrap();
        assert!(result.occurrences.is_empty());
        assert_eq!(result.contexts.len(), 1);
    });
}

#[test]
fn sdk_ancestor_anchor_excludes_self_validates_rules_and_does_not_backtrack() {
    let source = "SELECT (SELECT (SELECT 1)) LIMIT 2;";
    let query = ProfileQuery {
        pattern: "//select_no_parens".into(),
        captures: vec![
            vec![CaptureStep::NearestAncestor("select_no_parens".into())],
            vec![
                CaptureStep::NearestAncestor("select_no_parens".into()),
                CaptureStep::XPath("/select_limit".into()),
            ],
        ],
        ..ProfileQuery::default()
    };
    let result = parse_profile_queries(Dialect::PostgreSql, source, &[query], &context()).unwrap();
    assert_eq!(result.matches.len(), 3);
    assert_eq!(result.matches[0].captures, [None, None]);
    assert_eq!(
        result.matches[1].captures[0],
        Some(result.matches[0].node_index)
    );
    assert!(result.matches[1].captures[1].is_some());
    assert_eq!(
        result.matches[2].captures[0],
        Some(result.matches[1].node_index)
    );
    assert_eq!(result.matches[2].captures[1], None);
    for path in [
        "SELECT",
        "unknownRule",
        "**/select_no_parens",
        "select_no_parens/",
    ] {
        let query = ProfileQuery {
            pattern: "//root".into(),
            captures: vec![vec![CaptureStep::NearestAncestor(path.into())]],
            ..ProfileQuery::default()
        };
        assert!(matches!(
            parse_profile_queries(Dialect::PostgreSql, source, &[query], &context()),
            Err(AntlrEngineError::QueryFailure { query_index: 0, .. })
        ));
    }
}

#[test]
fn inserted_recovery_capture_is_present_without_fabricated_range() {
    let catalog = profile(
        "antlr-cobol85",
        "queries:\n- data_key: recovery\n  graph_label: Recovery\n  pattern: //DIVISION\n  type: entity\n",
    );
    with_identity(
        "IDENTIFICATION . PROGRAM-ID. HELLO. PROCEDURE DIVISION. STOP RUN.",
        &context(),
        |input| {
            let result = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
            let inserted = result
                .occurrences
                .iter()
                .find(|o| result.syntax.nodes[o.matched.node_index].missing)
                .unwrap();
            let node = &result.syntax.nodes[inserted.matched.node_index];
            assert_eq!(node.class, NodeClass::Error);
            assert!(node.error);
            assert!(matches!(node.status, SpanStatus::Unindexed(_)));
            assert!(inserted.matched.prepared.is_none());
            assert!(inserted.matched.original.is_none());
            assert_eq!(
                present(&inserted.name).node_index,
                inserted.matched.node_index
            );
            assert!(present(&inserted.name).prepared.is_none());
        },
    );
    let catalog = profile(
        "antlr-postgresql",
        "queries:\n- data_key: script\n  graph_label: File\n  pattern: //root\n  type: entity\n  captures:\n    name:\n    - xpath: /colid\n    span:\n    - xpath: /root\n",
    );
    with_identity("SELECT 1;", &context(), |input| {
        let result = parse_semantic_occurrences(&catalog.profiles()[0], input).unwrap();
        assert_eq!(result.occurrences.len(), 1);
        assert!(matches!(
            result.occurrences[0].name,
            CaptureBinding::Missing
        ));
        assert!(matches!(
            result.occurrences[0].span,
            CaptureBinding::Missing
        ));
    });
}

#[test]
fn five_semantic_consumers_run_concurrently_with_independent_sdk_vocabulary() {
    let body_catalog = catalog(include_bytes!("fixtures/catalog/shipped/plpgsql.yaml"));
    let language = plpgsql_fixture();
    let factory =
        PreparedTreeSitterProfile::prepare(&body_catalog.profiles()[0], language, &context())
            .unwrap();
    std::thread::scope(|scope| {
        for (language, yaml, source) in SHIPPED {
            let factory = &factory;
            scope.spawn(move || {
                let catalog = catalog(yaml);
                with_identity(source, &context(), |input| {
                    assert_shipped_observations(language, &catalog.profiles()[0], factory, input);
                });
            });
        }
    });
}

#[test]
fn plsql_calls_native_raw() {
    let yaml = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/plsql.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/plsql.yaml")
    );
    let mut fixture: serde_json::Value = serde_saphyr::from_str_with_options(
        std::str::from_utf8(yaml).unwrap(),
        serde_saphyr::options! {
            duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
            merge_keys: serde_saphyr::MergeKeyPolicy::Error,
            strict_booleans: true,
        },
    )
    .unwrap();
    assert!(!fixture["queries"].as_array().unwrap().iter().any(|query| {
        query["pattern"] == "//procedure_call/routine_name"
            || query["pattern"] == "//function_call/routine_name"
    }));
    let calls: Vec<_> = fixture["queries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|query| {
            query["pattern"] == "//call_statement/routine_name"
                || (query["pattern"] == "//general_element_part"
                    && query["child_rule"] == "function_argument")
        })
        .cloned()
        .collect();
    assert_eq!(calls.len(), 2);
    fixture["queries"] = serde_json::Value::Array(calls.clone());
    let bounded = catalog(&serde_json::to_vec(&fixture).unwrap());
    let profile = &bounded.profiles()[0];
    let cases = [
        (
            "BEGIN p; p(); CALL pkg.p@remote(1); pkg.p().q(); END;",
            vec!["p", "p", "pkg.p@remote", "pkg.p", "q"],
            vec![],
        ),
        (
            "BEGIN resultado := pkg.fn(ajustar(1)); END;",
            vec![],
            vec!["fn", "ajustar"],
        ),
        (
            "BEGIN resultado := pkg.fn@remote(); END;",
            vec![],
            vec!["fn"],
        ),
        ("BEGIN resultado := fn()(); END;", vec![], vec!["fn"]),
        (
            "BEGIN \"Pkg\".\"F\"\"n\"(); α.β(); resultado := \"Pkg\".\"F\"\"n\"(); resultado := α.β(); END;",
            vec!["\"Pkg\".\"F\"\"n\"", "α.β"],
            vec!["\"F\"\"n\"", "β"],
        ),
        (
            "BEGIN resultado := valor; fn(arg=>valor); resultado := 'function_argument'; END;",
            vec!["fn"],
            vec![],
        ),
        ("BEGIN itens(1) := fn(2); END;", vec![], vec!["itens", "fn"]),
        (
            "BEGIN\r\n  resultado := α.β(ajustar(1));\r\nEND;",
            vec![],
            vec!["β", "ajustar"],
        ),
    ];
    for (source, statements, expressions) in cases {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            no_diagnostics(&document);
            assert!(
                !document
                    .syntax
                    .nodes
                    .iter()
                    .any(|node| node.error || node.missing)
            );
            for (query_index, expected) in [(0, statements), (1, expressions)] {
                let selected: Vec<_> = document
                    .occurrences
                    .iter()
                    .filter(|occurrence| occurrence.query_index == query_index)
                    .collect();
                let names: Vec<_> = selected
                    .iter()
                    .map(|occurrence| text(&occurrence.name))
                    .collect();
                assert_eq!(names, expected, "{source}");
                let ordinals: Vec<_> = selected
                    .iter()
                    .map(|occurrence| occurrence.matched.node_index)
                    .collect();
                assert!(ordinals.windows(2).all(|pair| pair[0] < pair[1]));
                for occurrence in selected {
                    let query = occurrence.query;
                    assert!(std::ptr::eq(
                        query,
                        profile.queries().value().as_slice()[query_index].value()
                    ));
                    assert_eq!(query.data_key().value(), "calls");
                    assert_eq!(query.graph_label().value(), "");
                    assert_eq!(*query.r#type().value(), QueryKind::Relation);
                    assert_eq!(query.relation_type().value(), "CALLS");
                    assert!(query.name_reject().value().contains("begin|declare"));
                    assert!(!query.name_reject().value().contains("^[^A-Za-z_]"));
                    assert!(matches!(occurrence.value, CaptureBinding::NotConfigured));
                    assert!(matches!(occurrence.span, CaptureBinding::NotConfigured));
                    assert!(matches!(occurrence.parent, CaptureBinding::NotConfigured));
                    let matched = &document.syntax.nodes[occurrence.matched.node_index];
                    assert_eq!(matched.class, NodeClass::Rule);
                    assert_eq!(
                        matched.kind,
                        if query_index == 0 {
                            "routine_name"
                        } else {
                            "general_element_part"
                        }
                    );
                    let name = present(&occurrence.name);
                    let name_node = &document.syntax.nodes[name.node_index];
                    assert_eq!(name_node.class, NodeClass::Rule);
                    assert_eq!(
                        name_node.kind,
                        if query_index == 0 {
                            "routine_name"
                        } else {
                            "id_expression"
                        }
                    );
                    if query_index == 0 {
                        assert_eq!(name.node_index, occurrence.matched.node_index);
                    } else {
                        assert_eq!(name_node.parent, Some(occurrence.matched.node_index));
                    }
                    let qualifier = present(&occurrence.qualifier);
                    let qualifier_kind = if query_index == 0 {
                        "call_statement"
                    } else {
                        "general_element"
                    };
                    assert_eq!(
                        document.syntax.nodes[qualifier.node_index].kind,
                        qualifier_kind
                    );
                    let mut parent = matched.parent;
                    while let Some(index) = parent {
                        if document.syntax.nodes[index].kind == qualifier_kind {
                            break;
                        }
                        parent = document.syntax.nodes[index].parent;
                    }
                    assert_eq!(parent, Some(qualifier.node_index));
                    for capture in [&occurrence.matched, name, qualifier] {
                        let node = &document.syntax.nodes[capture.node_index];
                        let raw = &source[node.span.clone().unwrap()];
                        assert_eq!(capture.prepared.as_ref().unwrap().text(), raw);
                        match capture.original.as_ref().unwrap() {
                            OriginalProjection::Exact(range) => assert_eq!(range.text(), raw),
                            _ => panic!("expected exact original call projection"),
                        }
                    }
                    if source.contains("fn()()") {
                        assert_eq!(
                            document
                                .syntax
                                .nodes
                                .iter()
                                .filter(|node| {
                                    node.parent == Some(occurrence.matched.node_index)
                                        && node.kind == "function_argument"
                                })
                                .count(),
                            2
                        );
                    }
                    if source.contains("pkg.fn@remote") {
                        assert_eq!(text(&occurrence.qualifier), "pkg.fn@remote()");
                    }
                    if source.contains("pkg.fn(ajustar") && text(&occurrence.name) == "fn" {
                        assert_eq!(text(&occurrence.qualifier), "pkg.fn(ajustar(1))");
                    }
                    if source.contains("itens(1)") && text(&occurrence.name) == "itens" {
                        assert_eq!(text(&occurrence.qualifier), "itens(1)");
                    }
                }
            }
        });
    }
    let mut custom = calls[1].clone();
    custom["name_reject"] = "^β$".into();
    fixture["queries"] = serde_json::json!([custom.clone(), custom]);
    let duplicated = catalog(&serde_json::to_vec(&fixture).unwrap());
    let custom_profile = &duplicated.profiles()[0];
    with_identity("BEGIN resultado := β(); END;", &context(), |input| {
        let document = parse_semantic_occurrences(custom_profile, input).unwrap();
        no_diagnostics(&document);
        assert_eq!(document.occurrences.len(), 2);
        assert_eq!(
            document.occurrences[0].matched.node_index,
            document.occurrences[1].matched.node_index
        );
        for (index, occurrence) in document.occurrences.iter().enumerate() {
            assert_eq!(occurrence.query_index, index);
            assert_eq!(text(&occurrence.name), "β");
            assert_eq!(
                occurrence.query.name_reject().value().as_bytes(),
                "^β$".as_bytes()
            );
            assert!(std::ptr::eq(
                occurrence.query,
                custom_profile.queries().value().as_slice()[index].value()
            ));
        }
    });
    let operation = context();
    with_identity("BEGIN p; END;", &operation, |input| {
        operation.cancellation().cancel();
        assert!(matches!(
            parse_semantic_occurrences(profile, input),
            Err(SemanticError::Operation(error)) if error.kind() == ErrorKind::Interrupted
        ));
    });
}

#[test]
fn db2_shipped_header_parameters_and_contexts_native() {
    let yaml = include_bytes!("fixtures/catalog/shipped/db2.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/db2.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/db2.yaml")
    );
    let catalog = catalog(yaml);
    let profile = &catalog.profiles()[0];
    let parameter_paths = [
        "//agg_fn_param_decl/parameter_name",
        "//ext_scalar_param_decl/parameter_name",
        "//ext_table_param_decl/parameter_name",
        "//param_decl_3/parameter_name",
        "//sourced_function_param_decl/parameter_name",
        "//param_decl_2/parameter_name",
        "//proc_ext_param/parameter_name",
        "//proc_parameter_list_item/parameter_name",
        "//method_signature/method_param_list/method_signature_parameter_decl/parameter_name",
        "//method_specification/method_spec_parameter_list_paren/method_spec_parameter_list/method_spec_parameter_decl/parameter_name",
    ];
    let actual_paths: Vec<_> = profile
        .queries()
        .value()
        .iter()
        .filter(|q| q.value().data_key().value() == "parameters")
        .map(|q| {
            assert_eq!(q.value().graph_label().value(), "Parameter");
            assert_eq!(*q.value().r#type().value(), QueryKind::Entity);
            assert_eq!(q.value().name_capture().value(), "");
            q.value().pattern().value().as_str()
        })
        .collect();
    assert_eq!(actual_paths, parameter_paths);
    assert_eq!(
        profile
            .comment_types()
            .value()
            .iter()
            .map(|v| v.value().as_str())
            .collect::<Vec<_>>(),
        ["LINE_COMMENT", "SQL_COMMENT"]
    );
    let mut configured: Vec<_> = profile
        .queries()
        .value()
        .iter()
        .map(|entry| configured_query(entry.value()))
        .collect();
    configured.extend(
        profile
            .antlr_contexts()
            .value()
            .iter()
            .map(|value| configured_context(value.value())),
    );
    parse_profile_queries(
        Dialect::Db2,
        "SELECT 1 FROM TABLE_F;",
        &configured,
        &context(),
    )
    .unwrap();
    let mut diagnostics = Vec::new();
    let mut verify = |source: &str, key: &str, name: &str, parameters: &[&str], nearest: &str| {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            if !document.syntax.diagnostics.is_empty() {
                diagnostics.push(format!("{source}\n{:?}", document.syntax.diagnostics));
                return;
            }
            if source.starts_with("-- declaração") {
                for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
                    let token = document
                        .syntax
                        .tokens
                        .iter()
                        .find(|token| token.kind == kind)
                        .unwrap();
                    assert_ne!(token.channel, 0);
                    assert_eq!(
                        source.get(token.span.clone().unwrap()),
                        token.text.as_deref()
                    );
                }
            }
            for occurrence in &document.occurrences {
                assert!(std::ptr::eq(
                    occurrence.query,
                    profile.queries().value().as_slice()[occurrence.query_index].value()
                ));
                let node = &document.syntax.nodes[occurrence.matched.node_index];
                assert_eq!(node.status, SpanStatus::Regular);
                let capture = present(&occurrence.name);
                let captured = &document.syntax.nodes[capture.node_index];
                if nearest == "method_signature"
                    && occurrence.query.data_key().value() == "parameters"
                {
                    let declaration = &document.syntax.nodes[captured.parent.unwrap()];
                    assert_eq!(declaration.kind, "method_signature_parameter_decl");
                    assert_eq!(
                        document.syntax.nodes[declaration.parent.unwrap()].kind,
                        "method_param_list"
                    );
                    let extent = declaration.span.as_ref().unwrap();
                    let name = captured.span.as_ref().unwrap();
                    assert!(extent.start <= name.start && extent.end >= name.end);
                }
                assert_eq!(
                    &source[captured.span.clone().unwrap()],
                    text(&occurrence.name)
                );
                assert_eq!(
                    text(&occurrence.name),
                    match capture.original.as_ref().unwrap() {
                        OriginalProjection::Exact(range) => range.text(),
                        _ => panic!("expected unchanged original bytes"),
                    }
                );
            }
            for category in ["functions", "procedures", "parameters"] {
                let actual: Vec<_> = document
                    .occurrences
                    .iter()
                    .filter(|o| o.query.data_key().value() == category)
                    .map(|o| text(&o.name))
                    .collect();
                let expected = if category == "parameters" {
                    parameters
                } else if category == key {
                    std::slice::from_ref(&name)
                } else {
                    &[]
                };
                assert_eq!(actual, expected, "{source}: {category}");
            }
            for occurrence in document.occurrences.iter().filter(|o| {
                o.query.data_key().value() == key || o.query.data_key().value() == "parameters"
            }) {
                let scope = &document.contexts[occurrence.contexts[0]];
                assert_eq!(scope.selector, nearest, "{source}");
                assert_eq!(
                    scope.label,
                    Some(if key == "procedures" {
                        "StoredProcedure"
                    } else {
                        "Function"
                    })
                );
                assert_eq!(text(&scope.name), name);
                assert_eq!(
                    document.syntax.nodes[scope.matched.node_index].kind,
                    nearest
                );
                if nearest == "method_specification" {
                    assert!(
                        occurrence
                            .contexts
                            .iter()
                            .skip(1)
                            .any(|index| document.contexts[*index].selector
                                == "create_type_structured_statement")
                    );
                }
                if occurrence.query.data_key().value() == "parameters" {
                    assert!(matches!(occurrence.value, CaptureBinding::NotConfigured));
                    assert!(matches!(occurrence.span, CaptureBinding::NotConfigured));
                    assert!(matches!(occurrence.parent, CaptureBinding::NotConfigured));
                    assert!(matches!(
                        occurrence.qualifier,
                        CaptureBinding::NotConfigured
                    ));
                    assert_eq!(
                        document.syntax.nodes[occurrence.matched.node_index].kind,
                        "parameter_name"
                    );
                }
            }
            assert!(
                !document
                    .occurrences
                    .iter()
                    .any(|o| o.query.graph_label().value() == "Comment")
            );
        });
    };
    let fixtures: [(&str, &str, &str, &[&str], &str); 11] = [
        (
            "CREATE FUNCTION F_SCALAR(P_SCALAR INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE JAVA PARAMETER STYLE JAVA EXTERNAL NAME 'pkg.scalar' NO SQL;",
            "functions",
            "F_SCALAR",
            &["P_SCALAR"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_TABLE(P_TABLE CLOB DEFAULT NULL AS LOCATOR) RETURNS TABLE(C_A INTEGER) LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.table' NO SQL;",
            "functions",
            "F_TABLE",
            &["P_TABLE"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_OLE(P_OLE VARCHAR(40) DEFAULT 'cmd') RETURNS TABLE(C_A INTEGER) LANGUAGE OLEDB EXTERNAL NAME 'provider';",
            "functions",
            "F_OLE",
            &["P_OLE"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_SOURCE(P_SOURCE INTEGER DEFAULT 1) RETURNS INTEGER SOURCE SOURCE_F(INTEGER);",
            "functions",
            "F_SOURCE",
            &["P_SOURCE"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_TEMPLATE(P_TEMPLATE INTEGER DEFAULT 1) RETURNS INTEGER AS TEMPLATE NOT DETERMINISTIC NO EXTERNAL ACTION;",
            "functions",
            "F_TEMPLATE",
            &["P_TEMPLATE"],
            "create_function_statement",
        ),
        (
            "CREATE FUNCTION F_SQL(P_SQL INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL BEGIN ATOMIC DECLARE LOCAL_F INTEGER DEFAULT 2; RETURN P_SQL; END;",
            "functions",
            "F_SQL",
            &["P_SQL"],
            "create_function_statement",
        ),
        (
            "CREATE PROCEDURE P_EXTERNAL(IN P_IN INTEGER DEFAULT 1,OUT P_OUT INTEGER,INOUT P_IO INTEGER) LANGUAGE JAVA PARAMETER STYLE JAVA EXTERNAL NAME 'pkg.proc';",
            "procedures",
            "P_EXTERNAL",
            &["P_IN", "P_OUT", "P_IO"],
            "create_procedure_statement",
        ),
        (
            "CREATE PROCEDURE P_SQL(IN P_IN INTEGER DEFAULT 1,OUT P_OUT INTEGER,INOUT P_IO INTEGER) LANGUAGE SQL BEGIN ATOMIC CALL OTHER_F(P_IN=>P_IN); SET P_OUT=P_IN; RETURN; END;",
            "procedures",
            "P_SQL",
            &["P_IN", "P_OUT", "P_IO"],
            "create_procedure_statement",
        ),
        (
            "CREATE METHOD METHOD_F(P_METHOD INTEGER,P_LOCATOR CLOB AS LOCATOR) RETURNS INTEGER FOR TYPE_F EXTERNAL NAME 'pkg.method';",
            "functions",
            "METHOD_F",
            &["P_METHOD", "P_LOCATOR"],
            "method_signature",
        ),
        (
            "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_SPEC INTEGER,P_SPEC_2 INTEGER) RETURNS INTEGER LANGUAGE SQL;",
            "functions",
            "METHOD_F",
            &["P_SPEC", "P_SPEC_2"],
            "method_specification",
        ),
        (
            "CREATE FUNCTION AGG_F(IN P_AGG DOUBLE DEFAULT 1) RETURNS DOUBLE AGGREGATE WITH (S_STATE DOUBLE) USING INITIALIZE PROCEDURE INIT_F ACCUMULATE PROCEDURE ACC_F MERGE PROCEDURE MERGE_F FINALIZE FUNCTION FINAL_F;",
            "functions",
            "AGG_F",
            &["P_AGG"],
            "create_function_aggregate_interface_statement",
        ),
    ];
    for (source, key, name, parameters, nearest) in fixtures {
        verify(source, key, name, parameters, nearest);
        // SQL method specifications require named parameters; their external
        // counterpart below supplies the legitimate unnamed control.
        if nearest != "method_specification" && name != "F_SQL" && name != "P_SQL" {
            let mut unnamed = source.to_owned();
            for parameter in parameters {
                unnamed = unnamed.replace(&format!("{parameter} "), "");
            }
            if nearest == "create_function_aggregate_interface_statement" {
                unnamed = unnamed.replace("(IN DOUBLE", "(DOUBLE");
            }
            verify(&unnamed, key, name, &[], nearest);
        }
        let open = source.find('(').unwrap();
        if nearest != "method_specification" {
            let mut depth = 1;
            let mut close = open + 1;
            for (offset, ch) in source[open + 1..].char_indices() {
                if ch == '(' {
                    depth += 1;
                }
                if ch == ')' {
                    depth -= 1;
                }
                if depth == 0 {
                    close = open + 1 + offset;
                    break;
                }
            }
            let empty = format!("{}{}", &source[..open + 1], &source[close..]);
            verify(&empty, key, name, &[], nearest);
        }
    }
    verify(
        "CREATE TYPE TYPE_F AS (ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(INTEGER) RETURNS INTEGER LANGUAGE C;",
        "functions",
        "METHOD_F",
        &[],
        "method_specification",
    );
    verify(
        "CREATE PROCEDURE P_SOURCE SOURCE REMOTE_SCHEMA.SOURCE_PROC_F() FOR SERVER SERVER_F;",
        "procedures",
        "P_SOURCE",
        &[],
        "create_procedure_statement",
    );
    verify(
        "CREATE VARIABLE CURSOR_F CURSOR DEFAULT (ASENSITIVE CURSOR(P_CURSOR INTEGER) WITH HOLD FOR SELECT 1 FROM TABLE_F);",
        "",
        "",
        &[],
        "",
    );
    verify("CALL OTHER_F(P_IN=>1);", "", "", &[], "");
    let raw = "-- declaração\r\nCREATE /* outer /* nested */ comment */ FUNCTION \"café\".\"função\"(\"parâmetro\" INTEGER DEFAULT 1) RETURNS INTEGER LANGUAGE SQL RETURN \"parâmetro\";\r\n";
    verify(
        raw,
        "functions",
        "\"café\".\"função\"",
        &["\"parâmetro\""],
        "create_function_statement",
    );
    assert!(
        diagnostics.is_empty(),
        "Db2 fixture diagnostics:\n{}",
        diagnostics.join("\n")
    );
    for source in [
        "CREATE FUNCTION F_SQL(INTEGER) RETURNS INTEGER LANGUAGE SQL RETURN 1;",
        "CREATE PROCEDURE P_SQL(IN INTEGER) LANGUAGE SQL BEGIN ATOMIC RETURN; END;",
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            assert!(!document.syntax.diagnostics.is_empty(), "{source}");
            assert!(
                graphit_ast::antlr_engine::parse_strict(Dialect::Db2, source, &context()).is_err()
            );
        });
    }
    let cancellation = Cancellation::default();
    let cancelled = SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        cancellation.clone(),
    );
    with_identity(fixtures[0].0, &cancelled, |input| {
        cancellation.cancel();
        assert!(
            matches!(parse_semantic_occurrences(profile, input), Err(SemanticError::Operation(error)) if error.kind() == ErrorKind::Interrupted)
        );
    });
    let expires = SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    );
    with_identity(fixtures[0].0, &expires, |input| {
        std::thread::sleep(Duration::from_millis(30));
        assert!(
            matches!(parse_semantic_occurrences(profile, input), Err(SemanticError::Operation(error)) if error.kind() == ErrorKind::TimedOut)
        );
    });
}

#[test]
fn db2_shipped_type_declarations_and_contexts_native() {
    let yaml = include_bytes!("fixtures/catalog/shipped/db2.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/db2.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/db2.yaml")
    );
    let catalog = catalog(yaml);
    let profile = &catalog.profiles()[0];
    let branches = [
        ("create_type_array_statement", "type_name"),
        ("create_type_cursor_statement", "type_name"),
        ("create_type_distinct_statement", "distinct_type_name"),
        ("create_type_row_statement", "type_name"),
        ("create_type_structured_statement", "type_name"),
    ];
    let mut gaps = Vec::new();
    for (body, name, branch) in [
        ("CREATE TYPE ARRAY_F AS INTEGER ARRAY[100];", "ARRAY_F", 0),
        ("CREATE TYPE CURSOR_F AS CURSOR;", "CURSOR_F", 1),
        ("CREATE TYPE DISTINCT_F AS INTEGER;", "DISTINCT_F", 2),
        ("CREATE TYPE ROW_F AS ROW(FIELD_F INTEGER);", "ROW_F", 3),
        (
            "CREATE TYPE TYPE_F AS(ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL METHOD METHOD_F(P_F INTEGER) RETURNS INTEGER LANGUAGE SQL;",
            "TYPE_F",
            4,
        ),
        (
            "CREATE TYPE CUR_STRONG_F AS ROW_F CURSOR;",
            "CUR_STRONG_F",
            1,
        ),
        (
            "CREATE TYPE CUR_ANCHOR_F AS ANCHOR ROW OF TABLE_F CURSOR;",
            "CUR_ANCHOR_F",
            1,
        ),
        ("CREATE TYPE \"Té\" AS ROW(FIELD_F INTEGER);", "\"Té\"", 3),
        (
            "CREATE TYPE \"Té\" AS(\"Aé\" INTEGER) NOT FINAL MODE DB2SQL METHOD \"Mé\"(\"Pé\" INTEGER) RETURNS INTEGER LANGUAGE SQL;",
            "\"Té\"",
            4,
        ),
        (
            "CREATE TYPE SAME_F AS(A_F INTEGER) NOT FINAL MODE DB2SQL METHOD SAME_F(SAME_F INTEGER) RETURNS INTEGER LANGUAGE SQL;",
            "SAME_F",
            4,
        ),
    ] {
        let source = format!("-- declaração α\r\n/* β /* γ */ δ */ {body}");
        let operation = context();
        with_identity(&source, &operation, |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            if !document.syntax.diagnostics.is_empty() {
                gaps.push(format!(
                    "{name}: native syntax {:?}",
                    document.syntax.diagnostics
                ));
                return;
            }
            let expected = branches[branch];
            let types = document
                .occurrences
                .iter()
                .filter(|o| o.query.data_key().value() == "types")
                .collect::<Vec<_>>();
            if types.len() != 1 {
                gaps.push(format!(
                    "{name}: stock Type count {}, expected 1",
                    types.len()
                ));
            } else {
                let occurrence = types[0];
                assert_eq!(occurrence.query.graph_label().value(), "Type");
                assert_eq!(*occurrence.query.r#type().value(), QueryKind::Entity);
                assert_eq!(text(&occurrence.name), name);
                let selected = present(&occurrence.name);
                let node = &document.syntax.nodes[selected.node_index];
                assert_eq!(node.kind, expected.1);
                assert_eq!(document.syntax.nodes[node.parent.unwrap()].kind, expected.0);
                assert_eq!(selected.node_index, occurrence.matched.node_index);
                assert_eq!(source.get(node.span.clone().unwrap()), Some(name));
                assert!(
                    matches!(selected.original.as_ref().unwrap(), OriginalProjection::Exact(range) if range.text() == name)
                );
                assert!(std::ptr::eq(
                    occurrence.query,
                    profile.queries().value().as_slice()[occurrence.query_index].value()
                ));
            }

            if name == "SAME_F" {
                let occurrences = document
                    .occurrences
                    .iter()
                    .filter(|o| {
                        matches!(
                            o.query.data_key().value().as_str(),
                            "types" | "functions" | "parameters"
                        )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(occurrences.len(), 3);
                for (index, occurrence) in occurrences.iter().enumerate() {
                    assert_eq!(text(&occurrence.name), "SAME_F");
                    let captured = present(&occurrence.name);
                    for other in occurrences.iter().skip(index + 1) {
                        let other = present(&other.name);
                        assert_ne!(captured.node_index, other.node_index);
                        assert_ne!(
                            document.syntax.nodes[captured.node_index].span,
                            document.syntax.nodes[other.node_index].span
                        );
                    }
                }
            }
            let frames = document
                .contexts
                .iter()
                .enumerate()
                .filter(|(_, c)| c.label == Some("Type"))
                .collect::<Vec<_>>();
            if frames.len() != 1 {
                gaps.push(format!(
                    "{name}: Type frame count {}, expected 1",
                    frames.len()
                ));
            } else {
                let (index, frame) = frames[0];
                if frame.selector != expected.0 || !matches!(frame.name, CaptureBinding::Present(_))
                {
                    gaps.push(format!(
                        "{name}: frame {} without declared branch/name",
                        frame.selector
                    ));
                } else {
                    assert!(frame.name_is_explicit);
                    assert_eq!(text(&frame.name), name);
                    assert_eq!(
                        document.syntax.nodes[frame.matched.node_index].kind,
                        expected.0
                    );
                    let declared = present(&frame.name);
                    assert_eq!(
                        document.syntax.nodes[declared.node_index].parent,
                        Some(frame.matched.node_index)
                    );
                    assert!(
                        matches!(declared.original.as_ref().unwrap(), OriginalProjection::Exact(range) if range.text() == name)
                    );
                    for occurrence in &types {
                        assert_eq!(occurrence.contexts[0], index);
                    }
                    if branch == 4 {
                        for occurrence in document.occurrences.iter().filter(|o| {
                            matches!(
                                o.query.data_key().value().as_str(),
                                "functions" | "parameters"
                            )
                        }) {
                            assert_eq!(
                                document.contexts[occurrence.contexts[0]].selector,
                                "method_specification"
                            );
                            assert!(occurrence.contexts.iter().skip(1).any(|i| *i == index));
                        }
                    }
                }
            }
            for token in &document.syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(text.as_str()));
                }
            }
        });
    }
    assert!(
        gaps.is_empty(),
        "native/stock Type gaps:\n{}",
        gaps.join("\n")
    );
    for body in [
        "SELECT COLUMN_F FROM TABLE_F;",
        "ALTER TYPE TYPE_F ADD METHOD METHOD_F(P_F INTEGER) RETURNS INTEGER LANGUAGE SQL;",
    ] {
        with_identity(body, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            assert!(
                document.syntax.diagnostics.is_empty(),
                "{body}: {:?}",
                document.syntax.diagnostics
            );
            assert!(
                !document
                    .occurrences
                    .iter()
                    .any(|o| o.query.data_key().value() == "types")
            );
            assert!(!document.contexts.iter().any(|c| c.label == Some("Type")));
        });
    }
    for source in [
        "CREATE TYPE CURSOR_F CURSOR;",
        "CREATE TYPE CURSOR_F ROW_F CURSOR;",
        "CREATE TYPE CURSOR_F AS ROW_F;",
    ] {
        let parsed = parse_profile_queries(Dialect::Db2, source, &[], &context()).unwrap();
        assert!(!parsed.syntax.diagnostics.is_empty(), "{source}");
        assert!(matches!(
            graphit_ast::antlr_engine::parse_strict(Dialect::Db2, source, &context()),
            Err(graphit_ast::antlr_engine::AntlrEngineError::Syntax { .. })
        ));
    }
}

#[test]
fn db2_shipped_variable_and_alias_subjects_native() {
    let yaml = include_bytes!("fixtures/catalog/shipped/db2.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/db2.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/db2.yaml")
    );
    let catalog = catalog(yaml);
    let profile = &catalog.profiles()[0];
    let mut gaps = Vec::new();
    for (body, expected) in [
        (
            "BEGIN ATOMIC DECLARE V_F INTEGER DEFAULT 1; END;",
            vec!["V_F"],
        ),
        (
            "BEGIN ATOMIC DECLARE V_A,V_B,V_C INTEGER DEFAULT 1; END;",
            vec!["V_A", "V_B", "V_C"],
        ),
        (
            "BEGIN ATOMIC DECLARE \"Vé\",V_F INTEGER DEFAULT 1; END;",
            vec!["\"Vé\"", "V_F"],
        ),
        (
            "CREATE PROCEDURE P_F() LANGUAGE SQL BEGIN DECLARE V_F INTEGER DEFAULT 1; END;",
            vec!["V_F"],
        ),
    ] {
        let source = format!("-- declaração α\r\n/* β /* γ */ δ */ {body}");
        with_identity(&source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            if !document.syntax.diagnostics.is_empty() {
                gaps.push(format!(
                    "{body}: native diagnostics {:?}",
                    document.syntax.diagnostics
                ));
                return;
            }
            let vars = document
                .occurrences
                .iter()
                .filter(|o| o.query.data_key().value() == "variables")
                .collect::<Vec<_>>();
            if vars.len() != expected.len() {
                gaps.push(format!(
                    "{body}: Variable count {}, expected {}",
                    vars.len(),
                    expected.len()
                ));
                return;
            }
            let mut declaration = None;
            for (occurrence, name) in vars.iter().zip(expected) {
                assert_eq!(occurrence.query.graph_label().value(), "Variable");
                assert_eq!(*occurrence.query.r#type().value(), QueryKind::Entity);
                assert_eq!(text(&occurrence.name), name);
                let selected = present(&occurrence.name);
                assert_eq!(selected.node_index, occurrence.matched.node_index);
                let node = &document.syntax.nodes[selected.node_index];
                assert_eq!(node.kind, "variable_name");
                assert_eq!(node.status, SpanStatus::Regular);
                assert_eq!(source.get(node.span.clone().unwrap()), Some(name));
                assert!(
                    matches!(selected.original.as_ref().unwrap(), OriginalProjection::Exact(range) if range.text() == name)
                );
                let parent = node.parent.unwrap();
                assert_eq!(
                    document.syntax.nodes[parent].kind,
                    "declare_variable_statement"
                );
                if let Some(previous) = declaration {
                    assert_eq!(previous, parent);
                }
                declaration = Some(parent);
                let statement = source
                    .get(document.syntax.nodes[parent].span.clone().unwrap())
                    .unwrap();
                assert!(statement.starts_with("DECLARE "));
                assert!(statement.ends_with("INTEGER DEFAULT 1"));
                assert_eq!(
                    document
                        .syntax
                        .nodes
                        .iter()
                        .filter(|n| n.parent == Some(parent) && n.kind == "data_type")
                        .count(),
                    1
                );
                assert_eq!(
                    document
                        .syntax
                        .nodes
                        .iter()
                        .filter(|n| n.parent == Some(parent)
                            && n.kind == "declare_variable_default_clause")
                        .count(),
                    1
                );
                if body.starts_with("CREATE PROCEDURE") {
                    assert_eq!(
                        document.contexts[occurrence.contexts[0]].selector,
                        "create_procedure_statement"
                    );
                    assert_eq!(text(&document.contexts[occurrence.contexts[0]].name), "P_F");
                }
            }
            for pair in vars.windows(2) {
                let a = present(&pair[0].name);
                let b = present(&pair[1].name);
                assert_ne!(a.node_index, b.node_index);
                assert_ne!(
                    document.syntax.nodes[a.node_index].span,
                    document.syntax.nodes[b.node_index].span
                );
            }
        });
    }
    for (body, name, branch) in [
        (
            "CREATE ALIAS A_TABLE_F FOR TABLE TABLE_REF_F;",
            "A_TABLE_F",
            "table_alias",
        ),
        (
            "CREATE ALIAS A_CHAIN_F FOR A_TARGET_F;",
            "A_CHAIN_F",
            "table_alias",
        ),
        (
            "CREATE ALIAS SAME_F FOR TABLE SAME_F;",
            "SAME_F",
            "table_alias",
        ),
        (
            "CREATE ALIAS A_MODULE_F FOR MODULE MODULE_REF_F;",
            "A_MODULE_F",
            "module_alias",
        ),
        (
            "CREATE ALIAS A_SEQ_F FOR SEQUENCE SEQ_REF_F;",
            "A_SEQ_F",
            "sequence_alias",
        ),
        (
            "CREATE OR REPLACE PUBLIC ALIAS A_F FOR TABLE T_REF_F;",
            "A_F",
            "table_alias",
        ),
        (
            "CREATE ALIAS \"Aé\" FOR TABLE \"Ré\";",
            "\"Aé\"",
            "table_alias",
        ),
    ] {
        let source = format!("-- declaração α\r\n/* β /* γ */ δ */ {body}");
        with_identity(&source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            if !document.syntax.diagnostics.is_empty() {
                gaps.push(format!(
                    "{body}: native diagnostics {:?}",
                    document.syntax.diagnostics
                ));
                return;
            }
            let aliases = document
                .occurrences
                .iter()
                .filter(|o| o.query.data_key().value() == "aliases")
                .collect::<Vec<_>>();
            if aliases.len() != 1 {
                gaps.push(format!("{body}: Alias count {}, expected 1", aliases.len()));
                return;
            }
            let occurrence = aliases[0];
            assert_eq!(occurrence.query.graph_label().value(), "Alias");
            assert_eq!(*occurrence.query.r#type().value(), QueryKind::Entity);
            assert_eq!(
                occurrence.query.pattern().value(),
                &format!("//create_alias_statement/{branch}/declared_alias_name/alias_name")
            );
            assert_eq!(text(&occurrence.name), name);
            let selected = present(&occurrence.name);
            assert_eq!(selected.node_index, occurrence.matched.node_index);
            let node = &document.syntax.nodes[selected.node_index];
            assert_eq!(node.kind, "alias_name");
            assert_eq!(node.status, SpanStatus::Regular);
            assert_eq!(source.get(node.span.clone().unwrap()), Some(name));
            assert!(
                matches!(selected.original.as_ref().unwrap(), OriginalProjection::Exact(range) if range.text() == name)
            );
            let parent = node.parent.unwrap();
            assert_eq!(document.syntax.nodes[parent].kind, "declared_alias_name");
            let typed = document.syntax.nodes[parent].parent.unwrap();
            assert_eq!(document.syntax.nodes[typed].kind, branch);
            let whole = document.syntax.nodes[typed].parent.unwrap();
            assert_eq!(document.syntax.nodes[whole].kind, "create_alias_statement");
            assert_eq!(
                source.get(document.syntax.nodes[whole].span.clone().unwrap()),
                Some(body.trim_end_matches(';'))
            );
            for token in &document.syntax.tokens {
                if token.eof {
                    assert_eq!(token.status, SpanStatus::Eof);
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(raw)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(raw.as_str()));
                }
            }
        });
    }
    for (source, default_count) in [
        (
            "BEGIN ATOMIC DECLARE V_NULL_F INTEGER DEFAULT NULL; END;",
            1,
        ),
        ("BEGIN ATOMIC DECLARE V_NONE_F INTEGER; END;", 0),
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            assert!(
                document.syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                document.syntax.diagnostics
            );
            let variable = document
                .occurrences
                .iter()
                .find(|o| o.query.data_key().value() == "variables")
                .unwrap();
            let declaration = document.syntax.nodes[present(&variable.name).node_index]
                .parent
                .unwrap();
            assert_eq!(
                document
                    .syntax
                    .nodes
                    .iter()
                    .filter(|n| n.parent == Some(declaration)
                        && n.kind == "declare_variable_default_clause")
                    .count(),
                default_count
            );
        });
    }
    let ddl = "CREATE TABLE T_DEFAULT_F(C_DEFAULT_F INTEGER DEFAULT);";
    with_identity(ddl, &context(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert!(
            document.syntax.diagnostics.is_empty(),
            "{:?}",
            document.syntax.diagnostics
        );
        assert!(
            document
                .syntax
                .nodes
                .iter()
                .any(|n| n.kind == "default_clause"
                    && ddl.get(n.span.clone().unwrap()) == Some("DEFAULT"))
        );
        assert!(
            document
                .syntax
                .nodes
                .iter()
                .all(|n| n.kind != "declare_variable_default_clause")
        );
    });
    let mixed = "BEGIN ATOMIC DECLARE V_F INTEGER DEFAULT 1; END; CREATE ALIAS A_AFTER_F FOR TABLE T_REF_F;";
    with_identity(mixed, &context(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert!(
            document.syntax.diagnostics.is_empty(),
            "{:?}",
            document.syntax.diagnostics
        );
        let subjects = document
            .occurrences
            .iter()
            .filter(|o| matches!(o.query.data_key().value().as_str(), "variables" | "aliases"))
            .collect::<Vec<_>>();
        assert_eq!(subjects.len(), 2);
        let variable = subjects
            .iter()
            .find(|o| o.query.data_key().value() == "variables")
            .unwrap();
        let alias = subjects
            .iter()
            .find(|o| o.query.data_key().value() == "aliases")
            .unwrap();
        assert_eq!(text(&variable.name), "V_F");
        assert_eq!(text(&alias.name), "A_AFTER_F");
        assert!(variable.contexts.is_empty());
        let compound = document
            .syntax
            .nodes
            .iter()
            .find(|n| n.kind == "compound_sql_inlined")
            .unwrap();
        assert_eq!(
            mixed.get(compound.span.clone().unwrap()),
            Some("BEGIN ATOMIC DECLARE V_F INTEGER DEFAULT 1; END")
        );
    });
    for source in [
        "BEGIN DECLARE SECTION;",
        "BEGIN DECLARE SECTION; END DECLARE SECTION;",
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            assert!(
                document.syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                document.syntax.diagnostics
            );
            assert!(
                document
                    .occurrences
                    .iter()
                    .all(|o| o.query.data_key().value() != "variables")
            );
            assert!(
                document
                    .syntax
                    .nodes
                    .iter()
                    .any(|n| n.kind == "begin_declare_section_statement")
            );
        });
    }
    assert!(
        gaps.is_empty(),
        "native stock subject gaps:\n{}",
        gaps.join("\n")
    );
    for source in [
        "BEGIN ATOMIC DECLARE V_F INTEGER DEFAULT 1;",
        "BEGIN ATOMIC DECLARE V_F, INTEGER DEFAULT 1; END;",
        "BEGIN ATOMIC DECLARE V_F INTEGER WITH DEFAULT 1; END;",
        "BEGIN ATOMIC DECLARE V_F INTEGER DEFAULT; END;",
        "CREATE ALIAS A_F TABLE T_F;",
        "CREATE ALIAS A_F FOR;",
        "CREATE ALIAS A_F,B_F FOR TABLE T_F;",
    ] {
        let raw = parse_profile_queries(Dialect::Db2, source, &[], &context()).unwrap();
        assert!(!raw.syntax.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                graphit_ast::antlr_engine::parse_strict(Dialect::Db2, source, &context()),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
}

#[test]
fn db2_shipped_index_keys_and_cluster_native() {
    let yaml = include_bytes!("fixtures/catalog/shipped/db2.yaml");
    assert_eq!(yaml, include_bytes!("fixtures/profiles/shipped/db2.yaml"));
    assert_eq!(
        yaml,
        include_bytes!("../../../crates/graphit-ast/profiles/queries/db2.yaml")
    );
    let catalog = catalog(yaml);
    let profile = &catalog.profiles()[0];
    let mut gaps = Vec::new();
    for (body, name, columns, expressions, clustered, unique) in [
        (
            "CREATE INDEX I_BARE_F ON T_F(C_F);",
            "I_BARE_F",
            vec!["C_F"],
            0,
            false,
            false,
        ),
        (
            "CREATE UNIQUE INDEX I_UNIQUE_F ON T_F(C_A ASC,C_B DESC,C_C RANDOM);",
            "I_UNIQUE_F",
            vec!["C_A", "C_B", "C_C"],
            0,
            false,
            true,
        ),
        (
            "CREATE INDEX I_OLD_F ON T_F(C_F ASC) CLUSTER;",
            "I_OLD_F",
            vec!["C_F"],
            0,
            true,
            false,
        ),
        (
            "CREATE INDEX I_CLUSTER_F ON T_F(C_F) CLUSTER;",
            "I_CLUSTER_F",
            vec!["C_F"],
            0,
            true,
            false,
        ),
        (
            "CREATE INDEX I_EXPR_F ON T_F(C_F+1 DESC);",
            "I_EXPR_F",
            vec![],
            1,
            false,
            false,
        ),
        (
            "CREATE INDEX I_CALL_F ON T_F(ABS(C_F));",
            "I_CALL_F",
            vec![],
            1,
            false,
            false,
        ),
        (
            "CREATE INDEX \"Ié\" ON \"Té\"(\"Cé\");",
            "\"Ié\"",
            vec!["\"Cé\""],
            0,
            false,
            false,
        ),
        (
            "CREATE INDEX CLUSTER ON CLUSTER(CLUSTER) CLUSTER;",
            "CLUSTER",
            vec!["CLUSTER"],
            0,
            true,
            false,
        ),
        (
            "CREATE UNIQUE INDEX I_TIME_F ON T_F(C_F,BUSINESS_TIME WITHOUT OVERLAPS);",
            "I_TIME_F",
            vec!["C_F"],
            0,
            false,
            true,
        ),
    ] {
        let source = format!("-- índice α\r\n/* β /* γ */ δ */ {body}");
        with_identity(&source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            if !document.syntax.diagnostics.is_empty() {
                gaps.push(format!("{body}: {:?}", document.syntax.diagnostics));
                return;
            }
            let indexes = document
                .occurrences
                .iter()
                .filter(|o| o.query.data_key().value() == "indexes")
                .collect::<Vec<_>>();
            if indexes.len() != 1 {
                gaps.push(format!("{body}: Index count {}", indexes.len()));
                return;
            }
            let index = indexes[0];
            assert_eq!(index.query.graph_label().value(), "Index");
            assert_eq!(*index.query.r#type().value(), QueryKind::Entity);
            assert_eq!(text(&index.name), name);
            let selected = present(&index.name);
            assert_eq!(
                document.syntax.nodes[selected.node_index].kind,
                "index_name"
            );
            assert_eq!(
                document.syntax.nodes[selected.node_index].parent,
                Some(index.matched.node_index)
            );
            assert_eq!(
                source.get(
                    index.matched.prepared.unwrap().start()..index.matched.prepared.unwrap().end()
                ),
                Some(body.trim_end_matches(';'))
            );
            assert!(
                matches!(selected.original.as_ref().unwrap(), OriginalProjection::Exact(r) if r.text() == name)
            );
            if unique {
                assert_eq!(text(&index.value), "UNIQUE");
            } else {
                assert!(matches!(index.value, CaptureBinding::Missing));
            }
            let keys = document
                .occurrences
                .iter()
                .filter(|o| o.query.data_key().value() == "columns")
                .collect::<Vec<_>>();
            assert_eq!(keys.len(), columns.len());
            for (key, expected) in keys.iter().zip(columns) {
                assert_eq!(text(&key.name), expected);
                let node = &document.syntax.nodes[present(&key.name).node_index];
                assert_eq!(node.kind, "column_name");
                assert_eq!(
                    document.syntax.nodes[node.parent.unwrap()].kind,
                    "index_col_opts_item"
                );
                assert!(
                    matches!(present(&key.name).original.as_ref().unwrap(), OriginalProjection::Exact(r) if r.text() == expected)
                );
                assert_eq!(
                    document.contexts[key.contexts[0]].selector,
                    "create_index_statement"
                );
                if text(&document.contexts[key.contexts[0]].name) != name {
                    gaps.push(format!("{body}: Index context lacks actual declared name"));
                }
            }
            assert!(
                document
                    .occurrences
                    .iter()
                    .all(|o| o.query.graph_label().value() != "Table")
            );
            assert_eq!(
                document
                    .syntax
                    .nodes
                    .iter()
                    .filter(|n| n.kind == "key_expression")
                    .count(),
                expressions
            );
            let cluster = document
                .syntax
                .nodes
                .iter()
                .filter(|n| n.kind == "index_cluster_clause")
                .collect::<Vec<_>>();
            if cluster.len() != usize::from(clustered) {
                gaps.push(format!(
                    "{body}: native cluster helper count {}",
                    cluster.len()
                ));
                return;
            }
            if let Some(clause) = cluster.first() {
                assert_eq!(source.get(clause.span.clone().unwrap()), Some("CLUSTER"));
                assert!(
                    document
                        .syntax
                        .nodes
                        .iter()
                        .any(|n| n.parent == Some(index.matched.node_index)
                            && n.kind == "index_cluster_clause")
                );
                assert!(
                    document
                        .syntax
                        .nodes
                        .iter()
                        .any(|n| n.kind == "CLUSTER" && n.class == NodeClass::Terminal)
                );
            }
            for token in &document.syntax.tokens {
                if token.eof {
                    assert_eq!(token.span, Some(source.len()..source.len()));
                } else if let (Some(span), Some(raw)) = (&token.span, &token.text) {
                    assert_eq!(source.get(span.clone()), Some(raw.as_str()));
                }
            }
        });
    }
    let ordinary = "CREATE TABLE CLUSTER(CLUSTER INTEGER);";
    with_identity(ordinary, &context(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert!(
            document.syntax.diagnostics.is_empty(),
            "{:?}",
            document.syntax.diagnostics
        );
        let frame = document
            .contexts
            .iter()
            .find(|c| c.selector == "create_table_statement")
            .unwrap();
        if text(&frame.name) != "CLUSTER" {
            gaps.push("ordinary CLUSTER Table context lacks actual table_name".to_owned());
        } else {
            let name = present(&frame.name);
            assert_eq!(document.syntax.nodes[name.node_index].kind, "table_name");
            assert!(
                matches!(name.original.as_ref().unwrap(), OriginalProjection::Exact(r) if r.text() == "CLUSTER")
            );
        }
        assert_eq!(
            document
                .occurrences
                .iter()
                .filter(|o| matches!(o.query.graph_label().value().as_str(), "Table" | "Column"))
                .count(),
            2
        );
    });
    for (source, selector, name, kind) in [
        (
            "CREATE VIEW V_VALUES_F(C_A) AS VALUES 11;",
            "create_view_statement",
            Some("V_VALUES_F"),
            "view_name",
        ),
        (
            "CREATE SCHEMA SCHEMA_F;",
            "create_schema_statement",
            Some("SCHEMA_F"),
            "schema_name",
        ),
        (
            "CREATE SCHEMA AUTHORIZATION OWNER_F;",
            "create_schema_statement",
            None,
            "schema_name",
        ),
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            assert!(
                document.syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                document.syntax.diagnostics
            );
            let frame = document
                .contexts
                .iter()
                .find(|c| c.selector == selector)
                .unwrap();
            if let Some(name) = name {
                let captured = present(&frame.name);
                if text(&frame.name) != name {
                    gaps.push(format!("{source}: context lacks actual declared name"));
                    return;
                }
                assert_eq!(document.syntax.nodes[captured.node_index].kind, kind);
                assert!(
                    matches!(captured.original.as_ref().unwrap(), OriginalProjection::Exact(r) if r.text() == name)
                );
            } else {
                if !matches!(frame.name, CaptureBinding::Missing) {
                    gaps.push(format!(
                        "{source}: unnamed Schema context fabricated a name"
                    ));
                }
                assert!(
                    document
                        .occurrences
                        .iter()
                        .all(|o| o.query.graph_label().value() != "Schema")
                );
            }
        });
    }

    assert!(gaps.is_empty(), "native Index gaps:\n{}", gaps.join("\n"));
    for source in [
        "CREATE INDEX ON T_F(C_F);",
        "CREATE INDEX I_F T_F(C_F);",
        "CREATE INDEX I_F ON T_F();",
        "CREATE INDEX I_F ON T_F(C_F,);",
        "CREATE INDEX I_F ON T_F(C_F+);",
        "CREATE INDEX I_F ON T_F(C_F) CLUSTER CLUSTER;",
        "CREATE INDEX I_F ON T_F(C_F) BOGUS;",
    ] {
        let raw = parse_profile_queries(Dialect::Db2, source, &[], &context()).unwrap();
        assert!(!raw.syntax.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                graphit_ast::antlr_engine::parse_strict(Dialect::Db2, source, &context()),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
}

#[test]
fn db2_shipped_trigger_target_wrapper_native() {
    let catalog = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let profile = &catalog.profiles()[0];
    let source = "CREATE TRIGGER TRIGGER_F AFTER INSERT ON TABLE_F FOR EACH ROW INSERT INTO AUDIT_F VALUES (11);";
    with_identity(source, &context(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert!(
            document.syntax.diagnostics.is_empty(),
            "{:?}",
            document.syntax.diagnostics
        );
        let frame = document
            .contexts
            .iter()
            .find(|c| c.selector == "create_trigger_statement")
            .unwrap();
        assert_eq!(text(&frame.name), "TRIGGER_F");
        let declared = present(&frame.name);
        assert_eq!(
            document.syntax.nodes[declared.node_index].kind,
            "trigger_name"
        );
        assert!(
            matches!(declared.original.as_ref().unwrap(), OriginalProjection::Exact(r) if r.text() == "TRIGGER_F")
        );
        let targets = document
            .occurrences
            .iter()
            .filter(|o| o.query.data_key().value() == "trigger_tables")
            .collect::<Vec<_>>();
        assert_eq!(targets.len(), 1, "native trigger target count");
        assert_eq!(*targets[0].query.r#type().value(), QueryKind::Relation);
        assert_eq!(targets[0].query.relation_type().value(), "REFERENCES");
        assert_eq!(text(&targets[0].name), "TABLE_F");
        let name = present(&targets[0].name);
        assert_eq!(
            document.syntax.nodes[name.node_index].kind,
            "table_or_view_name"
        );
        assert!(
            matches!(name.original.as_ref().unwrap(), OriginalProjection::Exact(r) if r.text() == "TABLE_F")
        );
        assert_eq!(
            document
                .occurrences
                .iter()
                .filter(|o| o.query.graph_label().value() == "Trigger")
                .count(),
            1
        );
        assert!(
            document
                .occurrences
                .iter()
                .all(|o| o.query.graph_label().value() != "Table")
        );
    });
}

#[test]
fn db2_type_qname_boundaries_native() {
    let catalog = catalog(include_bytes!("fixtures/catalog/shipped/db2.yaml"));
    let profile = &catalog.profiles()[0];
    let mut gaps = Vec::new();
    for (source, names) in [
        (
            "CREATE TYPE APP.ARRAY_F AS INTEGER ARRAY[];",
            vec!["APP.ARRAY_F"],
        ),
        (
            "CREATE TYPE APP.CURSOR_F AS APP.ROW_F CURSOR;",
            vec!["APP.CURSOR_F"],
        ),
        (
            "CREATE TYPE APP.DISTINCT_F AS INTEGER;",
            vec!["APP.DISTINCT_F"],
        ),
        (
            "CREATE TYPE APP.ROW_F AS ROW(FIELD_F INTEGER);",
            vec!["APP.ROW_F"],
        ),
        (
            "CREATE TYPE APP.TYPE_F AS(ATTRIBUTE_F INTEGER) NOT FINAL MODE DB2SQL;",
            vec!["APP.TYPE_F"],
        ),
        (
            "CREATE TYPE APP.CURSOR_F AS APP.MOD_F.ROW_F CURSOR;",
            vec!["APP.CURSOR_F"],
        ),
        (
            "CREATE TYPE \"S.é\".\"T\"\"F\" AS INTEGER ARRAY[];",
            vec!["\"S.é\".\"T\"\"F\""],
        ),
    ] {
        with_identity(source, &context(), |input| {
            let document = parse_semantic_occurrences(profile, input).unwrap();
            if !document.syntax.diagnostics.is_empty() {
                gaps.push(format!("{source}: {:?}", document.syntax.diagnostics));
                return;
            }
            let types = document
                .occurrences
                .iter()
                .filter(|o| o.query.data_key().value() == "types")
                .collect::<Vec<_>>();
            if types.len() != names.len() {
                gaps.push(format!("{source}: Type count {}", types.len()));
                return;
            }
            for (entity, name) in types.iter().zip(names) {
                assert_eq!(text(&entity.name), name);
                let selected = present(&entity.name);
                assert!(matches!(
                    document.syntax.nodes[selected.node_index].kind.as_str(),
                    "type_name" | "distinct_type_name"
                ));
                assert!(
                    matches!(selected.original.as_ref().unwrap(), OriginalProjection::Exact(r) if r.text() == name)
                );
                let name_node = selected.node_index;
                let ids = document
                    .syntax
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| {
                        n.kind == "id_" && {
                            let mut ancestor = n.parent;
                            let mut within = false;
                            while let Some(a) = ancestor {
                                if a == name_node {
                                    within = true;
                                    break;
                                }
                                ancestor = document.syntax.nodes[a].parent;
                            }
                            within
                        }
                    })
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                assert_eq!(ids.len(), 2);
                assert_eq!(
                    Some(present(&document.contexts[entity.contexts[0]].name).node_index),
                    Some(name_node)
                );
            }
        });
    }
    for source in [
        "CREATE METHOD METHOD_F(P_F APP.PARAM_T) RETURNS INTEGER FOR APP.TYPE_F EXTERNAL NAME 'pkg.method';",
        "ALTER TYPE APP.TYPE_F ADD METHOD METHOD_F() RETURNS INTEGER LANGUAGE SQL;",
        "BEGIN ATOMIC DECLARE V_F APP.MOD_F.ROW_F; END;",
        "BEGIN ATOMIC DECLARE V_F APP.MOD_F.ARRAY_F; END;",
        "CREATE TYPE APP.CURSOR_F AS APP.MOD_F.ROW_F CURSOR;",
        "ALTER MODULE MOD_F DROP TYPE TYPE_F;",
    ] {
        let raw = parse_profile_queries(Dialect::Db2, source, &[], &context()).unwrap();
        if !raw.syntax.diagnostics.is_empty() {
            gaps.push(format!("{source}: {:?}", raw.syntax.diagnostics));
        }
    }
    assert!(
        gaps.is_empty(),
        "native type QName gaps:\n{}",
        gaps.join("\n")
    );
    for source in [
        "CREATE TYPE APP. AS INTEGER ARRAY[];",
        "CREATE TYPE APP..TYPE_F AS INTEGER ARRAY[];",
        "CREATE TYPE APP.MOD_F.TYPE_F AS INTEGER ARRAY[];",
        "CREATE TYPE CURSOR_F AS APP.MOD_F.EXTRA_F.ROW_F CURSOR;",
        "CREATE METHOD METHOD_F(P_F APP.MOD_F.PARAM_T) RETURNS INTEGER FOR APP.TYPE_F EXTERNAL NAME 'pkg.method';",
        "ALTER MODULE MOD_F DROP TYPE APP.TYPE_F;",
    ] {
        let raw = parse_profile_queries(Dialect::Db2, source, &[], &context()).unwrap();
        assert!(!raw.syntax.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                graphit_ast::antlr_engine::parse_strict(Dialect::Db2, source, &context()),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
    }
}

#[test]
fn antlr_rejects_tree_sitter_context_settings_and_removed_scalar_capture_configuration() {
    for (field, body) in [
        ("context_name_queries", "context_name_queries: {}\n"),
        ("context_types", "context_types: {}\n"),
    ] {
        let catalog = profile("antlr-postgresql", body);
        with_identity("SELECT 1;", &context(), |input| {
            assert!(
                matches!(parse_semantic_occurrences(&catalog.profiles()[0], input),
                Err(SemanticError::UnsupportedProfileField { query_index: None, field: actual }) if actual == field)
            );
        });
    }
    for (field, companion) in [
        ("name_capture", ""),
        ("value_capture", ", value_label: Value"),
        ("span_capture", ""),
        ("parent_capture", ", parent_label: Scope"),
        ("qualifier_capture", ""),
        ("declaration_capture", ""),
        ("context_capture", ""),
    ] {
        let catalog = profile(
            "antlr-postgresql",
            &format!(
                "queries: [{{data_key: script, graph_label: File, pattern: '//root', {field}: 'name'{companion}, type: entity}}]\n"
            ),
        );
        with_identity("SELECT 1;", &context(), |input| {
            assert!(
                matches!(parse_semantic_occurrences(&catalog.profiles()[0], input),
                Err(SemanticError::UnsupportedProfileField { query_index: Some(0), field: actual }) if actual == field)
            );
        });
    }
}
