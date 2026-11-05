//! Native Data facade exercised with real runtime-key issuer admissions and SDK corpora.
#[path = "support/knowledge_manifest_fixture.rs"]
mod manifest;

use futures::TryStreamExt;
use graphit_auth::*;
use graphit_auth::{profile::ProfileLimits, store::AuthStore};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, ArtifactStore, CapturedPublicationScope, CopyLimits,
        ProjectRecordDirectory, ScopeBindingRecord, ScopeLimits,
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
fn head_limits() -> KnowledgeHeadReadLimits {
    KnowledgeHeadReadLimits {
        max_slug_bytes: n(256),
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

#[tokio::test]
async fn metadata_heads_keep_literal_dates_and_exact_budgets_without_body_reads() {
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
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let body = "B".repeat(60000);
    let mut first = page("z", &[]);
    first.body = Some(&body);
    first.updated_at = None;
    first.stale_after = None;
    let second = page("á", &[]);
    let docs = [first, second];
    let prepared = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    let published = publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits())
        .await
        .unwrap();
    let exact = KnowledgeHeadReadLimits {
        max_slug_bytes: n(2),
        ..head_limits()
    };
    let expected = KnowledgeHeadRead::Published {
        generation: published.default,
        heads: vec![
            KnowledgeHead {
                slug: "z".into(),
                title: "".into(),
                updated_at: None,
            },
            KnowledgeHead {
                slug: "á".into(),
                title: "".into(),
                updated_at: second.updated_at,
            },
        ],
    };
    assert_eq!(
        read_native_knowledge_heads(ctx.snapshot(), &read, exact)
            .await
            .unwrap(),
        expected
    );
    let KnowledgeIndexRead::Published {
        documents: full, ..
    } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("full corpus");
    };
    let KnowledgeHeadRead::Published { heads, .. } = &expected else {
        panic!("heads");
    };
    assert_eq!(full.len(), heads.len());
    for (limited, expected_error) in [(
        KnowledgeHeadReadLimits {
            max_slug_bytes: n(1),
            ..exact
        },
        "slug_bytes",
    )] {
        assert!(
            matches!(read_native_knowledge_heads(ctx.snapshot(), &read, limited).await,Err(KnowledgeIndexError::LimitExceeded(name)) if name==expected_error)
        );
    }
    assert_eq!(
        read_native_knowledge_heads(ctx.snapshot(), &read, exact)
            .await
            .unwrap(),
        expected
    );
}

#[tokio::test]
async fn metadata_empty_published_preserves_generation_and_present_schema_failure() {
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
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let prepared = h::prepare(ctx.snapshot(), &[], &[], hlimits()).unwrap();
    let published = publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits())
        .await
        .unwrap();
    assert_eq!(
        read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
        KnowledgeHeadRead::Published {
            generation: published.default.clone(),
            heads: vec![]
        }
    );
    // Foreign fixture schema, even when the three requested names exist: open
    // must validate the full corpus schema before exposing metadata.
    std::fs::remove_dir_all(f.project.runtime_root().join("knowledge/index.lance")).unwrap();
    let schema = Arc::new(arrow_schema::Schema::new(vec![
        arrow_schema::Field::new("slug", arrow_schema::DataType::Utf8, false),
        arrow_schema::Field::new("title", arrow_schema::DataType::Utf8, false),
        arrow_schema::Field::new(
            "updated_at",
            arrow_schema::DataType::Timestamp(
                arrow_schema::TimeUnit::Nanosecond,
                Some("UTC".into()),
            ),
            true,
        ),
    ]));
    graphit_data::lance_module_store::LanceModuleStore::create(
        &published.default.corpus_uri,
        schema.clone(),
        vec![arrow_array::RecordBatch::new_empty(schema)],
    )
    .await
    .unwrap();
    let error = read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
        .await
        .unwrap_err();
    assert!(matches!(error, KnowledgeIndexError::InvalidField("schema")));
}

#[tokio::test]
async fn real_admissions_classify_before_payload_then_publish_named_and_read_owned() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "development");
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let docs = [page("z", &["ghost", "z", "ghost", ""]), page("á", &[])];
    let prepared = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    for operation in [ProjectWriteOperation::Memory, ProjectWriteOperation::Task] {
        let admission = authority.admit_write(&grant, &ctx, operation).unwrap();
        assert!(matches!(
            publish_native_authored(ctx.snapshot(), &admission, &prepared, write_limits()).await,
            Err(KnowledgeIndexError::WrongAdmission)
        ));
        assert!(!f.project.runtime_root().join("knowledge").exists());
    }
    for module in [ProjectReadModule::Memory, ProjectReadModule::Task] {
        let admission = authority.admit_read(&grant, &ctx, module).unwrap();
        assert!(matches!(
            read_native_knowledge_index(ctx.snapshot(), &admission, read_limits()).await,
            Err(KnowledgeIndexError::WrongAdmission)
        ));
        assert!(matches!(
            read_native_knowledge_heads(ctx.snapshot(), &admission, head_limits()).await,
            Err(KnowledgeIndexError::WrongAdmission)
        ));
        assert!(!f.project.runtime_root().join("knowledge").exists());
    }
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    assert_eq!(
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        KnowledgeIndexRead::NoCorpus
    );
    assert_eq!(
        read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
        KnowledgeHeadRead::NoCorpus
    );
    let foreign = Arc::new(SnapshotContext::new(
        ctx.deadline().expect("finite fixture deadline"),
        Cancellation::default(),
    ));
    assert!(matches!(
        read_native_knowledge_index(&foreign, &read, read_limits()).await,
        Err(KnowledgeIndexError::DifferentRequest)
    ));
    assert!(matches!(
        read_native_knowledge_heads(&foreign, &read, head_limits()).await,
        Err(KnowledgeIndexError::DifferentRequest)
    ));
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let first = publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits())
        .await
        .unwrap();
    assert_eq!(first.default.version, first.named.as_ref().unwrap().version);
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
        panic!("published");
    };
    assert_eq!(generation, first.named.unwrap());
    assert_eq!(
        documents
            .iter()
            .map(|d| d.slug.as_str())
            .collect::<Vec<_>>(),
        ["z", "á"]
    );
    assert_eq!(
        documents[0].resolved_wiki_targets,
        ["ghost", "z", "ghost", ""]
    );
    assert_eq!(documents[0].body, docs[0].body.unwrap());
    assert_eq!(documents[0].title, "");
    // Every SDK batch is observed through EOF; repeated reading preserves the corpus.
    let KnowledgeIndexRead::Published {
        generation: repeated_generation,
        documents: repeated,
    } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("complete corpus still required");
    };
    assert_eq!(repeated_generation, generation);
    assert_eq!(repeated, documents);
    // A foreign producer can persist duplicate slugs. Validate globally across
    // real SDK batches before returning any owned corpus to the consumer.
    let dataset = lance::dataset::builder::DatasetBuilder::from_uri(&first.default.corpus_uri)
        .load()
        .await
        .unwrap();
    let schema = Arc::new(arrow_schema::Schema::from(dataset.schema()));
    drop(dataset);
    let store =
        graphit_data::lance_module_store::LanceModuleStore::open(&first.default.corpus_uri, schema)
            .await
            .unwrap();
    let snapshot = store.snapshot();
    let mut scanner = snapshot.scan();
    scanner.batch_size(1);
    let mut stream = scanner.try_into_stream().await.unwrap();
    let batch = stream.try_next().await.unwrap().unwrap();
    drop(stream);
    drop(scanner);
    snapshot
        .prepare_replace(vec![batch.clone(), batch])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let default_version = VersionContext::select(&f.paths, Checkout::NonGit, "", None, "").unwrap();
    let (default_bearer, default_reference) = f.issue_for(&issuer, &parent, &default_version);
    let default_grant = issuer
        .verify(
            default_bearer.expose_for_transport(),
            &default_reference,
            f.scope_for(&default_version),
            &ctx,
        )
        .unwrap();
    let default_read = authority
        .admit_read(&default_grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    assert!(matches!(
        read_native_knowledge_index(ctx.snapshot(), &default_read, read_limits()).await,
        Err(KnowledgeIndexError::DuplicateSlug)
    ));
    assert!(matches!(
        read_native_knowledge_heads(ctx.snapshot(), &default_read, head_limits()).await,
        Err(KnowledgeIndexError::DuplicateSlug)
    ));
    let repeated = publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits()).await;
    assert!(
        matches!(repeated, Err(KnowledgeIndexError::InitialTargetChanged)),
        "published Main must refuse a second creation: {repeated:?}"
    );
    cancel.cancel();
    assert!(matches!(
        read_native_knowledge_heads(ctx.snapshot(), &read, head_limits()).await,
        Err(KnowledgeIndexError::Request(_))
    ));
    assert!(matches!(
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits()).await,
        Err(KnowledgeIndexError::Request(_))
    ));
}

#[tokio::test]
async fn absent_main_never_masks_corrupt_dataset_or_invalid_capture() {
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
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let runtime = f.project.runtime_root();
    let knowledge = runtime.join("knowledge");
    let corpus = knowledge.join("index.lance");
    let limits = write_limits();
    let artifacts = ArtifactStore::new(f.project.clone(), limits.artifacts);
    for prefix in [runtime, knowledge.as_path(), corpus.as_path()] {
        assert_eq!(
            read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap(),
            KnowledgeIndexRead::NoCorpus
        );
        assert_eq!(
            read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
                .await
                .unwrap(),
            KnowledgeHeadRead::NoCorpus
        );
        if prefix == runtime {
            // The writer births the private runtime with the platform's policy.
            drop(artifacts.writer_with_context(ctx.snapshot()).unwrap());
        } else if prefix == knowledge {
            let mut writer = artifacts.writer_with_context(ctx.snapshot()).unwrap();
            writer
                .create_record_directory(ProjectRecordDirectory::Knowledge, limits.native)
                .unwrap();
        } else {
            std::fs::create_dir(prefix).unwrap();
            std::fs::set_permissions(prefix, std::fs::metadata(&knowledge).unwrap().permissions())
                .unwrap();
        }
    }
    assert_eq!(
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        KnowledgeIndexRead::NoCorpus
    );
    assert_eq!(
        read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
            .await
            .unwrap(),
        KnowledgeHeadRead::NoCorpus
    );
    assert_eq!(
        read_native_knowledge_dates(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        KnowledgeDateRead::NoCorpus
    );
    assert_eq!(
        read_native_knowledge_page(ctx.snapshot(), &read, "page", read_limits())
            .await
            .unwrap(),
        KnowledgeIndexPageRead::NoCorpus
    );
    // A manifest selected by the SDK but unreadable is corruption, not absence.
    std::fs::remove_dir(&corpus).unwrap();
    let prepared = h::prepare(ctx.snapshot(), &[page("page", &[])], &[], hlimits()).unwrap();
    let published = publish_native_authored(ctx.snapshot(), &write, &prepared, limits)
        .await
        .unwrap();
    assert_eq!(std::path::Path::new(&published.default.corpus_uri), corpus);
    manifest::corrupt_published_manifest(&corpus);
    let error = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap_err();
    assert!(matches!(error, KnowledgeIndexError::Branch(_)));
    let head_error = read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
        .await
        .unwrap_err();
    assert!(matches!(head_error, KnowledgeIndexError::Branch(_)));
    assert!(std::error::Error::source(&head_error).is_some());
    assert!(std::error::Error::source(&error).is_some());
    assert!(
        std::error::Error::source(
            &read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
                .await
                .unwrap_err()
        )
        .is_some()
    );
    // A present non-directory and a replaced captured root are causal errors,
    // never the authoritative absence produced by the four valid probes.
    std::fs::remove_dir_all(&knowledge).unwrap();
    std::fs::write(&knowledge, b"not a directory").unwrap();
    let error = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap_err();
    assert!(std::error::Error::source(&error).is_some());
    assert!(
        std::error::Error::source(
            &read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
                .await
                .unwrap_err()
        )
        .is_some()
    );
    std::fs::remove_file(&knowledge).unwrap();
    assert_eq!(
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        KnowledgeIndexRead::NoCorpus
    );
    std::fs::rename(&f.root, f.root.with_extension("replaced")).unwrap();
    std::fs::create_dir(&f.root).unwrap();
    let error = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap_err();
    assert!(std::error::Error::source(&error).is_some());
    assert!(
        std::error::Error::source(
            &read_native_knowledge_heads(ctx.snapshot(), &read, head_limits())
                .await
                .unwrap_err()
        )
        .is_some()
    );
}

#[tokio::test]
async fn concurrent_initializers_on_one_executor_confirm_one_default_and_refuse_residue() {
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
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let docs = [page("winner", &[])];
    let prepared = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    let (a, b) = tokio::join!(
        publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits()),
        publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits())
    );
    let (winner, loser) = match (a, b) {
        (Ok(w), Err(e)) | (Err(e), Ok(w)) => (w, e),
        other => panic!("one confirmed winner required: {other:?}"),
    };
    assert!(
        matches!(loser, KnowledgeIndexError::InitialTargetChanged),
        "the losing writer must observe the published Main: {loser:?}"
    );
    assert!(std::error::Error::source(&loser).is_none());
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
        panic!("published");
    };
    assert_eq!(generation, winner.default);
    assert_eq!(documents[0].slug, "winner");
}

#[tokio::test]
async fn authenticated_versions_select_distinct_persisted_pins_and_reject_foreign_grants() {
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
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let docs = [page("default-old", &[])];
    let prepared = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    let first = publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits())
        .await
        .unwrap();
    assert!(first.named.is_none());
    assert_eq!(first.default.physical_branch, None);

    let feature =
        VersionContext::select(&f.paths, Checkout::NonGit, "development", None, "").unwrap();
    let other = VersionContext::select(&f.paths, Checkout::NonGit, "other", None, "").unwrap();
    let branch_name = feature.lance_branch();
    assert!(!branch_name.is_empty());
    assert_ne!(branch_name, other.lance_branch());
    assert!(
        issuer
            .verify(
                bearer.expose_for_transport(),
                &reference,
                f.scope_for(&feature),
                &ctx
            )
            .is_err()
    );
    let (feature_bearer, feature_reference) = f.issue_for(&issuer, &parent, &feature);
    let feature_grant = issuer
        .verify(
            feature_bearer.expose_for_transport(),
            &feature_reference,
            f.scope_for(&feature),
            &ctx,
        )
        .unwrap();
    assert!(
        issuer
            .verify(
                feature_bearer.expose_for_transport(),
                &feature_reference,
                f.scope_for(&other),
                &ctx
            )
            .is_err()
    );
    let feature_read = authority
        .admit_read(&feature_grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();

    let dataset = lance::dataset::builder::DatasetBuilder::from_uri(&first.default.corpus_uri)
        .load()
        .await
        .unwrap();
    let schema = Arc::new(arrow_schema::Schema::from(dataset.schema()));
    drop(dataset);
    let branches = graphit_data::lance_branch_store::LanceBranchStore::open(
        &first.default.corpus_uri,
        schema.clone(),
        None,
    )
    .await
    .unwrap();
    let pinned = branches
        .capture_target(Some(&branch_name))
        .await
        .unwrap()
        .materialize()
        .await
        .unwrap();
    let pinned_version = pinned.version();
    let store = graphit_data::lance_module_store::LanceModuleStore::open(
        &first.default.corpus_uri,
        schema.clone(),
    )
    .await
    .unwrap();
    let snapshot = store.snapshot();
    let scanner = snapshot.scan();
    let mut stream = scanner.try_into_stream().await.unwrap();
    let batch = stream.try_next().await.unwrap().unwrap();
    drop(stream);
    drop(scanner);
    let mut columns = batch.columns().to_vec();
    columns[0] = Arc::new(arrow_array::StringArray::from(vec!["default-new"]));
    let changed = arrow_array::RecordBatch::try_new(schema, columns).unwrap();
    snapshot
        .prepare_replace(vec![changed])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();

    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let KnowledgeIndexRead::Published {
        generation: default_generation,
        documents: default_documents,
    } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("default published");
    };
    let KnowledgeIndexRead::Published {
        generation: feature_generation,
        documents: feature_documents,
    } = read_native_knowledge_index(ctx.snapshot(), &feature_read, read_limits())
        .await
        .unwrap()
    else {
        panic!("feature published");
    };
    assert_eq!(default_generation.physical_branch, None);
    assert_eq!(
        feature_generation.physical_branch.as_deref(),
        Some(branch_name.as_str())
    );
    assert_eq!(feature_generation.inherited_target, None);
    assert_eq!(feature_generation.version, pinned_version);
    assert_ne!(default_generation.version, feature_generation.version);
    assert_eq!(default_documents[0].slug, "default-new");
    assert_eq!(feature_documents[0].slug, "default-old");

    let (other_bearer, other_reference) = f.issue_for(&issuer, &parent, &other);
    let other_grant = issuer
        .verify(
            other_bearer.expose_for_transport(),
            &other_reference,
            f.scope_for(&other),
            &ctx,
        )
        .unwrap();
    let other_read = authority
        .admit_read(&other_grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let KnowledgeIndexRead::Published {
        generation: inherited,
        documents,
    } = read_native_knowledge_index(ctx.snapshot(), &other_read, read_limits())
        .await
        .unwrap()
    else {
        panic!("inherited default");
    };
    assert_eq!(inherited.physical_branch, None);
    assert_eq!(
        inherited.inherited_target.as_deref(),
        Some(other.lance_branch().as_str())
    );
    assert_eq!(documents, default_documents);
    for (admission, expected_generation, expected_slug) in [
        (&read, &default_generation, "default-new"),
        (&feature_read, &feature_generation, "default-old"),
        (&other_read, &inherited, "default-new"),
    ] {
        let KnowledgeHeadRead::Published { generation, heads } =
            read_native_knowledge_heads(ctx.snapshot(), admission, head_limits())
                .await
                .unwrap()
        else {
            panic!("metadata published")
        };
        assert_eq!(&generation, expected_generation);
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].slug, expected_slug);
        assert_eq!(heads[0].title, "");
    }
}

#[tokio::test]
async fn revoked_real_admissions_preserve_auth_denial_before_corpus_or_birth() {
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
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let prepared = h::prepare(ctx.snapshot(), &[], &[], hlimits()).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        read_native_knowledge_heads(ctx.snapshot(), &read, head_limits()).await,
        Err(KnowledgeIndexError::Auth(AuthError::Denied))
    ));
    assert!(matches!(
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits()).await,
        Err(KnowledgeIndexError::Auth(AuthError::Denied))
    ));
    assert!(matches!(
        publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits()).await,
        Err(KnowledgeIndexError::Auth(AuthError::Denied))
    ));
    assert!(!f.project.runtime_root().join("knowledge").exists());
}

#[tokio::test]
async fn expired_original_request_never_turns_into_no_corpus_or_new_deadline() {
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
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let prepared = h::prepare(ctx.snapshot(), &[], &[], hlimits()).unwrap();
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        ctx.deadline().expect("finite fixture deadline"),
    ))
    .await;
    assert!(matches!(
        read_native_knowledge_heads(ctx.snapshot(), &read, head_limits()).await,
        Err(KnowledgeIndexError::Request(_))
    ));
    for error in [
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap_err(),
        publish_native_authored(ctx.snapshot(), &write, &prepared, write_limits())
            .await
            .unwrap_err(),
    ] {
        let KnowledgeIndexError::Request(cause) = error else {
            panic!("original timeout required");
        };
        assert_eq!(cause.kind(), std::io::ErrorKind::TimedOut);
    }
    assert!(!f.project.runtime_root().join("knowledge").exists());
}
