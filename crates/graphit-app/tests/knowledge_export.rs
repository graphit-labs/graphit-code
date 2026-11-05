#![cfg(any(target_os = "linux", target_os = "macos", windows))]
//! Real Auth/Data/Core composition; no replacement admissions, corpus DTOs or journals.
use graphit_app::knowledge_export::*;
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy,
        RetainedBytesLimits, RetainedFileSet, ScopeBindingRecord, ScopeLimits, SnapshotPolicy,
        SourceReadLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_fs::NodeKind;
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use graphit_knowledge::{
    compilation_fingerprint::CompilationFingerprintLimits,
    document::DocumentParseLimits,
    native_document_mapping::{AuthoredDocumentInput, NativeAuthoredMappingLimits},
    native_index_publication::{NativeDocumentPublicationLimits, publish_native_documents},
    publication_metadata::{
        CompiledDocument, PublicationMetadataLimits, PublicationMetadataPolicy,
        compile_captured_document,
    },
    references::ReferenceLimits,
};
use sha2::{Digest, Sha256};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
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
    fn issuer(&self, ctx: &RequestContext<'_>, perms: &[Permission]) -> RoleIssuer {
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
        let source = SourceAuthorityPolicy {
            ordinary: perms,
            administrative: perms,
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

fn permissions() -> Vec<Permission> {
    vec![
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
        Permission::External(ExternalEffect::PublishedFilesystem),
    ]
}
fn policy(f: &Fixture) -> KnowledgeTextExportPolicy {
    let (modes, markdown) = markdown_parameters(f);
    KnowledgeTextExportPolicy {
        index: read_limits(),
        text: markdown,
        modes,
    }
}
fn published(result: KnowledgeTextExportOutcome) -> KnowledgeTextPublished {
    match result {
        KnowledgeTextExportOutcome::Published(value) => *value,
        other => panic!("expected published, got {other:?}"),
    }
}
fn page_name(slug: &str) -> String {
    let digest = Sha256::digest(slug.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("p-{hex}.md")
}
async fn publish_fixture(
    f: &Fixture,
    issuer: &RoleIssuer,
    ctx: &RequestContext<'_>,
    inputs: &[(&str, &str)],
) {
    let caller = issuer.authenticate_runtime(&f.secret(), ctx).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let write = authority
        .admit_authenticated_write(&caller, ctx, f.scope(), ProjectWriteOperation::Knowledge)
        .unwrap();
    let readers: Vec<_> = inputs
        .iter()
        .enumerate()
        .map(|(i, (_, body))| capture(f, ctx, &format!("source-{i}.md"), body))
        .collect();
    let compiled: Vec<_> = readers.iter().map(|reader| compile(reader, ctx)).collect();
    let documents: Vec<_> = compiled
        .iter()
        .zip(inputs)
        .map(|(compiled, (slug, _))| input(compiled, slug))
        .collect();
    publish_native_documents(ctx.snapshot(), &write, &documents, limits())
        .await
        .unwrap();
}

const A: &str = "---\ntitle: Página Á\ntype: guide\ndescription: integral description survives truncated preview\n---\n# A\n\n[[z]]\n";
const Z: &str = "\u{feff}---\r\ntitle: Z\r\ntype: custom\r\ndescription: ''\r\n---\r\n# Literal\r\n\r\nOlá 世界 [[a]] [[ghost|Ausente]]\r\n";

#[tokio::test]
async fn full18_file_backed_export_to_external_fresh_directory() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    publish_fixture(&f, &issuer, &ctx, &[("z", Z), ("a", A)]).await;
    // The exporter needs no MutateKnowledge permission after the producer fixture.
    let issuer = f.issuer(
        &ctx,
        &[
            Permission::Read(Module::Knowledge),
            Permission::External(ExternalEffect::PublishedFilesystem),
        ],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let p = policy(&f);
    let destination = f._dir.path().join("external-export");
    assert!(!destination.starts_with(&f.root));
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
        .unwrap();
    let KnowledgeIndexRead::Published {
        generation: expected_generation,
        documents,
    } = read_native_knowledge_index(ctx.snapshot(), &read, p.index)
        .await
        .unwrap()
    else {
        panic!("published corpus required")
    };
    let expected_plan = {
        let mut stage = graphit_knowledge::markdown_export::MarkdownExportStage::new_in(
            ctx.snapshot(),
            f._dir.path(),
            p.modes,
            p.text,
            graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
        )
        .unwrap();
        for document in documents {
            stage.push(ctx.snapshot(), document).unwrap();
        }
        stage.finish(ctx.snapshot())
    }
    .unwrap();
    let result = published(
        export_text_authenticated(
            &issuer,
            &caller,
            f.scope(),
            &ctx,
            KnowledgeTextExportRequest {
                destination: &destination,
                format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
            },
            &p,
        )
        .await
        .unwrap(),
    );
    assert_eq!(result.output, destination);
    assert_eq!(result.generation, expected_generation);
    assert_eq!(result.fingerprint, expected_plan.payload.fingerprint());
    assert_eq!(result.communities, expected_plan.communities);
    assert_eq!(result.communities.groups.len(), 1);
    assert_eq!(result.communities.groups[0].members, ["a", "z"]);
    assert_eq!(result.communities.groups[0].label, "Página Á");
    assert_eq!(result.communities.groups[0].cohesion, 1.0);
    for node in expected_plan.payload.nodes() {
        let path = destination.join(node.relative());
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        assert!(
            graphit_core::artifact_store::PayloadMode::from_permissions(&metadata.permissions())
                .unwrap()
                == node.mode(),
            "mode mismatch at {}",
            path.display()
        );
        if let Some(digest) = node.digest() {
            let bytes = std::fs::read(&path).unwrap();
            let actual: [u8; 32] = Sha256::digest(&bytes).into();
            assert_eq!(actual, digest);
            assert_eq!(u64::try_from(bytes.len()).unwrap(), node.bytes());
        } else {
            assert!(metadata.is_dir());
        }
    }

    assert!(result.cleanup.is_ok());
    let page = std::fs::read_to_string(destination.join("pages").join(page_name("z"))).unwrap();
    assert!(page.ends_with("# Literal\r\n\r\nOlá 世界 [[a]] [[ghost|Ausente]]\r\n"));
    let a = std::fs::read_to_string(destination.join("pages").join(page_name("a"))).unwrap();
    assert!(a.contains("integral description survives truncated preview"));
    assert!(
        std::fs::read_to_string(destination.join("index.md"))
            .unwrap()
            .contains("Página Á")
    );
    assert!(!page.contains(&result.generation.corpus_uri));
    assert!(!a.contains(&result.generation.corpus_uri));
    let private = expected_plan.directory().parent().unwrap().to_owned();
    assert!(private.starts_with(f._dir.path()));
    assert!(expected_plan.close().1.is_ok());
    assert!(!private.exists());
}

#[tokio::test]
async fn first_source_communities_remain_owned_after_request() {
    let report = {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, &permissions());
        publish_fixture(
            &f,
            &issuer,
            &ctx,
            &[
                ("a", A),
                ("z", Z),
                (
                    "alone",
                    "---\ntitle: Solitário\ntype: guide\n---\nNo links.\n",
                ),
            ],
        )
        .await;
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let destination = f._dir.path().join("communities");
        let result = published(
            export_text_authenticated(
                &issuer,
                &caller,
                f.scope(),
                &ctx,
                KnowledgeTextExportRequest {
                    destination: &destination,
                    format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
                },
                &policy(&f),
            )
            .await
            .unwrap(),
        );
        assert!(result.cleanup.is_ok());
        assert!(destination.join("pages").join(page_name("alone")).is_file());
        result.communities
    };
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].members, ["a", "z"]);
    assert_eq!(report.groups[0].label, "Página Á");
    assert_eq!(report.groups[0].cohesion, 1.0);
}

#[tokio::test]
async fn absent_and_published_empty_preserve_an_existing_external_destination() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let p = policy(&f);
    let dest = f._dir.path().join("export");
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(dest.join("foreign"), b"preserved").unwrap();
    let entries = || {
        let mut names: Vec<_> = std::fs::read_dir(f._dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        names
    };
    let before = entries();
    assert!(matches!(
        export_text_authenticated(
            &issuer,
            &caller,
            f.scope(),
            &ctx,
            KnowledgeTextExportRequest {
                destination: &dest,
                format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown
            },
            &p
        )
        .await
        .unwrap(),
        KnowledgeTextExportOutcome::NoCorpus
    ));
    publish_fixture(&f, &issuer, &ctx, &[]).await;
    let result = export_text_authenticated(
        &issuer,
        &caller,
        f.scope(),
        &ctx,
        KnowledgeTextExportRequest {
            destination: &dest,
            format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
        },
        &p,
    )
    .await
    .unwrap();
    let KnowledgeTextExportOutcome::PublishedEmpty(generation) = result else {
        panic!("published empty required")
    };
    assert!(generation.version > 0);
    assert_eq!(std::fs::read(dest.join("foreign")).unwrap(), b"preserved");
    assert_eq!(entries(), before);
}

#[tokio::test]
async fn cancel_before_invalid_destination_has_no_private_export_effect() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    cancel.cancel();
    let error = export_text_authenticated(
        &issuer,
        &caller,
        f.scope(),
        &ctx,
        KnowledgeTextExportRequest {
            destination: Path::new("relative"),
            format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
        },
        &policy(&f),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, KnowledgeTextExportError::Request(ref cause) if cause.kind() == std::io::ErrorKind::Interrupted)
    );
}

#[tokio::test]
async fn decoder_and_individual_file_limits_fail_before_publication() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    publish_fixture(&f, &issuer, &ctx, &[("a", A), ("z", Z)]).await;
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let p = policy(&f);
    let dest = f._dir.path().join("export");
    let count = std::fs::read_dir(f._dir.path()).unwrap().count();
    for index in [true, false] {
        let mut limited = p.clone();
        if index {
            limited.index.content.max_identifier_bytes = n(1);
        } else {
            limited.text.max_file_bytes = n(1);
        }
        let error = export_text_authenticated(
            &issuer,
            &caller,
            f.scope(),
            &ctx,
            KnowledgeTextExportRequest {
                destination: &dest,
                format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
            },
            &limited,
        )
        .await
        .unwrap_err();
        assert!(if index {
            matches!(error, KnowledgeTextExportError::Index(_))
        } else {
            matches!(error, KnowledgeTextExportError::Preparation(_))
        });
        assert!(!dest.exists());
        assert_eq!(std::fs::read_dir(f._dir.path()).unwrap().count(), count);
    }
    assert!(
        published(
            export_text_authenticated(
                &issuer,
                &caller,
                f.scope(),
                &ctx,
                KnowledgeTextExportRequest {
                    destination: &dest,
                    format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown
                },
                &p
            )
            .await
            .unwrap()
        )
        .cleanup
        .is_ok()
    );
}

#[tokio::test]
async fn occupied_output_preserves_foreign_bytes_and_reports_already_exists() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    publish_fixture(&f, &issuer, &ctx, &[("a", A)]).await;
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let p = policy(&f);
    for directory in [false, true] {
        let dest = f._dir.path().join(if directory {
            "foreign-dir"
        } else {
            "foreign-file"
        });
        let file = if directory {
            std::fs::create_dir(&dest).unwrap();
            dest.join("foreign")
        } else {
            dest.clone()
        };
        std::fs::write(&file, b"original").unwrap();
        let count = std::fs::read_dir(f._dir.path()).unwrap().count();
        let error = export_text_authenticated(
            &issuer,
            &caller,
            f.scope(),
            &ctx,
            KnowledgeTextExportRequest {
                destination: &dest,
                format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
            },
            &p,
        )
        .await
        .unwrap_err();
        let KnowledgeTextExportError::Publication {
            cause,
            output,
            cleanup,
        } = error
        else {
            panic!("exclusive rename error required, got {error:?}")
        };
        assert_eq!(cause.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(output, dest);
        assert!(cleanup.is_ok());
        assert_eq!(std::fs::read(file).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(f._dir.path()).unwrap().count(), count);
    }
}

#[tokio::test]
async fn missing_or_invalid_parent_is_not_created() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    publish_fixture(&f, &issuer, &ctx, &[("a", A)]).await;
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let p = policy(&f);
    let missing = f._dir.path().join("missing");
    let file = f._dir.path().join("file-parent");
    std::fs::write(&file, b"unchanged").unwrap();
    use std::{error::Error, io::ErrorKind};
    for (dest, expected) in [
        (missing.join("export"), ErrorKind::NotFound),
        (file.join("export"), ErrorKind::NotADirectory),
        (PathBuf::from("relative"), ErrorKind::InvalidInput),
        (f._dir.path().join("../traversal"), ErrorKind::InvalidInput),
    ] {
        let error = export_text_authenticated(
            &issuer,
            &caller,
            f.scope(),
            &ctx,
            KnowledgeTextExportRequest {
                destination: &dest,
                format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
            },
            &p,
        )
        .await
        .unwrap_err();
        let mut cause: Option<&(dyn Error + 'static)> = Some(&error);
        let mut actual = None;
        while let Some(error) = cause {
            if let Some(error) = error.downcast_ref::<std::io::Error>() {
                actual = Some(error.kind());
                break;
            }
            cause = error.source();
        }
        assert_eq!(actual, Some(expected), "{dest:?}: {error:?}");
    }
    assert!(!missing.exists());
    assert_eq!(std::fs::read(file).unwrap(), b"unchanged");
}

#[tokio::test]
async fn both_permissions_and_original_request_lineage_are_required_even_without_corpus() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let p = policy(&f);
    let dest = f.root.join("export");
    for perms in [
        vec![],
        vec![Permission::Read(Module::Knowledge)],
        vec![Permission::External(ExternalEffect::PublishedFilesystem)],
    ] {
        let issuer = f.issuer(&ctx, &perms);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        assert!(matches!(
            export_text_authenticated(
                &issuer,
                &caller,
                f.scope(),
                &ctx,
                KnowledgeTextExportRequest {
                    destination: &dest,
                    format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown
                },
                &p
            )
            .await
            .unwrap_err(),
            KnowledgeTextExportError::Authorization(_)
        ));
    }
    let issuer = f.issuer(&ctx, &permissions());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let other_cancel = Cancellation::default();
    let other = request(&other_cancel);
    assert!(matches!(
        export_text_authenticated(
            &issuer,
            &caller,
            f.scope(),
            &other,
            KnowledgeTextExportRequest {
                destination: &dest,
                format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown
            },
            &p
        )
        .await
        .unwrap_err(),
        KnowledgeTextExportError::Authorization(_)
    ));
    let other_issuer = f.issuer(&ctx, &permissions());
    assert!(matches!(
        export_text_authenticated(
            &other_issuer,
            &caller,
            f.scope(),
            &ctx,
            KnowledgeTextExportRequest {
                destination: &dest,
                format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown
            },
            &p
        )
        .await
        .unwrap_err(),
        KnowledgeTextExportError::Authorization(_)
    ));
    let mut installation_only = f.scope();
    installation_only.project = None;
    assert!(matches!(
        export_text_authenticated(
            &issuer,
            &caller,
            installation_only,
            &ctx,
            KnowledgeTextExportRequest {
                destination: &dest,
                format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown
            },
            &p
        )
        .await
        .unwrap_err(),
        KnowledgeTextExportError::Authorization(_)
    ));
    assert!(!dest.exists());
}

#[tokio::test]
async fn revoked_runtime_identity_prevents_external_publication() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    publish_fixture(&f, &issuer, &ctx, &[("a", A)]).await;
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(f.runtime.remove(ctx.snapshot()).failure().is_none());
    let destination = f._dir.path().join("revoked-export");
    let error = export_text_authenticated(
        &issuer,
        &caller,
        f.scope(),
        &ctx,
        KnowledgeTextExportRequest {
            destination: &destination,
            format: graphit_knowledge::okf_export::KnowledgeTextExportFormat::Markdown,
        },
        &policy(&f),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, KnowledgeTextExportError::Authorization(_)));
    assert!(!destination.exists());
}

fn native_package_reader(
    ctx: &RequestContext<'_>,
    archive: &Path,
    copy: CopyLimits,
) -> CapturedSourceReader {
    let retention = RetainedBytesLimits::new(
        usize::try_from(copy.total_file_bytes_limit()).unwrap(),
        copy.node_limit(),
        copy.total_relative_path_bytes_limit(),
    )
    .unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([], retention).unwrap(),
        retention,
    )
    .unwrap();
    CapturedSourceReader::capture(
        archive,
        NodeKind::File,
        &policy,
        SourceReadLimits::new(65536, 65536, copy.total_file_bytes_limit()).unwrap(),
        Arc::clone(ctx.snapshot()),
    )
    .unwrap()
}

#[tokio::test]
async fn native_knowledge_package_freezes_lexical_and_published_semantic_snapshots() {
    use futures_util::TryStreamExt;
    use graphit_app::knowledge_package_export::*;
    use graphit_data::{
        knowledge_embeddings::*,
        knowledge_snapshot::{KnowledgeSnapshotPage, read_knowledge_snapshot},
        lance_module_store::LanceModuleStore,
    };
    use graphit_hub::native_package::{PackageKind, stage_import};
    use graphit_knowledge::embedding_projection::{
        KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE, prepare_knowledge_embedding_document,
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
        .unwrap();
    let copy = CopyLimits::new(16 * 1024 * 1024, 2048, 262144, 32).unwrap();
    let destination = f.root.join("native-lexical.zip");
    assert!(matches!(
        export_native_knowledge_package(&ctx, &read, &caller, &destination, read_limits())
            .await
            .unwrap(),
        KnowledgePackageExportOutcome::NoCorpus
    ));
    assert!(!destination.exists());
    publish_fixture(&f, &issuer, &ctx, &[("a", A)]).await;
    let KnowledgePackageExportOutcome::Exported {
        generation: lexical,
    } = export_native_knowledge_package(&ctx, &read, &caller, &destination, read_limits())
        .await
        .unwrap()
    else {
        panic!("real lexical package required")
    };
    assert!(lexical.version > 0);
    let archive = native_package_reader(&ctx, &destination, copy);
    let staged = stage_import(
        &archive,
        Some(PackageKind::Knowledge),
        &f.root.join("read-lexical"),
        copy,
        ctx.snapshot(),
    )
    .unwrap();
    assert_eq!(std::fs::read_dir(staged.path()).unwrap().count(), 1);
    let source_uri = staged
        .path()
        .join("index.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let source = LanceModuleStore::open(&source_uri, physical_schema())
        .await
        .unwrap()
        .snapshot();
    assert!(source.version() > 0);
    assert!(
        matches!(read_knowledge_snapshot(ctx.snapshot(), &source, "a", read_limits()).await.unwrap(), KnowledgeSnapshotPage::Found { document, .. } if document.body == A)
    );
    let inside = f
        .project
        .runtime_root()
        .join("knowledge/index.lance/archive.zip");
    assert_eq!(
        export_native_knowledge_package(&ctx, &read, &caller, &inside, read_limits())
            .await
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert!(!inside.exists());
    let denied_issuer = f.issuer(&ctx, &[Permission::Read(Module::Knowledge)]);
    let denied = denied_issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap();
    let denied_authority = ProjectRecordAuthority::new(&denied_issuer);
    let denied_read = denied_authority
        .admit_authenticated_read(&denied, &ctx, f.scope(), ProjectReadModule::Knowledge)
        .unwrap();
    let denied_path = f.root.join("denied.zip");
    assert!(
        export_native_knowledge_package(&ctx, &denied_read, &denied, &denied_path, read_limits())
            .await
            .is_err()
    );
    assert!(!denied_path.exists());
    let other = request(&cancel);
    assert!(
        export_native_knowledge_package(&other, &read, &caller, &denied_path, read_limits())
            .await
            .is_err()
    );
    assert!(!denied_path.exists());
    let KnowledgeEmbeddingRead::Published(capture) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("source capture required")
    };
    let identity = KnowledgeEmbeddingIdentity::new(
        [7; 32],
        NonZeroU32::new(2).unwrap(),
        KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE.into(),
    )
    .unwrap();
    let mut input = None;
    while let Some(documents) = capture.next_documents(ctx.snapshot(), &read).await.unwrap() {
        for document in documents {
            let projection = prepare_knowledge_embedding_document(
                ctx.snapshot(),
                &capture,
                &document,
                &identity,
            )
            .unwrap();
            assert_eq!(projection.slug(), "a");
            input = Some(projection.input_fingerprint());
        }
    }
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
        .unwrap();
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &capture,
        &identity,
        &[KnowledgeEmbeddingRow {
            input: KnowledgeEmbeddingInput {
                slug: "a",
                input_fingerprint: input.unwrap(),
            },
            vector: &[1., 0.],
        }],
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap();
    let semantic_destination = f.root.join("native-semantic.zip");
    let KnowledgePackageExportOutcome::Exported { generation } =
        export_native_knowledge_package(&ctx, &read, &caller, &semantic_destination, read_limits())
            .await
            .unwrap()
    else {
        panic!("semantic package required")
    };
    assert!(generation.version > lexical.version);
    let semantic_archive = native_package_reader(&ctx, &semantic_destination, copy);
    let semantic_stage = stage_import(
        &semantic_archive,
        Some(PackageKind::Knowledge),
        &f.root.join("read-semantic"),
        copy,
        ctx.snapshot(),
    )
    .unwrap();
    assert_eq!(std::fs::read_dir(semantic_stage.path()).unwrap().count(), 1);
    let nn_uri = semantic_stage
        .path()
        .join("index.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let pin =
        graphit_data::lance_branch_store::BranchSnapshot::capture_observed(&nn_uri, None, None)
            .await
            .unwrap();
    assert_eq!(
        native_knowledge_embedding_dimensions(pin.schema())
            .unwrap()
            .unwrap()
            .get(),
        2
    );
    let nn = pin.module_snapshot();
    let scan = nn.scan();
    let mut stream = scan.try_into_stream().await.unwrap();
    let batch = stream.try_next().await.unwrap().unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(
        batch
            .column_by_name("vector_id")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow_array::FixedSizeBinaryArray>()
            .unwrap()
            .value(0),
        [7; 32]
    );
    assert!(stream.try_next().await.unwrap().is_none());
    drop(stream);
    drop(scan);
    let KnowledgeEmbeddingRead::Published(capture) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("current corpus")
    };
    let update_reader = crate::capture(&f, &ctx, "updated-native-a.md", Z);
    let updated = compile(&update_reader, &ctx);
    let graphit_data::knowledge_document_update::KnowledgeDocumentUpdateResult::Updated {
        generation: next,
        ..
    } = graphit_knowledge::document_republication::republish_native_documents(
        ctx.snapshot(),
        &read,
        &write,
        capture.source_generation(),
        &[crate::input(&updated, "a")],
        graphit_knowledge::document_republication::NativeDocumentRepublicationLimits {
            publication: limits(),
            read: read_limits(),
        },
    )
    .await
    .unwrap()
    else {
        panic!("real source update required")
    };
    assert!(next.version > generation.version);
    // Both exported generations remain frozen; a newer Source cannot reuse stale NN.
    assert!(
        matches!(read_knowledge_snapshot(ctx.snapshot(), &source, "a", read_limits()).await.unwrap(), KnowledgeSnapshotPage::Found { document, .. } if document.body == A)
    );
    let pending = f.root.join("pending.zip");
    assert!(matches!(
        export_native_knowledge_package(&ctx, &read, &caller, &pending, read_limits())
            .await
            .unwrap(),
        KnowledgePackageExportOutcome::Exported { .. }
    ));
    assert!(
        pending.exists(),
        "an edited Source with pending vectors is still exportable"
    );
}

#[tokio::test]
async fn native_knowledge_package_empty_corpus_needs_no_model_or_second_dataset() {
    use graphit_app::knowledge_package_export::*;
    use graphit_data::knowledge_embeddings::*;
    use graphit_hub::native_package::{PackageKind, stage_import};
    use graphit_knowledge::embedding_projection::KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, &permissions());
    publish_fixture(&f, &issuer, &ctx, &[]).await;
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
        .unwrap();
    let KnowledgeEmbeddingRead::Published(capture) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("actual empty source required")
    };
    while let Some(documents) = capture.next_documents(ctx.snapshot(), &read).await.unwrap() {
        assert!(documents.is_empty());
    }
    let identity = KnowledgeEmbeddingIdentity::new(
        [9; 32],
        NonZeroU32::new(2).unwrap(),
        KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE.into(),
    )
    .unwrap();
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &capture,
        &identity,
        &[],
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap();
    let copy = CopyLimits::new(16 * 1024 * 1024, 2048, 262144, 32).unwrap();
    let destination = f.root.join("native-empty.zip");
    let KnowledgePackageExportOutcome::Exported { generation } =
        export_native_knowledge_package(&ctx, &read, &caller, &destination, read_limits())
            .await
            .unwrap()
    else {
        panic!("actual empty pair required")
    };
    assert!(generation.version > 0);
    let reader = native_package_reader(&ctx, &destination, copy);
    let stage = stage_import(
        &reader,
        Some(PackageKind::Knowledge),
        &f.root.join("read-empty"),
        copy,
        ctx.snapshot(),
    )
    .unwrap();
    assert!(stage.path().join("index.lance").is_dir());
    assert_eq!(std::fs::read_dir(stage.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn authenticated_okf_and_obsidian_keep_native_metadata_and_literal_bodies() {
    use graphit_knowledge::okf_export::KnowledgeTextExportFormat;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let producer = f.issuer(&ctx, &permissions());
    let authored_a = A.replace("[[z]]", "[[z]] [[z#Literal|Destino explícito]] [[%7A]]");
    let authored_z = Z.replace("[[a]]", "[[a]] [[a#A|Página autora]]");
    publish_fixture(
        &f,
        &producer,
        &ctx,
        &[("z", &authored_z), ("a", &authored_a)],
    )
    .await;
    let issuer = f.issuer(
        &ctx,
        &[
            Permission::Read(Module::Knowledge),
            Permission::External(ExternalEffect::PublishedFilesystem),
        ],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let policy = policy(&f);
    let brand = Brand::new("knowledge-test").unwrap();
    for (name, format) in [
        (
            "okf",
            KnowledgeTextExportFormat::Okf {
                brand: brand.clone(),
            },
        ),
        (
            "obsidian",
            KnowledgeTextExportFormat::Obsidian {
                brand: brand.clone(),
            },
        ),
    ] {
        let destination = f._dir.path().join(name);
        let result = published(
            export_text_authenticated(
                &issuer,
                &caller,
                f.scope(),
                &ctx,
                KnowledgeTextExportRequest {
                    destination: &destination,
                    format,
                },
                &policy,
            )
            .await
            .unwrap(),
        );
        assert!(result.cleanup.is_ok());
        let index = std::fs::read_to_string(destination.join("index.md")).unwrap();
        let (root, index_body) = index
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap();
        let root: serde_json::Value = serde_saphyr::from_str(root).unwrap();
        assert_eq!(root, serde_json::json!({"okf_version":"0.2"}));
        for (slug, body, kind, title, description) in [
            (
                "a",
                "# A\n\n[[z]] [[z#Literal|Destino explícito]] [[%7A]]\n",
                "guide",
                "Página Á",
                "integral description survives truncated preview",
            ),
            (
                "z",
                "# Literal\r\n\r\nOlá 世界 [[a]] [[a#A|Página autora]] [[ghost|Ausente]]\r\n",
                "custom",
                "Z",
                "",
            ),
        ] {
            let relative = index_body
                .split_once(&format!("[{title}]("))
                .unwrap()
                .1
                .split_once(')')
                .unwrap()
                .0;
            assert!(destination.join(relative).is_file());
            assert!(
                index_body.contains(&format!("[{title}]({relative})")),
                "{index_body}"
            );
            let page = std::fs::read_to_string(destination.join(relative)).unwrap();
            let (header, content) = page
                .strip_prefix("---\n")
                .unwrap()
                .split_once("\n---\n")
                .unwrap();
            let header: serde_json::Value = serde_saphyr::from_str(header).unwrap();
            assert_eq!(header["id"], slug);
            assert_eq!(header["type"], kind);
            assert_eq!(header["title"], title);
            assert_eq!(header["description"], description);
            assert_eq!(
                header["generated"]["by"],
                "process:knowledge-test-knowledge-wiki"
            );
            let native = &header["graphit"];
            assert_eq!(native.as_object().unwrap().len(), 15);
            assert_eq!(native["title"], title);
            assert_eq!(native["description"], description);
            assert_eq!(header["sources"][0]["resource"], native["source_path"]);
            if native["updated_at"].is_null() {
                assert!(header["generated"].get("at").is_none());
            } else {
                assert_eq!(header["generated"]["at"], native["updated_at"]);
            }
            assert!(header.get("stale_after").is_none());
            let expected = if name == "obsidian" {
                let target_title = if slug == "a" { "Z" } else { "Página Á" };
                let target = index_body
                    .split_once(&format!("[{target_title}]("))
                    .unwrap()
                    .1
                    .split_once(')')
                    .unwrap()
                    .0;
                assert!(destination.join(target).is_file());
                let target = target.strip_prefix("pages/").unwrap();
                if slug == "a" {
                    format!(
                        "# A\n\n[[{target}|z]] [[{target}#Literal|Destino explícito]] [[{target}|%7A]]\n"
                    )
                } else {
                    format!(
                        "# Literal\r\n\r\nOlá 世界 [[{target}|a]] [[{target}#A|Página autora]] [[ghost|Ausente]]\r\n"
                    )
                }
            } else {
                body.to_owned()
            };
            assert!(content.ends_with(&expected), "{content}");
            assert!(content.contains("Content"));
        }
    }
}
