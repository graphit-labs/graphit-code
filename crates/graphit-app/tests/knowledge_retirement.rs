//! Real App authority, retained supervisor cleanup and native Knowledge effects.
#![cfg(target_os = "linux")]
use graphit_app::knowledge_retirement::*;
use graphit_auth::*;
use graphit_auth::{
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
    git_checkout::GitCheckout,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata},
    project_inventory::{CapturedProjectInventory, GlobalProjectInventory},
    project_paths::{InstallationPaths, ProjectPaths},
    version_context::{Checkout, VersionContext},
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    supervisor::{DesiredProject, ProjectContext, ProjectSupervisor, ProjectSupervisors},
    watcher::{IgnoreConfig, WatchConfig},
};
use graphit_data::{
    knowledge_document_projection as h, knowledge_embeddings::*, knowledge_index::*,
};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use std::{
    collections::HashMap,
    fs, io,
    num::{NonZeroU32, NonZeroUsize},
    path::Path,
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot};
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
fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 256).unwrap()
}
fn bootstrap() -> BootstrapLimits {
    BootstrapLimits::new(65536, 256).unwrap()
}
fn documents() -> AuthDocumentLimits {
    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn profiles() -> ProfileLimits {
    ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn permissions() -> Vec<Permission> {
    vec![
        Permission::Configure,
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
    ]
}
fn authority_policy(perms: &[Permission]) -> IssuerAuthorityPolicy {
    let source = SourceAuthorityPolicy {
        ordinary: perms,
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    IssuerAuthorityPolicy::new(
        source,
        source,
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
    .unwrap()
}
fn ttl() -> TtlPolicy {
    TtlPolicy {
        ttl: Duration::from_secs(900),
        maximum: Duration::from_secs(3600),
        refresh_margin: Duration::from_secs(120),
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    install: InstallationPaths,
    paths: ProjectPaths,
    id: ProjectId,
    auth: DaemonAuth,
    project: CapturedPublicationScope,
    record: ScopeBindingRecord,
    version: VersionContext,
}
impl Fixture {
    fn new(ctx: &RequestContext<'_>, version: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let install = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let paths = ProjectPaths::at(&root, &install).unwrap();
        let id = ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), ctx.snapshot().cancellation())
            .unwrap()
            .ensure(&ProjectMetadata::new("fixture").unwrap())
            .unwrap()
            .id();
        let auth = DaemonAuth::start(
            &install,
            std::slice::from_ref(&paths),
            StartupPolicy {
                bootstrap: bootstrap(),
                runtime: graphit_core::runtime_key::RuntimeKeyLimits::new(65536, 256, 65536)
                    .unwrap(),
                scope: scopes(),
                document: documents(),
                profiles: profiles(),
                authority: authority_policy(&permissions()),
                ttl: ttl(),
            },
            ctx,
        )
        .unwrap();
        let project =
            CapturedPublicationScope::project(&paths, id, scopes(), ctx.snapshot()).unwrap();
        let record = project.binding().unwrap();
        let version = VersionContext::select(&paths, Checkout::NonGit, version, None, "").unwrap();
        let f = Self {
            _temp: temp,
            install,
            paths,
            id,
            auth,
            project,
            record,
            version,
        };
        let checkout = GitCheckout::inspect(
            f.paths.root(),
            Duration::from_secs(5),
            ctx.snapshot().cancellation(),
        )
        .unwrap();
        let capture = CapturedProjectInventory::capture_in_scope(
            &f.paths,
            &checkout,
            &f.version,
            &f.project,
            native(),
            ctx.snapshot(),
        )
        .unwrap();
        let private = installation_bootstrap::prepare(&f.install, bootstrap(), ctx.snapshot())
            .into_installation()
            .unwrap();
        let mut writer = f.store().writer(private, ctx.snapshot()).unwrap();
        writer.register_checkout(&capture).unwrap();
        writer
            .retain_version(&capture, f.other().version())
            .unwrap();
        let default = VersionContext::select(&f.paths, Checkout::NonGit, "", None, "").unwrap();
        writer.retain_version(&capture, default.version()).unwrap();
        drop(writer);
        f
    }
    fn store(&self) -> GlobalProjectInventory {
        GlobalProjectInventory::at(&self.install, 65536).unwrap()
    }
    fn metadata(&self) -> Vec<u8> {
        fs::read(self.install.root().join("project-inventory.json")).unwrap()
    }
    fn other(&self) -> VersionContext {
        VersionContext::select(&self.paths, Checkout::NonGit, "B", None, "").unwrap()
    }
    fn scope(&self) -> ScopeInput<'_> {
        self.scope_for(&self.version)
    }
    fn scope_for<'a>(&'a self, version: &'a VersionContext) -> ScopeInput<'a> {
        ScopeInput {
            installation: self.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.record,
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
            self.auth
                .runtime()
                .check(ctx.snapshot())
                .unwrap()
                .secret(ctx.snapshot())
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }
    fn issuer(&self, ctx: &RequestContext<'_>, perms: &[Permission]) -> RoleIssuer {
        let local = AuthStore::new(
            AuthDocumentStore::capture(
                self.auth.installation().live.clone(),
                documents(),
                ctx.snapshot(),
            )
            .unwrap(),
            profiles(),
            ctx,
        )
        .unwrap();
        RoleIssuer::new(
            self.auth.installation(),
            self.auth.runtime().clone(),
            Arc::new(local),
            authority_policy(perms),
            ttl(),
            ctx,
        )
        .unwrap()
    }
}
fn watch(root: &Path) -> WatchConfig {
    WatchConfig {
        root: root.to_owned(),
        ast: IgnoreConfig::default(),
        wiki: IgnoreConfig::default(),
        control_files: Vec::new(),
        control_directories: Vec::new(),
        debounce: Duration::from_millis(20),
    }
}
struct Held {
    registry: ProjectSupervisors,
    cancelled: mpsc::UnboundedReceiver<()>,
    release: oneshot::Sender<()>,
    sibling_events: mpsc::UnboundedReceiver<()>,
}
async fn held(f: &Fixture, fail: bool) -> Held {
    let (cancelled, observed) = mpsc::unbounded_channel();
    let (release, cleanup) = oneshot::channel();
    let mut cleanup = Some(cleanup);
    let (entered, waiting) = oneshot::channel();
    let mut entered = Some(entered);
    let a = ProjectSupervisor::start(
        ProjectContext {
            id: f.id,
            version: f.version.clone(),
        },
        watch(f.paths.root()),
        move |_, _, stop| {
            let cleanup = cleanup.take().unwrap();
            let entered = entered.take().unwrap();
            let cancelled = cancelled.clone();
            async move {
                entered.send(()).unwrap();
                stop.cancelled().await;
                cancelled.send(()).unwrap();
                cleanup.await.unwrap();
                if fail {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "actual producer cleanup failed",
                    ))
                } else {
                    Ok(())
                }
            }
        },
    )
    .await
    .unwrap();
    waiting.await.unwrap();
    let (events, sibling_events) = mpsc::unbounded_channel();
    let b = ProjectSupervisor::start(
        ProjectContext {
            id: f.id,
            version: f.other(),
        },
        watch(f.paths.root()),
        move |_, _, _| {
            let events = events.clone();
            async move {
                let _ = events.send(());
                Ok(())
            }
        },
    )
    .await
    .unwrap();
    let mut owners = HashMap::from([(a.context().key(), a), (b.context().key(), b)]);
    let desired = owners
        .values()
        .map(|s| DesiredProject {
            context: s.context().clone(),
            active: true,
        })
        .collect();
    let mut registry = ProjectSupervisors::default();
    registry
        .reconcile(desired, |ctx| {
            std::future::ready(Ok(owners.remove(&ctx.key()).unwrap()))
        })
        .await
        .unwrap();
    Held {
        registry,
        cancelled: observed,
        release,
        sibling_events,
    }
}
fn io_kind(error: &(dyn std::error::Error + 'static)) -> Option<io::ErrorKind> {
    if let Some(e) = error.downcast_ref::<io::Error>() {
        return Some(e.kind());
    }
    if let Some(e) = error.downcast_ref::<graphit_core::artifact_store::ArtifactError>() {
        return Some(e.kind());
    }
    error.source().and_then(io_kind)
}
async fn seed<'a>(
    f: &'a Fixture,
    ctx: &'a RequestContext<'a>,
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    semantic: bool,
) -> KnowledgeFirstPublication {
    let write = authority
        .admit_authenticated_write(caller, ctx, f.scope(), ProjectWriteOperation::Knowledge)
        .unwrap();
    let first = Box::pin(publish(ctx, &write)).await;
    if semantic {
        let read = authority
            .admit_authenticated_read(caller, ctx, f.scope(), ProjectReadModule::Knowledge)
            .unwrap();
        Box::pin(publish_semantic(ctx, &write, &read, f)).await;
    }
    let other = f.other();
    let write = authority
        .admit_authenticated_write(
            caller,
            ctx,
            f.scope_for(&other),
            ProjectWriteOperation::Knowledge,
        )
        .unwrap();
    // The first-publication boundary cannot create a second default corpus.
    // Prepare sibling B with the public native branch foundation under its writer.
    write.recheck().unwrap();
    let project = write.project().unwrap();
    let limits = write_limits();
    let store = graphit_core::artifact_store::ArtifactStore::new(
        project.scope.live.clone(),
        limits.artifacts,
    );
    let writer = store.writer_with_context(ctx.snapshot()).unwrap();
    let branches = graphit_data::lance_branch_store::LanceBranchStore::open(
        &first.default.corpus_uri,
        physical_schema(),
        None,
    )
    .await
    .unwrap();
    let branch = project.version.lance_branch();
    let target = branches.capture_target(Some(&branch)).await.unwrap();
    assert_eq!(target.inherited_target(), Some(branch.as_str()));
    let target = target.materialize().await.unwrap();
    assert_eq!(target.physical_branch(), Some(branch.as_str()));
    drop(target);
    drop(branches);
    drop(writer);
    write.recheck().unwrap();
    first
}
async fn read_index<'a>(
    f: &'a Fixture,
    ctx: &'a RequestContext<'a>,
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
    version: &'a VersionContext,
) -> KnowledgeIndexRead {
    let read = authority
        .admit_authenticated_read(
            caller,
            ctx,
            f.scope_for(version),
            ProjectReadModule::Knowledge,
        )
        .unwrap();
    read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
}

async fn semantic_generation<'a>(
    f: &'a Fixture,
    ctx: &'a RequestContext<'a>,
    authority: &'a ProjectRecordAuthority<'_>,
    caller: &'a AuthenticatedCaller<'a>,
) -> KnowledgeEmbeddingGeneration {
    let read = authority
        .admit_authenticated_read(caller, ctx, f.scope(), ProjectReadModule::Knowledge)
        .unwrap();
    let KnowledgeEmbeddingRead::Published(capture) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("fixture requires native index")
    };
    let generation = capture
        .semantic_generation()
        .expect("fixture requires native E-A");
    drop(capture);
    generation
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
    f: &Fixture,
) -> (KnowledgeGeneration, String) {
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
    drop(capture);
    let uri = url::Url::from_file_path(f.project.runtime_root().join("knowledge/index.lance"))
        .unwrap()
        .to_string();
    (
        KnowledgeGeneration {
            corpus_uri: uri.clone(),
            version: generation.version,
            physical_branch: generation.physical_branch,
            inherited_target: generation.inherited_target,
        },
        uri,
    )
}

fn publish_local(auth: &DaemonAuth, paths: &InstallationPaths, ctx: &RequestContext<'_>) {
    let id = InstallationIdentityStore::new(paths)
        .unwrap()
        .load()
        .unwrap()
        .unit_id()
        .unwrap()
        .clone();
    let scope = CapturedPublicationScope::installation(
        paths,
        id,
        [paths.root().to_owned()],
        scopes(),
        ctx.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(scope, documents(), ctx.snapshot()).unwrap(),
        profiles(),
        ctx,
    )
    .unwrap();
    let before = store.load(ctx).unwrap();
    let result = store
        .mutate(before, ctx, |doc, ctx| {
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
                    name: "actual-local".into(),
                    provider: "local".into(),
                    username: "tester".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new("test-local-key".into())),
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
            doc.activate("actual-local", doc.revision(), ctx)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert!(result.failure().is_none());
    let ScopeCapture { live, record } = auth.installation();
    assert!(live.binding().unwrap() == *record);
}

#[test]
fn runtime_cleanup_precedes_single_corpus_retirement_and_keeps_sibling_and_core() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "A");
        let issuer = f.auth.issuer();
        let caller = issuer.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        seed(&f, &ctx, &authority, &caller, true).await;
        let before_semantic = semantic_generation(&f, &ctx, &authority, &caller).await;
        let other = f.other();
        let default = VersionContext::select(&f.paths, Checkout::NonGit, "", None, "").unwrap();
        let before_b = read_index(&f, &ctx, &authority, &caller, &other).await;
        let before_default = read_index(&f, &ctx, &authority, &caller, &default).await;
        let metadata = f.metadata();
        let Held {
            mut registry,
            mut cancelled,
            release,
            mut sibling_events,
        } = held(&f, false).await;
        let mut release = Some(release);
        let driver = async {
            cancelled.recv().await.unwrap();
            let KnowledgeIndexRead::Published { generation, .. } =
                read_index(&f, &ctx, &authority, &caller, &f.version).await
            else {
                panic!("I must remain until real cleanup joins")
            };
            assert_eq!(generation.version, before_semantic.version);
            assert_eq!(
                before_semantic,
                semantic_generation(&f, &ctx, &authority, &caller).await
            );
            release.take().unwrap().send(()).unwrap();
        };
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                retire_knowledge_selection(
                    &authority,
                    &caller,
                    f.scope(),
                    &mut registry,
                    retirement_limits(),
                    &ctx
                ),
                driver
            )
        })
        .await;
        let (result, ()) = match completed {
            Ok(result) => result,
            Err(_) => {
                if let Some(release) = release.take() {
                    let _ = release.send(());
                }
                let _ = tokio::time::timeout(Duration::from_secs(5), registry.shutdown()).await;
                panic!("retirement did not complete its observed cleanup journey");
            }
        };
        let result = result.unwrap();
        let KnowledgeBranchRetirement::Retired { generation } = result.index else {
            panic!("the single documentary/vector corpus must retire once")
        };
        assert_eq!(generation.version, before_semantic.version);
        assert_eq!(generation.physical_branch, before_semantic.physical_branch);
        assert_eq!(
            generation.inherited_target,
            before_semantic.inherited_target
        );
        assert_eq!(registry.supervised().count(), 1);
        assert_eq!(
            registry.supervised().next().unwrap().version.version(),
            other.version()
        );
        assert_eq!(
            before_b,
            read_index(&f, &ctx, &authority, &caller, &other).await
        );
        assert_eq!(
            before_default,
            read_index(&f, &ctx, &authority, &caller, &default).await
        );
        assert_eq!(metadata, f.metadata());
        while sibling_events.try_recv().is_ok() {}
        fs::write(f.paths.root().join("sibling-still-produces.md"), b"event").unwrap();
        tokio::time::timeout(Duration::from_secs(5), sibling_events.recv())
            .await
            .unwrap()
            .unwrap();
        registry.shutdown().await.unwrap();
    });
}

#[test]
fn independent_permission_denials_and_foreign_lineage_precede_stop() {
    native_test(async {
        for case in 0..4 {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, "A");
            let full = f.auth.issuer();
            let caller = full.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(full);
            seed(&f, &ctx, &authority, &caller, true).await;
            let before = read_index(&f, &ctx, &authority, &caller, &f.version).await;
            let before_e = semantic_generation(&f, &ctx, &authority, &caller).await;
            let metadata = f.metadata();
            let Held {
                mut registry,
                mut cancelled,
                release,
                ..
            } = held(&f, false).await;
            let restricted = f.issuer(
                &ctx,
                &match case {
                    0 => vec![Permission::MutateModule(Module::Knowledge)],
                    1 => vec![Permission::Configure],
                    _ => permissions(),
                },
            );
            let restricted_caller = restricted
                .authenticate_runtime(&f.secret(&ctx), &ctx)
                .unwrap();
            let restricted_authority = ProjectRecordAuthority::new(&restricted);
            let foreign_ctx = request(&cancel);
            let error = match case {
                0 | 1 => {
                    retire_knowledge_selection(
                        &restricted_authority,
                        &restricted_caller,
                        f.scope(),
                        &mut registry,
                        retirement_limits(),
                        &ctx,
                    )
                    .await
                }
                2 => {
                    retire_knowledge_selection(
                        &authority,
                        &caller,
                        f.scope(),
                        &mut registry,
                        retirement_limits(),
                        &foreign_ctx,
                    )
                    .await
                }
                _ => {
                    retire_knowledge_selection(
                        &restricted_authority,
                        &caller,
                        f.scope(),
                        &mut registry,
                        retirement_limits(),
                        &ctx,
                    )
                    .await
                }
            }
            .unwrap_err();
            assert_eq!(error.phase(), KnowledgeRetirementPhase::Admission);
            assert!(matches!(
                error.error(),
                KnowledgeSelectionRetirementError::Admission(_)
            ));
            assert!(error.index().is_none());
            assert!(matches!(
                cancelled.try_recv(),
                Err(mpsc::error::TryRecvError::Empty)
            ));
            assert_eq!(registry.supervised().count(), 2);
            assert_eq!(metadata, f.metadata());
            assert_eq!(
                before,
                read_index(&f, &ctx, &authority, &caller, &f.version).await
            );
            assert_eq!(
                before_e,
                semantic_generation(&f, &ctx, &authority, &caller).await
            );
            release.send(()).unwrap();
            registry.shutdown().await.unwrap();
        }
    });
}

#[test]
fn default_and_initial_cancellation_do_not_signal_owner() {
    native_test(async {
        for cancelled_request in [false, true] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, if cancelled_request { "A" } else { "" });
            let issuer = f.auth.issuer();
            let caller = issuer.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(issuer);
            let metadata = f.metadata();
            let Held {
                mut registry,
                mut cancelled,
                release,
                ..
            } = held(&f, false).await;
            if cancelled_request {
                cancel.cancel();
            }
            let error = retire_knowledge_selection(
                &authority,
                &caller,
                f.scope(),
                &mut registry,
                retirement_limits(),
                &ctx,
            )
            .await
            .unwrap_err();
            assert_eq!(error.phase(), KnowledgeRetirementPhase::Admission);
            if cancelled_request {
                assert_eq!(io_kind(&error), Some(io::ErrorKind::Interrupted));
            } else {
                assert!(
                    matches!(error.error(), KnowledgeSelectionRetirementError::Admission(e) if matches!(e.as_ref(), KnowledgeIndexError::Branch(graphit_data::lance_branch_store::BranchError::DefaultSnapshot)))
                );
            }
            assert!(matches!(
                cancelled.try_recv(),
                Err(mpsc::error::TryRecvError::Empty)
            ));
            assert_eq!(registry.supervised().count(), 2);
            assert_eq!(metadata, f.metadata());
            assert!(
                !f.project
                    .runtime_root()
                    .join("knowledge/index.lance")
                    .exists()
            );
            release.send(()).unwrap();
            registry.shutdown().await.unwrap();
        }
    });
}

#[test]
fn actual_cleanup_error_retains_owner_and_blocks_data() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "A");
        let issuer = f.auth.issuer();
        let caller = issuer.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        seed(&f, &ctx, &authority, &caller, true).await;
        let before = read_index(&f, &ctx, &authority, &caller, &f.version).await;
        let before_e = semantic_generation(&f, &ctx, &authority, &caller).await;
        let metadata = f.metadata();
        let Held {
            mut registry,
            mut cancelled,
            release,
            ..
        } = held(&f, true).await;
        let mut release = Some(release);
        let driver = async {
            cancelled.recv().await.unwrap();
            release.take().unwrap().send(()).unwrap();
        };
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                retire_knowledge_selection(
                    &authority,
                    &caller,
                    f.scope(),
                    &mut registry,
                    retirement_limits(),
                    &ctx
                ),
                driver
            )
        })
        .await;
        let (result, ()) = match completed {
            Ok(result) => result,
            Err(_) => {
                if let Some(release) = release.take() {
                    let _ = release.send(());
                }
                let _ = tokio::time::timeout(Duration::from_secs(5), registry.shutdown()).await;
                panic!("retirement did not complete its observed cleanup journey");
            }
        };
        let error = result.unwrap_err();
        assert_eq!(error.phase(), KnowledgeRetirementPhase::Stop);
        assert!(
            matches!(error.error(), KnowledgeSelectionRetirementError::Stop(e) if e.kind() == io::ErrorKind::PermissionDenied && e.to_string() == "actual producer cleanup failed")
        );
        assert!(error.index().is_none());
        assert_eq!(registry.supervised().count(), 2);
        assert_eq!(metadata, f.metadata());
        assert_eq!(
            before,
            read_index(&f, &ctx, &authority, &caller, &f.version).await
        );
        assert_eq!(
            before_e,
            semantic_generation(&f, &ctx, &authority, &caller).await
        );
        registry.shutdown().await.unwrap();
    });
}

#[test]
fn cancellation_and_deadline_during_join_wait_for_cleanup_then_block_data() {
    native_test(async {
        for timeout in [false, true] {
            let setup_cancel = Cancellation::default();
            let setup = request(&setup_cancel);
            let f = Fixture::new(&setup, "A");
            let issuer = f.auth.issuer();
            let setup_caller = issuer
                .authenticate_runtime(&f.secret(&setup), &setup)
                .unwrap();
            let authority = ProjectRecordAuthority::new(issuer);
            seed(&f, &setup, &authority, &setup_caller, true).await;
            let before = read_index(&f, &setup, &authority, &setup_caller, &f.version).await;
            let before_e = semantic_generation(&f, &setup, &authority, &setup_caller).await;
            let metadata = f.metadata();
            let Held {
                mut registry,
                mut cancelled,
                release,
                ..
            } = held(&f, false).await;
            let cancel = Cancellation::default();
            let deadline = Instant::now() + Duration::from_secs(2);
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
                deadline,
            )
            .unwrap();
            let caller = issuer.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let mut release = Some(release);
            let driver = async {
                cancelled.recv().await.unwrap();
                if timeout {
                    tokio::time::sleep_until(deadline.into()).await;
                } else {
                    cancel.cancel();
                }
                assert_eq!(metadata, f.metadata());
                release.take().unwrap().send(()).unwrap();
            };
            let completed = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::join!(
                    retire_knowledge_selection(
                        &authority,
                        &caller,
                        f.scope(),
                        &mut registry,
                        retirement_limits(),
                        &ctx
                    ),
                    driver
                )
            })
            .await;
            let (result, ()) = match completed {
                Ok(result) => result,
                Err(_) => {
                    if let Some(release) = release.take() {
                        let _ = release.send(());
                    }
                    let _ = tokio::time::timeout(Duration::from_secs(5), registry.shutdown()).await;
                    panic!("retirement did not complete its observed cleanup journey");
                }
            };
            let error = result.unwrap_err();
            assert_eq!(error.phase(), KnowledgeRetirementPhase::Index);
            assert_eq!(
                io_kind(&error),
                Some(if timeout {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::Interrupted
                })
            );
            assert!(error.index().is_none());
            assert_eq!(registry.supervised().count(), 1);
            assert_eq!(metadata, f.metadata());
            assert_eq!(
                before,
                read_index(&f, &setup, &authority, &setup_caller, &f.version).await
            );
            assert_eq!(
                before_e,
                semantic_generation(&f, &setup, &authority, &setup_caller).await
            );
            registry.shutdown().await.unwrap();
        }
    });
}

#[test]
fn local_auth_rejects_foreign_corpus_file_before_retirement() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx, "A");
        publish_local(&f.auth, &f.install, &ctx);
        let issuer = f.auth.issuer();
        let caller = issuer.authenticate_local("test-local-key", &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        seed(&f, &ctx, &authority, &caller, true).await;
        let metadata = f.metadata();
        let path = f.project.runtime_root().join("knowledge/index.lance");
        fs::remove_dir_all(&path).unwrap();
        fs::write(&path, b"real other file").unwrap();
        let Held {
            mut registry,
            mut cancelled,
            release,
            ..
        } = held(&f, false).await;
        let mut release = Some(release);
        let driver = async {
            cancelled.recv().await.unwrap();
            release.take().unwrap().send(()).unwrap();
        };
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                retire_knowledge_selection(
                    &authority,
                    &caller,
                    f.scope(),
                    &mut registry,
                    retirement_limits(),
                    &ctx
                ),
                driver
            )
        })
        .await;
        let (result, ()) = match completed {
            Ok(result) => result,
            Err(_) => {
                if let Some(release) = release.take() {
                    let _ = release.send(());
                }
                let _ = tokio::time::timeout(Duration::from_secs(5), registry.shutdown()).await;
                panic!("retirement did not complete its observed cleanup journey");
            }
        };
        let error = result.unwrap_err();
        assert_eq!(error.phase(), KnowledgeRetirementPhase::Index);
        assert!(matches!(
            error.error(),
            KnowledgeSelectionRetirementError::Index(_)
        ));
        assert!(error.index().is_none());
        assert!(io_kind(&error).is_some());
        assert!(!format!("{error:?} {error}").contains(f.project.runtime_root().to_str().unwrap()));
        assert_eq!(fs::read(&path).unwrap(), b"real other file");
        assert_eq!(metadata, f.metadata());
        assert_eq!(registry.supervised().count(), 1);
        registry.shutdown().await.unwrap();
    });
}

#[test]
fn absent_corpora_are_not_created_and_native_limit_remains_causal() {
    native_test(async {
        for limited in [false, true] {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            let f = Fixture::new(&ctx, "A");
            let issuer = f.auth.issuer();
            let caller = issuer.authenticate_runtime(&f.secret(&ctx), &ctx).unwrap();
            let authority = ProjectRecordAuthority::new(issuer);
            if limited {
                seed(&f, &ctx, &authority, &caller, true).await;
            }
            let metadata = f.metadata();
            let before = read_index(&f, &ctx, &authority, &caller, &f.version).await;
            let Held {
                mut registry,
                mut cancelled,
                release,
                ..
            } = held(&f, false).await;
            let mut limits = retirement_limits();
            if limited {
                limits.native = NativeDocumentFileLimits::new(65536, 1, 256, 65536).unwrap();
            }
            let mut release = Some(release);
            let driver = async {
                cancelled.recv().await.unwrap();
                release.take().unwrap().send(()).unwrap();
            };
            let completed = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::join!(
                    retire_knowledge_selection(
                        &authority,
                        &caller,
                        f.scope(),
                        &mut registry,
                        limits,
                        &ctx
                    ),
                    driver
                )
            })
            .await;
            let (result, ()) = match completed {
                Ok(result) => result,
                Err(_) => {
                    if let Some(release) = release.take() {
                        let _ = release.send(());
                    }
                    let _ = tokio::time::timeout(Duration::from_secs(5), registry.shutdown()).await;
                    panic!("retirement did not complete its observed cleanup journey");
                }
            };
            if limited {
                let error = result.unwrap_err();
                assert_eq!(error.phase(), KnowledgeRetirementPhase::Index);
                assert!(matches!(
                    error.error(),
                    KnowledgeSelectionRetirementError::Index(_)
                ));
                assert_eq!(io_kind(&error), Some(io::ErrorKind::FileTooLarge));
                assert!(error.index().is_none());
            } else {
                let result = result.unwrap();
                assert_eq!(result.index, KnowledgeBranchRetirement::NoCorpus);
                assert!(
                    !f.project
                        .runtime_root()
                        .join("knowledge/index.lance")
                        .exists()
                );
            }
            assert_eq!(
                before,
                read_index(&f, &ctx, &authority, &caller, &f.version).await
            );
            assert_eq!(metadata, f.metadata());
            assert_eq!(registry.supervised().count(), 1);
            registry.shutdown().await.unwrap();
        }
    });
}
