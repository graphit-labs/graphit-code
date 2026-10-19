use graphit_ast::antlr_engine::{
    AntlrEngineError, CaptureStep, Dialect, ParsedProfileQueries, PreparedProfileQueries,
    ProfileQuery, SpanStatus, UnindexedReason, parse, parse_profile_queries,
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
fn xpath(path: &str) -> Vec<CaptureStep> {
    vec![CaptureStep::XPath(path.into())]
}
fn request(pattern: &str, captures: Vec<Vec<CaptureStep>>) -> ProfileQuery {
    ProfileQuery {
        pattern: pattern.into(),
        captures,
        ..ProfileQuery::default()
    }
}
struct Fixture<'s> {
    parsed: ParsedProfileQueries<'s>,
    source: &'s str,
}
impl<'s> std::ops::Deref for Fixture<'s> {
    type Target = ParsedProfileQueries<'s>;
    fn deref(&self) -> &Self::Target {
        &self.parsed
    }
}
fn query<'s>(dialect: Dialect, source: &'s str, queries: &[ProfileQuery]) -> Fixture<'s> {
    Fixture {
        parsed: parse_profile_queries(dialect, source, queries, &context()).unwrap(),
        source,
    }
}
fn text<'s>(result: &Fixture<'s>, node: usize) -> &'s str {
    &result.source[result.syntax.nodes[node].span.clone().unwrap()]
}

#[test]
fn keyword_guards_are_direct_case_insensitive_and_precede_select_xpath() {
    let source = "CREATE OR REPLACE PACKAGE pkg AS C_MAX CONSTANT NUMBER:=42; V_URL VARCHAR2(128):='https://api.acme.com'; C_MIN constant NUMBER:=1; END;";
    let mut positive = request(
        "//variable_declaration",
        vec![
            xpath("/identifier"),
            xpath("/default_value_part/expression"),
        ],
    );
    positive.keyword = "CONSTANT".into();
    let mut negative = positive.clone();
    negative.negate = true;
    let mut intermediate = request("//variable_declaration", vec![]);
    intermediate.keyword = "constant".into();
    intermediate.select = "/identifier".into();
    let mut root = request("//sql_script", vec![]);
    root.keyword = "CONSTANT".into();
    let mut negative_root = root.clone();
    negative_root.negate = true;
    let result = query(
        Dialect::PlSql,
        source,
        &[positive, negative, intermediate, root, negative_root],
    );
    assert!(result.syntax.diagnostics.is_empty());
    for (index, names, values) in [
        (0, vec!["C_MAX", "C_MIN"], vec!["42", "1"]),
        (1, vec!["V_URL"], vec!["'https://api.acme.com'"]),
    ] {
        let matches: Vec<_> = result
            .matches
            .iter()
            .filter(|item| item.query_index == index)
            .collect();
        assert_eq!(
            matches
                .iter()
                .map(|item| text(&result, item.captures[0].unwrap()))
                .collect::<Vec<_>>(),
            names
        );
        assert_eq!(
            matches
                .iter()
                .map(|item| text(&result, item.captures[1].unwrap()))
                .collect::<Vec<_>>(),
            values
        );
    }
    assert_eq!(
        result
            .matches
            .iter()
            .filter(|item| item.query_index == 2)
            .map(|item| text(&result, item.node_index))
            .collect::<Vec<_>>(),
        ["C_MAX", "C_MIN"]
    );
    assert!(!result.matches.iter().any(|item| item.query_index == 3));
    assert_eq!(
        result
            .matches
            .iter()
            .filter(|item| item.query_index == 4)
            .map(|item| item.node_index)
            .collect::<Vec<_>>(),
        [0]
    );
}

#[test]
fn child_rule_guards_keep_native_types_and_nested_calls() {
    let source =
        "DECLARE TYPE r_t IS RECORD(id NUMBER); TYPE c_t IS REF CURSOR; BEGIN NULL; END; /";
    let mut positive = request("//type_declaration", vec![]);
    positive.child_rule = "record_type_def".into();
    positive.select = "/identifier".into();
    let mut negative = positive.clone();
    negative.negate = true;
    let result = query(Dialect::PlSql, source, &[positive, negative]);
    assert!(result.syntax.diagnostics.is_empty());
    assert_eq!(result.matches.len(), 2);
    for (item, (index, name)) in result.matches.iter().zip([(0, "r_t"), (1, "c_t")]) {
        assert_eq!(item.query_index, index);
        assert_eq!(text(&result, item.node_index), name);
        let parent = result.syntax.nodes[item.node_index].parent.unwrap();
        assert_eq!(result.syntax.nodes[parent].kind, "type_declaration");
    }
    for (source, names) in [
        (
            "BEGIN resultado:=calcular(ajustar(1)); END; /",
            vec!["calcular", "ajustar"],
        ),
        (
            "BEGIN resultado:=valor; texto:='function_argument'; END; /",
            vec![],
        ),
    ] {
        let mut calls = request("//general_element_part", vec![xpath("/id_expression")]);
        calls.child_rule = "function_argument".into();
        let result = query(Dialect::PlSql, source, &[calls]);
        assert!(result.syntax.diagnostics.is_empty());
        assert_eq!(
            result
                .matches
                .iter()
                .map(|item| text(&result, item.captures[0].unwrap()))
                .collect::<Vec<_>>(),
            names
        );
        assert!(
            result
                .matches
                .windows(2)
                .all(|pair| pair[0].node_index < pair[1].node_index)
        );
    }
}

#[test]
fn nearest_descendant_uses_breadth_first_ties_and_never_backtracks() {
    for (definition, expected) in [
        (
            "status TEXT DEFAULT 'ABERTO', id INT NOT NULL, vl INT CHECK(vl>0)",
            vec![Some("'ABERTO'"), None, None],
        ),
        ("status TEXT NOT NULL DEFAULT 'ABERTO'", vec![None]),
        (
            "status TEXT DEFAULT 'ABERTO' NOT NULL",
            vec![Some("'ABERTO'")],
        ),
    ] {
        let source = format!("CREATE TABLE pedido({definition});");
        let result = query(
            Dialect::PostgreSql,
            &source,
            &[request(
                "//columnDef",
                vec![
                    xpath("/colid"),
                    vec![
                        CaptureStep::XPath("/colquallist".into()),
                        CaptureStep::NearestDescendant("colconstraintelem".into()),
                        CaptureStep::XPath("/b_expr".into()),
                    ],
                    vec![CaptureStep::NearestDescendant("columnDef".into())],
                ],
            )],
        );
        assert!(result.syntax.diagnostics.is_empty());
        assert_eq!(
            result
                .matches
                .iter()
                .map(|item| item.captures[1].map(|node| text(&result, node)))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(result.matches.iter().all(|item| item.captures[2].is_none()));
        assert_eq!(
            text(&result, result.matches[0].captures[0].unwrap()),
            "status"
        );
    }
    let result = query(
        Dialect::PostgreSql,
        "CREATE TABLE pedido(status INT CHECK(status > 0));",
        &[request(
            "/root",
            vec![vec![
                CaptureStep::NearestDescendant("columnDef".into()),
                CaptureStep::NearestDescendant("a_expr".into()),
            ]],
        )],
    );
    assert_eq!(
        text(&result, result.matches[0].captures[0].unwrap()),
        "status > 0"
    );
}

#[test]
fn all_dialects_preserve_capture_slots_selected_identity_and_duplicate_queries() {
    for (dialect, source, pattern, path, expected) in [
        (
            Dialect::PlSql,
            "CREATE TABLE pedido(id NUMBER);",
            "//create_table",
            "/table_name",
            "pedido",
        ),
        (
            Dialect::PostgreSql,
            "CREATE TABLE pedido(id INT);",
            "//createstmt",
            "/qualified_name",
            "pedido",
        ),
        (
            Dialect::Db2,
            "CREATE TABLE pedido LIKE modelo COMPRESS NO;",
            "//create_table_statement",
            "/table_name",
            "pedido",
        ),
        (
            Dialect::TSql,
            "CREATE TABLE pedido(id INT);",
            "//create_table",
            "/table_name",
            "pedido",
        ),
        (
            Dialect::Cobol85,
            "IDENTIFICATION DIVISION. PROGRAM-ID. HELLO. PROCEDURE DIVISION. STOP RUN.",
            "//programIdParagraph",
            "/programName",
            "HELLO",
        ),
    ] {
        let item = request(pattern, vec![xpath(path), Vec::new(), xpath(path)]);
        let result = query(dialect, source, &[item.clone(), item]);
        assert!(result.syntax.diagnostics.is_empty(), "{dialect:?}");
        assert_eq!(result.matches.len(), 2, "{dialect:?}");
        for (index, matched) in result.matches.iter().enumerate() {
            assert_eq!(matched.query_index, index);
            assert_eq!(matched.captures[1], Some(matched.node_index));
            assert_eq!(matched.captures[0], matched.captures[2]);
            assert_eq!(text(&result, matched.captures[0].unwrap()), expected);
        }
        assert_eq!(result.matches[0].node_index, result.matches[1].node_index);
        let empty = query(dialect, source, &[]);
        assert!(empty.matches.is_empty());
        assert_eq!(empty.syntax, parse(dialect, source, &context()).unwrap());
    }
}

#[test]
fn official_xpath_token_wildcard_and_direct_child_selection_remain_available() {
    let result = query(
        Dialect::PlSql,
        "CREATE UNIQUE INDEX ix ON pedido(id);",
        &[request(
            "//create_index",
            vec![xpath("/UNIQUE"), xpath("/index_name")],
        )],
    );
    assert_eq!(
        text(&result, result.matches[0].captures[0].unwrap()),
        "UNIQUE"
    );
    assert_eq!(text(&result, result.matches[0].captures[1].unwrap()), "ix");
    let result = query(
        Dialect::TSql,
        "DECLARE @x INT, @y INT;",
        &[request("//declare_local/LOCAL_ID", vec![])],
    );
    assert!(result.syntax.diagnostics.is_empty());
    assert_eq!(
        result
            .matches
            .iter()
            .map(|item| text(&result, item.node_index))
            .collect::<Vec<_>>(),
        ["@x", "@y"]
    );
    let result = query(
        Dialect::TSql,
        "SELECT 1;",
        &[request("/tsql_file/*", vec![])],
    );
    assert!(!result.matches.is_empty());
}

fn assert_query_error(query: ProfileQuery, slot: Option<usize>) {
    let first = request("//SELECT", vec![]);
    match parse_profile_queries(Dialect::TSql, "SELECT 1;", &[first, query], &context())
        .unwrap_err()
    {
        AntlrEngineError::QueryFailure {
            query_index,
            cause,
            control,
        } => {
            assert_eq!(query_index, 1);
            assert!(!cause.what().is_empty());
            assert!(control.is_none());
            if let Some(slot) = slot {
                assert!(
                    cause.what().contains(&format!("capture {slot}:")),
                    "{cause}"
                );
            }
        }
        other => panic!("{other:?}"),
    }
}
#[test]
fn malformed_native_xpath_and_capture_rules_fail_even_without_matches() {
    for pattern in [
        "",
        "//",
        "//unknown_rule",
        "//SELECT ",
        "//SELECT\0",
        "\u{feff}//SELECT",
        "//tsql_file[]",
        "//SELECT[SELECT]",
        "//tsql_file[rule:batch]",
    ] {
        assert_query_error(request(pattern, vec![]), None);
    }
    for path in [
        "",
        "//unknown_rule",
        "/SELECT ",
        "/SELECT\0",
        "\u{feff}/SELECT",
        "/**/tsql_file",
        "/SELECT/",
    ] {
        assert_query_error(
            request("//declare_local", vec![Vec::new(), xpath(path)]),
            Some(1),
        );
    }
    for step in [
        CaptureStep::NearestAncestor("SELECT".into()),
        CaptureStep::NearestDescendant("unknown_rule".into()),
    ] {
        assert_query_error(request("//declare_local", vec![vec![step]]), Some(0));
    }
    let mut invalid = request("//tsql_file", vec![]);
    invalid.child_rule = "SELECT".into();
    assert_query_error(invalid, None);
    let mut invalid = request("//tsql_file", vec![]);
    invalid.negate = true;
    assert_query_error(invalid, None);
}

#[test]
fn unicode_bom_crlf_recovery_and_eof_preserve_original_coordinates() {
    let source = "\u{feff}select N'café🙂';\r\n";
    let result = query(
        Dialect::TSql,
        source,
        &[request("//SELECT", vec![]), request("//EOF", vec![])],
    );
    assert_eq!(text(&result, result.matches[0].node_index), "select");
    assert_eq!(
        result.syntax.nodes[result.matches[0].node_index].span,
        Some(3..9)
    );
    assert_eq!(
        result.syntax.nodes[result.matches[1].node_index].span,
        Some(source.len()..source.len())
    );
    let result = query(
        Dialect::Cobol85,
        "IDENTIFICATION . PROGRAM-ID. HELLO. PROCEDURE DIVISION. STOP RUN.",
        &[request("//DIVISION", vec![])],
    );
    let inserted = result
        .matches
        .iter()
        .map(|item| &result.syntax.nodes[item.node_index])
        .find(|node| node.missing)
        .unwrap();
    assert!(inserted.error);
    assert_eq!(inserted.span, None);
    assert_eq!(
        inserted.status,
        SpanStatus::Unindexed(UnindexedReason::InsertedRecoveryToken)
    );
}

#[test]
fn overlapping_descendant_roots_deduplicate_in_tree_order_and_exclude_self() {
    let result = query(
        Dialect::PostgreSql,
        "SELECT 1+(2+(3+(4+5)));",
        &[request("//a_expr//a_expr//a_expr", vec![])],
    );
    let expected: Vec<_> = result
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            if node.kind != "a_expr" {
                return None;
            }
            let mut ancestors = 0;
            let mut parent = node.parent;
            while let Some(index) = parent {
                let node = &result.syntax.nodes[index];
                if node.kind == "a_expr" {
                    ancestors += 1;
                }
                parent = node.parent;
            }
            (ancestors >= 2).then_some(index)
        })
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(
        result
            .matches
            .iter()
            .map(|item| item.node_index)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn prepared_selectors_reuse_grammar_binding_without_retaining_source_and_isolate_threads() {
    let operation = context();
    let requests = [request("//columnDef", vec![xpath("/colid")])];
    let prepared =
        PreparedProfileQueries::new(Dialect::PostgreSql, false, &requests, &operation).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let prepared = &prepared;
            scope.spawn(move || {
                for _ in 0..8 {
                    let source = "CREATE TABLE pedido(status TEXT DEFAULT 'ABERTO');";
                    let result = prepared.parse(source, &context()).unwrap();
                    assert_eq!(result.matches.len(), 1);
                    let captured = result.matches[0].captures[0].unwrap();
                    assert_eq!(
                        &source[result.syntax.nodes[captured].span.clone().unwrap()],
                        "status"
                    );
                }
            });
        }
    });
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = SnapshotContext::new(Instant::now() + Duration::from_secs(60), cancellation);
    assert!(
        matches!(prepared.parse("SELECT 1;", &cancelled), Err(AntlrEngineError::Operation(ref error)) if error.kind() == ErrorKind::Interrupted)
    );
    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_millis(1),
        Cancellation::default(),
    );
    assert!(
        matches!(prepared.parse("SELECT 1;", &expired), Err(AntlrEngineError::Operation(ref error)) if error.kind() == ErrorKind::TimedOut)
    );
    let cancellation = Cancellation::default();
    let operation = SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        cancellation.clone(),
    );
    let requests = [request(&"//tsql_file".repeat(100_000), vec![])];
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(10));
            cancellation.cancel();
        });
        let error =
            PreparedProfileQueries::new(Dialect::TSql, false, &requests, &operation).unwrap_err();
        assert!(
            matches!(error, AntlrEngineError::Operation(ref cause) if cause.kind() == ErrorKind::Interrupted),
            "{error:?}"
        );
    });
}
