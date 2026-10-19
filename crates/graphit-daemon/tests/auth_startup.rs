use graphit_auth::{
    AdminSessionPolicy, AuthError, IssuerAuthorityPolicy, IssuerLimits, Obligation, Permission,
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput, ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeBindingRecord, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::{Cancellation, FileLock},
    installation_bootstrap::BootstrapLimits,
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyEffect, RuntimeKeyError, RuntimeKeyLimits},
    version_context::{Checkout, VersionContext},
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupError, StartupPolicy, StartupResources};
use graphit_hub::{NativeBinding, native::ContentLimits, policy::RequestContext};
use secrecy::SecretString;
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_until(cancel, Instant::now() + Duration::from_secs(30))
}
fn request_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 128).unwrap()
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
fn policy() -> StartupPolicy {
    let permissions = [Permission::Read(Module::Ast)];
    policy_with_permissions(&permissions, &[])
}
fn policy_with_permissions(
    ordinary: &[Permission],
    administrative: &[Permission],
) -> StartupPolicy {
    let source = || SourceAuthorityPolicy {
        ordinary,
        administrative,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope: scopes(),
        document: documents(),
        profiles: profiles(),
        authority: IssuerAuthorityPolicy::new(
            source(),
            source(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 1024,
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
struct Fixture {
    _directory: tempfile::TempDir,
    paths: InstallationPaths,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("installation");
        let paths =
            InstallationPaths::resolve(Brand::default(), directory.path(), None, Some(&root))
                .unwrap();
        Self {
            _directory: directory,
            paths,
        }
    }
    fn key_path(&self) -> PathBuf {
        self.paths.global_path(&["daemon", "mcp.key"]).unwrap()
    }
    fn lock_path(&self) -> PathBuf {
        self.paths.global_path(&["daemon.pid"]).unwrap()
    }
}
fn key(auth: &DaemonAuth, ctx: &RequestContext<'_>) -> String {
    let checked = auth.runtime().check(ctx.snapshot()).unwrap();
    String::from_utf8(checked.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
}
fn admitted(auth: &DaemonAuth, key: &str, ctx: &RequestContext<'_>) {
    assert_eq!(
        auth.issuer()
            .authenticate_runtime(key, ctx)
            .unwrap()
            .authorize(Permission::Read(Module::Ast))
            .unwrap(),
        Obligation::Unconditional
    );
}
fn remove(auth: &DaemonAuth, ctx: &RequestContext<'_>) {
    let removed = auth.remove_runtime(ctx.snapshot());
    assert_eq!(removed.effects().key, RuntimeKeyEffect::Applied);
    assert!(removed.failure().is_none());
    assert!(removed.cleanup_failure().is_none());
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
        auth.installation()
            .live
            .allowed_roots()
            .map(ToOwned::to_owned),
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
                        mcp_key: Some(Secret::new(SecretString::from("test-local-key"))),
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
fn real_installation_runtime_and_current_local_profile_admit_only_valid_credentials() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let current = key(&auth, &ctx);
    admitted(&auth, &current, &ctx);
    assert!(auth.issuer().authenticate_runtime("wrong", &ctx).is_err());
    assert!(
        auth.issuer()
            .authenticate_local("test-local-key", &ctx)
            .is_err()
    );
    let persisted = InstallationIdentityStore::new(&fixture.paths)
        .unwrap()
        .load()
        .unwrap();
    assert!(persisted.is_provisioned());
    publish_local(&auth, &fixture.paths, &ctx);
    assert_eq!(
        auth.issuer()
            .authenticate_local("test-local-key", &ctx)
            .unwrap()
            .authorize(Permission::Read(Module::Ast))
            .unwrap(),
        Obligation::Unconditional
    );
    assert!(auth.issuer().authenticate_local("wrong", &ctx).is_err());
    remove(&auth, &ctx);
    assert!(!fixture.key_path().exists());
}
#[test]
fn startup_original_ends_without_ending_service_but_new_requests_remain_cancellable() {
    let fixture = Fixture::new();
    let startup = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(2);
    let auth = {
        let ctx = request_until(&startup, deadline);
        DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap()
    };
    startup.cancel();
    std::thread::sleep(
        deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
    );
    let fresh = Cancellation::default();
    let ctx = request(&fresh);
    let current = key(&auth, &ctx);
    admitted(&auth, &current, &ctx);
    let maintained = auth.maintain(ctx.snapshot());
    assert!(maintained.failure().is_none());
    let cancelled = Cancellation::default();
    let denied = request(&cancelled);
    cancelled.cancel();
    assert!(
        auth.issuer()
            .authenticate_runtime(&current, &denied)
            .is_err()
    );
    assert_eq!(
        auth.runtime().check(denied.snapshot()).err(),
        Some(RuntimeKeyError::Cancelled)
    );
    remove(&auth, &ctx);
}
#[test]
fn same_root_is_exclusive_before_rotation_other_roots_work_and_restart_gets_new_key() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let auth = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let first = key(&auth, &ctx);
    let failure = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx)
        .err()
        .unwrap();
    assert!(
        matches!(failure.cause(),StartupError::Io(error) if error.kind()==io::ErrorKind::WouldBlock)
    );
    assert!(failure.resources().is_none());
    admitted(&auth, &first, &ctx);
    let other = Fixture::new();
    let independent = DaemonAuth::start(&other.paths, &[], policy(), &ctx).unwrap();
    admitted(&independent, &key(&independent, &ctx), &ctx);
    remove(&independent, &ctx);
    remove(&auth, &ctx);
    assert!(auth.issuer().authenticate_runtime(&first, &ctx).is_err());
    drop(auth);
    let restarted = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    let second = key(&restarted, &ctx);
    assert_ne!(first, second);
    admitted(&restarted, &second, &ctx);
    remove(&restarted, &ctx);
}
#[test]
fn cancelled_and_expired_startup_do_not_create_runtime_or_installation() {
    let fixture = Fixture::new();
    let service_signal = Cancellation::default();
    let service = RequestContext::service(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        &service_signal,
    )
    .unwrap();
    let failure = DaemonAuth::start(&fixture.paths, &[], policy(), &service)
        .err()
        .unwrap();
    let (cause, resources) = failure.into_parts();
    assert!(
        matches!(cause, StartupError::Io(error) if error.kind() == io::ErrorKind::InvalidInput)
    );
    assert!(resources.is_none());
    assert!(!fixture.paths.root().exists());
    assert!(!fixture.key_path().exists());
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    cancel.cancel();
    let failure = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx)
        .err()
        .unwrap();
    let (cause, resources) = failure.into_parts();
    assert!(matches!(cause, StartupError::Io(error) if error.kind() == io::ErrorKind::Interrupted));
    assert!(resources.is_none());
    assert!(!fixture.paths.root().exists());
    let fresh = Cancellation::default();
    let expired = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        &fresh,
        Instant::now() + Duration::from_millis(10),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let failure = DaemonAuth::start(&fixture.paths, &[], policy(), &expired)
        .err()
        .unwrap();
    let (cause, resources) = failure.into_parts();
    assert!(matches!(cause, StartupError::Io(error) if error.kind() == io::ErrorKind::TimedOut));
    assert!(resources.is_none());
    assert!(!fixture.paths.root().exists());
    assert!(!fixture.key_path().exists());
}
#[test]
fn post_key_failure_retains_native_resources_and_cancelled_cleanup_is_retryable() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut invalid = policy();
    invalid.ttl.ttl = Duration::ZERO;
    let failure = DaemonAuth::start(&fixture.paths, &[], invalid, &ctx)
        .err()
        .unwrap();
    let (cause, resources) = failure.into_parts();
    assert!(matches!(cause, StartupError::Authority(_)));
    assert!(fixture.key_path().exists());
    let StartupResources::RuntimeOwner { owner, singleton } = resources.unwrap() else {
        panic!("post-key failure must retain owner")
    };
    assert_eq!(
        FileLock::try_acquire(fixture.lock_path())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    let stopped = Cancellation::default();
    let cancelled = request(&stopped);
    stopped.cancel();
    let cleanup = owner.remove(cancelled.snapshot());
    assert_eq!(cleanup.effects().key, RuntimeKeyEffect::NotApplied);
    assert_eq!(cleanup.failure(), Some(RuntimeKeyError::Cancelled));
    assert!(fixture.key_path().exists());
    assert_eq!(
        FileLock::try_acquire(fixture.lock_path())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    let removed = owner.remove(ctx.snapshot());
    assert_eq!(removed.effects().key, RuntimeKeyEffect::Applied);
    assert!(removed.failure().is_none());
    assert!(!fixture.key_path().exists());
    drop(owner);
    drop(singleton);
    let restarted = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    remove(&restarted, &ctx);
}
#[test]
fn runtime_sdk_failure_keeps_original_outcome_and_singleton_until_caller_releases() {
    let fixture = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut invalid = policy();
    invalid.runtime = RuntimeKeyLimits::new(1, 1, 12).unwrap();
    let failure = DaemonAuth::start(&fixture.paths, &[], invalid, &ctx)
        .err()
        .unwrap();
    let (cause, resources) = failure.into_parts();
    let StartupResources::RuntimeStart { outcome, singleton } = resources.unwrap() else {
        panic!("Core failure outcome must be forwarded")
    };
    assert!(matches!(cause, StartupError::Runtime(error) if Some(error) == outcome.failure()));
    assert!(outcome.failure().is_some());
    assert_eq!(outcome.effects().key, RuntimeKeyEffect::NotApplied);
    assert_eq!(
        FileLock::try_acquire(fixture.lock_path())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(!fixture.key_path().exists());
    drop(outcome);
    drop(singleton);
    let restarted = DaemonAuth::start(&fixture.paths, &[], policy(), &ctx).unwrap();
    remove(&restarted, &ctx);
}

struct Project {
    scope: CapturedPublicationScope,
    binding: ScopeBindingRecord,
    version: VersionContext,
}
impl Fixture {
    fn project_paths(&self, name: &str) -> ProjectPaths {
        let root = self._directory.path().join(name);
        std::fs::create_dir(&root).unwrap();
        ProjectPaths::at(root, &self.paths).unwrap()
    }
}
impl Project {
    fn capture(paths: &ProjectPaths, ctx: &RequestContext<'_>) -> Self {
        let identity = ProjectIdentityStore::at(paths)
            .writer(Duration::from_secs(5), ctx.cancellation())
            .unwrap()
            .ensure(&ProjectMetadata::new("maintained-project").unwrap())
            .unwrap();
        let scope =
            CapturedPublicationScope::project(paths, identity.id(), scopes(), ctx.snapshot())
                .unwrap();
        let binding = scope.binding().unwrap();
        let version = VersionContext::select(paths, Checkout::NonGit, "", None, "").unwrap();
        Self {
            scope,
            binding,
            version,
        }
    }
    fn input<'a>(&'a self, auth: &'a DaemonAuth) -> ScopeInput<'a> {
        ScopeInput {
            installation: auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.scope,
                    record: &self.binding,
                },
                version: &self.version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        }
    }
}
#[test]
fn trusted_sibling_projects_admit_runtime_local_records_and_managed_runs() {
    let fixture = Fixture::new();
    let paths = [
        fixture.project_paths("first"),
        fixture.project_paths("second"),
    ];
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let permissions = [Permission::Read(Module::Task), Permission::WriteTask];
    let auth = DaemonAuth::start(
        &fixture.paths,
        &paths,
        policy_with_permissions(&permissions, &permissions),
        &ctx,
    )
    .unwrap();
    let mut expected = vec![fixture.paths.root(), paths[0].root(), paths[1].root()];
    expected.sort();
    assert_eq!(
        auth.installation().live.allowed_roots().collect::<Vec<_>>(),
        expected
    );
    publish_local(&auth, &fixture.paths, &ctx);
    let raw = key(&auth, &ctx);
    let runtime = auth.issuer().authenticate_runtime(&raw, &ctx).unwrap();
    let local = auth
        .issuer()
        .authenticate_local("test-local-key", &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(auth.issuer());
    for project_paths in &paths {
        let project = Project::capture(project_paths, &ctx);
        let scope = project.input(&auth);
        for caller in [&runtime, &local] {
            let read = authority
                .admit_authenticated_read(caller, &ctx, scope, ProjectReadModule::Task)
                .unwrap();
            assert_eq!(read.recheck(), Ok(()));
            assert!(std::ptr::eq(read.request().unwrap(), &ctx));
            assert_eq!(
                read.project().unwrap().scope.live.owner_root(),
                project_paths.root()
            );
            let write = authority
                .admit_authenticated_write(caller, &ctx, scope, ProjectWriteOperation::Task)
                .unwrap();
            assert_eq!(write.recheck(), Ok(Obligation::Unconditional));
            assert!(write.actor().unwrap() == caller.actor().unwrap());
            let session = auth.issuer().bootstrap_admin(caller).unwrap();
            let parent = auth.issuer().admin_parent(caller, &session).unwrap();
            let run = auth
                .issuer()
                .start_run(&parent, &scope, Duration::from_secs(60))
                .unwrap();
            auth.issuer().close_run(&parent, &run).unwrap();
        }
    }
    let unknown = Project::capture(&fixture.project_paths("unregistered"), &ctx);
    for caller in [&runtime, &local] {
        assert!(matches!(
            authority.admit_authenticated_read(
                caller,
                &ctx,
                unknown.input(&auth),
                ProjectReadModule::Task
            ),
            Err(AuthError::Denied)
        ));
        assert!(matches!(
            authority.admit_authenticated_write(
                caller,
                &ctx,
                unknown.input(&auth),
                ProjectWriteOperation::Task
            ),
            Err(AuthError::Denied)
        ));
        let session = auth.issuer().bootstrap_admin(caller).unwrap();
        let parent = auth.issuer().admin_parent(caller, &session).unwrap();
        assert!(matches!(
            auth.issuer()
                .start_run(&parent, &unknown.input(&auth), Duration::from_secs(60)),
            Err(AuthError::Denied)
        ));
    }
    remove(&auth, &ctx);
}
#[test]
fn trusted_root_does_not_grant_permissions_or_replace_original_request() {
    let fixture = Fixture::new();
    let paths = fixture.project_paths("trusted");
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let permissions = [Permission::Read(Module::Task)];
    let auth = DaemonAuth::start(
        &fixture.paths,
        std::slice::from_ref(&paths),
        policy_with_permissions(&permissions, &[]),
        &ctx,
    )
    .unwrap();
    publish_local(&auth, &fixture.paths, &ctx);
    let project = Project::capture(&paths, &ctx);
    let raw = key(&auth, &ctx);
    let runtime = auth.issuer().authenticate_runtime(&raw, &ctx).unwrap();
    let local = auth
        .issuer()
        .authenticate_local("test-local-key", &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(auth.issuer());
    let fresh_cancel = Cancellation::default();
    let foreign = request(&fresh_cancel);
    for caller in [&runtime, &local] {
        let read = authority
            .admit_authenticated_read(caller, &ctx, project.input(&auth), ProjectReadModule::Task)
            .unwrap();
        assert_eq!(read.recheck(), Ok(()));
        assert!(matches!(
            authority.admit_authenticated_write(
                caller,
                &ctx,
                project.input(&auth),
                ProjectWriteOperation::Task
            ),
            Err(AuthError::Denied)
        ));
        assert!(matches!(
            authority.admit_authenticated_read(
                caller,
                &foreign,
                project.input(&auth),
                ProjectReadModule::Task
            ),
            Err(AuthError::Denied)
        ));
    }
    let retained = authority
        .admit_authenticated_read(
            &runtime,
            &ctx,
            project.input(&auth),
            ProjectReadModule::Task,
        )
        .unwrap();
    cancel.cancel();
    assert!(retained.recheck().is_err());
    assert!(retained.request().is_err());
    assert!(retained.project().is_err());
    remove(&auth, &foreign);
}
#[test]
fn duplicate_trusted_root_failure_keeps_owned_runtime_for_explicit_cleanup() {
    let fixture = Fixture::new();
    let project = fixture.project_paths("trusted");
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let failure = DaemonAuth::start(
        &fixture.paths,
        &[project.clone(), project.clone()],
        policy(),
        &ctx,
    )
    .err()
    .unwrap();
    assert!(
        matches!(failure.cause(), StartupError::Io(error) if error.kind() == io::ErrorKind::InvalidInput)
    );
    let StartupResources::RuntimeOwner { owner, singleton } = failure.into_resources().unwrap()
    else {
        panic!("post-key scope failure retains resources")
    };
    assert!(fixture.key_path().exists());
    assert_eq!(
        FileLock::try_acquire(fixture.lock_path())
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        owner.remove(ctx.snapshot()).effects().key,
        RuntimeKeyEffect::Applied
    );
    drop(owner);
    drop(singleton);
    let restarted = DaemonAuth::start(&fixture.paths, &[project], policy(), &ctx).unwrap();
    remove(&restarted, &ctx);
}

#[test]
fn another_installation_capture_does_not_replace_the_admitted_realm() {
    let fixture = Fixture::new();
    let paths = fixture.project_paths("trusted");
    let other = Fixture::new();
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let permissions = [Permission::Read(Module::Task)];
    let auth = DaemonAuth::start(
        &fixture.paths,
        std::slice::from_ref(&paths),
        policy_with_permissions(&permissions, &[]),
        &ctx,
    )
    .unwrap();
    let independent = DaemonAuth::start(
        &other.paths,
        &[],
        policy_with_permissions(&permissions, &[]),
        &ctx,
    )
    .unwrap();
    let project = Project::capture(&paths, &ctx);
    let raw = key(&auth, &ctx);
    let caller = auth.issuer().authenticate_runtime(&raw, &ctx).unwrap();
    let mut wrong_installation = project.input(&auth);
    wrong_installation.installation = independent.installation();
    let authority = ProjectRecordAuthority::new(auth.issuer());
    assert!(matches!(
        authority.admit_authenticated_read(
            &caller,
            &ctx,
            wrong_installation,
            ProjectReadModule::Task
        ),
        Err(AuthError::Denied)
    ));
    let foreign_authority = ProjectRecordAuthority::new(independent.issuer());
    assert!(matches!(
        foreign_authority.admit_authenticated_read(
            &caller,
            &ctx,
            project.input(&auth),
            ProjectReadModule::Task
        ),
        Err(AuthError::Denied)
    ));
    remove(&auth, &ctx);
    remove(&independent, &ctx);
}
