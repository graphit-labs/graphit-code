//! Actual retained FILE capture -> admitted editorial metadata.
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
    document::DocumentParseLimits,
    publication_metadata::{
        AdmittedScalarState, CompilationMetadataError, CompiledDocument, FieldProvenance,
        PublicationMetadataLimits, PublicationMetadataPolicy, compile_captured_document,
        compile_captured_file_bytes,
    },
};
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
    fn with_document(document: &[u8]) -> Self {
        Self::with_deadline(document, Instant::now() + Duration::from_secs(30))
    }
    fn with_deadline(document: &[u8], deadline: Instant) -> Self {
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
        let request = Arc::new(SnapshotContext::new(deadline, Cancellation::default()));
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
    let length = u64::try_from(fixture.document_bytes.max(1)).unwrap();
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
        SourceReadLimits::new(fixture.document_bytes.max(1), 4096, length)?,
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

fn limits() -> PublicationMetadataLimits {
    let cap = |n| NonZeroUsize::new(n).unwrap();
    PublicationMetadataLimits {
        max_input_bytes: cap(4096),
        max_title_bytes: cap(1024),
        max_kind_bytes: cap(1024),
        max_description_bytes: cap(1024),
        max_effective_summary_bytes: cap(1024),
        max_effective_metadata_bytes: cap(4096),
        max_owned_metadata_bytes: cap(4096),
        max_root_entries: cap(100),
        max_typed_nodes: cap(100),
        max_typed_depth: cap(10),
        max_typed_scalar_bytes: cap(4096),
        max_typed_events: cap(200),
        max_typed_lookahead_characters: cap(1024),
        max_typed_flow_depth: cap(10),
    }
}
fn policy() -> PublicationMetadataPolicy {
    PublicationMetadataPolicy {
        fallback_title: "F".into(),
        kind_on_absent: "guide".into(),
    }
}
fn compile<'a>(
    fixture: &Fixture,
    reader: &'a CapturedSourceReader,
    caps: PublicationMetadataLimits,
) -> Result<CompiledDocument<'a>, CompilationMetadataError> {
    compile_captured_document(
        reader,
        Path::new(""),
        &fixture.request,
        policy(),
        parse_limits(fixture.document_bytes.max(1)),
        caps,
    )
}
fn expect_limit(error: CompilationMetadataError, expected: &'static str) {
    assert!(matches!(error, CompilationMetadataError::Limit(name) if name == expected));
    assert!(error.source().is_none());
}

#[test]
fn external_file_buffer_matches_retained_editorial_and_keeps_retained_delete_contract() {
    for body in [
        b"".as_slice(),
        b"# Guide\nwhole body\n",
        b"---\ntitle: T\ndescription: ''\ntype: null\n---\n# Whole\n",
    ] {
        let f = Fixture::with_document(body);
        let reader = capture(&f, &f.request, body.len().max(1)).unwrap();
        let bytes = body.to_vec();
        let external = compile_captured_file_bytes(
            &reader,
            Path::new(""),
            &bytes,
            &f.request,
            policy(),
            parse_limits(body.len().max(1)),
            limits(),
        )
        .unwrap();
        let retained = compile(&f, &reader, limits()).unwrap();
        assert_eq!(external.whole_source(), retained.whole_source());
        assert_eq!(external.whole_source().as_ptr(), bytes.as_ptr());
        assert_eq!(external.title(), retained.title());
        assert_eq!(external.kind(), retained.kind());
        assert_eq!(external.description(), retained.description());
        assert_eq!(external.summary(), retained.summary());
        assert_eq!(external.summary_truncated(), retained.summary_truncated());
        assert_eq!(external.summary_limit(), retained.summary_limit());
        assert_eq!(
            external.recipe_policy().fallback_title,
            retained.recipe_policy().fallback_title
        );
        drop(external);
        drop(retained);
        fs::remove_file(f.scope.owner_root().join("document.md")).unwrap();
        assert_eq!(
            compile(&f, &reader, limits())
                .unwrap()
                .whole_source()
                .as_bytes(),
            body
        );
        let error = compile_captured_file_bytes(
            &reader,
            Path::new(""),
            &bytes,
            &f.request,
            policy(),
            parse_limits(body.len().max(1)),
            limits(),
        )
        .err()
        .unwrap();
        assert!(matches!(error, CompilationMetadataError::PhysicalSource(_)));
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .is_some()
        );
    }
}

#[test]
fn external_buffer_rejects_real_length_digest_path_and_foreign_request_errors() {
    let f = Fixture::with_document(b"# Guide\n");
    let reader = capture(&f, &f.request, f.document_bytes).unwrap();
    for bytes in [b"# Guide".as_slice(), b"# Other\n"] {
        let error = compile_captured_file_bytes(
            &reader,
            Path::new(""),
            bytes,
            &f.request,
            policy(),
            parse_limits(f.document_bytes),
            limits(),
        )
        .err()
        .unwrap();
        assert!(
            matches!(&error, CompilationMetadataError::PhysicalSource(e) if e.kind()==io::ErrorKind::InvalidData)
        );
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
    let error = compile_captured_file_bytes(
        &reader,
        Path::new("other.md"),
        b"# Guide\n",
        &f.request,
        policy(),
        parse_limits(f.document_bytes),
        limits(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error,
        CompilationMetadataError::MissingCapturedFile
    ));
    let foreign = request();
    let error = compile_captured_file_bytes(
        &reader,
        Path::new(""),
        b"# Guide\n",
        &foreign,
        policy(),
        parse_limits(f.document_bytes),
        limits(),
    )
    .err()
    .unwrap();
    assert!(
        matches!(&error, CompilationMetadataError::OriginalRequest(e) if e.kind()==io::ErrorKind::InvalidInput)
    );
}

#[test]
fn absence_null_empty_and_comment_only_keep_distinct_values_and_provenance() {
    for (source, description, kind, null_desc, null_kind) in [
        ("# H\r\n\r\nbody\r\n", None, "guide", false, false),
        ("---\n---\n# H\n", None, "guide", false, false),
        (
            "---\n# comment only\n---\n# H\n",
            None,
            "guide",
            false,
            false,
        ),
        (
            "---\ndescription: null\ntype: ~\n---\n# H\n",
            None,
            "guide",
            true,
            true,
        ),
        (
            "---\ndescription: \"\"\ntype: \"\"\n---\n# H\n",
            Some(""),
            "",
            false,
            false,
        ),
        (
            "---\ndescription: \"null\"\ntype: 'domain/a 🍀'\n---\n# H\n",
            Some("null"),
            "domain/a 🍀",
            false,
            false,
        ),
    ] {
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len().max(1)).unwrap();
        let result = compile(&fixture, &reader, limits()).unwrap();
        assert_eq!(
            (result.title(), result.description(), result.kind()),
            ("H", description, kind)
        );
        assert_eq!(result.whole_source().as_bytes(), source.as_bytes());
        assert_eq!(
            result.whole_source().as_ptr(),
            reader.retained(Path::new("")).unwrap().as_ptr()
        );
        assert_eq!(
            result.physical_source_digest().as_bytes(),
            &reader.nodes().next().unwrap().digest().unwrap()
        );
        assert_eq!(result.provenance().title(), &FieldProvenance::Unknown);
        if null_desc {
            assert!(matches!(
                result.provenance().description(),
                FieldProvenance::AuthoredScalar {
                    state: AdmittedScalarState::Null,
                    ..
                }
            ));
        } else if description.is_none() {
            assert_eq!(result.provenance().description(), &FieldProvenance::Absent);
        } else {
            assert!(
                matches!(result.provenance().description(), FieldProvenance::AuthoredScalar { state: AdmittedScalarState::Text, span } if source.get(span.clone()).is_some())
            );
        }
        if kind == "guide" {
            assert!(
                matches!(result.provenance().kind(), FieldProvenance::SuppliedKind { authored_null_span } if authored_null_span.is_some() == null_kind)
            );
        } else {
            assert!(matches!(
                result.provenance().kind(),
                FieldProvenance::AuthoredScalar {
                    state: AdmittedScalarState::Text,
                    ..
                }
            ));
        }
    }
}

#[test]
fn integral_description_survives_summary_prefix_and_yaml_scalar_semantics() {
    for (source, expected) in [
        ("---\ndescription: \"  α\\nβ  \"\n---\n# H\n", "  α\nβ  "),
        ("---\ndescription: '   '\n---\n# H\n", "   "),
        ("---\ndescription: |-\n  α\n  β\n---\n# H\n", "α\nβ"),
        ("---\ndescription: >-\n  α\n  β\n---\n# H\n", "α β"),
    ] {
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
        let mut parser = parse_limits(source.len());
        parser.max_summary_bytes = NonZeroUsize::new(2).unwrap();
        let result = compile_captured_document(
            &reader,
            Path::new(""),
            &fixture.request,
            policy(),
            parser,
            limits(),
        )
        .unwrap();
        assert_eq!(result.description(), Some(expected));
        assert_eq!(result.whole_source(), source);
        assert_eq!(result.summary_limit(), 2);
        if expected.trim().starts_with('α') {
            assert_eq!(result.summary(), "α");
            assert!(result.summary_truncated());
        }
    }
}

#[test]
fn literal_title_precedence_and_strict_known_fields_use_the_real_sdk_boundary() {
    for (source, title) in [
        ("---\ntitle: ' '\n---\n# H\n", "H"),
        ("---\ntitle: ''\n---\nbody\n", "F"),
        ("---\ntitle: 123\n---\n# H\n", "123"),
        (
            "---\ntitle: .nan\nunknown: [.inf, 1e999]\n---\n# H\n",
            ".nan",
        ),
        ("---\ntitle: .inf\n---\n# H\n", ".inf"),
        ("---\ntitle: 1e999\n---\n# H\n", "1e999"),
        (
            "---\n0xB: x\n11: y\ndescription: text\ntype: yes\n---\n# H\n",
            "H",
        ),
    ] {
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
        assert_eq!(compile(&fixture, &reader, limits()).unwrap().title(), title);
    }
    let source = b"body\n";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    let empty = PublicationMetadataPolicy {
        fallback_title: "  ".into(),
        kind_on_absent: "".into(),
    };
    let result = compile_captured_document(
        &reader,
        Path::new(""),
        &fixture.request,
        empty,
        parse_limits(source.len()),
        limits(),
    )
    .unwrap();
    assert_eq!((result.title(), result.kind()), ("", ""));
    for field in ["description", "type"] {
        for token in ["123", "true", "TRUE", ".nan", ".inf", "[x]", "{x: y}"] {
            let source = format!("---\n{field}: {token}\n---\n# H\n");
            let fixture = Fixture::with_document(source.as_bytes());
            let reader = capture(&fixture, &fixture.request, source.len()).unwrap();

            let error = compile(&fixture, &reader, limits()).err().unwrap();
            assert!(
                matches!(
                    error,
                    CompilationMetadataError::TypedMetadata(_)
                        | CompilationMetadataError::Parser(_)
                ),
                "{field}/{token}: {error}"
            );
            assert!(error.source().is_some());
        }
        for text in ["123", "true", ".nan", ".inf", "1e999", "yes", "on"] {
            // SDK treats unquoted 1e999/yes/on as text. Other values need quoting.
            let value = if matches!(text, "1e999" | "yes" | "on") {
                text.to_string()
            } else {
                format!("'{text}'")
            };
            let source = format!("---\n{field}: {value}\n---\n# H\n");
            let fixture = Fixture::with_document(source.as_bytes());
            let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
            let result = compile(&fixture, &reader, limits()).unwrap();
            match field {
                "description" => assert_eq!(result.description(), Some(text)),
                _ => assert_eq!(result.kind(), text),
            }
        }
    }
}

#[test]
fn original_binding_and_retained_bom_crlf_survive_removed_path_and_lookup_denials() {
    let source = b"\xef\xbb\xbf---\r\ntitle: X\r\ntype: custom\r\n---\r\n# H\r\n\r\nbody\n";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    // The factory uses retained source even when the physical path is gone.
    #[cfg(unix)]
    fs::remove_file(fixture._paths.root().join("document.md")).unwrap();
    let result = compile(&fixture, &reader, limits()).unwrap();
    assert_eq!(result.whole_source().as_bytes(), source);
    assert_eq!(
        result.whole_source().as_ptr(),
        reader.retained(Path::new("")).unwrap().as_ptr()
    );
    let foreign = request();
    let error = compile_captured_document(
        &reader,
        Path::new(""),
        &foreign,
        policy(),
        parse_limits(source.len()),
        limits(),
    )
    .err()
    .unwrap();
    assert!(
        matches!(&error, CompilationMetadataError::OriginalRequest(e) if e.kind() == io::ErrorKind::InvalidInput)
    );
    assert!(error.source().is_some());
    let error = compile_captured_document(
        &reader,
        Path::new("unselected.bin"),
        &fixture.request,
        policy(),
        parse_limits(source.len()),
        limits(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error,
        CompilationMetadataError::MissingCapturedFile
    ));
}

#[test]
fn unretained_file_is_a_real_capture_state() {
    let source = b"# H\n";
    let fixture = Fixture::with_document(source);
    let retention = RetainedBytesLimits::new(source.len(), 2, 4096).unwrap();
    let no_retention = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new(source.len() as u64, 1, 1, 1).unwrap(),
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap();
    let reader = CapturedSourceReader::capture_project(
        &fixture.scope,
        Path::new("document.md"),
        NodeKind::File,
        &no_retention,
        SourceReadLimits::new(source.len(), 4096, source.len() as u64).unwrap(),
        &fixture.request,
    )
    .unwrap();
    assert!(reader.retained(Path::new("")).is_none());
    assert!(reader.nodes().next().unwrap().digest().is_some());
    assert!(matches!(
        compile(&fixture, &reader, limits()).err().unwrap(),
        CompilationMetadataError::UnretainedFile
    ));
}

#[test]
fn each_text_capacity_has_exact_and_one_under_admission_without_partial_output() {
    let source = "---\ntitle: TT\ndescription: α\ntype: abc\n---\n# H\n";
    let fixture = Fixture::with_document(source.as_bytes());
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    // title2 + description2 + kind3 + summary2 =9 effective;
    // policy6 + title2 + description2 + authoredkind3 + summary2 =15 owned.
    for (field, exact, label) in [
        (0, source.len() + 6, "input bytes"),
        (1, 2, "title bytes"),
        (2, 3, "kind bytes"),
        (3, 2, "description bytes"),
        (4, 2, "effective summary bytes"),
        (5, 9, "effective metadata bytes"),
        (6, 15, "owned metadata bytes"),
    ] {
        let mut cap = limits();
        let slot = match field {
            0 => &mut cap.max_input_bytes,
            1 => &mut cap.max_title_bytes,
            2 => &mut cap.max_kind_bytes,
            3 => &mut cap.max_description_bytes,
            4 => &mut cap.max_effective_summary_bytes,
            5 => &mut cap.max_effective_metadata_bytes,
            _ => &mut cap.max_owned_metadata_bytes,
        };
        *slot = NonZeroUsize::new(exact).unwrap();
        let result = compile(&fixture, &reader, cap).unwrap();
        assert_eq!(
            (
                result.title(),
                result.description(),
                result.kind(),
                result.summary()
            ),
            ("TT", Some("α"), "abc", "α")
        );
        drop(result);
        let slot = match field {
            0 => &mut cap.max_input_bytes,
            1 => &mut cap.max_title_bytes,
            2 => &mut cap.max_kind_bytes,
            3 => &mut cap.max_description_bytes,
            4 => &mut cap.max_effective_summary_bytes,
            5 => &mut cap.max_effective_metadata_bytes,
            _ => &mut cap.max_owned_metadata_bytes,
        };
        *slot = NonZeroUsize::new(exact - 1).unwrap();

        expect_limit(compile(&fixture, &reader, cap).err().unwrap(), label);
    }
}

#[test]
fn real_sdk_budgets_constrain_entries_nodes_events_scalar_bytes_and_nested_containers() {
    let source = "---\ndescription: α\ntype: abc\n---\n# H\n";
    let fixture = Fixture::with_document(source.as_bytes());
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    for (field, exact) in [(0, 2), (1, 5), (2, 10), (3, 20)] {
        let mut caps = limits();
        let slot = match field {
            0 => &mut caps.max_root_entries,
            1 => &mut caps.max_typed_nodes,
            2 => &mut caps.max_typed_events,
            _ => &mut caps.max_typed_scalar_bytes,
        };
        *slot = NonZeroUsize::new(exact).unwrap();
        drop(compile(&fixture, &reader, caps).unwrap());
        let slot = match field {
            0 => &mut caps.max_root_entries,
            1 => &mut caps.max_typed_nodes,
            2 => &mut caps.max_typed_events,
            _ => &mut caps.max_typed_scalar_bytes,
        };
        *slot = NonZeroUsize::new(exact - 1).unwrap();
        let error = compile(&fixture, &reader, caps).err().unwrap();
        if field == 0 {
            expect_limit(error, "root entries");
        } else {
            assert!(matches!(error, CompilationMetadataError::TypedMetadata(_)));
            assert!(error.source().is_some());
        }
    }
    let source = "---\nunknown: [[x]]\n---\n# H\n";
    let fixture = Fixture::with_document(source.as_bytes());
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    for (depth, flow) in [(3, 2), (10, 2)] {
        let mut caps = limits();
        caps.max_typed_depth = NonZeroUsize::new(depth).unwrap();
        caps.max_typed_flow_depth = NonZeroUsize::new(flow).unwrap();
        drop(compile(&fixture, &reader, caps).unwrap());
        if depth == 3 {
            caps.max_typed_depth = NonZeroUsize::new(2).unwrap();
        } else {
            caps.max_typed_flow_depth = NonZeroUsize::new(1).unwrap();
        }
        assert!(matches!(
            compile(&fixture, &reader, caps).err().unwrap(),
            CompilationMetadataError::TypedMetadata(_)
        ));
    }
}

#[test]
fn sdk_simple_key_lookahead_counts_characters_independently_of_utf8_bytes() {
    for key in ["k".repeat(32), "α".repeat(32)] {
        assert_eq!(key.chars().count(), 32);
        let source = format!("---\n{key}: x\n---\n# H\n");
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
        let mut caps = limits();
        caps.max_typed_lookahead_characters = NonZeroUsize::new(32).unwrap();
        drop(compile(&fixture, &reader, caps).unwrap());
        caps.max_typed_lookahead_characters = NonZeroUsize::new(31).unwrap();
        assert!(matches!(
            compile(&fixture, &reader, caps).err().unwrap(),
            CompilationMetadataError::TypedMetadata(_)
        ));
        caps = limits();
        caps.max_input_bytes = NonZeroUsize::new(source.len() + 6).unwrap();
        drop(compile(&fixture, &reader, caps).unwrap());
        caps.max_input_bytes = NonZeroUsize::new(source.len() + 5).unwrap();
        expect_limit(
            compile(&fixture, &reader, caps).err().unwrap(),
            "input bytes",
        );
    }
}

#[test]
fn simple_metadata_exact_and_one_under_totals_bound_output() {
    let source = b"# H\n\nbody\n";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    let mut caps = limits();
    caps.max_effective_metadata_bytes = NonZeroUsize::new(10).unwrap();
    caps.max_owned_metadata_bytes = NonZeroUsize::new(11).unwrap();
    let mut parser = parse_limits(source.len());
    parser.max_summary_bytes = NonZeroUsize::new(32).unwrap();
    let result = compile_captured_document(
        &reader,
        Path::new(""),
        &fixture.request,
        policy(),
        parser,
        caps,
    )
    .unwrap();
    assert_eq!(
        (result.title(), result.kind(), result.summary()),
        ("H", "guide", "body")
    );
    drop(result);
    for (effective, owned, label) in [
        (9, 11, "effective metadata bytes"),
        (10, 10, "owned metadata bytes"),
    ] {
        caps.max_effective_metadata_bytes = NonZeroUsize::new(effective).unwrap();
        caps.max_owned_metadata_bytes = NonZeroUsize::new(owned).unwrap();
        expect_limit(
            compile_captured_document(
                &reader,
                Path::new(""),
                &fixture.request,
                policy(),
                parser,
                caps,
            )
            .err()
            .unwrap(),
            label,
        );
    }
}

#[test]
fn cancellation_and_past_deadline_preserve_real_causes_and_inputs() {
    let source = b"# H\n";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    fixture.request.cancellation().cancel();
    let error = compile(&fixture, &reader, limits()).err().unwrap();
    assert!(
        matches!(&error,CompilationMetadataError::OriginalRequest(e) if e.kind()==io::ErrorKind::Interrupted)
    );
    assert!(error.source().is_some());
    assert_eq!(reader.retained(Path::new("")).unwrap(), source);
    let deadline = Instant::now() + Duration::from_secs(1);
    let fixture = Fixture::with_deadline(source, deadline);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    let error = compile(&fixture, &reader, limits()).err().unwrap();
    assert!(
        matches!(&error,CompilationMetadataError::OriginalRequest(e) if e.kind()==io::ErrorKind::TimedOut)
    );
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(reader.retained(Path::new("")).unwrap(), source);
}

#[test]
fn malformed_known_duplicates_reject_without_io_or_partial_output() {
    let source = b"---\ndescription: x\n'description': y\n---\n# H\n";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    let error = compile(&fixture, &reader, limits()).err().unwrap();
    assert!(matches!(error, CompilationMetadataError::Parser(_)));
    assert_eq!(reader.retained(Path::new("")).unwrap(), source);
}

#[test]
fn selected_file_compiles_from_directory_plan_and_directory_is_not_a_file() {
    let source = b"# H\n";
    let fixture = Fixture::with_document(source);
    let root = fixture._temporary.0.join("project/bundle");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("doc.md"), source).unwrap();
    fs::write(root.join("other"), b"x").unwrap();
    let retention = RetainedBytesLimits::new(source.len(), 1, 4096).unwrap();
    let capture_policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        CopyLimits::new((source.len() + 1) as u64, 3, 11, 4).unwrap(),
        RetainedFileSet::new([PathBuf::from("doc.md")], retention).unwrap(),
        retention,
    )
    .unwrap();
    let reader = CapturedSourceReader::capture_project(
        &fixture.scope,
        Path::new("bundle"),
        NodeKind::Directory,
        &capture_policy,
        SourceReadLimits::new(source.len(), 4096, (source.len() + 1) as u64).unwrap(),
        &fixture.request,
    )
    .unwrap();
    assert_eq!(reader.nodes().len(), 3);
    let caps = limits();
    let result = compile_captured_document(
        &reader,
        Path::new("doc.md"),
        &fixture.request,
        policy(),
        parse_limits(source.len()),
        caps,
    )
    .unwrap();
    assert_eq!(result.whole_source().as_bytes(), source);
    drop(result);
    let error = compile_captured_document(
        &reader,
        Path::new(""),
        &fixture.request,
        policy(),
        parse_limits(source.len()),
        caps,
    )
    .err()
    .unwrap();
    assert!(matches!(error, CompilationMetadataError::NonFile));
    assert!(error.source().is_none());
}
#[test]
fn empty_retained_file_is_valid_and_source_does_not_become_absent() {
    let fixture = Fixture::with_document(b"");
    let reader = capture(&fixture, &fixture.request, 1).unwrap();
    assert_eq!(reader.retained(Path::new("")), Some(b"".as_slice()));
    let supplied = PublicationMetadataPolicy {
        fallback_title: String::new(),
        kind_on_absent: "guide".into(),
    };
    let result = compile_captured_document(
        &reader,
        Path::new(""),
        &fixture.request,
        supplied,
        parse_limits(1),
        limits(),
    )
    .unwrap();
    assert_eq!(
        (
            result.whole_source(),
            result.title(),
            result.summary(),
            result.kind()
        ),
        ("", "", "", "guide")
    );
    assert!(result.description().is_none());
    assert!(!result.summary_truncated());
}
