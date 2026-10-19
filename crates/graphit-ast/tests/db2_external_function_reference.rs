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
        "-- α\r\n/* β /* γ */ δ */ CREATE FUNCTION SCHEMA_F.F_REF({parameters}) RETURNS {returns} LANGUAGE C PARAMETER STYLE SQL NOT FENCED EXTERNAL NAME 'pkg.fn' NO SQL;"
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
    let id = ProfileId::new("external-function-reference", 256).unwrap();
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
fn named_and_anonymous_references_retain_native_nodes_and_original_ranges() {
    let queries = [
        selected("//ext_scalar_param_decl/method_reference_type"),
        selected("//ext_scalar_param_decl/method_reference_type/method_named_type"),
        selected("//ext_scalar_param_decl/method_reference_type/method_named_type/type_name"),
        selected("//ext_scalar_param_decl/data_type"),
        selected("//ext_scalar_param_decl/parameter_name"),
    ];
    for (parameter, reference, referent, name) in [
        (
            "IN P_REF REF(APP.TYPE_F)",
            "REF(APP.TYPE_F)",
            "APP.TYPE_F",
            Some("P_REF"),
        ),
        ("REF(TYPE_F)", "REF(TYPE_F)", "TYPE_F", None),
        (
            "\"parâmetro\" ReF(/* π */\"σ\" . \"τύπος\")",
            "ReF(/* π */\"σ\" . \"τύπος\")",
            "\"σ\" . \"τύπος\"",
            Some("\"parâmetro\""),
        ),
    ] {
        let source = external(parameter, "INTEGER");
        let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
        assert_native_source(&source, &parsed.syntax);
        assert_eq!(raw_matches(&source, &parsed, 0), [reference]);
        assert_eq!(raw_matches(&source, &parsed, 1), [referent]);
        assert_eq!(raw_matches(&source, &parsed, 2), [referent]);
        assert!(raw_matches(&source, &parsed, 3).is_empty());
        assert_eq!(
            raw_matches(&source, &parsed, 4),
            name.into_iter().collect::<Vec<_>>()
        );
        let reference_match = parsed.matches.iter().find(|m| m.query_index == 0).unwrap();
        let named_match = parsed.matches.iter().find(|m| m.query_index == 1).unwrap();
        let type_match = parsed.matches.iter().find(|m| m.query_index == 2).unwrap();
        let reference_node = &parsed.syntax.nodes[reference_match.node_index];
        assert_eq!(reference_node.kind, "method_reference_type");
        assert_eq!(
            parsed.syntax.nodes[reference_node.parent.unwrap()].kind,
            "ext_scalar_param_decl"
        );
        assert_eq!(
            parsed.syntax.nodes[named_match.node_index].parent,
            Some(reference_match.node_index)
        );
        assert_eq!(
            parsed.syntax.nodes[type_match.node_index].parent,
            Some(named_match.node_index)
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
fn reference_parameters_preserve_direct_types_defaults_and_lob_locator() {
    let queries = [
        selected("//ext_scalar_param_decl/data_type"),
        selected("//ext_scalar_param_decl/method_reference_type"),
        selected("//ext_scalar_param_decl/parameter_name"),
        selected("//ext_scalar_param_decl/routine_default_clause/routine_default_value"),
        selected("//ext_scalar_param_decl/as_locator"),
        selected("//create_function_external_scalar_statement/external_scalar_cast_source_type"),
        selected("//create_function_external_scalar_statement/data_type_3/data_type"),
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type",
        ),
    ];
    for (returns, source_type, target_type) in [
        ("INTEGER", "INTEGER", None),
        (
            "VARCHAR(20) CAST FROM VARCHAR(40)",
            "VARCHAR(40)",
            Some("VARCHAR(20)"),
        ),
    ] {
        let source = external(
            "IN P_INT INTEGER DEFAULT 7, REF(TYPE_F), IN P_LOC CLOB DEFAULT NULL AS LOCATOR",
            returns,
        );
        let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
        assert_native_source(&source, &parsed.syntax);
        assert_eq!(raw_matches(&source, &parsed, 0), ["INTEGER", "CLOB"]);
        assert_eq!(raw_matches(&source, &parsed, 1), ["REF(TYPE_F)"]);
        assert_eq!(raw_matches(&source, &parsed, 2), ["P_INT", "P_LOC"]);
        assert_eq!(raw_matches(&source, &parsed, 3), ["7", "NULL"]);
        assert_eq!(raw_matches(&source, &parsed, 4), ["AS LOCATOR"]);
        assert_eq!(
            raw_matches(&source, &parsed, 5),
            target_type
                .map(|_| source_type)
                .into_iter()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            raw_matches(&source, &parsed, 7),
            target_type
                .is_none()
                .then_some(source_type)
                .into_iter()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            raw_matches(&source, &parsed, 6),
            target_type.into_iter().collect::<Vec<_>>()
        );
        for matched in parsed
            .matches
            .iter()
            .filter(|m| m.query_index <= 2 || m.query_index == 4)
        {
            let node = &parsed.syntax.nodes[matched.node_index];
            assert_eq!(
                parsed.syntax.nodes[node.parent.unwrap()].kind,
                "ext_scalar_param_decl"
            );
        }
    }
}

#[test]
fn malformed_reference_parameters_keep_native_diagnostics() {
    for parameter in [
        "P_BAD REF()",
        "P_BAD REF(APP.)",
        "P_BAD REF(TYPE_F",
        "P_BAD REF(APP.MOD.TYPE_F)",
        "IN REF(TYPE_F)",
        "P_BAD REF TYPE_F",
    ] {
        let source = external(parameter, "INTEGER");
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
fn reference_stock_semantics_preserve_anonymous_slots_metadata_and_operation_causes() {
    let catalog = shipped_catalog();
    let profile = &catalog.profiles()[0];
    let source = external(
        "\"parâmetro\" REF(\"σ\".\"τύπος\"), REF(TYPE_F), IN P_LOC CLOB DEFAULT NULL AS LOCATOR",
        "INTEGER",
    )
    .replace("SCHEMA_F.F_REF", "\"café\".\"função\"");
    with_identity(&source, &operation(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert_native_source(&source, &document.syntax);
        assert!(std::ptr::eq(document.profile, profile));
        for (key, expected) in [
            ("functions", vec!["\"café\".\"função\""]),
            ("parameters", vec!["\"parâmetro\"", "P_LOC"]),
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
        let references: Vec<_> = document
            .syntax
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == "method_reference_type")
            .collect();
        assert_eq!(references.len(), 2);
        let actual: Vec<_> = references.iter().map(|(index, node)| {
            let raw = source.get(node.span.clone().unwrap()).unwrap();
            assert!(matches!(&projected.nodes[*index], Some(OriginalProjection::Exact(range)) if range.text() == raw));
            raw
        }).collect();
        assert_eq!(actual, ["REF(\"σ\".\"τύπος\")", "REF(TYPE_F)"]);
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
    assert!(
        matches!(parse_profile_queries(Dialect::Db2, &source, &[selected("//ext_scalar_param_decl/method_reference_type")], &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::TimedOut)
    );
    assert!(matches!(
        parse_profile_queries(
            Dialect::Db2,
            &source,
            &[selected(
                "//ext_scalar_param_decl/method_reference_type/MISSING_VOCABULARY_F"
            )],
            &operation()
        ),
        Err(AntlrEngineError::QueryFailure { query_index: 0, .. })
    ));
}

#[test]
fn xml_representations_preserve_all_external_parameter_branches_and_original_ranges() {
    let xml_named = "xMl /* ψ */ As cLoB(/* π */2M)";
    let source = external(
        &format!(
            "\"xml_α\" {xml_named}, XML AS CLOB(2048), IN P_REF REF(APP.TYPE_F), REF(TYPE_F), P_INT INTEGER DEFAULT 7, IN P_LOC CLOB DEFAULT NULL AS LOCATOR"
        ),
        "INTEGER",
    );
    let queries = [
        selected("//ext_scalar_param_decl/procedure_external_xml_type"),
        selected("//ext_scalar_param_decl/procedure_external_xml_type/integer_value"),
        selected("//ext_scalar_param_decl/procedure_external_xml_type/kmg"),
        selected("//ext_scalar_param_decl/data_type"),
        selected("//ext_scalar_param_decl/method_reference_type"),
        selected("//ext_scalar_param_decl/parameter_name"),
        selected("//ext_scalar_param_decl/routine_default_clause/routine_default_value"),
        selected("//ext_scalar_param_decl/as_locator"),
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type",
        ),
    ];
    let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
    assert_native_source(&source, &parsed.syntax);
    assert_eq!(
        raw_matches(&source, &parsed, 0),
        [xml_named, "XML AS CLOB(2048)"]
    );
    assert_eq!(raw_matches(&source, &parsed, 1), ["2", "2048"]);
    assert_eq!(raw_matches(&source, &parsed, 2), ["M"]);
    assert_eq!(raw_matches(&source, &parsed, 3), ["INTEGER", "CLOB"]);
    assert_eq!(
        raw_matches(&source, &parsed, 4),
        ["REF(APP.TYPE_F)", "REF(TYPE_F)"]
    );
    assert_eq!(
        raw_matches(&source, &parsed, 5),
        ["\"xml_α\"", "P_REF", "P_INT", "P_LOC"]
    );
    assert_eq!(raw_matches(&source, &parsed, 6), ["7", "NULL"]);
    assert_eq!(raw_matches(&source, &parsed, 7), ["AS LOCATOR"]);
    assert_eq!(raw_matches(&source, &parsed, 8), ["INTEGER"]);
    for matched in parsed.matches.iter().filter(|m| m.query_index <= 2) {
        let node = &parsed.syntax.nodes[matched.node_index];
        assert_eq!(
            parsed.syntax.nodes[node.parent.unwrap()].kind,
            if matched.query_index == 0 {
                "ext_scalar_param_decl"
            } else {
                "procedure_external_xml_type"
            }
        );
    }
    let catalog = shipped_catalog();
    let profile = &catalog.profiles()[0];
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
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert_native_source(&source, &document.syntax);
        assert!(std::ptr::eq(document.profile, profile));
        for (key, expected) in [
            ("functions", vec!["SCHEMA_F.F_REF"]),
            ("parameters", vec!["\"xml_α\"", "P_REF", "P_INT", "P_LOC"]),
        ] {
            let actual: Vec<_> = document
                .occurrences
                .iter()
                .filter(|o| o.query.data_key().value() == key)
                .map(|o| match &o.name {
                    CaptureBinding::Present(name) => name.prepared.as_ref().unwrap().text(),
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
                let raw = source
                    .get(
                        document.syntax.nodes[capture.node_index]
                            .span
                            .clone()
                            .unwrap(),
                    )
                    .unwrap();
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
                let context = &document.contexts[occurrence.contexts[0]];
                assert_eq!(context.selector, "create_function_statement");
                assert_eq!(context.label, Some("Function"));
            }
        }
    });
    // Preserve existing bare XML recognition without validating server representation.
    let bare = external("P_BARE XML", "INTEGER");
    let legacy = parse_profile_queries(Dialect::Db2, &bare, &queries, &operation()).unwrap();
    assert_native_source(&bare, &legacy.syntax);
    assert!(raw_matches(&bare, &legacy, 0).is_empty());
    assert_eq!(raw_matches(&bare, &legacy, 3), ["XML"]);
}

#[test]
fn malformed_xml_representations_preserve_native_diagnostics_and_operation_causes() {
    for parameter in [
        "P_XML XML AS CLOB()",
        "P_XML XML AS CLOB(M)",
        "P_XML XML AS CLOB(2",
        "P_XML XML AS BLOB(2)",
        "P_XML XML AS CLOB(2K) AS CLOB(2)",
        "IN XML AS CLOB(2)",
    ] {
        let source = external(parameter, "INTEGER");
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
    let source = external("IN P_XML XML AS CLOB(2M)", "INTEGER");
    let catalog = shipped_catalog();
    let profile = &catalog.profiles()[0];
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
    assert!(
        matches!(parse_profile_queries(Dialect::Db2, &source, &[selected("//ext_scalar_param_decl/procedure_external_xml_type")], &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::TimedOut)
    );
    assert!(matches!(
        parse_profile_queries(
            Dialect::Db2,
            &source,
            &[selected(
                "//ext_scalar_param_decl/procedure_external_xml_type/MISSING_VOCABULARY_F"
            )],
            &operation()
        ),
        Err(AntlrEngineError::QueryFailure { query_index: 0, .. })
    ));
}

#[test]
fn ordinary_external_returns_preserve_native_domains_and_original() {
    let queries = [
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type",
        ),
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type/data_type",
        ),
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type/method_reference_type",
        ),
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type/procedure_external_xml_type",
        ),
        selected("//create_function_external_scalar_statement/as_locator"),
        selected("//create_function_external_scalar_statement/external_scalar_cast_source_type"),
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type/procedure_external_xml_type/integer_value",
        ),
        selected(
            "//create_function_external_scalar_statement/external_scalar_ordinary_return_type/procedure_external_xml_type/kmg",
        ),
    ];
    for (returns, expected, branch, locator) in [
        ("INTEGER", "INTEGER", 1, false),
        ("DATE", "DATE", 1, false),
        ("DECIMAL(11,2)", "DECIMAL(11,2)", 1, false),
        ("CLOB(2M) AS LOCATOR", "CLOB(2M)", 1, true),
        ("APP.TYPE_F", "APP.TYPE_F", 1, false),
        (r#""σ"."τύ""πος""#, r#""σ"."τύ""πος""#, 1, false),
        ("REF(APP.TYPE_F)", "REF(APP.TYPE_F)", 2, false),
        (
            r#"rEf(/* π */"σ" . "τύπος")"#,
            r#"rEf(/* π */"σ" . "τύπος")"#,
            2,
            false,
        ),
        ("XML AS CLOB(2M)", "XML AS CLOB(2M)", 3, false),
        ("XML AS CLOB(2048)", "XML AS CLOB(2048)", 3, false),
        (
            "xMl /* ψ */ As cLoB(/* π */2M)",
            "xMl /* ψ */ As cLoB(/* π */2M)",
            3,
            false,
        ),
        // Existing recognizer forms; these do not prove server legality.
        (
            "ANCHOR DATA TYPE TO APP.V_F",
            "ANCHOR DATA TYPE TO APP.V_F",
            1,
            false,
        ),
        ("XML", "XML", 1, false),
    ] {
        let source = external("P_F INTEGER", returns);
        let prepared = source.clone();
        assert_ne!(prepared.as_ptr(), source.as_ptr());
        let context = operation();
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
            prepared.as_bytes(),
            OriginPlan {
                segments: &segments,
                removed_original: &[],
                anchors: &[],
            },
            &context,
        )
        .unwrap();
        let parsed = parse_profile_queries(Dialect::Db2, &prepared, &queries, &context).unwrap();
        assert_native_source(&prepared, &parsed.syntax);
        assert_eq!(raw_matches(&prepared, &parsed, 0), [expected]);
        for query in 1..=3 {
            assert_eq!(
                raw_matches(&prepared, &parsed, query),
                if query == branch {
                    vec![expected]
                } else {
                    vec![]
                }
            );
        }
        assert_eq!(
            raw_matches(&prepared, &parsed, 4),
            if locator { vec!["AS LOCATOR"] } else { vec![] }
        );
        assert!(raw_matches(&prepared, &parsed, 5).is_empty());
        let wrapper = parsed
            .matches
            .iter()
            .find(|m| m.query_index == 0)
            .unwrap()
            .node_index;
        let native = &parsed.syntax.nodes[wrapper];
        assert_eq!(native.kind, "external_scalar_ordinary_return_type");
        assert_eq!(
            parsed.syntax.nodes[native.parent.unwrap()].kind,
            "create_function_external_scalar_statement"
        );
        let child = parsed
            .matches
            .iter()
            .find(|m| m.query_index == branch)
            .unwrap()
            .node_index;
        assert_eq!(parsed.syntax.nodes[child].parent, Some(wrapper));
        if branch == 3 {
            assert_eq!(
                raw_matches(&prepared, &parsed, 6),
                if expected.contains("2048") {
                    vec!["2048"]
                } else {
                    vec!["2"]
                }
            );
            assert_eq!(
                raw_matches(&prepared, &parsed, 7),
                if expected.contains("2048") {
                    vec![]
                } else {
                    vec!["M"]
                }
            );
        } else {
            assert!(raw_matches(&prepared, &parsed, 6).is_empty());
            assert!(raw_matches(&prepared, &parsed, 7).is_empty());
        }
        let projected = parsed.syntax.project(&input).unwrap();
        for matched in &parsed.matches {
            let span = parsed.syntax.nodes[matched.node_index]
                .span
                .clone()
                .unwrap();
            let Some(OriginalProjection::Exact(range)) = &projected.nodes[matched.node_index]
            else {
                panic!("exact native Original");
            };
            assert_eq!(range.text(), &prepared[span.clone()]);
            assert_eq!(range.text().as_ptr(), source[span.start..].as_ptr());
        }
    }
}

#[test]
fn malformed_ordinary_external_returns_keep_native_diagnostics() {
    for returns in [
        "REF()",
        "REF(APP.)",
        "REF(APP.TYPE_F",
        "XML AS CLOB()",
        "XML AS CLOB(2M",
        "XML AS BLOB(2)",
        "VARCHAR()",
    ] {
        let source = external("P_F INTEGER", returns);
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
