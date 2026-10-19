use graphit_ast::{entity_source::select_entity_source, source_inspection::SourceInspectionError};
use graphit_core::{
    file_io::SnapshotContext,
    file_lock::Cancellation,
    text_slice::{Match, Request},
};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

const TEN_LINES: &str = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj";

fn original() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

#[test]
fn relative_range_head_tail_preserve_global_numbers_and_full_total() {
    let selection = Request {
        start_line: 2,
        end_line: 4,
        head: 2,
        tail: 1,
        line_numbers: true,
        ..Request::default()
    };
    let result = select_entity_source(TEN_LINES, 3, 7, &selection, &original()).unwrap();
    assert_eq!(result.source, "   5: e");
    assert_eq!(result.total_lines, 10);
    assert_eq!((result.start_line, result.end_line), (Some(5), Some(5)));
    assert!(result.matches.is_empty());
}

#[test]
fn defaults_positive_overshoot_and_zero_sentinels_stay_in_window() {
    let context = original();
    for selection in [
        Request::default(),
        Request {
            head: 100,
            tail: 100,
            ..Request::default()
        },
        Request {
            start_line: 1,
            end_line: 100,
            ..Request::default()
        },
    ] {
        let result = select_entity_source(TEN_LINES, 3, 7, &selection, &context).unwrap();
        assert_eq!(result.source, "c\nd\ne\nf\ng");
        assert_eq!(result.total_lines, 10);
        assert_eq!((result.start_line, result.end_line), (Some(3), Some(7)));
    }
    for end_line in [0, 110] {
        let selection = Request {
            start_line: 100,
            end_line,
            ..Request::default()
        };
        let result = select_entity_source(TEN_LINES, 3, 7, &selection, &context).unwrap();
        assert_eq!(result.source, "g");
        assert_eq!(result.total_lines, 10);
        assert_eq!((result.start_line, result.end_line), (Some(7), Some(7)));
    }
}

#[test]
fn pattern_context_uses_global_matches_and_cannot_escape_selected_window() {
    let selection = Request {
        start_line: 2,
        end_line: 4,
        pattern: "E".into(),
        before: 100,
        after: 100,
        ..Request::default()
    };
    let result = select_entity_source(TEN_LINES, 3, 7, &selection, &original()).unwrap();
    assert_eq!(result.total_lines, 10);
    assert_eq!((result.start_line, result.end_line), (Some(4), Some(6)));
    assert_eq!(
        result.matches,
        vec![
            Match {
                line_number: 4,
                line: "d".into(),
                is_match: false
            },
            Match {
                line_number: 5,
                line: "e".into(),
                is_match: true
            },
            Match {
                line_number: 6,
                line: "f".into(),
                is_match: false
            },
        ]
    );
    assert_eq!(result.source, "     4: d\n>    5: e\n     6: f");
}

#[test]
fn regex_separators_no_match_and_invalid_regex_preserve_core_outcomes() {
    let context = original();
    let mut selection = Request {
        pattern: "c|g".into(),
        is_regex: true,
        ..Request::default()
    };
    let result = select_entity_source(TEN_LINES, 3, 7, &selection, &context).unwrap();
    assert_eq!(result.total_lines, 10);
    assert_eq!(result.source, ">    3: c\n---\n>    7: g");
    assert_eq!(
        result
            .matches
            .iter()
            .map(|m| m.line_number)
            .collect::<Vec<_>>(),
        vec![3, 7]
    );

    selection.pattern = "C".into();
    let no_match = select_entity_source(TEN_LINES, 3, 7, &selection, &context).unwrap();
    assert_eq!(no_match.total_lines, 10);
    assert!(no_match.source.is_empty());
    assert!(no_match.matches.is_empty());

    selection.pattern = "[".into();
    match select_entity_source(TEN_LINES, 3, 7, &selection, &context).unwrap_err() {
        SourceInspectionError::Selection(cause) => {
            assert!(cause.contains("unclosed character class"))
        }
        other => panic!("expected Core selection error, got {other}"),
    }
}

#[test]
fn invalid_windows_and_original_requests_fail_before_clamping() {
    let context = original();
    for (start, end) in [(0, 1), (7, 6), (1, 11), (11, 11), (usize::MAX, usize::MAX)] {
        assert!(matches!(
            select_entity_source(TEN_LINES, start, end, &Request::default(), &context),
            Err(SourceInspectionError::Selection(_))
        ));
    }
    for selection in [
        Request {
            head: -1,
            ..Request::default()
        },
        Request {
            tail: -1,
            ..Request::default()
        },
        Request {
            start_line: -1,
            ..Request::default()
        },
        Request {
            end_line: -1,
            ..Request::default()
        },
        Request {
            before: -1,
            ..Request::default()
        },
        Request {
            after: -1,
            ..Request::default()
        },
    ] {
        match select_entity_source(TEN_LINES, 3, 7, &selection, &context).unwrap_err() {
            SourceInspectionError::Selection(cause) => {
                assert_eq!(cause, "selection parameters cannot be negative");
            }
            other => panic!("expected Core validation error, got {other}"),
        }
    }
    let reversed = Request {
        start_line: 100,
        end_line: 99,
        ..Request::default()
    };
    match select_entity_source(TEN_LINES, 3, 7, &reversed, &context).unwrap_err() {
        SourceInspectionError::Selection(cause) => {
            assert_eq!(cause, "end_line must be at least start_line");
        }
        other => panic!("expected reversed request error, got {other}"),
    }
}

#[test]
fn raw_bytes_empty_source_and_final_empty_line_remain_visible() {
    let context = original();
    let text = "\u{feff}alpha\r\nβ\0\r\nlast\n";
    let result = select_entity_source(text, 1, 3, &Request::default(), &context).unwrap();
    assert_eq!(result.source, "\u{feff}alpha\r\nβ\0\r\nlast");
    assert_eq!(result.total_lines, 4);
    assert_eq!((result.start_line, result.end_line), (Some(1), Some(3)));

    let trailing = select_entity_source("a\nb\n", 2, 3, &Request::default(), &context).unwrap();
    assert_eq!(trailing.source, "b\n");
    assert_eq!(trailing.total_lines, 3);
    assert_eq!((trailing.start_line, trailing.end_line), (Some(2), Some(3)));

    let empty = select_entity_source("", 1, 1, &Request::default(), &context).unwrap();
    assert_eq!(empty.source, "");
    assert_eq!(empty.total_lines, 1);
    assert_eq!((empty.start_line, empty.end_line), (Some(1), Some(1)));
}

#[test]
fn actual_original_cancellation_and_deadline_preserve_io_causes() {
    let cancelled = original();
    cancelled.cancellation().cancel();
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    for (context, expected) in [
        (&cancelled, io::ErrorKind::Interrupted),
        (&expired, io::ErrorKind::TimedOut),
    ] {
        let error =
            select_entity_source(TEN_LINES, 3, 7, &Request::default(), context).unwrap_err();
        assert!(error.source().is_some());
        match error {
            SourceInspectionError::Operation(cause) => assert_eq!(cause.kind(), expected),
            other => panic!("expected original operation error, got {other}"),
        }
    }
    let live = select_entity_source(TEN_LINES, 3, 7, &Request::default(), &original()).unwrap();
    assert_eq!(live.source, "c\nd\ne\nf\ng");
}
