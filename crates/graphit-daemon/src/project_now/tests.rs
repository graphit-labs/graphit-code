//! Native consumer tests use public SDK operations and temporary private storage.
use super::*;
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ProjectWriteOperation,
    RoleDefinition, RoleIssuer, ScopeCapture, ServiceAdmissionLimits, ServiceAuthorityInput,
    SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyEffect, RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_data::{
    knowledge_document_projection as h,
    knowledge_index::{
        KnowledgeIndexContentLimits, KnowledgeIndexWriteLimits, publish_native_authored,
    },
};
use graphit_hub::{
    NativeBinding,
    native::{ContentLimits, Role, RoleCredentialReference},
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{CreateMemory, DeleteMemory, MemoryStoreReadLimits, MemoryStoreWriteLimits},
    task_spec::CheckKind,
    task_store::{
        CheckUpdate, ClaimUpdate, CreateCheck, CreateSession, CreateTask, TaskCancellation,
        TaskGeneration, TaskStoreWriteLimits,
    },
};
use secrecy::SecretString;
use std::{
    num::{NonZeroU32, NonZeroU128, NonZeroUsize},
    path::PathBuf,
    time::{Duration, Instant},
};

fn n(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn content() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 1048576,
        max_output_bytes: 1048576,
        max_depth: 64,
        max_nodes: 65536,
        max_scalar_bytes: 65536,
        max_collection_entries: 65536,
    }
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(content(), cancel, Instant::now() + Duration::from_secs(120)).unwrap()
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(1048576, 65536, 256, 8 * 1024 * 1024).unwrap()
}
fn limits() -> ProjectNowLimits {
    ProjectNowLimits {
        task: TaskStoreReadLimits {
            native: native(),
            batch_rows: NonZeroU32::new(1).unwrap(),
        },
        memory: MemoryHeadReadLimits {
            batch_rows: NonZeroU32::new(1).unwrap(),
            native: native(),
        },
        knowledge: KnowledgeHeadReadLimits {
            max_slug_bytes: n(256),
            batch_rows: NonZeroU32::new(1).unwrap(),
            native: native(),
        },
    }
}
fn artifacts() -> ArtifactLimits {
    let copy = CopyLimits::new(1048576, 128, 65536, 16).unwrap();
    ArtifactLimits::new(1048576, 65536, copy).unwrap()
}
fn task_write() -> TaskStoreWriteLimits {
    TaskStoreWriteLimits {
        read: limits().task,
        artifacts: artifacts(),
        references: graphit_records::task_store::TaskReferencesInputLimits {
            max_entries: n(content().max_collection_entries),
            max_text_bytes: n(content().max_input_bytes),
        },
    }
}
fn memory_write() -> MemoryStoreWriteLimits {
    MemoryStoreWriteLimits {
        read: MemoryStoreReadLimits {
            max_query_bytes: n(4 * 1024 * 1024),
            batch_rows: NonZeroU32::new(1).unwrap(),
            native: native(),
        },
        artifacts: artifacts(),
        max_input_bytes: n(1048576),
        max_row_bytes: n(2 * 1048576),
    }
}
fn knowledge_write() -> KnowledgeIndexWriteLimits {
    KnowledgeIndexWriteLimits {
        content: KnowledgeIndexContentLimits {
            max_identifier_bytes: n(256),
        },
        native: native(),
        artifacts: artifacts(),
    }
}
fn reads_and_writes() -> Vec<Permission> {
    vec![
        Permission::Read(Module::Task),
        Permission::Read(Module::Memory),
        Permission::Read(Module::Knowledge),
        Permission::WriteTask,
        Permission::WriteOwnTask,
        Permission::CoordinateTask,
        Permission::WriteMemory,
        Permission::MutateModule(Module::Knowledge),
    ]
}

struct Fixture {
    issuer: RoleIssuer,
    store: Arc<AuthStore>,
    installation_paths: InstallationPaths,
    peer_root: PathBuf,
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
    paths: ProjectPaths,
    root: PathBuf,
    _directory: tempfile::TempDir,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>, permissions: &[Permission]) -> Self {
        Self::with_administrative(ctx, permissions, &[])
    }
    fn with_administrative(
        ctx: &RequestContext<'_>,
        permissions: &[Permission],
        administrative: &[Permission],
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        let peer_root = directory.path().join("peer-project");
        let installation_root = directory.path().join("installation");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&peer_root).unwrap();
        let install = InstallationPaths::resolve(
            Brand::new("now-test").unwrap(),
            &root,
            None,
            Some(&installation_root),
        )
        .unwrap();
        let private = installation_bootstrap::prepare(
            &install,
            BootstrapLimits::new(65536, 256).unwrap(),
            ctx.snapshot(),
        )
        .into_installation()
        .unwrap();
        let identity = InstallationIdentityStore::new(&install)
            .unwrap()
            .ensure(Duration::from_secs(5), ctx.snapshot().cancellation())
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
        let project_identity = ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), ctx.snapshot().cancellation())
            .unwrap()
            .ensure(&ProjectMetadata::new("now-fixture").unwrap())
            .unwrap();
        let scope_limits = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            identity.unit_id().clone(),
            [install.root().to_owned(), root.clone(), peer_root.clone()],
            scope_limits,
            ctx.snapshot(),
        )
        .unwrap();
        let installation_record = installation.binding().unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            project_identity.id(),
            scope_limits,
            ctx.snapshot(),
        )
        .unwrap();
        let project_record = project.binding().unwrap();
        let version = VersionContext::select(&paths, Checkout::NonGit, "main", None, "").unwrap();
        let store_scope = CapturedPublicationScope::installation(
            &install,
            identity.unit_id().clone(),
            [install.root().to_owned(), root.clone(), peer_root.clone()],
            scope_limits,
            ctx.snapshot(),
        )
        .unwrap();
        let profiles = ProfileLimits {
            bytes: 65536,
            nodes: 65536,
            scalar_bytes: 65536,
            collection_entries: 128,
            providers: 16,
            profiles: 16,
        };
        let store = Arc::new(
            AuthStore::new(
                AuthDocumentStore::capture(
                    store_scope,
                    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
                    ctx.snapshot(),
                )
                .unwrap(),
                profiles,
                ctx,
            )
            .unwrap(),
        );
        let source = || SourceAuthorityPolicy {
            ordinary: permissions,
            administrative,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = IssuerAuthorityPolicy::new(
            source(),
            source(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 4096,
            },
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap();
        let issuer = RoleIssuer::new(
            ScopeCapture {
                live: &installation,
                record: &installation_record,
            },
            runtime.clone(),
            store.clone(),
            policy,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap();
        Self {
            issuer,
            store,
            installation_paths: install,
            peer_root,
            runtime,
            installation,
            installation_record,
            project,
            project_record,
            version,
            paths,
            root,
            _directory: directory,
        }
    }
    fn publish_local(&self, ctx: &RequestContext<'_>) {
        let observed = self.store.load(ctx).unwrap();
        let result = self
            .store
            .mutate(observed, ctx, |doc, ctx| {
                doc.add_provider(
                    "local".into(),
                    ProviderConfig {
                        kind: ProviderKind::Local {
                            allow_aws_credential_chain: false,
                            broker: None,
                        },
                        embedding: AiService::Disabled,
                        rerank: AiService::Disabled,
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: S3Source::LocalCredentials,
                        },
                    },
                    doc.revision(),
                    1,
                    ctx,
                )?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("local").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(
                    ProfileData {
                        name: "temporary-local".into(),
                        provider: "local".into(),
                        username: "native-fixture".into(),
                        organization: None,
                        teams: vec![],
                        credentials: ProfileCredentials::Local(LocalCredentials {
                            anonymous: false,
                            mcp_key: Some(Secret::new(SecretString::from(
                                "temporary-native-local-key",
                            ))),
                            broker_key: None,
                            embedding_api_key: None,
                            rerank_api_key: None,
                            s3: None,
                        }),
                    },
                    expected,
                    1,
                    ctx,
                )?;
                doc.activate("temporary-local", doc.revision(), ctx)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
        assert!(result.failure().is_none());
    }
    fn scope(&self) -> ScopeInput<'_> {
        self.scope_for(&self.version)
    }
    fn scope_for<'a>(&'a self, version: &'a VersionContext) -> ScopeInput<'a> {
        ScopeInput {
            installation: ScopeCapture {
                live: &self.installation,
                record: &self.installation_record,
            },
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
    fn secret(&self, ctx: &RequestContext<'_>) -> String {
        String::from_utf8(
            self.runtime
                .check(ctx.snapshot())
                .unwrap()
                .secret(ctx.snapshot())
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }
    fn remove(&self, ctx: &RequestContext<'_>) {
        let outcome = self.runtime.remove(ctx.snapshot());
        assert_eq!(outcome.effects().key, RuntimeKeyEffect::Applied);
        assert!(outcome.failure().is_none());
        assert!(outcome.cleanup_failure().is_none());
    }
}
fn session_input(key: &str) -> CreateSession {
    CreateSession {
        idempotency_key: key.into(),
        title: key.into(),
        description: "Native Now fixture demand".into(),
        strategy: "Read actual heads".into(),
    }
}
fn task_input(session_id: SessionId, key: &str) -> CreateTask {
    CreateTask {
        commit: None,
        idempotency_key: key.into(),
        session_id,
        parent_id: None,
        depends_on: vec![],
        title: key.into(),
        description: "Native Now fixture work".into(),
        task_type: "task".into(),
        priority: 2,
        schedule: None,
        checks: vec![
            CreateCheck {
                kind: CheckKind::Acceptance,
                text: "Produce fixture outcome".into(),
            },
            CreateCheck {
                kind: CheckKind::Test,
                text: "Verify fixture outcome".into(),
            },
        ],
    }
}
fn memory_input(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: key.into(),
        body: "PRIVATE_BODY_MUST_NOT_BE_WIRE".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: false,
            important: true,
        },
        tags: vec![],
        references: None,
    }
}
fn page(slug: &str, at: Option<SystemTime>) -> h::AuthoredPageFields<'_> {
    h::AuthoredPageFields {
        slug: Some(slug),
        title: Some(slug),
        body: Some("PRIVATE_KNOWLEDGE_BODY"),
        kind: Some("page"),
        summary: Some("Private summary"),
        description: None,
        summary_truncated: false,
        source_path: "private/wiki.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([1; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "fallback",
                kind_on_absent: "page",
                max_summary_bytes: n(256),
            }),
        },
        updated_at: at,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    }
}
fn prepared<'data, 'context>(
    ctx: &'context RequestContext<'_>,
    pages: &[h::AuthoredPageFields<'data>],
) -> h::PreparedKnowledgeDocumentRows<'data, 'context> {
    h::prepare(
        ctx.snapshot(),
        pages,
        &[],
        h::KnowledgeDocumentProjectionLimits {
            max_documents: n(128),
            max_observations: n(128),
            max_rows: n(1024),
            max_field_bytes: n(65536),
            max_text_bytes: n(4 * 1024 * 1024),
        },
    )
    .unwrap()
}
async fn seed<'a>(
    f: &'a Fixture,
    caller: &'a AuthenticatedCaller<'a>,
    ctx: &'a RequestContext<'a>,
    with_tasks: bool,
) {
    seed_scope(
        &f.issuer,
        caller,
        ctx,
        f.scope(),
        with_tasks,
        "seed-knowledge",
    )
    .await;
}
async fn seed_scope<'a>(
    issuer: &'a RoleIssuer,
    caller: &'a AuthenticatedCaller<'a>,
    ctx: &'a RequestContext<'a>,
    scope: ScopeInput<'a>,
    with_tasks: bool,
    knowledge_slug: &str,
) {
    let authority = ProjectRecordAuthority::new(issuer);
    if with_tasks {
        let write = authority
            .admit_authenticated_write(caller, ctx, scope, ProjectWriteOperation::Task)
            .unwrap();
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("seed-session"),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            task_write(),
        )
        .await
        .unwrap();
        let task = NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, "seed-task"),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            task_write(),
        )
        .await
        .unwrap();
        NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            Duration::from_secs(90),
            task_write(),
        )
        .await
        .unwrap();
        let coordinate = authority
            .admit_authenticated_write(caller, ctx, scope, ProjectWriteOperation::CoordinateTask)
            .unwrap();
        NativeTaskStore::claim_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            Duration::from_secs(90),
            task_write(),
        )
        .await
        .unwrap();
    }
    let memory = authority
        .admit_authenticated_write(caller, ctx, scope, ProjectWriteOperation::Memory)
        .unwrap();
    NativeMemoryStore::create(
        ctx.snapshot(),
        &memory,
        memory_input("seed-memory"),
        memory_write(),
    )
    .await
    .unwrap();
    let knowledge = authority
        .admit_authenticated_write(caller, ctx, scope, ProjectWriteOperation::Knowledge)
        .unwrap();
    let pages = [page(knowledge_slug, Some(UNIX_EPOCH))];
    publish_native_authored(
        ctx.snapshot(),
        &knowledge,
        &prepared(ctx, &pages),
        knowledge_write(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn native_two_projects_share_caller_without_sharing_any_section() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, &reads_and_writes());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    seed(&f, &caller, &ctx, true).await;
    let peer_paths = ProjectPaths::at(&f.peer_root, &f.installation_paths).unwrap();
    let peer_identity = ProjectIdentityStore::at(&peer_paths)
        .writer(Duration::from_secs(5), ctx.snapshot().cancellation())
        .unwrap()
        .ensure(&ProjectMetadata::new("peer-project").unwrap())
        .unwrap();
    let peer = CapturedPublicationScope::project(
        &peer_paths,
        peer_identity.id(),
        ScopeLimits::new(16, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let peer_record = peer.binding().unwrap();
    let peer_version =
        VersionContext::select(&peer_paths, Checkout::NonGit, "main", None, "").unwrap();
    let mut peer_scope = f.scope();
    peer_scope.project = Some(ProjectCapture {
        scope: ScopeCapture {
            live: &peer,
            record: &peer_record,
        },
        version: &peer_version,
    });
    let absent = read_project_now(&authority, &caller, &ctx, peer_scope, limits())
        .await
        .unwrap();
    assert_eq!(absent.project_id(), peer_identity.id());
    empty(absent.sections().tasks(), SectionAvailability::NoCorpus);
    empty(absent.sections().sessions(), SectionAvailability::NoCorpus);
    empty(absent.sections().memories(), SectionAvailability::NoCorpus);
    empty(absent.sections().knowledge(), SectionAvailability::NoCorpus);
    seed_scope(&f.issuer, &caller, &ctx, peer_scope, true, "peer-knowledge").await;
    let first = read_project_now(&authority, &caller, &ctx, f.scope(), limits())
        .await
        .unwrap();
    let second = read_project_now(&authority, &caller, &ctx, peer_scope, limits())
        .await
        .unwrap();
    assert_ne!(first.project_id(), second.project_id());
    assert_eq!(second.project_id(), peer_identity.id());
    assert_eq!(first.sections().tasks().total(), Some(1));
    assert_eq!(second.sections().tasks().total(), Some(1));
    assert_ne!(
        first.sections().tasks().items()[0].id,
        second.sections().tasks().items()[0].id
    );
    assert_eq!(
        first.sections().tasks().items()[0].session_id,
        first.sections().sessions().items()[0].id
    );
    assert_eq!(
        second.sections().tasks().items()[0].session_id,
        second.sections().sessions().items()[0].id
    );
    assert_eq!(first.sections().sessions().total(), Some(1));
    assert_eq!(second.sections().sessions().total(), Some(1));
    assert_ne!(
        first.sections().sessions().items()[0].id,
        second.sections().sessions().items()[0].id
    );
    assert_eq!(first.sections().memories().total(), Some(1));
    assert_eq!(second.sections().memories().total(), Some(1));
    assert_ne!(
        first.sections().memories().items()[0].id,
        second.sections().memories().items()[0].id
    );
    assert_eq!(first.sections().knowledge().total(), Some(1));
    assert_eq!(second.sections().knowledge().total(), Some(1));
    assert_eq!(
        first.sections().knowledge().items()[0].slug,
        "seed-knowledge"
    );
    assert_eq!(
        second.sections().knowledge().items()[0].slug,
        "peer-knowledge"
    );
    f.remove(&ctx);
}

#[tokio::test]
async fn native_local_caller_and_incomplete_task_preserve_other_corpora() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, &reads_and_writes());
    assert!(
        f.issuer
            .authenticate_local("temporary-native-local-key", &ctx)
            .is_err()
    );
    f.publish_local(&ctx);
    assert!(f.issuer.authenticate_local("wrong", &ctx).is_err());
    let caller = f
        .issuer
        .authenticate_local("temporary-native-local-key", &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    seed(&f, &caller, &ctx, false).await;
    // A present native index directory without SDK metadata fails acquisition.
    drop(graphit_fs::create_private_directory(&f.project.runtime_root().join("task")).unwrap());
    drop(
        graphit_fs::create_private_directory(&f.project.runtime_root().join("task/index.lance"))
            .unwrap(),
    );
    let now = read_project_now(&authority, &caller, &ctx, f.scope(), limits())
        .await
        .unwrap();
    failed(now.sections().tasks(), NowErrorCode::Unavailable);
    failed(now.sections().sessions(), NowErrorCode::Unavailable);
    assert_eq!(
        now.sections().memories().availability(),
        SectionAvailability::Available
    );
    assert_eq!(now.sections().memories().total(), Some(1));
    assert_eq!(
        now.sections().knowledge().availability(),
        SectionAvailability::Available
    );
    assert_eq!(now.sections().knowledge().total(), Some(1));
    assert_eq!(now.sections().knowledge().items()[0].slug, "seed-knowledge");
    f.remove(&ctx);
}

fn failed<T: Serialize>(section: &NowSection<T>, error: NowErrorCode) {
    assert_eq!(section.availability(), SectionAvailability::Failed);
    assert!(section.items().is_empty());
    assert_eq!(section.total(), None);
    assert_eq!(section.has_more(), None);
    assert_eq!(section.error(), Some(error));
    let value = serde_json::to_value(section).unwrap();
    assert!(value["total"].is_null());
    assert!(value["has_more"].is_null());
}
fn empty<T>(section: &NowSection<T>, availability: SectionAvailability) {
    assert_eq!(section.availability(), availability);
    assert!(section.items().is_empty());
    assert_eq!(section.total(), Some(0));
    assert_eq!(section.has_more(), Some(false));
    assert_eq!(section.error(), None);
}

#[tokio::test]
async fn native_no_corpus_partial_policy_and_all_denied_keep_real_identity() {
    for permissions in [
        reads_and_writes(),
        vec![Permission::Read(Module::Task)],
        vec![],
    ] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, &permissions);
        let secret = f.secret(&ctx);
        let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&f.issuer);
        let now = read_project_now(&authority, &caller, &ctx, f.scope(), limits())
            .await
            .unwrap();
        assert_eq!(
            now.project_id(),
            project_identity(ctx.snapshot(), &f.scope().project.unwrap()).unwrap()
        );
        if permissions.is_empty() {
            failed(now.sections().tasks(), NowErrorCode::Forbidden);
            failed(now.sections().sessions(), NowErrorCode::Forbidden);
        } else {
            empty(now.sections().tasks(), SectionAvailability::NoCorpus);
            empty(now.sections().sessions(), SectionAvailability::NoCorpus);
        }
        if permissions.len() <= 1 {
            failed(now.sections().memories(), NowErrorCode::Forbidden);
            failed(now.sections().knowledge(), NowErrorCode::Forbidden);
        } else {
            empty(now.sections().memories(), SectionAvailability::NoCorpus);
            empty(now.sections().knowledge(), SectionAvailability::NoCorpus);
        }
        assert!(!f.project.runtime_root().join("task").exists());
        assert!(!f.project.runtime_root().join("memory").exists());
        assert!(!f.project.runtime_root().join("knowledge").exists());
        f.remove(&ctx);
    }
}

#[tokio::test]
async fn native_populated_sdk_lifecycle_counts_metadata_and_allowlist() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, &reads_and_writes());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("active-session"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let completed = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "completed"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let claimed = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        completed.specification.id,
        Duration::from_secs(60),
        task_write(),
    )
    .await
    .unwrap();
    let mut record = claimed.record;
    for check in record.specification.checks.clone() {
        record = NativeTaskStore::verify_task_check(
            ctx.snapshot(),
            &write,
            record.specification.id,
            CheckUpdate {
                claim: ClaimUpdate {
                    token: &claimed.token,
                    expected_revision: record.specification.revision,
                },
                lease: Duration::from_secs(60),
                check_id: check.id,
                passed: true,
                evidence: "Native fixture validation",
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            task_write(),
        )
        .await
        .unwrap();
    }
    NativeTaskStore::complete_task(
        ctx.snapshot(),
        &write,
        record.specification.id,
        ClaimUpdate {
            token: &claimed.token,
            expected_revision: record.specification.revision,
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let cancelled = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "cancelled"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        cancelled.specification.id,
        TaskCancellation {
            claim: None,
            reason: "Native fixture cancelled",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let active_task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "active-task"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let owned = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        active_task.specification.id,
        Duration::from_secs(60),
        task_write(),
    )
    .await
    .unwrap();
    let coordinate = authority
        .admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::CoordinateTask,
        )
        .unwrap();
    let active_session = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        Duration::from_secs(60),
        task_write(),
    )
    .await
    .unwrap();
    let memory_write_admission = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    let memory = NativeMemoryStore::create(
        ctx.snapshot(),
        &memory_write_admission,
        memory_input("memory-title"),
        memory_write(),
    )
    .await
    .unwrap();
    let other = VersionContext::select(&f.paths, Checkout::NonGit, "other", None, "").unwrap();
    let knowledge_write_admission = authority
        .admit_authenticated_write(
            &caller,
            &ctx,
            f.scope_for(&other),
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    let pages = [
        page("negative", Some(UNIX_EPOCH - Duration::from_nanos(17))),
        page("unknown", None),
    ];
    publish_native_authored(
        ctx.snapshot(),
        &knowledge_write_admission,
        &prepared(&ctx, &pages),
        knowledge_write(),
    )
    .await
    .unwrap();
    let now = read_project_now(&authority, &caller, &ctx, f.scope_for(&other), limits())
        .await
        .unwrap();
    let tasks = now.sections().tasks();
    assert_eq!(tasks.total(), Some(1));
    assert_eq!(tasks.items()[0].id, owned.record.specification.id);
    assert_eq!(
        tasks.items()[0].owner,
        owned.record.ownership.owner.unwrap()
    );
    let sessions = now.sections().sessions();
    assert_eq!(sessions.total(), Some(1));
    assert_eq!(sessions.items()[0].id, active_session.record.id);
    assert_eq!(sessions.items()[0].completed_tasks, 1);
    assert_eq!(sessions.items()[0].total_tasks, 2);
    let memories = now.sections().memories();
    assert_eq!(memories.total(), Some(1));
    assert_eq!(memories.items()[0].updated_by, memory.updated_by);
    assert_eq!(
        now.sections().knowledge().items()[0].updated_at.as_deref(),
        Some("-17")
    );
    assert_eq!(now.sections().knowledge().items()[1].updated_at, None);
    let wire = serde_json::to_value(&now).unwrap();
    keys(&wire, &["project_id", "generated_at", "sections"]);
    keys(
        &wire["sections"],
        &["tasks", "sessions", "memories", "knowledge"],
    );
    for name in ["tasks", "sessions", "memories", "knowledge"] {
        keys(
            &wire["sections"][name],
            &["availability", "items", "total", "has_more", "error"],
        );
    }
    keys(
        &wire["sections"]["tasks"]["items"][0],
        &[
            "id",
            "session_id",
            "title",
            "owner",
            "lease_expires_at",
            "updated_at",
        ],
    );
    keys(
        &wire["sections"]["sessions"]["items"][0],
        &[
            "id",
            "title",
            "owner",
            "lease_expires_at",
            "updated_at",
            "completed_tasks",
            "total_tasks",
        ],
    );
    keys(
        &wire["sections"]["memories"]["items"][0],
        &["id", "title", "updated_by", "updated_at"],
    );
    keys(
        &wire["sections"]["knowledge"]["items"][0],
        &["slug", "title", "updated_at"],
    );
    assert!(!wire.to_string().contains("PRIVATE_"));
    assert!(!wire.to_string().contains(f.root.to_str().unwrap()));
    assert_eq!(
        wire["sections"]["knowledge"]["items"][1]["updated_at"],
        serde_json::Value::Null
    );
    // Knowledge first publication confirms real default/named SDK pins. A
    // subsequent native Task write materializes only its selected named version.
    let named_task = authority
        .admit_authenticated_write(
            &caller,
            &ctx,
            f.scope_for(&other),
            ProjectWriteOperation::Task,
        )
        .unwrap();
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &named_task,
        task_input(session.id, "named-only-task"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let default_now = read_project_now(&authority, &caller, &ctx, f.scope(), limits())
        .await
        .unwrap();
    assert_eq!(default_now.project_id(), now.project_id());
    assert_eq!(default_now.sections().knowledge().total(), Some(2));
    assert_eq!(default_now.sections().sessions().items()[0].total_tasks, 2);
    let named_now = read_project_now(&authority, &caller, &ctx, f.scope_for(&other), limits())
        .await
        .unwrap();
    assert_eq!(named_now.sections().knowledge().total(), Some(2));
    assert_eq!(named_now.sections().sessions().items()[0].total_tasks, 3);
    assert_eq!(
        named_now.sections().knowledge().items()[0].title,
        "negative"
    );
    f.remove(&ctx);
}

#[tokio::test]
async fn native_published_empty_sections_and_incomplete_memory_are_distinct() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, &reads_and_writes());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    let task = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    NativeTaskStore::create_session(
        ctx.snapshot(),
        &task,
        session_input("open-empty-session"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        task_write(),
    )
    .await
    .unwrap();
    let memory = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    let saved = NativeMemoryStore::create(
        ctx.snapshot(),
        &memory,
        memory_input("deleted-memory"),
        memory_write(),
    )
    .await
    .unwrap();
    NativeMemoryStore::delete(
        ctx.snapshot(),
        &memory,
        saved.id,
        DeleteMemory {
            expected_revision: saved.revision,
            idempotency_key: "delete-memory".into(),
            reason: "Native tombstone fixture".into(),
        },
        memory_write(),
    )
    .await
    .unwrap();
    let knowledge = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Knowledge)
        .unwrap();
    publish_native_authored(
        ctx.snapshot(),
        &knowledge,
        &prepared(&ctx, &[]),
        knowledge_write(),
    )
    .await
    .unwrap();
    let now = read_project_now(&authority, &caller, &ctx, f.scope(), limits())
        .await
        .unwrap();
    empty(now.sections().tasks(), SectionAvailability::Available);
    empty(now.sections().sessions(), SectionAvailability::Available);
    empty(now.sections().memories(), SectionAvailability::Available);
    empty(now.sections().knowledge(), SectionAvailability::Available);
    f.remove(&ctx);
    let f2 = Fixture::new(&ctx, &reads_and_writes());
    let secret2 = f2.secret(&ctx);
    let caller2 = f2.issuer.authenticate_runtime(&secret2, &ctx).unwrap();
    let authority2 = ProjectRecordAuthority::new(&f2.issuer);
    std::fs::create_dir_all(f2.project.runtime_root()).unwrap();
    drop(graphit_fs::create_private_directory(&f2.project.runtime_root().join("memory")).unwrap());
    let broken = read_project_now(&authority2, &caller2, &ctx, f2.scope(), limits())
        .await
        .unwrap();
    failed(broken.sections().memories(), NowErrorCode::InvalidData);
    empty(broken.sections().tasks(), SectionAvailability::NoCorpus);
    empty(broken.sections().knowledge(), SectionAvailability::NoCorpus);
    f2.remove(&ctx);
}

fn keys(value: &serde_json::Value, expected: &[&str]) {
    let actual: std::collections::BTreeSet<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(actual, expected.iter().copied().collect());
}
fn id(value: u128) -> NonZeroU128 {
    NonZeroU128::new(value).unwrap()
}
fn ownership(lease: u64) -> Ownership {
    Ownership {
        owner: Some("a".repeat(64)),
        claim_epoch: 1,
        claimed_at: Some(1),
        heartbeat_at: Some(2),
        lease_expires_at: Some(lease),
    }
}
fn task_head(value: u128, updated_at: u64) -> TaskActivityHead {
    TaskActivityHead {
        id: TaskId::new(id(value)),
        session_id: SessionId::new(id(1)),
        title: format!("task-{value}"),
        status: TaskStatus::InProgress,
        ownership: ownership(1000),
        progress: None,
        revision: 1,
        created_at: 1,
        updated_at,
    }
}
fn session_head(value: u128) -> SessionHead {
    SessionHead {
        id: SessionId::new(id(value)),
        title: format!("session-{value}"),
        status: TaskStatus::InProgress,
        revision: 1,
        ownership: ownership(1000),
        progress: None,
        completed_tasks: 1,
        total_tasks: 3,
        updated_at: 7,
    }
}

#[test]
fn projection_activity_totals_top20_numeric_order_and_all_contributors() {
    let mut heads: Vec<_> = (1..=24).rev().map(|i| task_head(i, 7)).collect();
    heads[0].ownership.owner = Some("b".repeat(64));
    let mut expired = task_head(30, u64::MAX);
    expired.ownership.lease_expires_at = Some(10);
    let mut absent = task_head(31, 8);
    absent.ownership = Ownership::default();
    let mut completed = task_head(32, 8);
    completed.status = TaskStatus::Completed;
    heads.extend([expired, absent, completed]);
    let section = tasks_section(heads, 10);
    assert_eq!(section.total(), Some(24));
    assert_eq!(section.items().len(), 20);
    assert_eq!(section.has_more(), Some(true));
    assert_eq!(section.items()[0].id.get().get(), 1);
    assert_eq!(section.items()[19].id.get().get(), 20);
    assert!(!active(TaskStatus::InProgress, &ownership(10), 10));
    assert!(!active(TaskStatus::InProgress, &ownership(9), 10));
    assert!(active(TaskStatus::InProgress, &ownership(11), 10));
    let mut contributors = vec![task_head(1, 9), task_head(2, 10)];
    contributors[1].ownership.owner = Some("b".repeat(64));
    let section = tasks_section(contributors, 10);
    assert_eq!(section.items()[0].owner, "b".repeat(64));
    assert_eq!(section.items()[1].owner, "a".repeat(64));
    let section = sessions_section((1..=24).rev().map(session_head).collect(), 10);
    assert_eq!(section.total(), Some(24));
    assert_eq!(section.items()[0].id.get().get(), 1);
    assert_eq!(section.items()[19].total_tasks, 3);
    let memories = (1..=24)
        .rev()
        .map(|i| MemoryActivityHead {
            id: MemoryId::new(id(i)),
            title: "memory".into(),
            updated_by: "b".repeat(64),
            updated_at: 7,
        })
        .collect();
    let section = memories_section(memories);
    assert_eq!(section.total(), Some(24));
    assert_eq!(section.items()[0].id.get().get(), 1);
    assert_eq!(section.has_more(), Some(true));
}

#[test]
fn projection_times_are_lossless_strings_and_unknown_last() {
    assert_eq!(milliseconds_ns(u64::MAX), "18446744073709551615000000");
    assert_eq!(system_time_ns(UNIX_EPOCH).unwrap(), "0");
    // Windows SystemTime represents 100ns ticks. These exact instants exceed
    // i64 nanoseconds in both directions without relying on finer precision.
    let wide = Duration::from_secs(10_000_000_000);
    assert_eq!(
        system_time_ns(UNIX_EPOCH - wide).unwrap(),
        "-10000000000000000000"
    );
    assert_eq!(
        system_time_ns(UNIX_EPOCH + wide).unwrap(),
        "10000000000000000000"
    );
    assert!(Observation::at(UNIX_EPOCH - Duration::from_nanos(100)).is_err());
    let at = Observation::at(UNIX_EPOCH + Duration::from_nanos(1_234_500)).unwrap();
    assert_eq!(at.generated_at, "1234500");
    assert_eq!(at.milliseconds, 1);
    let heads = vec![
        KnowledgeHead {
            slug: "unknown".into(),
            title: "".into(),
            updated_at: None,
        },
        KnowledgeHead {
            slug: "b".into(),
            title: "".into(),
            updated_at: Some(UNIX_EPOCH - Duration::from_nanos(100)),
        },
        KnowledgeHead {
            slug: "a".into(),
            title: "".into(),
            updated_at: Some(UNIX_EPOCH - Duration::from_nanos(100)),
        },
        KnowledgeHead {
            slug: "latest".into(),
            title: "".into(),
            updated_at: Some(UNIX_EPOCH + Duration::from_nanos(1_000)),
        },
    ];
    let section = knowledge_section(heads);
    assert_eq!(
        section
            .items()
            .iter()
            .map(|h| h.slug.as_str())
            .collect::<Vec<_>>(),
        vec!["latest", "a", "b", "unknown"]
    );
    let wire = serde_json::to_value(section).unwrap();
    assert_eq!(wire["items"][1]["updated_at"], "-100");
    assert!(wire["items"][3]["updated_at"].is_null());
}

#[test]
#[cfg(target_pointer_width = "64")]
fn projection_safe_counts_validate_all_active_sessions_before_top20() {
    let max = usize::try_from(SAFE_COUNT_MAX).unwrap();
    assert_eq!(safe_count(max).unwrap(), SAFE_COUNT_MAX);
    assert_eq!(safe_count(max + 1), Err(NowErrorCode::LimitExceeded));
    let mut heads: Vec<_> = (1..=24).map(session_head).collect();
    heads[23].total_tasks = max + 1;
    failed(&sessions_section(heads, 10), NowErrorCode::LimitExceeded);
    let mut head = session_head(1);
    head.completed_tasks = max;
    head.total_tasks = max;
    let wire = serde_json::to_value(sessions_section(vec![head], 10)).unwrap();
    assert_eq!(
        wire["items"][0]["total_tasks"].as_u64(),
        Some(SAFE_COUNT_MAX)
    );
}

#[test]
fn projection_task_session_errors_and_finite_error_privacy() {
    let observation = TaskHeadObservation::Published {
        generation: TaskGeneration {
            version: 1,
            physical_branch: None,
            inherited_target: None,
        },
        tasks: Ok(vec![task_head(1, 7)]),
        sessions: Err(Box::new(TaskStoreError::Invalid("PRIVATE_ERROR_CAUSE"))),
    };
    let (tasks, sessions) = task_sections(observation, 10);
    assert_eq!(tasks.total(), Some(1));
    failed(&sessions, NowErrorCode::InvalidData);
    let observation = TaskHeadObservation::Published {
        generation: TaskGeneration {
            version: 1,
            physical_branch: None,
            inherited_target: None,
        },
        tasks: Err(Box::new(TaskStoreError::Invalid("PRIVATE_ERROR_CAUSE"))),
        sessions: Err(Box::new(TaskStoreError::CountsUnavailable)),
    };
    let (tasks, sessions) = task_sections(observation, 10);
    failed(&tasks, NowErrorCode::InvalidData);
    failed(&sessions, NowErrorCode::CountsUnavailable);
    assert!(
        !serde_json::to_string(&sessions)
            .unwrap()
            .contains("PRIVATE_")
    );
    assert_eq!(auth_code(&AuthError::Request), NowErrorCode::Unavailable);
    assert_eq!(auth_code(&AuthError::Expired), NowErrorCode::Forbidden);
    assert_eq!(auth_code(&AuthError::Limit), NowErrorCode::LimitExceeded);
    assert_eq!(
        task_code(&TaskStoreError::DifferentRequest),
        NowErrorCode::Forbidden
    );
    assert_eq!(
        task_code(&TaskStoreError::RevisionConflict),
        NowErrorCode::Unavailable
    );
    assert_eq!(
        memory_code(&MemoryStoreError::IncompleteCorpus),
        NowErrorCode::InvalidData
    );
    assert_eq!(
        memory_code(&MemoryStoreError::Limit("private")),
        NowErrorCode::LimitExceeded
    );
    assert_eq!(
        knowledge_code(&KnowledgeIndexError::DuplicateSlug),
        NowErrorCode::InvalidData
    );
    assert_eq!(
        knowledge_code(&KnowledgeIndexError::Overflow("private")),
        NowErrorCode::LimitExceeded
    );
    let empty = task_sections(
        TaskHeadObservation::Published {
            generation: TaskGeneration {
                version: 1,
                physical_branch: None,
                inherited_target: None,
            },
            tasks: Ok(vec![]),
            sessions: Ok(vec![]),
        },
        10,
    );
    super::tests::empty(&empty.0, SectionAvailability::Available);
    super::tests::empty(&empty.1, SectionAvailability::Available);
}

#[tokio::test]
async fn native_final_boundary_rechecks_revocation_cancel_and_physical_identity() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, &reads_and_writes());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    seed(&f, &caller, &ctx, true).await;
    let authority = ProjectRecordAuthority::new(&f.issuer);
    let task = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let memory = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory)
        .unwrap();
    let knowledge = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge)
        .unwrap();
    let snapshot = read_project_now(&authority, &caller, &ctx, f.scope(), limits())
        .await
        .unwrap();
    assert_eq!(snapshot.sections().tasks().total(), Some(1));
    assert_eq!(snapshot.sections().sessions().total(), Some(1));
    assert_eq!(snapshot.sections().memories().total(), Some(1));
    assert_eq!(snapshot.sections().knowledge().total(), Some(1));
    assert_eq!(snapshot.sections().tasks().items().len(), 1);
    assert_eq!(snapshot.sections().sessions().items().len(), 1);
    assert_eq!(snapshot.sections().memories().items().len(), 1);
    assert_eq!(snapshot.sections().knowledge().items().len(), 1);
    let refreshed =
        reauthorize_project_now(&authority, &caller, &ctx, f.scope(), &snapshot).unwrap();
    assert_eq!(
        serde_json::to_value(&refreshed).unwrap(),
        serde_json::to_value(&snapshot).unwrap()
    );
    f.remove(&ctx);
    let denied_heartbeat =
        reauthorize_project_now(&authority, &caller, &ctx, f.scope(), &snapshot).unwrap();
    assert_eq!(denied_heartbeat.generated_at(), snapshot.generated_at());
    failed(denied_heartbeat.sections().tasks(), NowErrorCode::Forbidden);
    failed(
        denied_heartbeat.sections().sessions(),
        NowErrorCode::Forbidden,
    );
    failed(
        denied_heartbeat.sections().memories(),
        NowErrorCode::Forbidden,
    );
    failed(
        denied_heartbeat.sections().knowledge(),
        NowErrorCode::Forbidden,
    );
    assert_eq!(snapshot.sections().tasks().total(), Some(1));
    assert_eq!(snapshot.sections().memories().total(), Some(1));
    let denied = finish(
        ctx.snapshot(),
        &f.scope().project.unwrap(),
        Some(&task),
        Some(&memory),
        Some(&knowledge),
        snapshot,
    )
    .unwrap();
    failed(denied.sections().tasks(), NowErrorCode::Forbidden);
    failed(denied.sections().sessions(), NowErrorCode::Forbidden);
    failed(denied.sections().memories(), NowErrorCode::Forbidden);
    failed(denied.sections().knowledge(), NowErrorCode::Forbidden);
    let f2 = Fixture::new(&ctx, &reads_and_writes());
    let secret2 = f2.secret(&ctx);
    let caller2 = f2.issuer.authenticate_runtime(&secret2, &ctx).unwrap();
    let authority2 = ProjectRecordAuthority::new(&f2.issuer);
    let snapshot = read_project_now(&authority2, &caller2, &ctx, f2.scope(), limits())
        .await
        .unwrap();
    let parked = f2._directory.path().join("parked");
    let swapped = match std::fs::rename(&f2.root, &parked) {
        Ok(()) => true,
        Err(error) if cfg!(windows) && error.raw_os_error() == Some(32) => false,
        Err(error) => panic!("unexpected root replacement failure: {error}"),
    };
    if swapped {
        std::fs::create_dir(&f2.root).unwrap();
        assert!(matches!(
            reauthorize_project_now(&authority2, &caller2, &ctx, f2.scope(), &snapshot),
            Err(ProjectNowError::Scope(_))
        ));
        assert!(matches!(
            finish(
                ctx.snapshot(),
                &f2.scope().project.unwrap(),
                None,
                None,
                None,
                snapshot
            ),
            Err(ProjectNowError::Scope(_))
        ));
        std::fs::remove_dir(&f2.root).unwrap();
        std::fs::rename(parked, &f2.root).unwrap();
    } else {
        // Pinned Windows ancestors omit share-delete: the attempted replacement
        // has no effect, and the same admitted caller must remain usable.
        assert!(f2.root.is_dir());
        assert!(!parked.exists());
        let retained =
            reauthorize_project_now(&authority2, &caller2, &ctx, f2.scope(), &snapshot).unwrap();
        assert_eq!(
            serde_json::to_value(retained).unwrap(),
            serde_json::to_value(&snapshot).unwrap()
        );
        let denied = finish(
            ctx.snapshot(),
            &f2.scope().project.unwrap(),
            None,
            None,
            None,
            snapshot,
        )
        .unwrap();
        failed(denied.sections().tasks(), NowErrorCode::Forbidden);
        failed(denied.sections().sessions(), NowErrorCode::Forbidden);
        failed(denied.sections().memories(), NowErrorCode::Forbidden);
        failed(denied.sections().knowledge(), NowErrorCode::Forbidden);
    }
    let snapshot = read_project_now(&authority2, &caller2, &ctx, f2.scope(), limits())
        .await
        .unwrap();
    cancel.cancel();
    assert!(matches!(
        reauthorize_project_now(&authority2, &caller2, &ctx, f2.scope(), &snapshot),
        Err(ProjectNowError::Request(error)) if error.kind() == io::ErrorKind::Interrupted
    ));
    assert!(
        matches!(finish(ctx.snapshot(), &f2.scope().project.unwrap(), None, None, None, snapshot), Err(ProjectNowError::Request(e)) if e.kind() == io::ErrorKind::Interrupted)
    );
    let cleanup_cancel = Cancellation::default();
    let cleanup = request(&cleanup_cancel);
    f2.remove(&cleanup);
}

#[tokio::test]
async fn native_original_request_scope_and_deadline_are_not_reconstructed() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, &reads_and_writes());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    let other_ctx = request(&cancel);
    let other = read_project_now(&authority, &caller, &other_ctx, f.scope(), limits())
        .await
        .unwrap();
    failed(other.sections().tasks(), NowErrorCode::Forbidden);
    failed(other.sections().knowledge(), NowErrorCode::Forbidden);
    let mut missing = f.scope();
    missing.project = None;
    assert!(matches!(
        read_project_now(&authority, &caller, &ctx, missing, limits()).await,
        Err(ProjectNowError::Scope(_))
    ));
    let expired_cancel = Cancellation::default();
    let expired = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        },
        &expired_cancel,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let short_caller = f.issuer.authenticate_runtime(&secret, &expired).unwrap();
    let snapshot = read_project_now(&authority, &short_caller, &expired, f.scope(), limits())
        .await
        .unwrap();
    tokio::time::sleep(
        expired
            .snapshot()
            .deadline()
            .expect("finite fixture deadline")
            .saturating_duration_since(Instant::now())
            + Duration::from_millis(10),
    )
    .await;
    assert!(
        matches!(finish(expired.snapshot(), &f.scope().project.unwrap(), None, None, None, snapshot), Err(ProjectNowError::Request(e)) if e.kind() == io::ErrorKind::TimedOut)
    );
    f.remove(&ctx);
}

fn role_administrative() -> Vec<Permission> {
    vec![
        Permission::ManageAccessGrants,
        Permission::Read(Module::Task),
        Permission::Read(Module::Memory),
        Permission::Read(Module::Knowledge),
    ]
}
fn configure_role(
    f: &Fixture,
    parent: &graphit_auth::AdminParent<'_>,
    role: Role,
    permissions: &[Permission],
) -> RoleCredentialReference {
    // Qualified fixture definition data; only the real parent can register it.
    let definition = RoleDefinition::tool_owned(
        role,
        f.paths.brand(),
        NativeBinding::KiroV3,
        permissions,
        [37; 32],
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
    )
    .unwrap();
    f.issuer
        .configure_tool_owned_definition(parent, definition)
        .unwrap()
}

#[tokio::test]
async fn native_role_modules_share_compositor_and_keep_independent_denials() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_administrative(&ctx, &reads_and_writes(), &role_administrative());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    seed(&f, &caller, &ctx, true).await;
    let session = f.issuer.bootstrap_admin(&caller).unwrap();
    let parent = f.issuer.admin_parent(&caller, &session).unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    for (role, modules) in [
        (
            Role::Scout,
            vec![Module::Task, Module::Memory, Module::Knowledge],
        ),
        (Role::Tracker, vec![Module::Task, Module::Knowledge]),
        (Role::Scribe, vec![Module::Task, Module::Memory]),
    ] {
        let permissions: Vec<_> = modules.iter().copied().map(Permission::Read).collect();
        let reference = configure_role(&f, &parent, role, &permissions);
        let run = f
            .issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(60))
            .unwrap();
        let bearer = f
            .issuer
            .issue(&parent, &run, &reference, &permissions, &f.scope())
            .unwrap();
        let grant = f
            .issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let now = read_project_now_role(&authority, &grant, &ctx, f.scope(), limits())
            .await
            .unwrap();
        assert_eq!(
            now.project_id(),
            project_identity(ctx.snapshot(), &f.scope().project.unwrap()).unwrap()
        );
        assert_eq!(now.sections().tasks().total(), Some(1));
        assert_eq!(now.sections().sessions().total(), Some(1));
        if modules.contains(&Module::Memory) {
            assert_eq!(now.sections().memories().total(), Some(1));
        } else {
            failed(now.sections().memories(), NowErrorCode::Forbidden);
        }
        if modules.contains(&Module::Knowledge) {
            assert_eq!(now.sections().knowledge().total(), Some(1));
        } else {
            failed(now.sections().knowledge(), NowErrorCode::Forbidden);
        }
        let wire = serde_json::to_value(&now).unwrap();
        keys(&wire, &["project_id", "generated_at", "sections"]);
        f.issuer.close_run(&parent, &run).unwrap();
        let revoked = read_project_now_role(&authority, &grant, &ctx, f.scope(), limits())
            .await
            .unwrap();
        failed(revoked.sections().tasks(), NowErrorCode::Forbidden);
        failed(revoked.sections().sessions(), NowErrorCode::Forbidden);
        failed(revoked.sections().memories(), NowErrorCode::Forbidden);
        failed(revoked.sections().knowledge(), NowErrorCode::Forbidden);
    }
    f.remove(&ctx);
}

#[tokio::test]
async fn native_role_now_preserves_original_version_and_physical_identity() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_administrative(&ctx, &reads_and_writes(), &role_administrative());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    seed(&f, &caller, &ctx, true).await;
    let session = f.issuer.bootstrap_admin(&caller).unwrap();
    let parent = f.issuer.admin_parent(&caller, &session).unwrap();
    let permissions = role_administrative()
        .into_iter()
        .filter(|p| matches!(p, Permission::Read(_)))
        .collect::<Vec<_>>();
    let reference = configure_role(&f, &parent, Role::Scout, &permissions);
    let run = f
        .issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(60))
        .unwrap();
    let bearer = f
        .issuer
        .issue(&parent, &run, &reference, &permissions, &f.scope())
        .unwrap();
    let grant = f
        .issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);

    let other_request = request(&cancel);
    let foreign = read_project_now_role(&authority, &grant, &other_request, f.scope(), limits())
        .await
        .unwrap();
    failed(foreign.sections().tasks(), NowErrorCode::Forbidden);
    failed(foreign.sections().memories(), NowErrorCode::Forbidden);
    failed(foreign.sections().knowledge(), NowErrorCode::Forbidden);
    let other_version =
        VersionContext::select(&f.paths, Checkout::NonGit, "other", None, "").unwrap();
    let foreign = read_project_now_role(
        &authority,
        &grant,
        &ctx,
        f.scope_for(&other_version),
        limits(),
    )
    .await
    .unwrap();
    failed(foreign.sections().tasks(), NowErrorCode::Forbidden);
    failed(foreign.sections().memories(), NowErrorCode::Forbidden);
    failed(foreign.sections().knowledge(), NowErrorCode::Forbidden);

    let peer_paths = ProjectPaths::at(&f.peer_root, &f.installation_paths).unwrap();
    let peer_identity = ProjectIdentityStore::at(&peer_paths)
        .writer(Duration::from_secs(5), ctx.snapshot().cancellation())
        .unwrap()
        .ensure(&ProjectMetadata::new("role-peer").unwrap())
        .unwrap();
    let peer_capture = CapturedPublicationScope::project(
        &peer_paths,
        peer_identity.id(),
        ScopeLimits::new(16, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let peer_record = peer_capture.binding().unwrap();
    let peer_version =
        VersionContext::select(&peer_paths, Checkout::NonGit, "main", None, "").unwrap();
    let peer_scope = ScopeInput {
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &peer_capture,
                record: &peer_record,
            },
            version: &peer_version,
        }),
        ..f.scope()
    };
    let foreign = read_project_now_role(&authority, &grant, &ctx, peer_scope, limits())
        .await
        .unwrap();
    failed(foreign.sections().tasks(), NowErrorCode::Forbidden);
    failed(foreign.sections().memories(), NowErrorCode::Forbidden);
    failed(foreign.sections().knowledge(), NowErrorCode::Forbidden);
    let mut wrong_binding = f.scope();
    wrong_binding.binding = NativeBinding::Claude;
    assert!(
        f.issuer
            .verify(
                bearer.expose_for_transport(),
                &reference,
                wrong_binding,
                &ctx
            )
            .is_err()
    );
    let mut missing = f.scope();
    missing.project = None;
    assert!(matches!(
        read_project_now_role(&authority, &grant, &ctx, missing, limits()).await,
        Err(ProjectNowError::Scope(_))
    ));
    let wrong_reference =
        RoleCredentialReference::new(Role::Scout, reference.source().clone(), [9; 32]);
    assert!(
        f.issuer
            .verify(
                bearer.expose_for_transport(),
                &wrong_reference,
                f.scope(),
                &ctx
            )
            .is_err()
    );

    let parked = f._directory.path().join("role-parked");
    let swapped = match std::fs::rename(&f.root, &parked) {
        Ok(()) => true,
        Err(error) if cfg!(windows) && error.raw_os_error() == Some(32) => false,
        Err(error) => panic!("unexpected role root replacement failure: {error}"),
    };
    if swapped {
        std::fs::create_dir(&f.root).unwrap();
        assert!(matches!(
            read_project_now_role(&authority, &grant, &ctx, f.scope(), limits()).await,
            Err(ProjectNowError::Scope(_))
        ));
        std::fs::remove_dir(&f.root).unwrap();
        std::fs::rename(&parked, &f.root).unwrap();
    } else {
        assert!(f.root.is_dir());
        assert!(!parked.exists());
        let retained = read_project_now_role(&authority, &grant, &ctx, f.scope(), limits())
            .await
            .unwrap();
        assert_eq!(retained.sections().tasks().total(), Some(1));
        assert_eq!(retained.sections().sessions().total(), Some(1));
        assert_eq!(retained.sections().memories().total(), Some(1));
        assert_eq!(retained.sections().knowledge().total(), Some(1));
    }
    f.issuer.close_run(&parent, &run).unwrap();
    cancel.cancel();
    assert!(
        matches!(read_project_now_role(&authority, &grant, &ctx, f.scope(), limits()).await, Err(ProjectNowError::Request(e)) if e.kind() == io::ErrorKind::Interrupted)
    );
    let cleanup_cancel = Cancellation::default();
    let cleanup = request(&cleanup_cancel);
    f.remove(&cleanup);
}

#[tokio::test]
async fn native_role_now_rechecks_closed_run_and_request_deadline_without_head_data() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_administrative(&ctx, &reads_and_writes(), &role_administrative());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let session = f.issuer.bootstrap_admin(&caller).unwrap();
    let parent = f.issuer.admin_parent(&caller, &session).unwrap();
    let permissions = [Permission::Read(Module::Task)];
    let reference = configure_role(&f, &parent, Role::Tracker, &permissions);
    let run = f
        .issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(60))
        .unwrap();
    let bearer = f
        .issuer
        .issue(&parent, &run, &reference, &permissions, &f.scope())
        .unwrap();
    let grant = f
        .issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    let valid = read_project_now_role(&authority, &grant, &ctx, f.scope(), limits())
        .await
        .unwrap();
    empty(valid.sections().tasks(), SectionAvailability::NoCorpus);
    // Auth owns deterministic expiry coverage with its private clock. Here the
    // real run is closed after admission to prove Now performs a fresh recheck.
    f.issuer.close_run(&parent, &run).unwrap();
    let revoked = read_project_now_role(&authority, &grant, &ctx, f.scope(), limits())
        .await
        .unwrap();
    failed(revoked.sections().tasks(), NowErrorCode::Forbidden);
    failed(revoked.sections().sessions(), NowErrorCode::Forbidden);

    let expired_cancel = Cancellation::default();
    let request_deadline = Instant::now() + Duration::from_millis(50);
    let expired_request = RequestContext::new(
        ContentLimits {
            max_input_bytes: 1048576,
            max_output_bytes: 1048576,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 65536,
            max_collection_entries: 65536,
        },
        &expired_cancel,
        request_deadline,
    )
    .unwrap();
    tokio::time::sleep_until((request_deadline + Duration::from_millis(1)).into()).await;
    assert!(
        matches!(read_project_now_role(&authority, &grant, &expired_request, f.scope(), limits()).await, Err(ProjectNowError::Request(e)) if e.kind() == io::ErrorKind::TimedOut)
    );
    f.remove(&ctx);
}

/// Genuine allowlisted observations for publisher coordination tests. The test
/// controls completion separately; this does not simulate a blocked Lance read.
#[cfg(target_os = "linux")]
pub(crate) async fn publisher_observations() -> (NowLocal, NowLocal) {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, &reads_and_writes());
    let secret = f.secret(&ctx);
    let caller = f.issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&f.issuer);
    let first = read_project_now(&authority, &caller, &ctx, f.scope(), limits())
        .await
        .unwrap();
    let second = read_project_now(&authority, &caller, &ctx, f.scope(), limits())
        .await
        .unwrap();
    f.remove(&ctx);
    (first, second)
}
