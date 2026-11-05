#![cfg(any(target_os = "linux", target_os = "macos", windows))]
//! Compiled FILE documents through Knowledge and the real Auth/Data producers.
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_io::SnapshotContext,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    native_path::NativePath,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{
    ArtifactType, NativeBinding, ProjectionLimits, SourceOrigin, SourceRecord, SourceRef,
    native::{ContentLimits, Role, RoleCredentialReference},
    policy::RequestContext,
};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};

fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        },
        cancel,
        Instant::now() + Duration::from_secs(180),
    )
    .unwrap()
}
fn issuer_limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 1024,
    }
}
fn permissions() -> Vec<Permission> {
    vec![
        Permission::Read(Module::Memory),
        Permission::Read(Module::Task),
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
        Permission::WriteMemory,
        Permission::WriteTask,
        Permission::ManageAuthProfiles,
        Permission::ManageAccessGrants,
        Permission::Delegate,
    ]
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>) -> Self {
        Self::with_version(ctx, "")
    }
    fn with_version(ctx: &RequestContext<'_>, version_name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&home).unwrap();
        let install = InstallationPaths::resolve(
            Brand::new("knowledge-test").unwrap(),
            &root,
            Some(&home),
            None,
        )
        .unwrap();
        let private = installation_bootstrap::prepare(
            &install,
            BootstrapLimits::new(65536, 256).unwrap(),
            ctx.snapshot(),
        )
        .into_installation()
        .unwrap();
        let runtime = Arc::new(
            RuntimeKeyOwner::start(
                private,
                RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                ctx.snapshot(),
            )
            .into_owner()
            .ok()
            .unwrap(),
        );
        let paths = ProjectPaths::at(&root, &install).unwrap();
        let limits = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("local-installation").unwrap(),
            [root.clone()],
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let version =
            VersionContext::select(&paths, Checkout::NonGit, version_name, None, "").unwrap();
        let installation_record = installation.binding().unwrap();
        let project_record = project.binding().unwrap();
        Self {
            _dir: dir,
            root,
            runtime,
            installation,
            installation_record,
            project,
            project_record,
            version,
        }
    }
    fn realm(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.installation,
            record: &self.installation_record,
        }
    }
    fn scope(&self) -> ScopeInput<'_> {
        self.scope_for(&self.version)
    }
    fn scope_for<'a>(&'a self, version: &'a VersionContext) -> ScopeInput<'a> {
        ScopeInput {
            installation: self.realm(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
    fn issuer(&self, ctx: &RequestContext<'_>) -> RoleIssuer {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("local-installation").unwrap(),
            [self.installation.owner_root().to_owned()],
            ScopeLimits::new(16, 4096, 128).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let physical = AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let local = Arc::new(
            AuthStore::new(
                physical,
                ProfileLimits {
                    bytes: 4096,
                    nodes: 4096,
                    scalar_bytes: 256,
                    collection_entries: 64,
                    providers: 16,
                    profiles: 16,
                },
                ctx,
            )
            .unwrap(),
        );
        let perms = permissions();
        let source = SourceAuthorityPolicy {
            ordinary: &perms,
            administrative: &perms,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(3600),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = IssuerAuthorityPolicy::new(
            source,
            source,
            issuer_limits(),
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 65536,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap();
        RoleIssuer::new(
            self.realm(),
            self.runtime.clone(),
            local,
            policy,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap()
    }
    fn secret(&self) -> String {
        std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap()
    }
    fn issue(
        &self,
        issuer: &RoleIssuer,
        parent: &AdminParent<'_>,
    ) -> (RoleBearer, RoleCredentialReference) {
        self.issue_for(issuer, parent, &self.version)
    }
    fn issue_for(
        &self,
        issuer: &RoleIssuer,
        parent: &AdminParent<'_>,
        version: &VersionContext,
    ) -> (RoleBearer, RoleCredentialReference) {
        let source = SourceRef::restore(
            SourceRecord {
                remote_file_origin: None,
                archive_sha256: None,
                origin: SourceOrigin::LocalLink,
                producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                artifact_type: ArtifactType::Agent,
                id: format!("roles/coordinator-{}", version.version()),
                physical_id: None,
                version: "1.0.0".into(),
                root: NativePath::new(self.root.clone()).unwrap(),
            },
            ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap(),
        )
        .unwrap();
        let perms = permissions();
        let definition = RoleDefinition::new(
            Role::Coordinator,
            &source,
            NativeBinding::KiroV3,
            &perms,
            [1; 32],
            issuer_limits(),
        )
        .unwrap();
        let reference = definition.reference().clone();
        issuer.register_definition(parent, definition).unwrap();
        let run = issuer
            .start_run(parent, &self.scope_for(version), Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(parent, &run, &reference, &perms, &self.scope_for(version))
            .unwrap();
        (bearer, reference)
    }
}
fn content() -> KnowledgeIndexContentLimits {
    KnowledgeIndexContentLimits {
        max_identifier_bytes: n(256),
    }
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn read_limits() -> KnowledgeIndexReadLimits {
    KnowledgeIndexReadLimits {
        content: content(),
        native: native(),
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
fn write_limits() -> KnowledgeIndexWriteLimits {
    let copy = CopyLimits::new(1024 * 1024, 1024, 65536, 16).unwrap();
    KnowledgeIndexWriteLimits {
        content: content(),
        native: native(),
        artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
    }
}
fn hlimits() -> h::KnowledgeDocumentProjectionLimits {
    h::KnowledgeDocumentProjectionLimits {
        max_documents: n(8),
        max_observations: n(8),
        max_rows: n(64),
        max_field_bytes: n(65536),
        max_text_bytes: n(131072),
    }
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

fn metadata_limits() -> PublicationMetadataLimits {
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
fn mapping_limits() -> NativeAuthoredMappingLimits {
    NativeAuthoredMappingLimits {
        max_field_bytes: n(10_000),
        max_uri_bytes: n(4096),
        max_target_bytes: n(4096),
    }
}
fn fingerprint_limits() -> CompilationFingerprintLimits {
    CompilationFingerprintLimits {
        max_preimage_bytes: n(100_000),
    }
}
fn reference_limits() -> ReferenceLimits {
    ReferenceLimits {
        max_identifier_bytes: n(4096),
    }
}

fn published_lint_parameters() -> (
    graphit_knowledge::published_lint::PublishedLintPolicy,
    graphit_knowledge::published_lint::PublishedLintLimits,
) {
    (
        graphit_knowledge::published_lint::PublishedLintPolicy {
            evaluated_at: UNIX_EPOCH + Duration::from_secs(120),
            max_age: Some(Duration::from_secs(60)),
        },
        graphit_knowledge::published_lint::PublishedLintLimits {
            max_pages: n(10),
            max_input_bytes: n(100_000),
            max_findings: n(100),
            max_reference_result_bytes: n(4096),
            document: parse_limits(10_000),
            references: reference_limits(),
            metadata: graphit_knowledge::published_metadata_lint::PublishedMetadataLintLimits {
                max_pages: n(10),
                max_input_bytes: n(100_000),
                max_findings: n(100),
            },
        },
    )
}

fn markdown_parameters(
    f: &Fixture,
) -> (
    graphit_knowledge::markdown_export::MarkdownExportModes,
    graphit_knowledge::markdown_export::MarkdownExportLimits,
) {
    use graphit_core::artifact_store::PayloadMode;
    use graphit_knowledge::markdown_export::{MarkdownExportLimits, MarkdownExportModes};
    // Explicit caller policy observed from this real fixture; no capture is fabricated.
    let mode =
        PayloadMode::from_permissions(&std::fs::metadata(&f.root).unwrap().permissions()).unwrap();
    let modes = MarkdownExportModes {
        directory: mode,
        file: mode,
    };
    let limits = MarkdownExportLimits {
        max_file_bytes: n(100_000),
        document: parse_limits(10_000),
        references: reference_limits(),
    };
    (modes, limits)
}

use graphit_core::artifact_store::{
    CapturedSourceReader, CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy,
    SourceReadLimits,
};
use graphit_fs::NodeKind;
use graphit_knowledge::{
    compilation_fingerprint::{CompilationFingerprintLimits, fingerprint_compiled},
    document::DocumentParseLimits,
    native_document_mapping::{
        AuthoredDocumentInput, NativeAuthoredMappingError, NativeAuthoredMappingLimits,
        prepare_native_authored_mapping,
    },
    native_index_publication::{
        NativeDocumentPublicationError as Error, NativeDocumentPublicationLimits,
        publish_native_documents,
    },
    publication_metadata::{
        CompiledDocument, PublicationMetadataLimits, PublicationMetadataPolicy,
        compile_captured_document,
    },
    references::{PageReferences, ReferenceGraph, ReferenceLimits},
};
use std::{error::Error as StdError, path::Path};

fn limits() -> NativeDocumentPublicationLimits {
    NativeDocumentPublicationLimits {
        parse: parse_limits(10_000),
        mapping: mapping_limits(),
        fingerprint: fingerprint_limits(),
        references: reference_limits(),
        projection: hlimits(),
        index: write_limits(),
        authored_references:
            graphit_knowledge::document_authored_references::DocumentAuthoredReferenceLimits {
                max_results: n(100),
                max_identifier_bytes: n(4096),
            },
    }
}
fn capture(f: &Fixture, ctx: &RequestContext<'_>, name: &str, body: &str) -> CapturedSourceReader {
    std::fs::write(f.root.join(name), body).unwrap();
    let bytes = body.len().max(1);
    let retention = RetainedBytesLimits::new(bytes, 1, 1).unwrap();
    let copy = CopyLimits::new(u64::try_from(bytes).unwrap(), 1, 1, 1).unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([PathBuf::new()], retention).unwrap(),
        retention,
    )
    .unwrap();
    CapturedSourceReader::capture_project(
        &f.project,
        Path::new(name),
        NodeKind::File,
        &policy,
        SourceReadLimits::new(bytes, 4096, u64::try_from(bytes).unwrap()).unwrap(),
        ctx.snapshot(),
    )
    .unwrap()
}
fn compile<'a>(reader: &'a CapturedSourceReader, ctx: &RequestContext<'_>) -> CompiledDocument<'a> {
    let mut parse = parse_limits(10_000);
    parse.max_summary_bytes = n(16);
    compile_captured_document(
        reader,
        Path::new(""),
        ctx.snapshot(),
        PublicationMetadataPolicy {
            fallback_title: "Título padrão".into(),
            kind_on_absent: "guide".into(),
        },
        parse,
        metadata_limits(),
    )
    .unwrap()
}
fn input<'d, 's>(
    compiled: &'d CompiledDocument<'s>,
    slug: &'d str,
) -> AuthoredDocumentInput<'d, 's> {
    AuthoredDocumentInput {
        compiled,
        slug,
        source_path: slug,
        updated_at: None,
        stale_since: None,
        stale_after: None,
    }
}
fn setup_grant<'a>(
    f: &'a Fixture,
    issuer: &'a RoleIssuer,
    ctx: &'a RequestContext<'a>,
) -> (RoleBearer, RoleCredentialReference) {
    let caller = issuer.authenticate_runtime(&f.secret(), ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    f.issue(issuer, &parent)
}
fn assert_no_birth(f: &Fixture) {
    assert!(!f.project.runtime_root().join("knowledge").exists());
}

#[tokio::test]
async fn captured_documents_roundtrip_full16_and_borrowed_references() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "development");
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let source_z = "\u{feff}---\r\ntitle: ''\r\ntype: custom\r\ndescription: ''\r\n---\r\n# Ignorado\r\n\r\nOlá 世界 [[a]] [[ghost|Ausente]] [[z]] [[a]]\r\n";
    let source_a = "---\ntitle: Página Á\ntype: ''\ndescription: null\n---\n# Outro\n\n[[z]]\n";
    let reader_z = capture(&f, &ctx, "physical-z.md", source_z);
    let reader_a = capture(&f, &ctx, "physical-a.md", source_a);
    let compiled_z = compile(&reader_z, &ctx);
    let compiled_a = compile(&reader_a, &ctx);
    assert_eq!(compiled_z.kind(), "custom");
    assert!(compiled_z.summary_truncated());
    assert_eq!(compiled_a.kind(), "");
    let mut z = input(&compiled_z, "z");
    z.source_path = "docs/logical-z.md";
    z.updated_at = Some(UNIX_EPOCH - Duration::from_nanos(17));
    z.stale_since = Some(UNIX_EPOCH + Duration::from_nanos(9));
    z.stale_after = Some(Duration::from_secs(60));
    let mut a = input(&compiled_a, "a");
    a.source_path = "docs/lógico-a.md";
    let inputs = [z, a];
    let mapping = prepare_native_authored_mapping(
        ctx.snapshot(),
        &inputs,
        limits().parse,
        limits().mapping,
        limits().fingerprint,
        limits().references,
    )
    .unwrap();
    let expected_targets: Vec<_> = mapping
        .documents(ctx.snapshot())
        .unwrap()
        .iter()
        .map(|d| d.resolved_wiki_targets())
        .collect();
    let first = publish_native_documents(ctx.snapshot(), &write, &inputs, limits())
        .await
        .unwrap();
    assert_eq!(
        first.named.as_ref().unwrap().physical_branch.as_deref(),
        Some(f.version.lance_branch().as_str())
    );
    let KnowledgeIndexRead::Published {
        generation,
        documents,
    } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published corpus required")
    };
    assert_eq!(&generation, first.named.as_ref().unwrap());
    let (lint_policy, lint_limits) = published_lint_parameters();
    let documents_before_lint = documents.clone();
    let lint = graphit_knowledge::published_lint::lint_published_snapshot(
        ctx.snapshot(),
        &generation,
        &documents,
        lint_policy,
        lint_limits,
    )
    .unwrap();
    assert!(std::ptr::eq(lint.generation, &generation));
    assert_eq!((lint.total_pages, lint.issue_count), (2, 5));
    assert_eq!(lint.metadata.required_errors, 1);
    assert_eq!(lint.metadata.findings.len(), 3);
    assert!(lint.orphans.is_empty());
    assert_eq!(
        lint.broken_links,
        [graphit_knowledge::published_lint::PublishedBrokenLink {
            source: "z".into(),
            target: "ghost".into(),
        }]
    );
    assert_eq!(
        lint.short_pages
            .iter()
            .map(|page| (page.slug, page.word_count))
            .collect::<Vec<_>>(),
        [("a", 3), ("z", 8)]
    );
    assert_eq!(
        lint.stale_pages,
        [graphit_knowledge::published_lint::PublishedStaleFinding {
            slug: "z",
            reason: graphit_knowledge::published_lint::PublishedStaleReason::Persisted {
                since: UNIX_EPOCH + Duration::from_nanos(9),
            },
        }]
    );
    assert_eq!(documents, documents_before_lint);
    let (modes, markdown_limits) = markdown_parameters(&f);
    let markdown = {
        let mut stage = graphit_knowledge::markdown_export::MarkdownExportStage::new_in(
            ctx.snapshot(),
            &std::env::temp_dir(),
            modes,
            markdown_limits,
            graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
        )
        .unwrap();
        for document in &documents {
            stage.push(ctx.snapshot(), document.clone()).unwrap();
        }
        stage.finish(ctx.snapshot())
    }
    .unwrap();
    assert_eq!(markdown.payload.nodes().len(), 5);
    assert_eq!(
        markdown
            .payload
            .nodes()
            .filter(|n| n.digest().is_some())
            .count(),
        3
    );
    assert_eq!(
        documents
            .iter()
            .map(|d| d.slug.as_str())
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    for (document, (expected, targets)) in documents
        .iter()
        .zip([(a, expected_targets[1]), (z, expected_targets[0])])
    {
        let c = expected.compiled;
        assert_eq!(document.slug, expected.slug);
        assert_eq!(document.title, c.title());
        assert_eq!(document.body.as_bytes(), c.whole_source().as_bytes());
        assert_eq!(document.kind, c.kind());
        assert_eq!(document.summary, c.summary());
        assert_eq!(document.description.as_deref(), c.description());
        assert_eq!(document.summary_truncated, c.summary_truncated());
        assert_eq!(document.source_path, expected.source_path);
        assert_eq!(
            document.physical_digest.unwrap().as_bytes(),
            c.physical_source_digest().as_bytes()
        );
        assert_eq!(
            document.compilation_digest.as_bytes(),
            fingerprint_compiled(ctx.snapshot(), c, limits().fingerprint)
                .unwrap()
                .as_bytes()
        );
        assert_ne!(
            document.physical_digest.unwrap().as_bytes(),
            document.compilation_digest.as_bytes()
        );
        assert_eq!(document.recipe, h::AUTHORED_RECIPE_ID);
        assert_eq!(
            document.recipe_parameters.fallback_title,
            c.recipe_policy().fallback_title
        );
        assert_eq!(
            document.recipe_parameters.kind_on_absent,
            c.recipe_policy().kind_on_absent
        );
        assert_eq!(
            u64::try_from(document.recipe_parameters.max_summary_bytes.get()).unwrap(),
            c.summary_limit()
        );
        assert_eq!(document.updated_at, expected.updated_at);
        assert_eq!(document.stale_since, expected.stale_since);
        assert_eq!(document.stale_after, expected.stale_after);
        assert_eq!(document.resolved_wiki_targets, targets);

        let rendered = graphit_knowledge::document_source::render_document_source(
            ctx.snapshot(),
            document,
            &graphit_core::text_slice::Request::default(),
            graphit_knowledge::document_source::DocumentSourceLimits {
                max_input_bytes: n(65536),
                max_targets: n(128),
                max_virtual_bytes: n(65536),
                max_virtual_lines: n(1000),
                max_output_bytes: n(65536),
                max_matches: n(1000),
            },
        )
        .unwrap();
        let (yaml, body) = rendered
            .source
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n\n")
            .unwrap();
        let header: serde_json::Value = serde_saphyr::from_str(yaml).unwrap();
        assert_eq!(body.as_bytes(), document.body.as_bytes());
        assert_eq!(
            header,
            serde_json::json!({
                "id": document.slug, "title": document.title, "type": document.kind,
                "summary": document.summary, "description": document.description,
                "summary_truncated": document.summary_truncated, "source_path": document.source_path,
                "physical_digest": hex::encode(document.physical_digest.unwrap().as_bytes()),
                "compilation_digest": hex::encode(document.compilation_digest.as_bytes()),
                "recipe": document.recipe,
                "recipe_parameters": {
                    "fallback_title": document.recipe_parameters.fallback_title,
                    "kind_on_absent": document.recipe_parameters.kind_on_absent,
                    "max_summary_bytes": document.recipe_parameters.max_summary_bytes.get(),
                },
                "updated_at": if document.slug == "z" { Some("1969-12-31T23:59:59.999999983Z") } else { None },
                "stale_since": if document.slug == "z" { Some("1970-01-01T00:00:00.000000009Z") } else { None },
                "stale_after_ns": document.stale_after.map(|d| u64::try_from(d.as_nanos()).unwrap()),
                "resolved_wiki_targets": document.resolved_wiki_targets,
            })
        );
    }
    use graphit_knowledge::published_metadata_lint::{
        PublishedMetadataField as Field, PublishedMetadataFinding as Finding,
        PublishedMetadataLintLimits as LintLimits, PublishedMetadataSeverity as Severity,
        lint_published_metadata,
    };
    assert!(documents.iter().all(|d| !d.summary.is_empty()));
    let metadata_report = lint_published_metadata(
        ctx.snapshot(),
        &documents,
        LintLimits {
            max_pages: n(2),
            max_input_bytes: n(1024),
            max_findings: n(3),
        },
    )
    .unwrap();
    assert_eq!(
        (metadata_report.total_pages, metadata_report.required_errors),
        (2, 1)
    );
    assert_eq!(
        metadata_report.findings,
        [
            Finding {
                slug: "a",
                field: Field::Kind,
                severity: Severity::Required
            },
            Finding {
                slug: "a",
                field: Field::Description,
                severity: Severity::Recommended
            },
            Finding {
                slug: "z",
                field: Field::Description,
                severity: Severity::Recommended
            },
        ]
    );
    assert_eq!(documents[0].description, None);
    assert_eq!(documents[1].description.as_deref(), Some(""));
    let views: Vec<Vec<&str>> = documents
        .iter()
        .map(|d| d.resolved_wiki_targets.iter().map(String::as_str).collect())
        .collect();
    let pages: Vec<_> = documents
        .iter()
        .zip(&views)
        .map(|(d, targets)| PageReferences {
            slug: &d.slug,
            title: &d.title,
            targets,
        })
        .collect();
    let graph = ReferenceGraph::build(ctx.snapshot(), &pages, reference_limits()).unwrap();
    let broken = graph.broken_links(ctx.snapshot(), n(10)).unwrap();
    assert_eq!(broken.len(), 1);
    assert_eq!((broken[0].source, broken[0].target), ("z", "ghost"));
    assert!(graph.orphans(ctx.snapshot(), n(10)).unwrap().is_empty());
    let error = publish_native_documents(ctx.snapshot(), &write, &inputs, limits())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Data(KnowledgeIndexError::InitialTargetChanged)
    ));
    assert!(StdError::source(&error).is_some());
    let reopened = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    assert_eq!(
        reopened,
        KnowledgeIndexRead::Published {
            generation,
            documents
        }
    );
}

#[tokio::test]
async fn empty_publication_has_a_generation_and_needs_only_write_permission() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    assert_eq!(
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        KnowledgeIndexRead::NoCorpus
    );
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let write_bearer = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[Permission::MutateModule(Module::Knowledge)],
            &f.scope(),
        )
        .unwrap();
    let write_grant = issuer
        .verify(
            write_bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &ctx,
        )
        .unwrap();
    assert!(matches!(
        authority.admit_read(&write_grant, &ctx, ProjectReadModule::Knowledge),
        Err(AuthError::Denied)
    ));
    let write = authority
        .admit_write(&write_grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let first = publish_native_documents(ctx.snapshot(), &write, &[], limits())
        .await
        .unwrap();
    assert!(first.named.is_none());
    let KnowledgeIndexRead::Published {
        generation,
        documents,
    } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("empty publication is not absence")
    };
    assert!(documents.is_empty());
    assert_eq!(generation, first.default);
    let (lint_policy, lint_limits) = published_lint_parameters();
    assert!(matches!(
        graphit_knowledge::published_lint::lint_published_snapshot(
            ctx.snapshot(),
            &generation,
            &documents,
            lint_policy,
            lint_limits,
        ),
        Err(graphit_knowledge::published_lint::PublishedLintError::NoDocuments)
    ));
    let (modes, markdown_limits) = markdown_parameters(&f);
    assert!(matches!(
        {
            let mut stage = graphit_knowledge::markdown_export::MarkdownExportStage::new_in(
                ctx.snapshot(),
                &std::env::temp_dir(),
                modes,
                markdown_limits,
                graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
            )
            .unwrap();
            for document in &documents {
                stage.push(ctx.snapshot(), document.clone()).unwrap();
            }
            stage.finish(ctx.snapshot())
        },
        Err(graphit_knowledge::markdown_export::MarkdownExportError::NoDocuments)
    ));
    assert!(matches!(
        graphit_knowledge::published_metadata_lint::lint_published_metadata(
            ctx.snapshot(),
            &documents,
            graphit_knowledge::published_metadata_lint::PublishedMetadataLintLimits {
                max_pages: n(1),
                max_input_bytes: n(1),
                max_findings: n(1),
            },
        ),
        Err(graphit_knowledge::published_metadata_lint::PublishedMetadataLintError::NoDocuments)
    ));
    assert!(matches!(
        publish_native_documents(ctx.snapshot(), &write, &[], limits()).await,
        Err(Error::Data(KnowledgeIndexError::InitialTargetChanged))
    ));
}

#[tokio::test]
async fn actual_auth_precedes_invalid_mapping_and_revocation_has_zero_birth() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let reader = capture(&f, &ctx, "bad-origin.md", "# Documento");
    let compiled = compile(&reader, &ctx);
    let invalid = [input(&compiled, "")];
    let wrong = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    assert!(matches!(
        publish_native_documents(ctx.snapshot(), &wrong, &invalid, limits()).await,
        Err(Error::Data(KnowledgeIndexError::WrongAdmission))
    ));
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let foreign = Arc::new(SnapshotContext::new(
        ctx.deadline().expect("finite fixture deadline"),
        Cancellation::default(),
    ));
    assert!(matches!(
        publish_native_documents(&foreign, &write, &invalid, limits()).await,
        Err(Error::Data(KnowledgeIndexError::DifferentRequest))
    ));
    assert_no_birth(&f);
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        publish_native_documents(ctx.snapshot(), &write, &invalid, limits()).await,
        Err(Error::Data(KnowledgeIndexError::Auth(AuthError::Denied)))
    ));
    assert_no_birth(&f);
}

#[tokio::test]
async fn cancelled_original_precedes_invalid_mapping_with_zero_birth() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let reader = capture(&f, &ctx, "cancelled.md", "# Documento");
    let compiled = compile(&reader, &ctx);
    cancel.cancel();
    let Error::Data(KnowledgeIndexError::Request(error)) =
        publish_native_documents(ctx.snapshot(), &write, &[input(&compiled, "")], limits())
            .await
            .unwrap_err()
    else {
        panic!("cancelled original request required")
    };
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    assert_no_birth(&f);
}

#[tokio::test]
async fn expired_original_precedes_invalid_mapping_with_zero_birth() {
    let cancel = Cancellation::default();
    let ctx = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        },
        &cancel,
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let reader = capture(&f, &ctx, "expired.md", "# Documento");
    let compiled = compile(&reader, &ctx);
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        ctx.deadline().expect("finite fixture deadline"),
    ))
    .await;
    let Error::Data(KnowledgeIndexError::Request(error)) =
        publish_native_documents(ctx.snapshot(), &write, &[input(&compiled, "")], limits())
            .await
            .unwrap_err()
    else {
        panic!("expired original request required")
    };
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_no_birth(&f);
}

#[tokio::test]
async fn complete_preparation_and_each_producer_cap_precede_birth() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let (bearer, reference) = setup_grant(&f, &issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let reader = capture(&f, &ctx, "bounded.md", "# Página\n\nTexto.");
    let compiled = compile(&reader, &ctx);
    let first = input(&compiled, "a");
    let inputs = [first, input(&compiled, "b")];
    let error = publish_native_documents(
        ctx.snapshot(),
        &write,
        &[first, input(&compiled, "")],
        limits(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        Error::Mapping(NativeAuthoredMappingError::InvalidOrigin { document: 1 })
    ));
    assert_no_birth(&f);
    let mut caps = limits();
    caps.projection.max_documents = n(1);
    assert!(matches!(
        publish_native_documents(ctx.snapshot(), &write, &inputs, caps).await,
        Err(Error::Projection(
            h::KnowledgeDocumentProjectionError::LimitExceeded {
                kind: h::ProjectionLimit::Documents,
            }
        ))
    ));
    assert_no_birth(&f);
    caps = limits();
    caps.index.content.max_identifier_bytes = n(1);
    assert!(matches!(
        publish_native_documents(
            ctx.snapshot(),
            &write,
            &[input(&compiled, "aa"), input(&compiled, "bb")],
            caps,
        )
        .await,
        Err(Error::Data(KnowledgeIndexError::LimitExceeded(
            "identifier_bytes"
        )))
    ));
    assert_no_birth(&f);
    caps = limits();
    caps.projection.max_documents = n(2);
    publish_native_documents(ctx.snapshot(), &write, &inputs, caps)
        .await
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let KnowledgeIndexRead::Published { documents, .. } =
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("boundary count must publish")
    };
    assert_eq!(documents.len(), 2);
}
