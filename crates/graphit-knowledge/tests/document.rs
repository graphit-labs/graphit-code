use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_knowledge::document::{
    DocumentParseError, DocumentParseLimits, MetadataKind, MetadataNode, ParseStats,
    ParsedDocument, parse_document,
};
use pulldown_cmark::{Event, Tag};
use serde_saphyr::granit_parser::{ErrorKind as YamlErrorKind, ScalarStyle};
use std::{
    error::Error,
    io,
    num::NonZeroUsize,
    time::{Duration, Instant},
};

fn cap(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn limits() -> DocumentParseLimits {
    DocumentParseLimits {
        max_document_bytes: cap(10_000),
        max_input_bytes: cap(20_000),
        max_frontmatter_bytes: cap(10_000),
        max_yaml_events: cap(1_000),
        max_markdown_events: cap(1_000),
        max_yaml_depth: cap(100),
        max_scalar_bytes: cap(10_000),
        max_links: cap(100),
        max_projection_bytes: cap(100_000),
        max_summary_bytes: cap(10_000),
    }
}

fn context() -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    )
}

fn field<'document, 'input>(
    document: &'document ParsedDocument<'input>,
    name: &str,
) -> &'document MetadataNode<'input> {
    let metadata = &document.metadata;
    let root = metadata.node(metadata.root.unwrap()).unwrap();
    let MetadataKind::Mapping { entries } = &root.kind else {
        panic!("expected mapping root")
    };
    let (_, value) = entries
        .iter()
        .find(|(key, _)| {
            matches!(
                &metadata.node(*key).unwrap().kind,
                MetadataKind::Scalar { value, .. } if value == name
            )
        })
        .unwrap();
    metadata.node(*value).unwrap()
}

#[test]
fn public_projection_borrows_original_bytes_and_outlives_context_and_fallback() {
    let input = include!("fixtures/document/unicode-crlf.rs")
        .as_bytes()
        .to_vec();
    let parsed = {
        let context = context();
        let fallback = String::from("owned fallback");
        parse_document(&context, &input, &fallback, limits()).unwrap()
    };
    assert_eq!(parsed.source.as_ptr(), input.as_ptr());
    assert_eq!(parsed.source.as_bytes(), input);
    assert_eq!(parsed.envelope.bom, Some(0..3));
    assert_eq!(
        &parsed.source[parsed.envelope.yaml.clone().unwrap()],
        "title: Café\r\n"
    );
    assert_eq!(&parsed.source[parsed.envelope.body.clone()], "# Olá\r\n");
    assert_eq!(parsed.title, "Café");
    assert!(matches!(
        &field(&parsed, "title").kind,
        MetadataKind::Scalar { value, .. } if value == "Café"
    ));
    let text = parsed
        .markdown
        .iter()
        .find(|event| matches!(&event.event, Event::Text(text) if text.as_ref() == "Olá"))
        .unwrap();
    assert_eq!(text.span, 29..33);
    assert_eq!(&parsed.source[text.span.clone()], "Olá");
    let heading = parsed
        .markdown
        .iter()
        .find(|event| matches!(&event.event, Event::Start(Tag::Heading { .. })))
        .unwrap();
    assert_eq!(heading.span, 27..35);
    for event in &parsed.markdown {
        assert!(parsed.source.get(event.span.clone()).is_some());
    }

    let metadata_input = include_bytes!("fixtures/document/metadata.md").to_vec();
    let metadata = {
        let context = context();
        let fallback = String::from("fallback");
        parse_document(&context, &metadata_input, &fallback, limits()).unwrap()
    };
    assert_eq!(metadata.source.as_ptr(), metadata_input.as_ptr());
    assert_eq!(metadata.title, "Café: graphit");
    assert_eq!(metadata.summary, "first line second line");
    assert!(matches!(
        &field(&metadata, "numeric").kind,
        MetadataKind::Scalar { value, style: ScalarStyle::Plain } if value == "0xFf001122"
    ));
    assert!(matches!(
        &field(&metadata, "quoted_null").kind,
        MetadataKind::Scalar { value, style: ScalarStyle::SingleQuoted } if value == "null"
    ));
    assert!(matches!(
        &field(&metadata, "references").kind,
        MetadataKind::Sequence { items } if items.is_empty()
    ));
    let root = metadata.metadata.root.unwrap();
    assert!(root.index() < metadata.metadata.nodes().len());
    for node in metadata.metadata.nodes() {
        assert!(metadata.source.get(node.span.clone()).is_some());
    }
}

#[test]
fn public_errors_keep_original_context_and_utf8_origins_without_io() {
    let original = context();
    let parsed = parse_document(&original, b"body\n", "f", limits()).unwrap();
    assert_eq!(parsed.summary, "body");

    let invalid_utf8 = parse_document(&original, b"a\xff", "f", limits()).unwrap_err();
    let DocumentParseError::InvalidUtf8(source) = &invalid_utf8 else {
        panic!("expected UTF-8 error: {invalid_utf8}")
    };
    assert_eq!(source.valid_up_to(), 1);
    assert!(invalid_utf8.source().is_some());

    original.cancellation().cancel();
    let cancelled = parse_document(&original, b"\xff", "f", limits()).unwrap_err();
    let DocumentParseError::Request(source) = &cancelled else {
        panic!("expected original cancellation before UTF-8 validation: {cancelled}")
    };
    assert_eq!(source.kind(), io::ErrorKind::Interrupted);
    assert!(cancelled.source().is_some());

    let expired = SnapshotContext::new(
        Instant::now() - Duration::from_millis(1),
        Cancellation::default(),
    );
    let deadline = parse_document(&expired, b"\xff", "f", limits()).unwrap_err();
    let DocumentParseError::Request(source) = &deadline else {
        panic!("expected deadline error before UTF-8 validation: {deadline}")
    };
    assert_eq!(source.kind(), io::ErrorKind::TimedOut);
    assert!(deadline.source().is_some());
}

#[test]
fn public_limits_distinguish_logical_budgets_native_depth_and_summary_policy() {
    let context = context();
    let input = "---\na: {b: é}\n---\n[x](u) [x](u)\n";
    let parsed = parse_document(&context, input.as_bytes(), "F", limits()).unwrap();
    assert_eq!(input.len(), 33);
    assert_eq!(parsed.envelope.frontmatter.as_ref().unwrap().len(), 19);
    assert_eq!(
        parsed.stats,
        ParseStats {
            yaml_events: 11,
            markdown_events: 9,
            yaml_depth: 2,
            links: 2,
            projection_bytes: 18,
        }
    );

    type CapCase = (&'static str, usize, fn(&mut DocumentParseLimits, usize));
    let cases: [CapCase; 8] = [
        ("document bytes", 33, |l, n| l.max_document_bytes = cap(n)),
        ("input bytes", 34, |l, n| l.max_input_bytes = cap(n)),
        ("frontmatter bytes", 19, |l, n| {
            l.max_frontmatter_bytes = cap(n)
        }),
        ("yaml events", 11, |l, n| l.max_yaml_events = cap(n)),
        ("markdown events", 9, |l, n| l.max_markdown_events = cap(n)),
        ("scalar bytes", 2, |l, n| l.max_scalar_bytes = cap(n)),
        ("links", 2, |l, n| l.max_links = cap(n)),
        ("projection bytes", 18, |l, n| {
            l.max_projection_bytes = cap(n)
        }),
    ];
    for (name, required, set) in cases {
        let mut bound = limits();
        set(&mut bound, required);
        assert!(
            parse_document(&context, input.as_bytes(), "F", bound).is_ok(),
            "exact {name}"
        );
        set(&mut bound, required - 1);
        let result = parse_document(&context, input.as_bytes(), "F", bound);
        assert!(
            matches!(result, Err(DocumentParseError::LimitExceeded(actual)) if actual == name),
            "one below {name}"
        );
    }

    let mut depth = limits();
    depth.max_yaml_depth = cap(2);
    assert!(parse_document(&context, input.as_bytes(), "F", depth).is_ok());
    assert!(matches!(
        parse_document(&context, b"---\na:\n  b: [x]\n---\n", "F", depth),
        Err(DocumentParseError::LimitExceeded("yaml depth"))
    ));
    depth.max_yaml_depth = cap(1);
    let native = parse_document(&context, b"---\n{a: [x]}\n---\n", "F", depth).unwrap_err();
    let DocumentParseError::Yaml { source, location } = &native else {
        panic!("expected native depth error: {native}")
    };
    assert!(matches!(
        source.kind(),
        YamlErrorKind::RecursionLimitExceeded
    ));
    assert_eq!(location.yaml_line, source.marker().line());
    assert_eq!(location.yaml_column, source.marker().col());
    assert!(native.source().is_some());

    for (maximum, expected, truncated) in [(6, "ééé", false), (3, "é", true)] {
        let mut bound = limits();
        bound.max_summary_bytes = cap(maximum);
        let parsed = parse_document(
            &context,
            "---\ndescription: ééé\n---\nbody\n".as_bytes(),
            "F",
            bound,
        )
        .unwrap();
        assert_eq!(parsed.summary, expected);
        assert_eq!(parsed.summary_truncated, truncated);
    }
}
