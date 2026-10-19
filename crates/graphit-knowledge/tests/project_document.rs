//! Composition of a known PROJECT file capture with the document parser.
//! This does not discover a documentation tree or admit a published corpus.
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy,
        RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy, SourceReadLimits,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_fs::NodeKind;
use graphit_knowledge::{
    document::{DocumentParseError, DocumentParseLimits, parse_document},
    preview::{MarkdownPreviewError, MarkdownPreviewLimits, PreviewSource, preview_markdown},
};
use pulldown_cmark::{Event, Tag};
use std::{
    error::Error,
    fs, io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const DOCUMENT: &[u8] = include_bytes!("fixtures/document/unicode-crlf.md");
const DOCUMENT_BYTES: usize = 35;

struct TemporaryDirectory(PathBuf);
impl TemporaryDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..16 {
            let nonce = NEXT
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .expect("fixture nonce exhausted");
            let path = std::env::temp_dir().join(format!(
                "graphit-knowledge-project-{}-{nonce}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create fixture directory: {error}"),
            }
        }
        panic!("fixture directory collision limit exceeded");
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            if std::thread::panicking() {
                eprintln!("fixture cleanup during unwinding failed: {error}");
            } else {
                panic!("fixture cleanup failed: {error}");
            }
        }
    }
}

fn request() -> Arc<SnapshotContext> {
    let context = SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    );
    Arc::new(context)
}

struct Fixture {
    // Fields drop in declaration order: native owners before directory cleanup.
    scope: CapturedPublicationScope,
    request: Arc<SnapshotContext>,
    _paths: ProjectPaths,
    document_bytes: usize,
    _temporary: TemporaryDirectory,
}
impl Fixture {
    fn new() -> Self {
        Self::with_document(DOCUMENT)
    }

    fn with_document(document: &[u8]) -> Self {
        let temporary = TemporaryDirectory::new();
        let root = temporary.0.join("project");
        fs::create_dir_all(root.join(".graphit")).unwrap();
        fs::write(root.join("document.md"), document).unwrap();
        // A mistaken directory capture exceeds the selected FILE content bounds.
        fs::write(root.join("unselected.bin"), [0; 4096]).unwrap();
        let installation = InstallationPaths::resolve(
            Brand::default(),
            &temporary.0,
            None,
            Some(&temporary.0.join("global")),
        )
        .unwrap();
        let paths = ProjectPaths::at(&root, &installation).unwrap();
        let request = request();
        let scope = CapturedPublicationScope::project(
            &paths,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            ScopeLimits::new(4, 4096, 256).unwrap(),
            &request,
        )
        .unwrap();
        Self {
            scope,
            request,
            _paths: paths,
            document_bytes: document.len(),
            _temporary: temporary,
        }
    }
}

fn capture(
    fixture: &Fixture,
    request: &Arc<SnapshotContext>,
    retention_bytes: usize,
) -> io::Result<CapturedSourceReader> {
    let retention = RetainedBytesLimits::new(retention_bytes, 1, 1)?;
    let length = u64::try_from(fixture.document_bytes).unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(length, 1, 1, 1)?,
        RetainedFileSet::new([PathBuf::new()], retention)?,
        retention,
    )?;
    CapturedSourceReader::capture_project(
        &fixture.scope,
        Path::new("document.md"),
        NodeKind::File,
        &policy,
        SourceReadLimits::new(fixture.document_bytes, 4096, length)?,
        request,
    )
}

fn parse_limits(document_bytes: usize) -> DocumentParseLimits {
    let cap = |n| NonZeroUsize::new(n).unwrap();
    DocumentParseLimits {
        max_document_bytes: cap(document_bytes),
        max_input_bytes: cap(1024),
        max_frontmatter_bytes: cap(1024),
        max_yaml_events: cap(100),
        max_markdown_events: cap(100),
        max_yaml_depth: cap(10),
        max_scalar_bytes: cap(1024),
        max_links: cap(10),
        max_projection_bytes: cap(10_000),
        max_summary_bytes: cap(1024),
    }
}

#[test]
fn known_project_file_projects_borrowed_unicode_ranges_under_original_request() {
    assert_eq!(DOCUMENT.len(), DOCUMENT_BYTES);
    let fixture = Fixture::new();
    let reader = capture(&fixture, &fixture.request, DOCUMENT_BYTES).unwrap();
    assert_eq!(
        reader
            .nodes()
            .map(|node| node.relative().to_owned())
            .collect::<Vec<_>>(),
        [PathBuf::new()]
    );
    reader.check_request(&fixture.request).unwrap();
    let foreign = request();
    assert_eq!(
        reader.check_request(&foreign).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let bytes = reader.retained(Path::new("")).unwrap();
    assert_eq!(bytes, DOCUMENT);
    let parsed = parse_document(
        &fixture.request,
        bytes,
        "fallback",
        parse_limits(DOCUMENT_BYTES),
    )
    .unwrap();
    assert_eq!(parsed.source.as_ptr(), bytes.as_ptr());
    assert_eq!(parsed.title, "Café");
    assert_eq!(parsed.envelope.bom, Some(0..3));
    assert_eq!(
        &parsed.source[parsed.envelope.yaml.clone().unwrap()],
        "title: Café\r\n"
    );
    assert_eq!(&parsed.source[parsed.envelope.body.clone()], "# Olá\r\n");
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
    assert!(
        parsed
            .markdown
            .iter()
            .all(|event| parsed.source.get(event.span.clone()).is_some())
    );
    assert!(
        parsed
            .metadata
            .nodes()
            .iter()
            .all(|node| parsed.source.get(node.span.clone()).is_some())
    );
}

#[test]
fn capture_and_parser_enforce_content_and_retention_bounds() {
    let fixture = Fixture::new();
    let reader = capture(&fixture, &fixture.request, DOCUMENT_BYTES).unwrap();
    let bytes = reader.retained(Path::new("")).unwrap();
    parse_document(
        &fixture.request,
        bytes,
        "fallback",
        parse_limits(DOCUMENT_BYTES),
    )
    .unwrap();
    let error = parse_document(
        &fixture.request,
        bytes,
        "fallback",
        parse_limits(DOCUMENT_BYTES - 1),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        DocumentParseError::LimitExceeded("document bytes")
    ));

    let retention = Fixture::new();
    assert_eq!(
        capture(&retention, &retention.request, DOCUMENT_BYTES - 1)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::FileTooLarge
    );
}

#[test]
fn cancellation_before_and_after_capture_and_expiry_deny_real_operations() {
    let before = Fixture::new();
    before.request.cancellation().cancel();
    assert_eq!(
        capture(&before, &before.request, DOCUMENT_BYTES)
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Interrupted
    );

    let after = Fixture::new();
    let reader = capture(&after, &after.request, DOCUMENT_BYTES).unwrap();
    let bytes = reader.retained(Path::new("")).unwrap();
    after.request.cancellation().cancel();
    assert_eq!(
        reader.check_request(&after.request).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    let error = parse_document(
        &after.request,
        bytes,
        "fallback",
        parse_limits(DOCUMENT_BYTES),
    )
    .unwrap_err();
    let DocumentParseError::Request(source) = error else {
        panic!("expected original request error");
    };
    assert_eq!(source.kind(), io::ErrorKind::Interrupted);

    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    let error =
        parse_document(&expired, DOCUMENT, "fallback", parse_limits(DOCUMENT_BYTES)).unwrap_err();
    let DocumentParseError::Request(source) = error else {
        panic!("expected original deadline error");
    };
    assert_eq!(source.kind(), io::ErrorKind::TimedOut);
}

#[cfg(unix)]
#[test]
fn replacement_with_same_length_denies_binding_before_new_payload_read() {
    let fixture = Fixture::new();
    let reader = capture(&fixture, &fixture.request, DOCUMENT_BYTES).unwrap();
    fs::rename(
        fixture._paths.root().join("document.md"),
        fixture._paths.root().join("original.md"),
    )
    .unwrap();
    fs::write(
        fixture._paths.root().join("document.md"),
        [b'x'; DOCUMENT_BYTES],
    )
    .unwrap();
    assert_eq!(
        reader.open_file(Path::new("")).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(reader.retained(Path::new("")).unwrap(), DOCUMENT);
}

#[test]
fn captured_project_summary_preview_borrows_parser_output_with_exact_limits() {
    const PROSE: &[u8] = "# H\r\n\r\n**ação** 🍀\r\n".as_bytes();
    assert_eq!(PROSE.len(), 24);
    let fixture = Fixture::with_document(PROSE);
    let reader = capture(&fixture, &fixture.request, PROSE.len()).unwrap();
    reader.check_request(&fixture.request).unwrap();
    let bytes = reader.retained(Path::new("")).unwrap();
    assert_eq!(bytes, PROSE);
    let parsed = parse_document(
        &fixture.request,
        bytes,
        "fallback",
        parse_limits(PROSE.len()),
    )
    .unwrap();
    assert_eq!(parsed.source.as_ptr(), bytes.as_ptr());
    assert_eq!(parsed.source.as_bytes(), PROSE);
    assert_eq!(parsed.envelope.body, 0..24);
    assert_eq!(parsed.title, "H");
    assert_eq!(parsed.summary, "ação 🍀");
    let body = &parsed.source[parsed.envelope.body.clone()];
    let limits = |input, output| MarkdownPreviewLimits {
        max_input_bytes: NonZeroUsize::new(input).unwrap(),
        max_output_bytes: NonZeroUsize::new(output).unwrap(),
        max_body_scalars: NonZeroUsize::new(1).unwrap(),
    };
    let preview =
        preview_markdown(&fixture.request, body, &parsed.summary, limits(35, 11)).unwrap();
    assert_eq!(preview.text(), "ação 🍀");
    assert_eq!(preview.text().as_ptr(), parsed.summary.as_ptr());
    assert_eq!(preview.source(), PreviewSource::Summary);
    assert_eq!(preview.source_range(), 0..11);
    assert!(!preview.truncated());
    for (input, output, label) in [(34, 11, "input bytes"), (35, 10, "output bytes")] {
        let error = preview_markdown(
            &fixture.request,
            body,
            &parsed.summary,
            limits(input, output),
        )
        .unwrap_err();
        assert!(matches!(&error, MarkdownPreviewError::LimitExceeded(actual) if *actual == label));
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        assert!(error.source().is_none());
    }
    assert_eq!(reader.retained(Path::new("")).unwrap(), PROSE);
    assert_eq!(parsed.source.as_bytes(), PROSE);
    assert_eq!(parsed.title, "H");
    assert_eq!(parsed.summary, "ação 🍀");
}

#[test]
fn captured_project_body_preview_owns_utf8_prefix_and_accounts_marker_bytes() {
    let fixture = Fixture::new();
    let reader = capture(&fixture, &fixture.request, DOCUMENT_BYTES).unwrap();
    reader.check_request(&fixture.request).unwrap();
    let bytes = reader.retained(Path::new("")).unwrap();
    let parsed = parse_document(
        &fixture.request,
        bytes,
        "fallback",
        parse_limits(DOCUMENT_BYTES),
    )
    .unwrap();
    assert_eq!(parsed.source.as_ptr(), bytes.as_ptr());
    assert_eq!(parsed.source.as_bytes(), DOCUMENT);
    assert_eq!(parsed.envelope.body, 27..35);
    assert_eq!(parsed.title, "Café");
    assert!(parsed.summary.is_empty());
    let body = &parsed.source[parsed.envelope.body.clone()];
    assert_eq!(body, "# Olá\r\n");
    let limits = |input, output| MarkdownPreviewLimits {
        max_input_bytes: NonZeroUsize::new(input).unwrap(),
        max_output_bytes: NonZeroUsize::new(output).unwrap(),
        max_body_scalars: NonZeroUsize::new(5).unwrap(),
    };
    let preview = preview_markdown(&fixture.request, body, &parsed.summary, limits(8, 9)).unwrap();
    assert_eq!(preview.text(), "# Olá…");
    assert_eq!(preview.text().len(), 9);
    assert_ne!(preview.text().as_ptr(), body.as_ptr());
    assert_eq!(preview.source(), PreviewSource::Body);
    assert_eq!(preview.source_range(), 0..6);
    assert_eq!(&body[preview.source_range()], "# Olá");
    assert_eq!(&parsed.source[27..33], &body[preview.source_range()]);
    assert_eq!(preview.text().as_bytes()[6..], [0xe2, 0x80, 0xa6]);
    assert!(preview.truncated());
    for (input, output, label) in [(7, 9, "input bytes"), (8, 8, "output bytes")] {
        let error = preview_markdown(
            &fixture.request,
            body,
            &parsed.summary,
            limits(input, output),
        )
        .unwrap_err();
        assert!(matches!(&error, MarkdownPreviewError::LimitExceeded(actual) if *actual == label));
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
        assert!(error.source().is_none());
    }
    assert_eq!(reader.retained(Path::new("")).unwrap(), DOCUMENT);
    assert_eq!(parsed.source.as_bytes(), DOCUMENT);
    assert_eq!(parsed.title, "Café");
    assert!(parsed.summary.is_empty());
}

#[test]
fn cancellation_after_real_capture_and_parse_denies_preview_without_extra_io() {
    let fixture = Fixture::new();
    let reader = capture(&fixture, &fixture.request, DOCUMENT_BYTES).unwrap();
    reader.check_request(&fixture.request).unwrap();
    let bytes = reader.retained(Path::new("")).unwrap();
    let parsed = parse_document(
        &fixture.request,
        bytes,
        "fallback",
        parse_limits(DOCUMENT_BYTES),
    )
    .unwrap();
    let body = &parsed.source[parsed.envelope.body.clone()];
    assert_eq!(parsed.source.as_ptr(), bytes.as_ptr());
    fixture.request.cancellation().cancel();
    let error = preview_markdown(
        &fixture.request,
        body,
        &parsed.summary,
        MarkdownPreviewLimits {
            max_input_bytes: NonZeroUsize::new(8).unwrap(),
            max_output_bytes: NonZeroUsize::new(9).unwrap(),
            max_body_scalars: NonZeroUsize::new(5).unwrap(),
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(error.source().is_some());
    let MarkdownPreviewError::Context(original) = &error else {
        panic!("expected original request cancellation");
    };
    assert_eq!(original.kind(), io::ErrorKind::Interrupted);
    assert_eq!(reader.retained(Path::new("")).unwrap(), DOCUMENT);
    assert_eq!(parsed.source.as_bytes(), DOCUMENT);
    assert_eq!(parsed.envelope.body, 27..35);
    assert_eq!(body, "# Olá\r\n");
    assert_eq!(parsed.title, "Café");
    assert!(parsed.summary.is_empty());
}
