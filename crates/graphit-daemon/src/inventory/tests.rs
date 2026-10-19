//! Runtime consumer tests: actual Core captures, issuer grants and native SDK publication.
use super::*;
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
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::ScopedPin,
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
    install: InstallationPaths,
}
impl Fixture {
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
        let identity = ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), ctx.snapshot().cancellation())
            .unwrap()
            .ensure(&ProjectMetadata::new("fixture").unwrap())
            .unwrap();
        let limits = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("local-installation").unwrap(),
            [root.clone()],
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let project =
            CapturedPublicationScope::project(&paths, identity.id(), limits, ctx.snapshot())
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
            install,
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
        self.issuer_for(ctx, self.realm())
    }
    fn issuer_for(&self, ctx: &RequestContext<'_>, realm: ScopeCapture<'_>) -> RoleIssuer {
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
            realm,
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
        self.issue_scope(issuer, parent, self.scope_for(version))
    }
    fn issue_scope(
        &self,
        issuer: &RoleIssuer,
        parent: &AdminParent<'_>,
        scope: ScopeInput<'_>,
    ) -> (RoleBearer, RoleCredentialReference) {
        let source = SourceRef::restore(
            SourceRecord {
                remote_file_origin: None,
                archive_sha256: None,
                origin: SourceOrigin::LocalLink,
                producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                artifact_type: ArtifactType::Agent,
                id: format!(
                    "roles/coordinator-{}",
                    scope.project.unwrap().version.version()
                ),
                physical_id: None,
                version: "1.0.0".into(),
                root: NativePath::new(scope.project.unwrap().scope.live.owner_root().to_owned())
                    .unwrap(),
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
            .start_run(parent, &scope, Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(parent, &run, &reference, &perms, &scope)
            .unwrap();
        (bearer, reference)
    }
}

#[tokio::test]
async fn physical_corpus_rename_preserves_public_partial_default_and_retention_attempt() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "named");
    let corpus = f.project.runtime_root().join("knowledge/index.lance");
    let directory = f.project.runtime_root().join("knowledge");
    let moved = f.project.runtime_root().join("knowledge-moved");
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_finished = finished.clone();
    let moved_worker = moved.clone();
    let observer = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until && !worker_finished.load(std::sync::atomic::Ordering::Acquire)
        {
            let published = std::fs::read_dir(corpus.join("_versions"))
                .ok()
                .is_some_and(|mut entries| {
                    entries.any(|entry| {
                        entry.is_ok_and(|entry| {
                            entry
                                .path()
                                .extension()
                                .is_some_and(|ext| ext == "manifest")
                        })
                    })
                });
            if published {
                return std::fs::rename(directory, moved_worker);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(std::io::ErrorKind::TimedOut.into())
    });
    let result = f.publish(&ctx, &GitCheckout::NonGit, limits()).await;
    finished.store(true, std::sync::atomic::Ordering::Release);
    observer.join().unwrap().unwrap();
    let failure = result.unwrap_err();
    assert!(
        matches!(&failure.cause, KnowledgeInventoryCause::Data(KnowledgeIndexError::PartialDefault { generation, .. }) if generation.physical_branch.is_none() && generation.version > 0)
    );
    assert!(failure.publication.is_none());
    assert_eq!(
        failure
            .retained
            .iter()
            .map(|r| r.version.as_str())
            .collect::<Vec<_>>(),
        ["main"]
    );
    assert!(failure.retention_failure.is_none());
    assert_eq!(f.versions(ctx.snapshot()), ["main"]);
    assert!(moved.join("index.lance/_versions").is_dir());
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

impl Fixture {
    fn private(&self, original: &SnapshotContext) -> PrivateInstallation {
        installation_bootstrap::prepare(
            &self.install,
            BootstrapLimits::new(65536, 256).unwrap(),
            original,
        )
        .into_installation()
        .unwrap()
    }
    fn store(&self) -> GlobalProjectInventory {
        GlobalProjectInventory::at(&self.install, 65536).unwrap()
    }
    fn versions(&self, original: &SnapshotContext) -> Vec<String> {
        let id = ProjectIdentityStore::at(&self.paths)
            .load()
            .unwrap()
            .unwrap()
            .id();
        self.store()
            .load(original)
            .unwrap()
            .get(id)
            .map(|p| p.versions().map(str::to_owned).collect())
            .unwrap_or_default()
    }
    fn input<'a>(
        &'a self,
        checkout: &'a GitCheckout,
        original: &SnapshotContext,
    ) -> KnowledgeInventoryInput<'a> {
        KnowledgeInventoryInput {
            project: &self.paths,
            checkout,
            installation: self.private(original),
        }
    }
    async fn publish(
        &self,
        ctx: &RequestContext<'_>,
        checkout: &GitCheckout,
        limits: KnowledgeInventoryLimits,
    ) -> Result<KnowledgeInventoryPublication, Box<KnowledgeInventoryError>> {
        let issuer = self.issuer(ctx);
        let caller = issuer.authenticate_runtime(&self.secret(), ctx).unwrap();
        let admin = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let (bearer, reference) = self.issue(&issuer, &parent);
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, self.scope(), ctx)
            .unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let write = authority
            .admit_write(&grant, ctx, ProjectWriteOperation::Knowledge)
            .unwrap();
        let docs = [page("document", &[])];
        let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
        publish_knowledge_and_retain(
            self.input(checkout, ctx.snapshot()),
            limits,
            ctx.snapshot(),
            &write,
            &rows,
        )
        .await
    }
}
fn limits() -> KnowledgeInventoryLimits {
    KnowledgeInventoryLimits {
        native: native(),
        inventory_bytes: 65536,
        data: write_limits(),
    }
}

#[tokio::test]
async fn actual_publications_keep_old_versions_and_isolate_two_projects() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let a = Fixture::with_version(&ctx, "release/β");
    let b = Fixture::with_version(&ctx, "");
    let capture = CapturedProjectInventory::capture_in_scope(
        &a.paths,
        &GitCheckout::NonGit,
        &a.version,
        &a.project,
        native(),
        ctx.snapshot(),
    )
    .unwrap();
    a.store()
        .writer(a.private(ctx.snapshot()), ctx.snapshot())
        .unwrap()
        .retain_version(&capture, "older")
        .unwrap();
    let mut result = a
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap();
    assert_eq!(
        result
            .retained
            .iter()
            .map(|r| r.version.as_str())
            .collect::<Vec<_>>(),
        ["main", "release/β"]
    );
    assert!(
        result
            .retained
            .iter()
            .all(|r| r.change == VersionRetention::Retained)
    );
    assert_eq!(
        result
            .publication
            .named
            .as_ref()
            .unwrap()
            .physical_branch
            .as_deref(),
        Some(a.version.lance_branch().as_str())
    );
    assert_eq!(a.versions(ctx.snapshot()), ["main", "older", "release/β"]);
    // Predicate checks use a real SDK result. Deliberately changing this owned
    // DTO exercises result validation only, not a substitute Data producer.
    let physical = result
        .publication
        .named
        .as_mut()
        .unwrap()
        .physical_branch
        .take();
    assert!(matches!(
        check_publication(&result.publication, &a.version.lance_branch()),
        Err(KnowledgeInventoryCause::ResultBinding(_))
    ));
    result.publication.named.as_mut().unwrap().physical_branch = physical;
    assert!(check_publication(&result.publication, &a.version.lance_branch()).is_ok());
    assert!(b.versions(ctx.snapshot()).is_empty());
    let second = b
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap();
    assert!(second.publication.named.is_none());
    assert_eq!(second.retained.len(), 1);
    assert_eq!(b.versions(ctx.snapshot()), ["main"]);
    assert_eq!(a.versions(ctx.snapshot()), ["main", "older", "release/β"]);
    let issuer = a.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&a.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = a.issue(&issuer, &parent);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, a.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
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
        panic!("actual corpus");
    };
    assert_eq!(generation, result.publication.named.unwrap());
    assert_eq!(documents.len(), 1);
}

#[tokio::test]
async fn existing_default_retention_is_a_real_noop_and_retry_keeps_corpus() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
    let capture = CapturedProjectInventory::capture_in_scope(
        &f.paths,
        &GitCheckout::NonGit,
        &f.version,
        &f.project,
        native(),
        ctx.snapshot(),
    )
    .unwrap();
    f.store()
        .writer(f.private(ctx.snapshot()), ctx.snapshot())
        .unwrap()
        .retain_version(&capture, "main")
        .unwrap();
    let result = f
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap();
    assert_eq!(result.retained[0].change, VersionRetention::Unchanged);
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap_err();
    assert!(matches!(failure.cause, KnowledgeInventoryCause::Data(_)));
    assert!(failure.publication.is_none());
    assert!(failure.retained.is_empty());
    assert_eq!(f.versions(ctx.snapshot()), ["main"]);
}

#[tokio::test]
async fn wrong_operation_foreign_request_and_root_fail_before_sdk() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
    let other = Fixture::with_version(&ctx, "");
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let docs = [page("document", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    let wrong = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let failure = publish_knowledge_and_retain(
        f.input(&GitCheckout::NonGit, ctx.snapshot()),
        limits(),
        ctx.snapshot(),
        &wrong,
        &rows,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        failure.cause,
        KnowledgeInventoryCause::Data(KnowledgeIndexError::WrongAdmission)
    ));
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let foreign = Arc::new(SnapshotContext::new(
        ctx.deadline().expect("finite fixture deadline"),
        Cancellation::default(),
    ));
    let failure = publish_knowledge_and_retain(
        f.input(&GitCheckout::NonGit, ctx.snapshot()),
        limits(),
        &foreign,
        &write,
        &rows,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        failure.cause,
        KnowledgeInventoryCause::Data(KnowledgeIndexError::DifferentRequest)
    ));
    let input = KnowledgeInventoryInput {
        project: &other.paths,
        checkout: &GitCheckout::NonGit,
        installation: f.private(ctx.snapshot()),
    };
    let failure = publish_knowledge_and_retain(input, limits(), ctx.snapshot(), &write, &rows)
        .await
        .unwrap_err();
    assert!(matches!(failure.cause, KnowledgeInventoryCause::Capture(_)));
    assert!(!f.project.runtime_root().join("knowledge").exists());
    assert!(!other.project.runtime_root().join("knowledge").exists());
    assert!(f.versions(ctx.snapshot()).is_empty());
}

#[tokio::test]
async fn missing_and_ephemeral_identity_and_checkout_mismatch_fail_before_sdk() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
    let failure = f
        .publish(
            &ctx,
            &GitCheckout::Branch {
                name: "feature".into(),
                default: "main".into(),
            },
            limits(),
        )
        .await
        .unwrap_err();
    assert!(matches!(failure.cause, KnowledgeInventoryCause::Capture(_)));
    let store = ProjectIdentityStore::at(&f.paths);
    let identity = store.load().unwrap().unwrap();
    let mut metadata = identity.metadata().clone();
    metadata.ephemeral = true;
    store
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .update(identity.id(), &metadata)
        .unwrap();
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap_err();
    assert!(matches!(failure.cause, KnowledgeInventoryCause::Capture(_)));
    std::fs::remove_file(f.paths.identity_file()).unwrap();
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap_err();
    assert!(
        matches!(&failure.cause, KnowledgeInventoryCause::Capture(e) if e.kind() == std::io::ErrorKind::NotFound)
    );
    assert!(!f.project.runtime_root().join("knowledge").exists());
}

#[tokio::test]
async fn second_real_inventory_commit_failure_preserves_publication_and_first_commit() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "named");
    // Obtain the actual bounded Core document size, then remove only this
    // temporary fixture document so the consumer performs both real commits.
    let capture = CapturedProjectInventory::capture_in_scope(
        &f.paths,
        &GitCheckout::NonGit,
        &f.version,
        &f.project,
        native(),
        ctx.snapshot(),
    )
    .unwrap();
    f.store()
        .writer(f.private(ctx.snapshot()), ctx.snapshot())
        .unwrap()
        .retain_version(&capture, "main")
        .unwrap();
    let document = f.install.root().join("project-inventory.json");
    let bytes = std::fs::metadata(&document).unwrap().len() as usize;
    std::fs::remove_file(&document).unwrap();
    let mut bounded = limits();
    bounded.inventory_bytes = bytes;
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, bounded)
        .await
        .unwrap_err();
    assert!(matches!(
        failure.cause,
        KnowledgeInventoryCause::Inventory(_)
    ));
    assert!(failure.publication.as_ref().unwrap().named.is_some());
    assert_eq!(
        failure.retained,
        [RetainedVersion {
            version: "main".into(),
            change: VersionRetention::Retained
        }]
    );
    assert_eq!(f.versions(ctx.snapshot()), ["main"]);
    assert!(
        f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
}

#[tokio::test]
async fn first_inventory_commit_failure_keeps_confirmed_sdk_generation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
    let mut bounded = limits();
    bounded.inventory_bytes = 1;
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, bounded)
        .await
        .unwrap_err();
    assert!(matches!(
        failure.cause,
        KnowledgeInventoryCause::Inventory(_)
    ));
    assert!(failure.publication.is_some());
    assert!(failure.retained.is_empty());
    assert!(f.versions(ctx.snapshot()).is_empty());
    assert!(
        f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .exists()
    );
}

#[tokio::test]
async fn real_git_worktrees_pin_and_inherited_read_do_not_infer_retention() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::with_version(&ctx, "");
    git(&f.root, &["init", "-b", "main"]);
    git(&f.root, &["add", ".knowledge-test/project.json"]);
    git(&f.root, &["commit", "-m", "fixture"]);
    let checkout = GitCheckout::inspect(&f.root, Duration::from_secs(10), &cancel).unwrap();
    f.version = VersionContext::select(&f.paths, checkout.as_checkout(), "", None, "").unwrap();
    f.publish(&ctx, &checkout, limits()).await.unwrap();
    let inherited =
        VersionContext::select(&f.paths, checkout.as_checkout(), "future", None, "").unwrap();
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue_for(&issuer, &parent, &inherited);
    let scope = f.scope_for(&inherited);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let KnowledgeIndexRead::Published { generation, .. } =
        read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("inherited corpus");
    };
    assert_eq!(
        generation.inherited_target.as_deref(),
        Some(inherited.lance_branch().as_str())
    );
    assert_eq!(f.versions(ctx.snapshot()), ["main"]);
    let second = f.root.join("second");
    git(
        &f.root,
        &["worktree", "add", "-b", "feature", second.to_str().unwrap()],
    );
    let paths = ProjectPaths::at(&second, &f.install).unwrap();
    assert_eq!(
        ProjectIdentityStore::at(&paths)
            .load()
            .unwrap()
            .unwrap()
            .id(),
        ProjectIdentityStore::at(&f.paths)
            .load()
            .unwrap()
            .unwrap()
            .id()
    );
    let second_checkout = GitCheckout::inspect(&second, Duration::from_secs(10), &cancel).unwrap();
    let pin = ScopedPin::new(&paths, "pinned/β").unwrap();
    let selection =
        VersionContext::select(&paths, second_checkout.as_checkout(), "", Some(&pin), "").unwrap();
    let captured = CapturedPublicationScope::project(
        &paths,
        ProjectIdentityStore::at(&paths)
            .load()
            .unwrap()
            .unwrap()
            .id(),
        ScopeLimits::new(16, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let record = captured.binding().unwrap();
    // Auth authorizes exact configured roots, not lexical descendants. Use
    // its real realm factory with both observed temporary worktree roots.
    let installation = CapturedPublicationScope::installation(
        &f.install,
        InstallationId::new("local-installation").unwrap(),
        [f.root.clone(), second.clone()],
        ScopeLimits::new(16, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let installation_record = installation.binding().unwrap();
    let realm = ScopeCapture {
        live: &installation,
        record: &installation_record,
    };
    let issuer = f.issuer_for(&ctx, realm);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let scope = ScopeInput {
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version: &selection,
        }),
        installation: realm,
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let (bearer, reference) = f.issue_scope(&issuer, &parent, scope);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let docs = [page("second", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    let input = KnowledgeInventoryInput {
        project: &paths,
        checkout: &second_checkout,
        installation: f.private(ctx.snapshot()),
    };
    // Both worktrees share the published corpus. First publication cannot adopt
    // that Main or create a pinned branch from it; only confirmed writes retain.
    let failure = publish_knowledge_and_retain(input, limits(), ctx.snapshot(), &write, &rows)
        .await
        .unwrap_err();
    assert!(matches!(
        failure.cause,
        KnowledgeInventoryCause::Data(KnowledgeIndexError::InitialTargetChanged)
    ));
    assert!(failure.publication.is_none());
    assert!(failure.retained.is_empty());
    assert!(failure.retention_failure.is_none());
    assert_eq!(f.versions(ctx.snapshot()), ["main"]);
    let pinned_read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let KnowledgeIndexRead::Published {
        generation: pinned_generation,
        ..
    } = read_native_knowledge_index(ctx.snapshot(), &pinned_read, read_limits())
        .await
        .unwrap()
    else {
        panic!("pinned view must retain the original inherited corpus");
    };
    assert_eq!(pinned_generation.version, generation.version);
    assert!(pinned_generation.physical_branch.is_none());
    assert_eq!(
        pinned_generation.inherited_target.as_deref(),
        Some(selection.lance_branch().as_str())
    );
    assert_eq!(f.versions(ctx.snapshot()), ["main"]);
}

#[tokio::test]
async fn original_cancel_expiry_replaced_identity_and_wrong_brand_prevent_sdk_effects() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
    let other = Fixture::with_version(&ctx, "");
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
    let docs = [page("document", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    let expired = Arc::new(SnapshotContext::new(
        Instant::now() - Duration::from_secs(1),
        Cancellation::default(),
    ));
    let failure = publish_knowledge_and_retain(
        f.input(&GitCheckout::NonGit, ctx.snapshot()),
        limits(),
        &expired,
        &write,
        &rows,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&failure.cause, KnowledgeInventoryCause::Request(e) if e.kind() == std::io::ErrorKind::TimedOut)
    );
    let wrong_brand = InstallationPaths::resolve(
        Brand::new("other-brand").unwrap(),
        &f.root,
        None,
        Some(f.install.root()),
    )
    .unwrap();
    let installation = installation_bootstrap::prepare(
        &wrong_brand,
        BootstrapLimits::new(65536, 256).unwrap(),
        ctx.snapshot(),
    )
    .into_installation()
    .unwrap();
    let input = KnowledgeInventoryInput {
        project: &f.paths,
        checkout: &GitCheckout::NonGit,
        installation,
    };
    let failure = publish_knowledge_and_retain(input, limits(), ctx.snapshot(), &write, &rows)
        .await
        .unwrap_err();
    assert!(matches!(failure.cause, KnowledgeInventoryCause::Capture(_)));
    std::fs::copy(other.paths.identity_file(), f.paths.identity_file()).unwrap();
    let failure = publish_knowledge_and_retain(
        f.input(&GitCheckout::NonGit, ctx.snapshot()),
        limits(),
        ctx.snapshot(),
        &write,
        &rows,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&failure.cause, KnowledgeInventoryCause::Capture(e) if e.kind() == std::io::ErrorKind::InvalidData)
    );
    let input = f.input(&GitCheckout::NonGit, ctx.snapshot());
    cancel.cancel();
    let failure = publish_knowledge_and_retain(input, limits(), ctx.snapshot(), &write, &rows)
        .await
        .unwrap_err();
    assert!(
        matches!(&failure.cause, KnowledgeInventoryCause::Request(e) if e.kind() == std::io::ErrorKind::Interrupted)
    );
    assert!(!f.project.runtime_root().join("knowledge").exists());
    assert!(!f.install.root().join("project-inventory.json").exists());
}

#[tokio::test]
async fn original_cancellation_during_real_inventory_contention_keeps_sdk_effects() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
    // An independent actual writer holds the native lock. The consumer itself
    // acquires no inventory writer until its SDK future has completed.
    let held = f
        .store()
        .writer(f.private(ctx.snapshot()), ctx.snapshot())
        .unwrap();
    let corpus = f
        .project
        .runtime_root()
        .join("knowledge/index.lance/_versions");
    let cancellation = cancel.clone();
    let observer = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            if std::fs::read_dir(&corpus).ok().is_some_and(|mut entries| {
                entries.any(|entry| {
                    entry.is_ok_and(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|ext| ext == "manifest")
                    })
                })
            }) {
                std::thread::sleep(Duration::from_millis(200));
                cancellation.cancel();
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        cancellation.cancel();
        panic!("actual SDK manifest was not observed");
    });
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap_err();
    observer.join().unwrap();
    drop(held);
    assert!(
        failure.publication.is_some()
            || matches!(
                &failure.cause,
                KnowledgeInventoryCause::Data(KnowledgeIndexError::PartialDefault { .. })
            )
    );
    assert!(failure.retained.is_empty());
    assert!(
        f.project
            .runtime_root()
            .join("knowledge/index.lance/_versions")
            .is_dir()
    );
    assert!(!f.install.root().join("project-inventory.json").exists());
}

#[tokio::test]
async fn original_deadline_during_real_inventory_contention_keeps_sdk_generation() {
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
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let f = Fixture::with_version(&ctx, "");
    let held = f
        .store()
        .writer(f.private(ctx.snapshot()), ctx.snapshot())
        .unwrap();
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap_err();
    drop(held);
    assert!(failure.publication.is_some());
    assert!(
        matches!(&failure.cause, KnowledgeInventoryCause::Inventory(e) | KnowledgeInventoryCause::Request(e) if e.kind() == std::io::ErrorKind::TimedOut)
    );
    assert!(failure.retained.is_empty());
    assert!(
        f.project
            .runtime_root()
            .join("knowledge/index.lance/_versions")
            .is_dir()
    );
    assert!(!f.install.root().join("project-inventory.json").exists());
}

#[tokio::test]
async fn readonly_installation_after_real_sdk_publication_preserves_effects() {
    use std::os::unix::fs::PermissionsExt;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
    let held = f
        .store()
        .writer(f.private(ctx.snapshot()), ctx.snapshot())
        .unwrap();
    let corpus = f
        .project
        .runtime_root()
        .join("knowledge/index.lance/_versions");
    let installation = f.install.root().to_owned();
    let observer = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            if std::fs::read_dir(&corpus).ok().is_some_and(|mut entries| {
                entries.any(|entry| {
                    entry.is_ok_and(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|ext| ext == "manifest")
                    })
                })
            }) {
                std::thread::sleep(Duration::from_millis(200));
                std::fs::set_permissions(installation, std::fs::Permissions::from_mode(0o500))
                    .unwrap();
                drop(held);
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("actual SDK manifest was not observed");
    });
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap_err();
    observer.join().unwrap();
    std::fs::set_permissions(f.install.root(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        failure.publication.is_some()
            || matches!(
                &failure.cause,
                KnowledgeInventoryCause::Data(KnowledgeIndexError::PartialDefault { .. })
            )
    );
    assert!(failure.retained.is_empty());
    assert!(
        f.project
            .runtime_root()
            .join("knowledge/index.lance/_versions")
            .is_dir()
    );
    assert!(!f.install.root().join("project-inventory.json").exists());
}

#[test]
fn real_role_revocation_after_sdk_prevents_retention() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
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
    let docs = [page("document", &[])];
    let rows = h::prepare(ctx.snapshot(), &docs, &[], hlimits()).unwrap();
    let held = f
        .store()
        .writer(f.private(ctx.snapshot()), ctx.snapshot())
        .unwrap();
    let corpus = f
        .project
        .runtime_root()
        .join("knowledge/index.lance/_versions");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    std::thread::scope(|scope| {
        let issuer = &issuer;
        let parent = &parent;
        let bearer = &bearer;
        let observer = scope.spawn(move || {
            let until = Instant::now() + Duration::from_secs(30);
            while Instant::now() < until {
                if std::fs::read_dir(&corpus).ok().is_some_and(|mut entries| {
                    entries.any(|entry| {
                        entry.is_ok_and(|entry| {
                            entry
                                .path()
                                .extension()
                                .is_some_and(|ext| ext == "manifest")
                        })
                    })
                }) {
                    std::thread::sleep(Duration::from_millis(200));
                    issuer.revoke(parent, bearer).unwrap();
                    drop(held);
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            panic!("actual SDK manifest was not observed");
        });
        let failure = runtime
            .block_on(publish_knowledge_and_retain(
                f.input(&GitCheckout::NonGit, ctx.snapshot()),
                limits(),
                ctx.snapshot(),
                &write,
                &rows,
            ))
            .unwrap_err();
        observer.join().unwrap();
        assert!(failure.publication.is_some());
        assert!(matches!(
            failure.cause,
            KnowledgeInventoryCause::Admission(AuthError::Denied)
        ));
        assert!(failure.retained.is_empty());
        assert!(
            f.project
                .runtime_root()
                .join("knowledge/index.lance/_versions")
                .is_dir()
        );
        assert!(!f.install.root().join("project-inventory.json").exists());
    });
}

#[tokio::test]
async fn actual_identity_change_after_sdk_keeps_corpus_and_rejects_stale_capture() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::with_version(&ctx, "");
    let held = f
        .store()
        .writer(f.private(ctx.snapshot()), ctx.snapshot())
        .unwrap();
    let corpus = f
        .project
        .runtime_root()
        .join("knowledge/index.lance/_versions");
    let paths = f.paths.clone();
    let cancellation = cancel.clone();
    let observer = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            if std::fs::read_dir(&corpus).ok().is_some_and(|mut entries| {
                entries.any(|entry| {
                    entry.is_ok_and(|entry| {
                        entry
                            .path()
                            .extension()
                            .is_some_and(|ext| ext == "manifest")
                    })
                })
            }) {
                std::thread::sleep(Duration::from_millis(200));
                let identity = ProjectIdentityStore::at(&paths).load().unwrap().unwrap();
                let mut metadata = identity.metadata().clone();
                metadata.name = "changed".into();
                ProjectIdentityStore::at(&paths)
                    .writer(Duration::from_secs(5), &cancellation)
                    .unwrap()
                    .update(identity.id(), &metadata)
                    .unwrap();
                drop(held);
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("actual SDK manifest was not observed");
    });
    let failure = f
        .publish(&ctx, &GitCheckout::NonGit, limits())
        .await
        .unwrap_err();
    observer.join().unwrap();
    assert!(failure.publication.is_some());
    assert!(
        matches!(&failure.cause, KnowledgeInventoryCause::Inventory(e) if e.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(failure.retained.is_empty());
    assert!(
        f.project
            .runtime_root()
            .join("knowledge/index.lance/_versions")
            .is_dir()
    );
    assert!(!f.install.root().join("project-inventory.json").exists());
}

fn git(root: &std::path::Path, args: &[&str]) {
    let mut command = std::process::Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            command.env_remove(key);
        }
    }
    let result = command
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            if cfg!(windows) {
                "core.hooksPath=NUL"
            } else {
                "core.hooksPath=/dev/null"
            },
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "gc.auto=0",
        ])
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
