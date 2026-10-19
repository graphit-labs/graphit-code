use super::*;
use crate::{ExternalEffect, IssuerLimits, ProjectCapture, Setting};
use graphit_core::{
    artifact_store::{ScopeContext, ScopeLimits},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    native_path::NativePath,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyLimits,
    version_context::{Checkout, VersionContext},
};
use graphit_hub::native::Role;
use graphit_hub::{
    ArtifactType, NativeBinding, ProjectionLimits, SourceOrigin, SourceRecord, SourceRef,
    native::ContentLimits,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
mod broker_records;
mod hub_definitions;
mod local;
mod native_installation;
mod presence_credentials;
#[path = "../profile_manager/tests.rs"]
pub(crate) mod profile_manager;
mod records;
mod records_deadline;
mod tool_owned_roles;
mod trusted_roots;

#[cfg(unix)]
#[test]
fn runtime_bootstrap_rechecks_physical_source_after_last_entropy() {
    use std::os::unix::fs::PermissionsExt;
    struct MutatingEntropy {
        path: std::path::PathBuf,
        mutate: AtomicBool,
        entropy: TestEntropy,
    }
    impl Entropy for MutatingEntropy {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
            self.entropy.fill(bytes)?;
            if self.mutate.swap(false, Ordering::SeqCst) {
                std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o400))
                    .unwrap();
            }
            Ok(())
        }
    }
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let entropy = Arc::new(MutatingEntropy {
        path: f.installation.owner_root().join("daemon/mcp.key"),
        mutate: AtomicBool::new(false),
        entropy: TestEntropy {
            fail: AtomicBool::new(false),
            counter: AtomicU64::new(0),
        },
    });
    let issuer = RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        authority(&all_permissions(), &all_permissions(), limits(), 7200),
        ttl(),
        &ctx,
        Arc::new(SystemClock),
        entropy.clone(),
    )
    .unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    entropy.mutate.store(true, Ordering::SeqCst);
    assert!(issuer.bootstrap_admin(&caller).is_err());
    assert!(issuer.lock().unwrap().sessions.is_empty());
}

fn limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 1024,
    }
}
fn ttl() -> TtlPolicy {
    TtlPolicy {
        ttl: Duration::from_secs(900),
        maximum: Duration::from_secs(3600),
        refresh_margin: Duration::from_secs(120),
    }
}
fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_before(cancel, Instant::now() + Duration::from_secs(30))
}
fn request_before(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 256,
            max_collection_entries: 64,
        },
        cancel,
        deadline,
    )
    .unwrap()
}

fn all_permissions() -> Vec<Permission> {
    use Permission::*;
    let mut p = graphit_core::module_settings::Module::ALL
        .map(Read)
        .to_vec();
    p.extend([
        WriteMemory,
        WriteOwnTask,
        WriteFencedMemory,
        InspectGit,
        CoordinateTask,
        Delegate,
        WriteTask,
        ReadUser,
        MutateUser,
        Configure,
        ManageAuthProfiles,
        ManageAccessGrants,
        ManageCache,
        External(ExternalEffect::NativeLaunch),
        External(ExternalEffect::HookExecution),
        External(ExternalEffect::RemoteRequest),
        External(ExternalEffect::PublishedFilesystem),
    ]);
    p.extend(
        graphit_core::module_settings::Module::ALL
            .into_iter()
            .filter(|m| {
                !matches!(
                    m,
                    graphit_core::module_settings::Module::Task
                        | graphit_core::module_settings::Module::Memory
                )
            })
            .map(MutateModule),
    );
    p
}
struct Fixture {
    runtime: Arc<RuntimeKeyOwner>,
    installation: CapturedPublicationScope,
    installation_record: ScopeBindingRecord,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
    source_root: std::path::PathBuf,
    _dir: tempfile::TempDir,
}
impl Fixture {
    fn new(request: &RequestContext<'_>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let root = dir.path().join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&root).unwrap();
        let install =
            InstallationPaths::resolve(Brand::default(), &root, Some(&home), None).unwrap();
        let private = installation_bootstrap::prepare(
            &install,
            BootstrapLimits::new(65536, 256).unwrap(),
            request.snapshot(),
        )
        .into_installation()
        .unwrap();
        let runtime = Arc::new(
            RuntimeKeyOwner::start(
                private,
                RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .into_owner()
            .ok()
            .unwrap(),
        );
        let paths = ProjectPaths::at(&root, &install).unwrap();
        let sl = ScopeLimits::new(16, 4096, 128).unwrap();

        let installation = CapturedPublicationScope::installation(
            &install,
            InstallationId::new("local-installation").unwrap(),
            [root.clone()],
            sl,
            request.snapshot(),
        )
        .unwrap();

        let project = CapturedPublicationScope::project(
            &paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            sl,
            request.snapshot(),
        )
        .unwrap();
        let version =
            VersionContext::select(&paths, Checkout::NonGit, "development", None, "").unwrap();
        let installation_record = installation.binding().unwrap();
        let project_record = project.binding().unwrap();
        Self {
            runtime,
            installation,
            installation_record,
            project,
            project_record,
            version,
            source_root: root,
            _dir: dir,
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
    fn definition(
        &self,
        role: Role,
        permissions: &[Permission],
        policy: u8,
        limits: IssuerLimits,
    ) -> RoleDefinition {
        self.definition_for_binding(role, permissions, policy, limits, NativeBinding::KiroV3)
    }
    fn definition_for_binding(
        &self,
        role: Role,
        permissions: &[Permission],
        policy: u8,
        limits: IssuerLimits,
        binding: NativeBinding,
    ) -> RoleDefinition {
        let source = self.source(role, ArtifactType::Agent);
        RoleDefinition::new(role, &source, binding, permissions, [policy; 32], limits).unwrap()
    }
    fn source(&self, role: Role, artifact_type: ArtifactType) -> SourceRef {
        let id = format!("roles/{}", role.as_str());
        SourceRef::restore(
            SourceRecord {
                remote_file_origin: None,
                archive_sha256: None,
                origin: SourceOrigin::LocalLink,
                producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                artifact_type,
                id: id.clone(),
                physical_id: None,
                version: "1.0.0".into(),
                root: NativePath::new(self.source_root.clone()).unwrap(),
            },
            ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap(),
        )
        .unwrap()
    }
    fn auth_store(&self, request: &RequestContext<'_>) -> Arc<AuthStore> {
        self.auth_store_policy(
            request,
            crate::profile::ProfileLimits {
                bytes: 4096,
                nodes: 4096,
                scalar_bytes: 256,
                collection_entries: 64,
                providers: 16,
                profiles: 16,
            },
        )
    }
    fn auth_store_policy(
        &self,
        request: &RequestContext<'_>,
        policy: crate::profile::ProfileLimits,
    ) -> Arc<AuthStore> {
        let paths = InstallationPaths::resolve(
            self.installation.brand().clone(),
            &self.source_root,
            None,
            Some(self.installation.owner_root()),
        )
        .unwrap();
        let scope = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("local-installation").unwrap(),
            [self.installation.owner_root().to_owned()],
            ScopeLimits::new(16, 4096, 128).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        let physical = graphit_core::auth_document::AuthDocumentStore::capture(
            scope,
            graphit_core::auth_document::AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        Arc::new(AuthStore::new(physical, policy, request).unwrap())
    }
    fn secret(&self) -> Zeroizing<String> {
        Zeroizing::new(
            std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap(),
        )
    }
    fn issuer(&self, request: &RequestContext<'_>, limits: IssuerLimits) -> RoleIssuer {
        RoleIssuer::new(
            self.realm(),
            Arc::clone(&self.runtime),
            self.auth_store(request),
            authority(&all_permissions(), &all_permissions(), limits, 7200),
            ttl(),
            request,
        )
        .unwrap()
    }
}
fn authority(
    ordinary: &[Permission],
    admin: &[Permission],
    limits: IssuerLimits,
    seconds: u64,
) -> crate::IssuerAuthorityPolicy {
    authority_result(
        ordinary,
        admin,
        crate::AdminSessionPolicy {
            lifetime: Duration::from_secs(seconds),
            maximum: Duration::from_secs(seconds),
        },
        limits,
    )
    .unwrap()
}
fn authority_result(
    ordinary: &[Permission],
    admin: &[Permission],
    sessions: crate::AdminSessionPolicy,
    limits: IssuerLimits,
) -> Result<crate::IssuerAuthorityPolicy, AuthError> {
    crate::IssuerAuthorityPolicy::new(
        crate::SourceAuthorityPolicy {
            ordinary,
            administrative: admin,
            sessions,
        },
        crate::SourceAuthorityPolicy {
            ordinary,
            administrative: admin,
            sessions,
        },
        limits,
        deny_services(),
    )
}

fn actor_bytes(actor: &Actor) -> [u8; 64] {
    let mut bytes = [0; 64];
    actor.write_identifier(&mut bytes);
    bytes
}

#[test]
fn actual_key_roles_scopes_and_conditional_obligations() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    assert!(matches!(
        issuer.authenticate_runtime("wrong-key-sentinel", &ctx),
        Err(AuthError::Denied)
    ));
    let caller_parent = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_parent = issuer.bootstrap_admin(&caller_parent).unwrap();
    let parent = issuer
        .admin_parent(&caller_parent, &session_parent)
        .unwrap();
    let roles = [
        (
            Role::Scout,
            Permission::Read(Module::Hub),
            Obligation::Unconditional,
        ),
        (
            Role::Tracker,
            Permission::InspectGit,
            Obligation::Unconditional,
        ),
        (Role::Scribe, Permission::WriteOwnTask, Obligation::OwnActor),
        (
            Role::Dream,
            Permission::WriteFencedMemory,
            Obligation::MemoryFenceRequired,
        ),
        (
            Role::Coordinator,
            Permission::Configure,
            Obligation::Unconditional,
        ),
    ];
    let mut actors = Vec::new();
    for (role, permission, obligation) in roles {
        let def = f.definition(role, &[permission], 1, limits());
        let reference = def.reference().clone();
        issuer.register_definition(&parent, def).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(3600))
            .unwrap();
        let token = issuer
            .issue(&parent, &run, &reference, &[permission], &f.scope())
            .unwrap();
        assert_eq!(token.expose_for_transport().len(), 47);
        let grant = issuer
            .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        assert_eq!(grant.authorize(permission), Ok(obligation));
        assert_eq!(
            grant.authorize(Permission::Delegate),
            Err(AuthError::Denied)
        );
        actors.push(actor_bytes(&grant.actor().unwrap()));
        let mut wrong = f.scope();
        wrong.binding = NativeBinding::Codex;
        assert!(matches!(
            issuer.verify(token.expose_for_transport(), &reference, wrong, &ctx),
            Err(AuthError::Denied)
        ));
    }
    actors.sort();
    actors.dedup();
    assert_eq!(actors.len(), 5);
    assert!(
        RoleDefinition::new(
            Role::Scout,
            &SourceRef::restore(
                SourceRecord {
                    remote_file_origin: None,
                    archive_sha256: None,
                    origin: SourceOrigin::LocalLink,
                    producer: "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
                    artifact_type: ArtifactType::Agent,
                    id: "bad".into(),
                    physical_id: None,
                    version: "1".into(),
                    root: NativePath::new(f.source_root.clone()).unwrap(),
                },
                ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap()
            )
            .unwrap(),
            NativeBinding::KiroV3,
            &[Permission::Configure],
            [0; 32],
            limits()
        )
        .is_err()
    );
}
#[test]
fn refresh_invalidates_old_nonce_preserves_actor_with_other_active_credentials() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let bounds = limits();
    let issuer = f.issuer(&ctx, bounds);
    let caller_parent = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_parent = issuer.bootstrap_admin(&caller_parent).unwrap();
    let parent = issuer
        .admin_parent(&caller_parent, &session_parent)
        .unwrap();
    let permission = Permission::Read(Module::Task);
    let def = f.definition(Role::Scout, &[permission], 1, bounds);
    let reference = def.reference().clone();
    issuer.register_definition(&parent, def).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let old = issuer
        .issue(&parent, &run, &reference, &[permission], &f.scope())
        .unwrap();
    let old_grant = issuer
        .verify(old.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let actor = old_grant.actor().unwrap();
    let sibling = issuer
        .issue(&parent, &run, &reference, &[permission], &f.scope())
        .unwrap();
    let fresh = issuer.refresh(&parent, &old, &f.scope()).unwrap();
    assert_eq!(old_grant.authorize(permission), Err(AuthError::Denied));
    assert!(old_grant.actor().is_err());
    assert!(old_grant.refresh_due().is_err());
    assert_ne!(old.expose_for_transport(), fresh.expose_for_transport());
    assert!(
        issuer
            .verify(old.expose_for_transport(), &reference, f.scope(), &ctx)
            .is_err()
    );
    let grant = issuer
        .verify(fresh.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    assert!(grant.actor().unwrap() == actor);
    issuer.revoke(&parent, &fresh).unwrap();
    assert_eq!(grant.authorize(permission), Err(AuthError::Denied));
    assert!(issuer.refresh(&parent, &fresh, &f.scope()).is_err());
    issuer
        .verify(sibling.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap()
        .authorize(permission)
        .unwrap();
}
#[test]
fn delegation_requires_explicit_permission_and_revocation_reaches_descendants() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let coord = f.definition(
        Role::Coordinator,
        &[Permission::Delegate, read],
        1,
        limits(),
    );
    let cref = coord.reference().clone();
    let scout = f.definition(Role::Scout, &[read], 1, limits());
    let sref = scout.reference().clone();
    issuer.register_definition(&root, coord).unwrap();
    issuer.register_definition(&root, scout).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(
            &root,
            &run,
            &cref,
            &[Permission::Delegate, read],
            &f.scope(),
        )
        .unwrap();
    let grant = issuer
        .verify(token.expose_for_transport(), &cref, f.scope(), &ctx)
        .unwrap();
    let child = issuer
        .delegate(&grant, &sref, &[read], Duration::from_secs(900))
        .unwrap();
    let childgrant = issuer
        .verify(child.bearer.expose_for_transport(), &sref, f.scope(), &ctx)
        .unwrap();
    assert!(childgrant.actor().unwrap() != grant.actor().unwrap());
    assert_eq!(childgrant.authorize(read), Ok(Obligation::Unconditional));
    assert!(
        issuer
            .delegate(&childgrant, &sref, &[read], Duration::from_secs(1))
            .is_err()
    );
    assert!(
        issuer
            .delegate(
                &grant,
                &sref,
                &[Permission::Configure],
                Duration::from_secs(1)
            )
            .is_err()
    );
    issuer.revoke(&root, &token).unwrap();
    assert!(childgrant.authorize(read).is_err());
    assert!(
        issuer
            .verify(child.bearer.expose_for_transport(), &sref, f.scope(), &ctx)
            .is_err()
    );
}
#[test]
fn definition_rotation_run_close_restart_and_real_generation_invalidate_proofs() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let def = f.definition(Role::Scout, &[read], 1, limits());
    let reference = def.reference().clone();
    issuer.register_definition(&root, def).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(&root, &run, &reference, &[read], &f.scope())
        .unwrap();
    let grant = issuer
        .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let replacement = f.definition(Role::Scout, &[read], 2, limits());
    let rref = replacement.reference().clone();
    issuer
        .replace_definition(&root, &reference, replacement)
        .unwrap();
    assert!(grant.authorize(read).is_err());
    let next = issuer
        .issue(&root, &run, &rref, &[read], &f.scope())
        .unwrap();
    let proof = issuer
        .verify(next.expose_for_transport(), &rref, f.scope(), &ctx)
        .unwrap();
    issuer.close_run(&root, &run).unwrap();
    assert!(proof.authorize(read).is_err());
    let run2 = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token2 = issuer
        .issue(&root, &run2, &rref, &[read], &f.scope())
        .unwrap();
    let restarted = f.issuer(&ctx, limits());
    assert!(
        restarted
            .verify(token2.expose_for_transport(), &rref, f.scope(), &ctx)
            .is_err()
    );
    let old_bytes = f.secret();
    std::fs::remove_file(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let maintenance = request(&cancel);
    let result = f.runtime.maintain(maintenance.snapshot());
    assert_eq!(result.failure(), None);
    assert_eq!(&*f.secret(), &*old_bytes);
    assert!(root.check().is_err());
    assert!(
        issuer
            .verify(token2.expose_for_transport(), &rref, f.scope(), &ctx)
            .is_err()
    );
    let fresh = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    fresh.check().unwrap();
}
#[test]
fn wrong_identity_version_scope_and_native_replacement_fail_without_business_writes() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let other = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let def = f.definition(Role::Scout, &[read], 1, limits());
    let reference = def.reference().clone();
    issuer.register_definition(&root, def).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(&root, &run, &reference, &[read], &f.scope())
        .unwrap();
    let business = f.source_root.join("business-write");
    assert!(
        issuer
            .verify(
                token.expose_for_transport(),
                &reference,
                other.scope(),
                &ctx
            )
            .is_err()
    );
    let paths = ProjectPaths::at(
        &f.source_root,
        &InstallationPaths::resolve(Brand::default(), &f.source_root, Some(&f.source_root), None)
            .unwrap(),
    )
    .unwrap();
    let version =
        VersionContext::select(&paths, Checkout::NonGit, "other-version", None, "").unwrap();
    let mut wrong = f.scope();
    wrong.project.as_mut().unwrap().version = &version;
    assert!(
        issuer
            .verify(token.expose_for_transport(), &reference, wrong, &ctx)
            .is_err()
    );
    for malformed in [
        "",
        &f.secret(),
        "gr1_bad",
        "gr1_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        "gr1_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB",
    ] {
        let error = match issuer.verify(malformed, &reference, f.scope(), &ctx) {
            Ok(_) => panic!("invalid token admitted"),
            Err(e) => e,
        };
        assert!(!format!("{error:?} {error}").contains(f.secret().as_str()));
    }
    let unknown = RoleCredentialReference::new(Role::Dream, reference.source().clone(), [9; 32]);
    assert!(
        issuer
            .verify(token.expose_for_transport(), &unknown, f.scope(), &ctx)
            .is_err()
    );
    assert!(!business.exists());
    let saved = f.source_root.with_extension("old");
    #[cfg(not(windows))]
    {
        std::fs::rename(&f.source_root, &saved).unwrap();
        std::fs::create_dir(&f.source_root).unwrap();
        assert!(
            issuer
                .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
                .is_err()
        );
        assert!(!business.exists());
    }
    #[cfg(windows)]
    {
        let source_root = f.source_root.clone();
        assert!(std::fs::rename(&source_root, &saved).is_err());
        assert!(source_root.is_dir());
        assert!(!saved.exists());
        assert_eq!(
            issuer
                .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
                .unwrap()
                .authorize(read),
            Ok(Obligation::Unconditional)
        );
        assert!(!business.exists());
        drop(token);
        drop(caller_root);
        let Fixture {
            project,
            installation,
            runtime,
            _dir: directory,
            ..
        } = f;
        drop(project);
        drop(installation);
        drop(runtime);
        // The issuer's captured run facts still retain the original PROJECT anchor.
        assert!(std::fs::rename(&source_root, &saved).is_err());
        drop(issuer);
        std::fs::rename(&source_root, &saved).unwrap();
        assert!(!source_root.exists());
        assert!(saved.is_dir());
        assert!(!saved.join("business-write").exists());
        drop(directory);
    }
}

struct TestClock {
    start: Moment,
    seconds: AtomicU64,
    rollback: AtomicBool,
}
impl Clock for TestClock {
    fn now(&self) -> Moment {
        let duration = Duration::from_secs(self.seconds.load(Ordering::SeqCst));
        Moment {
            utc: if self.rollback.load(Ordering::SeqCst) {
                self.start.utc - Duration::from_secs(1)
            } else {
                self.start.utc + duration
            },
            mono: self.start.mono + duration,
        }
    }
}
struct TestEntropy {
    fail: AtomicBool,
    counter: AtomicU64,
}
impl Entropy for TestEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(AuthError::Entropy);
        }
        let n = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        bytes.fill(1);
        bytes[..8].copy_from_slice(&n.to_le_bytes());
        Ok(())
    }
}
#[test]
fn exact_expiry_clock_rollback_entropy_failure_and_cancellation() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let clock = Arc::new(TestClock {
        start: Moment {
            utc: SystemTime::now(),
            mono: Instant::now(),
        },
        seconds: AtomicU64::new(0),
        rollback: AtomicBool::new(false),
    });
    let entropy = Arc::new(TestEntropy {
        fail: AtomicBool::new(false),
        counter: AtomicU64::new(0),
    });
    let issuer = RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        authority(&all_permissions(), &all_permissions(), limits(), 7200),
        ttl(),
        &ctx,
        clock.clone(),
        entropy.clone(),
    )
    .unwrap();
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let def = f.definition(Role::Scout, &[read], 1, limits());
    let reference = def.reference().clone();
    issuer.register_definition(&root, def).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(&root, &run, &reference, &[read], &f.scope())
        .unwrap();
    let grant = issuer
        .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    entropy.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        issuer.refresh(&root, &token, &f.scope()),
        Err(AuthError::Entropy)
    ));
    assert_eq!(grant.authorize(read), Ok(Obligation::Unconditional));
    entropy.fail.store(false, Ordering::SeqCst);
    clock.seconds.store(780, Ordering::SeqCst);
    assert!(grant.refresh_due().unwrap());
    clock.rollback.store(true, Ordering::SeqCst);
    assert_eq!(grant.authorize(read), Err(AuthError::Expired));
    clock.rollback.store(false, Ordering::SeqCst);
    clock.seconds.store(900, Ordering::SeqCst);
    assert_eq!(grant.authorize(read), Err(AuthError::Expired));
    assert!(issuer.refresh(&root, &token, &f.scope()).is_err());
    cancel.cancel();
    assert_eq!(grant.authorize(read), Err(AuthError::Request));
}
#[test]
fn delegation_denies_other_native_binding_without_changing_registry() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let coordinator = f.definition(
        Role::Coordinator,
        &[Permission::Delegate, read],
        1,
        limits(),
    );
    let coordinator_ref = coordinator.reference().clone();
    let scout = f.definition_for_binding(Role::Scout, &[read], 1, limits(), NativeBinding::Codex);
    let scout_ref = scout.reference().clone();
    issuer.register_definition(&root, coordinator).unwrap();
    issuer.register_definition(&root, scout).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(
            &root,
            &run,
            &coordinator_ref,
            &[Permission::Delegate, read],
            &f.scope(),
        )
        .unwrap();
    let parent = issuer
        .verify(
            token.expose_for_transport(),
            &coordinator_ref,
            f.scope(),
            &ctx,
        )
        .unwrap();
    assert!(matches!(
        issuer.delegate(&parent, &scout_ref, &[read], Duration::from_secs(900)),
        Err(AuthError::Denied)
    ));
    let refreshed = issuer.refresh(&root, &token, &f.scope()).unwrap();
    assert!(parent.authorize(Permission::Delegate).is_err());
    assert!(parent.actor().is_err());
    assert!(
        issuer
            .delegate(&parent, &scout_ref, &[read], Duration::from_secs(1))
            .is_err()
    );
    assert!(
        issuer
            .verify(
                refreshed.expose_for_transport(),
                &coordinator_ref,
                f.scope(),
                &ctx
            )
            .is_ok()
    );
}

#[test]
fn concurrent_issuance_preserves_all_legitimate_credentials() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let policy = limits();
    let issuer = f.issuer(&ctx, policy);
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let definition = f.definition(Role::Scout, &[read], 1, policy);
    let reference = definition.reference().clone();
    issuer.register_definition(&root, definition).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let tokens = std::thread::scope(|threads| {
        let mut workers = Vec::new();
        for _ in 0..16 {
            let (issuer, root, run, reference, f) = (&issuer, &root, &run, &reference, &f);
            workers.push(
                threads.spawn(move || issuer.issue(root, run, reference, &[read], &f.scope())),
            );
        }
        let mut tokens = Vec::new();
        for worker in workers {
            tokens.push(worker.join().unwrap().unwrap());
        }
        tokens
    });
    assert_eq!(tokens.len(), 16);
    for token in &tokens {
        assert!(
            issuer
                .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
                .is_ok()
        );
    }
    assert!(
        issuer
            .verify(
                tokens[0].expose_for_transport(),
                &reference,
                f.scope(),
                &ctx
            )
            .is_ok()
    );
}

#[test]
fn new_core_observations_are_allowed_without_resetting_scope_identity() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let definition = f.definition(Role::Scout, &[read], 1, limits());
    let reference = definition.reference().clone();
    issuer.register_definition(&root, definition).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(&root, &run, &reference, &[read], &f.scope())
        .unwrap();
    std::fs::create_dir_all(f.project.runtime_root()).unwrap();
    f.project
        .revalidate(&ScopeContext::new(
            ctx.deadline().unwrap(),
            cancellation.clone(),
        ))
        .unwrap();
    let latest = f.project.binding().unwrap();
    assert!(latest.directory_count() > f.project_record.directory_count());
    let mut fresh = f.scope();
    fresh.project.as_mut().unwrap().scope.record = &latest;
    assert_eq!(
        issuer
            .verify(token.expose_for_transport(), &reference, fresh, &ctx)
            .unwrap()
            .authorize(read),
        Ok(Obligation::Unconditional)
    );
}

#[test]
fn partial_wire_observations_cannot_hide_replaced_project_and_provider_runtime() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    std::fs::create_dir_all(f.project.runtime_root()).unwrap();
    let home = f._dir.path().join("home");
    let paths =
        InstallationPaths::resolve(Brand::default(), &f.source_root, Some(&home), None).unwrap();
    let project_paths = ProjectPaths::at(&f.source_root, &paths).unwrap();
    let scope_limits = ScopeLimits::new(16, 4096, 128).unwrap();
    let scope_ctx = ScopeContext::new(ctx.deadline().unwrap(), cancellation.clone());
    let project_id = "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap();

    let original =
        CapturedPublicationScope::project(&project_paths, project_id, scope_limits, ctx.snapshot())
            .unwrap();
    let mut wire = serde_json::to_value(original.binding().unwrap()).unwrap();
    wire["directories"].as_array_mut().unwrap().retain(|d| {
        serde_json::from_value::<NativePath>(d["path"].clone())
            .unwrap()
            .as_path()
            == original.owner_root()
    });
    assert_eq!(wire["directories"].as_array().unwrap().len(), 1);
    let partial =
        ScopeBindingRecord::pure_restore(&serde_json::to_vec(&wire).unwrap(), scope_limits, 65536)
            .unwrap();
    assert_eq!(partial.directory_count(), 1);
    assert!(
        partial
            .matches_scope_bounded(&original, &scope_ctx)
            .unwrap()
    );
    let providers = [ScopeCapture {
        live: &original,
        record: &partial,
    }];
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().scope = providers[0];
    scope.providers = &providers;
    let issuer = f.issuer(&ctx, limits());
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let definition = f.definition(Role::Scout, &[read], 1, limits());
    let reference = definition.reference().clone();
    issuer.register_definition(&root, definition).unwrap();
    let run = issuer
        .start_run(&root, &scope, Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(&root, &run, &reference, &[read], &scope)
        .unwrap();
    #[cfg(not(windows))]
    let saved = original.runtime_root().with_extension("old");
    #[cfg(not(windows))]
    {
        std::fs::rename(original.runtime_root(), &saved).unwrap();
        std::fs::create_dir(original.runtime_root()).unwrap();
        let fresh = CapturedPublicationScope::project(
            &project_paths,
            project_id,
            scope_limits,
            ctx.snapshot(),
        )
        .unwrap();
        assert!(
            partial.matches_scope_bounded(&fresh, &scope_ctx).unwrap(),
            "data observations alone do not see replacement"
        );
        let fresh_providers = [ScopeCapture {
            live: &fresh,
            record: &partial,
        }];
        let mut new_scope = scope;
        new_scope.project.as_mut().unwrap().scope = fresh_providers[0];
        new_scope.providers = &fresh_providers;
        // The realm remains valid; the retained original Core proofs deny the request.
        root.check().unwrap();
        assert!(matches!(
            issuer.verify(token.expose_for_transport(), &reference, new_scope, &ctx),
            Err(AuthError::Denied)
        ));
        assert!(matches!(
            issuer.refresh(&root, &token, &new_scope),
            Err(AuthError::Denied)
        ));
        assert!(matches!(
            issuer.issue(&root, &run, &reference, &[read], &new_scope),
            Err(AuthError::Denied)
        ));
        assert!(!f.source_root.join("business-write").exists());
    }
    #[cfg(windows)]
    {
        let owner_root = original.owner_root().to_owned();
        let saved = owner_root.with_extension("old");
        assert!(std::fs::rename(&owner_root, &saved).is_err());
        assert!(owner_root.is_dir());
        assert!(!saved.exists());
        root.check().unwrap();
        assert_eq!(
            issuer
                .verify(token.expose_for_transport(), &reference, scope, &ctx)
                .unwrap()
                .authorize(read),
            Ok(Obligation::Unconditional)
        );
        assert!(!f.source_root.join("business-write").exists());
        drop(token);
        drop(caller_root);
        drop(original);
        let Fixture {
            project,
            installation,
            runtime,
            _dir: directory,
            ..
        } = f;
        drop(project);
        drop(installation);
        drop(runtime);
        // Both project and provider originals are still retained by the issuer's run facts.
        assert!(std::fs::rename(&owner_root, &saved).is_err());
        drop(issuer);
        std::fs::rename(&owner_root, &saved).unwrap();
        assert!(!owner_root.exists());
        assert!(saved.is_dir());
        drop(directory);
    }
}

#[test]
fn every_limit_requires_positive_nonoverflowing_input() {
    type LimitSetter = fn(&mut IssuerLimits, usize);
    let setters: [(Setting, LimitSetter); 3] = [
        (Setting::MaxPermissions, |l, n| l.max_permissions = n),
        (Setting::MaxScopeBytes, |l, n| l.max_scope_bytes = n),
        (Setting::MaxParentBearerBytes, |l, n| {
            l.max_parent_bearer_bytes = n
        }),
    ];
    for (field, set) in setters {
        for invalid in [0, usize::MAX] {
            let mut policy = limits();
            set(&mut policy, invalid);
            let error = policy.validate().unwrap_err();
            assert_eq!(error, AuthError::InvalidSetting(field));
            assert!(error.to_string().ends_with(field.as_str()));
        }
    }
    for policy in [
        TtlPolicy {
            ttl: Duration::ZERO,
            ..ttl()
        },
        TtlPolicy {
            maximum: Duration::ZERO,
            ..ttl()
        },
        TtlPolicy {
            refresh_margin: Duration::ZERO,
            ..ttl()
        },
        TtlPolicy {
            refresh_margin: Duration::from_secs(451),
            ..ttl()
        },
        TtlPolicy {
            refresh_margin: ttl().ttl,
            ..ttl()
        },
        TtlPolicy {
            maximum: Duration::from_secs(1),
            ..ttl()
        },
    ] {
        assert!(matches!(
            policy.validate(),
            Err(AuthError::InvalidSetting(_))
        ));
    }
}

#[test]
fn permission_and_parent_input_bounds_preserve_existing_grants() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let read = Permission::Read(Module::Task);
    let mut policy = limits();
    policy.max_parent_bearer_bytes = 64;
    let issuer = f.issuer(&ctx, policy);
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    assert!(matches!(
        issuer.authenticate_runtime(&"a".repeat(64 + 1), &ctx),
        Err(AuthError::Limit)
    ));
    let definition = f.definition(Role::Scout, &[read], 1, policy);
    let reference = definition.reference().clone();
    issuer.register_definition(&root, definition).unwrap();
    issuer
        .register_definition(&root, f.definition(Role::Tracker, &[read], 1, policy))
        .unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let sibling_run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    issuer.close_run(&root, &sibling_run).unwrap();
    let token = issuer
        .issue(&root, &run, &reference, &[read], &f.scope())
        .unwrap();
    assert!(
        issuer
            .issue(
                &root,
                &run,
                &reference,
                &[read, Permission::Configure],
                &f.scope()
            )
            .is_err()
    );
    issuer
        .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap()
        .authorize(read)
        .unwrap();
    let mut one_permission = limits();
    one_permission.max_permissions = 1;
    assert_eq!(
        f.scope().validate(
            &InstallationId::new("local-installation").unwrap(),
            f.installation.brand().key(),
            &ctx,
        ),
        Ok(())
    );
    let source_definition = f.definition(Role::Scout, &[read], 1, one_permission);
    assert_eq!(source_definition.permissions(), &[read]);
    let mut too_short = policy;
    too_short.max_parent_bearer_bytes -= 1;
    let short = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        authority(&[read], &[read], too_short, 3600),
        ttl(),
        &ctx,
    )
    .unwrap();
    assert!(matches!(
        short.authenticate_runtime(&f.secret(), &ctx),
        Err(AuthError::Limit)
    ));
}

struct ControlledEntropy {
    counter: AtomicU64,
    calls: AtomicU64,
    repeat: AtomicBool,
    fail: AtomicBool,
    cancel_next: AtomicBool,
    cancellation: Mutex<Cancellation>,
}
impl Entropy for ControlledEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(AuthError::Entropy);
        }
        let n = if self.repeat.load(Ordering::SeqCst) {
            self.counter.load(Ordering::SeqCst)
        } else {
            self.counter.fetch_add(1, Ordering::SeqCst) + 1
        };
        bytes.fill(1);
        bytes[..8].copy_from_slice(&n.to_le_bytes());
        if self.cancel_next.swap(false, Ordering::SeqCst) {
            self.cancellation.lock().unwrap().cancel();
        }
        Ok(())
    }
}

#[test]
fn entropy_collision_failure_and_cancel_during_nonce_leave_previous_registry_intact() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let entropy = Arc::new(ControlledEntropy {
        counter: AtomicU64::new(0),
        calls: AtomicU64::new(0),
        repeat: AtomicBool::new(false),
        fail: AtomicBool::new(false),
        cancel_next: AtomicBool::new(false),
        cancellation: Mutex::new(cancellation.clone()),
    });
    let issuer = RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        authority(&all_permissions(), &all_permissions(), limits(), 7200),
        ttl(),
        &ctx,
        Arc::new(SystemClock),
        entropy.clone(),
    )
    .unwrap();
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let definition = f.definition(
        Role::Coordinator,
        &[Permission::Delegate, read],
        1,
        limits(),
    );
    let reference = definition.reference().clone();
    issuer.register_definition(&root, definition).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(
            &root,
            &run,
            &reference,
            &[Permission::Delegate, read],
            &f.scope(),
        )
        .unwrap();
    assert_eq!(entropy.counter.load(Ordering::SeqCst), 4);
    entropy.repeat.store(true, Ordering::SeqCst);
    let calls = entropy.calls.load(Ordering::SeqCst);
    assert!(matches!(
        issuer.refresh(&root, &token, &f.scope()),
        Err(AuthError::Entropy)
    ));
    assert_eq!(entropy.calls.load(Ordering::SeqCst) - calls, 8);
    entropy.repeat.store(false, Ordering::SeqCst);
    entropy.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        issuer.issue(&root, &run, &reference, &[read], &f.scope()),
        Err(AuthError::Entropy)
    ));
    entropy.fail.store(false, Ordering::SeqCst);
    entropy.cancel_next.store(true, Ordering::SeqCst);
    assert!(matches!(
        issuer.refresh(&root, &token, &f.scope()),
        Err(AuthError::Request)
    ));
    let fresh_cancel = Cancellation::default();
    let fresh_ctx = request(&fresh_cancel);
    let caller_parent = issuer
        .authenticate_runtime(&f.secret(), &fresh_ctx)
        .unwrap();
    let parent = issuer.admin_parent(&caller_parent, &session_root).unwrap();
    let old = issuer
        .verify(
            token.expose_for_transport(),
            &reference,
            f.scope(),
            &fresh_ctx,
        )
        .unwrap();
    old.authorize(read).unwrap();
    for operation in 0..4 {
        let op_cancel = Cancellation::default();
        let op_ctx = request(&op_cancel);
        let caller_op_parent = issuer.authenticate_runtime(&f.secret(), &op_ctx).unwrap();
        let op_parent = issuer
            .admin_parent(&caller_op_parent, &session_root)
            .unwrap();
        let op_grant = issuer
            .verify(token.expose_for_transport(), &reference, f.scope(), &op_ctx)
            .unwrap();
        *entropy.cancellation.lock().unwrap() = op_cancel.clone();
        entropy.cancel_next.store(true, Ordering::SeqCst);
        let error = match operation {
            0 => issuer
                .start_run(&op_parent, &f.scope(), Duration::from_secs(3600))
                .err(),
            1 => issuer
                .issue(&op_parent, &run, &reference, &[read], &f.scope())
                .err(),
            2 => issuer.refresh(&op_parent, &token, &f.scope()).err(),
            _ => issuer
                .delegate(&op_grant, &reference, &[read], Duration::from_secs(900))
                .err(),
        };
        assert_eq!(error, Some(AuthError::Request));
        assert!(op_cancel.is_cancelled());
        old.authorize(read).unwrap();
    }
    parent.check().unwrap();
}

#[test]
fn exact_parent_expiry_caps_root_child_and_refresh_without_extending_run() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let clock = Arc::new(TestClock {
        start: Moment {
            utc: SystemTime::now(),
            mono: Instant::now(),
        },
        seconds: AtomicU64::new(0),
        rollback: AtomicBool::new(false),
    });
    let entropy = Arc::new(TestEntropy {
        fail: AtomicBool::new(false),
        counter: AtomicU64::new(0),
    });
    let issuer = RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        authority(&all_permissions(), &all_permissions(), limits(), 60),
        ttl(),
        &ctx,
        clock.clone(),
        entropy,
    )
    .unwrap();
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let definition = f.definition(
        Role::Coordinator,
        &[Permission::Delegate, read],
        1,
        limits(),
    );
    let reference = definition.reference().clone();
    issuer.register_definition(&root, definition).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(
            &root,
            &run,
            &reference,
            &[Permission::Delegate, read],
            &f.scope(),
        )
        .unwrap();
    let grant = issuer
        .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let child = issuer
        .delegate(&grant, &reference, &[read], Duration::from_secs(3600))
        .unwrap();
    clock.seconds.store(59, Ordering::SeqCst);
    grant.authorize(read).unwrap();
    let refreshed = issuer.refresh(&root, &token, &f.scope()).unwrap();
    issuer
        .verify(
            child.bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &ctx,
        )
        .unwrap()
        .authorize(read)
        .unwrap();
    clock.seconds.store(60, Ordering::SeqCst);
    assert!(matches!(root.check(), Err(AuthError::Expired)));
    for bearer in [&refreshed, &child.bearer] {
        assert!(matches!(
            issuer.verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx),
            Err(AuthError::Expired)
        ));
    }
    assert!(matches!(
        issuer.refresh(&root, &refreshed, &f.scope()),
        Err(AuthError::Expired)
    ));
}

#[test]
fn root_scope_authorization_denies_unconfigured_project_and_provider_roots() {
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let other = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let mut scope = f.scope();
    scope.project = other.scope().project;
    assert!(matches!(
        issuer.start_run(&root, &scope, Duration::from_secs(3600)),
        Err(AuthError::Denied)
    ));
    let home = f._dir.path().join("home");
    let paths =
        InstallationPaths::resolve(Brand::default(), &f.source_root, Some(&home), None).unwrap();
    let provider = CapturedPublicationScope::installation(
        &paths,
        InstallationId::new("local-installation").unwrap(),
        [other.source_root.clone()],
        ScopeLimits::new(16, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    assert_eq!(provider.owner_root(), f.installation.owner_root());
    let provider_record = provider.binding().unwrap();
    let providers = [ScopeCapture {
        live: &provider,
        record: &provider_record,
    }];
    let mut scope = f.scope();
    scope.providers = &providers;
    assert!(matches!(
        issuer.start_run(&root, &scope, Duration::from_secs(3600)),
        Err(AuthError::Denied)
    ));
    assert!(issuer.lock().unwrap().runs.is_empty());
    assert!(issuer.lock().unwrap().tokens.is_empty());
    root.check().unwrap();
}

#[test]
fn unavailable_registry_fails_closed_without_exposing_secrets() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller_root = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session_root = issuer.bootstrap_admin(&caller_root).unwrap();
    let root = issuer.admin_parent(&caller_root, &session_root).unwrap();
    let read = Permission::Read(Module::Task);
    let definition = f.definition(Role::Scout, &[read], 1, limits());
    let reference = definition.reference().clone();
    issuer.register_definition(&root, definition).unwrap();
    let run = issuer
        .start_run(&root, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(&root, &run, &reference, &[read], &f.scope())
        .unwrap();
    let grant = issuer
        .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    std::thread::scope(|threads| {
        assert!(
            threads
                .spawn(|| {
                    let _guard = issuer.state.lock().unwrap();
                    panic!("test registry failure");
                })
                .join()
                .is_err()
        );
    });
    for result in [root.check(), grant.authorize(read).map(|_| ())] {
        let error = result.unwrap_err();
        assert_eq!(error, AuthError::State);
        assert!(!format!("{error:?} {error}").contains(f.secret().as_str()));
        assert!(!format!("{error:?} {error}").contains(token.expose_for_transport()));
    }
    let encoded_reference = reference.encode(4096, &ctx).unwrap();
    assert!(
        !std::str::from_utf8(&encoded_reference)
            .unwrap()
            .contains(f.secret().as_str())
    );
}

#[test]
fn original_request_deadline_expires_proof_without_resetting_valid_role_token() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let setup_ctx = request(&cancellation);
    let f = Fixture::new(&setup_ctx);
    let issuer = f.issuer(&setup_ctx, limits());
    let caller_setup_parent = issuer
        .authenticate_runtime(&f.secret(), &setup_ctx)
        .unwrap();
    let session_setup_parent = issuer.bootstrap_admin(&caller_setup_parent).unwrap();
    let setup_parent = issuer
        .admin_parent(&caller_setup_parent, &session_setup_parent)
        .unwrap();
    let read = Permission::Read(Module::Task);
    let definition = f.definition(Role::Scout, &[read], 1, limits());
    let reference = definition.reference().clone();
    issuer
        .register_definition(&setup_parent, definition)
        .unwrap();
    let run = issuer
        .start_run(&setup_parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(&setup_parent, &run, &reference, &[read], &f.scope())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    let ctx = request_before(&cancellation, deadline);
    let caller_parent = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let parent = issuer
        .admin_parent(&caller_parent, &session_setup_parent)
        .unwrap();
    let grant = issuer
        .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    grant.authorize(read).unwrap();
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    assert_eq!(parent.check(), Err(AuthError::Request));
    assert_eq!(grant.authorize(read), Err(AuthError::Request));
    assert!(matches!(
        issuer.refresh(&parent, &token, &f.scope()),
        Err(AuthError::Request)
    ));
    issuer
        .verify(
            token.expose_for_transport(),
            &reference,
            f.scope(),
            &setup_ctx,
        )
        .unwrap()
        .authorize(read)
        .unwrap();
}

#[test]
fn role_definition_and_parent_permissions_reject_bad_sources_duplicates_and_over_budget() {
    use graphit_core::module_settings::Module;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation);
    let f = Fixture::new(&ctx);
    let read = Permission::Read(Module::Task);
    let source = f.source(Role::Scout, ArtifactType::Agent);
    let definition = |source: &SourceRef, permissions: &[Permission], policy| {
        RoleDefinition::new(
            Role::Scout,
            source,
            NativeBinding::KiroV3,
            permissions,
            [1; 32],
            policy,
        )
    };
    assert!(matches!(
        definition(
            &f.source(Role::Scout, ArtifactType::Skill),
            &[read],
            limits()
        ),
        Err(AuthError::Invalid)
    ));
    assert!(matches!(
        definition(&source, &[read, read], limits()),
        Err(AuthError::Invalid)
    ));
    assert!(matches!(
        definition(&source, &[Permission::Configure], limits()),
        Err(AuthError::Denied)
    ));
    let mut policy = limits();
    policy.max_permissions = 1;
    assert!(definition(&source, &[read], policy).is_ok());
    assert!(matches!(
        definition(&source, &[read, Permission::Read(Module::Memory)], policy),
        Err(AuthError::Limit)
    ));
    assert!(matches!(
        authority_result(
            &[read, Permission::Read(Module::Memory)],
            &[],
            crate::AdminSessionPolicy {
                lifetime: Duration::from_secs(60),
                maximum: Duration::from_secs(60)
            },
            policy
        ),
        Err(AuthError::Limit)
    ));
}

std::thread_local! {
    static BEFORE_FINAL_SOURCE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const {std::cell::RefCell::new(None)};
}
pub(super) fn before_final_source() {
    let hook = BEFORE_FINAL_SOURCE.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

fn request_for_documents(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 256,
            max_collection_entries: 64,
        },
        cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn clock() -> Arc<TestClock> {
    Arc::new(TestClock {
        start: Moment {
            utc: SystemTime::now(),
            mono: Instant::now(),
        },
        seconds: AtomicU64::new(0),
        rollback: AtomicBool::new(false),
    })
}
fn fixture_issuer(
    f: &Fixture,
    ctx: &RequestContext<'_>,
    clock: Arc<TestClock>,
    ordinary: &[Permission],
    admin: &[Permission],
    cap: IssuerLimits,
    seconds: u64,
) -> RoleIssuer {
    RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(ctx),
        authority(ordinary, admin, cap, seconds),
        ttl(),
        ctx,
        clock,
        Arc::new(OsEntropy),
    )
    .unwrap()
}

#[test]
fn ordinary_runtime_has_no_admin_expiry_or_implicit_session_and_actor_is_server_owned() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let clock = clock();
    let read = Permission::Read(Module::Task);
    let issuer = fixture_issuer(
        &f,
        &ctx,
        Arc::clone(&clock),
        &[Permission::WriteOwnTask],
        &[Permission::ManageAccessGrants, read],
        limits(),
        60,
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert_eq!(issuer.lock().unwrap().sessions.len(), 0);
    assert_eq!(
        caller.authorize(Permission::WriteOwnTask),
        Ok(Obligation::OwnActor)
    );
    assert_eq!(caller.authorize(read), Err(AuthError::Denied));
    let actor = actor_bytes(&caller.actor().unwrap());
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    clock.seconds.store(60, Ordering::SeqCst);
    assert_eq!(parent.check(), Err(AuthError::Expired));
    caller.authorize(Permission::WriteOwnTask).unwrap();
    assert_eq!(actor_bytes(&caller.actor().unwrap()), actor);
    assert_eq!(issuer.lock().unwrap().sessions.len(), 1);
    assert!(issuer.lock().unwrap().runs.is_empty());
    assert!(issuer.lock().unwrap().credentials.is_empty());
}

#[test]
fn new_session_cannot_adopt_other_live_run_or_revive_expired_lineage() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let clock = clock();
    let issuer = fixture_issuer(
        &f,
        &ctx,
        Arc::clone(&clock),
        &all_permissions(),
        &all_permissions(),
        limits(),
        60,
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let first = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &first).unwrap();
    let read = Permission::Read(Module::Task);
    let def = f.definition(
        Role::Coordinator,
        &[Permission::Delegate, read],
        1,
        limits(),
    );
    let reference = def.reference().clone();
    issuer.register_definition(&parent, def).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let token = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[Permission::Delegate, read],
            &f.scope(),
        )
        .unwrap();
    let second = issuer.bootstrap_admin(&caller).unwrap();
    let other = issuer.admin_parent(&caller, &second).unwrap();
    assert!(matches!(
        issuer.issue(&other, &run, &reference, &[read], &f.scope()),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        issuer.refresh(&other, &token, &f.scope()),
        Err(AuthError::Denied)
    ));
    assert_eq!(issuer.revoke(&other, &token), Err(AuthError::Denied));
    assert_eq!(issuer.close_run(&other, &run), Err(AuthError::Denied));
    let grant = issuer
        .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    clock.seconds.store(60, Ordering::SeqCst);
    assert_eq!(grant.authorize(read), Err(AuthError::Expired));
    assert!(matches!(
        issuer.delegate(&grant, &reference, &[read], Duration::from_secs(60)),
        Err(AuthError::Expired)
    ));
    let new = issuer.bootstrap_admin(&caller).unwrap();
    let fresh = issuer.admin_parent(&caller, &new).unwrap();
    assert!(matches!(
        issuer.issue(&fresh, &run, &reference, &[read], &f.scope()),
        Err(AuthError::Denied)
    ));
    assert!(
        issuer
            .verify(token.expose_for_transport(), &reference, f.scope(), &ctx)
            .is_err()
    );
    let restarted = f.issuer(&ctx, limits());
    let restarted_caller = restarted.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        restarted.admin_parent(&restarted_caller, &new),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        restarted.bootstrap_admin(&caller),
        Err(AuthError::Denied)
    ));
}

#[test]
fn management_permission_is_required_even_for_a_valid_subset() {
    use graphit_core::module_settings::Module;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let read = Permission::Read(Module::Task);
    let issuer = fixture_issuer(&f, &ctx, clock(), &all_permissions(), &[read], limits(), 60);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    assert_eq!(
        issuer.register_definition(&parent, f.definition(Role::Scout, &[read], 1, limits())),
        Err(AuthError::Denied)
    );
    // A registered definition still cannot be replaced after removal of the explicit management ceiling.
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let first = f.definition(Role::Scout, &[read], 1, limits());
    let reference = first.reference().clone();
    issuer.register_definition(&parent, first).unwrap();
    // The second policy is immutable; exercise the register/replace branch on a server fixture without that permission.
    let restricted = fixture_issuer(&f, &ctx, clock(), &[], &[read], limits(), 60);
    {
        let mut state = restricted.lock().unwrap();
        let definition = f.definition(Role::Scout, &[read], 1, limits());
        state.profiles.push(Profile {
            definition,
            generation: 1,
        });
    }
    let caller = restricted.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = restricted.bootstrap_admin(&caller).unwrap();
    let parent = restricted.admin_parent(&caller, &session).unwrap();
    assert_eq!(
        restricted.replace_definition(
            &parent,
            &reference,
            f.definition(Role::Scout, &[read], 2, limits())
        ),
        Err(AuthError::Denied)
    );
}

#[test]
fn session_policy_is_explicit_and_expiry_prunes_only_expired_sessions() {
    use crate::AdminSessionPolicy;
    for (lifetime, maximum) in [
        (Duration::ZERO, Duration::from_secs(1)),
        (Duration::from_secs(2), Duration::from_secs(1)),
        (Duration::from_secs(1), Duration::ZERO),
    ] {
        assert!(
            authority_result(&[], &[], AdminSessionPolicy { lifetime, maximum }, limits()).is_err()
        );
    }
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let clock = clock();
    let issuer = fixture_issuer(&f, &ctx, Arc::clone(&clock), &[], &[], limits(), 60);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let first = issuer.bootstrap_admin(&caller).unwrap();
    let second = issuer.bootstrap_admin(&caller).unwrap();
    issuer.admin_parent(&caller, &second).unwrap();
    issuer.admin_parent(&caller, &first).unwrap();
    clock.seconds.store(60, Ordering::SeqCst);
    let next = issuer.bootstrap_admin(&caller).unwrap();
    assert_ne!(first.id, next.id);
    assert_eq!(issuer.lock().unwrap().sessions.len(), 1);
    let policy = authority_result(
        &[],
        &[],
        AdminSessionPolicy {
            lifetime: Duration::MAX,
            maximum: Duration::MAX,
        },
        limits(),
    )
    .unwrap();
    let overflow = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        policy,
        ttl(),
        &ctx,
    )
    .unwrap();
    let caller = overflow.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        overflow.bootstrap_admin(&caller),
        Err(AuthError::Invalid)
    ));
    assert!(overflow.lock().unwrap().sessions.is_empty());
}

#[test]
fn session_entropy_collision_failure_and_cancel_publish_nothing() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let entropy = Arc::new(ControlledEntropy {
        counter: AtomicU64::new(0),
        calls: AtomicU64::new(0),
        repeat: AtomicBool::new(false),
        fail: AtomicBool::new(false),
        cancel_next: AtomicBool::new(false),
        cancellation: Mutex::new(cancel.clone()),
    });
    let issuer = RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        authority(&[], &[], limits(), 60),
        ttl(),
        &ctx,
        Arc::new(SystemClock),
        entropy.clone(),
    )
    .unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let first = issuer.bootstrap_admin(&caller).unwrap();
    entropy.repeat.store(true, Ordering::SeqCst);
    let calls = entropy.calls.load(Ordering::SeqCst);
    assert!(matches!(
        issuer.bootstrap_admin(&caller),
        Err(AuthError::Entropy)
    ));
    assert_eq!(entropy.calls.load(Ordering::SeqCst) - calls, 8);
    issuer.admin_parent(&caller, &first).unwrap();
    entropy.repeat.store(false, Ordering::SeqCst);
    entropy.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        issuer.bootstrap_admin(&caller),
        Err(AuthError::Entropy)
    ));
    entropy.fail.store(false, Ordering::SeqCst);
    entropy.cancel_next.store(true, Ordering::SeqCst);
    assert!(matches!(
        issuer.bootstrap_admin(&caller),
        Err(AuthError::Request)
    ));
    assert_eq!(issuer.lock().unwrap().sessions.len(), 1);
}

#[test]
fn runtime_generation_cannot_be_rolled_back_by_an_old_observation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let old = caller.source.generation();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let secret = f.secret();
    std::fs::remove_file(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    assert!(issuer.authenticate_runtime(&secret, &ctx).is_err());
    assert!(!f.installation.owner_root().join("daemon/mcp.key").exists());
    let maintenance = request(&cancel);
    assert_eq!(f.runtime.maintain(maintenance.snapshot()).failure(), None);
    assert_eq!(&*f.secret(), &*secret);
    let fresh = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    assert_eq!(fresh.source.generation(), old + 1);
    assert!(caller.check().is_err());
    assert!(issuer.admin_parent(&fresh, &session).is_err());
    {
        let mut state = issuer.lock().unwrap();
        assert_eq!(
            issuer.observe_generation(&mut state, old),
            Err(AuthError::Denied)
        );
        assert_eq!(state.runtime_generation, old + 1);
    }
    let operation = request(&cancel);
    assert_eq!(f.runtime.remove(operation.snapshot()).failure(), None);
    assert!(fresh.check().is_err());
    assert!(issuer.authenticate_runtime(&secret, &ctx).is_err());
    assert!(!f.installation.owner_root().join("daemon/mcp.key").exists());
}

#[test]
fn concurrent_runtime_requests_authenticate_and_authorize_actual_records() {
    let cancel = Cancellation::default();
    let setup = request(&cancel);
    let f = Fixture::new(&setup);
    let issuer = f.issuer(&setup, limits());
    let secret = f.secret();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        let run = || {
            let cancel = Cancellation::default();
            let ctx = request(&cancel);
            barrier.wait();
            let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
            caller.check().unwrap();
            caller
                .authorize(Permission::Read(
                    graphit_core::module_settings::Module::Task,
                ))
                .unwrap();
            let authority = crate::ProjectRecordAuthority::new(&issuer);
            let scope = f.scope();
            let admission = authority
                .admit_authenticated_read(&caller, &ctx, scope, crate::ProjectReadModule::Task)
                .unwrap();
            admission.recheck().unwrap();
            assert_eq!(admission.module(), crate::ProjectReadModule::Task);
        };
        let first = threads.spawn(run);
        let second = threads.spawn(run);
        first.join().unwrap();
        second.join().unwrap();
    });
}

#[test]
fn wrong_real_owner_and_brand_never_construct_authority() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let g = Fixture::new(&ctx);
    assert!(matches!(
        RoleIssuer::new(
            g.realm(),
            Arc::clone(&f.runtime),
            f.auth_store(&ctx),
            authority(&[], &[], limits(), 60),
            ttl(),
            &ctx
        ),
        Err(AuthError::Denied)
    ));
    let paths = InstallationPaths::resolve(
        Brand::new("otherbrand").unwrap(),
        &f.source_root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let wrong = CapturedPublicationScope::installation(
        &paths,
        InstallationId::new("local-installation").unwrap(),
        [f.source_root.clone()],
        ScopeLimits::new(16, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let record = wrong.binding().unwrap();
    assert!(matches!(
        RoleIssuer::new(
            ScopeCapture {
                live: &wrong,
                record: &record
            },
            Arc::clone(&f.runtime),
            f.auth_store(&ctx),
            authority(&[], &[], limits(), 60),
            ttl(),
            &ctx
        ),
        Err(AuthError::Denied)
    ));
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    caller.check().unwrap();
    assert!(issuer.bootstrap_admin(&caller).is_ok());
}

#[cfg(unix)]
#[test]
fn foreign_equal_bytes_and_nonprivate_key_are_denied_without_repair() {
    use std::{
        fs::OpenOptions,
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    for foreign in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, limits());
        let secret = f.secret();
        let key = f.installation.owner_root().join("daemon/mcp.key");
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        if foreign {
            std::fs::rename(&key, key.with_extension("retained")).unwrap();
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&key)
                .unwrap();
            file.write_all(secret.as_bytes()).unwrap();
            file.sync_all().unwrap();
        } else {
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        assert!(caller.check().is_err());
        assert!(issuer.authenticate_runtime(&secret, &ctx).is_err());
        assert_eq!(std::fs::read(&key).unwrap(), secret.as_bytes());
        if !foreign {
            assert_eq!(
                std::fs::metadata(&key).unwrap().permissions().mode() & 0o777,
                0o644
            );
        }
    }
}

#[test]
fn final_source_check_denies_change_or_cancellation_after_full_scope_inspection() {
    use graphit_core::module_settings::Module;
    for change in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, limits());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let read = Permission::Read(Module::Task);
        let def = f.definition(Role::Scout, &[read], 1, limits());
        let reference = def.reference().clone();
        issuer.register_definition(&parent, def).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(60))
            .unwrap();
        let token = issuer
            .issue(&parent, &run, &reference, &[read], &f.scope())
            .unwrap();
        let key = f.installation.owner_root().join("daemon/mcp.key");
        let original = f.secret();
        let captured_cancel = cancel.clone();
        BEFORE_FINAL_SOURCE.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                if change {
                    std::fs::remove_file(&key).unwrap();
                } else {
                    captured_cancel.cancel();
                }
            }))
        });
        let effect_ctx = request(&cancel);
        let result = issuer.verify(
            token.expose_for_transport(),
            &reference,
            f.scope(),
            &effect_ctx,
        );
        assert!(result.is_err());

        if change {
            assert!(!f.installation.owner_root().join("daemon/mcp.key").exists());
            assert_eq!(
                f.runtime
                    .check(request(&Cancellation::default()).snapshot())
                    .err(),
                Some(RuntimeKeyError::Conflict)
            );
        } else {
            assert_eq!(&*f.secret(), &*original);
        }
    }
}

#[test]
fn refresh_rechecks_old_credential_after_entropy_before_publishing_new_nonce() {
    use graphit_core::module_settings::Module;

    struct AdvanceOnEntropy {
        clock: Arc<TestClock>,
        next_second: AtomicU64,
        entropy: TestEntropy,
    }
    impl Entropy for AdvanceOnEntropy {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
            self.entropy.fill(bytes)?;
            let next = self.next_second.swap(0, Ordering::SeqCst);
            if next != 0 {
                self.clock.seconds.store(next, Ordering::SeqCst);
            }
            Ok(())
        }
    }

    // Both enter refresh before the old900s expiry. Only the second crosses it
    // during nonce generation; session7200/run3600/new expiry1798 stay valid.
    for after_entropy in [899, 901] {
        let cancel = Cancellation::default();
        let ctx = request_before(&cancel, Instant::now() + Duration::from_secs(10800));
        let f = Fixture::new(&ctx);
        let clock = clock();
        let entropy = Arc::new(AdvanceOnEntropy {
            clock: Arc::clone(&clock),
            next_second: AtomicU64::new(0),
            entropy: TestEntropy {
                fail: AtomicBool::new(false),
                counter: AtomicU64::new(0),
            },
        });
        let issuer = RoleIssuer::with_sources(
            f.realm(),
            Arc::clone(&f.runtime),
            f.auth_store(&ctx),
            authority(&all_permissions(), &all_permissions(), limits(), 7200),
            ttl(),
            &ctx,
            clock.clone(),
            entropy.clone(),
        )
        .unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let read = Permission::Read(Module::Task);
        let definition = f.definition(Role::Scout, &[read], 1, limits());
        let reference = definition.reference().clone();
        issuer.register_definition(&parent, definition).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(3600))
            .unwrap();
        let token = issuer
            .issue(&parent, &run, &reference, &[read], &f.scope())
            .unwrap();
        let old_hash = token_hash(token.expose_for_transport()).unwrap();
        let (id, old_expiry, before_tokens, run_next) = {
            let state = issuer.lock().unwrap();
            let id = state.tokens[&old_hash];
            (
                id,
                state.credentials[&id].expiry,
                state.tokens.clone(),
                state.runs[&run.id].next,
            )
        };
        clock.seconds.store(898, Ordering::SeqCst);
        issuer
            .check_credential(&issuer.lock().unwrap(), id, &ctx)
            .unwrap();
        entropy.next_second.store(after_entropy, Ordering::SeqCst);
        let refreshed = issuer.refresh(&parent, &token, &f.scope());
        assert_eq!(clock.seconds.load(Ordering::SeqCst), after_entropy);
        // A parent/run/new expiry failure must not accidentally qualify this case.
        let now = clock.now();
        let state = issuer.lock().unwrap();
        issuer.parent_locked(&parent, &state).unwrap();
        state.runs[&run.id].expiry.remaining(now).unwrap();
        Expiry::after(
            ttl().ttl,
            Moment {
                utc: clock.start.utc + Duration::from_secs(898),
                mono: clock.start.mono + Duration::from_secs(898),
            },
        )
        .unwrap()
        .remaining(now)
        .unwrap();
        assert_eq!(state.runs[&run.id].next, run_next);
        if after_entropy == 901 {
            assert_eq!(refreshed.err(), Some(AuthError::Expired));
            assert!(state.tokens == before_tokens);
            assert_eq!(state.credentials[&id].hash, old_hash);
            assert_eq!(state.credentials[&id].expiry.utc, old_expiry.utc);
            assert_eq!(state.credentials[&id].expiry.mono, old_expiry.mono);
            assert_eq!(state.credentials.len(), 1);
            assert_eq!(state.runs.len(), 1);
            assert_eq!(state.sessions.len(), 1);
            assert_eq!(old_expiry.remaining(now), Err(AuthError::Expired));
        } else {
            old_expiry.remaining(now).unwrap();
            let refreshed = refreshed.unwrap();
            let new_hash = token_hash(refreshed.expose_for_transport()).unwrap();
            assert_ne!(new_hash, old_hash);
            assert!(!state.tokens.contains_key(&old_hash));
            assert!(state.tokens.get(&new_hash) == Some(&id));
            drop(state);
            issuer
                .verify(
                    refreshed.expose_for_transport(),
                    &reference,
                    f.scope(),
                    &ctx,
                )
                .unwrap()
                .authorize(read)
                .unwrap();
        }
    }
}

fn deny_services() -> crate::ServiceAuthorityInput<'static> {
    crate::ServiceAuthorityInput {
        rules: &[],
        limits: crate::ServiceAdmissionLimits {
            max_rules: 16,
            max_binding_bytes: 65536,
        },
        native_refresh_margin: Duration::ZERO,
    }
}
mod hub_credentials;
mod hub_write_credentials;
mod service_credentials;
mod user_subject;

#[test]
fn physical_scope_subset_preserves_actual_captures_without_native_binding() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let other = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let providers = [f.realm(), f.scope().project.unwrap().scope];
    let physical = PhysicalScopeInput {
        installation: f.realm(),
        project: f.scope().project,
        providers: &providers,
    };
    assert_eq!(issuer.validate_physical_scope(&physical, &ctx), Ok(()));
    let wrong_record = PhysicalScopeInput {
        installation: ScopeCapture {
            live: &f.installation,
            record: &other.installation_record,
        },
        ..physical
    };
    assert_eq!(
        issuer.validate_physical_scope(&wrong_record, &ctx),
        Err(AuthError::Denied)
    );
    let foreign_installation = PhysicalScopeInput {
        installation: other.realm(),
        ..physical
    };
    assert_eq!(
        issuer.validate_physical_scope(&foreign_installation, &ctx),
        Err(AuthError::Denied)
    );
    let wrong_version = PhysicalScopeInput {
        project: Some(ProjectCapture {
            scope: f.scope().project.unwrap().scope,
            version: &other.version,
        }),
        ..physical
    };
    assert_eq!(
        issuer.validate_physical_scope(&wrong_version, &ctx),
        Err(AuthError::Denied)
    );
    let wrong_owner = PhysicalScopeInput {
        project: Some(ProjectCapture {
            scope: f.realm(),
            version: &f.version,
        }),
        ..physical
    };
    assert_eq!(
        issuer.validate_physical_scope(&wrong_owner, &ctx),
        Err(AuthError::Denied)
    );
    let foreign_providers = [other.scope().project.unwrap().scope];
    let foreign_provider = PhysicalScopeInput {
        providers: &foreign_providers,
        ..physical
    };
    assert_eq!(
        issuer.validate_physical_scope(&foreign_provider, &ctx),
        Err(AuthError::Denied)
    );
    assert_eq!(
        physical.validate(
            &InstallationId::new("local-installation").unwrap(),
            "other-brand",
            &ctx
        ),
        Err(AuthError::Denied)
    );
}
#[test]
fn physical_scope_subset_keeps_original_cancellation_and_deadline() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let scope = f.scope();
    let physical = scope.physical();
    assert_eq!(issuer.validate_physical_scope(&physical, &ctx), Ok(()));
    let expiry_cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_millis(50);
    let expired = request_before(&expiry_cancel, deadline);
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    assert_eq!(
        issuer.validate_physical_scope(&physical, &expired),
        Err(AuthError::Request)
    );
    cancel.cancel();
    assert_eq!(
        issuer.validate_physical_scope(&physical, &ctx),
        Err(AuthError::Request)
    );
}
#[test]
fn physical_scope_subset_keeps_role_global_and_project_version_rules() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let mut input = f.scope();
    input.global_version = " invalid ";
    assert_eq!(issuer.validate_scope(&input, &ctx), Ok(()));
    input.project = None;
    assert_eq!(
        issuer.validate_physical_scope(&input.physical(), &ctx),
        Ok(())
    );
    assert_eq!(issuer.validate_scope(&input, &ctx), Err(AuthError::Invalid));
    input.global_version = "";
    assert_eq!(issuer.validate_scope(&input, &ctx), Err(AuthError::Invalid));
    input.global_version = "main";
    assert_eq!(issuer.validate_scope(&input, &ctx), Ok(()));
}

mod user_records;

mod imported_ast;
mod imported_knowledge;
mod installed_hub;
