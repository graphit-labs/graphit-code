use super::*;
use crate::{
    ProjectReadAdmission, ProjectReadModule, ProjectRecordAuthority, ProjectWriteAdmission,
    ProjectWriteOperation,
};
use graphit_core::module_settings::Module;

pub(super) fn issue(
    f: &Fixture,
    issuer: &RoleIssuer,
    parent: &AdminParent<'_>,
    role: Role,
    permissions: &[Permission],
    scope: &ScopeInput<'_>,
) -> (RunHandle, RoleBearer, RoleCredentialReference) {
    let definition = f.definition(role, permissions, 1, limits());
    let reference = definition.reference().clone();
    issuer.register_definition(parent, definition).unwrap();
    let run = issuer
        .start_run(parent, scope, Duration::from_secs(3600))
        .unwrap();
    let bearer = issuer
        .issue(parent, &run, &reference, permissions, scope)
        .unwrap();
    (run, bearer, reference)
}

pub(super) fn read_denied(admission: &ProjectReadAdmission<'_>) {
    assert!(admission.recheck().is_err());
    assert!(admission.project().is_err());
    assert!(admission.request().is_err());
    assert!(admission.project_and_request().is_err());
}
pub(super) fn write_denied(admission: &ProjectWriteAdmission<'_>) {
    assert!(admission.recheck().is_err());
    assert!(admission.actor().is_err());
    assert!(admission.task_actor().is_err());
    assert!(admission.project().is_err());
    assert!(admission.request().is_err());
    assert!(admission.project_and_request().is_err());
}

fn can_read(role: Role, module: ProjectReadModule) -> bool {
    match role {
        Role::Scout | Role::Coordinator => true,
        Role::Tracker => matches!(
            module,
            ProjectReadModule::Task | ProjectReadModule::Knowledge
        ),
        Role::Scribe => matches!(module, ProjectReadModule::Memory | ProjectReadModule::Task),
        Role::Dream => module == ProjectReadModule::Memory,
    }
}
fn can_write(role: Role, operation: ProjectWriteOperation) -> bool {
    match role {
        Role::Coordinator => true,
        Role::Scribe => matches!(
            operation,
            ProjectWriteOperation::Memory | ProjectWriteOperation::OwnTask
        ),
        Role::Dream => operation == ProjectWriteOperation::FencedMemory,
        Role::Scout | Role::Tracker => false,
    }
}
fn exercise_role_matrix<'a>(
    f: &'a Fixture,
    issuer: &'a RoleIssuer,
    parent: &AdminParent<'_>,
    ctx: &'a RequestContext<'a>,
) {
    let authority = ProjectRecordAuthority::new(issuer);
    let read = [
        (ProjectReadModule::Memory, Permission::Read(Module::Memory)),
        (ProjectReadModule::Task, Permission::Read(Module::Task)),
        (ProjectReadModule::Hub, Permission::Read(Module::Hub)),
        (
            ProjectReadModule::Knowledge,
            Permission::Read(Module::Knowledge),
        ),
    ];
    let write = [
        (
            ProjectWriteOperation::Memory,
            Permission::WriteMemory,
            Obligation::Unconditional,
        ),
        (
            ProjectWriteOperation::OwnTask,
            Permission::WriteOwnTask,
            Obligation::OwnActor,
        ),
        (
            ProjectWriteOperation::FencedMemory,
            Permission::WriteFencedMemory,
            Obligation::MemoryFenceRequired,
        ),
        (
            ProjectWriteOperation::Task,
            Permission::WriteTask,
            Obligation::Unconditional,
        ),
        (
            ProjectWriteOperation::CoordinateTask,
            Permission::CoordinateTask,
            Obligation::Unconditional,
        ),
        (
            ProjectWriteOperation::Knowledge,
            Permission::MutateModule(Module::Knowledge),
            Obligation::Unconditional,
        ),
    ];
    for role in [
        Role::Scout,
        Role::Tracker,
        Role::Scribe,
        Role::Dream,
        Role::Coordinator,
    ] {
        let permissions: Vec<_> = read
            .iter()
            .filter(|(module, _)| can_read(role, *module))
            .map(|(_, permission)| *permission)
            .chain(
                write
                    .iter()
                    .filter(|(operation, _, _)| can_write(role, *operation))
                    .map(|(_, permission, _)| *permission),
            )
            .collect();
        let (_, bearer, reference) = issue(f, issuer, parent, role, &permissions, &f.scope());
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), ctx)
            .unwrap();

        for (module, _) in read {
            let result = authority.admit_read(&grant, ctx, module);
            if can_read(role, module) {
                let admission = result.unwrap();
                assert_eq!(admission.module(), module);
                assert_eq!(admission.recheck(), Ok(()));
                assert!(std::ptr::eq(admission.request().unwrap(), ctx));
                let (project, original) = admission.project_and_request().unwrap();
                assert!(std::ptr::eq(original, ctx));
                assert!(std::ptr::eq(project, grant.scope.project.as_ref().unwrap()));
                assert!(std::ptr::eq(
                    admission.project().unwrap(),
                    grant.scope.project.as_ref().unwrap()
                ));
                assert!(std::ptr::eq(
                    admission.project().unwrap().version,
                    &f.version
                ));
            } else {
                assert!(matches!(result, Err(AuthError::Denied)));
            }
        }
        for (operation, _, obligation) in write {
            let result = authority.admit_write(&grant, ctx, operation);
            if can_write(role, operation) {
                let admission = result.unwrap();
                assert_eq!(admission.operation(), operation);
                assert_eq!(admission.recheck(), Ok(obligation));
                assert_eq!(
                    actor_bytes(&admission.actor().unwrap()),
                    actor_bytes(&grant.actor().unwrap())
                );
                if matches!(
                    operation,
                    ProjectWriteOperation::OwnTask
                        | ProjectWriteOperation::Task
                        | ProjectWriteOperation::CoordinateTask
                ) {
                    assert_eq!(
                        actor_bytes(&admission.task_actor().unwrap()),
                        actor_bytes(&grant.actor().unwrap())
                    );
                } else {
                    assert!(matches!(admission.task_actor(), Err(AuthError::Denied)));
                }
                assert!(std::ptr::eq(admission.request().unwrap(), ctx));
                let (project, original) = admission.project_and_request().unwrap();
                assert!(std::ptr::eq(original, ctx));
                assert!(std::ptr::eq(project, grant.scope.project.as_ref().unwrap()));
                assert!(std::ptr::eq(
                    admission.project().unwrap(),
                    grant.scope.project.as_ref().unwrap()
                ));
            } else {
                assert!(matches!(result, Err(AuthError::Denied)));
            }
        }
    }
}

#[test]
fn real_runtime_roles_and_closed_read_write_matrix() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    exercise_role_matrix(&f, &issuer, &parent, &ctx);
    assert!(!f.source_root.join("records").exists());
    assert!(!f.installation.owner_root().join("auth.json").exists());
}

#[test]
fn permission_subset_does_not_infer_other_operations_from_role() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let perms = [
        Permission::WriteTask,
        Permission::Read(Module::Memory),
        Permission::ReadUser,
    ];
    let (_, bearer, reference) = issue(&f, &issuer, &parent, Role::Coordinator, &perms, &f.scope());
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    assert_eq!(
        authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .unwrap()
            .recheck(),
        Ok(())
    );
    assert!(
        authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
            .is_ok()
    );
    assert!(matches!(
        authority.admit_write(&grant, &ctx, ProjectWriteOperation::CoordinateTask),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        authority.admit_read(&grant, &ctx, ProjectReadModule::Knowledge),
        Err(AuthError::Denied)
    ));
}

#[test]
fn different_issuer_with_equal_realm_and_same_policy_is_denied() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let foreign = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&foreign);
    assert!(matches!(
        authority.admit_read(&grant, &ctx, ProjectReadModule::Memory),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        authority.admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask),
        Err(AuthError::Denied)
    ));
}

#[test]
fn distinct_request_with_equal_deadline_and_cancellation_is_denied() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let replacement = request_before(&cancel, ctx.deadline().unwrap());
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    assert!(!ctx.snapshot().same_request(replacement.snapshot()));
    assert!(matches!(
        authority.admit_read(&grant, &replacement, ProjectReadModule::Memory),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        authority.admit_write(&grant, &replacement, ProjectWriteOperation::OwnTask),
        Err(AuthError::Denied)
    ));
}

#[test]
fn installation_grant_with_read_user_cannot_select_project_records() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let mut scope = f.scope();
    scope.project = None;
    scope.global_version = "development";
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Coordinator,
        &[
            Permission::Read(Module::Memory),
            Permission::ReadUser,
            Permission::WriteMemory,
        ],
        &scope,
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    assert_eq!(
        grant.authorize(Permission::ReadUser),
        Ok(Obligation::Unconditional)
    );
    let authority = ProjectRecordAuthority::new(&issuer);
    assert!(matches!(
        authority.admit_read(&grant, &ctx, ProjectReadModule::Memory),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        authority.admit_write(&grant, &ctx, ProjectWriteOperation::Memory),
        Err(AuthError::Denied)
    ));
}

#[test]
fn revoke_and_close_run_invalidate_every_getter() {
    for revoke in [true, false] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, limits());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let (run, bearer, reference) = issue(
            &f,
            &issuer,
            &parent,
            Role::Scribe,
            &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
            &f.scope(),
        );
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .unwrap();
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
            .unwrap();
        assert_eq!(read.module(), ProjectReadModule::Memory);
        assert_eq!(write.operation(), ProjectWriteOperation::OwnTask);
        if revoke {
            issuer.revoke(&parent, &bearer).unwrap();
        } else {
            issuer.close_run(&parent, &run).unwrap();
        }
        assert_eq!(read.module(), ProjectReadModule::Memory);
        assert_eq!(write.operation(), ProjectWriteOperation::OwnTask);
        read_denied(&read);
        write_denied(&write);
    }
}

#[test]
fn definition_generation_change_invalidates_existing_admissions() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let perms = [Permission::Read(Module::Memory), Permission::WriteOwnTask];
    let (_, bearer, reference) = issue(&f, &issuer, &parent, Role::Scribe, &perms, &f.scope());
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    issuer
        .replace_definition(
            &parent,
            &reference,
            f.definition(Role::Scribe, &perms, 2, limits()),
        )
        .unwrap();
    read_denied(&read);
    write_denied(&write);
}

#[test]
fn actual_expiry_without_request_renewal_invalidates_every_getter() {
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
    let issuer = RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        authority(&all_permissions(), &all_permissions(), limits(), 7200),
        ttl(),
        &ctx,
        clock.clone(),
        Arc::new(TestEntropy {
            fail: AtomicBool::new(false),
            counter: AtomicU64::new(0),
        }),
    )
    .unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    clock.seconds.store(900, Ordering::SeqCst);
    assert_eq!(read.recheck(), Err(AuthError::Expired));
    assert_eq!(write.recheck(), Err(AuthError::Expired));
    read_denied(&read);
    write_denied(&write);
}

#[test]
fn cancellation_invalidates_every_getter_and_new_admission() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    cancel.cancel();
    assert_eq!(read.module(), ProjectReadModule::Memory);
    assert_eq!(write.operation(), ProjectWriteOperation::OwnTask);
    assert_eq!(read.recheck(), Err(AuthError::Request));
    assert_eq!(write.recheck(), Err(AuthError::Request));
    read_denied(&read);
    write_denied(&write);
    assert!(matches!(
        authority.admit_read(&grant, &ctx, ProjectReadModule::Memory),
        Err(AuthError::Request)
    ));
}

#[test]
fn poisoned_issuer_preserves_static_errors_for_actual_admissions() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    assert_eq!(read.recheck(), Ok(()));
    assert_eq!(write.recheck(), Ok(Obligation::OwnActor));
    std::thread::scope(|scope| {
        assert!(
            scope
                .spawn(|| {
                    let _lock = issuer.state.lock().unwrap();
                    panic!("project admission poison fixture");
                })
                .join()
                .is_err()
        );
    });
    assert_eq!(read.recheck(), Err(AuthError::State));
    assert_eq!(write.recheck(), Err(AuthError::State));
    read_denied(&read);
    write_denied(&write);
}

#[test]
fn project_root_replacement_invalidates_every_getter() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    std::fs::rename(&f.source_root, f.source_root.with_extension("retained")).unwrap();
    std::fs::create_dir(&f.source_root).unwrap();
    read_denied(&read);
    write_denied(&write);
    assert!(!f.source_root.join("records").exists());
}

#[test]
fn runtime_key_removal_invalidates_every_getter() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    assert_eq!(f.runtime.remove(ctx.snapshot()).failure(), None);
    read_denied(&read);
    write_denied(&write);
}

#[test]
fn wrong_version_is_rejected_before_project_admission() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        &f.scope(),
    );
    let paths = ProjectPaths::at(
        &f.source_root,
        &InstallationPaths::resolve(
            Brand::default(),
            &f.source_root,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap(),
    )
    .unwrap();
    let other_version =
        VersionContext::select(&paths, Checkout::NonGit, "different-version", None, "").unwrap();
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().version = &other_version;
    assert!(
        issuer
            .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
            .is_err()
    );
}

#[test]
fn final_physical_recheck_catches_key_change_during_admission() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scout,
        &[Permission::Read(Module::Memory)],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let key = f.installation.owner_root().join("daemon/mcp.key");
    BEFORE_FINAL_SOURCE.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || std::fs::remove_file(key).unwrap()))
    });
    assert!(
        authority
            .admit_read(&grant, &ctx, ProjectReadModule::Memory)
            .is_err()
    );
    assert!(BEFORE_FINAL_SOURCE.with(|slot| slot.borrow().is_none()));
}

fn put_local(store: &AuthStore, name: &str, key: &str, activate: bool, ctx: &RequestContext<'_>) {
    use crate::profile::{
        AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
        ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    };
    let snapshot = store.load(ctx).unwrap();
    let publication = store
        .mutate(snapshot, ctx, |doc, ctx| {
            if doc.provider("local").is_none() {
                doc.add_provider(
                    "local".into(),
                    ProviderConfig {
                        kind: ProviderKind::Local {
                            allow_aws_credential_chain: false,
                            broker: Some(crate::profile::BrokerService {
                                endpoint: "https://broker.example".into(),
                                mcp_resource: None,
                                allow_anonymous: true,
                            }),
                        },
                        embedding: AiService::Broker,
                        rerank: AiService::Broker,
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
            }
            let previous = doc.profile(name);
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("local").unwrap().revision(),
                profile_revision: previous.map(|p| p.revision()),
            };
            let now = previous.map_or(1, |p| p.updated_at() + 1);
            doc.replace_profile_data(
                ProfileData {
                    name: name.into(),
                    provider: "local".into(),
                    username: name.into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new(SecretString::from(key))),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                },
                expected,
                now,
                ctx,
            )?;
            if activate {
                doc.activate(name, doc.revision(), ctx)?;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(
        publication.effect(),
        graphit_core::auth_document::publication::AuthPublicationEffect::Applied
    );
    assert_eq!(publication.failure(), None);
    assert_eq!(publication.cleanup_failure(), None);
}

#[test]
fn local_unrelated_edit_preserves_fresh_credential_but_not_original_admission() {
    let cancel = Cancellation::default();
    let startup = request_for_documents(&cancel);
    let f = Fixture::new(&startup);
    let store = f.auth_store(&startup);
    put_local(&store, "a", "project-source-a", true, &startup);
    put_local(&store, "b", "project-source-b", false, &startup);
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        Arc::clone(&store),
        authority(&all_permissions(), &all_permissions(), limits(), 7200),
        ttl(),
        &startup,
    )
    .unwrap();
    let ctx = request_for_documents(&cancel);
    let caller = issuer.authenticate_local("project-source-a", &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (run, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let actor = actor_bytes(&write.actor().unwrap());
    let caller_actor = actor_bytes(&caller.actor().unwrap());
    let source_generation = caller.source.generation();
    let expiries = {
        let state = issuer.lock().unwrap();
        [
            state.sessions[&session.id].expiry,
            state.runs[&run.id].expiry,
            state.credentials[&grant.id].expiry,
        ]
    };
    let mutation = request_for_documents(&cancel);
    put_local(&store, "b", "project-source-b-replaced", false, &mutation);
    read_denied(&read);
    write_denied(&write);

    // Fresh verification preserves the credential, never the old physical proof.
    let fresh = request_for_documents(&cancel);
    assert!(matches!(
        authority.admit_read(&grant, &fresh, ProjectReadModule::Memory),
        Err(AuthError::Denied)
    ));
    let fresh_caller = issuer
        .authenticate_local("project-source-a", &fresh)
        .unwrap();
    assert_eq!(actor_bytes(&fresh_caller.actor().unwrap()), caller_actor);
    assert_eq!(fresh_caller.source.generation(), source_generation);
    let fresh_parent = issuer.admin_parent(&fresh_caller, &session).unwrap();
    let fresh_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    assert!(fresh_grant.id == grant.id);
    assert_eq!(fresh_grant.source.generation(), source_generation);
    let fresh_read = authority
        .admit_read(&fresh_grant, &fresh, ProjectReadModule::Memory)
        .unwrap();
    let fresh_write = authority
        .admit_write(&fresh_grant, &fresh, ProjectWriteOperation::OwnTask)
        .unwrap();
    assert_eq!(fresh_read.recheck(), Ok(()));
    assert_eq!(fresh_write.recheck(), Ok(Obligation::OwnActor));
    assert_eq!(actor_bytes(&fresh_write.actor().unwrap()), actor);
    assert!(std::ptr::eq(fresh_read.request().unwrap(), &fresh));
    assert!(std::ptr::eq(fresh_write.request().unwrap(), &fresh));
    assert!(std::ptr::eq(
        fresh_write.project().unwrap(),
        fresh_grant.scope.project.as_ref().unwrap()
    ));
    {
        let state = issuer.lock().unwrap();
        let current = [
            state.sessions[&session.id].expiry,
            state.runs[&run.id].expiry,
            state.credentials[&fresh_grant.id].expiry,
        ];
        for (current, original) in current.into_iter().zip(expiries) {
            assert_eq!(current.utc, original.utc);
            assert_eq!(current.mono, original.mono);
        }
        assert_eq!(state.runs[&run.id].session, fresh_parent.session);
    }
    read_denied(&read);
    write_denied(&write);
    put_local(&store, "a", "project-source-a-replaced", false, &mutation);
    read_denied(&read);
    write_denied(&write);
    read_denied(&fresh_read);
    write_denied(&fresh_write);
    let after_key_change = request_for_documents(&cancel);
    assert!(
        issuer
            .verify(
                bearer.expose_for_transport(),
                &reference,
                f.scope(),
                &after_key_change
            )
            .is_err()
    );
}

#[test]
fn issuer_store_binding_replacement_invalidates_existing_admissions() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scribe,
        &[Permission::Read(Module::Memory), Permission::WriteOwnTask],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let root = f.installation.owner_root();
    std::fs::rename(root, root.with_extension("retained")).unwrap();
    std::fs::create_dir(root).unwrap();
    read_denied(&read);
    write_denied(&write);
}

#[test]
fn cancellation_during_final_source_fence_cannot_escape_as_admission() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Dream,
        &[
            Permission::Read(Module::Memory),
            Permission::WriteFencedMemory,
        ],
        &f.scope(),
    );
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let signal = cancel.clone();
    BEFORE_FINAL_SOURCE.with(|slot| *slot.borrow_mut() = Some(Box::new(move || signal.cancel())));
    assert!(matches!(
        authority.admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory),
        Err(AuthError::Request)
    ));
    assert!(BEFORE_FINAL_SOURCE.with(|slot| slot.borrow().is_none()));
}

#[test]
fn real_local_roles_and_closed_read_write_matrix() {
    let cancel = Cancellation::default();
    let startup = request_for_documents(&cancel);
    let f = Fixture::new(&startup);
    let store = f.auth_store(&startup);
    put_local(
        &store,
        "matrix",
        "project-matrix-local-source",
        true,
        &startup,
    );
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        Arc::clone(&store),
        authority(&all_permissions(), &all_permissions(), limits(), 7200),
        ttl(),
        &startup,
    )
    .unwrap();
    let ctx = request_for_documents(&cancel);
    let caller = issuer
        .authenticate_local("project-matrix-local-source", &ctx)
        .unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let document_before = std::fs::read(f.installation.owner_root().join("auth.json")).unwrap();
    exercise_role_matrix(&f, &issuer, &parent, &ctx);
    assert!(
        std::fs::read(f.installation.owner_root().join("auth.json")).unwrap() == document_before
    );
    assert!(!f.source_root.join("records").exists());
}

#[test]
fn authenticated_runtime_reads_and_writes_preserve_class_actor_and_original_scope() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    exercise_authenticated_classes(&f, &issuer, &caller, &ctx);
    assert!(!f.installation.owner_root().join("auth.json").exists());
    assert!(!f.source_root.join("records").exists());
}

fn exercise_authenticated_classes<'a>(
    f: &'a Fixture,
    issuer: &'a RoleIssuer,
    caller: &'a AuthenticatedCaller<'a>,
    ctx: &'a RequestContext<'a>,
) {
    let authority = ProjectRecordAuthority::new(issuer);
    for module in [
        ProjectReadModule::Memory,
        ProjectReadModule::Task,
        ProjectReadModule::Knowledge,
        ProjectReadModule::Ast,
        ProjectReadModule::Hub,
    ] {
        let admission = authority
            .admit_authenticated_read(caller, ctx, f.scope(), module)
            .unwrap();
        assert_eq!(admission.module(), module);
        assert_eq!(admission.recheck(), Ok(()));
        assert!(std::ptr::eq(admission.request().unwrap(), ctx));
        let (project, original) = admission.project_and_request().unwrap();
        assert!(std::ptr::eq(original, ctx));
        assert!(std::ptr::eq(project.scope.live, &f.project));
        assert!(std::ptr::eq(project.scope.record, &f.project_record));
        assert!(std::ptr::eq(project.version, &f.version));
        let project = admission.project().unwrap();
        assert!(std::ptr::eq(project.scope.live, &f.project));
        assert!(std::ptr::eq(project.scope.record, &f.project_record));
        assert!(std::ptr::eq(project.version, &f.version));
    }
    for (operation, obligation) in [
        (ProjectWriteOperation::Memory, Obligation::Unconditional),
        (ProjectWriteOperation::OwnTask, Obligation::OwnActor),
        (
            ProjectWriteOperation::FencedMemory,
            Obligation::MemoryFenceRequired,
        ),
        (ProjectWriteOperation::Task, Obligation::Unconditional),
        (
            ProjectWriteOperation::CoordinateTask,
            Obligation::Unconditional,
        ),
        (ProjectWriteOperation::Knowledge, Obligation::Unconditional),
    ] {
        let admission = authority
            .admit_authenticated_write(caller, ctx, f.scope(), operation)
            .unwrap();
        assert_eq!(admission.operation(), operation);
        assert_eq!(admission.recheck(), Ok(obligation));
        assert_eq!(
            actor_bytes(&admission.actor().unwrap()),
            actor_bytes(&caller.actor().unwrap())
        );
        assert!(std::ptr::eq(admission.request().unwrap(), ctx));
        let (project, original) = admission.project_and_request().unwrap();
        assert!(std::ptr::eq(original, ctx));
        assert!(std::ptr::eq(project.scope.live, &f.project));
        assert!(std::ptr::eq(project.scope.record, &f.project_record));
        assert!(std::ptr::eq(project.version, &f.version));
        assert!(std::ptr::eq(
            admission.project().unwrap().scope.live,
            &f.project
        ));
    }
}

#[test]
fn authenticated_local_scope_and_actor_survive_only_the_original_physical_observation() {
    let cancel = Cancellation::default();
    let startup = request_for_documents(&cancel);
    let f = Fixture::new(&startup);
    let store = f.auth_store(&startup);
    put_local(&store, "records", "local-records-key", true, &startup);
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        Arc::clone(&store),
        authority(&all_permissions(), &all_permissions(), limits(), 7200),
        ttl(),
        &startup,
    )
    .unwrap();
    let ctx = request_for_documents(&cancel);
    let caller = issuer
        .authenticate_local("local-records-key", &ctx)
        .unwrap();
    let before = std::fs::read(f.installation.owner_root().join("auth.json")).unwrap();
    exercise_authenticated_classes(&f, &issuer, &caller, &ctx);
    assert_eq!(
        std::fs::read(f.installation.owner_root().join("auth.json")).unwrap(),
        before
    );
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::OwnTask)
        .unwrap();
    let mutation = request_for_documents(&cancel);
    put_local(&store, "records", "changed-records-key", true, &mutation);
    read_denied(&read);
    write_denied(&write);
}

#[test]
fn authenticated_ordinary_policy_never_infers_other_record_permissions() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store(&ctx),
        authority(
            &[Permission::Read(Module::Task), Permission::WriteOwnTask],
            &all_permissions(),
            limits(),
            7200,
        ),
        ttl(),
        &ctx,
    )
    .unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    assert!(
        authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
            .is_ok()
    );
    assert!(matches!(
        authority.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        authority.admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Knowledge),
        Err(AuthError::Denied)
    ));
    assert_eq!(
        authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::OwnTask)
            .unwrap()
            .recheck(),
        Ok(Obligation::OwnActor)
    );
    assert!(matches!(
        authority.admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task),
        Err(AuthError::Denied)
    ));
}

#[test]
fn authenticated_foreign_issuer_request_and_scope_never_admit_records() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let equivalent = request_before(&cancel, ctx.deadline().unwrap());
    let f = Fixture::new(&ctx);
    let other = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let foreign = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let foreign_authority = ProjectRecordAuthority::new(&foreign);
    assert!(matches!(
        foreign_authority.admit_authenticated_read(
            &caller,
            &ctx,
            f.scope(),
            ProjectReadModule::Task
        ),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        foreign_authority.admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::Task
        ),
        Err(AuthError::Denied)
    ));
    let authority = ProjectRecordAuthority::new(&issuer);
    assert!(!ctx.snapshot().same_request(equivalent.snapshot()));
    assert!(matches!(
        authority.admit_authenticated_read(
            &caller,
            &equivalent,
            f.scope(),
            ProjectReadModule::Task
        ),
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        authority.admit_authenticated_write(
            &caller,
            &equivalent,
            f.scope(),
            ProjectWriteOperation::Task
        ),
        Err(AuthError::Denied)
    ));
    let mut missing = f.scope();
    missing.project = None;
    let mut installation = f.scope();
    installation.installation = other.realm();
    let mut project = f.scope();
    project.project = other.scope().project;
    let mut record = f.scope();
    record.project.as_mut().unwrap().scope.record = &other.project_record;
    let mut version = f.scope();
    version.project.as_mut().unwrap().version = &other.version;
    for scope in [missing, installation, project, record, version] {
        assert!(matches!(
            authority.admit_authenticated_read(&caller, &ctx, scope, ProjectReadModule::Task),
            Err(AuthError::Denied)
        ));
        assert!(matches!(
            authority.admit_authenticated_write(&caller, &ctx, scope, ProjectWriteOperation::Task),
            Err(AuthError::Denied)
        ));
    }
    assert!(!f.source_root.join("records").exists());
}

#[test]
fn authenticated_original_cancellation_invalidates_access_but_not_classification() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::OwnTask)
        .unwrap();
    cancel.cancel();
    assert_eq!(read.module(), ProjectReadModule::Task);
    assert_eq!(write.operation(), ProjectWriteOperation::OwnTask);
    assert_eq!(read.recheck(), Err(AuthError::Request));
    assert_eq!(write.recheck(), Err(AuthError::Request));
    read_denied(&read);
    write_denied(&write);
}

#[test]
fn authenticated_key_and_project_replacement_invalidate_original_admissions() {
    for key in [true, false] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx, limits());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
            .unwrap();
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::OwnTask)
            .unwrap();
        if key {
            std::fs::remove_file(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        } else {
            std::fs::rename(&f.source_root, f.source_root.with_extension("retained")).unwrap();
            std::fs::create_dir(&f.source_root).unwrap();
        }
        read_denied(&read);
        write_denied(&write);
    }
}

#[test]
fn authenticated_admissions_remain_valid_after_state_contention() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::OwnTask)
        .unwrap();
    let held = issuer.lock().unwrap();
    std::thread::scope(|threads| {
        let (ready, waiting) = std::sync::mpsc::channel();
        let read = &read;
        let write = &write;
        let checked = threads.spawn(move || {
            ready.send(()).unwrap();
            (read.recheck(), write.recheck())
        });
        waiting.recv().unwrap();
        drop(held);
        assert_eq!(checked.join().unwrap(), (Ok(()), Ok(Obligation::OwnActor)));
    });
}

// Capture all roots while both projects exist. Later checks must keep these receipts.
fn installation_isolation_fixture(
    ctx: &RequestContext<'_>,
) -> (
    Fixture,
    CapturedPublicationScope,
    ScopeBindingRecord,
    VersionContext,
) {
    let mut f = Fixture::new(ctx);
    let sibling = f._dir.path().join("sibling");
    std::fs::create_dir(&sibling).unwrap();
    let install = InstallationPaths::resolve(
        Brand::default(),
        &sibling,
        Some(&f._dir.path().join("home")),
        None,
    )
    .unwrap();
    std::fs::create_dir_all(install.global_path(&["runtime"]).unwrap()).unwrap();
    let paths = ProjectPaths::at(&sibling, &install).unwrap();
    let sl = f.installation.limits();
    f.installation = CapturedPublicationScope::installation(
        &install,
        InstallationId::new("local-installation").unwrap(),
        [f.source_root.clone(), sibling.clone()],
        sl,
        ctx.snapshot(),
    )
    .unwrap();
    f.installation_record = f.installation.binding().unwrap();
    let project = CapturedPublicationScope::project(
        &paths,
        "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap(),
        sl,
        ctx.snapshot(),
    )
    .unwrap();
    let version =
        VersionContext::select(&paths, Checkout::NonGit, "development", None, "").unwrap();
    let retired = std::mem::replace(&mut f.project, project);
    let retired_record = std::mem::replace(&mut f.project_record, f.project.binding().unwrap());
    let retired_version = std::mem::replace(&mut f.version, version);
    f.source_root = sibling;
    (f, retired, retired_record, retired_version)
}

#[test]
fn installation_isolation_runtime_and_local_keep_sibling_admission() {
    for local in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request_for_documents(&cancel);
        let (f, retired, retired_record, retired_version) = installation_isolation_fixture(&ctx);
        let store = f.auth_store(&ctx);
        if local {
            put_local(&store, "sibling", "sibling-local-key", true, &ctx);
        }
        let issuer = RoleIssuer::new(
            f.realm(),
            Arc::clone(&f.runtime),
            store,
            authority(&all_permissions(), &all_permissions(), limits(), 7200),
            ttl(),
            &ctx,
        )
        .unwrap();
        let secret = f.secret();
        std::fs::remove_dir_all(retired.owner_root()).unwrap();
        let caller = if local {
            issuer
                .authenticate_local("sibling-local-key", &ctx)
                .unwrap()
        } else {
            issuer.authenticate_runtime(&secret, &ctx).unwrap()
        };
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
            .unwrap();
        assert_eq!(read.recheck(), Ok(()));
        assert!(std::ptr::eq(read.project().unwrap().scope.live, &f.project));
        assert!(std::ptr::eq(read.request().unwrap(), &ctx));
        let retired_scope = ScopeInput {
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &retired,
                    record: &retired_record,
                },
                version: &retired_version,
            }),
            ..f.scope()
        };
        assert!(
            authority
                .admit_authenticated_read(&caller, &ctx, retired_scope, ProjectReadModule::Task)
                .is_err()
        );
        // An installation-owned provider still requires all of its captured roots.
        let providers = [f.realm()];
        assert!(
            authority
                .admit_authenticated_read(
                    &caller,
                    &ctx,
                    ScopeInput {
                        providers: &providers,
                        ..f.scope()
                    },
                    ProjectReadModule::Task
                )
                .is_err()
        );
        std::fs::rename(&f.source_root, f.source_root.with_extension("retained")).unwrap();
        std::fs::create_dir(&f.source_root).unwrap();
        read_denied(&read);
    }
}

#[test]
fn installation_isolation_retained_role_keeps_original_sibling_scope() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let (f, retired, _, _) = installation_isolation_fixture(&ctx);
    let issuer = f.issuer(&ctx, limits());
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let (_, bearer, reference) = issue(
        &f,
        &issuer,
        &parent,
        Role::Scout,
        &[Permission::Read(Module::Task)],
        &f.scope(),
    );
    std::fs::remove_dir_all(retired.owner_root()).unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert_eq!(read.recheck(), Ok(()));
    assert!(std::ptr::eq(read.project().unwrap().scope.live, &f.project));
    std::fs::rename(&f.source_root, f.source_root.with_extension("retained")).unwrap();
    std::fs::create_dir(&f.source_root).unwrap();
    read_denied(&read);
}

#[test]
fn installation_isolation_still_rejects_installation_and_runtime_replacement() {
    for runtime in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let (f, retired, _, _) = installation_isolation_fixture(&ctx);
        let issuer = f.issuer(&ctx, limits());
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        std::fs::remove_dir_all(retired.owner_root()).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
            .unwrap();
        let root = if runtime {
            f.installation.runtime_root()
        } else {
            f.installation.owner_root()
        };
        std::fs::rename(root, root.with_extension("retained")).unwrap();
        std::fs::create_dir(root).unwrap();
        read_denied(&read);
        assert!(
            issuer
                .validate_physical_scope(&f.scope().physical(), &ctx)
                .is_err()
        );
    }
}
