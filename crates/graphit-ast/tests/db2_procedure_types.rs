use graphit_ast::{
    antlr_engine::{
        AntlrEngineError, CapturePath, CaptureStep, Dialect, NodeClass, ParsedProfileQueries,
        ParsedSyntax, ProfileQuery, SpanStatus, parse, parse_profile_queries, parse_strict,
    },
    prepared_input::{
        ByteSpan, OriginError, OriginPlan, OriginSegment, OriginalProjection, PreparedInput,
    },
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    io::ErrorKind,
    time::{Duration, Instant},
};

const C_OPTIONS: &str = "LANGUAGE C PARAMETER STYLE SQL NO SQL EXTERNAL NAME 'pkg.proc';";
const JAVA_OPTIONS: &str = "LANGUAGE JAVA PARAMETER STYLE JAVA NO SQL EXTERNAL NAME 'pkg.proc';";

fn operation() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}

fn external(parameters: &str, options: &str) -> String {
    format!("-- α\r\nCREATE PROCEDURE SCHEMA_F.PROC_F({parameters}) {options}")
}

fn sql(parameters: &str) -> String {
    format!(
        "-- α\r\nCREATE PROCEDURE SCHEMA_F.PROC_F({parameters}) LANGUAGE SQL BEGIN DECLARE LOCAL_F INTEGER; SET LOCAL_F=1; END;"
    )
}

fn query(pattern: &str, captures: Vec<CapturePath>) -> ProfileQuery {
    ProfileQuery {
        pattern: pattern.into(),
        captures,
        ..ProfileQuery::default()
    }
}

fn selected(pattern: &str) -> ProfileQuery {
    query(pattern, vec![Vec::new()])
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

#[test]
fn external_xml_clob_parameters_retain_native_types_names_and_defaults() {
    let queries = [
        query(
            "//proc_ext_param/procedure_external_xml_type",
            vec![
                Vec::new(),
                vec![CaptureStep::XPath("/integer_value".into())],
                vec![CaptureStep::XPath("/kmg".into())],
            ],
        ),
        selected("//proc_ext_param/parameter_name"),
        selected("//proc_ext_param/routine_default_clause/routine_default_value"),
    ];
    for size in ["0", "1", "2048", "2K", "2M", "2G"] {
        let xml_type = format!("Xml /* β /* γ */ δ */ aS ClOb({size})");
        let source = external(
            &format!(
                "IN \"Pé\" {xml_type} DEFAULT NULL, OUT P_OUT XML AS CLOB(2048), INOUT P_IO XML AS CLOB(2K), XML AS CLOB(2M)"
            ),
            C_OPTIONS,
        );
        let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
        assert_native_source(&source, &parsed.syntax);
        assert_eq!(
            raw_matches(&source, &parsed, 0),
            [
                xml_type.as_str(),
                "XML AS CLOB(2048)",
                "XML AS CLOB(2K)",
                "XML AS CLOB(2M)",
            ]
        );
        assert_eq!(
            raw_matches(&source, &parsed, 1),
            ["\"Pé\"", "P_OUT", "P_IO"]
        );
        assert_eq!(raw_matches(&source, &parsed, 2), ["NULL"]);
        let types: Vec<_> = parsed
            .matches
            .iter()
            .filter(|matched| matched.query_index == 0)
            .collect();
        assert!(
            types
                .windows(2)
                .all(|pair| pair[0].node_index < pair[1].node_index)
        );
        for matched in &types {
            let node = &parsed.syntax.nodes[matched.node_index];
            assert_eq!(
                parsed.syntax.nodes[node.parent.unwrap()].kind,
                "proc_ext_param"
            );
            let integer = &parsed.syntax.nodes[matched.captures[1].unwrap()];
            assert_eq!(integer.kind, "integer_value");
            assert_eq!(integer.parent, Some(matched.node_index));
        }
        let integer = &parsed.syntax.nodes[types[0].captures[1].unwrap()];
        assert_eq!(
            source.get(integer.span.clone().unwrap()),
            Some(size.trim_end_matches(['K', 'M', 'G']))
        );
        if size.ends_with(['K', 'M', 'G']) {
            let unit = &parsed.syntax.nodes[types[0].captures[2].unwrap()];
            assert_eq!(unit.kind, "kmg");
            assert_eq!(
                source.get(unit.span.clone().unwrap()),
                Some(&size[size.len() - 1..])
            );
        } else {
            assert_eq!(types[0].captures[2], None);
        }
        for kind in ["LINE_COMMENT", "SQL_COMMENT"] {
            assert!(
                parsed
                    .syntax
                    .tokens
                    .iter()
                    .any(|token| token.kind == kind && token.channel != 0)
            );
        }
        assert_eq!(
            parsed
                .syntax
                .nodes
                .iter()
                .filter(|node| node.kind == "create_procedure_statement")
                .count(),
            1
        );
    }
}

#[test]
fn existing_procedure_types_keep_direct_native_contexts() {
    let queries = [
        selected("//proc_ext_param/data_type"),
        selected("//proc_parameter_list_item/data_type"),
        selected("//proc_ext_param/parameter_name"),
        selected("//proc_parameter_list_item/parameter_name"),
    ];
    let common = [
        "SMALLINT",
        "INTEGER",
        "BIGINT",
        "DEC(10,2)",
        "FLOAT(53)",
        "REAL",
        "DOUBLE PRECISION",
        "CHAR(1)",
        "VARCHAR(20 OCTETS)",
        "CHARACTER VARYING(20)",
        "CLOB(2M)",
        "GRAPHIC(1 CODEUNITS16)",
        "VARGRAPHIC(20)",
        "DBCLOB(2M)",
        "BINARY(1)",
        "VARBINARY(20)",
        "BLOB(2M)",
        "DATE",
        "TIME",
        "TIMESTAMP",
        "NCHAR(1)",
        "NVARCHAR(20)",
        "NCLOB(2M)",
        "TYPE_F",
        "SCHEMA_F.TYPE_F",
        "SCHEMA_F.MODULE_F.TYPE_F",
        "\"Schéma\".\"Module\".\"Type\"",
    ];
    for sql_family in [false, true] {
        let additional: &[&str] = if sql_family {
            &[
                "BOOLEAN",
                "CURSOR",
                "DECFLOAT(16)",
                "TIMESTAMP(12)",
                "XML",
                "ANCHOR GLOBAL_F",
                "ANCHOR DATA TYPE TO SCHEMA_F.MODULE_F.GLOBAL_F",
                "ANCHOR DATA TYPE TO SCHEMA_F.TABLE_F.COLUMN_F",
                "ANCHOR ROW TABLE_F",
                "ANCHOR ROW OF SCHEMA_F.TABLE_F",
            ]
        } else {
            &["DB2SECURITYLABEL", "SYSPROC.DB2SECURITYLABEL"]
        };
        for data_type in common.iter().chain(additional) {
            let parameters = format!("IN \"Pé\" {data_type}");
            let source = if sql_family {
                sql(&parameters)
            } else {
                external(&parameters, C_OPTIONS)
            };
            let parsed =
                parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
            assert_native_source(&source, &parsed.syntax);
            assert_eq!(
                raw_matches(&source, &parsed, usize::from(sql_family)),
                [*data_type]
            );
            assert_eq!(
                raw_matches(&source, &parsed, usize::from(sql_family) + 2),
                ["\"Pé\""]
            );
            assert_eq!(parsed.matches.len(), 2);
            let matched = parsed
                .matches
                .iter()
                .find(|matched| matched.query_index == usize::from(sql_family))
                .unwrap();
            assert_eq!(
                parsed.syntax.nodes[parsed.syntax.nodes[matched.node_index].parent.unwrap()].kind,
                if sql_family {
                    "proc_parameter_list_item"
                } else {
                    "proc_ext_param"
                }
            );
        }
    }
    let source = external("IN P_BOOLEAN BOOLEAN", JAVA_OPTIONS);
    let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &operation()).unwrap();
    assert_native_source(&source, &parsed.syntax);
    assert_eq!(raw_matches(&source, &parsed, 0), ["BOOLEAN"]);
}

#[test]
fn malformed_xml_representations_retain_diagnostics_and_strict_errors() {
    for annotation in [
        "XML AS BLOB(2M)",
        "XML AS DBCLOB(2M)",
        "XML AS CLOB()",
        "XML AS CLOB(M)",
        "XML AS CLOB(2M",
        "XML AS CLOB(2M) AS CLOB(2M)",
        "INTEGER AS CLOB(2M)",
    ] {
        let source = external(&format!("IN P_XML {annotation}"), C_OPTIONS);
        let syntax = parse(Dialect::Db2, &source, &operation()).unwrap();
        assert!(!syntax.diagnostics.is_empty(), "{source}");
        assert!(
            matches!(
                parse_strict(Dialect::Db2, &source, &operation()),
                Err(AntlrEngineError::Syntax { .. })
            ),
            "{source}"
        );
        for token in syntax.tokens.iter().filter(|token| !token.eof) {
            if let (Some(span), Some(text)) = (&token.span, &token.text) {
                assert_eq!(source.get(span.clone()), Some(text.as_str()));
            }
        }
    }
    let source = sql("IN P_XML XML AS CLOB(2M)");
    assert!(matches!(
        parse_strict(Dialect::Db2, &source, &operation()),
        Err(AntlrEngineError::Syntax { .. })
    ));
}

#[test]
fn xml_queries_preserve_source_binding_and_original_operation_causes() {
    let source = external("IN P_XML XML AS CLOB(2M)", C_OPTIONS);
    let queries = [selected("//proc_ext_param/procedure_external_xml_type")];
    let original = operation();
    let parsed = parse_profile_queries(Dialect::Db2, &source, &queries, &original).unwrap();
    assert_native_source(&source, &parsed.syntax);
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
        &original,
    )
    .unwrap();
    let projected = parsed.syntax.project(&input).unwrap();
    let selected_index = parsed.matches[0].node_index;
    assert!(
        matches!(&projected.nodes[selected_index], Some(OriginalProjection::Exact(range)) if range.text() == "XML AS CLOB(2M)")
    );
    let other_source = source.clone();
    let unrelated = parse_strict(Dialect::Db2, &other_source, &original).unwrap();
    assert!(matches!(
        unrelated.project(&input),
        Err(OriginError::InvalidPlan)
    ));

    let cancelled = operation();
    cancelled.cancellation().cancel();
    assert!(
        matches!(parse_profile_queries(Dialect::Db2, &source, &queries, &cancelled),
        Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::Interrupted)
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    );
    assert!(
        matches!(parse_profile_queries(Dialect::Db2, &source, &queries, &expired),
        Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::TimedOut)
    );
    let invalid_query = [selected(
        "//proc_ext_param/procedure_external_xml_type/MISSING_VOCABULARY_F",
    )];
    assert!(matches!(
        parse_profile_queries(Dialect::Db2, &source, &invalid_query, &original),
        Err(AntlrEngineError::QueryFailure { query_index: 0, .. })
    ));
}
