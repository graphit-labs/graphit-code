//! Real Auth/Core admissions and FULL16 projection; vectors below qualify storage only.
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
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
use graphit_data::knowledge_embeddings::*;
use graphit_data::{knowledge_document_projection as h, knowledge_index::*};
use graphit_hub::{
    ArtifactType, NativeBinding, ProjectionLimits, SourceOrigin, SourceRecord, SourceRef,
    native::{ContentLimits, Role, RoleCredentialReference},
    policy::RequestContext,
};
use graphit_knowledge::embedding_projection::*;
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
        let version = VersionContext::select(&paths, Checkout::NonGit, "", None, "").unwrap();
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
        ScopeInput {
            installation: self.realm(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version: &self.version,
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
        let source = SourceRef::restore(
            SourceRecord {
                remote_file_origin: None,
                archive_sha256: None,
                origin: SourceOrigin::LocalLink,
                producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                artifact_type: ArtifactType::Agent,
                id: "roles/coordinator-main".into(),
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
            .start_run(parent, &self.scope(), Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(parent, &run, &reference, &perms, &self.scope())
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
fn page<'a>(slug: &'a str, targets: &'a [&'a str]) -> h::AuthoredPageFields<'a> {
    h::AuthoredPageFields {
        slug: Some(slug),
        title: Some(""),
        body: Some("---\ntitle: body literal\n---\ntext\n"),
        kind: Some("page"),
        summary: Some("text"),
        description: None,
        summary_truncated: false,
        source_path: "wiki/document.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([0xff; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(16),
            }),
        },
        updated_at: Some(UNIX_EPOCH - Duration::from_nanos(17)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: targets,
    }
}

macro_rules! setup {
    ($cancel:ident,$ctx:ident,$f:ident,$issuer:ident,$authority:ident,$read:ident,$write:ident,$grant:ident) => {
        let $cancel = Cancellation::default();
        let $ctx = request(&$cancel);
        let $f = Fixture::new(&$ctx);
        let $issuer = $f.issuer(&$ctx);
        let caller = $issuer.authenticate_runtime(&$f.secret(), &$ctx).unwrap();
        let admin = $issuer.bootstrap_admin(&caller).unwrap();
        let parent = $issuer.admin_parent(&caller, &admin).unwrap();
        let (bearer, reference) = $f.issue(&$issuer, &parent);
        let $grant = $issuer
            .verify(bearer.expose_for_transport(), &reference, $f.scope(), &$ctx)
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $read = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Knowledge)
            .unwrap();
        let $write = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::Knowledge)
            .unwrap();
    };
}
// Body retention is confined to this fixture's image/ABI assertions.
struct TestCorpus {
    capture: KnowledgeEmbeddingCapture,
    documents: Vec<KnowledgeDocument>,
}
impl std::ops::Deref for TestCorpus {
    type Target = KnowledgeEmbeddingCapture;
    fn deref(&self) -> &Self::Target {
        &self.capture
    }
}
impl TestCorpus {
    fn documents(&self) -> &[KnowledgeDocument] {
        &self.documents
    }
}
async fn capture(ctx: &RequestContext<'_>, read: &ProjectReadAdmission<'_>) -> TestCorpus {
    let capture = match capture_knowledge_embedding_corpus(ctx.snapshot(), read, read_limits())
        .await
        .unwrap()
    {
        KnowledgeEmbeddingRead::Published(value) => *value,
        KnowledgeEmbeddingRead::NoCorpus => panic!("expected Knowledge corpus"),
    };
    let mut documents = Vec::new();
    while let Some(batch) = capture.next_documents(ctx.snapshot(), read).await.unwrap() {
        assert!(batch.len() <= read_limits().batch_rows.get() as usize);
        documents.extend(batch);
    }
    documents.sort_unstable_by(|a, b| a.slug.cmp(&b.slug));
    TestCorpus { capture, documents }
}

async fn publish_pages(
    ctx: &RequestContext<'_>,
    write: &ProjectWriteAdmission<'_>,
    pages: &[h::AuthoredPageFields<'_>],
) {
    let prepared = h::prepare(ctx.snapshot(), pages, &[], hlimits()).unwrap();
    publish_native_authored(ctx.snapshot(), write, &prepared, write_limits())
        .await
        .unwrap();
}

#[tokio::test]
async fn full_corpus_has_literal_text_fixed_digest_and_no_semantic_effects() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let body = format!(
        "α \"quoted\" {{{{ literal }}}}\r\n\t\u{1}{}",
        "完整\n".repeat(600)
    );
    let mut first = page("alpha", &["beta"]);
    first.title = Some("Título \"α\"");
    first.summary = Some("s\t\u{1}");
    first.body = Some(&body);
    let mut second = page("beta", &[]);
    second.description = Some("");
    publish_pages(&ctx, &write, &[first, second]).await;
    let corpus = capture(&ctx, &read).await;
    assert_eq!(corpus.documents().len(), 2); // batch_rows=1 must read to EOF
    assert!(corpus.semantic_generation().is_none());
    let generation = corpus.source_generation();
    let projection = prepare_knowledge_embedding_projection(
        ctx.snapshot(),
        &corpus,
        corpus.documents(),
        [7; 32],
        2,
    )
    .unwrap();
    assert!(std::ptr::eq(projection.capture(), &corpus.capture));
    let unsupported = KnowledgeEmbeddingIdentity::new(
        [7; 32],
        NonZeroU32::new(2).unwrap(),
        "unregistered-recipe".into(),
    )
    .unwrap();
    assert!(matches!(
        prepare_knowledge_embedding_document(
            ctx.snapshot(),
            &corpus,
            &corpus.documents()[0],
            &unsupported
        ),
        Err(KnowledgeEmbeddingProjectionError::InvalidRecipe)
    ));
    assert_eq!(projection.pending_documents().count(), 2);
    assert_eq!(
        projection
            .current_inputs()
            .map(|input| input.slug)
            .collect::<Vec<_>>(),
        corpus
            .documents()
            .iter()
            .map(|document| document.slug.as_str())
            .collect::<Vec<_>>()
    );
    let alpha = projection
        .documents()
        .iter()
        .find(|d| d.slug() == "alpha")
        .unwrap();
    let expected = format!(
        "{{\"recipe\":\"knowledge-document-json-v1\",\"title\":\"Título \\\"α\\\"\",\"kind\":\"page\",\"description\":null,\"summary\":\"s\\t\\u0001\"}}\n{body}"
    );
    assert_eq!(alpha.text(), expected);
    // Independent Node crypto oracle for ordered JSON ABI, vector_id [7;32], width 2.
    assert_eq!(
        hex::encode(alpha.input_fingerprint()),
        "47ffc660952760b9957cf06323b8c4a17e6311fde3a66b852fa217d365de078f"
    );
    let beta = projection
        .documents()
        .iter()
        .find(|d| d.slug() == "beta")
        .unwrap();
    assert_eq!(
        beta.text(),
        "{\"recipe\":\"knowledge-document-json-v1\",\"title\":\"\",\"kind\":\"page\",\"description\":\"\",\"summary\":\"text\"}\n---\ntitle: body literal\n---\ntext\n"
    );
    assert!(!format!("{projection:?} {alpha:?}").contains("quoted"));
    assert_eq!(
        corpus
            .documents()
            .iter()
            .find(|d| d.slug == "alpha")
            .unwrap()
            .body,
        body
    );
    let recaptured = capture(&ctx, &read).await;
    assert_eq!(recaptured.source_generation(), generation);
    assert!(recaptured.semantic_generation().is_none());
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/embeddings.lance")
            .exists()
    );
}

#[tokio::test]
async fn included_fields_change_inputs_but_external_metadata_does_not() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let base = page("base", &[]);
    let mut empty = page("empty", &[]);
    empty.description = Some("");
    let mut title = page("title", &[]);
    title.title = Some("different");
    let mut kind = page("kind", &[]);
    kind.kind = Some("guide");
    let mut summary = page("summary", &[]);
    summary.summary = Some("different");
    let mut body = page("body", &[]);
    body.body = Some("different\r\n");
    let mut metadata = page("metadata", &["base"]);
    metadata.source_path = "wiki/another.md";
    metadata.compilation_digest = h::CompilationDigest32::from_bytes([3; 32]);
    metadata.updated_at = Some(UNIX_EPOCH + Duration::from_secs(9));
    metadata.stale_since = Some(UNIX_EPOCH + Duration::from_secs(10));
    metadata.stale_after = Some(Duration::from_secs(11));
    publish_pages(
        &ctx,
        &write,
        &[base, empty, title, kind, summary, body, metadata],
    )
    .await;
    let corpus = capture(&ctx, &read).await;
    let projection = prepare_knowledge_embedding_projection(
        ctx.snapshot(),
        &corpus,
        corpus.documents(),
        [7; 32],
        2,
    )
    .unwrap();
    let find = |slug| {
        projection
            .documents()
            .iter()
            .find(|d| d.slug() == slug)
            .unwrap()
    };
    let base = find("base");
    for slug in ["empty", "title", "kind", "summary", "body"] {
        assert_ne!(find(slug).text(), base.text(), "{slug}");
        assert_ne!(
            find(slug).input_fingerprint(),
            base.input_fingerprint(),
            "{slug}"
        );
    }
    assert_eq!(find("metadata").text(), base.text());
    assert_eq!(
        find("metadata").input_fingerprint(),
        base.input_fingerprint()
    );
    for (config, dimensions) in [([8; 32], 2), ([7; 32], 3)] {
        let changed = prepare_knowledge_embedding_projection(
            ctx.snapshot(),
            &corpus,
            corpus.documents(),
            config,
            dimensions,
        )
        .unwrap();
        for (before, after) in projection.documents().iter().zip(changed.documents()) {
            assert_eq!(before.text(), after.text());
            assert_ne!(before.input_fingerprint(), after.input_fingerprint());
        }
    }
}

#[tokio::test]
async fn attached_input_is_skipped_only_for_the_same_identity_and_fingerprint() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_pages(&ctx, &write, &[page("alpha", &[]), page("beta", &[])]).await;
    let corpus = capture(&ctx, &read).await;
    let projection = prepare_knowledge_embedding_projection(
        ctx.snapshot(),
        &corpus,
        corpus.documents(),
        [7; 32],
        2,
    )
    .unwrap();
    let first = &projection.documents()[0];
    let attached_slug = first.slug().to_owned();
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        projection.capture(),
        projection.identity(),
        &[KnowledgeEmbeddingRow {
            input: KnowledgeEmbeddingInput {
                slug: first.slug(),
                input_fingerprint: first.input_fingerprint(),
            },
            vector: &[1.0, 0.0],
        }],
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap();
    drop(projection);
    drop(corpus);
    let corpus = capture(&ctx, &read).await;
    assert!(corpus.semantic_generation().is_some());
    let projection = prepare_knowledge_embedding_projection(
        ctx.snapshot(),
        &corpus,
        corpus.documents(),
        [7; 32],
        2,
    )
    .unwrap();
    assert_eq!(projection.current_inputs().count(), 2);
    assert_eq!(projection.pending_documents().count(), 1);
    assert!(
        !projection
            .documents()
            .iter()
            .find(|d| d.slug() == attached_slug)
            .unwrap()
            .is_pending()
    );
    // A stored input from different text under the same model/recipe/width is pending.
    let pending = projection.pending_documents().next().unwrap();
    let stale_slug = pending.slug().to_owned();
    let mut stale_fingerprint = pending.input_fingerprint();
    stale_fingerprint[0] ^= 1;
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        projection.capture(),
        projection.identity(),
        &[KnowledgeEmbeddingRow {
            input: KnowledgeEmbeddingInput {
                slug: pending.slug(),
                input_fingerprint: stale_fingerprint,
            },
            vector: &[0.0, 1.0],
        }],
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap();
    drop(projection);
    drop(corpus);
    let corpus = capture(&ctx, &read).await;
    let projection = prepare_knowledge_embedding_projection(
        ctx.snapshot(),
        &corpus,
        corpus.documents(),
        [7; 32],
        2,
    )
    .unwrap();
    assert_eq!(projection.current_inputs().count(), 2);
    assert_eq!(
        projection
            .pending_documents()
            .map(|d| d.slug())
            .collect::<Vec<_>>(),
        vec![stale_slug.as_str()]
    );
    assert!(
        !projection
            .documents()
            .iter()
            .find(|d| d.slug() == attached_slug)
            .unwrap()
            .is_pending()
    );
    for (config, dimensions) in [([8; 32], 2), ([7; 32], 3)] {
        let changed = prepare_knowledge_embedding_projection(
            ctx.snapshot(),
            &corpus,
            corpus.documents(),
            config,
            dimensions,
        )
        .unwrap();
        assert_eq!(changed.pending_documents().count(), 2);
    }
    let recaptured = capture(&ctx, &read).await;
    assert_eq!(
        corpus.semantic_generation(),
        recaptured.semantic_generation()
    );
    assert_eq!(recaptured.stored_inputs().count(), 2);
}

#[tokio::test]
async fn invalid_dimensions_cancelled_and_expired_requests_return_typed_errors() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_pages(&ctx, &write, &[page("alpha", &[])]).await;
    let corpus = capture(&ctx, &read).await;
    for dimensions in [
        0,
        u64::from(i32::MAX as u32) + 1,
        u64::from(u32::MAX) + 1,
        u64::MAX,
    ] {
        assert!(matches!(
            prepare_knowledge_embedding_projection(
                ctx.snapshot(),
                &corpus,
                corpus.documents(),
                [7; 32],
                dimensions
            ),
            Err(KnowledgeEmbeddingProjectionError::InvalidDimensions)
        ));
    }
    assert!(corpus.semantic_generation().is_none());
    cancel.cancel();
    let error = prepare_knowledge_embedding_projection(
        ctx.snapshot(),
        &corpus,
        corpus.documents(),
        [7; 32],
        2,
    )
    .unwrap_err();
    assert!(
        matches!(error, KnowledgeEmbeddingProjectionError::Request(ref e) if e.kind() == std::io::ErrorKind::Interrupted)
    );
    let live = Cancellation::default();
    let expired =
        graphit_core::file_io::SnapshotContext::new(Instant::now() - Duration::from_secs(1), live);
    let error =
        prepare_knowledge_embedding_projection(&expired, &corpus, corpus.documents(), [7; 32], 2)
            .unwrap_err();
    assert!(
        matches!(error, KnowledgeEmbeddingProjectionError::Request(ref e) if e.kind() == std::io::ErrorKind::TimedOut)
    );
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/embeddings.lance")
            .exists()
    );
}

#[tokio::test]
async fn admitted_batches_preserve_projection_abi_and_moved_text_for_both_batch_sizes() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let literal = format!("literal\r\nα\t{}", "完整\n".repeat(600));
    let mut a = page("a", &[]);
    a.body = Some(&literal);
    let mut b = page("b", &[]);
    b.description = Some("");
    publish_pages(&ctx, &write, &[a, b]).await;
    let baseline = capture(&ctx, &read).await;
    let projection = prepare_knowledge_embedding_projection(
        ctx.snapshot(),
        &baseline,
        baseline.documents(),
        [7; 32],
        2,
    )
    .unwrap();
    let expected: std::collections::BTreeMap<_, _> = projection
        .documents()
        .iter()
        .map(|doc| {
            (
                doc.slug().to_owned(),
                (
                    doc.text().to_owned(),
                    doc.input_fingerprint(),
                    doc.is_pending(),
                ),
            )
        })
        .collect();
    for size in [1, 2] {
        let mut limits = read_limits();
        limits.batch_rows = NonZeroU32::new(size).unwrap();
        let KnowledgeEmbeddingRead::Published(actual) =
            capture_knowledge_embedding_corpus(ctx.snapshot(), &read, limits)
                .await
                .unwrap()
        else {
            panic!("actual admitted source")
        };
        let mut inputs = std::collections::BTreeMap::new();
        let mut batches = 0;
        while let Some(documents) = actual.next_documents(ctx.snapshot(), &read).await.unwrap() {
            assert!(documents.len() <= size as usize);
            let projected = prepare_knowledge_embedding_projection(
                ctx.snapshot(),
                &actual,
                &documents,
                [7; 32],
                2,
            )
            .unwrap();
            assert!(std::ptr::eq(projected.capture(), &*actual));
            for document in projected.into_documents() {
                let slug = document.slug().to_owned();
                let fingerprint = document.input_fingerprint();
                let pending = document.is_pending();
                let text = document.into_text();
                assert!(inputs.insert(slug, (text, fingerprint, pending)).is_none());
            }
            batches += 1;
        }
        assert_eq!(inputs, expected);
        assert!(batches >= 2 / size);
    }
    assert!(
        !f.project
            .runtime_root()
            .join("knowledge/embeddings.lance")
            .exists()
    );
    // Supplied DTOs remain ordinary data: the helper does not infer pin provenance.
    let mut supplied = baseline.documents().to_vec();
    supplied[0].body.push_str(" supplied change");
    let ordinary =
        prepare_knowledge_embedding_projection(ctx.snapshot(), &baseline, &supplied, [7; 32], 2)
            .unwrap();
    assert_ne!(
        ordinary.documents()[0].input_fingerprint(),
        expected.get(&supplied[0].slug).unwrap().1
    );
}
