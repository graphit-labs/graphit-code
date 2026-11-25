use graphit_ast::{
    RangeError, SourceText,
    entity_extent::{EntityExtent, EntityExtentError},
    entity_source::select_entity_source,
};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation, text_slice::Request};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};

fn original() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    )
}

#[test]
fn half_open_unicode_crlf_lf_and_eof_use_inclusive_original_lines() {
    let text = "α\r\nb\n";
    let source = SourceText::new(text.as_bytes(), text.len()).unwrap();
    let original = original();
    for (start, end, first, last, raw) in [
        (0, 4, 1, 1, "α\r\n"),
        (0, 5, 1, 2, "α\r\nb"),
        (0, 6, 1, 2, "α\r\nb\n"),
        (4, 6, 2, 2, "b\n"),
        (3, 4, 1, 1, "\n"),
    ] {
        let extent =
            EntityExtent::new(&source, source.range(start, end).unwrap(), &original).unwrap();
        assert_eq!((extent.start_line(), extent.end_line()), (first, last));
        assert_eq!((extent.range().start(), extent.range().end()), (start, end));
        assert_eq!(extent.range().text(), raw);
    }
    let source = SourceText::new("α".as_bytes(), 2).unwrap();
    let extent = EntityExtent::new(&source, source.range(0, 2).unwrap(), &original).unwrap();
    assert_eq!((extent.start_line(), extent.end_line()), (1, 1));
}

#[test]
fn newline_only_empty_logical_line_and_empty_spans_are_distinct() {
    let source = SourceText::new(b"a\n\nb", 4).unwrap();
    let original = original();
    let extent = EntityExtent::new(&source, source.range(2, 3).unwrap(), &original).unwrap();
    assert_eq!((extent.start_line(), extent.end_line()), (2, 2));
    assert_eq!(extent.range().text(), "\n");
    assert!(matches!(
        EntityExtent::new(&source, source.range(0, 0).unwrap(), &original),
        Err(EntityExtentError::EmptyRange)
    ));
    let empty = SourceText::new(b"", 1).unwrap();
    assert!(matches!(
        EntityExtent::new(&empty, empty.range(0, 0).unwrap(), &original),
        Err(EntityExtentError::EmptyRange)
    ));
}

#[test]
fn foreign_equal_allocations_and_relative_subslice_ranges_are_rejected() {
    let first = String::from("abc\nz");
    let foreign = String::from("abc\nz");
    let source = SourceText::new(first.as_bytes(), first.len()).unwrap();
    let other = SourceText::new(foreign.as_bytes(), foreign.len()).unwrap();
    let original = original();
    assert!(matches!(
        EntityExtent::new(&source, other.range(1, 3).unwrap(), &original),
        Err(EntityExtentError::ForeignRange)
    ));
    let relative = SourceText::new(&first.as_bytes()[4..], 1).unwrap();
    assert!(matches!(
        EntityExtent::new(&source, relative.range(0, 1).unwrap(), &original),
        Err(EntityExtentError::ForeignRange)
    ));
}

#[test]
fn canonical_utf8_range_error_preserves_actual_cause() {
    let source = SourceText::new("α".as_bytes(), 2).unwrap();
    let foreign = SourceText::new(b"a", 1).unwrap();
    let error = EntityExtent::new(&source, foreign.range(0, 1).unwrap(), &original())
        .err()
        .unwrap();
    assert!(matches!(
        error,
        EntityExtentError::Range(RangeError::NotCharBoundary)
    ));
    assert!(error.source().is_some());
}

#[test]
fn raw_bom_nul_crlf_and_multibyte_text_and_points_are_preserved() {
    let text = "\u{feff}α\0\r\nb";
    let source = SourceText::new(text.as_bytes(), text.len()).unwrap();
    let candidate = source.range(0, 8).unwrap();
    let extent = EntityExtent::new(&source, candidate, &original()).unwrap();
    assert_eq!(extent.range().text(), "\u{feff}α\0\r\n");
    assert_eq!((extent.range().start(), extent.range().end()), (0, 8));
    assert_eq!(extent.range().start_point(), candidate.start_point());
    assert_eq!(extent.range().end_point(), candidate.end_point());
    assert_eq!((extent.start_line(), extent.end_line()), (1, 1));
}

#[test]
fn same_original_cancellation_and_deadline_retain_io_causes() {
    let source = SourceText::new(b"a", 1).unwrap();
    let candidate = source.range(0, 1).unwrap();
    let cancelled = original();
    cancelled.cancellation().cancel();
    let deadline = SnapshotContext::new(Instant::now(), Cancellation::default());
    for (original, expected) in [
        (&cancelled, io::ErrorKind::Interrupted),
        (&deadline, io::ErrorKind::TimedOut),
    ] {
        let error = EntityExtent::new(&source, candidate, original)
            .err()
            .unwrap();
        assert!(error.source().is_some());
        match error {
            EntityExtentError::Operation(cause) => assert_eq!(cause.kind(), expected),
            _ => panic!("expected actual original operation error"),
        }
    }
    assert!(EntityExtent::new(&source, candidate, &original()).is_ok());
}

#[test]
fn converted_window_composes_with_actual_core_selection_and_global_totals() {
    let text = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj";
    let source = SourceText::new(text.as_bytes(), text.len()).unwrap();
    let original = original();
    let extent = EntityExtent::new(&source, source.range(4, 14).unwrap(), &original).unwrap();
    assert_eq!((extent.start_line(), extent.end_line()), (3, 7));
    let selection = Request {
        start_line: 2,
        end_line: 4,
        head: 2,
        tail: 1,
        line_numbers: true,
        ..Request::default()
    };
    let result = select_entity_source(
        text,
        extent.start_line(),
        extent.end_line(),
        &selection,
        &original,
    )
    .unwrap();
    assert_eq!(result.source, "   5: e");
    assert_eq!(
        (result.start_line, result.end_line, result.total_lines),
        (Some(5), Some(5), 10)
    );
}
