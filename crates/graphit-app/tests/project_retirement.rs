#![cfg(target_os = "linux")]
use graphit_app::project_retirement::{
    CheckoutCredential, CheckoutRetirementError, CheckoutRetirementInput, retire_checkout,
};
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ScopeCapture,
    ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::{self, BootstrapLimits, PrivateInstallation},
    installation_identity::InstallationIdentityStore,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectId, ProjectIdentityStore, ProjectMetadata},
    project_inventory::{CapturedProjectInventory, GlobalProjectInventory},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyLimits,
    version_context::VersionContext,
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    supervisor::{DesiredProject, ProjectContext, ProjectSupervisor, ProjectSupervisors},
    watcher::{IgnoreConfig, WatchConfig},
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use std::{
    collections::HashMap,
    fs, io,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

fn content() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 65536,
        max_output_bytes: 65536,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 64,
    }
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(content(), cancel, Instant::now() + Duration::from_secs(30)).unwrap()
}
fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 256).unwrap()
}
fn bootstrap() -> BootstrapLimits {
    BootstrapLimits::new(65536, 256).unwrap()
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
fn profiles() -> graphit_auth::profile::ProfileLimits {
    graphit_auth::profile::ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn documents() -> AuthDocumentLimits {
    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn policy(configure: bool) -> StartupPolicy {
    let permissions = if configure {
        vec![Permission::Configure]
    } else {
        vec![]
    };
    let origin = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    StartupPolicy {
        bootstrap: bootstrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope: scopes(),
        document: documents(),
        profiles: profiles(),
        authority: IssuerAuthorityPolicy::new(
            origin(),
            origin(),
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
        .unwrap(),
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    }
}
fn git(root: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os().filter(|(key, _)| key.to_string_lossy().starts_with("GIT_"))
    {
        command.env_remove(key);
    }
    let result = command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
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
struct Fixture {
    _temp: tempfile::TempDir,
    paths: InstallationPaths,
    projects: Vec<ProjectPaths>,
    ids: Vec<ProjectId>,
    versions: Vec<VersionContext>,
    auth: DaemonAuth,
    raw: String,
}
impl Fixture {
    fn new(configure: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let mut projects = Vec::new();
        let mut ids = Vec::new();
        for name in ["a", "b"] {
            let root = temp.path().join(name);
            fs::create_dir(&root).unwrap();
            fs::write(root.join("source"), name).unwrap();
            let project = ProjectPaths::at(&root, &paths).unwrap();
            let id = ProjectIdentityStore::at(&project)
                .writer(Duration::from_secs(5), &cancel)
                .unwrap()
                .ensure(&ProjectMetadata::new(name).unwrap())
                .unwrap()
                .id();
            projects.push(project);
            ids.push(id);
        }
        git(projects[0].root(), &["init", "-b", "main"]);
        git(
            projects[0].root(),
            &["add", ".graphit/project.json", "source"],
        );
        git(projects[0].root(), &["commit", "-m", "initial"]);
        let second = temp.path().join("a-second");
        git(
            projects[0].root(),
            &["worktree", "add", "-b", "feature", second.to_str().unwrap()],
        );
        projects.push(ProjectPaths::at(&second, &paths).unwrap());
        ids.push(ids[0]);
        let auth = DaemonAuth::start(&paths, &projects, policy(configure), &ctx).unwrap();
        let raw = {
            let key = auth.runtime().check(ctx.snapshot()).unwrap();
            String::from_utf8(key.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
        };
        let store = GlobalProjectInventory::at(&paths, 65536).unwrap();
        let mut versions = Vec::new();
        for project in &projects {
            let checkout =
                GitCheckout::inspect(project.root(), Duration::from_secs(5), &cancel).unwrap();
            let version =
                VersionContext::select(project, checkout.as_checkout(), "", None, "").unwrap();
            let capture = CapturedProjectInventory::capture(
                project,
                &checkout,
                &version,
                scopes(),
                native(),
                ctx.snapshot(),
            )
            .unwrap();
            let installation = installation_bootstrap::prepare(&paths, bootstrap(), ctx.snapshot())
                .into_installation()
                .unwrap();
            store
                .writer(installation, ctx.snapshot())
                .unwrap()
                .register_checkout(&capture)
                .unwrap();
            versions.push(version);
        }
        Self {
            _temp: temp,
            paths,
            projects,
            ids,
            versions,
            auth,
            raw,
        }
    }
    fn store(&self) -> GlobalProjectInventory {
        GlobalProjectInventory::at(&self.paths, 65536).unwrap()
    }
    fn installation(&self, ctx: &RequestContext<'_>) -> PrivateInstallation {
        installation_bootstrap::prepare(&self.paths, bootstrap(), ctx.snapshot())
            .into_installation()
            .unwrap()
    }
    fn input<'a>(&'a self, ctx: &RequestContext<'_>) -> CheckoutRetirementInput<'a> {
        CheckoutRetirementInput {
            credential: CheckoutCredential::Runtime(&self.raw),
            installation: self.installation(ctx),
            expected: self.ids[0],
            checkout: self.projects[0].root(),
        }
    }
    fn context(&self, index: usize) -> ProjectContext {
        ProjectContext {
            id: self.ids[index],
            version: self.versions[index].clone(),
        }
    }
    fn registered(&self, ctx: &RequestContext<'_>, index: usize) -> bool {
        self.store()
            .load(ctx.snapshot())
            .unwrap()
            .get(self.ids[index])
            .unwrap()
            .checkouts()
            .any(|c| c.path() == self.projects[index].root())
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
async fn held_registry(
    f: &Fixture,
    fail: bool,
) -> (
    ProjectSupervisors,
    mpsc::UnboundedReceiver<()>,
    oneshot::Sender<()>,
) {
    let (cancelled, observed) = mpsc::unbounded_channel();
    let (release, cleanup) = oneshot::channel();
    let mut cleanup = Some(cleanup);
    let (entered, waiting) = oneshot::channel();
    let mut entered = Some(entered);
    let ctx = f.context(0);
    let owner = ProjectSupervisor::start(ctx, watch(f.projects[0].root()), move |_, _, stop| {
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
                    "producer cleanup failed",
                ))
            } else {
                Ok(())
            }
        }
    })
    .await
    .unwrap();
    waiting.await.unwrap();
    let sibling =
        ProjectSupervisor::start(f.context(1), watch(f.projects[1].root()), |_, _, _| async {
            Ok(())
        })
        .await
        .unwrap();
    let mut owners = HashMap::from([
        (owner.context().key(), owner),
        (sibling.context().key(), sibling),
    ]);
    let desired = owners
        .values()
        .map(|owner| DesiredProject {
            context: owner.context().clone(),
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
    (registry, observed, release)
}

#[tokio::test]
async fn actual_cleanup_precedes_removal_and_preserves_sibling_checkout_versions_and_files() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let input = f.input(&ctx);
    let (mut registry, mut observed, release) = held_registry(&f, false).await;
    let driver = async {
        observed.recv().await.unwrap();
        assert!(f.registered(&ctx, 0));
        release.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx),
        driver
    );
    assert!(result.unwrap());
    let snapshot = f.store().load(ctx.snapshot()).unwrap();
    let a = snapshot.get(f.ids[0]).unwrap();
    assert_eq!(a.versions().collect::<Vec<_>>(), ["feature", "main"]);
    assert_eq!(a.checkouts().count(), 1);
    assert_eq!(a.checkouts().next().unwrap().path(), f.projects[2].root());
    assert!(f.registered(&ctx, 1));
    assert_eq!(registry.supervised().count(), 1);
    assert!(f.projects[0].root().join("source").exists());
    assert!(f.projects[0].identity_file().exists());
    registry.shutdown().await.unwrap();
}
#[tokio::test]
async fn missing_physical_checkout_is_removed_by_exact_history_without_recreation() {
    for local in [false, true] {
        let f = Fixture::new(true);
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        if local {
            publish_local(&f.auth, &f.paths, &ctx);
        }
        fs::remove_dir_all(f.projects[0].root()).unwrap();
        let mut registry = ProjectSupervisors::default();
        let mut input = f.input(&ctx);
        if local {
            input.credential = CheckoutCredential::Local("test-local-key");
        }
        assert!(
            retire_checkout(&f.auth, input, &mut registry, 65536, &ctx)
                .await
                .unwrap()
        );
        assert!(!f.projects[0].root().exists());
        assert!(f.registered(&ctx, 2));
    }
}
#[tokio::test]
async fn absent_id_or_path_is_unchanged_and_does_not_stop_siblings() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let before = fs::read(f.paths.root().join("project-inventory.json")).unwrap();
    let mut registry = ProjectSupervisors::default();
    let mut input = f.input(&ctx);
    input.expected = f.ids[1];
    assert!(
        !retire_checkout(&f.auth, input, &mut registry, 65536, &ctx)
            .await
            .unwrap()
    );
    let missing = f._temp.path().join("absent");
    let mut input = f.input(&ctx);
    input.checkout = &missing;
    assert!(
        !retire_checkout(&f.auth, input, &mut registry, 65536, &ctx)
            .await
            .unwrap()
    );
    assert!(!missing.exists());
    assert_eq!(
        fs::read(f.paths.root().join("project-inventory.json")).unwrap(),
        before
    );
}
#[tokio::test]
async fn configure_denial_prevents_cleanup_and_mutation() {
    let f = Fixture::new(false);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let (mut registry, mut observed, release) = held_registry(&f, false).await;
    assert!(matches!(
        retire_checkout(&f.auth, f.input(&ctx), &mut registry, 65536, &ctx).await,
        Err(CheckoutRetirementError::Admission(_))
    ));
    assert!(observed.try_recv().is_err());
    assert_eq!(registry.supervised().count(), 2);
    assert!(f.registered(&ctx, 0));
    release.send(()).unwrap();
    registry.shutdown().await.unwrap();
}
#[tokio::test]
async fn cancellation_during_cleanup_joins_owners_but_keeps_metadata() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let (mut registry, mut observed, release) = held_registry(&f, false).await;
    let input = f.input(&ctx);
    let driver = async {
        observed.recv().await.unwrap();
        cancel.cancel();
        release.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx),
        driver
    );
    assert!(
        matches!(result,Err(CheckoutRetirementError::Request(e)) if e.kind()==io::ErrorKind::Interrupted)
    );
    assert_eq!(registry.supervised().count(), 1);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    assert!(f.registered(&fresh, 0));
    registry.shutdown().await.unwrap();
}
#[tokio::test]
async fn deadline_during_cleanup_joins_owners_but_keeps_metadata() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(1);
    let ctx = RequestContext::new(content(), &cancel, deadline).unwrap();
    let (mut registry, mut observed, release) = held_registry(&f, false).await;
    let input = f.input(&ctx);
    let driver = async {
        observed.recv().await.unwrap();
        tokio::time::sleep_until(deadline.into()).await;
        release.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx),
        driver
    );
    assert!(
        matches!(result,Err(CheckoutRetirementError::Request(e)) if e.kind()==io::ErrorKind::TimedOut)
    );
    let fresh = request(&cancel);
    assert!(f.registered(&fresh, 0));
    assert_eq!(registry.supervised().count(), 1);
    registry.shutdown().await.unwrap();
}
#[tokio::test]
async fn cleanup_failure_preserves_cause_and_inventory_without_restart() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let (mut registry, mut observed, release) = held_registry(&f, true).await;
    let input = f.input(&ctx);
    let driver = async {
        observed.recv().await.unwrap();
        release.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx),
        driver
    );
    assert!(
        matches!(result,Err(CheckoutRetirementError::Stop(e)) if e.kind()==io::ErrorKind::PermissionDenied && e.to_string()=="producer cleanup failed")
    );
    assert!(f.registered(&ctx, 0));
    assert_eq!(registry.supervised().count(), 2);
    registry.shutdown().await.unwrap();
    assert_eq!(registry.supervised().count(), 0);
    assert!(f.registered(&ctx, 0));
}
#[tokio::test]
async fn metadata_failure_after_cleanup_preserves_error_and_stopped_effect() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let (mut registry, mut observed, release) = held_registry(&f, false).await;
    let input = f.input(&ctx);
    let driver = async {
        observed.recv().await.unwrap();
        fs::write(
            f.paths.root().join("project-inventory.json"),
            b"invalid JSON",
        )
        .unwrap();
        release.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx),
        driver
    );
    assert!(
        matches!(result,Err(CheckoutRetirementError::Inventory(e)) if e.kind()==io::ErrorKind::InvalidData)
    );
    assert_eq!(registry.supervised().count(), 1);
    registry.shutdown().await.unwrap();
}
#[tokio::test]
async fn foreign_private_request_and_installation_are_rejected_before_metadata() {
    let f = Fixture::new(true);
    let other = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let foreign = request(&cancel);
    let mut registry = ProjectSupervisors::default();
    let input = f.input(&foreign);
    assert!(matches!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx).await,
        Err(CheckoutRetirementError::Installation(_))
    ));
    let mut input = f.input(&ctx);
    input.installation = other.installation(&ctx);
    assert!(matches!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx).await,
        Err(CheckoutRetirementError::Scope(_))
    ));
    assert!(f.registered(&ctx, 0));
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

#[tokio::test]
async fn actual_local_profile_uses_configure_without_runtime_credential_conversion() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    publish_local(&f.auth, &f.paths, &ctx);
    let mut registry = ProjectSupervisors::default();
    let mut input = f.input(&ctx);
    input.credential = CheckoutCredential::Local("test-local-key");
    assert!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx)
            .await
            .unwrap()
    );
    assert!(!f.registered(&ctx, 0));
    assert!(f.registered(&ctx, 2));
}
#[tokio::test]
async fn cancelled_original_prevents_all_cleanup_and_publication() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let input = f.input(&ctx);
    cancel.cancel();
    let mut registry = ProjectSupervisors::default();
    assert!(
        matches!(retire_checkout(&f.auth,input,&mut registry,65536,&ctx).await,Err(CheckoutRetirementError::Request(e)) if e.kind()==io::ErrorKind::Interrupted)
    );
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    assert!(f.registered(&fresh, 0));
}
#[tokio::test]
async fn replaced_installation_after_cleanup_does_not_publish_inventory() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let before = fs::read(f.paths.root().join("project-inventory.json")).unwrap();
    let (mut registry, mut observed, release) = held_registry(&f, false).await;
    let input = f.input(&ctx);
    let held = f._temp.path().join("held-installation");
    let driver = async {
        observed.recv().await.unwrap();
        fs::rename(f.paths.root(), &held).unwrap();
        fs::create_dir(f.paths.root()).unwrap();
        release.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(
        retire_checkout(&f.auth, input, &mut registry, 65536, &ctx),
        driver
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read(held.join("project-inventory.json")).unwrap(),
        before
    );
    assert_eq!(registry.supervised().count(), 1);
    registry.shutdown().await.unwrap();
}

#[tokio::test]
async fn parking_keeps_history_and_explicit_retirement_removes_only_matching_parked_context() {
    let f = Fixture::new(true);
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut registry = ProjectSupervisors::default();
    registry
        .reconcile(
            vec![
                DesiredProject {
                    context: f.context(0),
                    active: false,
                },
                DesiredProject {
                    context: f.context(1),
                    active: false,
                },
            ],
            |_| {
                std::future::ready(Err(io::Error::other(
                    "parked desired must not start a producer",
                )))
            },
        )
        .await
        .unwrap();
    assert_eq!(registry.parked().count(), 2);
    assert!(f.registered(&ctx, 0));
    assert!(
        retire_checkout(&f.auth, f.input(&ctx), &mut registry, 65536, &ctx)
            .await
            .unwrap()
    );
    assert_eq!(registry.parked().count(), 1);
    assert_eq!(registry.parked().next().unwrap().id, f.ids[1]);
    assert!(f.registered(&ctx, 1));
    assert!(f.projects[0].root().exists());
    registry.shutdown().await.unwrap();
}
