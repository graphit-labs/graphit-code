// Shared genuine Core/Auth/SDK setup; tests keep their own independent corpus.
use arrow_array::RecordBatch;
use futures::TryStreamExt;
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::artifact_store::ArtifactStore;
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
use lance::{
    Dataset,
    dataset::{CommitBuilder, WriteMode, WriteParams, write::InsertBuilder},
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

#[allow(dead_code)]
struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
    paths: ProjectPaths,
}
#[allow(dead_code)]
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
            paths,
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
fn identity(config: u8, dimensions: u32) -> KnowledgeEmbeddingIdentity {
    KnowledgeEmbeddingIdentity::new(
        [config; 32],
        NonZeroU32::new(dimensions).unwrap(),
        "knowledge-document-json-v1".into(),
    )
    .unwrap()
}
fn input<'a>(slug: &'a str, fingerprint: u8) -> KnowledgeEmbeddingInput<'a> {
    KnowledgeEmbeddingInput {
        slug,
        input_fingerprint: [fingerprint; 32],
    }
}
fn row<'a>(slug: &'a str, fingerprint: u8, vector: &'a [f32]) -> KnowledgeEmbeddingRow<'a> {
    KnowledgeEmbeddingRow {
        input: input(slug, fingerprint),
        vector,
    }
}
// This fixture may retain bodies for image/ABI assertions; productive consumers do not.
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

async fn publish_source(
    ctx: &RequestContext<'_>,
    write: &ProjectWriteAdmission<'_>,
    slugs: &[&str],
) -> KnowledgeFirstPublication {
    let pages: Vec<_> = slugs.iter().map(|slug| page(slug, &[])).collect();
    let prepared = h::prepare(ctx.snapshot(), &pages, &[], hlimits()).unwrap();
    publish_native_authored(ctx.snapshot(), write, &prepared, write_limits())
        .await
        .unwrap()
}
fn committed(
    value: KnowledgeEmbeddingPublication,
) -> (KnowledgeEmbeddingGeneration, KnowledgeEmbeddingRecoveryKey) {
    match value {
        KnowledgeEmbeddingPublication::Committed {
            generation,
            recovery,
            ..
        } => (generation, recovery),
        KnowledgeEmbeddingPublication::Unchanged => panic!("expected publication"),
    }
}
async fn sdk_batches(dataset: &Dataset) -> Vec<RecordBatch> {
    dataset
        .scan()
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}

// Independent SDK observation remains possible after the Original is cancelled.
// The documentary corpus must survive every rejected/aborted vector operation.
async fn assert_source_without_vectors(f: &Fixture) {
    let path = f.project.runtime_root().join("knowledge/index.lance");
    assert!(path.is_dir());
    let uri = url::Url::from_file_path(path).unwrap().to_string();
    let dataset = Dataset::open(&uri).await.unwrap();
    let schema = arrow_schema::Schema::from(dataset.schema());
    native_knowledge_embedding_dimensions(&schema).unwrap();
    if schema.field_with_name("vector").is_ok() {
        assert_eq!(
            dataset
                .count_rows(Some("vector IS NOT NULL".into()))
                .await
                .unwrap(),
            0
        );
    }
}

async fn overwrite(dataset: Arc<Dataset>, batches: Vec<RecordBatch>) -> Dataset {
    let transaction = InsertBuilder::new(Arc::clone(&dataset))
        .with_params(&WriteParams {
            mode: WriteMode::Overwrite,
            skip_auto_cleanup: true,
            ..Default::default()
        })
        .execute_uncommitted(batches)
        .await
        .unwrap();
    CommitBuilder::new(dataset)
        .with_max_retries(0)
        .with_skip_auto_cleanup(true)
        .execute(transaction)
        .await
        .unwrap()
}
