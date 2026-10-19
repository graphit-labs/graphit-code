use graphit_ast::{
    antlr_engine::model::{
        AntlrEntityExtent, AntlrIssue, AntlrModelError, AntlrProjection, AntlrTypeObservation,
        AntlrValueObservation, ParsedAntlrDocument,
    },
    catalog::{
        CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind,
        ProvenanceView,
    },
    prepared_input::{
        ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput, SyntheticReason,
    },
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
    semantic_fields::ContributionConflictField,
    symbols::{IdentityLimits, IdentityLimitsInput},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use sqlparser::tokenizer::Token;
use std::{error::Error, io::ErrorKind, time::Instant};
type TestResult = Result<(), Box<dyn Error>>;
fn context() -> SnapshotContext {
    SnapshotContext::service(Cancellation::default())
}

fn limits() -> IdentityLimits {
    IdentityLimits::new(IdentityLimitsInput {
        source_bytes: 1 << 20,
        path_components: 32,
        path_component_bytes: 4096,
        identity_text_bytes: 4096,
        owner_depth: 64,
        uid_bytes: 32768,
    })
    .unwrap()
}

fn catalog(bytes: &[u8]) -> CatalogDraft {
    let original = context();
    let id = ProfileId::new("semantic-json-fixture", 256).unwrap();
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

fn with_input(
    text: &str,
    original: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>) -> TestResult,
) -> TestResult {
    let prepared = text.to_owned();
    assert_ne!(text.as_ptr(), prepared.as_ptr());
    let span = ByteSpan {
        start: 0,
        end: u64::try_from(text.len())?,
    };
    let segments = [OriginSegment::Direct {
        prepared: span,
        original: span,
    }];
    let input = PreparedInput::new(
        text.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        original,
    )?;
    action(&input)
}

fn custom(queries: &str) -> CatalogDraft {
    catalog(format!("language: plsql\nparser: antlr4\ngrammar: antlr-plsql\nembed_labels: [Table,Column,Variable,Constant]\nantlr_contexts: [{{pattern: '//create_table', label: Table}}]\nqueries:\n{queries}").as_bytes())
}

#[test]
fn stock_oracle_table_columns_have_native_table_owners_and_full_extents() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let original = context();
    let source =
        r#"CREATE TABLE "ACME"."PEDIDO" ("ID_PEDIDO" NUMBER,"DT_PEDIDO" DATE,"VL_TOTAL" NUMBER);"#;
    with_input(source, &original, |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["src", "pedido.sql"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        assert!(
            model.diagnostics().is_empty(),
            "{:?}",
            model
                .diagnostics()
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        assert_eq!(model.entities().len(), 4);
        let table = model
            .entities()
            .iter()
            .position(|e| e.identity().label() == "Table")
            .unwrap();
        assert_eq!(model.entities()[table].name(), "PEDIDO");
        assert!(model.entities()[table].namespace().is_empty());
        for entity in model.entities() {
            let AntlrEntityExtent::Exact(extent) = entity.extent() else {
                panic!("full original declaration")
            };
            assert_eq!(
                entity.name_raw().text().as_ptr(),
                source[entity.name_raw().start()..].as_ptr()
            );
            assert_eq!(
                entity.identity().subject().start(),
                entity.name_raw().start()
            );
            if entity.identity().label() == "Column" {
                assert_eq!(
                    entity.contributions().len(),
                    2,
                    "stock name and value descriptors merge"
                );
                assert_eq!(entity.namespace().len(), 1);
                let owner = &entity.namespace()[0];
                assert_eq!(owner.name(), "PEDIDO");
                assert_eq!(
                    owner.identity().uid().as_str(),
                    model.entities()[table].identity().uid().as_str()
                );
                assert_eq!(
                    parsed.observations().syntax.nodes[owner.node_index()].kind,
                    "create_table"
                );
                assert!(extent.range().text().contains(entity.name_raw().text()));
                assert!(!extent.range().text().contains(','));
                assert!(
                    entity
                        .contributions()
                        .iter()
                        .all(|c| matches!(c.declared_type, AntlrTypeObservation::Present { .. }))
                );
            } else {
                assert_eq!(extent.range().text(), source.strip_suffix(';').unwrap());
            }
            for contribution in entity.contributions() {
                assert!(std::ptr::eq(
                    contribution.occurrence.query,
                    draft.profiles()[0].queries().value().as_slice()
                        [contribution.occurrence.query_index]
                        .value()
                ));
                assert!(matches!(
                    contribution.occurrence.query.graph_label().provenance(),
                    ProvenanceView::Declared(_)
                ));
            }
        }
        assert_eq!(model.owner_links().len(), 3);
        assert!(
            model
                .owner_links()
                .iter()
                .all(|link| link.owner_entity_indices == [table])
        );
        let projections = model.code_projections()?;
        assert_eq!(projections.len(), 4);
        assert!(
            projections
                .iter()
                .all(|p| matches!(p, AntlrProjection::Available(_)))
        );
        Ok(())
    })
}

#[test]
fn stock_defaults_and_variable_constants_merge_without_losing_values() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    with_input(
        "CREATE TABLE pedido(id NUMBER DEFAULT 1,status VARCHAR2(20) DEFAULT 'ABERTO',absent NUMBER); DECLARE v NUMBER := 1; c CONSTANT VARCHAR2(100) := 'a''b'; BEGIN NULL; END; /",
        &context(),
        |input| {
            let parsed =
                ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["x.sql"], limits())?;
            let model = parsed.assemble()?;
            assert!(
                model.diagnostics().is_empty(),
                "{:?}",
                model
                    .diagnostics()
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>()
            );
            for (name, expected) in [("id", "1"), ("status", "ABERTO"), ("v", "1"), ("c", "a'b")] {
                let entity = model.entities().iter().find(|e| e.name() == name).unwrap();
                assert_eq!(entity.contributions().len(), 2);
                assert!(entity.conflicts().is_empty());
                let value = entity
                    .contributions()
                    .iter()
                    .find_map(|c| match &c.declared_value {
                        AntlrValueObservation::Scalar { token, .. } => Some(token),
                        _ => None,
                    })
                    .unwrap();
                match value {
                    Token::Number(value, false) | Token::SingleQuotedString(value) => {
                        assert_eq!(value, expected)
                    }
                    _ => panic!("typed scalar"),
                };
                if matches!(name, "v" | "c") {
                    let AntlrEntityExtent::Exact(extent) = entity.extent() else {
                        panic!("variable extent")
                    };
                    assert!(extent.range().text().ends_with(';'));
                }
            }
            let absent = model
                .entities()
                .iter()
                .find(|e| e.name() == "absent")
                .unwrap();
            assert!(
                absent
                    .contributions()
                    .iter()
                    .any(|c| matches!(c.declared_value, AntlrValueObservation::Missing))
            );
            assert!(
                absent
                    .contributions()
                    .iter()
                    .any(|c| matches!(c.declared_value, AntlrValueObservation::NotConfigured))
            );
            Ok(())
        },
    )
}

#[test]
fn actual_identifier_codec_keeps_quotes_unicode_and_exact_numbers_expressions() -> TestResult {
    let draft = custom(
        "  - {data_key: variables, graph_label: Variable, pattern: '//variable_declaration', value_label: Value, type: entity, captures: {name: [{xpath: '/identifier'}], value: [{xpath: '/default_value_part/expression'}]}}\n",
    );
    let long = "x".repeat(20000);
    let source = format!(
        "DECLARE \"A\"\"Ω\" NUMBER := 1000000000000000000000000000000000000000000000000000000000; empty VARCHAR2(1) := ''; s VARCHAR2(32767) := '{long}\nline'; expr NUMBER := 1+2; BEGIN NULL; END; /"
    );
    with_input(&source, &context(), |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["values.sql"], limits())?;
        let model = parsed.assemble()?;
        assert!(
            model.diagnostics().is_empty(),
            "{:?}",
            model
                .diagnostics()
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        let quoted = model
            .entities()
            .iter()
            .find(|e| e.name() == "A\"Ω")
            .unwrap();
        assert_eq!(quoted.name_raw().text(), "\"A\"\"Ω\"");
        assert!(
            matches!(&quoted.contributions()[0].declared_value,AntlrValueObservation::Scalar{token:Token::Number(n,false),..} if n.len()>50)
        );
        let empty = model
            .entities()
            .iter()
            .find(|e| e.name() == "empty")
            .unwrap();
        assert!(
            matches!(&empty.contributions()[0].declared_value,AntlrValueObservation::Scalar{token:Token::SingleQuotedString(s),..} if s.is_empty())
        );
        let text = model.entities().iter().find(|e| e.name() == "s").unwrap();
        assert!(
            matches!(&text.contributions()[0].declared_value,AntlrValueObservation::Scalar{raw,token:Token::SingleQuotedString(s)} if raw.text().len()>20000 && s.ends_with("\nline"))
        );
        let expr = model
            .entities()
            .iter()
            .find(|e| e.name() == "expr")
            .unwrap();
        assert!(
            matches!(&expr.contributions()[0].declared_value,AntlrValueObservation::Expression{raw} if raw.text()=="1+2")
        );
        Ok(())
    })
}

#[test]
fn genuine_identity_data_keys_homonyms_paths_and_labels_remain_distinct() -> TestResult {
    let draft = custom(
        "  - {data_key: variables, graph_label: Variable, pattern: '//variable_declaration', type: entity, captures: {name: [{xpath: '/identifier'}]}}\n  - {data_key: custom, graph_label: Variable, pattern: '//variable_declaration', type: entity, captures: {name: [{xpath: '/identifier'}]}}\n  - {data_key: constants, graph_label: Constant, pattern: '//variable_declaration', type: entity, captures: {name: [{xpath: '/identifier'}]}}\n",
    );
    with_input(
        "DECLARE v NUMBER; BEGIN NULL; END; / DECLARE v NUMBER; BEGIN NULL; END; /",
        &context(),
        |input| {
            let left =
                ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["left.sql"], limits())?;
            let right = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["right.sql"],
                limits(),
            )?;
            let model = left.assemble()?;
            let other = right.assemble()?;
            assert_eq!(model.entities().len(), 6);
            let variables: Vec<_> = model
                .entities()
                .iter()
                .filter(|e| e.data_key() == "variables")
                .collect();
            let custom: Vec<_> = model
                .entities()
                .iter()
                .filter(|e| e.data_key() == "custom")
                .collect();
            assert_eq!(
                variables[0].identity().uid().as_str(),
                custom[0].identity().uid().as_str()
            );
            assert_ne!(
                variables[0].identity().uid().as_str(),
                variables[1].identity().uid().as_str()
            );
            assert_ne!(
                model.entities()[0].identity().uid().as_str(),
                other.entities()[0].identity().uid().as_str()
            );
            assert_ne!(
                variables[0].identity().uid().as_str(),
                model
                    .entities()
                    .iter()
                    .find(|e| e.data_key() == "constants")
                    .unwrap()
                    .identity()
                    .uid()
                    .as_str()
            );
            Ok(())
        },
    )
}

#[test]
fn conflicts_invalid_owners_and_spans_block_projection_in_both_query_orders() -> TestResult {
    let good = "  - {data_key: variables, graph_label: Variable, pattern: '//variable_declaration', value_label: Value, type: entity, captures: {name: [{xpath: '/identifier'}], value: [{xpath: '/default_value_part/expression'}]}}\n";
    for bad in [
        "  - {data_key: variables, graph_label: Variable, pattern: '//variable_declaration', value_label: Value, type: entity, captures: {name: [{xpath: '/identifier'}], value: [{xpath: '/identifier'}]}}\n",
        "  - {data_key: variables, graph_label: Variable, pattern: '//variable_declaration', parent_label: Table, type: entity, captures: {name: [{xpath: '/identifier'}], parent: [{xpath: '/identifier'}]}}\n",
        "  - {data_key: variables, graph_label: Variable, pattern: '//variable_declaration', type: entity, captures: {name: [{xpath: '/identifier'}], span: [{xpath: '/identifier'}]}}\n",
    ] {
        for queries in [format!("{good}{bad}"), format!("{bad}{good}")] {
            let draft = custom(&queries);
            with_input(
                "DECLARE v NUMBER := 1; BEGIN NULL; END; /",
                &context(),
                |input| {
                    let parsed = ParsedAntlrDocument::collect(
                        &draft.profiles()[0],
                        input,
                        &["x.sql"],
                        limits(),
                    )?;
                    let model = parsed.assemble()?;
                    assert_eq!(model.entities().len(), 1);
                    assert!(
                        model
                            .code_projections()?
                            .iter()
                            .all(|p| matches!(p, AntlrProjection::Unavailable(_)))
                    );
                    assert!(
                        !model.entities()[0].conflicts().is_empty()
                            || !model.entities()[0].diagnostic_indices().is_empty()
                    );
                    if bad.contains("value:") {
                        assert_eq!(
                            model.entities()[0].conflicts()[0].field,
                            ContributionConflictField::Value
                        );
                    }
                    Ok(())
                },
            )?;
        }
    }
    Ok(())
}

#[test]
fn unsupported_inputs_declarations_rejections_and_relations_are_observable() -> TestResult {
    for grammar in ["antlr-postgresql", "antlr-tsql", "antlr-cobol85"] {
        let draft = catalog(
            format!("language: fixture\nparser: antlr4\ngrammar: {grammar}\nqueries: []\n")
                .as_bytes(),
        );
        with_input("not a valid source", &context(), |input| {
            assert!(matches!(
                ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["x"], limits()),
                Err(AntlrModelError::UnsupportedProfile)
            ));
            Ok(())
        })?;
    }
    let draft = custom(
        "  - {data_key: rejected, graph_label: Variable, pattern: '//variable_declaration', name_reject: '^v$', type: entity, captures: {name: [{xpath: '/identifier'}]}}\n  - {data_key: refs, pattern: '//variable_declaration/identifier', relation_type: REFERENCES, target_labels: [Variable], type: relation}\n  - {data_key: blocks, graph_label: Function, pattern: '//anonymous_block', type: entity}\n",
    );
    with_input(
        "DECLARE v NUMBER; BEGIN NULL; END; /",
        &context(),
        |input| {
            let parsed =
                ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["x.sql"], limits())?;
            let model = parsed.assemble()?;
            assert!(model.entities().is_empty());
            assert!(
                model
                    .diagnostics()
                    .iter()
                    .any(|d| matches!(d.issue, AntlrIssue::ExcludedName))
            );
            assert!(
                model
                    .diagnostics()
                    .iter()
                    .any(|d| matches!(d.issue, AntlrIssue::UnsupportedDeclaration))
            );
            assert_eq!(model.unresolved_relations().len(), 1);
            let relation = &parsed.observations().occurrences[model.unresolved_relations()[0]];
            assert_eq!(relation.query.relation_type().value(), "REFERENCES");
            assert_eq!(
                relation.query.target_labels().value().as_slice()[0].value(),
                "Variable"
            );
            Ok(())
        },
    )
}

#[test]
fn nonexact_extent_keeps_exact_identity_but_never_available_code() -> TestResult {
    let draft = custom(
        "  - {data_key: tables, graph_label: Table, pattern: '//create_table/table_name', type: entity}\n",
    );
    let original = context();
    let source = "CREATE TABLE pedido(id NUMBER);";
    let name = source.find("pedido").unwrap();
    let pieces = [
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: 0,
                end: name as u64,
            },
            reason: SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: name as u64,
                end: (name + 6) as u64,
            },
            original: ByteSpan {
                start: name as u64,
                end: (name + 6) as u64,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: (name + 6) as u64,
                end: source.len() as u64,
            },
            reason: SyntheticReason::Wrapper,
        },
    ];
    let removed = [
        ByteSpan {
            start: 0,
            end: name as u64,
        },
        ByteSpan {
            start: (name + 6) as u64,
            end: source.len() as u64,
        },
    ];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &pieces,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed = ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["x.sql"], limits())?;
    let model = parsed.assemble()?;
    assert_eq!(model.entities().len(), 1);
    assert!(matches!(
        model.entities()[0].extent(),
        AntlrEntityExtent::NonExact(OriginalProjection::Compound(_))
    ));
    assert!(matches!(
        model.code_projections()?.as_slice(),
        [AntlrProjection::Unavailable(_)]
    ));
    assert_eq!(model.entities()[0].name_raw().text(), "pedido");
    Ok(())
}

#[test]
fn recovery_and_actual_original_controls_prevent_partial_delivery() -> TestResult {
    let draft = custom(
        "  - {data_key: variables, graph_label: Variable, pattern: '//variable_declaration', type: entity, captures: {name: [{xpath: '/identifier'}]}}\n",
    );
    with_input(
        "DECLARE v NUMBER := ; BEGIN NULL; END; /",
        &context(),
        |input| {
            let parsed =
                ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["x.sql"], limits())?;
            let model = parsed.assemble()?;
            assert!(!parsed.observations().syntax.diagnostics.is_empty());
            assert!(
                model
                    .diagnostics()
                    .iter()
                    .any(|d| matches!(d.issue, AntlrIssue::Recovery { .. }))
            );
            assert!(
                model
                    .code_projections()?
                    .iter()
                    .all(|p| matches!(p, AntlrProjection::Unavailable(_)))
            );
            Ok(())
        },
    )?;
    let original = context();
    with_input("DECLARE v NUMBER; BEGIN NULL; END; /", &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["x.sql"], limits())?;
        let model = parsed.assemble()?;
        original.cancellation().cancel();
        assert!(
            matches!(model.code_projections(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(parsed.assemble(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        assert!(
            matches!(ParsedAntlrDocument::collect(&draft.profiles()[0],input,&["x.sql"],limits()),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    let original = SnapshotContext::new(Instant::now(), Cancellation::default());
    // The actual prepared input itself rejects this expired operation.
    assert!(
        PreparedInput::new(
            b"x",
            b"x",
            OriginPlan {
                segments: &[OriginSegment::Direct {
                    prepared: ByteSpan { start: 0, end: 1 },
                    original: ByteSpan { start: 0, end: 1 }
                }],
                removed_original: &[],
                anchors: &[]
            },
            &original
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn actual_original_expiry_is_checked_after_collection() -> TestResult {
    let draft = custom(
        "  - {data_key: variables, graph_label: Variable, pattern: '//variable_declaration', type: entity, captures: {name: [{xpath: '/identifier'}]}}\n",
    );
    let deadline = Instant::now() + std::time::Duration::from_secs(1);
    let original = SnapshotContext::new(deadline, Cancellation::default());
    with_input("DECLARE v NUMBER; BEGIN NULL; END; /", &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["x.sql"], limits())?;
        let model = parsed.assemble()?;
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        assert!(
            matches!(model.code_projections(), Err(AntlrModelError::Operation(e)) if e.kind() == ErrorKind::TimedOut)
        );
        assert!(
            matches!(parsed.assemble(), Err(AntlrModelError::Operation(e)) if e.kind() == ErrorKind::TimedOut)
        );
        Ok(())
    })
}

#[test]
fn oracle_callable_native_headers_owners_and_qnames() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let original = context();
    let source = r#"CREATE OR REPLACE FUNCTION "Ω"."F""Q" RETURN NUMBER IS BEGIN RETURN 1; END;
/
CREATE OR REPLACE PROCEDURE "Ω"."P" IS BEGIN NULL; END;
/
CREATE PACKAGE "Ω"."PKG" AS
 FUNCTION f RETURN NUMBER;
 PROCEDURE p;
END "PKG";
/
CREATE PACKAGE BODY "Ω"."PKG" AS
 FUNCTION f RETURN NUMBER IS v NUMBER := 1; BEGIN RETURN v; END;
 PROCEDURE p IS
  FUNCTION inner_f RETURN NUMBER IS BEGIN RETURN 2; END;
 BEGIN NULL; END;
END "PKG";
/"#;
    with_input(source, &original, |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["callables.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        assert!(
            model
                .diagnostics()
                .iter()
                .all(|d| matches!(d.issue, AntlrIssue::DeclarationReference { .. })),
            "{:?}",
            model
                .diagnostics()
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        assert_eq!(model.diagnostics().len(), 2, "two END package references");
        assert_eq!(model.entities().len(), 10);
        let mut kinds = std::collections::BTreeSet::new();
        for entity in model.entities() {
            let kind = parsed.observations().syntax.nodes[entity.declaration_node_index()]
                .kind
                .as_str();
            kinds.insert(kind);
            assert!(matches!(entity.extent(), AntlrEntityExtent::Exact(_)));
            let AntlrEntityExtent::Exact(extent) = entity.extent() else {
                unreachable!()
            };
            assert!(extent.range().start() <= entity.name_raw().start());
            assert!(extent.range().end() >= entity.name_raw().end());
            assert_eq!(
                entity.name_raw().text().as_ptr(),
                source[entity.name_raw().start()..].as_ptr()
            );
            if entity.identity().label() != "Variable" {
                let parts = entity.qualified_name().unwrap().components();
                assert_eq!(
                    (
                        parts.last().unwrap().raw.start(),
                        parts.last().unwrap().raw.end(),
                        parts.last().unwrap().raw.text()
                    ),
                    (
                        entity.name_raw().start(),
                        entity.name_raw().end(),
                        entity.name_raw().text()
                    )
                );
                assert_eq!(parts.last().unwrap().decoded.as_ref(), entity.name());
                assert_eq!(
                    parsed.observations().syntax.nodes[parts.last().unwrap().node_index].kind,
                    "id_expression"
                );
                assert!(parts.windows(2).all(|w| w[0].raw.end() < w[1].raw.start()));
            }
            assert!(
                entity
                    .namespace()
                    .iter()
                    .all(|n| n.identity().label() != "Schema")
            );
            if entity.name() == "F\"Q" || entity.name() == "P" || entity.name() == "PKG" {
                let parts = entity.qualified_name().unwrap().components();
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0].decoded.as_ref(), "Ω");
                assert!(parts.iter().all(|p| p.quote_style == Some('"')));
                assert!(entity.namespace().is_empty());
            }
            let owners: Vec<_> = entity.namespace().iter().map(|n| n.name()).collect();
            if entity.name() == "inner_f" {
                assert_eq!(owners, ["PKG", "p"]);
            }
            if entity.name() == "v" {
                assert_eq!(owners, ["PKG", "f"]);
            }
            if entity.name() == "f" || entity.name() == "p" {
                assert_eq!(owners, ["PKG"]);
            }
            if entity.identity().label() == "Function" {
                assert!(entity.contributions().iter().all(|c| {
                    c.type_roles
                        .iter()
                        .any(|t| t.role == graphit_ast::antlr_engine::model::AntlrTypeRole::Return)
                }));
            }
        }
        for expected in [
            "create_function_body",
            "create_procedure_body",
            "function_spec",
            "procedure_spec",
            "function_body",
            "procedure_body",
            "create_package",
            "create_package_body",
        ] {
            assert!(kinds.contains(expected), "{expected}");
        }
        assert!(
            model
                .code_projections()?
                .iter()
                .all(|p| matches!(p, AntlrProjection::Available(_)))
        );
        let packages: Vec<_> = model
            .entities()
            .iter()
            .filter(|e| e.identity().label() == "Package")
            .collect();
        assert_eq!(packages.len(), 2);
        assert_ne!(
            packages[0].identity().uid().as_str(),
            packages[1].identity().uid().as_str()
        );
        Ok(())
    })
}

#[test]
fn oracle_callable_duplicate_queries_keep_native_identity_and_capture() -> TestResult {
    let query = "  - {data_key: first, graph_label: Function, pattern: '//create_function_body/function_name', type: entity}\n";
    let other = query.replace("first", "second");
    let source = r#"CREATE FUNCTION "Ω"."F" RETURN NUMBER IS BEGIN RETURN 1; END; /"#;
    let mut runs = Vec::new();
    for queries in [
        format!("{query}{query}{other}"),
        format!("{other}{query}{query}"),
    ] {
        let draft = catalog(format!("language: plsql\nparser: antlr4\ngrammar: antlr-plsql\nembed_labels: [Function]\nqueries:\n{queries}").as_bytes());
        let original = context();
        with_input(source, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["function.sql"],
                limits(),
            )?;
            assert!(parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            assert!(model.diagnostics().is_empty());
            assert_eq!(model.entities().len(), 2);
            assert_eq!(
                model.entities()[0].identity().uid().as_str(),
                model.entities()[1].identity().uid().as_str()
            );
            let mut values = Vec::new();
            for entity in model.entities() {
                assert_eq!(entity.name_raw().text(), "\"F\"");
                assert_eq!(
                    entity.contributions().len(),
                    if entity.data_key() == "first" { 2 } else { 1 }
                );
                for c in entity.contributions() {
                    let graphit_ast::antlr_engine::semantic::CaptureBinding::Present(name) =
                        &c.occurrence.name
                    else {
                        panic!("name")
                    };
                    let Some(OriginalProjection::Exact(raw)) = name.original else {
                        panic!("origin")
                    };
                    assert_eq!(raw.text(), "\"Ω\".\"F\"");
                    assert_ne!(raw.start(), entity.name_raw().start());
                }
                values.push((
                    entity.data_key().to_owned(),
                    entity.identity().uid().as_str().to_owned(),
                ));
            }
            values.sort();
            runs.push(values);
            Ok(())
        })?;
    }
    assert_eq!(runs[0], runs[1]);
    Ok(())
}

#[test]
fn oracle_callable_nonexact_component_never_becomes_an_identity() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "CREATE FUNCTION f RETURN NUMBER IS BEGIN RETURN 1; END; /";
    let original = context();
    let start = source.find(" f ").unwrap() + 1;
    let end = start + 1;
    let pieces = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: start as u64,
            },
            original: ByteSpan {
                start: 0,
                end: start as u64,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: start as u64,
                end: end as u64,
            },
            reason: SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: end as u64,
                end: source.len() as u64,
            },
            original: ByteSpan {
                start: end as u64,
                end: source.len() as u64,
            },
        },
    ];
    let removed = [ByteSpan {
        start: start as u64,
        end: end as u64,
    }];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &pieces,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed =
        ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["synthetic.sql"], limits())?;
    assert!(parsed.observations().syntax.diagnostics.is_empty());
    let model = parsed.assemble()?;
    assert!(model.entities().is_empty());
    assert!(
        model
            .diagnostics()
            .iter()
            .any(|d| matches!(d.issue, AntlrIssue::NonExact { role: "name", .. }))
    );
    Ok(())
}

#[test]
fn oracle_callable_recovery_and_original_guards_remain_typed() -> TestResult {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    with_input(
        "CREATE FUNCTION f RETURN NUMBER IS BEGIN RETURN ( ; END; /",
        &context(),
        |input| {
            let parsed =
                ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["bad.sql"], limits())?;
            assert!(!parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            assert!(
                model
                    .diagnostics()
                    .iter()
                    .any(|d| matches!(d.issue, AntlrIssue::Recovery { .. }))
            );
            assert!(
                model
                    .code_projections()?
                    .iter()
                    .all(|p| matches!(p, AntlrProjection::Unavailable(_)))
            );
            Ok(())
        },
    )?;
    let source = "CREATE FUNCTION f RETURN NUMBER IS BEGIN RETURN 1; END; /";
    let original = context();
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["cancel.sql"], limits())?;
        let model = parsed.assemble()?;
        original.cancellation().cancel();
        assert!(
            matches!(model.code_projections(), Err(AntlrModelError::Operation(e)) if e.kind() == ErrorKind::Interrupted)
        );
        assert!(
            matches!(parsed.assemble(), Err(AntlrModelError::Operation(e)) if e.kind() == ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    let deadline = Instant::now() + std::time::Duration::from_secs(1);
    let original = SnapshotContext::new(deadline, Cancellation::default());
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["deadline.sql"], limits())?;
        let model = parsed.assemble()?;
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        assert!(
            matches!(model.code_projections(), Err(AntlrModelError::Operation(e)) if e.kind() == ErrorKind::TimedOut)
        );
        assert!(
            matches!(parsed.assemble(), Err(AntlrModelError::Operation(e)) if e.kind() == ErrorKind::TimedOut)
        );
        Ok(())
    })
}

#[test]
fn oracle_typed_declarations_preserve_native_shapes_modes_and_sources() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleDeclaration as D, AntlrOracleExpression as E, AntlrOracleTypeKind as K,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = r#"CREATE PROCEDURE "Ω"."p"(a IN NUMBER,b OUT NOCOPY NUMBER,c IN OUT NUMBER,d NUMBER DEFAULT 1,e INOUT NUMBER,"κ""q" in in out nocopy nocopy NUMBER DEFAULT fn(1,2)) IS
 TYPE r_t IS RECORD(id NUMBER NOT NULL DEFAULT 2);
 TYPE t_t IS TABLE OF NUMBER NOT NULL;
 TYPE a_t IS TABLE OF NUMBER INDEX BY VARCHAR2(10);
 TYPE v_t IS VARRAY(10+1) OF NUMBER NOT NULL;
 TYPE c_t IS REF CURSOR RETURN r_t;
 comum NUMBER;
BEGIN NULL; END;
/"#;
    let original = context();
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["typed.sql"], limits())?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        assert!(
            model.diagnostics().is_empty(),
            "{:?}",
            model
                .diagnostics()
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        assert_eq!(model.entities().len(), 13);
        let parameters: Vec<_> = model
            .entities()
            .iter()
            .filter(|e| e.identity().label() == "Parameter")
            .collect();
        assert_eq!(parameters.len(), 6);
        for entity in &parameters {
            assert_eq!(
                entity
                    .namespace()
                    .iter()
                    .map(|o| o.name())
                    .collect::<Vec<_>>(),
                ["p"]
            );
            let AntlrEntityExtent::Exact(extent) = entity.extent() else {
                panic!("individual extent")
            };
            assert!(!extent.range().text().starts_with("CREATE"));
            assert!(matches!(
                entity.contributions()[0].declared_type,
                AntlrTypeObservation::Present { .. }
            ));
            assert!(matches!(
                entity.contributions()[0].declared_value,
                AntlrValueObservation::NotConfigured
            ));
            let Some(D::Parameter {
                modes,
                default_value,
            }) = entity.oracle_declaration()
            else {
                panic!("parameter facts")
            };
            let kinds: Vec<_> = modes.iter().map(|m| m.node.kind.as_str()).collect();
            let expected: &[&str] = match entity.name() {
                "a" => &["IN"],
                "b" => &["OUT", "NOCOPY"],
                "c" => &["IN", "OUT"],
                "d" => &[],
                "e" => &["INOUT"],
                _ => &["IN", "IN", "OUT", "NOCOPY", "NOCOPY"],
            };
            assert_eq!(kinds, expected);
            for m in modes {
                assert_eq!(m.node.class, graphit_ast::antlr_engine::NodeClass::Terminal);
                let Some(OriginalProjection::Exact(raw)) = m.original else {
                    panic!("exact mode")
                };
                assert_eq!(raw.text().as_ptr(), source[raw.start()..].as_ptr());
            }
            match (entity.name(), default_value) {
                (
                    "d",
                    Some(E::Present {
                        node_index,
                        node,
                        original: Some(OriginalProjection::Exact(raw)),
                    }),
                ) => {
                    assert_eq!(raw.text(), "1");
                    assert_eq!(node.kind, "expression");
                    assert_eq!(
                        parsed.observations().syntax.nodes[*node_index].kind,
                        "expression"
                    );
                }
                (
                    "κ\"q",
                    Some(E::Present {
                        original: Some(OriginalProjection::Exact(raw)),
                        ..
                    }),
                ) => assert_eq!(raw.text(), "fn(1,2)"),
                ("d" | "κ\"q", _) => panic!("missing default"),
                (_, None) => {}
                _ => panic!("invented default"),
            }
        }
        let types: Vec<_> = model
            .entities()
            .iter()
            .filter(|e| e.identity().label() == "Type")
            .collect();
        assert_eq!(types.len(), 5);
        for entity in types {
            let Some(D::Type(details)) = entity.oracle_declaration() else {
                panic!("type facts")
            };
            let graphit_ast::antlr_engine::model::AntlrOracleTypeDeclaration {
                kind,
                definition,
                element_type,
                index_type,
                bound,
                modifiers,
            } = details.as_ref();
            let expected = match entity.name() {
                "r_t" => K::Record,
                "t_t" => K::NestedTable,
                "a_t" => K::AssociativeArray,
                "v_t" => K::Varray,
                "c_t" => K::RefCursor,
                _ => panic!("invented type"),
            };
            assert_eq!(*kind, expected);
            let AntlrTypeObservation::Present { node_index, raw } = definition else {
                panic!("type body")
            };
            assert!(
                parsed.observations().syntax.nodes[*node_index]
                    .kind
                    .ends_with("_type_def")
            );
            assert_eq!(raw.text().as_ptr(), source[raw.start()..].as_ptr());
            assert_eq!(
                element_type.is_some(),
                matches!(kind, K::NestedTable | K::AssociativeArray | K::Varray)
            );
            assert_eq!(index_type.is_some(), *kind == K::AssociativeArray);
            assert_eq!(bound.is_some(), *kind == K::Varray);
            if let Some(E::Present {
                original: Some(OriginalProjection::Exact(raw)),
                ..
            }) = bound
            {
                assert_eq!(raw.text(), "10+1");
            }
            let modifier_kinds: Vec<_> = modifiers.iter().map(|m| m.node.kind.as_str()).collect();
            if *kind == K::Record {
                assert!(
                    !modifier_kinds.contains(&"NOT"),
                    "field NOT NULL must not escape record"
                );
            }
            if matches!(kind, K::NestedTable | K::Varray) {
                assert!(modifier_kinds.contains(&"NOT"));
                assert!(modifier_kinds.contains(&"NULL_"));
            }
            assert_eq!(
                entity
                    .namespace()
                    .iter()
                    .map(|n| n.name())
                    .collect::<Vec<_>>(),
                ["p"]
            );
        }
        assert!(
            model
                .entities()
                .iter()
                .find(|e| e.name() == "comum")
                .unwrap()
                .oracle_declaration()
                .is_none()
        );
        Ok(())
    })
}

#[test]
fn oracle_typed_declarations_keep_optional_native_types_and_variant_syntax() -> TestResult {
    use graphit_ast::antlr_engine::model::{AntlrOracleDeclaration as D, AntlrOracleTypeKind as K};
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let original = context();
    let source = "CREATE PROCEDURE p(a,b IN) IS TYPE a_t IS VARYING ARRAY(2*3) OF NUMBER; TYPE b_t IS ARRAY(4) OF NUMBER; TYPE c_t IS TABLE OF NUMBER INDEXED BY PLS_INTEGER; TYPE d_t IS TABLE OF a_t%TYPE; TYPE e_t IS TABLE OF tab%ROWTYPE; BEGIN NULL; END; /";
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["variants.sql"], limits())?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        assert!(
            model.diagnostics().is_empty(),
            "{:?}",
            model
                .diagnostics()
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        for entity in model.entities() {
            match entity.oracle_declaration() {
                Some(D::Parameter { .. }) => assert!(matches!(
                    entity.contributions()[0].declared_type,
                    AntlrTypeObservation::Missing
                )),
                Some(D::Type(details)) => {
                    let kind = &details.kind;
                    let element_type = &details.element_type;
                    if matches!(entity.name(), "a_t" | "b_t") {
                        assert_eq!(*kind, K::Varray);
                    }
                    if entity.name() == "c_t" {
                        assert_eq!(*kind, K::AssociativeArray);
                    }
                    if entity.name() == "d_t" || entity.name() == "e_t" {
                        let Some(AntlrTypeObservation::Present { raw, .. }) = element_type else {
                            panic!("nominal native type")
                        };
                        assert!(raw.text().contains('%'));
                    }
                }
                _ => {}
            }
        }
        assert_eq!(
            model
                .entities()
                .iter()
                .filter(|e| e.identity().label() == "Type")
                .count(),
            5
        );
        Ok(())
    })
}

#[test]
fn oracle_typed_declarations_keep_package_parameters_and_query_policy() -> TestResult {
    use graphit_ast::antlr_engine::model::AntlrOracleDeclaration as D;
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = r#"CREATE PACKAGE "Ω"."PK" AS FUNCTION f("δ" NUMBER DEFAULT 2) RETURN NUMBER; PROCEDURE p(a IN NUMBER); END "PK";
/
CREATE PACKAGE BODY "Ω"."PK" AS FUNCTION f("δ" NUMBER DEFAULT 2) RETURN NUMBER IS TYPE tt IS TABLE OF NUMBER; BEGIN RETURN 2; END; PROCEDURE p(a IN NUMBER) IS PROCEDURE inner_p(a IN NUMBER) IS BEGIN NULL; END; BEGIN NULL; END; END "PK";
/"#;
    let original = context();
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["package.sql"], limits())?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        assert!(
            model
                .diagnostics()
                .iter()
                .all(|d| matches!(d.issue, AntlrIssue::DeclarationReference { .. })),
            "{:?}",
            model
                .diagnostics()
                .iter()
                .map(|d| &d.issue)
                .collect::<Vec<_>>()
        );
        let params: Vec<_> = model
            .entities()
            .iter()
            .filter(|e| matches!(e.oracle_declaration(), Some(D::Parameter { .. })))
            .collect();
        assert_eq!(params.len(), 5);
        assert_eq!(
            params
                .iter()
                .map(|e| e.identity().uid().as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            5
        );
        for p in params {
            let owners: Vec<_> = p.namespace().iter().map(|o| o.name()).collect();
            assert_eq!(owners[0], "PK");
            assert_eq!(
                owners.len(),
                if owners.last() == Some(&"inner_p") {
                    3
                } else {
                    2
                }
            );
        }
        let typ = model.entities().iter().find(|e| e.name() == "tt").unwrap();
        assert_eq!(
            typ.namespace().iter().map(|o| o.name()).collect::<Vec<_>>(),
            ["PK", "f"]
        );
        Ok(())
    })?;
    let q = "  - {data_key: first, graph_label: Type, pattern: '//type_declaration/identifier', type: entity}\n";
    let other = q.replace("first", "second");
    let source = "DECLARE TYPE t IS TABLE OF NUMBER; BEGIN NULL; END; / DECLARE TYPE t IS VARRAY(2) OF NUMBER; BEGIN NULL; END; /";
    let mut runs = Vec::new();
    for queries in [format!("{q}{q}{other}"), format!("{other}{q}{q}")] {
        let draft=catalog(format!("language: plsql\nparser: antlr4\ngrammar: antlr-plsql\nembed_labels: [Type]\nqueries:\n{queries}").as_bytes());
        let original = context();
        with_input(source, &original, |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["homonyms.sql"],
                limits(),
            )?;
            assert!(parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            assert!(model.diagnostics().is_empty());
            assert_eq!(model.entities().len(), 4);
            let mut keys = Vec::new();
            for e in model.entities() {
                assert!(e.oracle_declaration().is_some());
                assert_eq!(
                    e.contributions().len(),
                    if e.data_key() == "first" { 2 } else { 1 }
                );
                keys.push((
                    e.identity().uid().as_str().to_owned(),
                    e.data_key().to_owned(),
                ));
            }
            keys.sort();
            runs.push(keys);
            Ok(())
        })?;
    }
    assert_eq!(runs[0], runs[1]);
    let rejected = q.replace("type: entity", "name_reject: '^t$', type: entity");
    let draft = catalog(
        format!(
            "language: plsql\nparser: antlr4\ngrammar: antlr-plsql\nqueries:\n{rejected}{other}"
        )
        .as_bytes(),
    );
    let original = context();
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["homonyms.sql"], limits())?;
        let model = parsed.assemble()?;
        assert_eq!(model.entities().len(), 2);
        assert!(model.entities().iter().all(|e| e.data_key() == "second"));
        assert!(
            model
                .diagnostics()
                .iter()
                .any(|d| matches!(d.issue, AntlrIssue::ExcludedName))
        );
        Ok(())
    })
}

#[test]
fn oracle_typed_declarations_preserve_synthetic_fields_and_recovery() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleDeclaration as D, AntlrOracleExpression as E,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "CREATE PROCEDURE p(a IN NUMBER DEFAULT 7) IS TYPE v_t IS VARRAY(10) OF NUMBER; BEGIN NULL; END; /";
    for (marker, len, role) in [
        ("a IN", 1, "parameter name"),
        ("IN NUMBER", 2, "mode"),
        ("NUMBER DEFAULT", 6, "type"),
        ("7", 1, "default"),
        ("v_t", 3, "type name"),
        ("VARRAY", 6, "definition"),
        ("10", 2, "bound"),
    ] {
        let start = source.find(marker).unwrap();
        let end = start + len;
        let original = context();
        let pieces = [
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
                original: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
            },
            OriginSegment::Synthetic {
                prepared: ByteSpan {
                    start: start as u64,
                    end: end as u64,
                },
                reason: SyntheticReason::Wrapper,
            },
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: end as u64,
                    end: source.len() as u64,
                },
                original: ByteSpan {
                    start: end as u64,
                    end: source.len() as u64,
                },
            },
        ];
        let removed = [ByteSpan {
            start: start as u64,
            end: end as u64,
        }];
        let input = PreparedInput::new(
            source.as_bytes(),
            source.as_bytes(),
            OriginPlan {
                segments: &pieces,
                removed_original: &removed,
                anchors: &[],
            },
            &original,
        )?;
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            &input,
            &["synthetic.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{role}"
        );
        let model = parsed.assemble()?;
        assert!(
            model
                .code_projections()?
                .iter()
                .all(|p| matches!(p, AntlrProjection::Unavailable(_))),
            "{role}"
        );
        let parameter = model.entities().iter().find(|e| e.name() == "a");
        let typ = model.entities().iter().find(|e| e.name() == "v_t");
        match role {
            "parameter name" => assert!(parameter.is_none()),
            "type name" => assert!(typ.is_none()),
            "type" => assert!(matches!(
                parameter.unwrap().contributions()[0].declared_type,
                AntlrTypeObservation::NonExact(_)
            )),
            "mode" => {
                let Some(D::Parameter { modes, .. }) = parameter.unwrap().oracle_declaration()
                else {
                    panic!("modes")
                };
                assert!(!matches!(
                    modes[0].original,
                    Some(OriginalProjection::Exact(_))
                ));
            }
            "default" => {
                let Some(D::Parameter {
                    default_value: Some(E::Present { original, .. }),
                    ..
                }) = parameter.unwrap().oracle_declaration()
                else {
                    panic!("default")
                };
                assert!(!matches!(original, Some(OriginalProjection::Exact(_))));
            }
            "definition" => {
                let Some(D::Type(details)) = typ.unwrap().oracle_declaration() else {
                    panic!("definition")
                };
                assert!(matches!(
                    &details.definition,
                    AntlrTypeObservation::NonExact(_)
                ));
            }
            "bound" => {
                let Some(D::Type(details)) = typ.unwrap().oracle_declaration() else {
                    panic!("type")
                };
                let Some(E::Present { original, .. }) = &details.bound else {
                    panic!("bound")
                };
                assert!(!matches!(original, Some(OriginalProjection::Exact(_))));
            }
            _ => unreachable!(),
        }
    }
    for source in [
        "CREATE PROCEDURE p(a NUMBER DEFAULT ) IS BEGIN NULL; END; /",
        "DECLARE TYPE v IS VARRAY() OF NUMBER; BEGIN NULL; END; /",
        "DECLARE TYPE t IS ; BEGIN NULL; END; /",
    ] {
        with_input(source, &context(), |input| {
            let parsed =
                ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["bad.sql"], limits())?;
            assert!(!parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            assert!(
                model
                    .code_projections()?
                    .iter()
                    .all(|p| matches!(p, AntlrProjection::Unavailable(_))),
                "bad source={source}; available={:?}; diagnostics={:?}; native={:?}; expressions={:?}",
                model
                    .code_projections()?
                    .iter()
                    .filter_map(|p| match p {
                        AntlrProjection::Available(v) => Some((
                            v.entity.name(),
                            v.entity.identity().label(),
                            v.extent.range().text()
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                model
                    .diagnostics()
                    .iter()
                    .map(|d| &d.issue)
                    .collect::<Vec<_>>(),
                parsed
                    .observations()
                    .syntax
                    .diagnostics
                    .iter()
                    .map(|d| (&d.phase, d.line, d.column, &d.span, &d.status))
                    .collect::<Vec<_>>(),
                model
                    .entities()
                    .iter()
                    .filter_map(|e| match e.oracle_declaration() {
                        Some(D::Type(details)) => match &details.bound {
                            Some(E::Present { node, original, .. }) => Some((
                                e.name(),
                                node.span.as_ref().map(|s| (s.start, s.end)),
                                format!("{:?}", node.status),
                                match original {
                                    Some(OriginalProjection::Exact(r)) => Some(r.text()),
                                    _ => None,
                                }
                            )),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            );
            assert!(!model.diagnostics().is_empty());
            Ok(())
        })?;
    }
    let original = context();
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["cancel.sql"], limits())?;
        original.cancellation().cancel();
        assert!(
            matches!(parsed.assemble(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    let deadline = Instant::now() + std::time::Duration::from_secs(1);
    let original = SnapshotContext::new(deadline, Cancellation::default());
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["timeout.sql"], limits())?;
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        assert!(
            matches!(parsed.assemble(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::TimedOut)
        );
        Ok(())
    })
}

fn application_source<'a>(
    syntax: &'a graphit_ast::antlr_engine::model::AntlrNativeSyntax<'_, '_>,
) -> Option<&'a str> {
    match syntax.original {
        Some(OriginalProjection::Exact(range)) => Some(range.text()),
        _ => None,
    }
}

#[test]
fn oracle_applications_preserve_groups_and_named_arguments() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationKind as K, AntlrOracleExpression as E,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "CREATE PROCEDURE p IS x NUMBER; BEGIN x := f(1,nome=>g(2))(3); x := empty_call(); x := \"Ω\"(4); x := keep_call(1) KEEP (DENSE_RANK FIRST ORDER BY 1); x := remote_fn@lnk(5); END; /";
    with_input(source, &context(), |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["apps.sql"], limits())?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let site = model
            .oracle_applications()
            .iter()
            .find(|a| application_source(&a.head) == Some("f(1,nome=>g(2))(3)"))
            .expect("native f head");
        assert_eq!(site.kind, K::GeneralElementPart);
        assert_eq!(
            site.groups
                .iter()
                .map(|g| g.arguments.len())
                .collect::<Vec<_>>(),
            [2, 1]
        );
        assert_eq!(
            application_source(&site.groups[0].syntax),
            Some("(1,nome=>g(2))")
        );
        assert_eq!(application_source(&site.groups[1].syntax), Some("(3)"));
        let named = &site.groups[0].arguments[1];
        assert_eq!(application_source(&named.syntax), Some("nome=>g(2)"));
        assert_eq!(
            application_source(named.name.as_ref().unwrap()),
            Some("nome")
        );
        let E::Present {
            original: Some(OriginalProjection::Exact(expr)),
            ..
        } = &named.expression
        else {
            panic!("exact native expression")
        };
        assert_eq!(expr.text(), "g(2)");
        let nested = model
            .oracle_applications()
            .iter()
            .find(|a| application_source(&a.head) == Some("g(2)"))
            .unwrap();
        assert_eq!(nested.groups.len(), 1);
        assert_eq!(nested.groups[0].arguments.len(), 1);
        let empty = model
            .oracle_applications()
            .iter()
            .find(|a| application_source(&a.head) == Some("empty_call()"))
            .unwrap();
        assert_eq!(empty.groups.len(), 1);
        assert!(empty.groups[0].arguments.is_empty());
        let quoted = model
            .oracle_applications()
            .iter()
            .find(|a| application_source(&a.head) == Some("\"Ω\"(4)"))
            .unwrap();
        assert_eq!(application_source(&quoted.groups[0].syntax), Some("(4)"));
        let keep = model
            .oracle_applications()
            .iter()
            .find(|a| application_source(&a.head).is_some_and(|s| s.starts_with("keep_call(")))
            .expect("native KEEP");
        assert_eq!(keep.groups.len(), 1);
        assert_eq!(keep.groups[0].arguments.len(), 1);
        assert_eq!(
            application_source(keep.groups[0].keep_clause.as_ref().unwrap()),
            Some("KEEP (DENSE_RANK FIRST ORDER BY 1)")
        );
        let remote = model
            .oracle_applications()
            .iter()
            .find(|a| application_source(&a.head) == Some("remote_fn@lnk(5)"))
            .expect("native database link retained");
        assert_eq!(remote.groups.len(), 1);
        for app in model.oracle_applications() {
            assert!(std::ptr::eq(
                model.oracle_application(app.occurrence_index).unwrap(),
                app
            ));
            assert!(model.unresolved_relations().contains(&app.occurrence_index));
            assert_eq!(
                app.boundaries
                    .iter()
                    .map(|b| b.node.kind.as_str())
                    .collect::<Vec<_>>(),
                ["body", "create_procedure_body"]
            );
            assert!(
                app.boundaries
                    .iter()
                    .all(|b| application_source(b).is_some())
            );
        }
        Ok(())
    })
}

#[test]
fn oracle_applications_keep_keyword_routine_and_boundary_order() -> TestResult {
    use graphit_ast::antlr_engine::model::AntlrOracleApplicationKind as K;
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "CREATE PROCEDURE p(a NUMBER DEFAULT sig()) IS v NUMBER := init_value(); PROCEDURE inner_p IS BEGIN nested_call(); END; BEGIN v := obj.EXISTS(1)(2).WHEN(3); first_call(1).second_call(2); bare_call; DECLARE local_v NUMBER := local_init(); BEGIN anonymous_call(); END; END; / SELECT top_call(),CHR(65 USING NCHAR_CS) FROM dual;";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["boundaries.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let keyword: Vec<_> = model
            .oracle_applications()
            .iter()
            .filter(|a| a.kind == K::KeywordMethod)
            .collect();
        assert_eq!(keyword.len(), 2);
        assert_eq!(
            keyword
                .iter()
                .map(|a| application_source(&a.head))
                .collect::<Vec<_>>(),
            [Some("EXISTS"), Some("WHEN")]
        );
        assert_eq!(
            keyword[0]
                .groups
                .iter()
                .map(|g| application_source(&g.syntax))
                .collect::<Vec<_>>(),
            [Some("(1)"), Some("(2)")]
        );
        assert_eq!(
            keyword[1]
                .groups
                .iter()
                .map(|g| application_source(&g.syntax))
                .collect::<Vec<_>>(),
            [Some("(3)")]
        );
        for (name, group) in [
            ("first_call", Some("(1)")),
            ("second_call", Some("(2)")),
            ("bare_call", None),
        ] {
            let site = model
                .oracle_applications()
                .iter()
                .find(|a| a.kind == K::RoutineName && application_source(&a.head) == Some(name))
                .expect(name);
            assert_eq!(
                site.groups
                    .first()
                    .and_then(|g| application_source(&g.syntax)),
                group
            );
            assert_eq!(site.groups.len(), usize::from(group.is_some()));
        }
        for (head, expected) in [
            (
                "sig()",
                vec!["default_value_part", "parameter", "create_procedure_body"],
            ),
            (
                "init_value()",
                vec![
                    "default_value_part",
                    "variable_declaration",
                    "create_procedure_body",
                ],
            ),
            (
                "nested_call",
                vec!["body", "procedure_body", "create_procedure_body"],
            ),
            (
                "local_init()",
                vec![
                    "default_value_part",
                    "variable_declaration",
                    "block",
                    "body",
                    "create_procedure_body",
                ],
            ),
            (
                "anonymous_call",
                vec!["body", "block", "body", "create_procedure_body"],
            ),
            ("top_call()", vec![]),
        ] {
            let site = model
                .oracle_applications()
                .iter()
                .find(|a| application_source(&a.head) == Some(head))
                .expect(head);
            assert_eq!(
                site.boundaries
                    .iter()
                    .map(|b| b.node.kind.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "{head}"
            );
        }
        let chr = parsed
            .observations()
            .occurrences
            .iter()
            .enumerate()
            .find(|(_, o)| o.query.pattern().value() == "//string_function/CHR")
            .unwrap_or_else(|| {
                panic!(
                    "dedicated native observations={:?}",
                    parsed
                        .observations()
                        .occurrences
                        .iter()
                        .filter(|o| o.query.relation_type().value() == "CALLS")
                        .map(|o| (
                            o.query.pattern().value().as_str(),
                            parsed.observations().syntax.nodes[o.matched.node_index]
                                .kind
                                .as_str()
                        ))
                        .collect::<Vec<_>>()
                )
            });
        assert!(model.unresolved_relations().contains(&chr.0));
        assert!(
            model.oracle_application(chr.0).is_none(),
            "dedicated arguments are not generic groups"
        );
        Ok(())
    })
}

#[test]
fn oracle_applications_preserve_duplicate_occurrences_and_nonexact_origins() -> TestResult {
    use graphit_ast::antlr_engine::model::AntlrOracleExpression as E;
    let stock = std::str::from_utf8(include_bytes!("fixtures/catalog/shipped/plsql.yaml"))?;
    let duplicate = "queries:\n  - data_key: alternate_calls\n    graph_label: \"\"\n    pattern: '//general_element_part'\n    child_rule: 'function_argument'\n    type: relation\n    relation_type: CALLS\n    captures: {name: [{xpath: '/id_expression'}]}\n";
    let profile = stock.replace("queries:\n", duplicate);
    let draft = catalog(profile.as_bytes());
    let source = "BEGIN x := outer_call(inner_call(2)); END; /";
    let original = context();
    with_input(source, &original, |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["duplicate.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        assert_eq!(model.oracle_applications().len(), 4);
        for head in ["outer_call(inner_call(2))", "inner_call(2)"] {
            let sites: Vec<_> = model
                .oracle_applications()
                .iter()
                .filter(|a| application_source(&a.head) == Some(head))
                .collect();
            assert_eq!(sites.len(), 2);
            assert_ne!(sites[0].occurrence_index, sites[1].occurrence_index);
            assert_eq!(sites[0].head.node_index, sites[1].head.node_index);
            assert_eq!(
                sites[0].groups[0].syntax.node_index,
                sites[1].groups[0].syntax.node_index
            );
            let keys: std::collections::BTreeSet<_> = sites
                .iter()
                .map(|a| {
                    parsed.observations().occurrences[a.occurrence_index]
                        .query
                        .data_key()
                        .value()
                        .as_str()
                })
                .collect();
            assert_eq!(
                keys,
                std::collections::BTreeSet::from(["calls", "alternate_calls"])
            );
        }
        Ok(())
    })?;
    let start = source.find('2').unwrap();
    let end = start + 1;
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: start as u64,
            },
            original: ByteSpan {
                start: 0,
                end: start as u64,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: start as u64,
                end: end as u64,
            },
            reason: SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: end as u64,
                end: source.len() as u64,
            },
            original: ByteSpan {
                start: end as u64,
                end: source.len() as u64,
            },
        },
    ];
    let removed = [ByteSpan {
        start: start as u64,
        end: end as u64,
    }];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed =
        ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["compound.sql"], limits())?;
    let model = parsed.assemble()?;
    assert_eq!(model.oracle_applications().len(), 4);
    for app in model.oracle_applications() {
        assert!(app.head.original.is_some());
        assert!(!matches!(
            app.head.original,
            Some(OriginalProjection::Exact(_))
        ));
        assert!(app.groups[0].syntax.original.is_some());
        let E::Present {
            original: Some(origin),
            ..
        } = &app.groups[0].arguments[0].expression
        else {
            panic!("native expression preserved")
        };
        assert!(!matches!(origin, OriginalProjection::Exact(_)));
    }
    for malformed in [
        "BEGIN x := retained_call(1); x := f(,); END; /",
        "BEGIN x := retained_call(1); x := f(1; END; /",
    ] {
        with_input(malformed, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["recovery.sql"],
                limits(),
            )?;
            assert!(!parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            assert!(
                !model.oracle_applications().is_empty(),
                "recovery keeps raw application evidence"
            );
            for app in model.oracle_applications() {
                assert!(model.unresolved_relations().contains(&app.occurrence_index));
                assert!(std::ptr::eq(
                    app.head.node,
                    &parsed.observations().syntax.nodes[app.head.node_index]
                ));
            }
            Ok(())
        })?;
    }
    with_input(source, &original, |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["cancel.sql"], limits())?;
        original.cancellation().cancel();
        assert!(
            matches!(parsed.assemble(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn oracle_collection_statements_keep_native_receivers_indices_and_shapes() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationKind as K, AntlrOracleExpression as E,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = r#"BEGIN items.DELETE; items.DELETE(1); items.DELETE(1,2); items.EXTEND; items.EXTEND(2); items.EXTEND(2,3); items.TRIM; items.TRIM(2); "Ω".DELETE(1,idx(2)); make_items(1).DELETE(idx(2)); END; /"#;
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["collections.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let sites: Vec<_> = model
            .oracle_applications()
            .iter()
            .filter(|a| a.kind == K::CollectionMethodStatement)
            .collect();
        assert_eq!(sites.len(), 10);
        let expected = [
            ("items.DELETE", "items", 0),
            ("items.DELETE(1)", "items", 1),
            ("items.DELETE(1,2)", "items", 2),
            ("items.EXTEND", "items", 0),
            ("items.EXTEND(2)", "items", 1),
            ("items.EXTEND(2,3)", "items", 2),
            ("items.TRIM", "items", 0),
            ("items.TRIM(2)", "items", 1),
            (r#""Ω".DELETE(1,idx(2))"#, r#""Ω""#, 2),
            ("make_items(1).DELETE(idx(2))", "make_items(1)", 1),
        ];
        for (statement_text, receiver_text, count) in expected {
            let site = sites
                .iter()
                .find(|a| {
                    application_source(&a.collection_statement.as_ref().unwrap().syntax)
                        == Some(statement_text)
                })
                .expect(statement_text);
            assert!(site.groups.is_empty(), "no synthetic function_argument");
            assert!(site.assignment.is_none());
            let statement = site.collection_statement.as_ref().unwrap();
            assert_eq!(statement.syntax.node.kind, "collection_method_call");
            assert_eq!(statement.indices.len(), count);
            let E::Present {
                node,
                original: Some(OriginalProjection::Exact(receiver)),
                ..
            } = &statement.receiver
            else {
                panic!("native receiver")
            };
            assert_eq!(node.kind, "expression");
            assert_eq!(receiver.text(), receiver_text);
            assert!(
                statement
                    .indices
                    .iter()
                    .all(|n| n.node.kind == "expression" && application_source(n).is_some())
            );
            assert_eq!(
                statement.delimiters.len(),
                if count == 0 { 0 } else { count + 1 }
            );
            assert!(
                statement.delimiters.iter().all(|n| matches!(
                    n.node.kind.as_str(),
                    "LEFT_PAREN" | "RIGHT_PAREN" | "COMMA"
                ))
            );
            assert_eq!(
                site.boundaries
                    .iter()
                    .map(|b| b.node.kind.as_str())
                    .collect::<Vec<_>>(),
                ["anonymous_block"]
            );
            assert_eq!(
                site.head.node_index,
                parsed.observations().occurrences[site.occurrence_index]
                    .matched
                    .node_index
            );
        }
        let last = sites
            .iter()
            .find(|a| {
                application_source(&a.collection_statement.as_ref().unwrap().syntax)
                    == Some("make_items(1).DELETE(idx(2))")
            })
            .unwrap()
            .collection_statement
            .as_ref()
            .unwrap();
        assert_eq!(application_source(&last.indices[0]), Some("idx(2)"));
        assert!(
            model
                .oracle_applications()
                .iter()
                .any(|a| application_source(&a.head) == Some("make_items(1)"))
        );
        assert!(
            model
                .oracle_applications()
                .iter()
                .any(|a| application_source(&a.head) == Some("idx(2)"))
        );
        Ok(())
    })
}

#[test]
fn oracle_assignment_context_preserves_target_path_without_resolving_writes() -> TestResult {
    use graphit_ast::antlr_engine::model::AntlrOracleAssignmentSide as S;
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "CREATE PROCEDURE p(items IN NUMBER) IS v NUMBER; BEGIN items(1) := fn(2); items(idx(1)) := value_fn(2); v := items.COUNT; v := items.FIRST; outside_call; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["assignment.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        for (head, side, nested) in [
            ("items(1)", S::Target, false),
            ("fn(2)", S::Value, true),
            ("items(idx(1))", S::Target, false),
            ("idx(1)", S::Target, true),
            ("value_fn(2)", S::Value, true),
        ] {
            let site = model
                .oracle_applications()
                .iter()
                .find(|a| application_source(&a.head) == Some(head))
                .expect(head);
            assert!(
                site.collection_statement.is_none(),
                "location does not prove collection type"
            );
            let assignment = site.assignment.as_ref().expect("native assignment");
            assert_eq!(assignment.side, side);
            assert_eq!(assignment.statement.node.kind, "assignment_statement");
            assert_eq!(
                assignment.branch.node.kind,
                if side == S::Target {
                    "general_element"
                } else {
                    "expression"
                }
            );
            assert_eq!(!assignment.expressions.is_empty(), nested, "{head}");
            assert!(
                assignment
                    .expressions
                    .iter()
                    .all(|e| e.node.kind == "expression" && application_source(e).is_some())
            );
            assert!(
                application_source(&assignment.statement)
                    .unwrap()
                    .contains(":=")
            );
        }
        let outside = model
            .oracle_applications()
            .iter()
            .find(|a| application_source(&a.head) == Some("outside_call"))
            .unwrap();
        assert!(outside.assignment.is_none());
        for name in ["COUNT", "FIRST"] {
            let application = model
                .oracle_applications()
                .iter()
                .find(|application| application_source(&application.head) == Some(name))
                .unwrap();
            assert!(application.member.is_some());
            let binding = model.oracle_binding(application.occurrence_index).unwrap();
            assert_eq!(
                binding.classification,
                graphit_ast::antlr_engine::model::AntlrOracleApplicationClass::Unresolved
            );
            assert!(
                binding.collection_method.is_none(),
                "NUMBER parameter is not Collection"
            );
        }
        Ok(())
    })
}

#[test]
fn oracle_collection_assignment_nonexact_duplicates_and_recovery_remain_observed() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationKind as K, AntlrOracleExpression as E,
    };
    let stock = std::str::from_utf8(include_bytes!("fixtures/catalog/shipped/plsql.yaml"))?;
    let custom=stock.replace("queries:\n","queries:\n  - {data_key: alternate_calls, graph_label: '', type: relation, relation_type: CALLS, pattern: '//collection_method_call/DELETE', name_reject: '^DELETE$', captures: {qualifier: [{nearest_ancestor: 'collection_method_call'}]}}\n");
    let draft = catalog(custom.as_bytes());
    let source = "BEGIN items(1) := fn(2); items.DELETE(3,idx(4)); END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["duplicate-collection.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let sites: Vec<_> = model
            .oracle_applications()
            .iter()
            .filter(|a| a.kind == K::CollectionMethodStatement)
            .collect();
        assert_eq!(sites.len(), 2);
        assert_ne!(sites[0].occurrence_index, sites[1].occurrence_index);
        assert_eq!(sites[0].head.node_index, sites[1].head.node_index);
        let keys: std::collections::BTreeSet<_> = sites
            .iter()
            .map(|a| {
                parsed.observations().occurrences[a.occurrence_index]
                    .query
                    .data_key()
                    .value()
                    .as_str()
            })
            .collect();
        assert_eq!(
            keys,
            std::collections::BTreeSet::from(["alternate_calls", "calls"])
        );
        Ok(())
    })?;
    for (marker, len, field) in [
        ("items(1)", 8, "assignment"),
        ("items.DELETE", 5, "receiver"),
        ("4", 1, "index"),
    ] {
        let start = source.find(marker).unwrap();
        let end = start + len;
        let original = context();
        let segments = [
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
                original: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
            },
            OriginSegment::Synthetic {
                prepared: ByteSpan {
                    start: start as u64,
                    end: end as u64,
                },
                reason: SyntheticReason::Wrapper,
            },
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: end as u64,
                    end: source.len() as u64,
                },
                original: ByteSpan {
                    start: end as u64,
                    end: source.len() as u64,
                },
            },
        ];
        let removed = [ByteSpan {
            start: start as u64,
            end: end as u64,
        }];
        let input = PreparedInput::new(
            source.as_bytes(),
            source.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &removed,
                anchors: &[],
            },
            &original,
        )?;
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            &input,
            &["nonexact-collection.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        if field == "assignment" {
            let site=model.oracle_applications().iter().find(|a|a.head.node.kind=="general_element_part"&&a.assignment.as_ref().is_some_and(|c|c.side==graphit_ast::antlr_engine::model::AntlrOracleAssignmentSide::Target)).unwrap();
            assert!(!matches!(
                site.assignment.as_ref().unwrap().branch.original,
                Some(OriginalProjection::Exact(_))
            ));
        } else {
            for site in model
                .oracle_applications()
                .iter()
                .filter(|a| a.kind == K::CollectionMethodStatement)
            {
                let collection = site.collection_statement.as_ref().unwrap();
                if field == "receiver" {
                    let E::Present {
                        original: Some(origin),
                        ..
                    } = &collection.receiver
                    else {
                        panic!("present native receiver")
                    };
                    assert!(!matches!(origin, OriginalProjection::Exact(_)));
                } else {
                    assert!(!matches!(
                        collection.indices[1].original,
                        Some(OriginalProjection::Exact(_))
                    ));
                }
            }
        }
    }
    for source in [
        "BEGIN retained_call(1); items.DELETE(,); END; /",
        "BEGIN retained_call(1); items.TRIM(; END; /",
    ] {
        with_input(source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["collection-recovery.sql"],
                limits(),
            )?;
            assert!(!parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            assert!(
                !model.oracle_applications().is_empty(),
                "retain valid raw evidence with native recovery"
            );
            for site in model.oracle_applications() {
                assert!(
                    model
                        .unresolved_relations()
                        .contains(&site.occurrence_index)
                );
                assert!(std::ptr::eq(
                    site.head.node,
                    &parsed.observations().syntax.nodes[site.head.node_index]
                ));
            }
            Ok(())
        })?;
    }
    let original = context();
    with_input(source, &original, |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["cancel-collection.sql"],
            limits(),
        )?;
        original.cancellation().cancel();
        assert!(
            matches!(parsed.assemble(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn oracle_local_binding_links_values_parameters_and_native_collection_types() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingState as S, AntlrOracleBoundKind as K,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    for (definition, collection, constructor) in [
        ("TABLE OF NUMBER", true, true),
        ("VARRAY(4) OF NUMBER", true, true),
        ("TABLE OF NUMBER INDEX BY PLS_INTEGER", true, false),
        ("RECORD (n NUMBER)", false, false),
        ("REF CURSOR", false, false),
    ] {
        let source = format!(
            "DECLARE TYPE numbers_t IS {definition}; xs numbers_t; BEGIN xs(1):=2; xs.DELETE(1); sink:=numbers_t(1); END; /"
        );
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["binding.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            let mut counts = [0; 3];
            for binding in model.oracle_bindings() {
                assert_eq!(
                    binding.receiver.state,
                    S::Resolved,
                    "{definition}: {:?}",
                    binding.receiver.issues
                );
                let candidate = &binding.receiver.candidates[0];
                assert!(!candidate.entity_indices.is_empty());
                assert!(
                    candidate
                        .scope_evidence
                        .iter()
                        .all(|s| matches!(s.original, Some(OriginalProjection::Exact(_))))
                );
                if candidate.kind == K::Value {
                    assert_eq!(binding.declared_type.as_ref().unwrap().state, S::Resolved);
                    assert_eq!(
                        binding.declared_type.as_ref().unwrap().candidates[0].kind,
                        K::Type
                    );
                }
                match binding.classification {
                    C::CollectionIndex => counts[0] += 1,
                    C::CollectionMethod => counts[1] += 1,
                    C::CollectionConstructor => counts[2] += 1,
                    C::Unresolved => {}
                }
                assert!(std::ptr::eq(
                    candidate.declaration.node,
                    &parsed.observations().syntax.nodes[candidate.declaration.node_index]
                ));
            }
            assert_eq!(
                counts,
                [
                    usize::from(collection),
                    usize::from(collection),
                    usize::from(constructor)
                ],
                "{definition}"
            );
            Ok(())
        })?;
    }
    let source = "DECLARE TYPE numbers_t IS TABLE OF NUMBER; PROCEDURE p(arg numbers_t) IS TYPE numbers_t IS RECORD (n NUMBER); BEGIN arg(10):=1; DECLARE arg NUMBER; BEGIN arg(20):=2; END; END; BEGIN NULL; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["parameter-binding.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let mut classes = Vec::new();
        for b in model.oracle_bindings() {
            assert_eq!(b.receiver.state, S::Resolved);
            classes.push(b.classification);
            if b.classification == C::CollectionIndex {
                assert_eq!(b.receiver.candidates[0].kind, K::Parameter);
                assert_eq!(
                    b.declared_type.as_ref().unwrap().candidates[0]
                        .scope
                        .node
                        .kind,
                    "anonymous_block",
                    "type lookup belongs to parameter declaration, before local record"
                );
            } else {
                assert_eq!(b.receiver.candidates[0].kind, K::Value);
            }
        }
        assert_eq!(
            classes.iter().filter(|&&c| c == C::CollectionIndex).count(),
            1
        );
        assert_eq!(classes.iter().filter(|&&c| c == C::Unresolved).count(), 1);
        Ok(())
    })?;
    Ok(())
}

#[test]
fn oracle_local_binding_preserves_profile_independence_ambiguity_and_visibility() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingIssue as I,
        AntlrOracleBindingState as S, AntlrOracleBoundKind as K,
    };
    let relation = "  - {data_key: calls, graph_label: '', pattern: '//general_element_part', child_rule: 'function_argument', type: relation, relation_type: CALLS, captures: {name: [{xpath: '/id_expression'}], qualifier: [{nearest_ancestor: 'general_element'}]}}\n";
    let draft = custom(
        &(relation.to_owned() + &relation.replace("data_key: calls", "data_key: duplicate_calls")),
    );
    for (declarations, expected, class) in [
        (
            "TYPE numbers_t IS TABLE OF NUMBER; xs numbers_t;",
            S::Resolved,
            C::CollectionIndex,
        ),
        (
            "TYPE numbers_t IS TABLE OF NUMBER; xs numbers_t; xs numbers_t;",
            S::Ambiguous,
            C::Unresolved,
        ),
        (
            "xs later_t; TYPE later_t IS TABLE OF NUMBER;",
            S::Resolved,
            C::Unresolved,
        ),
    ] {
        let source = format!("DECLARE {declarations} BEGIN xs(1):=2; END; /");
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["hidden-binding.sql"],
                limits(),
            )?;
            assert!(parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            assert!(model.entities().is_empty(), "no entity query");
            assert_eq!(
                model.oracle_bindings().len(),
                2,
                "raw duplicate queries retained"
            );
            for b in model.oracle_bindings() {
                assert_eq!(b.receiver.state, expected);
                assert_eq!(b.classification, class);
                assert!(
                    b.receiver
                        .candidates
                        .iter()
                        .all(|c| c.entity_indices.is_empty())
                );
                if declarations.starts_with("xs later_t") {
                    assert_eq!(b.declared_type.as_ref().unwrap().state, S::Missing);
                }
            }
            Ok(())
        })?;
    }
    for barrier in [
        "xs EXCEPTION;",
        "SUBTYPE xs IS NUMBER;",
        "CURSOR xs IS SELECT 1 FROM dual;",
    ] {
        let source = format!(
            "DECLARE TYPE numbers_t IS TABLE OF NUMBER; xs numbers_t; BEGIN DECLARE {barrier} BEGIN xs(1):=2; END; END; /"
        );
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["shadow-binding.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            for b in model.oracle_bindings() {
                assert_eq!(b.receiver.state, S::Resolved);
                assert_eq!(b.receiver.candidates[0].kind, K::Other);
                assert_eq!(b.classification, C::Unresolved);
            }
            Ok(())
        })?;
    }
    let source = "DECLARE TYPE numbers_t IS TABLE OF NUMBER; BEGIN numbers_t(1):=2; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["invalid-constructor.sql"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        assert_eq!(model.oracle_bindings().len(), 2);
        for b in model.oracle_bindings() {
            assert_eq!(b.receiver.state, S::Resolved);
            assert_eq!(b.classification, C::Unresolved);
            assert_eq!(b.issues, vec![I::InvalidContext]);
        }
        Ok(())
    })?;
    Ok(())
}

#[test]
fn oracle_local_binding_qualifies_names_without_generic_unicode_folding() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingState as S,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    for (decl, reference, expected) in [
        ("Mixed", "mixed", S::Resolved),
        ("\"MIXED\"", "mixed", S::Resolved),
        ("\"Mixed\"", "mixed", S::Missing),
        ("\"Mixed\"", "\"Mixed\"", S::Resolved),
        ("\"ação\"", "\"ação\"", S::Resolved),
        ("\"ação\"", "\"AÇÃO\"", S::Missing),
    ] {
        let source = format!(
            "DECLARE TYPE collection_t IS TABLE OF NUMBER; {decl} collection_t; BEGIN {reference}(1):=2; END; /"
        );
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["name-binding.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{decl} {reference}: {:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            assert_eq!(model.oracle_bindings().len(), 1);
            let b = &model.oracle_bindings()[0];
            assert_eq!(
                b.receiver.state, expected,
                "{decl} {reference}: {:?}",
                b.receiver.issues
            );
            assert_eq!(
                b.classification,
                if expected == S::Resolved {
                    C::CollectionIndex
                } else {
                    C::Unresolved
                }
            );
            assert!(b.receiver.decoded_name.is_some());
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn oracle_local_binding_requires_local_scope_markers_and_keeps_unrelated_origins() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingState as S,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "DECLARE TYPE collection_t IS TABLE OF NUMBER; xs collection_t; BEGIN xs(1):=2; DECLARE ys NUMBER; BEGIN ys(2):=3; END; END; /";
    for (marker, affected) in [
        ("BEGIN xs", true),
        ("BEGIN ys", false),
        ("xs collection", true),
    ] {
        let start = source.find(marker).unwrap();
        let end = start + if marker.starts_with("BEGIN") { 5 } else { 2 };
        let original = context();
        let segments = [
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
                original: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
            },
            OriginSegment::Synthetic {
                prepared: ByteSpan {
                    start: start as u64,
                    end: end as u64,
                },
                reason: SyntheticReason::Wrapper,
            },
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: end as u64,
                    end: source.len() as u64,
                },
                original: ByteSpan {
                    start: end as u64,
                    end: source.len() as u64,
                },
            },
        ];
        let removed = [ByteSpan {
            start: start as u64,
            end: end as u64,
        }];
        let input = PreparedInput::new(
            source.as_bytes(),
            source.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &removed,
                anchors: &[],
            },
            &original,
        )?;
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            &input,
            &["scope-origin.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let b=model.oracle_bindings().iter().find(|b|b.receiver.reference.is_some_and(|r|matches!(r.original,Some(OriginalProjection::Exact(raw)) if raw.text()=="xs"))).unwrap();
        assert_eq!(
            b.receiver.state,
            if affected {
                S::Indeterminate
            } else {
                S::Resolved
            },
            "{marker}"
        );
        assert_eq!(
            b.classification,
            if affected {
                C::Unresolved
            } else {
                C::CollectionIndex
            }
        );
        original.cancellation().cancel();
        assert!(
            matches!(parsed.assemble(),Err(AntlrModelError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
    }
    Ok(())
}

#[test]
fn oracle_local_binding_keeps_charset_and_unqualified_boundaries_explicit() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingIssue as I,
        AntlrOracleBindingState as S,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    for (decl, reference, state) in [
        ("ação", "ação", S::Resolved),
        ("ação", "AÇÃO", S::Indeterminate),
        ("ß", "SS", S::Indeterminate),
    ] {
        let source = format!("DECLARE {decl} NUMBER; BEGIN {reference}(1):=2; END; /");
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["charset-binding.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{decl}/{reference}: {:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            assert_eq!(model.oracle_bindings().len(), 1);
            let b = &model.oracle_bindings()[0];
            assert_eq!(
                b.receiver.state, state,
                "{decl}/{reference}: {:?}",
                b.receiver.issues
            );
            assert_eq!(b.classification, C::Unresolved);
            if state == S::Indeterminate {
                assert!(b.receiver.issues.contains(&I::DatabaseCharacterSet));
            }
            Ok(())
        })?;
    }
    for (declarations, statement) in [
        ("xs pkg.collection_t;", "xs(1):=2;"),
        ("xs source_value%TYPE;", "xs(1):=2;"),
        ("xs REF collection_t;", "xs(1):=2;"),
        (
            "TYPE collection_t IS TABLE OF NUMBER; xs collection_t;",
            "xs(1)(2):=3;",
        ),
        (
            "TYPE collection_t IS TABLE OF NUMBER; xs collection_t;",
            "sink:=pkg.xs(1);",
        ),
        (
            "TYPE collection_t IS TABLE OF NUMBER; xs collection_t;",
            "sink:=xs@remote_link(1);",
        ),
        (
            "TYPE collection_t IS TABLE OF NUMBER; xs collection_t;",
            "SELECT xs(1) INTO sink FROM dual;",
        ),
        (
            "TYPE collection_t IS TABLE OF NUMBER; xs collection_t;",
            "FOR i IN 1..2 LOOP xs(1):=2; END LOOP;",
        ),
        (
            "TYPE collection_t IS TABLE OF NUMBER; xs collection_t; ys NUMBER:=xs(1);",
            "NULL;",
        ),
        (
            "TYPE collection_t IS TABLE OF NUMBER; xs collection_t;",
            "$IF $$flag $THEN xs(1):=2; $END",
        ),
    ] {
        let source = format!("DECLARE {declarations} BEGIN {statement} END; /");
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["boundary-binding.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            assert!(!model.oracle_bindings().is_empty(), "{source}");
            for b in model.oracle_bindings() {
                assert_eq!(b.classification, C::Unresolved, "{source}");
            }
            Ok(())
        })?;
    }
    let source = "DECLARE TYPE collection_t IS TABLE OF NUMBER; xs collection_t; BEGIN xs(collection_t(1)):=2; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["nested-constructor.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        assert_eq!(
            model
                .oracle_bindings()
                .iter()
                .filter(|b| b.classification == C::CollectionConstructor)
                .count(),
            1,
            "index expression isn't assignment destination"
        );
        assert_eq!(
            model
                .oracle_bindings()
                .iter()
                .filter(|b| b.classification == C::CollectionIndex)
                .count(),
            1
        );
        Ok(())
    })?;
    let source = "DECLARE TYPE collection_t IS TABLE OF NUMBER; xs collection_t; $IF $$flag $THEN xs NUMBER; $END BEGIN xs(1):=2; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["conditional-shadow.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        assert_eq!(model.oracle_bindings().len(), 1);
        assert_eq!(
            model.oracle_bindings()[0].receiver.state,
            S::Indeterminate,
            "conditional declaration remains possible shadow"
        );
        assert!(
            model.oracle_bindings()[0]
                .receiver
                .issues
                .contains(&I::UnsupportedScope)
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn oracle_local_binding_rejects_scope_joins_and_incomplete_native_types() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingIssue as I,
        AntlrOracleBindingState as S,
    };
    let relation = "  - {data_key: calls, graph_label: '', pattern: '//general_element_part', child_rule: 'function_argument', type: relation, relation_type: CALLS, captures: {name: [{xpath: '/id_expression'}], qualifier: [{nearest_ancestor: 'general_element'}]}}\n  - {data_key: calls, graph_label: '', pattern: '//collection_method_call/DELETE', type: relation, relation_type: CALLS, captures: {qualifier: [{nearest_ancestor: 'collection_method_call'}]}}\n";
    let draft = custom(relation);
    let source =
        "DECLARE TYPE t IS VARRAY() OF NUMBER; items t; BEGIN items(1):=2; items.DELETE; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["empty-bound-binding.sql"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        assert!(model.entities().is_empty(), "type queries excluded");
        assert_eq!(model.oracle_bindings().len(), 2);
        for b in model.oracle_bindings() {
            assert_eq!(b.receiver.state, S::Resolved);
            assert_eq!(b.classification, C::Unresolved);
            assert!(b.collection_type.is_none());
            assert!(b.issues.contains(&I::MissingNativeField));
            assert!(
                b.type_evidence.iter().any(|f| f
                    .node
                    .span
                    .as_ref()
                    .is_some_and(|s| s.start == s.end))
            );
        }
        Ok(())
    })?;
    let prefix = "DECLARE TYPE t IS TABLE OF NUMBER; items t; BEGIN ";
    let removed = "NULL; END; / BEGIN ";
    let suffix = "items(1):=2; END; /";
    let source = format!("{prefix}{removed}{suffix}");
    let prepared = format!("{prefix}{suffix}");
    let original = context();
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: prefix.len() as u64,
            },
            original: ByteSpan {
                start: 0,
                end: prefix.len() as u64,
            },
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: prefix.len() as u64,
                end: prepared.len() as u64,
            },
            original: ByteSpan {
                start: (prefix.len() + removed.len()) as u64,
                end: source.len() as u64,
            },
        },
    ];
    let removed_original = [ByteSpan {
        start: prefix.len() as u64,
        end: (prefix.len() + removed.len()) as u64,
    }];
    let input = PreparedInput::new(
        source.as_bytes(),
        prepared.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed_original,
            anchors: &[],
        },
        &original,
    )?;
    let parsed =
        ParsedAntlrDocument::collect(&draft.profiles()[0], &input, &["scope-join.sql"], limits())?;
    assert!(parsed.observations().syntax.diagnostics.is_empty());
    let model = parsed.assemble()?;
    assert_eq!(model.oracle_bindings().len(), 1);
    let b = &model.oracle_bindings()[0];
    assert_eq!(b.receiver.state, S::Indeterminate);
    assert_eq!(b.classification, C::Unresolved);
    assert!(b.receiver.issues.contains(&I::NonExactOrigin));
    assert!(
        b.receiver
            .scope_continuity
            .iter()
            .any(|c| !c.verified && matches!(c.original, OriginalProjection::Compound(_)))
    );
    let source = "DECLARE TYPE t IS TABLE OF NUMBER; items t; BEGIN items(1):=2; END; /";
    let split = source.find("BEGIN").unwrap();
    let original = context();
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: split as u64,
            },
            original: ByteSpan {
                start: 0,
                end: split as u64,
            },
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: split as u64,
                end: source.len() as u64,
            },
            original: ByteSpan {
                start: split as u64,
                end: source.len() as u64,
            },
        },
    ];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    let parsed = ParsedAntlrDocument::collect(
        &draft.profiles()[0],
        &input,
        &["contiguous-scope.sql"],
        limits(),
    )?;
    let model = parsed.assemble()?;
    let b = &model.oracle_bindings()[0];
    assert_eq!(b.receiver.state, S::Resolved);
    assert_eq!(b.classification, C::CollectionIndex);
    assert!(
        b.receiver
            .scope_continuity
            .iter()
            .any(|c| c.verified && matches!(c.original, OriginalProjection::Compound(_)))
    );
    Ok(())
}

#[test]
fn oracle_local_binding_keeps_mapped_type_evidence_and_real_entity_multiplicity() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingIssue as I,
        AntlrOracleBindingState as S,
    };
    let stock = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    for (old_body, new_body, same) in [
        ("RECORD(n NUMBER)", "TABLE OF NUMBER", false),
        ("TABLE OF NUMBER", "TABLE OF NUMBER", true),
    ] {
        let prefix = "DECLARE TYPE t IS ";
        let suffix = "; items t; BEGIN items(1):=2; END; /";
        let source = format!("{prefix}{old_body}{suffix}");
        let prepared = format!("{prefix}{new_body}{suffix}");
        let original = context();
        let fragments = [ByteSpan {
            start: prefix.len() as u64,
            end: (prefix.len() + old_body.len()) as u64,
        }];
        let segments = [
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: 0,
                    end: prefix.len() as u64,
                },
                original: ByteSpan {
                    start: 0,
                    end: prefix.len() as u64,
                },
            },
            OriginSegment::MappedAtom {
                prepared: ByteSpan {
                    start: prefix.len() as u64,
                    end: (prefix.len() + new_body.len()) as u64,
                },
                original_fragments: &fragments,
            },
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: (prefix.len() + new_body.len()) as u64,
                    end: prepared.len() as u64,
                },
                original: ByteSpan {
                    start: (prefix.len() + old_body.len()) as u64,
                    end: source.len() as u64,
                },
            },
        ];
        let input = PreparedInput::new(
            source.as_bytes(),
            prepared.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )?;
        let parsed = ParsedAntlrDocument::collect(
            &stock.profiles()[0],
            &input,
            &["mapped-type-binding.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let b = &model.oracle_bindings()[0];
        assert_eq!(
            b.receiver.state,
            if same { S::Resolved } else { S::Indeterminate }
        );
        assert_eq!(b.classification, C::Unresolved);
        if same {
            assert!(b.issues.contains(&I::NonExactOrigin));
            assert!(
                b.type_evidence
                    .iter()
                    .any(|e| matches!(e.original, Some(OriginalProjection::Unmappable(_))))
            );
        } else {
            assert!(b.receiver.scope_continuity.iter().any(|c| !c.verified));
        }
    }
    let relation = "  - {data_key: calls, graph_label: '', pattern: '//general_element_part', child_rule: 'function_argument', type: relation, relation_type: CALLS, captures: {name: [{xpath: '/id_expression'}], qualifier: [{nearest_ancestor: 'general_element'}]}}\n";
    let entities = "  - {data_key: vars_a, graph_label: Variable, pattern: '//variable_declaration', type: entity, captures: {name: [{xpath: '/identifier'}]}}\n  - {data_key: vars_b, graph_label: Variable, pattern: '//variable_declaration', type: entity, captures: {name: [{xpath: '/identifier'}]}}\n";
    let draft = custom(&(relation.to_owned() + entities));
    let source = "DECLARE TYPE t IS TABLE OF NUMBER; items t; BEGIN items(1):=2; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["entity-tuples-binding.sql"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        let b = &model.oracle_bindings()[0];
        assert_eq!(b.receiver.state, S::Resolved);
        assert_eq!(b.receiver.candidates.len(), 1);
        assert_eq!(
            b.receiver.candidates[0].entity_indices.len(),
            2,
            "real data_key tuples are not lexical ambiguity"
        );
        assert!(
            b.declared_type.as_ref().unwrap().candidates[0]
                .entity_indices
                .is_empty(),
            "no Type entity query"
        );
        assert_eq!(b.classification, C::CollectionIndex);
        Ok(())
    })?;
    let draft = custom(
        &(relation.to_owned()
            + &entities.replace("type: entity", "type: entity, name_reject: '^items$'")),
    );
    let source = "DECLARE TYPE t IS TABLE OF NUMBER; items t; BEGIN DECLARE items NUMBER; BEGIN items(1):=2; END; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["rejected-shadow-binding.sql"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        let b = &model.oracle_bindings()[0];
        assert_eq!(b.receiver.state, S::Resolved);
        assert!(b.receiver.candidates[0].entity_indices.is_empty());
        assert_eq!(b.receiver.candidates[0].scope.node.kind, "block");
        assert_eq!(
            b.classification,
            C::Unresolved,
            "rejected inner scalar still shadows outer collection"
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn oracle_local_binding_scope_order_siblings_and_native_recovery() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingState as S,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "DECLARE TYPE t IS TABLE OF NUMBER; BEGIN DECLARE TYPE t IS RECORD(n NUMBER); a t; BEGIN a(1):=2; END; DECLARE b t; TYPE t IS RECORD(n NUMBER); BEGIN b(2):=3; END; a(3):=4; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["type-visibility.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        assert_eq!(model.oracle_bindings().len(), 3);
        for b in model.oracle_bindings() {
            let Some(OriginalProjection::Exact(head)) = model
                .oracle_application(b.occurrence_index)
                .unwrap()
                .head
                .original
            else {
                panic!("actual head");
            };
            match head.text() {
                "a(1)" => {
                    assert_eq!(b.receiver.state, S::Resolved);
                    assert_eq!(b.classification, C::Unresolved);
                    assert_eq!(
                        b.declared_type.as_ref().unwrap().candidates[0]
                            .scope
                            .node
                            .kind,
                        "block"
                    );
                }
                "b(2)" => {
                    assert_eq!(b.receiver.state, S::Resolved);
                    assert_eq!(b.classification, C::CollectionIndex);
                    assert_eq!(
                        b.declared_type.as_ref().unwrap().candidates[0]
                            .scope
                            .node
                            .kind,
                        "anonymous_block"
                    );
                }
                "a(3)" => {
                    assert_eq!(b.receiver.state, S::Missing);
                    assert_eq!(b.classification, C::Unresolved);
                }
                other => panic!("unexpected native application {other}"),
            }
        }
        Ok(())
    })?;
    let source = "CREATE PROCEDURE p(items local_t) IS TYPE local_t IS TABLE OF NUMBER; BEGIN items(1):=2; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["future-parameter-type.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let b = &model.oracle_bindings()[0];
        assert_eq!(b.receiver.state, S::Resolved);
        assert_eq!(b.declared_type.as_ref().unwrap().state, S::Missing);
        assert_eq!(b.classification, C::Unresolved);
        Ok(())
    })?;
    let source =
        "DECLARE TYPE t IS TABLE OF NUMBER; items t:=unknown(,); BEGIN items(1):=2; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["recovered-declaration-binding.sql"],
            limits(),
        )?;
        assert!(!parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let b = model
            .oracle_bindings()
            .iter()
            .find(|b| b.receiver.decoded_name.as_deref() == Some("items"))
            .expect("valid application retained after native declaration recovery");
        assert_eq!(b.receiver.state, S::Indeterminate);
        assert_eq!(b.classification, C::Unresolved);
        Ok(())
    })?;
    let source = "DECLARE TYPE t IS TABLE OF NUMBER; xs t; BEGIN xs(1):=2; END; / DECLARE TYPE u IS TABLE OF NUMBER; ys u; BEGIN ys(2):=3; END; /";
    let start = source.find("BEGIN").unwrap();
    let end = start + 5;
    let original = context();
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: start as u64,
            },
            original: ByteSpan {
                start: 0,
                end: start as u64,
            },
        },
        OriginSegment::Synthetic {
            prepared: ByteSpan {
                start: start as u64,
                end: end as u64,
            },
            reason: SyntheticReason::Wrapper,
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: end as u64,
                end: source.len() as u64,
            },
            original: ByteSpan {
                start: end as u64,
                end: source.len() as u64,
            },
        },
    ];
    let removed = [ByteSpan {
        start: start as u64,
        end: end as u64,
    }];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    let parsed = ParsedAntlrDocument::collect(
        &draft.profiles()[0],
        &input,
        &["independent-scope-origin.sql"],
        limits(),
    )?;
    assert!(parsed.observations().syntax.diagnostics.is_empty());
    let model = parsed.assemble()?;
    assert_eq!(model.oracle_bindings().len(), 2);
    for b in model.oracle_bindings() {
        if b.receiver.decoded_name.as_deref() == Some("xs") {
            assert_eq!(b.receiver.state, S::Indeterminate);
        } else {
            assert_eq!(b.receiver.decoded_name.as_deref(), Some("ys"));
            assert_eq!(b.receiver.state, S::Resolved);
            assert_eq!(b.classification, C::CollectionIndex);
        }
    }
    Ok(())
}

#[test]
fn oracle_collection_members_bind_native_accessors_and_parameters() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingState as S, AntlrOracleBoundKind as B,
        AntlrOracleCollectionMethodKind as K,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    for definition in [
        "TABLE OF NUMBER",
        "VARRAY(8) OF NUMBER",
        "TABLE OF NUMBER INDEX BY PLS_INTEGER",
    ] {
        let source = format!(
            "DECLARE TYPE values_t IS {definition}; xs values_t; n NUMBER; b BOOLEAN; PROCEDURE p(arg values_t) IS BEGIN n:=arg.COUNT; END; BEGIN n:=xs.CoUnT; n:=xs.FIRST; n:=xs.LAST; n:=xs.LIMIT; n:=xs.NEXT(1); b:=xs.EXISTS(1); END; /"
        );
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["members.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            let mut kinds = std::collections::BTreeSet::new();
            let mut parameters = 0;
            let mut next_provenance = std::collections::BTreeSet::new();
            let members: Vec<_> = model
                .oracle_applications()
                .iter()
                .filter(|application| application.member.is_some())
                .collect();
            assert_eq!(members.len(), 8, "{definition}");
            for application in members {
                let member = application.member.as_ref().unwrap();
                let binding = model.oracle_binding(application.occurrence_index).unwrap();
                assert_eq!(
                    binding.receiver.state,
                    S::Resolved,
                    "{:?}",
                    binding.receiver.issues
                );
                assert_eq!(
                    binding.classification,
                    C::CollectionMethod,
                    "{:?}",
                    binding.issues
                );
                assert!(binding.issues.is_empty());
                assert_eq!(binding.declared_type.as_ref().unwrap().state, S::Resolved);
                assert!(binding.member_continuity.as_ref().unwrap().verified);
                assert!(!binding.type_evidence.is_empty());
                assert_eq!(member.separator.as_ref().unwrap().node.kind, "PERIOD");
                assert_eq!(
                    application_source(member.receiver.as_ref().unwrap()),
                    Some(if binding.receiver.candidates[0].kind == B::Parameter {
                        "arg"
                    } else {
                        "xs"
                    })
                );
                assert!(member.name.is_some());
                let kind = binding.collection_method.unwrap();
                kinds.insert(format!("{kind:?}"));
                parameters += usize::from(binding.receiver.candidates[0].kind == B::Parameter);
                if matches!(kind, K::Count | K::First | K::Last | K::Limit) {
                    assert!(application.groups.is_empty());
                    assert!(member.delimiters.is_empty());
                } else {
                    assert_eq!(application.groups.len(), 1);
                    assert_eq!(application.groups[0].arguments.len(), 1);
                    assert_eq!(
                        member
                            .delimiters
                            .iter()
                            .map(|delimiter| delimiter.node.kind.as_str())
                            .collect::<Vec<_>>(),
                        ["LEFT_PAREN", "RIGHT_PAREN"]
                    );
                }
                if kind == K::Next {
                    next_provenance.insert(
                        parsed.observations().occurrences[application.occurrence_index].query_index,
                    );
                }
            }
            assert_eq!(parameters, 1);
            assert_eq!(
                next_provenance.len(),
                2,
                "general and terminal query occurrences survive"
            );
            assert_eq!(
                kinds,
                ["Count", "First", "Last", "Limit", "Next", "Exists"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn oracle_collection_members_reject_unproven_receivers_and_invocations() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingIssue as I,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    for (declarations, operation) in [
        (
            "TYPE values_t IS RECORD (COUNT NUMBER); xs values_t;",
            "n:=xs.COUNT;",
        ),
        ("xs NUMBER;", "n:=xs.COUNT;"),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "DECLARE xs NUMBER; BEGIN n:=xs.COUNT; END;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; TYPE values_t IS VARRAY(2) OF NUMBER; xs values_t;",
            "n:=xs.COUNT;",
        ),
        ("xs outside_t;", "n:=xs.COUNT;"),
        (
            "xs later_t; TYPE later_t IS TABLE OF NUMBER;",
            "n:=xs.COUNT;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; SUBTYPE alias_t IS values_t; xs alias_t;",
            "n:=xs.COUNT;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.COUNT();",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.COUNT(1);",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.NEXT;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.NEXT();",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.NEXT(index_name=>1);",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.NEXT(1,2);",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.EXISTS;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=values_t.COUNT;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.COUNT.NEXT(1);",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=(xs.COUNT).NEXT(1);",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=((xs.COUNT)).NEXT(1);",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=(xs.COUNT).another_member;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.inner_value.COUNT;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs(1).COUNT;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "n:=xs.COUNT@remote;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "xs.COUNT:=1;",
        ),
        (
            "TYPE values_t IS TABLE OF NUMBER; xs values_t;",
            "SELECT xs.COUNT INTO n FROM dual;",
        ),
    ] {
        let source = format!("DECLARE {declarations} n NUMBER; BEGIN {operation} END; /");
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["unproven-member.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{source}: {:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            let members: Vec<_> = model
                .oracle_applications()
                .iter()
                .filter(|application| application.member.is_some())
                .collect();
            assert!(!members.is_empty(), "{source}");
            for application in members {
                let binding = model.oracle_binding(application.occurrence_index).unwrap();
                assert_eq!(
                    binding.classification,
                    C::Unresolved,
                    "{source}: {:?}",
                    binding.issues
                );
                assert!(binding.collection_method.is_none());
                if operation.contains("COUNT.NEXT") {
                    assert!(
                        application.member.as_ref().unwrap().receiver.is_none(),
                        "do not skip COUNT"
                    );
                }
                if operation == "xs.COUNT:=1;" {
                    assert!(binding.issues.contains(&I::InvalidContext));
                }
                if [
                    "n:=xs.COUNT();",
                    "n:=xs.COUNT(1);",
                    "n:=xs.NEXT;",
                    "n:=xs.NEXT();",
                    "n:=xs.NEXT(index_name=>1);",
                    "n:=xs.NEXT(1,2);",
                    "n:=xs.EXISTS;",
                ]
                .contains(&operation)
                {
                    assert!(
                        binding.issues.contains(&I::UnsupportedInvocation),
                        "{source}: {:?}",
                        binding.issues
                    );
                }
            }
            Ok(())
        })?;
    }
    for operation in ["xs(xs.COUNT):=1;", "n:=(xs.COUNT);", "n:=sink((xs.COUNT));"] {
        let source = format!(
            "DECLARE TYPE values_t IS TABLE OF NUMBER; xs values_t; n NUMBER; BEGIN {operation} END; /"
        );
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["member-index.sql"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            let application = model
                .oracle_applications()
                .iter()
                .find(|application| application.member.is_some())
                .unwrap();
            let binding = model.oracle_binding(application.occurrence_index).unwrap();
            assert_eq!(
                binding.classification,
                C::CollectionMethod,
                "{operation}: {:?}",
                binding.issues
            );
            assert!(
                !application
                    .assignment
                    .as_ref()
                    .unwrap()
                    .expressions
                    .is_empty()
            );
            Ok(())
        })?;
    }
    let query = "  - {data_key: calls, graph_label: '', pattern: '//general_element_part', type: relation, relation_type: CALLS, captures: {name: [{xpath: '/id_expression'}], qualifier: [{nearest_ancestor: 'general_element'}]}}\n";
    let custom_draft = custom(query);
    for spelling in ["\"COUNT\"", "\"count\""] {
        let source = format!(
            "DECLARE TYPE values_t IS TABLE OF NUMBER; xs values_t; n NUMBER; BEGIN n:=xs.{spelling}; END; /"
        );
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &custom_draft.profiles()[0],
                input,
                &["quoted-member.sql"],
                limits(),
            )?;
            let model = parsed.assemble()?;
            let members: Vec<_> = model
                .oracle_applications()
                .iter()
                .filter(|application| application.member.is_some())
                .collect();
            assert_eq!(members.len(), 1);
            for application in members {
                let binding = model.oracle_binding(application.occurrence_index).unwrap();
                assert_eq!(binding.classification, C::Unresolved);
                assert!(binding.issues.contains(&I::UnsupportedReference));
                assert!(
                    application_source(application.member.as_ref().unwrap().name.as_ref().unwrap())
                        .unwrap()
                        .starts_with('"')
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn oracle_collection_members_bind_without_entity_queries_and_keep_duplicates() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingState as S,
        AntlrOracleCollectionMethodKind as K,
    };
    let relation = "  - {data_key: calls, graph_label: '', pattern: '//general_element', child_rule: general_element, select: '/general_element_part/id_expression/regular_id/non_reserved_keywords_pre12c/COUNT', captures: {qualifier: [{nearest_ancestor: general_element}]}, type: relation, relation_type: CALLS}\n";
    let draft = custom(
        &(relation.to_owned()
            + &relation.replace("data_key: calls", "data_key: alternative_calls")),
    );
    let source = "DECLARE TYPE values_t IS TABLE OF NUMBER; xs values_t; n NUMBER; BEGIN n:=xs.COUNT; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["no-entity-members.sql"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        assert!(model.entities().is_empty());
        assert_eq!(model.oracle_bindings().len(), 2);
        let mut keys = std::collections::BTreeSet::new();
        for binding in model.oracle_bindings() {
            assert_eq!(binding.receiver.state, S::Resolved);
            assert_eq!(binding.classification, C::CollectionMethod);
            assert_eq!(binding.collection_method, Some(K::Count));
            assert!(binding.receiver.candidates[0].entity_indices.is_empty());
            assert!(
                binding.declared_type.as_ref().unwrap().candidates[0]
                    .entity_indices
                    .is_empty()
            );
            assert!(!binding.type_evidence.is_empty());
            keys.insert(
                parsed.observations().occurrences[binding.occurrence_index]
                    .query
                    .data_key()
                    .value()
                    .as_str(),
            );
        }
        assert_eq!(
            keys,
            std::collections::BTreeSet::from(["calls", "alternative_calls"])
        );
        assert_eq!(
            model.oracle_applications()[0].head.node_index,
            model.oracle_applications()[1].head.node_index
        );
        Ok(())
    })
}

#[test]
fn oracle_collection_members_require_integral_continuity_and_usable_fields() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleApplicationClass as C, AntlrOracleBindingIssue as I,
        AntlrOracleBindingState as S,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "DECLARE TYPE values_t IS TABLE OF NUMBER; xs values_t; n NUMBER; BEGIN n:=xs.COUNT; END; /";
    let check =
        |input: &PreparedInput<'_, '_, '_, '_>, verified: bool, resolved: bool| -> TestResult {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["member-origin.sql"],
                limits(),
            )?;
            assert!(parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            let application = model
                .oracle_applications()
                .iter()
                .find(|application| application.member.is_some())
                .unwrap();
            let binding = model.oracle_binding(application.occurrence_index).unwrap();
            assert_eq!(
                binding.member_continuity.as_ref().unwrap().verified,
                verified
            );
            assert_eq!(binding.receiver.state == S::Resolved, resolved);
            assert_eq!(
                binding.classification,
                if verified && resolved {
                    C::CollectionMethod
                } else {
                    C::Unresolved
                }
            );
            if !verified {
                assert!(
                    binding.issues.contains(&I::NonExactOrigin),
                    "{:?}",
                    binding.issues
                );
            }
            if verified && resolved {
                assert!(matches!(
                    application.member.as_ref().unwrap().syntax.original,
                    Some(OriginalProjection::Compound(_))
                ));
            }
            Ok(())
        };
    let dot = source.find(".COUNT").unwrap();
    let whole = ByteSpan {
        start: 0,
        end: source.len() as u64,
    };
    let original = context();
    let split = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: dot as u64,
            },
            original: ByteSpan {
                start: 0,
                end: dot as u64,
            },
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: dot as u64,
                end: whole.end,
            },
            original: ByteSpan {
                start: dot as u64,
                end: whole.end,
            },
        },
    ];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &split,
            removed_original: &[],
            anchors: &[],
        },
        &original,
    )?;
    check(&input, true, true)?;
    for (start, end) in [(dot, dot + 1), (dot + 1, dot + 6)] {
        let removed = [ByteSpan {
            start: start as u64,
            end: end as u64,
        }];
        let segments = [
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
                original: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
            },
            OriginSegment::Synthetic {
                prepared: removed[0],
                reason: SyntheticReason::Wrapper,
            },
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: end as u64,
                    end: whole.end,
                },
                original: ByteSpan {
                    start: end as u64,
                    end: whole.end,
                },
            },
        ];
        let input = PreparedInput::new(
            source.as_bytes(),
            source.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &removed,
                anchors: &[],
            },
            &original,
        )?;
        check(&input, false, true)?;
    }
    let comment = "/*removed*/";
    let original_code = format!("{}{comment}{}", &source[..dot], &source[dot..]);
    let removed = [ByteSpan {
        start: dot as u64,
        end: (dot + comment.len()) as u64,
    }];
    let segments = [
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: 0,
                end: dot as u64,
            },
            original: ByteSpan {
                start: 0,
                end: dot as u64,
            },
        },
        OriginSegment::Direct {
            prepared: ByteSpan {
                start: dot as u64,
                end: whole.end,
            },
            original: ByteSpan {
                start: (dot + comment.len()) as u64,
                end: original_code.len() as u64,
            },
        },
    ];
    let input = PreparedInput::new(
        original_code.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &removed,
            anchors: &[],
        },
        &original,
    )?;
    check(&input, false, true)?;
    for changed in [true, false] {
        let start = if changed { dot + 1 } else { dot - 2 };
        let end = dot + 6;
        let original_code = if changed {
            source.replacen("xs.COUNT", "xs.XOUNT", 1)
        } else {
            source.to_owned()
        };
        let fragments = [ByteSpan {
            start: start as u64,
            end: end as u64,
        }];
        let segments = [
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
                original: ByteSpan {
                    start: 0,
                    end: start as u64,
                },
            },
            OriginSegment::MappedAtom {
                prepared: fragments[0],
                original_fragments: &fragments,
            },
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: end as u64,
                    end: whole.end,
                },
                original: ByteSpan {
                    start: end as u64,
                    end: whole.end,
                },
            },
        ];
        let input = PreparedInput::new(
            original_code.as_bytes(),
            source.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &original,
        )?;
        check(&input, !changed, changed)?;
    }
    let recovery = "DECLARE TYPE values_t IS TABLE OF NUMBER; xs values_t; n NUMBER; BEGIN n:=xs.NEXT(,); keep_call(1); END; /";
    with_input(recovery, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["member-recovery.sql"],
            limits(),
        )?;
        assert!(!parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        assert!(
            model
                .oracle_applications()
                .iter()
                .any(
                    |application| application_source(&application.head) == Some("keep_call")
                        && application.groups.len() == 1
                        && application.groups[0].arguments.len() == 1
                )
        );
        for application in model
            .oracle_applications()
            .iter()
            .filter(|application| application.member.is_some())
        {
            let binding = model.oracle_binding(application.occurrence_index).unwrap();
            assert_eq!(binding.classification, C::Unresolved);
            assert!(binding.collection_method.is_none());
        }
        Ok(())
    })?;
    Ok(())
}
#[test]
fn oracle_calls_native_source_and_candidate_contract() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleBindingState as S, AntlrOracleBoundKind as K,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = r#"DECLARE
 FUNCTION helper(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END;
 PROCEDURE sink(v NUMBER) IS BEGIN NULL; END;
 PROCEDURE caller IS x NUMBER;
 BEGIN x := helper(1); sink(2); END;
BEGIN caller; END;
/"#;
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["local-calls.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let nodes = &parsed.observations().syntax.nodes;
        let caller = model
            .entities()
            .iter()
            .find(|e| e.name() == "caller")
            .expect("native caller entity");
        assert_eq!(
            nodes[caller.declaration_node_index()].kind,
            "procedure_body"
        );
        let bodies = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent == Some(caller.declaration_node_index()) && n.kind == "body")
            .collect::<Vec<_>>();
        assert_eq!(bodies.len(), 1, "one native body directly owned by caller");
        {
            let (name, native_kind) = ("helper", "function_body");
            assert!(
                model
                    .oracle_bindings()
                    .iter()
                    .any(|b| b.receiver.decoded_name.as_deref() == Some(name)),
                "{name}: {:?}",
                model
                    .oracle_bindings()
                    .iter()
                    .map(|b| {
                        let a = model.oracle_application(b.occurrence_index).unwrap();
                        (
                            b.occurrence_index,
                            a.kind,
                            a.head.node.kind.as_str(),
                            a.head.original.and_then(|o| {
                                if let OriginalProjection::Exact(r) = o {
                                    Some(r.text())
                                } else {
                                    None
                                }
                            }),
                            b.receiver.decoded_name.as_deref(),
                            b.receiver.state,
                            &b.receiver.issues,
                        )
                    })
                    .collect::<Vec<_>>()
            );
            let binding = model
                .oracle_bindings()
                .iter()
                .find(|b| b.receiver.decoded_name.as_deref() == Some(name))
                .expect("native callable lookup");
            assert_eq!(
                binding.receiver.state,
                S::Resolved,
                "{name}: {:?}",
                binding.receiver.issues
            );
            assert_eq!(binding.receiver.candidates.len(), 1);
            let candidate = &binding.receiver.candidates[0];
            assert_eq!(candidate.kind, K::Other);
            assert_eq!(candidate.declaration.node.kind, native_kind);
            assert!(!candidate.entity_indices.is_empty());
            assert!(
                candidate
                    .entity_indices
                    .iter()
                    .all(|&i| model.entities()[i].declaration_node_index()
                        == candidate.declaration.node_index)
            );
            assert!(
                binding
                    .receiver
                    .scope_continuity
                    .iter()
                    .all(|proof| proof.verified)
            );
            let application = model
                .oracle_application(binding.occurrence_index)
                .expect("retained application");
            let owner = application
                .boundaries
                .iter()
                .find(|s| {
                    matches!(
                        s.node.kind.as_str(),
                        "function_body"
                            | "procedure_body"
                            | "create_function_body"
                            | "create_procedure_body"
                    )
                })
                .expect("native callable ancestor");
            assert_eq!(owner.node_index, caller.declaration_node_index());
            assert!(
                application
                    .boundaries
                    .iter()
                    .any(|s| s.node_index == bodies[0].0)
            );
            assert!(matches!(
                application.head.original,
                Some(OriginalProjection::Exact(_))
            ));
        }
        let sink_application = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(a.head.original,
            Some(OriginalProjection::Exact(r)) if r.text() == "sink")
            })
            .expect("native routine_name sink");
        assert_eq!(
            sink_application.kind,
            graphit_ast::antlr_engine::model::AntlrOracleApplicationKind::RoutineName
        );
        let sink_b1 = model
            .oracle_binding(sink_application.occurrence_index)
            .unwrap();
        assert_eq!(sink_b1.receiver.state, S::Indeterminate);
        assert_eq!(
            sink_b1.receiver.issues,
            [graphit_ast::antlr_engine::model::AntlrOracleBindingIssue::UnsupportedReference]
        );
        let sink_call = model
            .oracle_call(sink_application.occurrence_index)
            .expect("CALLS supplements routine_name");
        let lookup = sink_call
            .call_lookup
            .as_ref()
            .expect("retained supplemental lookup");
        assert_eq!(lookup.state, S::Resolved);
        assert_eq!(lookup.decoded_name.as_deref(), Some("sink"));
        for (name, text) in [("helper", "helper(1)"), ("sink", "sink")] {
            let app = model
                .oracle_applications()
                .iter()
                .find(|a| {
                    matches!(a.head.original,
                Some(OriginalProjection::Exact(r)) if r.text() == text)
                })
                .unwrap();
            let call = model
                .oracle_call(app.occurrence_index)
                .expect("local callable outcome");
            let graphit_ast::antlr_engine::model::AntlrOracleCallSourceState::Bound {
                entity_index: source,
            } = call.source
            else {
                panic!("{:?}", call.source);
            };
            assert_eq!(model.entities()[source].name(), "caller");
            let graphit_ast::antlr_engine::model::AntlrOracleCallTargetState::Resolved {
                entity_index: target,
            } = call.target
            else {
                panic!("{:?}", call.target);
            };
            assert_eq!(model.entities()[target].name(), name);
            assert_eq!(call.group_outcomes.len(), app.groups.len());
            assert_eq!(
                call.call_index,
                model
                    .oracle_calls()
                    .iter()
                    .position(|c| c.occurrence_index == app.occurrence_index)
                    .unwrap()
            );
        }

        Ok(())
    })?;
    let source = "CREATE FUNCTION recur(x NUMBER) RETURN NUMBER IS BEGIN IF x = 0 THEN RETURN x; ELSE RETURN recur(x-1); END IF; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["self-call.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let entity = model
            .entities()
            .iter()
            .find(|e| e.name() == "recur")
            .expect("native recursive function");
        let binding = model
            .oracle_bindings()
            .iter()
            .find(|b| b.receiver.decoded_name.as_deref() == Some("recur"))
            .expect("retained self reference");
        assert_eq!(
            binding.receiver.state,
            S::Missing,
            "B1 does not invent a global callable candidate: {:?}",
            binding.receiver.issues
        );
        let application = model.oracle_application(binding.occurrence_index).unwrap();
        assert!(
            application
                .boundaries
                .iter()
                .any(|s| s.node_index == entity.declaration_node_index())
        );
        assert!(application.boundaries.iter().any(|s| s.node.kind == "body"));
        assert_eq!(application.groups.len(), 1);
        assert_eq!(application.groups[0].arguments.len(), 1);
        let call = model
            .oracle_call(application.occurrence_index)
            .expect("self-recursion outcome");
        let graphit_ast::antlr_engine::model::AntlrOracleCallSourceState::Bound {
            entity_index: source,
        } = call.source
        else {
            panic!("{:?}", call.source);
        };
        assert_eq!(
            call.target,
            graphit_ast::antlr_engine::model::AntlrOracleCallTargetState::Resolved {
                entity_index: source
            }
        );

        Ok(())
    })
}

#[test]
fn oracle_calls_keep_nested_callers_groups_and_noncallable_bindings() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleCallReason as Reason, AntlrOracleCallSourceState as Source,
        AntlrOracleCallTargetState as Target,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = r#"DECLARE
 TYPE nums IS TABLE OF NUMBER; items nums;
 FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END;
 PROCEDURE outer_p IS
   PROCEDURE inner_p IS x NUMBER;
   BEGIN x := f(1)(2); items(1) := f(3); END;
 BEGIN inner_p; END;
BEGIN outer_p; END; /"#;
    with_input(source, &context(), |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["nested.sql"], limits())?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let application = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(
                    a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(1)(2)"
                )
            })
            .expect("native repeated argument groups");
        let call = model.oracle_call(application.occurrence_index).unwrap();
        let Source::Bound { entity_index } = call.source else {
            panic!("{:?}", call.source);
        };
        assert_eq!(model.entities()[entity_index].name(), "inner_p");
        let Target::Resolved { entity_index } = call.target else {
            panic!("{:?}", call.target);
        };
        assert_eq!(model.entities()[entity_index].name(), "f");
        assert_eq!(call.group_outcomes.len(), 2);
        assert!(matches!(
            call.group_outcomes[0].target,
            Target::Resolved { .. }
        ));
        assert_eq!(call.group_outcomes[1].group_index, 1);
        assert_eq!(
            call.group_outcomes[1].target,
            Target::Unresolved {
                reason: Reason::UnsupportedInvocation
            }
        );
        assert_eq!(
            call.group_outcomes[1].syntax.node_index,
            application.groups[1].syntax.node_index
        );
        let collection = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(
                    a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "items(1)"
                )
            })
            .unwrap();
        assert!(
            model.oracle_call(collection.occurrence_index).is_none(),
            "collection LHS is not a callable"
        );
        let inner = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(
                    a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "inner_p"
                )
            })
            .unwrap();
        let call = model.oracle_call(inner.occurrence_index).unwrap();
        let Source::Bound { entity_index } = call.source else {
            panic!("{:?}", call.source);
        };
        assert_eq!(model.entities()[entity_index].name(), "outer_p");
        let outer = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(
                    a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "outer_p"
                )
            })
            .unwrap();
        assert!(matches!(
            model.oracle_call(outer.occurrence_index).unwrap().source,
            Source::Unavailable {
                reason: Reason::MissingCaller
            }
        ));
        Ok(())
    })?;
    let source = "DECLARE FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE p IS f NUMBER; x NUMBER; BEGIN x := f(1); END; BEGIN p; END; /";
    with_input(source, &context(), |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["shadow.sql"], limits())?;
        let model = parsed.assemble()?;
        let app = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(
                    a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(1)"
                )
            })
            .unwrap();
        let binding = model.oracle_binding(app.occurrence_index).unwrap();
        assert_eq!(
            binding.receiver.state,
            graphit_ast::antlr_engine::model::AntlrOracleBindingState::Resolved
        );
        assert_eq!(
            binding.receiver.candidates[0].kind,
            graphit_ast::antlr_engine::model::AntlrOracleBoundKind::Value
        );
        assert!(
            model.oracle_call(app.occurrence_index).is_none(),
            "inner scalar shadows callable without fallback"
        );
        Ok(())
    })
}

#[test]
fn oracle_calls_preserve_ambiguity_names_and_declaration_boundaries() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleCallReason as Reason, AntlrOracleCallSourceState as Source,
        AntlrOracleCallTargetState as Target,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "DECLARE FUNCTION f(v NUMBER) RETURN NUMBER; FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE p IS x NUMBER; BEGIN x := f(1); absent; END; BEGIN p; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["ambiguous.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let app = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(
                    a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(1)"
                )
            })
            .unwrap();
        let Target::Ambiguous { entity_indices } =
            &model.oracle_call(app.occurrence_index).unwrap().target
        else {
            panic!("spec/body must not be coalesced by spelling");
        };
        assert!(entity_indices.len() >= 2);
        for &i in entity_indices {
            assert_eq!(model.entities()[i].name(), "f");
        }
        let absent = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(
                    a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "absent"
                )
            })
            .unwrap();
        assert_eq!(
            model.oracle_call(absent.occurrence_index).unwrap().target,
            Target::NotFound
        );
        Ok(())
    })?;
    let source = r#"DECLARE FUNCTION "Foo"(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END;
 PROCEDURE p IS x NUMBER; BEGIN x := "Foo"(1); x := foo(2); x := "FOO"(3); END;
BEGIN p; END; /"#;
    with_input(source, &context(), |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["quoted.sql"], limits())?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        for (text, resolved) in [
            ("\"Foo\"(1)", true),
            ("foo(2)", false),
            ("\"FOO\"(3)", false),
        ] {
            let app = model
                .oracle_applications()
                .iter()
                .find(|a| {
                    matches!(
                        a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == text
                    )
                })
                .unwrap();
            let target = &model.oracle_call(app.occurrence_index).unwrap().target;
            if resolved {
                let Target::Resolved { entity_index } = target else {
                    panic!("{target:?}");
                };
                assert_eq!(model.entities()[*entity_index].name(), "Foo");
            } else {
                assert_eq!(*target, Target::NotFound);
            }
        }
        Ok(())
    })?;
    let source = "CREATE PROCEDURE p(a NUMBER DEFAULT unknown()) IS x NUMBER := unknown(); BEGIN x := unknown(); END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["boundaries.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let mut body = 0;
        let mut outside = 0;
        for call in model.oracle_calls() {
            match call.source {
                Source::Bound { entity_index } => {
                    assert_eq!(model.entities()[entity_index].name(), "p");
                    body += 1;
                }
                Source::Unavailable {
                    reason: Reason::OutsideBody,
                } => outside += 1,
                ref state => panic!("unexpected native source: {state:?}"),
            }
        }
        assert_eq!((body, outside), (1, 2));
        Ok(())
    })
}

#[test]
fn oracle_calls_require_contiguous_original_and_preserve_cancellation() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleCallReason as Reason, AntlrOracleCallSourceState as Source,
        AntlrOracleCallTargetState as Target,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "DECLARE FUNCTION helper(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE caller IS x NUMBER; BEGIN x := helper(1); END; BEGIN caller; END; /";
    let marker = source.find("BEGIN x").unwrap();
    let split = marker + 5;
    for synthetic in [false, true] {
        let original = context();
        let middle_end = split + 1;
        let segment = if synthetic {
            OriginSegment::Synthetic {
                prepared: ByteSpan {
                    start: marker as u64,
                    end: middle_end as u64,
                },
                reason: SyntheticReason::Wrapper,
            }
        } else {
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: marker as u64,
                    end: middle_end as u64,
                },
                original: ByteSpan {
                    start: marker as u64,
                    end: middle_end as u64,
                },
            }
        };
        let segments = [
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: 0,
                    end: marker as u64,
                },
                original: ByteSpan {
                    start: 0,
                    end: marker as u64,
                },
            },
            segment,
            OriginSegment::Direct {
                prepared: ByteSpan {
                    start: middle_end as u64,
                    end: source.len() as u64,
                },
                original: ByteSpan {
                    start: middle_end as u64,
                    end: source.len() as u64,
                },
            },
        ];
        let removed = if synthetic {
            vec![ByteSpan {
                start: marker as u64,
                end: middle_end as u64,
            }]
        } else {
            vec![]
        };
        let prepared = source.to_owned();
        let input = PreparedInput::new(
            source.as_bytes(),
            prepared.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &removed,
                anchors: &[],
            },
            &original,
        )?;
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            &input,
            &["call-origin.sql"],
            limits(),
        )?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let app = model.oracle_applications().iter().find(|a| matches!(a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "helper(1)")).unwrap();
        let call = model.oracle_call(app.occurrence_index).unwrap();
        if synthetic {
            assert!(matches!(
                call.source,
                Source::Unavailable {
                    reason: Reason::NonExactOrigin
                }
            ));
            assert!(matches!(
                call.target,
                Target::Unresolved {
                    reason: Reason::NonExactOrigin
                }
            ));
        } else {
            assert!(matches!(call.source, Source::Bound { .. }));
            assert!(matches!(call.target, Target::Resolved { .. }));
            assert!(
                matches!(
                    call.caller.unwrap().original,
                    Some(OriginalProjection::Compound(_))
                ),
                "structural contiguous Compound is supported"
            );
        }
        original.cancellation().cancel();
        assert!(
            matches!(parsed.assemble(), Err(AntlrModelError::Operation(e)) if e.kind() == ErrorKind::Interrupted)
        );
    }
    Ok(())
}

#[test]
fn oracle_calls_reject_direct_lhs_and_enforce_target_rule_for_ambiguity() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleCallReason as Reason, AntlrOracleCallTargetState as Target,
    };
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let source = "DECLARE TYPE nums IS TABLE OF NUMBER; xs nums; FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE p IS BEGIN f(1) := 2; xs(f(2)) := 3; END; BEGIN p; END; /";
    with_input(source, &context(), |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["lhs.sql"], limits())?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let app = model.oracle_applications().iter().find(|a| matches!(a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(1)")).unwrap();
        let lhs = app.assignment.as_ref().unwrap();
        assert_eq!(
            lhs.side,
            graphit_ast::antlr_engine::model::AntlrOracleAssignmentSide::Target
        );
        assert!(lhs.expressions.is_empty(), "direct native assignment head");
        assert_eq!(
            model.oracle_call(app.occurrence_index).unwrap().target,
            Target::Unresolved {
                reason: Reason::UnsupportedInvocation
            }
        );
        let inner = model.oracle_applications().iter().find(|a| matches!(a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(2)")).unwrap();
        assert!(!inner.assignment.as_ref().unwrap().expressions.is_empty());
        assert!(
            matches!(
                model.oracle_call(inner.occurrence_index).unwrap().target,
                Target::Resolved { .. }
            ),
            "real invocation inside an index remains a call"
        );
        Ok(())
    })?;
    let stock = std::str::from_utf8(include_bytes!("fixtures/catalog/shipped/plsql.yaml"))?;
    let wrong_policy = stock.replace(
        "  CALLS: { labels: [Function, Procedure, Package] }",
        "  CALLS: { labels: [Package] }",
    );
    assert_ne!(stock, wrong_policy);
    let draft = catalog(wrong_policy.as_bytes());
    let source = "DECLARE FUNCTION f(v NUMBER) RETURN NUMBER; FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE p IS x NUMBER; BEGIN x := f(1); END; BEGIN p; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["target-rule.sql"],
            limits(),
        )?;
        let model = parsed.assemble()?;
        let app = model.oracle_applications().iter().find(|a| matches!(a.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(1)")).unwrap();
        assert_eq!(
            model
                .oracle_binding(app.occurrence_index)
                .unwrap()
                .receiver
                .state,
            graphit_ast::antlr_engine::model::AntlrOracleBindingState::Ambiguous
        );
        assert_eq!(
            model.oracle_call(app.occurrence_index).unwrap().target,
            Target::Unresolved {
                reason: Reason::UnsupportedPolicy
            }
        );
        Ok(())
    })
}

#[test]
fn oracle_calls_preserve_supported_data_keys_and_reject_qualified_targets() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleCallSourceState as Source, AntlrOracleCallTargetState as Target,
    };
    let stock = std::str::from_utf8(include_bytes!("fixtures/catalog/shipped/plsql.yaml"))?;
    let profile = stock
        .replace("data_key: functions", "data_key: local_functions")
        .replace("data_key: procedures", "data_key: local_procedures");
    let draft = catalog(profile.as_bytes());
    let source = "DECLARE FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE p IS x NUMBER; BEGIN x := f(1); x := pkg.f(2); x := f@remote(3); END; BEGIN p; END; /";
    with_input(source, &context(), |input| {
        let parsed = ParsedAntlrDocument::collect(
            &draft.profiles()[0],
            input,
            &["configured-keys.sql"],
            limits(),
        )?;
        assert!(
            parsed.observations().syntax.diagnostics.is_empty(),
            "{:?}",
            parsed.observations().syntax.diagnostics
        );
        let model = parsed.assemble()?;
        let mut local = 0;
        let mut qualified = 0;
        for call in model.oracle_calls() {
            let app = model.oracle_application(call.occurrence_index).unwrap();
            if matches!(app.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(1)")
            {
                let Target::Resolved {
                    entity_index: target,
                } = call.target
                else {
                    panic!("{:?}", call.target);
                };
                let Source::Bound {
                    entity_index: source,
                } = call.source
                else {
                    panic!("{:?}", call.source);
                };
                assert_eq!(model.entities()[target].data_key(), "local_functions");
                assert_eq!(model.entities()[source].data_key(), "local_procedures");
                local += 1;
            } else if matches!(app.head.original, Some(OriginalProjection::Exact(r)) if r.text() == "f(2)" || r.text() == "f@remote(3)")
            {
                assert!(
                    matches!(call.target, Target::Unresolved { .. }),
                    "no local-name fallback for qualified/dblink"
                );
                qualified += 1;
            }
        }
        assert_eq!((local, qualified), (1, 2));
        Ok(())
    })
}

#[test]
fn oracle_calls_complete_policy_multiplicity_visibility_and_name_contracts() -> TestResult {
    use graphit_ast::antlr_engine::model::{
        AntlrOracleBoundKind as Kind, AntlrOracleCallReason as Reason,
        AntlrOracleCallSourceState as Source, AntlrOracleCallTargetState as Target,
    };
    let stock = std::str::from_utf8(include_bytes!("fixtures/catalog/shipped/plsql.yaml"))?;
    let source = "DECLARE FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE p IS x NUMBER; BEGIN x := f(1); END; BEGIN p; END; /";
    let no_rule = stock.replace("  CALLS: { labels: [Function, Procedure, Package] }\n", "");
    assert_ne!(stock, no_rule);
    let function_query = "  - data_key: functions\n    graph_label: Function\n    pattern: \"//function_body/identifier\"\n    type: entity\n";
    assert_eq!(stock.matches(function_query).count(), 1);
    let no_function = stock.replace(function_query, "");
    let extra_function = "queries:\n  - {data_key: alternate_functions, graph_label: Function, pattern: '//function_body/identifier', type: entity}\n";
    let extra_procedure = "queries:\n  - {data_key: alternate_procedures, graph_label: Procedure, pattern: '//procedure_body/identifier', type: entity}\n";
    for (profile, expected) in [
        (no_rule, "missing_rule"),
        (no_function, "missing_entity"),
        (stock.replace("queries:\n", extra_function), "target_rows"),
        (stock.replace("queries:\n", extra_procedure), "source_rows"),
    ] {
        let draft = catalog(profile.as_bytes());
        with_input(source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["complete-policy.sql"],
                limits(),
            )?;
            assert!(parsed.observations().syntax.diagnostics.is_empty());
            let model = parsed.assemble()?;
            let app = model
                .oracle_applications()
                .iter()
                .find(|a| {
                    matches!(a.head.original,
                Some(OriginalProjection::Exact(r)) if r.text() == "f(1)")
                })
                .unwrap();
            let call = model.oracle_call(app.occurrence_index).unwrap();
            match expected {
                "missing_rule" => assert_eq!(
                    call.target,
                    Target::Unresolved {
                        reason: Reason::MissingTargetRule
                    }
                ),
                "missing_entity" => {
                    assert!(model.entities().iter().all(|e| e.name() != "f"));
                    assert_eq!(
                        call.target,
                        Target::Unresolved {
                            reason: Reason::MissingEntity
                        }
                    );
                }
                "target_rows" => {
                    let Target::Ambiguous { entity_indices } = &call.target else {
                        panic!("{:?}", call.target);
                    };
                    assert_eq!(entity_indices.len(), 2);
                    let keys = entity_indices
                        .iter()
                        .map(|&i| model.entities()[i].data_key())
                        .collect::<std::collections::BTreeSet<_>>();
                    assert_eq!(
                        keys,
                        ["functions", "alternate_functions"].into_iter().collect()
                    );
                    let declaration = model.entities()[entity_indices[0]].declaration_node_index();
                    assert!(
                        entity_indices
                            .iter()
                            .all(|&i| model.entities()[i].declaration_node_index() == declaration)
                    );
                }
                "source_rows" => {
                    let Source::Ambiguous { entity_indices } = &call.source else {
                        panic!("{:?}", call.source);
                    };
                    assert_eq!(entity_indices.len(), 2);
                    assert_eq!(
                        entity_indices
                            .iter()
                            .map(|&i| model.entities()[i].data_key())
                            .collect::<std::collections::BTreeSet<_>>(),
                        ["procedures", "alternate_procedures"].into_iter().collect()
                    );
                    assert!(matches!(call.target, Target::Resolved { .. }));
                }
                _ => unreachable!(),
            }
            Ok(())
        })?;
    }
    let draft = catalog(stock.as_bytes());
    let later = "DECLARE PROCEDURE p IS x NUMBER; BEGIN x := f(1); END; FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; BEGIN p; END; /";
    with_input(later, &context(), |input| {
        let parsed =
            ParsedAntlrDocument::collect(&draft.profiles()[0], input, &["later.sql"], limits())?;
        assert!(parsed.observations().syntax.diagnostics.is_empty());
        let model = parsed.assemble()?;
        let app = model
            .oracle_applications()
            .iter()
            .find(|a| {
                matches!(a.head.original,
            Some(OriginalProjection::Exact(r)) if r.text() == "f(1)")
            })
            .unwrap();
        assert_eq!(
            model.oracle_call(app.occurrence_index).unwrap().target,
            Target::NotFound
        );
        Ok(())
    })?;
    for (declaration, reference, expected_name, expected_reason) in [
        ("ação", "AÇÃO", None, Some(Reason::DatabaseCharacterSet)),
        ("\"f\"\"x\"", "\"f\"\"x\"", Some("f\"x"), None),
    ] {
        let source = format!(
            "DECLARE FUNCTION {declaration}(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE p IS x NUMBER; BEGIN x := {reference}(1); END; BEGIN p; END; /"
        );
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["name-proof.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            let text = format!("{reference}(1)");
            let app = model
                .oracle_applications()
                .iter()
                .find(|a| {
                    matches!(a.head.original,
                Some(OriginalProjection::Exact(r)) if r.text() == text)
                })
                .unwrap();
            let binding = model.oracle_binding(app.occurrence_index).unwrap();
            if expected_reason == Some(Reason::DatabaseCharacterSet) {
                assert_eq!(
                    binding.receiver.state,
                    graphit_ast::antlr_engine::model::AntlrOracleBindingState::Indeterminate
                );
                assert!(binding.receiver.issues.contains(&graphit_ast::antlr_engine::model::AntlrOracleBindingIssue::DatabaseCharacterSet));
            }
            let call = model.oracle_call(app.occurrence_index).unwrap_or_else(|| {
                panic!(
                    "missing outcome for {reference}: {:?}, {:?}, {:?}",
                    binding.receiver.state,
                    binding.receiver.issues,
                    binding
                        .receiver
                        .candidates
                        .iter()
                        .map(|c| (
                            c.kind,
                            c.declaration.node.kind.as_str(),
                            c.decoded_name.as_deref()
                        ))
                        .collect::<Vec<_>>()
                )
            });
            if let Some(reason) = expected_reason {
                assert_eq!(call.target, Target::Unresolved { reason });
                assert!(call.binding_issues.contains(&graphit_ast::antlr_engine::model::AntlrOracleBindingIssue::DatabaseCharacterSet));
            } else {
                let Target::Resolved { entity_index } = call.target else {
                    panic!("{:?}", call.target);
                };
                assert_eq!(Some(model.entities()[entity_index].name()), expected_name);
            }
            Ok(())
        })?;
    }
    for (parameters, declarations, expected_kind) in [
        ("f NUMBER", "", Kind::Parameter),
        ("", "TYPE f IS TABLE OF NUMBER;", Kind::Type),
        ("", "CURSOR f IS SELECT 1 FROM dual;", Kind::Other),
    ] {
        let header = if parameters.is_empty() {
            "p".to_owned()
        } else {
            format!("p({parameters})")
        };
        let source = format!(
            "DECLARE FUNCTION f(v NUMBER) RETURN NUMBER IS BEGIN RETURN v; END; PROCEDURE {header} IS {declarations} x NUMBER; BEGIN x := f(1); END; BEGIN p; END; /"
        );
        with_input(&source, &context(), |input| {
            let parsed = ParsedAntlrDocument::collect(
                &draft.profiles()[0],
                input,
                &["shadow-kind.sql"],
                limits(),
            )?;
            assert!(
                parsed.observations().syntax.diagnostics.is_empty(),
                "{:?}",
                parsed.observations().syntax.diagnostics
            );
            let model = parsed.assemble()?;
            let app = model
                .oracle_applications()
                .iter()
                .find(|a| {
                    matches!(a.head.original,
                Some(OriginalProjection::Exact(r)) if r.text() == "f(1)")
                })
                .unwrap();
            let binding = model.oracle_binding(app.occurrence_index).unwrap();
            assert_eq!(binding.receiver.candidates.len(), 1);
            assert_eq!(binding.receiver.candidates[0].kind, expected_kind);
            assert!(
                model.oracle_call(app.occurrence_index).is_none(),
                "noncallable shadow cannot fall back to outer Function"
            );
            Ok(())
        })?;
    }
    Ok(())
}
