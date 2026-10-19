//! Authenticated, corpus-local retirement with real native SDK effects.
use arrow_array::Array as _;
use futures::TryStreamExt;
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
use graphit_data::knowledge_embeddings::*;
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
    paths: ProjectPaths,
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
        self.issue_permissions(issuer, parent, version, &permissions())
    }
    fn issue_permissions(
        &self,
        issuer: &RoleIssuer,
        parent: &AdminParent<'_>,
        version: &VersionContext,
        perms: &[Permission],
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
        let definition = RoleDefinition::new(
            Role::Coordinator,
            &source,
            NativeBinding::KiroV3,
            perms,
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
            .issue(parent, &run, &reference, perms, &self.scope_for(version))
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

fn retirement_limits() -> KnowledgeRetirementLimits {
    let w = write_limits();
    KnowledgeRetirementLimits {
        native: w.native,
        artifacts: w.artifacts,
    }
}
fn native_test(future: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(future)
        })
        .unwrap()
        .join()
        .unwrap();
}
macro_rules! setup {
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$write:ident, $grant:ident, $bearer:ident, $reference:ident) => {
        let $issuer = $f.issuer(&$ctx);
        let caller = $issuer.authenticate_runtime(&$f.secret(), &$ctx).unwrap();
        let admin = $issuer.bootstrap_admin(&caller).unwrap();
        let parent = $issuer.admin_parent(&caller, &admin).unwrap();
        let ($bearer, $reference) = $f.issue(&$issuer, &parent);
        let $grant = $issuer
            .verify(
                $bearer.expose_for_transport(),
                &$reference,
                $f.scope(),
                &$ctx,
            )
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $write = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::Knowledge)
            .unwrap();
    };
}
async fn publish(
    ctx: &RequestContext<'_>,
    write: &ProjectWriteAdmission<'_>,
) -> KnowledgeFirstPublication {
    let docs = [page("literal-default", &[])];
    let prepared = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    publish_native_authored(ctx.snapshot(), write, &prepared, write_limits())
        .await
        .unwrap()
}
async fn publish_semantic(
    ctx: &RequestContext<'_>,
    write: &ProjectWriteAdmission<'_>,
    read: &ProjectReadAdmission<'_>,
) -> KnowledgeGeneration {
    let KnowledgeEmbeddingRead::Published(capture) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), read, read_limits())
            .await
            .unwrap()
    else {
        panic!("published index required only for fixture preparation");
    };
    while capture
        .next_documents(ctx.snapshot(), read)
        .await
        .unwrap()
        .is_some()
    {}
    let identity = KnowledgeEmbeddingIdentity::new(
        [7; 32],
        NonZeroU32::new(2).unwrap(),
        "knowledge-document-json-v1".into(),
    )
    .unwrap();
    let rows = [KnowledgeEmbeddingRow {
        input: KnowledgeEmbeddingInput {
            slug: "literal-default",
            input_fingerprint: [1; 32],
        },
        vector: &[1.0, 0.0],
    }];
    let KnowledgeEmbeddingPublication::Committed { generation, .. } = publish_knowledge_embeddings(
        ctx.snapshot(),
        write,
        &capture,
        &identity,
        &rows,
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("first semantic publication");
    };
    let published = KnowledgeGeneration {
        corpus_uri: capture.source_generation().corpus_uri.clone(),
        version: generation.version,
        physical_branch: generation.physical_branch,
        inherited_target: generation.inherited_target,
    };
    let (observed, batches) =
        image(&published.corpus_uri, published.physical_branch.as_deref()).await;
    assert_eq!(observed, published.version);
    let mut stored = Vec::new();
    for batch in batches {
        let vectors = batch
            .column_by_name("vector")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow_array::FixedSizeListArray>()
            .unwrap();
        for row in 0..vectors.len() {
            if vectors.is_valid(row) {
                let values = vectors.value(row);
                let values = values
                    .as_any()
                    .downcast_ref::<arrow_array::Float32Array>()
                    .unwrap();
                stored.push(values.values().to_vec());
            }
        }
    }
    assert_eq!(stored, vec![vec![1.0, 0.0]]);
    published
}
async fn branches(uri: &str) -> graphit_data::lance_branch_store::LanceBranchStore {
    let sdk = lance::dataset::builder::DatasetBuilder::from_uri(uri)
        .load()
        .await
        .unwrap();
    graphit_data::lance_branch_store::LanceBranchStore::open(
        uri,
        Arc::new(arrow_schema::Schema::from(sdk.schema())),
        None,
    )
    .await
    .unwrap()
}
async fn image(uri: &str, branch: Option<&str>) -> (u64, Vec<arrow_array::RecordBatch>) {
    let sdk = lance::dataset::builder::DatasetBuilder::from_uri(uri)
        .load()
        .await
        .unwrap();
    let sdk = if let Some(branch) = branch {
        sdk.checkout_branch(branch).await.unwrap()
    } else {
        sdk
    };
    let version = sdk.version_id();
    let scan = sdk.scan();
    let batches = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    (version, batches)
}
fn io_kind(error: &(dyn std::error::Error + 'static)) -> Option<std::io::ErrorKind> {
    if let Some(io) = error.downcast_ref::<std::io::Error>() {
        return Some(io.kind());
    }
    if let Some(artifact) = error.downcast_ref::<graphit_core::artifact_store::ArtifactError>() {
        return Some(artifact.kind());
    }
    error.source().and_then(io_kind)
}

#[test]
fn index_retirement_named_retirement_preserves_default_sibling_and_other_corpus() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, "A");
        setup!(f, ctx, issuer, authority, write, grant, bearer, reference);
        let first = publish(&ctx, &write).await;
        let uri = &first.default.corpus_uri;
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
            .unwrap();
        let semantic = publish_semantic(&ctx, &write, &read).await;
        assert_eq!(semantic.corpus_uri, *uri);
        assert_eq!(
            semantic.physical_branch.as_deref(),
            Some(f.version.lance_branch().as_str())
        );
        assert!(
            !f.project
                .runtime_root()
                .join("knowledge/embeddings.lance")
                .exists()
        );
        let b = VersionContext::select(&f.paths, Checkout::NonGit, "B", None, "")
            .unwrap()
            .lance_branch();
        branches(uri)
            .await
            .capture_target(Some(&b))
            .await
            .unwrap()
            .materialize()
            .await
            .unwrap();
        let default = image(uri, None).await;
        let sibling = image(uri, Some(&b)).await;
        let other = f.project.runtime_root().join("memory/index.lance");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("untouched"), b"other corpus sentinel").unwrap();
        let result = retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
            .await
            .unwrap();
        assert_eq!(
            result,
            KnowledgeBranchRetirement::Retired {
                generation: semantic
            }
        );
        assert_eq!(image(uri, None).await, default);
        assert_eq!(image(uri, Some(&b)).await, sibling);
        assert_eq!(
            std::fs::read(other.join("untouched")).unwrap(),
            b"other corpus sentinel"
        );
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
            .unwrap();
        let KnowledgeIndexRead::Published {
            generation,
            documents,
        } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
        else {
            panic!("existing corpus");
        };
        assert_eq!(generation.inherited_target, Some(f.version.lance_branch()));
        assert_eq!(documents[0].slug, "literal-default");
        let e = retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
            .await
            .unwrap_err();
        assert!(matches!(
            e.error(),
            KnowledgeIndexError::Branch(
                graphit_data::lance_branch_store::BranchError::InheritedSnapshot
            )
        ));
        assert!(e.confirmed().is_none());
    });
}

#[test]
fn index_retirement_absence_is_exact_and_never_creates_corpus() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, "A");
        setup!(f, ctx, issuer, authority, write, grant, bearer, reference);
        let knowledge = f.project.runtime_root().join("knowledge");
        assert_eq!(
            retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
                .await
                .unwrap(),
            KnowledgeBranchRetirement::NoCorpus
        );
        assert!(!knowledge.exists());
        publish(&ctx, &write).await;
        std::fs::remove_dir_all(knowledge.join("index.lance")).unwrap();
        std::fs::write(knowledge.join("index.lance"), b"not a directory").unwrap();
        let e = retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
            .await
            .unwrap_err();
        assert!(e.confirmed().is_none());
        assert_eq!(
            std::fs::read(knowledge.join("index.lance")).unwrap(),
            b"not a directory"
        );
        std::fs::remove_file(knowledge.join("index.lance")).unwrap();
        std::fs::create_dir(knowledge.join("index.lance")).unwrap();
        let e = retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
            .await
            .unwrap_err();
        assert!(matches!(e.error(), KnowledgeIndexError::Branch(_)));
        assert!(e.confirmed().is_none());
    });
}

#[test]
fn index_retirement_authority_original_default_and_initial_cancel_guard_effects() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        setup!(f, ctx, issuer, authority, write, grant, bearer, reference);
        let first = publish(&ctx, &write).await;
        let before = image(&first.default.corpus_uri, None).await;
        let e = retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
            .await
            .unwrap_err();
        assert!(matches!(
            e.error(),
            KnowledgeIndexError::Branch(
                graphit_data::lance_branch_store::BranchError::DefaultSnapshot
            )
        ));
        let foreign = Arc::new(SnapshotContext::new(
            ctx.snapshot().required_deadline().unwrap(),
            cancel.clone(),
        ));
        let e = retire_native_knowledge_index(&foreign, &write, retirement_limits())
            .await
            .unwrap_err();
        assert!(matches!(e.error(), KnowledgeIndexError::DifferentRequest));
        let task = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
            .unwrap();
        let e = retire_native_knowledge_index(ctx.snapshot(), &task, retirement_limits())
            .await
            .unwrap_err();
        assert!(matches!(e.error(), KnowledgeIndexError::WrongAdmission));
        let other =
            VersionContext::select(&f.paths, Checkout::NonGit, "foreign", None, "").unwrap();
        assert!(
            issuer
                .verify(
                    bearer.expose_for_transport(),
                    &reference,
                    f.scope_for(&other),
                    &ctx
                )
                .is_err()
        );
        cancel.cancel();
        let e = retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
            .await
            .unwrap_err();
        assert_eq!(io_kind(&e), Some(std::io::ErrorKind::Interrupted));
        assert!(e.confirmed().is_none());
        assert_eq!(image(&first.default.corpus_uri, None).await, before);
    });
}

#[test]
fn index_retirement_tag_conflict_keeps_actual_sdk_cause_without_confirmation() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, "A");
        setup!(f, ctx, issuer, authority, write, grant, bearer, reference);
        let first = publish(&ctx, &write).await;
        let sdk = lance::dataset::builder::DatasetBuilder::from_uri(&first.default.corpus_uri)
            .load()
            .await
            .unwrap();
        let branch = f.version.lance_branch();
        let named = first.named.unwrap();
        sdk.tags()
            .create(
                "retained",
                lance::dataset::refs::Ref::Version(Some(branch.clone()), Some(named.version)),
            )
            .await
            .unwrap();
        let before = image(&first.default.corpus_uri, Some(&branch)).await;
        let e = retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
            .await
            .unwrap_err();
        let KnowledgeIndexError::Branch(graphit_data::lance_branch_store::BranchError::Sdk(cause)) =
            e.error()
        else {
            panic!("actual SDK cause");
        };
        assert!(matches!(cause.as_ref(), lance::Error::RefConflict { .. }));
        assert!(e.confirmed().is_none());
        assert_eq!(
            image(&first.default.corpus_uri, Some(&branch)).await,
            before
        );
        assert!(!format!("{e:?} {e}").contains(&first.default.corpus_uri));
    });
}

#[test]
fn index_retirement_native_limits_and_permission_denial_prevent_effects() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, "A");
        setup!(f, ctx, issuer, authority, write, grant, bearer, reference);
        let first = publish(&ctx, &write).await;
        let before = image(&first.default.corpus_uri, Some(&f.version.lance_branch())).await;
        let mut limits = retirement_limits();
        limits.native = NativeDocumentFileLimits::new(65536, 1, 256, 65536).unwrap();
        let e = retire_native_knowledge_index(ctx.snapshot(), &write, limits)
            .await
            .unwrap_err();
        assert_eq!(io_kind(&e), Some(std::io::ErrorKind::FileTooLarge));
        assert!(e.confirmed().is_none());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let admin = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let (read_bearer, read_reference) = f.issue_permissions(
            &issuer,
            &parent,
            &f.version,
            &[Permission::Read(Module::Knowledge)],
        );
        let read_grant = issuer
            .verify(
                read_bearer.expose_for_transport(),
                &read_reference,
                f.scope(),
                &ctx,
            )
            .unwrap();
        assert!(
            authority
                .admit_write(&read_grant, &ctx, ProjectWriteOperation::Knowledge)
                .is_err()
        );
        assert_eq!(
            image(&first.default.corpus_uri, Some(&f.version.lance_branch())).await,
            before
        );
    });
}

#[test]
fn index_retirement_writer_wait_cancellation_and_deadline_preserve_branch_and_release_lock() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, "A");
        setup!(f, ctx, issuer, authority, write, grant, bearer, reference);
        let first = publish(&ctx, &write).await;
        let limits = retirement_limits();
        let store =
            graphit_core::artifact_store::ArtifactStore::new(f.project.clone(), limits.artifacts);
        let holder = store.writer_with_context(ctx.snapshot()).unwrap();
        let future = retire_native_knowledge_index(ctx.snapshot(), &write, limits);
        tokio::pin!(future);
        tokio::select! {
            result = &mut future => panic!("writer must remain held: {result:?}"),
            () = tokio::time::sleep(Duration::from_millis(50)) => cancel.cancel(),
        }
        let e = future.await.unwrap_err();
        assert_eq!(io_kind(&e), Some(std::io::ErrorKind::Interrupted));
        assert!(e.confirmed().is_none());
        let deadline_cancel = Cancellation::default();
        let deadline_ctx = RequestContext::new(
            ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 65536,
                max_scalar_bytes: 4096,
                max_collection_entries: 128,
            },
            &deadline_cancel,
            Instant::now() + Duration::from_millis(500),
        )
        .unwrap();
        let deadline_grant = issuer
            .verify(
                bearer.expose_for_transport(),
                &reference,
                f.scope(),
                &deadline_ctx,
            )
            .unwrap();
        let deadline_write = authority
            .admit_write(
                &deadline_grant,
                &deadline_ctx,
                ProjectWriteOperation::Knowledge,
            )
            .unwrap();
        let e = retire_native_knowledge_index(deadline_ctx.snapshot(), &deadline_write, limits)
            .await
            .unwrap_err();
        assert_eq!(io_kind(&e), Some(std::io::ErrorKind::TimedOut));
        assert!(e.confirmed().is_none());
        let e = retire_native_knowledge_index(deadline_ctx.snapshot(), &deadline_write, limits)
            .await
            .unwrap_err();
        assert_eq!(io_kind(&e), Some(std::io::ErrorKind::TimedOut));
        drop(holder);
        let fresh = Arc::new(SnapshotContext::new(
            Instant::now() + Duration::from_secs(5),
            Cancellation::default(),
        ));
        drop(store.writer_with_context(&fresh).unwrap());
        assert_eq!(
            image(&first.default.corpus_uri, Some(&f.version.lance_branch()))
                .await
                .0,
            first.named.unwrap().version
        );
    });
}

#[test]
fn index_retirement_tag_conflict_preserves_documents_and_vectors_together() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::with_version(&ctx, "A");
        setup!(f, ctx, issuer, authority, write, grant, bearer, reference);
        let first = publish(&ctx, &write).await;
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
            .unwrap();
        let generation = publish_semantic(&ctx, &write, &read).await;
        assert_eq!(generation.corpus_uri, first.default.corpus_uri);
        let branch = f.version.lance_branch();
        let sdk = lance::dataset::builder::DatasetBuilder::from_uri(&generation.corpus_uri)
            .load()
            .await
            .unwrap();
        sdk.tags()
            .create(
                "retained",
                lance::dataset::refs::Ref::Version(Some(branch.clone()), Some(generation.version)),
            )
            .await
            .unwrap();
        drop(sdk);
        let before = image(&generation.corpus_uri, Some(&branch)).await;
        let failure = retire_native_knowledge_index(ctx.snapshot(), &write, retirement_limits())
            .await
            .unwrap_err();
        assert!(matches!(failure.error(), KnowledgeIndexError::Branch(
            graphit_data::lance_branch_store::BranchError::Sdk(cause))
            if matches!(cause.as_ref(), lance::Error::RefConflict { .. })));
        assert!(failure.confirmed().is_none());
        assert_eq!(image(&generation.corpus_uri, Some(&branch)).await, before);
        assert!(
            !f.project
                .runtime_root()
                .join("knowledge/embeddings.lance")
                .exists()
        );
    });
}
