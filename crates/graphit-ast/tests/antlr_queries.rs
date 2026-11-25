use graphit_ast::antlr_engine::{
    AntlrEngineError, Dialect, ParsedQueries, SpanStatus, UnindexedReason, parse, parse_queries,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use std::{
    error::Error,
    io::ErrorKind,
    sync::Barrier,
    time::{Duration, Instant},
};

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}
fn paths(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
fn query<'s>(dialect: Dialect, source: &'s str, values: &[&str]) -> ParsedQueries<'s> {
    parse_queries(dialect, source, &paths(values), &context()).unwrap()
}
fn cases() -> [(Dialect, &'static str); 5] {
    [
        (Dialect::Db2, "SELECT x FROM t;"),
        (Dialect::TSql, "select N'café' as label;"),
        (Dialect::PostgreSql, "select 'café' as label;"),
        (Dialect::PlSql, "BEGIN NULL; END;"),
        (
            Dialect::Cobol85,
            "IDENTIFICATION DIVISION. PROGRAM-ID. HELLO. PROCEDURE DIVISION. STOP RUN.",
        ),
    ]
}

#[test]
fn five_native_roots_and_empty_query_list_preserve_parsing() {
    for (dialect, source) in cases() {
        let entry = dialect.entry_rule();
        let result = query(
            dialect,
            source,
            &[&format!("/{entry}"), &format!("//{entry}")],
        );
        assert_eq!(
            result
                .matches
                .iter()
                .map(|m| (m.query_index, m.node_index))
                .collect::<Vec<_>>(),
            [(0, 0), (1, 0)]
        );
        assert_eq!(result.syntax.nodes[0].kind, entry);
        assert!(result.syntax.diagnostics.is_empty(), "{dialect:?}");
        let empty = query(dialect, source, &[]);
        assert!(empty.matches.is_empty());
        assert_eq!(empty.syntax, parse(dialect, source, &context()).unwrap());
    }
}

#[test]
fn real_profile_name_paths_work_in_all_five_grammars() {
    let sql = "CREATE TABLE pedido (id INT);";
    for (dialect, source, path, parent, expected) in [
        (
            Dialect::PlSql,
            sql,
            "//create_table/table_name",
            "create_table",
            "pedido",
        ),
        (
            Dialect::PostgreSql,
            sql,
            "//createstmt/qualified_name",
            "createstmt",
            "pedido",
        ),
        (
            Dialect::Db2,
            "CREATE TABLE pedido LIKE modelo COMPRESS NO;",
            "//create_table_statement/table_name",
            "create_table_statement",
            "pedido",
        ),
        (
            Dialect::TSql,
            sql,
            "//create_table/table_name",
            "create_table",
            "pedido",
        ),
        (
            Dialect::Cobol85,
            cases()[4].1,
            "//programIdParagraph/programName",
            "programIdParagraph",
            "HELLO",
        ),
    ] {
        let mixed = format!("//{}{}", dialect.entry_rule(), path);
        let result = query(dialect, source, &[path, &mixed]);
        assert!(
            result.syntax.diagnostics.is_empty(),
            "{dialect:?}: {:?}",
            result.syntax.diagnostics
        );
        assert_eq!(result.matches.len(), 2, "{dialect:?}");
        assert_eq!(result.matches[0].query_index, 0);
        assert_eq!(result.matches[1].query_index, 1);
        assert_eq!(result.matches[0].node_index, result.matches[1].node_index);
        let node = &result.syntax.nodes[result.matches[0].node_index];
        assert_eq!(&source[node.span.clone().unwrap()], expected);
        assert_eq!(result.syntax.nodes[node.parent.unwrap()].kind, parent);
    }
}

#[test]
fn terminals_wildcards_inversion_and_duplicate_queries_have_real_ordinals() {
    let source = "-- café\r\nSeLeCt N'Árvore' AS label;";
    let result = query(
        Dialect::TSql,
        source,
        &[
            "//SELECT",
            "/tsql_file/SELECT",
            "//*",
            "//SELECT",
            "/!tsql_file",
            "//'SELECT'",
        ],
    );
    let selected = result
        .matches
        .iter()
        .filter(|m| m.query_index == 0)
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    let index = selected[0].node_index;
    assert_eq!(
        &source[result.syntax.nodes[index].span.clone().unwrap()],
        "SeLeCt"
    );
    assert!(
        !result
            .matches
            .iter()
            .any(|m| m.query_index == 1 || m.query_index == 4)
    );
    assert_eq!(
        result
            .matches
            .iter()
            .filter(|m| m.query_index == 2)
            .map(|m| m.node_index)
            .collect::<Vec<_>>(),
        (0..result.syntax.nodes.len()).collect::<Vec<_>>()
    );
    for duplicate in [3, 5] {
        assert_eq!(
            result
                .matches
                .iter()
                .filter(|m| m.query_index == duplicate)
                .map(|m| m.node_index)
                .collect::<Vec<_>>(),
            [index]
        );
    }
    assert!(
        result
            .matches
            .windows(2)
            .all(|pair| (pair[0].query_index, pair[0].node_index)
                < (pair[1].query_index, pair[1].node_index))
    );
    let overlap = query(Dialect::TSql, source, &["//*//*"]);
    assert!(
        overlap
            .matches
            .windows(2)
            .all(|pair| pair[0].node_index < pair[1].node_index)
    );
    assert!(
        overlap
            .matches
            .iter()
            .all(|m| m.node_index < overlap.syntax.nodes.len())
    );
}

#[test]
fn bad_queries_return_the_actual_native_cause_and_typed_index() {
    for invalid in [
        "",
        "//",
        "//unknown_rule",
        "//select_statement[SELECT]",
        "**",
        "//SELECT ",
        "//SELECT\0",
    ] {
        let error = parse_queries(
            Dialect::TSql,
            "SELECT 1;",
            &paths(&["//SELECT", invalid]),
            &context(),
        )
        .unwrap_err();
        assert!(error.source().is_some(), "{invalid:?}");
        match error {
            AntlrEngineError::QueryFailure {
                query_index,
                cause,
                control,
            } => {
                assert_eq!(query_index, 1, "{invalid:?}");
                assert!(!cause.what().is_empty());
                assert!(control.is_none());
            }
            other => panic!("{invalid:?}: {other:?}"),
        }
    }
}

#[test]
fn recovered_nodes_and_eof_keep_their_observed_spans() {
    let source = "IDENTIFICATION . PROGRAM-ID. HELLO. PROCEDURE DIVISION. STOP RUN.";
    let result = query(Dialect::Cobol85, source, &["//DIVISION", "//EOF"]);
    assert!(!result.syntax.diagnostics.is_empty());
    let recovered = result
        .matches
        .iter()
        .filter(|m| m.query_index == 0)
        .map(|m| &result.syntax.nodes[m.node_index])
        .find(|node| node.missing)
        .unwrap();
    assert!(recovered.error);
    assert_eq!(recovered.span, None);
    assert_eq!(
        recovered.status,
        SpanStatus::Unindexed(UnindexedReason::InsertedRecoveryToken)
    );
    let eof = result
        .matches
        .iter()
        .filter(|m| m.query_index == 1)
        .map(|m| &result.syntax.nodes[m.node_index])
        .collect::<Vec<_>>();
    assert_eq!(eof.len(), 1);
    assert_eq!(eof[0].span, Some(source.len()..source.len()));
    assert_eq!(eof[0].status, SpanStatus::Eof);
}

#[test]
fn selected_cobol_literal_keeps_compound_origin_and_actual_input_binding() {
    use graphit_ast::{
        antlr_engine::preprocessing::{CobolFormat, prepare_cobol},
        prepared_input::{OriginError, OriginPiece, OriginalProjection, PreparedInput},
    };
    let source = "000100 IDENTIFICATION DIVISION.\n000200 PROGRAM-ID. HELLO.\n000300 PROCEDURE DIVISION.\n000400     DISPLAY 'HEL\n000500-    'LO'.\n000600     STOP RUN.\n";
    let operation = context();
    let prepared = prepare_cobol(source, CobolFormat::Fixed, &operation).unwrap();
    let result = parse_queries(
        Dialect::Cobol85,
        prepared.text(),
        &paths(&["//*"]),
        &operation,
    )
    .unwrap();
    assert!(result.syntax.diagnostics.is_empty());
    let matched = result
        .matches
        .iter()
        .find(|m| {
            let node = &result.syntax.nodes[m.node_index];
            node.span
                .as_ref()
                .is_some_and(|span| prepared.text().get(span.clone()) == Some("'HELLO'"))
        })
        .unwrap()
        .node_index;
    prepared.with_origin_plan(|plan| {
        let input = PreparedInput::new(
            source.as_bytes(),
            prepared.text().as_bytes(),
            plan,
            &operation,
        )
        .unwrap();
        let projected = result.syntax.project(&input).unwrap();
        let Some(OriginalProjection::Compound(pieces)) = &projected.nodes[matched] else {
            panic!("literal must preserve compound pieces");
        };
        assert!(pieces.len() >= 2);
        let text = pieces
            .iter()
            .map(|piece| match piece {
                OriginPiece::Original(range) => range.text(),
                _ => panic!("literal piece must remain original"),
            })
            .collect::<String>();
        assert_eq!(text, "'HELLO'");
        let foreign = prepared.text().to_owned();
        let unrelated = query(Dialect::Cobol85, &foreign, &["//*"]);
        assert!(matches!(
            unrelated.syntax.project(&input),
            Err(OriginError::InvalidPlan)
        ));
    });
}

#[test]
fn selected_unicode_token_projects_to_the_exact_original_bytes() {
    use graphit_ast::prepared_input::{
        ByteSpan, OriginPlan, OriginSegment, OriginalProjection, PreparedInput,
    };
    let source = "\u{feff}-- café\r\nSeLeCt N'Árvore' AS label;\n";
    let operation = context();
    let result = parse_queries(Dialect::TSql, source, &paths(&["//SELECT"]), &operation).unwrap();
    let span = ByteSpan {
        start: 0,
        end: source.len() as u64,
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
        &operation,
    )
    .unwrap();
    let projected = result.syntax.project(&input).unwrap();
    let index = result.matches[0].node_index;
    let Some(OriginalProjection::Exact(range)) = &projected.nodes[index] else {
        panic!("copied token must be exact");
    };
    assert_eq!(range.text(), "SeLeCt");
    assert_eq!(range.start(), source.find("SeLeCt").unwrap());
    assert_eq!(range.end(), source.find("SeLeCt").unwrap() + 6);
}

#[test]
fn concurrent_queries_use_independent_native_roots_and_trees() {
    let barrier = Barrier::new(4);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..8 {
                    for (dialect, source) in cases() {
                        let result = query(
                            dialect,
                            source,
                            &[&format!("/{}", dialect.entry_rule()), "//*"],
                        );
                        assert_eq!(
                            (result.matches[0].query_index, result.matches[0].node_index),
                            (0, 0)
                        );
                        assert_eq!(result.matches.len(), result.syntax.nodes.len() + 1);
                        assert!(result.syntax.diagnostics.is_empty());
                        for node in &result.syntax.nodes {
                            if let Some(span) = &node.span {
                                assert!(source.get(span.clone()).is_some());
                            }
                        }
                    }
                }
            });
        }
    });
}

#[test]
fn original_cancellation_and_deadline_never_return_partial_queries() {
    let cancelled = context();
    cancelled.cancellation().cancel();
    assert!(
        matches!(parse_queries(Dialect::TSql, "SELECT 1;", &paths(&["//*"]), &cancelled), Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::Interrupted)
    );
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    assert!(
        matches!(parse_queries(Dialect::TSql, "SELECT 1;", &paths(&["//*"]), &expired), Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::TimedOut)
    );
    let operation = context();
    let source = "SELECT 1;\n".repeat(100_000);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(5));
            operation.cancellation().cancel();
        });
        assert!(
            matches!(parse_queries(Dialect::PostgreSql, &source, &paths(&["//*"]), &operation), Err(AntlrEngineError::Operation(error)) if error.kind() == ErrorKind::Interrupted)
        );
    });
}
