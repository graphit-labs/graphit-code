//! Retained FILE -> native P1 -> pure mapping -> actual Data and reference kernels.
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
use graphit_data::knowledge_document_projection::{
    self as projection, KnowledgeDocumentProjectionError, KnowledgeDocumentProjectionLimits,
    MemberRow,
};
use graphit_fs::NodeKind;
use graphit_knowledge::{
    compilation_fingerprint::{CompilationFingerprintLimits, fingerprint_compiled},
    document::{DocumentParseLimits, parse_document},
    native_document_mapping::{
        AuthoredDocumentInput, NativeAuthoredMappingError, NativeAuthoredMappingLimits,
        PreparedAuthoredMapping, prepare_native_authored_mapping,
    },
    publication_metadata::{
        CompilationMetadataError, CompiledDocument, PublicationMetadataLimits,
        PublicationMetadataPolicy, compile_captured_document,
    },
    references::{PageReferences, ReferenceError, ReferenceGraph, ReferenceLimits},
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
    time::{Duration, Instant, SystemTime},
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
                "graphit-native-mapping-{}-{nonce}",
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
    physical_name: PathBuf,
    _temporary: TemporaryDirectory,
}
impl Fixture {
    fn with_document(document: &[u8]) -> Self {
        Self::with_name(
            document,
            Path::new("document.md"),
            Instant::now() + Duration::from_secs(30),
        )
    }
    fn with_name(document: &[u8], physical_name: &Path, deadline: Instant) -> Self {
        let temporary = TemporaryDirectory::new();
        let root = temporary.0.join("project");
        fs::create_dir_all(root.join(".graphit")).unwrap();
        fs::write(root.join(physical_name), document).unwrap();
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
            physical_name: physical_name.to_owned(),
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
        &fixture.physical_name,
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
        max_links: cap(100),
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

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
fn mapping_limits() -> NativeAuthoredMappingLimits {
    NativeAuthoredMappingLimits {
        max_field_bytes: nz(10_000),
        max_uri_bytes: nz(4096),
        max_target_bytes: nz(4096),
    }
}
fn fingerprint_limits() -> CompilationFingerprintLimits {
    CompilationFingerprintLimits {
        max_preimage_bytes: nz(100_000),
    }
}
fn reference_limits() -> ReferenceLimits {
    ReferenceLimits {
        max_identifier_bytes: nz(4096),
    }
}
fn projection_limits() -> KnowledgeDocumentProjectionLimits {
    KnowledgeDocumentProjectionLimits {
        max_documents: nz(10),
        max_observations: nz(10),
        max_rows: nz(200),
        max_field_bytes: nz(10_000),
        max_text_bytes: nz(100_000),
    }
}
fn input<'d, 's>(
    compiled: &'d CompiledDocument<'s>,
    slug: &'d str,
    origin: &'d str,
) -> AuthoredDocumentInput<'d, 's> {
    AuthoredDocumentInput {
        compiled,
        slug,
        source_path: origin,
        updated_at: None,
        stale_since: None,
        stale_after: None,
    }
}
fn prepare<'d, 's, 'r>(
    ctx: &'r Arc<SnapshotContext>,
    inputs: &[AuthoredDocumentInput<'d, 's>],
    caps: NativeAuthoredMappingLimits,
) -> Result<PreparedAuthoredMapping<'d, 's, 'r>, NativeAuthoredMappingError> {
    prepare_native_authored_mapping(
        ctx,
        inputs,
        parse_limits(10_000),
        caps,
        fingerprint_limits(),
        reference_limits(),
    )
}
fn views<'a>(
    mapped: &'a PreparedAuthoredMapping<'_, '_, '_>,
    ctx: &Arc<SnapshotContext>,
) -> Vec<Vec<&'a str>> {
    mapped
        .documents(ctx)
        .unwrap()
        .iter()
        .map(|d| {
            d.resolved_wiki_targets()
                .iter()
                .map(String::as_str)
                .collect()
        })
        .collect()
}
fn expect_limit(error: NativeAuthoredMappingError, name: &'static str) {
    assert!(matches!(error, NativeAuthoredMappingError::LimitExceeded(k) if k == name));
}
#[test]
fn integral_source_editorial_states_digests_and_temporal_state_reach_real_headers() {
    for (source, description) in [
        (
            "\u{feff}---\r\ntitle: ''\r\nkind: custom\r\ndescription: ''\r\nreferences: []\r\n---\r\n# Ignored title\r\n\r\n[[ghost|Visible label]]\r\n",
            Some(""),
        ),
        (
            "---\ntitle: T\nkind: ''\ndescription: null\n---\n# H\n\nbody\n",
            None,
        ),
        ("# H\n\nbody\n", None),
    ] {
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
        let compiled = compile(&fixture, &reader, limits()).unwrap();
        let updated = SystemTime::UNIX_EPOCH + Duration::from_secs(123);
        let since = SystemTime::UNIX_EPOCH + Duration::from_secs(124);
        let mut document = input(&compiled, "opaque-ID", "docs/logical.md");
        document.updated_at = Some(updated);
        document.stale_since = Some(since);
        document.stale_after = Some(Duration::from_secs(60));
        let mapped = prepare(&fixture.request, &[document], mapping_limits()).unwrap();
        let targets = views(&mapped, &fixture.request);
        let fields = mapped.authored_fields(&fixture.request, &targets).unwrap();
        let expected_digest =
            fingerprint_compiled(&fixture.request, &compiled, fingerprint_limits()).unwrap();
        let prepared =
            projection::prepare(&fixture.request, &fields, &[], projection_limits()).unwrap();
        let MemberRow::DocumentHeader(header) = &prepared.rows(&fixture.request).unwrap()[0] else {
            panic!("actual H header missing");
        };
        assert_eq!(header.slug, "opaque-ID");
        assert_eq!(header.source_path, "docs/logical.md");
        assert_eq!(header.body.as_bytes(), source.as_bytes());
        assert!(std::ptr::eq(
            header.body.as_ptr(),
            compiled.whole_source().as_ptr()
        ));
        assert!(std::ptr::eq(
            header.title.as_ptr(),
            compiled.title().as_ptr()
        ));
        assert_eq!(header.title, compiled.title());
        assert_eq!(header.kind, compiled.kind());
        assert_eq!(header.description, description);
        assert_eq!(header.summary, compiled.summary());
        assert_eq!(header.summary_truncated, compiled.summary_truncated());
        assert_eq!(
            header.physical_digest.unwrap().as_bytes(),
            compiled.physical_source_digest().as_bytes()
        );
        assert_eq!(
            header.compilation_digest.as_bytes(),
            expected_digest.as_bytes()
        );
        assert_ne!(
            header.physical_digest.unwrap().as_bytes(),
            header.compilation_digest.as_bytes()
        );
        assert_eq!(header.recipe.id(), projection::AUTHORED_RECIPE_ID);
        let parameters = header.recipe.parameters();
        assert_eq!(
            parameters.fallback_title,
            compiled.recipe_policy().fallback_title
        );
        assert_eq!(
            parameters.kind_on_absent,
            compiled.recipe_policy().kind_on_absent
        );
        assert_eq!(
            parameters.max_summary_bytes.get() as u64,
            compiled.summary_limit()
        );
        assert_eq!(
            (header.updated_at, header.stale_since, header.stale_after),
            (Some(updated), Some(since), Some(Duration::from_secs(60)))
        );
        assert_eq!(reader.retained(Path::new("")).unwrap(), source.as_bytes());
    }
}

#[test]
fn summary_truncation_and_policy_are_not_recomputed_by_mapping() {
    let source = b"# Long heading\n\nlong summary of the document\n";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    let mut parser = parse_limits(source.len());
    parser.max_summary_bytes = nz(4);
    let compiled = compile_captured_document(
        &reader,
        Path::new(""),
        &fixture.request,
        PublicationMetadataPolicy {
            fallback_title: "Custom fallback".into(),
            kind_on_absent: "manual".into(),
        },
        parser,
        limits(),
    )
    .unwrap();
    assert!(compiled.summary_truncated());
    let mapped = prepare(
        &fixture.request,
        &[input(&compiled, "a", "docs/a")],
        mapping_limits(),
    )
    .unwrap();
    let targets = views(&mapped, &fixture.request);
    let fields = mapped.authored_fields(&fixture.request, &targets).unwrap();
    let prepared =
        projection::prepare(&fixture.request, &fields, &[], projection_limits()).unwrap();
    let MemberRow::DocumentHeader(header) = &prepared.rows(&fixture.request).unwrap()[0] else {
        panic!("header");
    };
    assert_eq!(header.summary, compiled.summary());
    assert!(header.summary_truncated);
    assert_eq!(header.recipe.parameters().max_summary_bytes, nz(4));
    assert_eq!(header.kind, "manual");
    assert_eq!(header.recipe.parameters().fallback_title, "Custom fallback");
}

#[test]
fn raw_targets_preserve_order_duplicates_empty_self_and_dangling_without_body_rewrite() {
    let source = b"---\ndescription: '[[yaml]]'\n---\n[x](b.md) [y](b#fragment) [[ghost|Label]] [[ghost]] [e](a%23b.md) [self](#h) [empty]() [[id.md]] [bad-escape](a%ZZ.md)\n\n[ref][r]\n\n[r]: b.md\n\n![image](img.md) [asset](x.png) [source](docs/a.md) [external](https://e/a.md) [absolute](/a.md) [scheme](wiki://a)\n\n`[inline](code.md)`\n\n```\n[[fenced]]\n```\n";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    let compiled = compile(&fixture, &reader, limits()).unwrap();
    let mapped = prepare(
        &fixture.request,
        &[
            input(&compiled, "a", "docs/a.md"),
            input(&compiled, "b", "docs/b.md"),
        ],
        mapping_limits(),
    )
    .unwrap();
    let targets = views(&mapped, &fixture.request);
    assert_eq!(
        targets[0],
        [
            "b",
            "b",
            "ghost",
            "ghost",
            "source-t-646f6373-6123622e6d64",
            "a",
            "",
            "id.md",
            "source-t-646f6373-61255a5a2e6d64",
            "b",
            "source-t-646f6373-646f6373-612e6d64"
        ]
    );
    let fields = mapped.authored_fields(&fixture.request, &targets).unwrap();
    let prepared =
        projection::prepare(&fixture.request, &fields, &[], projection_limits()).unwrap();
    let rows = prepared.rows(&fixture.request).unwrap();
    let MemberRow::DocumentHeader(header) = &rows[0] else {
        panic!("header");
    };
    assert_eq!(header.body.as_bytes(), source);
    assert_eq!(header.resolved_wiki_targets.as_slice(), targets[0]);
    let pages = fields
        .iter()
        .map(|f| PageReferences {
            slug: f.slug.unwrap(),
            title: f.title.unwrap(),
            targets: f.resolved_wiki_targets,
        })
        .collect::<Vec<_>>();
    let graph = ReferenceGraph::build(&fixture.request, &pages, reference_limits()).unwrap();
    let broken = graph.broken_links(&fixture.request, nz(20)).unwrap();
    assert!(
        broken
            .iter()
            .any(|link| link.source == "a" && link.target == "ghost")
    );
    assert!(
        !broken
            .iter()
            .any(|link| link.target == "b" || link.target.is_empty())
    );
    let targets_in_rows: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            MemberRow::WikiTarget {
                document_index: 0,
                target,
                ..
            } => Some(*target),
            _ => None,
        })
        .collect();
    assert_eq!(targets_in_rows, targets[0]);
}

#[test]
fn relative_markdown_uses_exact_source_identity_and_preserves_literal_ids() {
    use graphit_data::knowledge_document_lifecycle::KnowledgeSourceIdentity;
    let source = b"[same](./manual.md#part) [nested](../guides/Guide%20%C3%A9.md?raw=1#part) [again](./manual.md) [[manual.md]] [[opaque]] [anchor](#part) [web](https://example.test/manual.md) [absolute](/manual.md) [network](//host/manual.md) [encoded](%2Fmanual.md) [other](image.png)";
    let f = Fixture::with_document(source);
    let reader = capture(&f, &f.request, source.len()).unwrap();
    let compiled = compile(&f, &reader, limits()).unwrap();
    let origin = KnowledgeSourceIdentity::from_relative_path(
        Path::new("docs/architecture/storage_layout.md"),
        None,
        nz(4096),
    )
    .unwrap();
    let target = KnowledgeSourceIdentity::from_relative_path(
        Path::new("docs/architecture/manual.md"),
        None,
        nz(4096),
    )
    .unwrap();
    let manual = KnowledgeSourceIdentity::from_relative_path(
        Path::new("docs/guides/Guide é.md"),
        Some("Authored Ω"),
        nz(4096),
    )
    .unwrap();
    let inputs = [
        input(&compiled, origin.slug(), origin.source_path()),
        input(&compiled, target.slug(), target.source_path()),
        input(&compiled, manual.slug(), manual.source_path()),
    ];
    let mapped = prepare(&f.request, &inputs, mapping_limits()).unwrap();
    let targets = views(&mapped, &f.request);
    assert_eq!(
        targets[0],
        [
            target.slug(),
            "Authored Ω",
            target.slug(),
            "manual.md",
            "opaque",
            origin.slug()
        ]
    );
    let fields = mapped.authored_fields(&f.request, &targets).unwrap();
    assert_eq!(fields[0].body.unwrap().as_bytes(), source);
    assert_eq!(
        fields[0].compilation_digest.as_bytes(),
        fingerprint_compiled(&f.request, &compiled, fingerprint_limits())
            .unwrap()
            .as_bytes()
    );
}

#[test]
fn relative_markdown_rejects_project_escape_and_bounds_canonical_identifier() {
    for raw in [
        "[x](../../outside.md)",
        "[x](%2e%2e/%2e%2e/outside.md)",
        "[x](a%00.md)",
    ] {
        let f = Fixture::with_document(raw.as_bytes());
        let reader = capture(&f, &f.request, raw.len()).unwrap();
        let compiled = compile(&f, &reader, limits()).unwrap();
        assert!(matches!(
            prepare(
                &f.request,
                &[input(&compiled, "source", "docs/a.md")],
                mapping_limits()
            ),
            Err(NativeAuthoredMappingError::InvalidTarget { document: 0 })
        ));
    }
    let raw = b"[missing](missing.md)";
    let f = Fixture::with_document(raw);
    let reader = capture(&f, &f.request, raw.len()).unwrap();
    let compiled = compile(&f, &reader, limits()).unwrap();
    let entries = [input(&compiled, "source", "docs/a.md")];
    let target = "source-t-646f6373-6d697373696e672e6d64";
    let mut caps = mapping_limits();
    caps.max_target_bytes = nz(target.len());
    let mapped = prepare(&f.request, &entries, caps).unwrap();
    assert_eq!(views(&mapped, &f.request)[0], [target]);
    caps.max_target_bytes = nz(target.len() - 1);
    expect_limit(
        prepare(&f.request, &entries, caps).unwrap_err(),
        "target bytes",
    );
}

#[test]
fn logical_identity_and_origin_are_exact_and_collisions_reject_the_whole_plan() {
    let fixture = Fixture::with_document(b"# H\n");
    let reader = capture(&fixture, &fixture.request, fixture.document_bytes).unwrap();
    let compiled = compile(&fixture, &reader, limits()).unwrap();
    for origin in [
        "", "/a", "a/", "a//b", ".", "a/..", "a/./b", "a:b", "a\\b", "a\nb",
    ] {
        assert!(matches!(
            prepare(
                &fixture.request,
                &[input(&compiled, "a", origin)],
                mapping_limits()
            ),
            Err(NativeAuthoredMappingError::InvalidOrigin { document: 0 })
        ));
    }
    assert!(matches!(
        prepare(
            &fixture.request,
            &[input(&compiled, "a", "one"), input(&compiled, "b", "one")],
            mapping_limits()
        ),
        Err(NativeAuthoredMappingError::DuplicateOrigin {
            first: 0,
            second: 1
        })
    ));
    let error = prepare(
        &fixture.request,
        &[input(&compiled, "a", "one"), input(&compiled, "a", "two")],
        mapping_limits(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        NativeAuthoredMappingError::References(ReferenceError::DuplicatePage)
    ));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<ReferenceError>()
            .is_some()
    );
    for slug in ["", "..a", "a/b", "a\\b", "a\nb"] {
        assert!(matches!(
            prepare(
                &fixture.request,
                &[input(&compiled, slug, "one")],
                mapping_limits()
            ),
            Err(NativeAuthoredMappingError::References(
                ReferenceError::InvalidIdentifier
            ))
        ));
    }
    let mapped = prepare(
        &fixture.request,
        &[
            input(&compiled, "A", "docs/A"),
            input(&compiled, "a", "docs/a"),
            input(&compiled, "é", "docs/é"),
            input(&compiled, "e\u{301}", "docs/e\u{301}"),
        ],
        mapping_limits(),
    )
    .unwrap();
    assert_eq!(mapped.documents(&fixture.request).unwrap().len(), 4);
}

#[cfg(unix)]
#[test]
fn native_non_utf8_filename_is_rejected_before_logical_mapping() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let name = PathBuf::from(OsString::from_vec(b"physical-\xff.md".to_vec()));
    let fixture = Fixture::with_name(
        b"# Physical\n",
        &name,
        Instant::now() + Duration::from_secs(30),
    );
    assert!(capture(&fixture, &fixture.request, fixture.document_bytes).is_err());
}

#[test]
fn derived_empty_invalid_decoded_targets_and_utf8_causes_are_not_filtered() {
    for (source, kind) in [
        ("[x](.md)", "empty"),
        ("[[a%2Fb]]", "identifier"),
        ("[x](%FF.md)", "utf8"),
    ] {
        let fixture = Fixture::with_document(source.as_bytes());
        let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
        let compiled = compile(&fixture, &reader, limits()).unwrap();
        let error = prepare(
            &fixture.request,
            &[input(&compiled, "a", "docs/a")],
            mapping_limits(),
        )
        .unwrap_err();
        match kind {
            "empty" => assert!(matches!(
                error,
                NativeAuthoredMappingError::InvalidTarget { document: 0 }
            )),
            "identifier" => {
                assert!(matches!(
                    error,
                    NativeAuthoredMappingError::References(ReferenceError::InvalidIdentifier)
                ));
                assert!(error.source().is_some());
            }
            "utf8" => {
                assert!(matches!(error, NativeAuthoredMappingError::PercentUtf8(_)));
                assert!(
                    error
                        .source()
                        .unwrap()
                        .downcast_ref::<std::str::Utf8Error>()
                        .is_some()
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(reader.retained(Path::new("")).unwrap(), source.as_bytes());
    }
}

#[test]
fn individual_mapping_caps_accept_equality_and_reject_one_over_including_external_raw_links() {
    let source = b"[x](%62%62.md) [y](https://example.test/asset) [[bb]]";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    let compiled = compile(&fixture, &reader, limits()).unwrap();
    let entry = input(&compiled, "a", "docs/a");
    let parsed = parse_document(&fixture.request, source, "F", parse_limits(source.len())).unwrap();
    let uris: Vec<_> = parsed
        .markdown
        .iter()
        .filter_map(|m| match &m.event {
            Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.len()),
            _ => None,
        })
        .collect();
    let max_field = [
        compiled.whole_source().len(),
        compiled.summary().len(),
        compiled.title().len(),
        compiled.kind().len(),
        entry.source_path.len(),
        entry.slug.len(),
        compiled.recipe_policy().fallback_title.len(),
        compiled.recipe_policy().kind_on_absent.len(),
    ]
    .into_iter()
    .max()
    .unwrap();
    for (kind, exact) in [
        ("field bytes", max_field),
        ("URI bytes", *uris.iter().max().unwrap()),
        ("target bytes", 2),
    ] {
        let mut caps = mapping_limits();
        let slot = match kind {
            "field bytes" => &mut caps.max_field_bytes,
            "URI bytes" => &mut caps.max_uri_bytes,
            "target bytes" => &mut caps.max_target_bytes,
            _ => unreachable!(),
        };
        *slot = nz(exact);
        let entries = [entry, input(&compiled, "bb", "docs/bb.md")];
        prepare(&fixture.request, &entries, caps).unwrap();
        let mut over = caps;
        match kind {
            "field bytes" => over.max_field_bytes = nz(exact - 1),
            "URI bytes" => over.max_uri_bytes = nz(exact - 1),
            "target bytes" => over.max_target_bytes = nz(exact - 1),
            _ => unreachable!(),
        }
        expect_limit(prepare(&fixture.request, &entries, over).unwrap_err(), kind);
    }
}

#[test]
fn producer_limits_errors_and_h_temporal_validation_keep_actual_causes() {
    let source = b"[x](b.md) [y](b.md)";
    let fixture = Fixture::with_document(source);
    let reader = capture(&fixture, &fixture.request, source.len()).unwrap();
    let compiled = compile(&fixture, &reader, limits()).unwrap();
    let entries = [input(&compiled, "a", "docs/a")];
    let parsed = parse_document(
        &fixture.request,
        source,
        &compiled.recipe_policy().fallback_title,
        parse_limits(source.len()),
    )
    .unwrap();
    let mut exact_parser = parse_limits(source.len());
    exact_parser.max_links = nz(2);
    exact_parser.max_markdown_events = nz(parsed.markdown.len());
    let repeated = [entries[0], input(&compiled, "b", "docs/b")];
    prepare_native_authored_mapping(
        &fixture.request,
        &repeated,
        exact_parser,
        mapping_limits(),
        fingerprint_limits(),
        reference_limits(),
    )
    .unwrap();
    let mut events_exceeded = exact_parser;
    events_exceeded.max_markdown_events = nz(parsed.markdown.len() - 1);
    let error = prepare_native_authored_mapping(
        &fixture.request,
        &repeated,
        events_exceeded,
        mapping_limits(),
        fingerprint_limits(),
        reference_limits(),
    )
    .unwrap_err();
    assert!(matches!(error, NativeAuthoredMappingError::Parser(_)));
    assert!(error.source().is_some());
    let mut parser = exact_parser;
    parser.max_links = nz(1);
    let error = prepare_native_authored_mapping(
        &fixture.request,
        &entries,
        parser,
        mapping_limits(),
        fingerprint_limits(),
        reference_limits(),
    )
    .unwrap_err();
    assert!(matches!(error, NativeAuthoredMappingError::Parser(_)));
    assert!(error.source().is_some());
    let mut fp = fingerprint_limits();
    fp.max_preimage_bytes = nz(1);
    let error = prepare_native_authored_mapping(
        &fixture.request,
        &entries,
        parse_limits(source.len()),
        mapping_limits(),
        fp,
        reference_limits(),
    )
    .unwrap_err();
    assert!(matches!(error, NativeAuthoredMappingError::Fingerprint(_)));
    assert!(error.source().is_some());
    let mut refs = reference_limits();
    refs.max_identifier_bytes = nz(1);
    let error = prepare_native_authored_mapping(
        &fixture.request,
        &[input(&compiled, "aa", "docs/a")],
        parse_limits(source.len()),
        mapping_limits(),
        fingerprint_limits(),
        refs,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        NativeAuthoredMappingError::References(ReferenceError::LimitExceeded("identifier bytes"))
    ));
    assert!(error.source().is_some());
    for stale_after in [Duration::ZERO, Duration::from_secs(1)] {
        let mut entry = entries[0];
        entry.stale_after = Some(stale_after);
        let mapped = prepare(&fixture.request, &[entry], mapping_limits()).unwrap();
        let targets = views(&mapped, &fixture.request);
        let fields = mapped.authored_fields(&fixture.request, &targets).unwrap();
        assert!(matches!(
            projection::prepare(&fixture.request, &fields, &[], projection_limits()),
            Err(KnowledgeDocumentProjectionError::InvalidPolicy)
        ));
    }
}

#[test]
fn views_must_match_original_groups_and_a_foreign_context_cannot_revive_them() {
    let fixture = Fixture::with_document(b"[[ghost]] [[ghost]]");
    let reader = capture(&fixture, &fixture.request, fixture.document_bytes).unwrap();
    let compiled = compile(&fixture, &reader, limits()).unwrap();
    let mapped = prepare(
        &fixture.request,
        &[input(&compiled, "a", "docs/a")],
        mapping_limits(),
    )
    .unwrap();
    for targets in [
        vec![],
        vec![vec!["ghost"]],
        vec![vec!["changed", "ghost"]],
        vec![vec!["ghost", "ghost"], vec![]],
    ] {
        assert!(matches!(
            mapped.authored_fields(&fixture.request, &targets),
            Err(NativeAuthoredMappingError::InvalidTargetViews)
        ));
    }
    let foreign = request();
    assert!(matches!(
        mapped.documents(&foreign),
        Err(NativeAuthoredMappingError::DifferentRequest)
    ));
    let target_views = views(&mapped, &fixture.request);
    assert!(matches!(
        mapped.authored_fields(&foreign, &target_views),
        Err(NativeAuthoredMappingError::DifferentRequest)
    ));
    let error = prepare(
        &foreign,
        &[input(&compiled, "a", "docs/a")],
        mapping_limits(),
    )
    .unwrap_err();
    assert!(matches!(error, NativeAuthoredMappingError::Request(_)));
    fixture.request.cancellation().cancel();
    for error in [
        mapped.documents(&foreign).err().unwrap(),
        mapped
            .authored_fields(&foreign, &target_views)
            .err()
            .unwrap(),
        prepare(
            &fixture.request,
            &[input(&compiled, "a", "docs/a")],
            mapping_limits(),
        )
        .unwrap_err(),
    ] {
        assert!(
            matches!(&error, NativeAuthoredMappingError::Request(e) if e.kind() == io::ErrorKind::Interrupted)
        );
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }
    assert_eq!(
        reader.retained(Path::new("")).unwrap(),
        b"[[ghost]] [[ghost]]"
    );
}

#[test]
fn empty_plans_still_validate_original_foreign_cancelled_and_expired_requests() {
    let ctx = request();
    let mapped = prepare(&ctx, &[], mapping_limits()).unwrap();
    assert!(mapped.documents(&ctx).unwrap().is_empty());
    assert!(mapped.authored_fields(&ctx, &[]).unwrap().is_empty());
    let foreign = request();
    assert!(matches!(
        mapped.documents(&foreign),
        Err(NativeAuthoredMappingError::DifferentRequest)
    ));
    ctx.cancellation().cancel();
    assert!(
        matches!(mapped.documents(&foreign), Err(NativeAuthoredMappingError::Request(e)) if e.kind() == io::ErrorKind::Interrupted)
    );
    assert!(
        matches!(prepare(&ctx, &[], mapping_limits()), Err(NativeAuthoredMappingError::Request(e)) if e.kind() == io::ErrorKind::Interrupted)
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    let error = prepare(&expired, &[], mapping_limits()).unwrap_err();
    assert!(
        matches!(&error, NativeAuthoredMappingError::Request(e) if e.kind() == io::ErrorKind::TimedOut)
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
}

#[test]
fn expired_nonempty_mapping_getters_do_not_use_a_fresh_context() {
    let deadline = Instant::now() + Duration::from_secs(2);
    let fixture = Fixture::with_name(b"# H\n", Path::new("document.md"), deadline);
    let reader = capture(&fixture, &fixture.request, fixture.document_bytes).unwrap();
    let compiled = compile(&fixture, &reader, limits()).unwrap();
    let mapped = prepare(
        &fixture.request,
        &[input(&compiled, "a", "docs/a")],
        mapping_limits(),
    )
    .unwrap();
    let target_views = views(&mapped, &fixture.request);
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    let foreign = request();
    for error in [
        mapped.documents(&foreign).err().unwrap(),
        mapped
            .authored_fields(&foreign, &target_views)
            .err()
            .unwrap(),
        prepare(
            &fixture.request,
            &[input(&compiled, "a", "docs/a")],
            mapping_limits(),
        )
        .unwrap_err(),
    ] {
        assert!(
            matches!(&error, NativeAuthoredMappingError::Request(e) if e.kind() == io::ErrorKind::TimedOut)
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
    }
}
