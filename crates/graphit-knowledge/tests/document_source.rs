use graphit_core::{
    file_io::SnapshotContext,
    file_lock::Cancellation,
    text_slice::{self, Request},
};
use graphit_data::{
    knowledge_document_projection::{CompilationDigest32, PhysicalSourceDigest32},
    knowledge_index::{KnowledgeDocument, KnowledgeRecipeParameters},
};
use graphit_knowledge::document_source::{
    DocumentSourceError, DocumentSourceLimits, render_document_source,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant, UNIX_EPOCH},
};

fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}
fn limits() -> DocumentSourceLimits {
    DocumentSourceLimits {
        max_input_bytes: n(65536),
        max_targets: n(100),
        max_virtual_bytes: n(65536),
        max_virtual_lines: n(1000),
        max_output_bytes: n(65536),
        max_matches: n(1000),
    }
}
fn document() -> KnowledgeDocument {
    KnowledgeDocument {
        slug: "guide".into(),
        title: "Storage: 'yes'\n---\n世界".into(),
        body: "\u{feff}---\r\ntitle: Original\r\n---\r\n# Corpo 世界\r\n\r\n[[guide]]\r\n".into(),
        kind: "custom".into(),
        summary: "  Resumo\n---\r\n".into(),
        description: Some(String::new()),
        summary_truncated: true,
        source_path: "docs/lógico.md".into(),
        physical_digest: Some(PhysicalSourceDigest32::from_bytes([17; 32])),
        compilation_digest: CompilationDigest32::from_bytes([42; 32]),
        recipe: "authored-document-v1".into(),
        recipe_parameters: KnowledgeRecipeParameters {
            fallback_title: "fallback".into(),
            kind_on_absent: "document".into(),
            max_summary_bytes: n(16),
        },
        updated_at: Some(UNIX_EPOCH - Duration::from_nanos(17)),
        stale_since: Some(UNIX_EPOCH + Duration::from_nanos(9)),
        stale_after: Some(Duration::from_secs(60)),
        resolved_wiki_targets: vec![
            "guide".into(),
            "missing".into(),
            "guide".into(),
            "other".into(),
        ],
    }
}
fn header(source: &str) -> (Value, &str) {
    let (yaml, body) = source
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n\n")
        .unwrap();
    (serde_saphyr::from_str(yaml).unwrap(), body)
}
fn input_size(d: &KnowledgeDocument, pattern: &str) -> usize {
    [
        &d.slug,
        &d.title,
        &d.body,
        &d.kind,
        &d.summary,
        d.description.as_deref().unwrap_or(""),
        &d.source_path,
        &d.recipe,
        &d.recipe_parameters.fallback_title,
        &d.recipe_parameters.kind_on_absent,
        pattern,
    ]
    .into_iter()
    .chain(d.resolved_wiki_targets.iter().map(String::as_str))
    .map(str::len)
    .sum()
}
fn assert_limit(error: DocumentSourceError, label: &str) {
    assert!(
        matches!(&error,DocumentSourceError::Limit(found) if *found==label),
        "{error:?}"
    );
    assert!(error.source().is_none());
}

#[test]
fn native_header_roundtrips_full16_metadata_and_preserves_body() {
    let d = document();
    let full = render_document_source(&context(), &d, &Request::default(), limits()).unwrap();
    let (h, body) = header(&full.source);
    assert_eq!(h.as_object().unwrap().len(), 15);
    assert_eq!(
        h,
        json!({
            "id":d.slug,"title":d.title,"type":d.kind,"summary":d.summary,"description":"","summary_truncated":true,"source_path":d.source_path,
            "physical_digest":hex::encode([17;32]),"compilation_digest":hex::encode([42;32]),"recipe":d.recipe,
            "recipe_parameters":{"fallback_title":"fallback","kind_on_absent":"document","max_summary_bytes":16},
            "updated_at":"1969-12-31T23:59:59.999999983Z","stale_since":"1970-01-01T00:00:00.000000009Z","stale_after_ns":60_000_000_000u64,
            "resolved_wiki_targets":["guide","missing","guide","other"]
        })
    );
    assert_eq!(body.as_bytes(), d.body.as_bytes());
    assert!(body.starts_with('\u{feff}'));
    assert_eq!(full.total_lines, full.source.split('\n').count());
    assert!(full.matches.is_empty());
    assert_ne!(h["physical_digest"], h["compilation_digest"]);
}

#[test]
fn null_empty_and_found_empty_body_remain_distinct() {
    let mut d = document();
    d.title.clear();
    d.kind.clear();
    d.summary.clear();
    d.body.clear();
    d.description = None;
    d.physical_digest = None;
    d.updated_at = None;
    d.stale_since = None;
    d.stale_after = None;
    d.resolved_wiki_targets.clear();
    let full = render_document_source(&context(), &d, &Request::default(), limits()).unwrap();
    let (h, body) = header(&full.source);
    assert_eq!(h["title"], "");
    assert_eq!(h["type"], "");
    assert_eq!(h["summary"], "");
    for key in [
        "description",
        "physical_digest",
        "updated_at",
        "stale_since",
        "stale_after_ns",
    ] {
        assert_eq!(h[key], Value::Null);
    }
    assert_eq!(h["resolved_wiki_targets"], json!([]));
    assert_eq!(body, "");
    assert!(!full.source.is_empty());
    let absent = render_document_source(
        &context(),
        &d,
        &Request {
            pattern: "absent needle".into(),
            ..Default::default()
        },
        limits(),
    )
    .unwrap();
    assert!(absent.source.is_empty());
    assert!(absent.matches.is_empty());
    assert_eq!(absent.total_lines, full.total_lines);
}

#[test]
fn core_selection_keeps_absolute_lines_contexts_and_order() {
    let mut d = document();
    d.body = "zero\nHIT alpha\nhit beta\nlast\n".into();
    let original = context();
    let full = render_document_source(&original, &d, &Request::default(), limits()).unwrap();
    let start = full.source.split('\n').position(|l| l == "zero").unwrap() + 1;
    let requests = [
        Request {
            start_line: start as i64,
            end_line: (start + 3) as i64,
            head: 3,
            tail: 2,
            ..Default::default()
        },
        Request {
            pattern: "hit".into(),
            before: 1,
            after: 1,
            ..Default::default()
        },
        Request {
            pattern: "^hit ".into(),
            is_regex: true,
            ..Default::default()
        },
        Request {
            pattern: "(?i)^hit ".into(),
            is_regex: true,
            before: 1,
            after: 1,
            ..Default::default()
        },
        Request {
            start_line: start as i64,
            line_numbers: true,
            ..Default::default()
        },
        Request {
            tail: 2,
            ..Default::default()
        },
    ];
    for req in &requests {
        let actual = render_document_source(&original, &d, req, limits()).unwrap();
        let expected = text_slice::apply(&full.source, req).unwrap();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(actual.total_lines, full.total_lines);
    }
    let selected = render_document_source(&original, &d, &requests[0], limits()).unwrap();
    assert_eq!(selected.source, "HIT alpha\nhit beta");
    assert_eq!(selected.start_line, Some(start + 1));
    assert_eq!(selected.end_line, Some(start + 2));
    let matched = render_document_source(&original, &d, &requests[1], limits()).unwrap();
    assert_eq!(matched.matches.len(), 4);
    assert_eq!(
        matched
            .matches
            .iter()
            .map(|m| m.is_match)
            .collect::<Vec<_>>(),
        [false, true, true, false]
    );
    assert_eq!(
        matched
            .matches
            .iter()
            .map(|m| m.line_number)
            .collect::<Vec<_>>(),
        (start..=start + 3).collect::<Vec<_>>()
    );
    assert_eq!(
        render_document_source(&original, &d, &requests[2], limits())
            .unwrap()
            .matches
            .len(),
        1
    );
    assert_eq!(
        render_document_source(&original, &d, &requests[5], limits())
            .unwrap()
            .source,
        "last\n"
    );
    for invalid in [
        Request {
            head: -1,
            ..Default::default()
        },
        Request {
            start_line: 5,
            end_line: 2,
            ..Default::default()
        },
        Request {
            is_regex: true,
            pattern: "[".into(),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            render_document_source(&original, &d, &invalid, limits()),
            Err(DocumentSourceError::Selection(_))
        ));
    }
}

#[test]
fn every_logical_cap_accepts_exact_usage_and_rejects_one_below() {
    let mut d = document();
    d.body = "zero\nHIT alpha\nhit beta\nlast\n".into();
    let original = context();
    let request = Request {
        pattern: "hit".into(),
        before: 1,
        after: 1,
        ..Default::default()
    };
    let full = render_document_source(&original, &d, &Request::default(), limits()).unwrap();
    let selected = render_document_source(&original, &d, &request, limits()).unwrap();
    let output =
        selected.source.len() + selected.matches.iter().map(|m| m.line.len()).sum::<usize>();
    let input = input_size(&d, &request.pattern);
    let mut exact = limits();
    exact.max_input_bytes = n(input);
    exact.max_targets = n(d.resolved_wiki_targets.len());
    exact.max_virtual_bytes = n(full.source.len());
    exact.max_virtual_lines = n(full.total_lines);
    exact.max_output_bytes = n(output);
    exact.max_matches = n(selected.matches.len());
    assert_eq!(
        render_document_source(&original, &d, &request, exact)
            .unwrap()
            .source,
        selected.source
    );
    for (label, cap) in [
        ("input bytes", input),
        ("targets", d.resolved_wiki_targets.len()),
        ("virtual bytes", full.source.len()),
        ("virtual lines", full.total_lines),
        ("output bytes", output),
        ("matches", selected.matches.len()),
    ] {
        let mut below = exact;
        match label {
            "input bytes" => below.max_input_bytes = n(cap - 1),
            "targets" => below.max_targets = n(cap - 1),
            "virtual bytes" => below.max_virtual_bytes = n(cap - 1),
            "virtual lines" => below.max_virtual_lines = n(cap - 1),
            "output bytes" => below.max_output_bytes = n(cap - 1),
            "matches" => below.max_matches = n(cap - 1),
            _ => unreachable!(),
        }
        assert_limit(
            render_document_source(&original, &d, &request, below).unwrap_err(),
            label,
        );
    }
    let mut tiny = limits();
    tiny.max_virtual_bytes = n(1);
    assert_limit(
        render_document_source(&original, &d, &Request::default(), tiny).unwrap_err(),
        "virtual bytes",
    );
    tiny.max_virtual_bytes = n(5); // Delimiter fits; the real YAML writer fails.
    assert_limit(
        render_document_source(&original, &d, &Request::default(), tiny).unwrap_err(),
        "virtual bytes",
    );
    assert_eq!(d.body, "zero\nHIT alpha\nhit beta\nlast\n");
}

#[test]
fn original_cancel_and_deadline_precede_invalid_selection() {
    let d = document();
    let invalid = Request {
        head: -1,
        ..Default::default()
    };
    let cancelled = Cancellation::default();
    cancelled.cancel();
    for (original, kind) in [
        (
            SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancelled),
            io::ErrorKind::Interrupted,
        ),
        (
            SnapshotContext::new(Instant::now(), Cancellation::default()),
            io::ErrorKind::TimedOut,
        ),
    ] {
        let error = render_document_source(&original, &d, &invalid, limits()).unwrap_err();
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            kind
        );
        assert!(matches!(error, DocumentSourceError::Request(_)));
    }
}

#[test]
fn unrepresentable_metadata_has_no_timestamp_or_interval_default() {
    let mut d = document();
    d.updated_at = Some(UNIX_EPOCH + Duration::from_secs(10_000_000_000));
    assert!(matches!(
        render_document_source(&context(), &d, &Request::default(), limits()),
        Err(DocumentSourceError::OutOfRange("updated_at"))
    ));
    d.updated_at = None;
    d.stale_after = Some(Duration::from_secs(u64::MAX));
    assert!(matches!(
        render_document_source(&context(), &d, &Request::default(), limits()),
        Err(DocumentSourceError::OutOfRange("stale_after_ns"))
    ));
}
