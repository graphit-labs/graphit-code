use graphit_ast::{
    antlr_engine::{
        AntlrEngineError, CaptureStep, Dialect, NodeClass, ParsedProfileQueries, ParsedSyntax,
        ProfileQuery, SpanStatus, parse, parse_profile_queries, parse_strict,
        semantic::{CaptureBinding, SemanticError, parse_semantic_occurrences},
    },
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput},
    profiles::{ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput},
    scalar_policy::{RegexLimits, ScalarPolicy},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    io::ErrorKind,
    time::{Duration, Instant},
};

const PARAMETER_TYPE: &str = "//param_decl_2/data_type_1";
const SCALAR_TYPE: &str = "//create_function_sql_scalar_table_or_row_statement/data_type_1";

fn operation() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}

fn function(parameters: &str, returns: &str, expression: &str) -> String {
    format!(
        "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION SCHEMA_F.F_SQL({parameters}) RETURNS {returns} LANGUAGE SQL BEGIN ATOMIC RETURN {expression}; END;"
    )
}

fn selected(pattern: &str) -> ProfileQuery {
    ProfileQuery {
        pattern: pattern.into(),
        captures: vec![Vec::new()],
        ..ProfileQuery::default()
    }
}

fn raw_matches<'s>(
    source: &'s str,
    parsed: &ParsedProfileQueries<'_>,
    query_index: usize,
) -> Vec<&'s str> {
    parsed
        .matches
        .iter()
        .filter(|matched| matched.query_index == query_index)
        .map(|matched| {
            let node = &parsed.syntax.nodes[matched.node_index];
            assert_eq!(matched.captures[0], Some(matched.node_index));
            assert_eq!(node.class, NodeClass::Rule);
            assert_eq!(node.status, SpanStatus::Regular);
            source.get(node.span.clone().unwrap()).unwrap()
        })
        .collect()
}

fn assert_native_source(source: &str, syntax: &ParsedSyntax<'_>) {
    assert!(
        syntax.diagnostics.is_empty(),
        "{source}: {:?}",
        syntax.diagnostics
    );
    for (index, node) in syntax.nodes.iter().enumerate() {
        assert!(!node.error && !node.missing, "{source}: {}", node.kind);
        assert!(node.parent.is_none_or(|parent| parent < index));
        if let Some(span) = &node.span {
            assert!(source.get(span.clone()).is_some());
        }
    }
    for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
        let token = syntax
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
    assert!(syntax.tokens.iter().any(|token| token.eof));
    for token in &syntax.tokens {
        if token.eof {
            assert_eq!(token.status, SpanStatus::Eof);
            assert_eq!(token.span, Some(source.len()..source.len()));
        } else if let (Some(span), Some(text)) = (&token.span, &token.text) {
            assert_eq!(source.get(span.clone()), Some(text.as_str()));
        }
    }
}

fn with_identity(
    source: &str,
    context: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>),
) {
    let bound = ByteSpan {
        start: 0,
        end: source.len() as u64,
    };
    let segments = [OriginSegment::Direct {
        prepared: bound,
        original: bound,
    }];
    let input = PreparedInput::new(
        source.as_bytes(),
        source.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        context,
    )
    .unwrap();
    action(&input);
}

fn shipped_catalog() -> CatalogDraft {
    let context = operation();
    let id = ProfileId::new("sql-function-types", 256).unwrap();
    let decoder = ProfileDecoder::new(
        ScalarPolicy::new(RegexLimits {
            compiled_size_policy_bytes: 1 << 20,
            dfa_cache_policy_bytes: 1 << 20,
        })
        .unwrap(),
    );
    let decoded = [decoder
        .decode(
            ProfileInput {
                id: &id,
                bytes: include_bytes!("fixtures/catalog/shipped/db2.yaml"),
            },
            &ProfileLimits::new(ProfileLimitsInput {
                input_bytes: 1 << 20,
                nodes: 32768,
                depth: 64,
                text_bytes: 1 << 20,
                scalar_bytes: 32768,
                collection_entries: 4096,
            })
            .unwrap(),
            &context,
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
        &context,
    )
    .unwrap()
}

#[test]
fn reference_parameters_and_scalar_returns_retain_native_contexts() {
    let queries = [
        ProfileQuery {
            pattern: PARAMETER_TYPE.into(),
            captures: vec![Vec::new(), vec![CaptureStep::XPath("/type_name".into())]],
            ..ProfileQuery::default()
        },
        ProfileQuery {
            pattern: SCALAR_TYPE.into(),
            captures: vec![Vec::new(), vec![CaptureStep::XPath("/type_name".into())]],
            ..ProfileQuery::default()
        },
        selected("//param_decl_2/parameter_name"),
    ];
    for name in ["TYPE_F", "SCHEMA_F.TYPE_F", "\"Á\".\"típo\""] {
        let reference = format!("ReF(/* π */{name})");
        let source = function(&format!("\"Pé\" {reference}"), &reference, "\"Pé\"");
        let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
        assert_native_source(&source, &parsed.syntax);
        assert_eq!(raw_matches(&source, &parsed, 0), [reference.as_str()]);
        assert_eq!(raw_matches(&source, &parsed, 1), [reference.as_str()]);
        assert_eq!(raw_matches(&source, &parsed, 2), ["\"Pé\""]);
        for matched in parsed
            .matches
            .iter()
            .filter(|matched| matched.query_index < 2)
        {
            let node = &parsed.syntax.nodes[matched.node_index];
            assert_eq!(node.kind, "data_type_1");
            let parent = &parsed.syntax.nodes[node.parent.unwrap()];
            assert_eq!(
                parent.kind,
                if matched.query_index == 0 {
                    "param_decl_2"
                } else {
                    "create_function_sql_scalar_table_or_row_statement"
                }
            );
            let type_name = &parsed.syntax.nodes[matched.captures[1].unwrap()];
            assert_eq!(type_name.kind, "type_name");
            assert_eq!(type_name.parent, Some(matched.node_index));
            assert_eq!(source.get(type_name.span.clone().unwrap()), Some(name));
        }
        assert!(
            !parsed
                .syntax
                .nodes
                .iter()
                .any(|node| node.kind == "data_type" || node.kind == "data_type_2")
        );
        let ordinals: std::collections::BTreeSet<_> = parsed
            .matches
            .iter()
            .map(|matched| matched.node_index)
            .collect();
        assert_eq!(ordinals.len(), parsed.matches.len());
        with_identity(&source, &operation(), |input| {
            let projected = parsed.syntax.project(input).unwrap();
            for matched in &parsed.matches {
                let raw = source
                    .get(
                        parsed.syntax.nodes[matched.node_index]
                            .span
                            .clone()
                            .unwrap(),
                    )
                    .unwrap();
                assert!(
                    matches!(&projected.nodes[matched.node_index], Some(OriginalProjection::Exact(range)) if range.text() == raw)
                );
            }
        });
    }
}

#[test]
fn scalar_type_families_preserve_raw_spelling_and_original_ranges() {
    let queries = [selected(PARAMETER_TYPE), selected(SCALAR_TYPE)];
    for (data_type, expression) in [
        ("INTEGER", "1"),
        ("VARCHAR(1)", "'a'"),
        ("SMALLINT", "1"),
        ("BIGINT", "1"),
        ("DECIMAL(1,0)", "1"),
        ("DECFLOAT(16)", "1"),
        ("DOUBLE PRECISION", "1"),
        ("CHARACTER(1 OCTETS) FOR BIT DATA", "NULL"),
        ("VARCHAR(1 CODEUNITS32)", "NULL"),
        ("CLOB(1 K OCTETS)", "NULL"),
        ("NCHAR(1)", "NULL"),
        ("NATIONAL CHARACTER VARYING(1)", "NULL"),
        ("NCLOB(1 M)", "NULL"),
        ("DATE", "CURRENT DATE"),
        ("TIMESTAMP(12)", "NULL"),
        ("BOOLEAN", "TRUE"),
        ("XML", "NULL"),
        ("ANCHOR DATA TYPE TO APP.TAB_F.COL_F", "NULL"),
        ("ANCHOR ROW OF APP.TAB_F", "NULL"),
        ("APP.ARRAY_F", "NULL"),
        ("APP.CURSOR_F", "NULL"),
        ("APP.ROW_F", "NULL"),
        ("\"cát\".\"schéma\".\"nomínal\"", "NULL"),
    ] {
        let source = function(&format!("P_F {data_type}"), data_type, expression);
        let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
        assert_native_source(&source, &parsed.syntax);
        assert_eq!(raw_matches(&source, &parsed, 0), [data_type], "{source}");
        assert_eq!(raw_matches(&source, &parsed, 1), [data_type], "{source}");
        with_identity(&source, &operation(), |input| {
            let projected = parsed.syntax.project(input).unwrap();
            for matched in &parsed.matches {
                assert!(
                    matches!(&projected.nodes[matched.node_index], Some(OriginalProjection::Exact(range)) if range.text() == data_type)
                );
            }
        });
    }
}

#[test]
fn malformed_types_keep_diagnostics_and_other_type_consumers_stay_bounded() {
    let mut sources = Vec::new();
    for malformed in [
        "REF()",
        "REF APP.TYPE_F",
        "REF(APP.TYPE_F",
        "REF(APP.TYPE_F,APP.OTHER_F)",
        "VARCHAR()",
        "FLOAT()",
    ] {
        sources.push(function(&format!("P_F {malformed}"), "INTEGER", "1"));
        sources.push(function("P_F INTEGER", malformed, "1"));
    }
    sources.extend([
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[REF(APP.TYPE_F)];".into(),
        function(
            "P_F INTEGER",
            "TABLE(C_A ANCHOR DATA TYPE TO APP.COL_F)",
            "SELECT C_A FROM SRC_F",
        ),
    ]);
    for source in sources {
        let syntax = parse(Dialect::Db2, &source, &operation()).unwrap();
        assert!(!syntax.diagnostics.is_empty(), "accepted {source}");
        assert!(
            matches!(
                parse_strict(Dialect::Db2, &source, &operation()),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "accepted {source}"
        );
        for diagnostic in &syntax.diagnostics {
            assert_eq!(diagnostic.grammar, "Db2Parser.g4");
            assert!(!diagnostic.message.is_empty());
            assert!(
                diagnostic
                    .span
                    .as_ref()
                    .is_some_and(|span| source.get(span.clone()).is_some())
            );
        }
    }
    for source in [
        "CREATE TYPE ARRAY_F AS INTEGER ARRAY[INTEGER];".to_owned(),
        function(
            "P_F INTEGER DEFAULT 1",
            "TABLE(C_A REF(APP.TYPE_F))",
            "SELECT C_A FROM SRC_F",
        ),
        function(
            "P_F INTEGER",
            "ROW(C_A INTEGER,C_B VARCHAR(1))",
            "SELECT C_A,C_B FROM SRC_F",
        ),
    ] {
        assert!(
            parse_strict(Dialect::Db2, &source, &operation())
                .unwrap()
                .diagnostics
                .is_empty(),
            "{source}"
        );
    }
}

#[test]
fn shipped_semantics_and_original_operation_causes_use_the_real_function_header() {
    let catalog = shipped_catalog();
    let profile = &catalog.profiles()[0];
    let source = "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION \"café\".\"função\"(\"parâmetro\" REF(\"Á\".\"típo\"),P_COUNT INTEGER DEFAULT 1) RETURNS REF(\"Á\".\"típo\") LANGUAGE SQL RETURN \"parâmetro\";";
    with_identity(source, &operation(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert_native_source(source, &document.syntax);
        assert!(std::ptr::eq(document.profile, profile));
        for (key, expected) in [
            ("functions", vec!["\"café\".\"função\""]),
            ("parameters", vec!["\"parâmetro\"", "P_COUNT"]),
        ] {
            let actual: Vec<_> = document
                .occurrences
                .iter()
                .filter(|occurrence| occurrence.query.data_key().value() == key)
                .map(|occurrence| match &occurrence.name {
                    CaptureBinding::Present(capture) => capture.prepared.as_ref().unwrap().text(),
                    _ => panic!("missing stock name"),
                })
                .collect();
            assert_eq!(actual, expected);
        }
        for occurrence in &document.occurrences {
            assert!(std::ptr::eq(
                occurrence.query,
                profile.queries().value().as_slice()[occurrence.query_index].value()
            ));
            assert_ne!(occurrence.query.graph_label().value(), "Type");
            if let CaptureBinding::Present(capture) = &occurrence.name {
                let node = &document.syntax.nodes[capture.node_index];
                let raw = source.get(node.span.clone().unwrap()).unwrap();
                assert_eq!(capture.prepared.as_ref().unwrap().text(), raw);
                assert!(
                    matches!(&capture.original, Some(OriginalProjection::Exact(range)) if range.text() == raw)
                );
            }
            if occurrence.query.data_key().value() == "parameters" {
                assert_eq!(
                    occurrence.query.pattern().value(),
                    "//param_decl_2/parameter_name"
                );
                assert_eq!(
                    document.syntax.nodes[occurrence.matched.node_index].kind,
                    "parameter_name"
                );
                assert!(matches!(occurrence.value, CaptureBinding::NotConfigured));
                assert!(matches!(occurrence.span, CaptureBinding::NotConfigured));
                assert!(matches!(occurrence.parent, CaptureBinding::NotConfigured));
                assert!(matches!(
                    occurrence.qualifier,
                    CaptureBinding::NotConfigured
                ));
                let context = &document.contexts[occurrence.contexts[0]];
                assert_eq!(context.selector, "create_function_statement");
                assert_eq!(context.label, Some("Function"));
            }
        }
        let projected = document.syntax.project(input).unwrap();
        let types: Vec<_> = document
            .syntax
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == "data_type_1")
            .collect();
        assert_eq!(types.len(), 3);
        for (index, node) in types {
            let raw = source.get(node.span.clone().unwrap()).unwrap();
            assert!(
                matches!(&projected.nodes[index], Some(OriginalProjection::Exact(range)) if range.text() == raw)
            );
        }
    });
    let cancelled = operation();
    with_identity(source, &cancelled, |input| {
        cancelled.cancellation().cancel();
        assert!(
            matches!(parse_semantic_occurrences(profile, input), Err(SemanticError::Operation(error)) if error.kind() == ErrorKind::Interrupted)
        );
    });
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert!(
        matches!(parse_profile_queries(Dialect::Db2, source, &[selected(PARAMETER_TYPE)], &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::TimedOut)
    );
    assert!(matches!(
        parse_profile_queries(
            Dialect::Db2,
            source,
            &[selected("//param_decl_2/data_type_1/MISSING_VOCABULARY_F")],
            &operation()
        ),
        Err(AntlrEngineError::QueryFailure { query_index: 0, .. })
    ));
}
