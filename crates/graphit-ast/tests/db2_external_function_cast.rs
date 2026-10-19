use graphit_ast::{
    antlr_engine::{
        AntlrEngineError, Dialect, NodeClass, ParsedProfileQueries, ParsedSyntax, ProfileQuery,
        SpanStatus, parse, parse_profile_queries, parse_strict,
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

fn operation() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}

fn external(parameters: &str, returns: &str) -> String {
    format!(
        "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION SCHEMA_F.F_CAST({parameters}) RETURNS {returns} LANGUAGE C PARAMETER STYLE SQL EXTERNAL NAME 'pkg.fn';"
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
    let id = ProfileId::new("external-function-cast", 256).unwrap();
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
fn inferred_cast_targets_retain_native_nodes_and_original_ranges() {
    let queries = [
        selected("//create_function_external_scalar_statement/data_type_3"),
        selected("//create_function_external_scalar_statement/data_type_3/inferred_builtin_type"),
        selected("//create_function_external_scalar_statement/external_scalar_cast_source_type"),
        selected("//ext_scalar_param_decl/parameter_name"),
        selected("//ext_scalar_param_decl/routine_default_clause/routine_default_value"),
    ];
    for (target, source_type) in [
        ("VARCHAR()", "VARCHAR(40)"),
        ("CHAR VARYING()", "VARCHAR(40)"),
        ("DECIMAL()", "INTEGER"),
        ("VaRcHaR(/* π */)", "VARCHAR(40)"),
    ] {
        let source = external(
            "\"parâmetro\" INTEGER DEFAULT 1",
            &format!("{target} CAST FROM {source_type}"),
        );
        let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
        assert_native_source(&source, &parsed.syntax);
        assert_eq!(raw_matches(&source, &parsed, 0), [target]);
        assert_eq!(raw_matches(&source, &parsed, 1), [target]);
        assert_eq!(raw_matches(&source, &parsed, 2), [source_type]);
        assert_eq!(raw_matches(&source, &parsed, 3), ["\"parâmetro\""]);
        assert_eq!(raw_matches(&source, &parsed, 4), ["1"]);
        let target_match = parsed.matches.iter().find(|m| m.query_index == 0).unwrap();
        let inferred_match = parsed.matches.iter().find(|m| m.query_index == 1).unwrap();
        let target_node = &parsed.syntax.nodes[target_match.node_index];
        let inferred_node = &parsed.syntax.nodes[inferred_match.node_index];
        assert_eq!(target_node.kind, "data_type_3");
        assert_eq!(inferred_node.kind, "inferred_builtin_type");
        assert_eq!(inferred_node.parent, Some(target_match.node_index));
        assert_eq!(
            parsed.syntax.nodes[target_node.parent.unwrap()].kind,
            "create_function_external_scalar_statement"
        );
        assert!(
            !parsed
                .syntax
                .nodes
                .iter()
                .any(|n| n.kind == "data_type" && n.parent == Some(target_match.node_index))
        );
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
fn explicit_cast_and_ordinary_returns_keep_their_native_domains() {
    let queries = [
        selected("//create_function_external_scalar_statement/data_type_3"),
        selected("//create_function_external_scalar_statement/data_type_3/data_type"),
        selected("//create_function_external_scalar_statement/data_type_3/inferred_builtin_type"),
        selected("//create_function_external_scalar_statement/external_scalar_cast_source_type"),
        selected("//ext_scalar_param_decl/parameter_name"),
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type",
        ),
    ];
    for (returns, targets, sources, ordinary) in [
        (
            "VARCHAR(20) CAST FROM VARCHAR(40)",
            vec!["VARCHAR(20)"],
            vec!["VARCHAR(40)"],
            vec![],
        ),
        ("INTEGER", vec![], vec![], vec!["INTEGER"]),
    ] {
        let source = external("P_COUNT INTEGER DEFAULT 1", returns);
        let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
        assert_native_source(&source, &parsed.syntax);
        assert_eq!(raw_matches(&source, &parsed, 0), targets);
        assert_eq!(raw_matches(&source, &parsed, 1), targets);
        assert!(raw_matches(&source, &parsed, 2).is_empty());
        assert_eq!(raw_matches(&source, &parsed, 3), sources);
        assert_eq!(raw_matches(&source, &parsed, 4), ["P_COUNT"]);
        assert_eq!(raw_matches(&source, &parsed, 5), ordinary);
        for matched in parsed.matches.iter().filter(|m| m.query_index == 3) {
            let node = &parsed.syntax.nodes[matched.node_index];
            assert_eq!(node.kind, "external_scalar_cast_source_type");
            assert_eq!(
                parsed.syntax.nodes[node.parent.unwrap()].kind,
                "create_function_external_scalar_statement"
            );
        }
    }
}

#[test]
fn malformed_and_misplaced_inference_keep_native_diagnostics() {
    for source in [
        external("", "FLOAT() CAST FROM INTEGER"),
        external("", "VARCHAR( CAST FROM VARCHAR(40)"),
        external("", "VARCHAR(20) CAST FROM VARCHAR()"),
        external("", "VARCHAR()"),
        external("P_BAD VARCHAR()", "INTEGER"),
        external("", "VARCHAR() AS LOCATOR CAST FROM VARCHAR(40)"),
    ] {
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
        for token in syntax.tokens.iter().filter(|token| !token.eof) {
            if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
}

#[test]
fn inferred_cast_stock_semantics_preserve_metadata_source_and_operation_causes() {
    let catalog = shipped_catalog();
    let profile = &catalog.profiles()[0];
    let source = external(
        "\"parâmetro\" INTEGER DEFAULT 1",
        "VARCHAR() CAST FROM VARCHAR(40)",
    )
    .replace("SCHEMA_F.F_CAST", "\"café\".\"função\"");
    with_identity(&source, &operation(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert_native_source(&source, &document.syntax);
        assert!(std::ptr::eq(document.profile, profile));
        for (key, expected) in [
            ("functions", vec!["\"café\".\"função\""]),
            ("parameters", vec!["\"parâmetro\""]),
        ] {
            let actual: Vec<_> = document
                .occurrences
                .iter()
                .filter(|o| o.query.data_key().value() == key)
                .map(|o| match &o.name {
                    CaptureBinding::Present(n) => n.prepared.as_ref().unwrap().text(),
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
                    "//ext_scalar_param_decl/parameter_name"
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
        let targets: Vec<_> = document
            .syntax
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == "data_type_3")
            .collect();
        assert_eq!(targets.len(), 1);
        let (index, target) = targets[0];
        assert_eq!(source.get(target.span.clone().unwrap()), Some("VARCHAR()"));
        assert!(
            matches!(&projected.nodes[index], Some(OriginalProjection::Exact(range)) if range.text() == "VARCHAR()")
        );
    });
    let cancelled = operation();
    with_identity(&source, &cancelled, |input| {
        cancelled.cancellation().cancel();
        assert!(
            matches!(parse_semantic_occurrences(profile, input), Err(SemanticError::Operation(error)) if error.kind() == ErrorKind::Interrupted)
        );
    });
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    let pattern = "//create_function_external_scalar_statement/data_type_3/inferred_builtin_type";
    assert!(
        matches!(parse_profile_queries(Dialect::Db2, &source, &[selected(pattern)], &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::TimedOut)
    );
    assert!(matches!(
        parse_profile_queries(
            Dialect::Db2,
            &source,
            &[selected(
                "//data_type_3/inferred_builtin_type/MISSING_VOCABULARY_F"
            )],
            &operation()
        ),
        Err(AntlrEngineError::QueryFailure { query_index: 0, .. })
    ));
}
