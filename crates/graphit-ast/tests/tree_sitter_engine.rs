use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use graphit_ast::tree_sitter_engine::{EngineError, GrammarLanguage, ParseControl};

fn json_language() -> GrammarLanguage {
    GrammarLanguage::from_language(tree_sitter_json::LANGUAGE.into()).unwrap()
}

#[test]
fn parses_real_grammar_and_preserves_utf8_crlf_ranges() {
    let language = json_language();
    let source = "{\r\n  \"name\": \"café 日本\"\r\n}";
    let tree = language.parse(source, ParseControl::default()).unwrap();
    assert_eq!(tree.source(), source);
    assert!(!tree.has_syntax_errors());
    let nodes = tree.nodes(ParseControl::default()).unwrap();
    assert_eq!(nodes[0].kind, "document");
    assert_eq!(nodes[0].range, 0..source.len());
    assert_eq!(nodes[0].parent, None);
    assert!(nodes.iter().any(|node| node.kind == "pair"));
    let range = tree.source_range(nodes[0].range.clone()).unwrap();
    assert_eq!(range.text(), source);
    for (index, node) in nodes.iter().enumerate() {
        assert!(source.get(node.range.clone()).is_some());
        if let Some(parent) = node.parent {
            assert!(parent < index);
        }
    }
}

#[test]
fn returns_syntax_diagnostics_without_rewriting_input() {
    let language = json_language();
    let source = "{\"name\": }";
    let tree = language.parse(source, ParseControl::default()).unwrap();
    assert!(tree.has_syntax_errors());
    assert_eq!(tree.source(), source);
    assert!(
        tree.nodes(ParseControl::default())
            .unwrap()
            .iter()
            .any(|node| node.error || node.missing)
    );
}

#[test]
fn sdk_queries_filter_captures_and_retain_property_metadata() {
    let language = json_language();
    let source = "{\r\n\"name\": \"café\", \"other\": \"x\"}";
    let tree = language.parse(source, ParseControl::default()).unwrap();
    let query = language
        .compile_query(
            r#"
      ((pair key: (string (string_content) @name)
              value: (string (string_content) @value))
       (#eq? @name "name") (#set! role "declaration"))
    "#,
        )
        .unwrap();
    let matches = tree.matches(&query, ParseControl::default()).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches,
        tree.matches(&query, ParseControl::default()).unwrap()
    );
    let nodes = tree.nodes(ParseControl::default()).unwrap();
    for capture in &matches[0].captures {
        let matching: Vec<_> = nodes
            .iter()
            .filter(|node| node.node_id == capture.node_id)
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].kind, "string_content");
        assert_eq!(matching[0].range, capture.range);
        assert_eq!(matching[0].start, capture.start);
        assert_eq!(matching[0].end, capture.end);
    }
    assert_eq!(matches[0].captures[0].text, "name");
    let value = &matches[0].captures[1];
    assert_eq!(value.name, "value");
    assert_eq!(value.text, "café");
    assert_eq!(&source[value.range.clone()], "café");
    assert_eq!(value.start.row, 1);
    assert_eq!(matches[0].properties[0].key, "role");
    assert_eq!(
        matches[0].properties[0].value.as_deref(),
        Some("declaration")
    );
}

#[test]
fn rejects_invalid_or_unimplemented_query_semantics() {
    let language = json_language();
    assert!(matches!(
        language.compile_query("(nonexistent_kind) @x"),
        Err(EngineError::Query(_))
    ));
    assert!(matches!(
        language.compile_query("((string) @x (#unknown? @x))"),
        Err(EngineError::UnsupportedQuerySemantics(_))
    ));
    assert!(matches!(
        language.compile_query("((string) @x (#is? local))"),
        Err(EngineError::UnsupportedQuerySemantics(_))
    ));
}

#[test]
fn tree_and_query_keep_generation_after_caller_drops_language() {
    let language = json_language();
    let query = language.compile_query("(number) @value").unwrap();
    let tree = language.parse("[1, 2]", ParseControl::default()).unwrap();
    drop(language);
    let captures = tree.matches(&query, ParseControl::default()).unwrap();
    assert_eq!(
        captures
            .iter()
            .map(|m| m.captures[0].text.as_str())
            .collect::<Vec<_>>(),
        ["1", "2"]
    );
    let other = json_language().compile_query("(number) @value").unwrap();
    assert!(matches!(
        tree.matches(&other, ParseControl::default()),
        Err(EngineError::GrammarMismatch)
    ));
}

#[test]
fn cancellation_deadline_and_next_fresh_parse() {
    let language = json_language();
    let cancelled = AtomicBool::new(true);
    let control = ParseControl {
        cancellation: Some(&cancelled),
        deadline: None,
    };
    assert!(matches!(
        language.parse("[]", control),
        Err(EngineError::Cancelled)
    ));
    cancelled.store(false, Ordering::Relaxed);
    let tree = language.parse("[]", control).unwrap();
    assert!(!tree.has_syntax_errors());
    let elapsed = ParseControl {
        cancellation: None,
        deadline: Some(Instant::now()),
    };
    assert!(matches!(
        language.parse("[]", elapsed),
        Err(EngineError::TimedOut)
    ));
    let query = language.compile_query("(array) @value").unwrap();
    cancelled.store(true, Ordering::Relaxed);
    assert!(matches!(
        tree.matches(&query, control),
        Err(EngineError::Cancelled)
    ));
}

#[test]
fn sdk_regex_and_negative_regex_predicates_preserve_active_queries() {
    let language = json_language();
    let tree = language
        .parse(r#"["keep", "skip", "keeper"]"#, ParseControl::default())
        .unwrap();
    let query = language
        .compile_query(
            r#"
      ((string_content) @value (#match? @value "^keep") (#not-match? @value "er$"))
    "#,
        )
        .unwrap();
    let matches = tree.matches(&query, ParseControl::default()).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].captures[0].text, "keep");
}

fn operation_context() -> graphit_core::file_io::SnapshotContext {
    graphit_core::file_io::SnapshotContext::new(
        Instant::now() + std::time::Duration::from_secs(60),
        graphit_core::file_lock::Cancellation::default(),
    )
}

fn assert_operation<T>(result: Result<T, EngineError>, kind: std::io::ErrorKind) {
    let Err(EngineError::Operation(error)) = result else {
        panic!("expected original operation error");
    };
    assert_eq!(error.kind(), kind);
    let error = EngineError::Operation(error);
    assert!(
        std::error::Error::source(&error)
            .unwrap()
            .is::<std::io::Error>()
    );
}

#[test]
fn contextual_walk_and_queries_preserve_identity_and_original_control() {
    let language = json_language();
    let context = operation_context();
    let tree = language
        .parse_with_context("[1, 2]", ParseControl::default(), &context)
        .unwrap();
    let query = language.compile_query("(number) @value").unwrap();
    let absent = language.compile_query("(string) @value").unwrap();
    drop(language);
    assert_eq!(
        tree.nodes(ParseControl::default()).unwrap(),
        tree.nodes_with_context(ParseControl::default(), &context)
            .unwrap()
    );
    assert_eq!(
        tree.matches(&query, ParseControl::default()).unwrap(),
        tree.matches_with_context(&query, ParseControl::default(), &context)
            .unwrap()
    );
    assert!(
        tree.matches_with_context(&absent, ParseControl::default(), &context)
            .unwrap()
            .is_empty()
    );
    let other = json_language().compile_query("(number) @value").unwrap();
    assert!(matches!(
        tree.matches_with_context(&other, ParseControl::default(), &context),
        Err(EngineError::GrammarMismatch)
    ));
    let optional_cancelled = AtomicBool::new(true);
    let cancelled_control = ParseControl {
        cancellation: Some(&optional_cancelled),
        deadline: None,
    };
    assert!(matches!(
        tree.nodes_with_context(cancelled_control, &context),
        Err(EngineError::Cancelled)
    ));
    assert!(matches!(
        tree.matches_with_context(&absent, cancelled_control, &context),
        Err(EngineError::Cancelled)
    ));
    let optional_expired = ParseControl {
        cancellation: None,
        deadline: Some(Instant::now()),
    };
    assert!(matches!(
        tree.nodes_with_context(optional_expired, &context),
        Err(EngineError::TimedOut)
    ));
    assert!(matches!(
        tree.matches_with_context(&query, optional_expired, &context),
        Err(EngineError::TimedOut)
    ));
    context.cancellation().cancel();
    // Original cancellation wins over an already elapsed optional deadline.
    assert_operation(
        tree.nodes_with_context(optional_expired, &context),
        std::io::ErrorKind::Interrupted,
    );
    assert_operation(
        tree.matches_with_context(&absent, optional_expired, &context),
        std::io::ErrorKind::Interrupted,
    );
    let expired = graphit_core::file_io::SnapshotContext::new(
        Instant::now(),
        graphit_core::file_lock::Cancellation::default(),
    );
    assert_operation(
        tree.nodes_with_context(ParseControl::default(), &expired),
        std::io::ErrorKind::TimedOut,
    );
    assert_operation(
        tree.matches_with_context(&absent, ParseControl::default(), &expired),
        std::io::ErrorKind::TimedOut,
    );
}

#[test]
fn running_walk_and_queries_observe_original_cancellation() {
    let language = json_language();
    let source = format!("[{}0]", "1,".repeat(100_000));
    let tree = language.parse(&source, ParseControl::default()).unwrap();
    let query = language.compile_query("(number) @value").unwrap();
    for walk in [true, false] {
        let context = operation_context();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(5));
                context.cancellation().cancel();
            });
            if walk {
                assert_operation(
                    tree.nodes_with_context(ParseControl::default(), &context),
                    std::io::ErrorKind::Interrupted,
                );
            } else {
                assert_operation(
                    tree.matches_with_context(&query, ParseControl::default(), &context),
                    std::io::ErrorKind::Interrupted,
                );
            }
        });
    }
}
