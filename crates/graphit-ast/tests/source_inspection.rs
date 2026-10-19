use graphit_ast::source_inspection::{
    IndexedSource, PhysicalSource, SourceInspection, SourceInspectionError, SourceOrigin,
    inspect_source,
};
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy,
        RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy, SourceReadLimits,
    },
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    project_paths::{InstallationPaths, ProjectPaths},
    text_slice::{Match, Request},
};
use graphit_fs::NodeKind;
use std::{
    error::Error,
    fs, io,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

fn request() -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ))
}

fn policy() -> SnapshotPolicy {
    let retention = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
    SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(1024 * 1024, 128, 65536, 8).unwrap(),
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap()
}

struct Fixture {
    _temporary: tempfile::TempDir,
    project: ProjectPaths,
    original: Arc<SnapshotContext>,
    scope: CapturedPublicationScope,
}

impl Fixture {
    fn new(files: &[(&str, &[u8])]) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("project");
        fs::create_dir_all(root.join(".graphit")).unwrap();
        fs::create_dir(root.join("corpus")).unwrap();
        for (name, bytes) in files {
            fs::write(root.join("corpus").join(name), bytes).unwrap();
        }
        let installation = InstallationPaths::resolve(
            Brand::default(),
            temporary.path(),
            None,
            Some(&temporary.path().join("installation")),
        )
        .unwrap();
        let project = ProjectPaths::at(root, &installation).unwrap();
        let original = request();
        let scope = CapturedPublicationScope::project(
            &project,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            ScopeLimits::new(4, 4096, 256).unwrap(),
            &original,
        )
        .unwrap();
        Self {
            _temporary: temporary,
            project,
            original,
            scope,
        }
    }

    fn capture(&self, name: &str) -> CapturedSourceReader {
        self.capture_with(name, NodeKind::File, &self.original, 65536)
            .unwrap()
    }

    fn capture_with(
        &self,
        name: &str,
        kind: NodeKind,
        original: &Arc<SnapshotContext>,
        range_bytes: usize,
    ) -> io::Result<CapturedSourceReader> {
        CapturedSourceReader::capture_project(
            &self.scope,
            &Path::new("corpus").join(name),
            kind,
            &policy(),
            SourceReadLimits::new(range_bytes, 4096, 1024 * 1024).unwrap(),
            original,
        )
    }
}

fn physical(reader: &CapturedSourceReader) -> PhysicalSource<'_> {
    PhysicalSource {
        reader,
        relative: Path::new(""),
    }
}

fn from_disk(fixture: &Fixture, reader: &CapturedSourceReader) -> SourceInspection {
    inspect_source(
        IndexedSource::Missing,
        Some(physical(reader)),
        &Request::default(),
        &fixture.original,
    )
    .unwrap()
}

fn physical_error_kind(error: SourceInspectionError) -> io::ErrorKind {
    assert!(error.source().is_some());
    match error {
        SourceInspectionError::PhysicalRead(error) => error.kind(),
        other => panic!("expected physical read error, got {other}"),
    }
}

#[test]
fn indexed_empty_and_original_text_take_priority_over_stale_or_wrong_request_capture() {
    let fixture = Fixture::new(&[("disk", b"physical differs")]);
    let reader = fixture.capture("disk");
    fs::remove_file(fixture.project.root().join("corpus/disk")).unwrap();
    let other_request = request();
    for text in ["", "\u{feff}á\r\nnext\n"] {
        for supplied in [None, Some(physical(&reader))] {
            let result = inspect_source(
                IndexedSource::Available(text),
                supplied,
                &Request::default(),
                &other_request,
            )
            .unwrap();
            assert_eq!(result.origin, SourceOrigin::Indexed);
            assert_eq!(result.selection.source, text);
        }
    }
    assert!(
        inspect_source(
            IndexedSource::Missing,
            Some(physical(&reader)),
            &Request::default(),
            &fixture.original,
        )
        .is_err()
    );
}

#[test]
fn any_extension_no_extension_and_empty_captured_file_are_valid_sources() {
    let fixture = Fixture::new(&[
        ("text.arbitrary", "\u{feff}á😀\r\nNUL:\0\n".as_bytes()),
        ("noextension", b"no grammar needed"),
        ("empty", b""),
    ]);
    for (name, expected) in [
        ("text.arbitrary", "\u{feff}á😀\r\nNUL:\0\n"),
        ("noextension", "no grammar needed"),
        ("empty", ""),
    ] {
        let reader = fixture.capture(name);
        assert_eq!(reader.nodes().next().unwrap().relative(), Path::new(""));
        let result = from_disk(&fixture, &reader);
        assert_eq!(result.origin, SourceOrigin::Physical);
        assert_eq!(result.selection.source, expected);
    }
}

#[test]
fn missing_without_capture_and_invalid_utf8_preserve_explicit_failure() {
    let original = request();
    let unavailable =
        inspect_source(IndexedSource::Missing, None, &Request::default(), &original).unwrap_err();
    assert!(matches!(
        unavailable,
        SourceInspectionError::PhysicalUnavailable
    ));

    let fixture = Fixture::new(&[("invalid", &[b'a', 0xff, b'b'])]);
    let reader = fixture.capture("invalid");
    let invalid = inspect_source(
        IndexedSource::Missing,
        Some(physical(&reader)),
        &Request::default(),
        &fixture.original,
    )
    .unwrap_err();
    assert!(invalid.source().is_some());
    match invalid {
        SourceInspectionError::InvalidUtf8(error) => {
            assert_eq!(error.valid_up_to(), 1);
            assert_eq!(error.error_len(), Some(1));
        }
        other => panic!("expected strict UTF8 failure, got {other}"),
    }
}

#[test]
fn identical_parameters_do_not_restore_original_request_identity() {
    let fixture = Fixture::new(&[("file", b"source")]);
    let reader = fixture.capture("file");
    let other = SnapshotContext::new(
        fixture
            .original
            .required_deadline()
            .expect("finite fixture deadline"),
        fixture.original.cancellation().clone(),
    );
    let error = inspect_source(
        IndexedSource::Missing,
        Some(physical(&reader)),
        &Request::default(),
        &other,
    )
    .unwrap_err();
    assert_eq!(physical_error_kind(error), io::ErrorKind::InvalidInput);
}

#[test]
fn cancellation_after_capture_and_expired_original_deadline_stop_delivery() {
    let fixture = Fixture::new(&[("file", b"source")]);
    let reader = fixture.capture("file");
    fixture.original.cancellation().cancel();
    for indexed in [IndexedSource::Available("indexed"), IndexedSource::Missing] {
        let error = inspect_source(
            indexed,
            Some(physical(&reader)),
            &Request::default(),
            &fixture.original,
        )
        .unwrap_err();
        assert!(error.source().is_some());
        match error {
            SourceInspectionError::Operation(error) => {
                assert_eq!(error.kind(), io::ErrorKind::Interrupted);
            }
            other => panic!("expected operation cancellation, got {other}"),
        }
    }
    let expired = SnapshotContext::new(Instant::now(), Cancellation::default());
    let error = inspect_source(
        IndexedSource::Available("indexed"),
        None,
        &Request::default(),
        &expired,
    )
    .unwrap_err();
    match error {
        SourceInspectionError::Operation(error) => {
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        }
        other => panic!("expected operation deadline, got {other}"),
    }
}

#[test]
fn changed_content_is_rejected_even_when_file_size_and_mtime_are_restored() {
    let fixture = Fixture::new(&[("file", b"first")]);
    let reader = fixture.capture("file");
    let path = fixture.project.root().join("corpus/file");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, b"other").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let error = inspect_source(
        IndexedSource::Missing,
        Some(physical(&reader)),
        &Request::default(),
        &fixture.original,
    )
    .unwrap_err();
    assert_eq!(physical_error_kind(error), io::ErrorKind::InvalidData);
}

#[test]
fn file_binding_replacement_and_reader_limit_do_not_return_partial_text() {
    let fixture = Fixture::new(&[("file", b"source")]);
    let reader = fixture.capture("file");
    let path = fixture.project.root().join("corpus/file");
    fs::rename(&path, fixture.project.root().join("corpus/saved")).unwrap();
    fs::write(&path, b"source").unwrap();
    let error = inspect_source(
        IndexedSource::Missing,
        Some(physical(&reader)),
        &Request::default(),
        &fixture.original,
    )
    .unwrap_err();
    assert!(matches!(error, SourceInspectionError::PhysicalRead(_)));

    let current = fixture
        .capture_with("file", NodeKind::File, &fixture.original, 3)
        .unwrap();
    let error = inspect_source(
        IndexedSource::Missing,
        Some(physical(&current)),
        &Request::default(),
        &fixture.original,
    )
    .unwrap_err();
    assert_eq!(physical_error_kind(error), io::ErrorKind::FileTooLarge);
}

#[test]
fn missing_directory_and_escape_paths_fail_in_real_capture_or_reader() {
    let fixture = Fixture::new(&[("file", b"source")]);
    let reader = fixture.capture("file");
    for (relative, expected) in [
        (Path::new("missing"), io::ErrorKind::NotFound),
        (Path::new("../file"), io::ErrorKind::InvalidInput),
        (fixture.project.root(), io::ErrorKind::InvalidInput),
    ] {
        let error = inspect_source(
            IndexedSource::Missing,
            Some(PhysicalSource {
                reader: &reader,
                relative,
            }),
            &Request::default(),
            &fixture.original,
        )
        .unwrap_err();
        assert_eq!(physical_error_kind(error), expected);
    }
    let missing = fixture.capture_with("absent", NodeKind::File, &fixture.original, 65536);
    assert_eq!(missing.err().unwrap().kind(), io::ErrorKind::NotFound);
    let directory = CapturedSourceReader::capture_project(
        &fixture.scope,
        Path::new("corpus"),
        NodeKind::Directory,
        &policy(),
        SourceReadLimits::new(65536, 4096, 1024 * 1024).unwrap(),
        &fixture.original,
    )
    .unwrap();
    let error = inspect_source(
        IndexedSource::Missing,
        Some(physical(&directory)),
        &Request::default(),
        &fixture.original,
    )
    .unwrap_err();
    assert_eq!(physical_error_kind(error), io::ErrorKind::InvalidInput);
}

#[cfg(unix)]
#[test]
fn linked_physical_selection_is_rejected_by_project_capture() {
    let fixture = Fixture::new(&[("file", b"source")]);
    std::os::unix::fs::symlink("file", fixture.project.root().join("corpus/link")).unwrap();
    let linked = fixture.capture_with("link", NodeKind::File, &fixture.original, 65536);
    assert!(linked.is_err());
}

#[test]
fn selectors_compose_range_head_tail_pattern_with_absolute_line_numbers() {
    let original = request();
    let result = inspect_source(
        IndexedSource::Available("zero\none\ntwo\nÁ match\nfour\nfive"),
        None,
        &Request {
            start_line: 2,
            end_line: 6,
            head: 4,
            tail: 2,
            pattern: "á".into(),
            after: 1,
            ..Request::default()
        },
        &original,
    )
    .unwrap();
    assert_eq!(result.selection.total_lines, 6);
    assert_eq!(result.selection.start_line, Some(4));
    assert_eq!(result.selection.end_line, Some(5));
    assert_eq!(result.selection.source, ">    4: Á match\n     5: four");
    assert_eq!(
        result.selection.matches,
        [
            Match {
                line_number: 4,
                line: "Á match".into(),
                is_match: true
            },
            Match {
                line_number: 5,
                line: "four".into(),
                is_match: false
            },
        ]
    );
}

#[test]
fn selector_errors_and_explicit_positive_clamp_preserve_core_semantics() {
    let original = request();
    for selection in [
        Request {
            head: -1,
            ..Request::default()
        },
        Request {
            start_line: 3,
            end_line: 2,
            ..Request::default()
        },
        Request {
            pattern: "[".into(),
            is_regex: true,
            ..Request::default()
        },
    ] {
        let error = inspect_source(
            IndexedSource::Available("one\ntwo"),
            None,
            &selection,
            &original,
        )
        .unwrap_err();
        let expected = graphit_core::text_slice::apply("one\ntwo", &selection).unwrap_err();
        match error {
            SourceInspectionError::Selection(error) => assert_eq!(error, expected),
            other => panic!("expected original selector error, got {other}"),
        }
    }
    let clamped = inspect_source(
        IndexedSource::Available("one\ntwo"),
        None,
        &Request {
            start_line: 99,
            end_line: 101,
            line_numbers: true,
            ..Request::default()
        },
        &original,
    )
    .unwrap();
    assert_eq!(clamped.selection.source, "   2: two");
    assert_eq!(clamped.selection.start_line, Some(2));
    assert_eq!(clamped.selection.end_line, Some(2));
}

#[test]
fn match_context_merges_and_separates_groups_and_no_matches_is_empty_success() {
    let fixture = Fixture::new(&[("text", b"hit\nnear\nhit\ngap\ngap\ngap\nhit\n")]);
    let reader = fixture.capture("text");
    let result = inspect_source(
        IndexedSource::Missing,
        Some(physical(&reader)),
        &Request {
            pattern: "hit".into(),
            before: 1,
            after: 1,
            ..Request::default()
        },
        &fixture.original,
    )
    .unwrap();
    assert_eq!(result.selection.total_lines, 8);
    assert_eq!(
        result.selection.source,
        ">    1: hit\n     2: near\n>    3: hit\n     4: gap\n---\n     6: gap\n>    7: hit\n     8: "
    );
    assert_eq!(result.selection.matches.len(), 7);
    let empty = inspect_source(
        IndexedSource::Missing,
        Some(physical(&reader)),
        &Request {
            pattern: "HIT".into(),
            is_regex: true,
            ..Request::default()
        },
        &fixture.original,
    )
    .unwrap();
    assert_eq!(empty.selection.source, "");
    assert!(empty.selection.matches.is_empty());
    assert_eq!(empty.selection.total_lines, 8);
}
