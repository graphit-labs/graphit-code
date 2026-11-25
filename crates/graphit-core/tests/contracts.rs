use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::{pagination, project_version, text_slice};
use serde_json::json;
use std::collections::HashMap;

fn select(text: &str, request: text_slice::Request) -> text_slice::Result {
    text_slice::apply(text, &request).unwrap()
}

#[test]
fn range_head_tail_apply_before_search_with_absolute_lines() {
    let result = select(
        "one\ntwo\nthree\nfour\nfive",
        text_slice::Request {
            start_line: 2,
            end_line: 5,
            head: 3,
            tail: 2,
            pattern: "four".into(),
            before: 1,
            ..Default::default()
        },
    );
    assert_eq!(result.total_lines, 5);
    assert_eq!((result.start_line, result.end_line), (Some(3), Some(4)));
    assert_eq!(result.source, "     3: three\n>    4: four");
    assert_eq!(
        result
            .matches
            .iter()
            .map(|m| (m.line_number, m.is_match))
            .collect::<Vec<_>>(),
        [(3, false), (4, true)]
    );
}

#[test]
fn literal_unicode_search_escapes_regex_and_formats_gaps() {
    let result = select(
        "ÉCOLE.*\nÉCOLE other\nécole.*",
        text_slice::Request {
            pattern: "école.*".into(),
            ..Default::default()
        },
    );
    assert_eq!(
        result
            .matches
            .iter()
            .map(|m| m.line_number)
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert!(result.matches.iter().all(|m| m.is_match));
    assert!(result.source.contains("\n---\n"));
}

#[test]
fn native_regex_supports_unicode_and_explicit_case_flags() {
    let result = select(
        "123\n١٢٣\nabc\nABC",
        text_slice::Request {
            pattern: r"^(\d+|(?i:abc))$".into(),
            is_regex: true,
            ..Default::default()
        },
    );
    assert_eq!(result.matches.len(), 4);
    let result = select(
        "abc\nABC",
        text_slice::Request {
            pattern: "^abc$".into(),
            is_regex: true,
            ..Default::default()
        },
    );
    assert_eq!(result.matches.len(), 1);
}

#[test]
fn overlapping_context_is_unique_and_large_context_saturates() {
    for context in [1, i64::MAX] {
        let result = select(
            "a\nhit\nb\nhit\nc",
            text_slice::Request {
                pattern: "hit".into(),
                before: context,
                after: context,
                ..Default::default()
            },
        );
        assert_eq!(
            result
                .matches
                .iter()
                .map(|m| m.line_number)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert_eq!(result.matches.iter().filter(|m| m.is_match).count(), 2);
    }
}

#[test]
fn empty_lines_trailing_newline_and_empty_search_are_preserved() {
    let result = select(
        "a\n\nb\n",
        text_slice::Request {
            tail: 3,
            line_numbers: true,
            ..Default::default()
        },
    );
    assert_eq!(result.total_lines, 4);
    assert_eq!(result.source, "   2: \n   3: b\n   4: ");
    assert_eq!(select("", Default::default()).total_lines, 1);
    assert!(
        select(
            "a",
            text_slice::Request {
                pattern: "absent".into(),
                ..Default::default()
            }
        )
        .matches
        .is_empty()
    );
}

#[test]
fn invalid_selection_parameters_and_regex_are_rejected() {
    for request in [
        text_slice::Request {
            before: -1,
            ..Default::default()
        },
        text_slice::Request {
            start_line: 3,
            end_line: 1,
            ..Default::default()
        },
        text_slice::Request {
            pattern: "[".into(),
            is_regex: true,
            ..Default::default()
        },
    ] {
        assert!(text_slice::apply("a", &request).is_err());
    }
}

#[test]
fn window_rejects_invalid_bounds_without_clamping_metadata() {
    for (start, end) in [(0, 1), (2, 1), (1, 4), (4, 4), (usize::MAX, usize::MAX)] {
        assert!(text_slice::apply_window("a\nb\nc", &Default::default(), start, end).is_err());
    }
}

#[test]
fn window_rejects_invalid_requests_before_relative_clamping() {
    for request in [
        text_slice::Request {
            head: -1,
            ..Default::default()
        },
        text_slice::Request {
            tail: -1,
            ..Default::default()
        },
        text_slice::Request {
            start_line: -1,
            ..Default::default()
        },
        text_slice::Request {
            end_line: -1,
            ..Default::default()
        },
        text_slice::Request {
            before: -1,
            ..Default::default()
        },
        text_slice::Request {
            after: -1,
            ..Default::default()
        },
        text_slice::Request {
            start_line: 100,
            end_line: 99,
            ..Default::default()
        },
        text_slice::Request {
            pattern: "[".into(),
            is_regex: true,
            ..Default::default()
        },
    ] {
        assert!(text_slice::apply_window("a\nb\nc", &request, 2, 2).is_err());
    }
}

#[test]
fn window_ranges_are_relative_and_overshoot_uses_current_clamping() {
    let text = "one\ntwo\nthree\nfour\nfive";
    for (start, end, expected) in [
        (2, 0, "three\nfour"),
        (99, 100, "four"),
        (0, 2, "two\nthree"),
    ] {
        let result = text_slice::apply_window(
            text,
            &text_slice::Request {
                start_line: start,
                end_line: end,
                ..Default::default()
            },
            2,
            4,
        )
        .unwrap();
        assert_eq!(result.source, expected);
        assert_eq!(result.total_lines, 5);
        assert!(result.start_line.unwrap() >= 2);
        assert!(result.end_line.unwrap() <= 4);
    }
}

#[test]
fn window_range_head_tail_pattern_keep_global_lines_and_full_total() {
    let result = text_slice::apply_window(
        "outside\none\ntwo\nthree\nfour\nfive\noutside",
        &text_slice::Request {
            start_line: 2,
            end_line: 5,
            head: 3,
            tail: 2,
            pattern: "^four$".into(),
            is_regex: true,
            before: i64::MAX,
            after: i64::MAX,
            ..Default::default()
        },
        2,
        6,
    )
    .unwrap();
    assert_eq!(result.total_lines, 7);
    assert_eq!((result.start_line, result.end_line), (Some(4), Some(5)));
    assert_eq!(result.source, "     4: three\n>    5: four");
    assert_eq!(
        result
            .matches
            .iter()
            .map(|m| (m.line_number, m.is_match))
            .collect::<Vec<_>>(),
        [(4, false), (5, true)]
    );
}

#[test]
fn window_literal_context_never_leaks_outside_or_normalizes_empty_lines() {
    let result = text_slice::apply_window(
        "outside\nÉCOLE.*\n\nécole.*\noutside",
        &text_slice::Request {
            pattern: "école.*".into(),
            before: i64::MAX,
            after: i64::MAX,
            ..Default::default()
        },
        2,
        4,
    )
    .unwrap();
    assert_eq!(
        result
            .matches
            .iter()
            .map(|m| m.line_number)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(result.total_lines, 5);
    assert_eq!(
        text_slice::apply_window(
            "a\n",
            &text_slice::Request {
                line_numbers: true,
                ..Default::default()
            },
            2,
            2
        )
        .unwrap()
        .source,
        "   2: "
    );
    assert_eq!(
        text_slice::apply_window("", &Default::default(), 1, 1)
            .unwrap()
            .total_lines,
        1
    );
}

#[test]
fn window_full_file_preserves_existing_apply_results() {
    for text in ["", "a\nb\n", "one\nhit\nthree"] {
        for request in [
            text_slice::Request::default(),
            text_slice::Request {
                head: 2,
                tail: 1,
                line_numbers: true,
                ..Default::default()
            },
            text_slice::Request {
                start_line: 2,
                end_line: 99,
                ..Default::default()
            },
            text_slice::Request {
                pattern: "hit".into(),
                before: 1,
                after: 1,
                ..Default::default()
            },
        ] {
            assert_eq!(
                text_slice::apply_window(text, &request, 1, text.split('\n').count()).unwrap(),
                text_slice::apply(text, &request).unwrap()
            );
        }
    }
}

fn spec() -> pagination::Spec {
    pagination::Spec {
        page_size: 2,
        cursor: String::new(),
        total: 0,
        default_page_size: 0,
        bind: json!({"query": "hello"}),
    }
}

#[test]
fn pages_borrow_rows_and_resume_without_duplicates() {
    let rows = [json!(1), json!(2), json!(3), json!(4), json!(5)];
    let mut request = spec();
    let first = pagination::open(&request).unwrap();
    assert_eq!(first.fetch_limit, 3);
    let page = first.finish(&rows);
    assert!(std::ptr::eq(page.results[0], &rows[0]));
    assert_eq!(page.results, [&rows[0], &rows[1]]);
    request.cursor = page.next_cursor;
    let second = pagination::open(&request).unwrap();
    assert_eq!(second.offset, 2);
    let page = second.finish_fetched(&rows[2..]);
    assert_eq!(page.results, [&rows[2], &rows[3]]);
    request.cursor = page.next_cursor;
    let last = pagination::open(&request)
        .unwrap()
        .finish_fetched(&rows[4..]);
    assert_eq!(last.results, [&rows[4]]);
    assert!(last.next_cursor.is_empty());
}

#[test]
fn total_caps_fetch_and_stops_cursor() {
    let mut request = spec();
    request.total = 3;
    let rows = [1, 2, 3, 4];
    request.cursor = pagination::open(&request)
        .unwrap()
        .finish(&rows)
        .next_cursor;
    let window = pagination::open(&request).unwrap();
    assert_eq!(
        (window.offset, window.page_size, window.fetch_limit),
        (2, 1, 3)
    );
    let page = window.finish(&rows);
    assert_eq!(page.results, [&3]);
    assert!(page.next_cursor.is_empty());
    let mut cursor: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&request.cursor).unwrap()).unwrap();
    cursor["offset"] = json!(3);
    request.cursor = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap());
    assert!(pagination::open(&request).is_err());
}

#[test]
fn defaults_limits_and_empty_pages() {
    let mut request = spec();
    request.page_size = 0;
    assert_eq!(pagination::open(&request).unwrap().page_size, 20);
    request.default_page_size = 200;
    assert_eq!(pagination::open(&request).unwrap().page_size, 100);
    request.total = 3;
    assert_eq!(pagination::open(&request).unwrap().page_size, 3);
    assert!(
        pagination::open(&request)
            .unwrap()
            .finish::<u8>(&[])
            .next_cursor
            .is_empty()
    );
    for (size, total) in [(-1, 0), (101, 0), (2, -1)] {
        request.page_size = size;
        request.total = total;
        assert!(pagination::open(&request).is_err());
    }
}

#[test]
fn cursors_reject_changed_queries_sizes_and_totals() {
    let mut request = spec();
    request.cursor = pagination::open(&request)
        .unwrap()
        .finish(&[1, 2, 3])
        .next_cursor;
    for changed in [
        pagination::Spec {
            bind: json!({"query": "other"}),
            ..request.clone()
        },
        pagination::Spec {
            page_size: 3,
            ..request.clone()
        },
        pagination::Spec {
            total: 5,
            ..request.clone()
        },
    ] {
        assert!(pagination::open(&changed).is_err());
    }
}

#[test]
fn independent_nested_maps_produce_the_same_query_binding() {
    let mut request = pagination::Spec {
        bind: HashMap::from([
            ("a", HashMap::from([("b", 2), ("a", 1)])),
            ("b", HashMap::from([("c", 3)])),
        ]),
        page_size: 2,
        cursor: String::new(),
        total: 0,
        default_page_size: 0,
    };
    request.cursor = pagination::open(&request)
        .unwrap()
        .finish(&[1, 2, 3])
        .next_cursor;
    for _ in 0..64 {
        request.bind = HashMap::from([
            ("b", HashMap::from([("c", 3)])),
            ("a", HashMap::from([("a", 1), ("b", 2)])),
        ]);
        assert_eq!(pagination::open(&request).unwrap().offset, 2);
    }
}

#[test]
fn malformed_noncanonical_and_overflowing_cursors_are_rejected() {
    let mut request = spec();
    let valid = pagination::open(&request)
        .unwrap()
        .finish(&[1, 2, 3])
        .next_cursor;
    for cursor in [
        "%%%".into(),
        format!("{valid}\n"),
        format!("{valid}="),
        URL_SAFE_NO_PAD.encode(br#"{"OFFSET":2,"fingerprint":"x"}"#),
        URL_SAFE_NO_PAD.encode(br#"{"offset":-1,"fingerprint":"x"}"#),
    ] {
        request.cursor = cursor;
        assert!(pagination::open(&request).is_err());
    }
    let mut cursor: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(valid).unwrap()).unwrap();
    cursor["offset"] = json!(usize::MAX);
    request.cursor = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap());
    assert!(pagination::open(&request).is_err());
    cursor["offset"] = json!(2);
    cursor["extra"] = json!(true);
    request.cursor = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap());
    assert!(pagination::open(&request).is_err());
}

#[test]
fn version_names_validate_and_branch_encoding_avoids_collisions() {
    assert_eq!(project_version::validate(" feature ").unwrap(), "feature");
    assert!(project_version::validate("bad\0name").is_err());
    assert_eq!(project_version::lance_branch_name("main", "main"), "");
    assert_eq!(
        project_version::lance_branch_name("feature/a", "main"),
        "feature/a"
    );
    let encoded = project_version::lance_branch_name("feature with space", "main");
    assert!(encoded.starts_with("h-"));
    assert_ne!(
        project_version::lance_branch_name(&encoded, "main"),
        encoded
    );
}
