use graphit_ast::{
    antlr_engine::{
        AntlrEngineError, NodeClass, SpanStatus,
        semantic::{CaptureBinding, SemanticError, parse_semantic_occurrences},
    },
    catalog::{CatalogDraft, CatalogLimits, CatalogLimitsInput, ProfileLayer, ProfileLayerKind},
    prepared_input::{ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput},
    profiles::{
        ProfileDecoder, ProfileId, ProfileInput, ProfileLimits, ProfileLimitsInput, QueryKind,
    },
    scalar_policy::{RegexLimits, ScalarPolicy},
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    io::ErrorKind,
    time::{Duration, Instant},
};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}

fn catalog(bytes: &[u8]) -> CatalogDraft {
    let operation = context();
    let id = ProfileId::new("plsql-native-calls", 256).unwrap();
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

fn with_input(
    source: &str,
    operation: &SnapshotContext,
    action: impl FnOnce(&PreparedInput<'_, '_, '_, '_>),
) {
    let span = ByteSpan {
        start: 0,
        end: u64::try_from(source.len()).unwrap(),
    };
    let segments = [OriginSegment::Direct {
        prepared: span,
        original: span,
    }];
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

fn dedicated(pattern: &str) -> bool {
    [
        "//string_function/",
        "//numeric_function/",
        "//other_function/",
        "//json_function/",
        "//xmltable/XMLTABLE",
        "//json_table_clause/JSON_TABLE",
        "//general_element/keyword_as_method/",
    ]
    .iter()
    .any(|prefix| pattern.starts_with(prefix))
}

fn assert_case(catalog: &CatalogDraft, source: &str, expected: &[(&str, &str)]) {
    let profile = &catalog.profiles()[0];
    with_input(source, &context(), |input| {
        let document = parse_semantic_occurrences(profile, input).unwrap();
        assert!(
            document.syntax.diagnostics.is_empty(),
            "{source}: {:?}",
            document.syntax.diagnostics
        );
        let mut observed: Vec<_> = document
            .occurrences
            .iter()
            .filter(|o| dedicated(o.query.pattern().value()))
            .collect();
        assert_eq!(
            observed.len(),
            expected.len(),
            "{source}: {:?}",
            observed
                .iter()
                .map(|o| (
                    o.query.pattern().value(),
                    o.matched.prepared.as_ref().map(|s| s.text())
                ))
                .collect::<Vec<_>>()
        );
        assert!(
            observed
                .windows(2)
                .all(|pair| (pair[0].query_index, pair[0].matched.node_index)
                    <= (pair[1].query_index, pair[1].matched.node_index))
        );
        observed.sort_by_key(|o| (o.matched.node_index, o.query_index));
        for (occurrence, (name, wrapper)) in observed.into_iter().zip(expected) {
            assert!(std::ptr::eq(
                occurrence.query,
                profile
                    .queries()
                    .value()
                    .iter()
                    .nth(occurrence.query_index)
                    .unwrap()
                    .value()
            ));
            assert_eq!(occurrence.query.data_key().value(), "calls");
            assert_eq!(occurrence.query.graph_label().value(), "");
            assert_eq!(*occurrence.query.r#type().value(), QueryKind::Relation);
            assert_eq!(occurrence.query.relation_type().value(), "CALLS");
            assert_eq!(occurrence.query.name_reject().value(), "");
            let node = &document.syntax.nodes[occurrence.matched.node_index];
            assert_eq!(node.class, NodeClass::Terminal);
            assert_eq!(node.kind, name.to_ascii_uppercase());
            assert_eq!(node.status, SpanStatus::Regular);
            let span = node.span.as_ref().unwrap();
            assert_eq!(&source[span.start..span.end], *name);
            let CaptureBinding::Present(captured) = &occurrence.name else {
                panic!("missing keyword")
            };
            assert_eq!(captured.node_index, occurrence.matched.node_index);
            assert_eq!(captured.prepared.as_ref().unwrap().text(), *name);
            let Some(OriginalProjection::Exact(original)) = &captured.original else {
                panic!("non-exact keyword")
            };
            assert_eq!(original.text(), *name);
            assert_eq!(
                original.text().as_ptr(),
                source.as_ptr().wrapping_add(span.start)
            );
            let CaptureBinding::Present(qualifier) = &occurrence.qualifier else {
                panic!("missing wrapper")
            };
            let parent = &document.syntax.nodes[qualifier.node_index];
            assert_eq!(parent.class, NodeClass::Rule);
            assert_eq!(parent.kind, *wrapper);
            let parent_span = parent.span.as_ref().unwrap();
            let Some(OriginalProjection::Exact(original)) = &qualifier.original else {
                panic!("non-exact wrapper")
            };
            assert_eq!(original.text(), &source[parent_span.start..parent_span.end]);
            assert!(parent_span.start <= span.start && span.end < parent_span.end);
            let mut ancestor = node.parent;
            loop {
                let index = ancestor.expect("wrapper must be a real ancestor");
                if document.syntax.nodes[index].kind == *wrapper {
                    assert_eq!(index, qualifier.node_index);
                    break;
                }
                ancestor = document.syntax.nodes[index].parent;
            }
            for &index in &occurrence.contexts {
                let ctx = &document.contexts[index];
                let mut ancestor = Some(qualifier.node_index);
                while ancestor != Some(ctx.matched.node_index) {
                    ancestor = document.syntax.nodes
                        [ancestor.expect("context must be native ancestor")]
                    .parent;
                }
            }
        }
    });
}

#[test]
fn dedicated_plsql_calls_native() {
    let stock = include_bytes!("fixtures/catalog/shipped/plsql.yaml");
    assert_eq!(
        stock.as_slice(),
        include_bytes!("fixtures/profiles/shipped/plsql.yaml").as_slice()
    );
    assert_eq!(
        stock.as_slice(),
        include_bytes!("../../../crates/graphit-ast/profiles/queries/plsql.yaml").as_slice()
    );
    let catalog = catalog(stock);
    assert_eq!(
        catalog.profiles()[0]
            .queries()
            .value()
            .iter()
            .filter(|q| dedicated(q.value().pattern().value()))
            .count(),
        84
    );
    for (source, expected) in [
        (
            "SELECT CHR(65 USING NCHAR_CS) FROM dual;",
            vec![("CHR", "string_function")],
        ),
        (
            "SELECT TRIM(BOTH 'x' FROM 'xx') FROM dual;",
            vec![("TRIM", "string_function")],
        ),
        (
            "SELECT TO_DATE('2020-01-01', 'YYYY-MM-DD') FROM dual;",
            vec![("TO_DATE", "string_function")],
        ),
        (
            "SELECT TO_DATE('x' DEFAULT '2020' ON CONVERSION ERROR, 'YYYY') FROM dual;",
            vec![("TO_DATE", "string_function")],
        ),
        (
            "SELECT SUM(DISTINCT x),SUM(x) OVER (),COUNT(*),COUNT(DISTINCT x),COUNT(x) OVER (),AVG(ALL x),AVG(x) OVER (),MIN(ALL x),MIN(x) OVER (),MAX(ALL x),MAX(x) OVER () FROM dual;",
            vec![
                ("SUM", "numeric_function"),
                ("SUM", "numeric_function"),
                ("COUNT", "numeric_function"),
                ("COUNT", "numeric_function"),
                ("COUNT", "numeric_function"),
                ("AVG", "numeric_function"),
                ("AVG", "numeric_function"),
                ("MIN", "numeric_function"),
                ("MIN", "numeric_function"),
                ("MAX", "numeric_function"),
                ("MAX", "numeric_function"),
            ],
        ),
        (
            "SELECT COUNT(*) KEEP (DENSE_RANK FIRST ORDER BY x) OVER () FROM dual;",
            vec![("COUNT", "numeric_function")],
        ),
        (
            "SELECT CAST(fn(1) AS NUMBER), XMLCAST(x AS VARCHAR2(10)), COALESCE(x,0), COLLECT(DISTINCT x), LISTAGG(x, ',') WITHIN GROUP (ORDER BY x), DECOMPOSE(x CANONICAL), EXTRACT(YEAR FROM CURRENT_TIMESTAMP) FROM dual;",
            vec![
                ("CAST", "other_function"),
                ("XMLCAST", "other_function"),
                ("COALESCE", "other_function"),
                ("COLLECT", "other_function"),
                ("LISTAGG", "other_function"),
                ("DECOMPOSE", "other_function"),
                ("EXTRACT", "other_function"),
            ],
        ),
        (
            "SELECT FIRST_VALUE(x) OVER (ORDER BY x),LAST_VALUE(x) OVER (ORDER BY x),LEAD(x) OVER (ORDER BY x),LAG(x) OVER (ORDER BY x),TRANSLATE(x USING NCHAR_CS),TREAT(x AS REF person_t),VALIDATE_CONVERSION(x AS NUMBER) FROM dual;",
            vec![
                ("FIRST_VALUE", "other_function"),
                ("LAST_VALUE", "other_function"),
                ("LEAD", "other_function"),
                ("LAG", "other_function"),
                ("TRANSLATE", "other_function"),
                ("TREAT", "other_function"),
                ("VALIDATE_CONVERSION", "other_function"),
            ],
        ),
        (
            "SELECT JSON_ARRAY(1, JSON_OBJECT('k' VALUE fn(2))), JSON_ARRAYAGG(x ORDER BY x), JSON_OBJECT(*), JSON_OBJECTAGG(KEY x VALUE y), JSON_QUERY(j,'$'), JSON_SERIALIZE('{}'), JSON_TRANSFORM(j,SET '$.x'=fn(1)), JSON_VALUE(j,'$') FROM dual;",
            vec![
                ("JSON_ARRAY", "json_function"),
                ("JSON_OBJECT", "json_function"),
                ("JSON_ARRAYAGG", "json_function"),
                ("JSON_OBJECT", "json_function"),
                ("JSON_OBJECTAGG", "json_function"),
                ("JSON_QUERY", "json_function"),
                ("JSON_SERIALIZE", "json_function"),
                ("JSON_TRANSFORM", "json_function"),
                ("JSON_VALUE", "json_function"),
            ],
        ),
        (
            "SELECT XMLAGG(XMLELEMENT(NAME e,x) ORDER BY x),XMLCOLATTVAL(x AS a),XMLFOREST(x AS a),XMLELEMENT(NAME e,x),XMLEXISTS('/e' PASSING x),XMLPARSE(CONTENT '<e/>'),XMLPI(NAME p,'x'),XMLQUERY('/e' PASSING x RETURNING CONTENT),XMLROOT(x,VERSION '1.0'),XMLSERIALIZE(CONTENT x AS CLOB) FROM dual;",
            vec![
                ("XMLAGG", "other_function"),
                ("XMLELEMENT", "other_function"),
                ("XMLCOLATTVAL", "other_function"),
                ("XMLFOREST", "other_function"),
                ("XMLELEMENT", "other_function"),
                ("XMLEXISTS", "other_function"),
                ("XMLPARSE", "other_function"),
                ("XMLPI", "other_function"),
                ("XMLQUERY", "other_function"),
                ("XMLROOT", "other_function"),
                ("XMLSERIALIZE", "other_function"),
            ],
        ),
        (
            "SELECT jt.x FROM JSON_TABLE(j,'$' COLUMNS (x NUMBER PATH '$.x')) jt;",
            vec![("JSON_TABLE", "json_table_clause")],
        ),
        (
            "SELECT xt.x FROM XMLTABLE('/e' PASSING doc COLUMNS x VARCHAR2(10) PATH 'x') xt;",
            vec![("XMLTABLE", "xmltable")],
        ),
        (
            "SELECT cHr(65 USING NCHAR_CS), TRIM('α' FROM 'αβ') FROM dual;",
            vec![("cHr", "string_function"), ("TRIM", "string_function")],
        ),
    ] {
        assert_case(&catalog, source, &expected);
    }

    for name in [
        "AVG",
        "CORR",
        "LAG_DIFF",
        "LAG_DIFF_PERCENT",
        "MAX",
        "MEDIAN",
        "MIN",
        "NTH_VALUE",
        "NTILE",
        "RATIO_TO_REPORT",
        "ROW_NUMBER",
        "SUM",
        "VARIANCE",
        "REGR_",
        "STDDEV",
        "VAR_",
        "VAR_POP",
        "COVAR_",
        "WM_CONCAT",
    ] {
        let args = match name {
            "ROW_NUMBER" => "",
            "CORR" | "NTH_VALUE" => "x,1",
            "NTILE" => "2",
            _ => "x",
        };
        let source = format!("SELECT {name}({args}) OVER (ORDER BY x) FROM dual;");
        assert_case(
            &catalog,
            &source,
            &[(
                name,
                if ["AVG", "MAX", "MIN", "SUM"].contains(&name) {
                    "numeric_function"
                } else {
                    "other_function"
                },
            )],
        );
    }
    for name in [
        "CUME_DIST",
        "DENSE_RANK",
        "PERCENT_RANK",
        "PERCENTILE_CONT",
        "PERCENTILE_DISC",
        "RANK",
    ] {
        let source = format!("SELECT {name}(1) WITHIN GROUP (ORDER BY x) FROM dual;");
        assert_case(&catalog, &source, &[(name, "other_function")]);
    }
    for name in [
        "PREDICTION",
        "PREDICTION_BOUNDS",
        "PREDICTION_COST",
        "PREDICTION_DETAILS",
        "PREDICTION_PROBABILITY",
        "PREDICTION_SET",
    ] {
        assert_case(
            &catalog,
            &format!("SELECT {name}(model USING x) FROM dual;"),
            &[(name, "other_function")],
        );
    }
    for name in [
        "TO_BINARY_DOUBLE",
        "TO_BINARY_FLOAT",
        "TO_NUMBER",
        "TO_TIMESTAMP",
        "TO_TIMESTAMP_TZ",
        "TO_DSINTERVAL",
        "TO_YMINTERVAL",
    ] {
        assert_case(
            &catalog,
            &format!("SELECT {name}(x DEFAULT '0' ON CONVERSION ERROR) FROM dual;"),
            &[(name, "other_function")],
        );
    }
    for name in ["EXISTS", "CASE", "WHEN", "THEN"] {
        assert_case(
            &catalog,
            &format!("BEGIN v := pkg.{name}(1); END; /"),
            &[(name, "general_element")],
        );
    }
}

#[test]
fn dedicated_plsql_non_calls_have_valid_native_controls() {
    let catalog = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    for source in [
        "BEGIN v := c%ROWCOUNT; END; /",
        "BEGIN v := SQL%BULK_ROWCOUNT(1); END; /",
        "SELECT TIME '12:00:00' FROM dual;",
        "SELECT CASE WHEN x=1 THEN 1 ELSE 0 END FROM dual;",
        "SELECT 1 FROM dual WHERE EXISTS (SELECT 1 FROM dual);",
        "BEGIN items(1) := fn(2); v := items.COUNT; END; /",
    ] {
        assert_case(&catalog, source, &[]);
    }
    assert_case(
        &catalog,
        "SELECT TRIM(BOTH 'x' FROM CAST('xx' AS VARCHAR2(2))) FROM dual;",
        &[("TRIM", "string_function"), ("CAST", "other_function")],
    );
}

#[test]
fn dedicated_plsql_custom_and_original_control_remain_explicit() {
    let catalog = catalog(br#"language: plsql
parser: antlr4
grammar: antlr-plsql
queries:
  - {data_key: calls, graph_label: '', type: relation, relation_type: CALLS, pattern: '//string_function/CHR', captures: {qualifier: [{nearest_ancestor: string_function}]}, name_reject: '^CHR$'}
  - {data_key: calls, graph_label: '', type: relation, relation_type: CALLS, pattern: '//string_function/CHR', captures: {qualifier: [{nearest_ancestor: string_function}]}, name_reject: '^CHR$'}
"#);
    let profile = &catalog.profiles()[0];
    let source = "SELECT CHR(65 USING NCHAR_CS) FROM dual;";
    with_input(source, &context(), |input| {
        let doc = parse_semantic_occurrences(profile, input).unwrap();
        assert!(doc.syntax.diagnostics.is_empty());
        assert_eq!(doc.occurrences.len(), 2);
        assert_eq!(
            doc.occurrences[0].matched.node_index,
            doc.occurrences[1].matched.node_index
        );
        for (i, occurrence) in doc.occurrences.iter().enumerate() {
            assert_eq!(occurrence.query_index, i);
            assert!(std::ptr::eq(
                occurrence.query,
                profile.queries().value().iter().nth(i).unwrap().value()
            ));
            assert_eq!(occurrence.query.name_reject().value(), "^CHR$");
            let CaptureBinding::Present(name) = &occurrence.name else {
                panic!("native CHR token");
            };
            assert_eq!(name.node_index, occurrence.matched.node_index);
            let CaptureBinding::Present(qualifier) = &occurrence.qualifier else {
                panic!("native CHR qualifier");
            };
            assert_eq!(
                doc.syntax.nodes[qualifier.node_index].kind,
                "string_function"
            );
        }
    });
    let operation = context();
    with_input(source, &operation, |input| {
        operation.cancellation().cancel();
        assert!(
            matches!(parse_semantic_occurrences(profile,input), Err(SemanticError::Operation(e)) if e.kind()==ErrorKind::Interrupted)
        );
    });
    let operation = SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    );
    with_input(source, &operation, |input| {
        std::thread::sleep(Duration::from_millis(25));
        assert!(
            matches!(parse_semantic_occurrences(profile,input), Err(SemanticError::Operation(e)) if e.kind()==ErrorKind::TimedOut)
        );
    });
}

#[test]
fn dedicated_plsql_invalid_query_and_recovery_are_not_empty_success() {
    let invalid = catalog(br#"language: plsql
parser: antlr4
grammar: antlr-plsql
queries: [{data_key: calls, type: relation, relation_type: CALLS, pattern: '//not_a_native_rule/CHR'}]
"#);
    with_input("SELECT 1 FROM dual;", &context(), |input| {
        assert!(matches!(
            parse_semantic_occurrences(&invalid.profiles()[0], input),
            Err(SemanticError::Engine {
                cause: AntlrEngineError::QueryFailure { .. },
                ..
            })
        ));
    });
    let stock = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    with_input("SELECT CAST( AS NUMBER) FROM dual;", &context(), |input| {
        let document = parse_semantic_occurrences(&stock.profiles()[0], input).unwrap();
        assert!(!document.syntax.diagnostics.is_empty());
        assert!(
            document
                .syntax
                .nodes
                .iter()
                .any(|n| n.error || n.missing || n.class == NodeClass::Error)
        );
    });
}

#[test]
fn oracle_collection_statement_raw_selectors_are_native() {
    let draft = catalog(include_bytes!("fixtures/catalog/shipped/plsql.yaml"));
    let profile = &draft.profiles()[0];
    assert_eq!(profile.queries().value().len(), 177);
    let source = "BEGIN items.DELETE; items.DELETE(1); items.DELETE(1,2); items.EXTEND; items.EXTEND(2); items.EXTEND(2,3); items.TRIM; items.TRIM(2); END; /";
    with_input(source, &context(), |input| {
        let raw = parse_semantic_occurrences(profile, input).unwrap();
        assert!(
            raw.syntax.diagnostics.is_empty(),
            "{:?}",
            raw.syntax.diagnostics
        );
        let calls: Vec<_> = raw
            .occurrences
            .iter()
            .filter(|o| {
                o.query
                    .pattern()
                    .value()
                    .starts_with("//collection_method_call/")
            })
            .collect();
        assert_eq!(calls.len(), 8);
        let mut heads = Vec::new();
        for occurrence in calls {
            let node = &raw.syntax.nodes[occurrence.matched.node_index];
            assert_eq!(node.class, NodeClass::Terminal);
            assert_eq!(node.status, SpanStatus::Regular);
            let CaptureBinding::Present(qualifier) = &occurrence.qualifier else {
                panic!("native qualifier")
            };
            assert_eq!(node.parent, Some(qualifier.node_index));
            assert_eq!(
                raw.syntax.nodes[qualifier.node_index].kind,
                "collection_method_call"
            );
            assert_eq!(occurrence.query.relation_type().value(), "CALLS");
            assert_eq!(occurrence.query.data_key().value(), "calls");
            assert!(matches!(occurrence.name, CaptureBinding::Present(_)));
            heads.push(node.kind.as_str());
        }
        heads.sort();
        assert_eq!(
            heads,
            [
                "DELETE", "DELETE", "DELETE", "EXTEND", "EXTEND", "EXTEND", "TRIM", "TRIM"
            ]
        );
    });
    with_input(
        "BEGIN v := items.COUNT; v := items.FIRST; items(1) := fn(2); END; /",
        &context(),
        |input| {
            let raw = parse_semantic_occurrences(profile, input).unwrap();
            assert!(raw.syntax.diagnostics.is_empty());
            assert!(!raw.occurrences.iter().any(|o| {
                o.query
                    .pattern()
                    .value()
                    .starts_with("//collection_method_call/")
            }));
            assert!(raw.occurrences.iter().any(|o| {
                o.query.pattern().value() == "//general_element_part"
                    && o.query.child_rule().value() == "function_argument"
                    && matches!(&o.name, CaptureBinding::Present(name)
                        if name.prepared.as_ref().is_some_and(|span| span.text() == "fn"))
            }));
        },
    );
}
