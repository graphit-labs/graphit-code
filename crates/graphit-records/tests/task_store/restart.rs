use super::*;
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    brand::Brand,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    project_paths::InstallationPaths,
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
};
use graphit_records::reference_projection::AuthoredReferencesField;

#[tokio::test]
async fn runtime_claims_survive_real_key_and_issuer_restart_with_current_authority() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let key_path = f.installation.owner_root().join("daemon/mcp.key");
    let old_secret = std::fs::read_to_string(&key_path).unwrap();
    let (task, session, task_claim, session_claim, old_actor) = {
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&old_secret, &ctx).unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let write = authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
            .unwrap();
        let coordinate = authority
            .admit_authenticated_write(
                &caller,
                &ctx,
                f.scope(),
                ProjectWriteOperation::CoordinateTask,
            )
            .unwrap();
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("restart-session"),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let task = NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, "restart-task"),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let task_claim = NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            Duration::from_secs(120),
            write_limits(),
        )
        .await
        .unwrap();
        let session_claim = NativeTaskStore::claim_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            Duration::from_secs(120),
            write_limits(),
        )
        .await
        .unwrap();
        let old_actor = write.actor().unwrap().identifier();
        (task, session, task_claim, session_claim, old_actor)
    };
    assert_eq!(Arc::strong_count(&f.runtime), 1);
    let paths = InstallationPaths::resolve(
        Brand::default(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    drop(f.runtime);
    let private = installation_bootstrap::prepare(
        &paths,
        BootstrapLimits::new(65536, 256).unwrap(),
        ctx.snapshot(),
    )
    .into_installation()
    .unwrap();
    f.runtime = Arc::new(
        RuntimeKeyOwner::start(
            private,
            RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
            ctx.snapshot(),
        )
        .into_owner()
        .ok()
        .unwrap(),
    );
    let ctx = request(&cancel);
    let secret = std::fs::read_to_string(key_path).unwrap();
    assert!(secret != old_secret);
    let issuer = f.issuer(&ctx);
    assert!(issuer.authenticate_runtime(&old_secret, &ctx).is_err());
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    let coordinate = authority
        .admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::CoordinateTask,
        )
        .unwrap();
    let new_actor = write.actor().unwrap().identifier();
    assert_ne!(old_actor, new_actor);
    let repeated = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(repeated.token == task_claim.token);
    assert_eq!(repeated.record, task_claim.record);
    let renewed = NativeTaskStore::heartbeat_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        ClaimUpdate {
            token: &task_claim.token,
            expected_revision: task_claim.record.specification.revision,
        },
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(renewed.ownership.owner, task_claim.record.ownership.owner);
    assert_eq!(
        renewed.ownership.claim_epoch,
        task_claim.record.ownership.claim_epoch
    );
    assert_eq!(
        renewed.specification.revision,
        task_claim.record.specification.revision + 1
    );
    let renewed_session = NativeTaskStore::heartbeat_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        ClaimUpdate {
            token: &session_claim.token,
            expected_revision: session_claim.record.revision,
        },
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        renewed_session.ownership.owner,
        session_claim.record.ownership.owner
    );
    assert_eq!(
        renewed_session.ownership.claim_epoch,
        session_claim.record.ownership.claim_epoch
    );
    let incorrect = ClaimToken::from_bytes([0; 32]);
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            ClaimUpdate {
                token: &incorrect,
                expected_revision: renewed.specification.revision
            },
            Duration::from_secs(120),
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            ClaimUpdate {
                token: &task_claim.token,
                expected_revision: task_claim.record.specification.revision
            },
            Duration::from_secs(120),
            write_limits()
        )
        .await,
        Err(TaskStoreError::RevisionConflict)
    ));
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let role = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let foreign = authority
        .admit_write(&role, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &foreign,
            task.specification.id,
            ClaimUpdate {
                token: &task_claim.token,
                expected_revision: renewed.specification.revision
            },
            Duration::from_secs(120),
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));

    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id: task.specification.id,
            history: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
            comments: None,
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(detail.history.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(detail.task, renewed);
    assert_eq!(detail.history.as_ref().unwrap().events.len(), 2);
    assert_eq!(
        detail.history.as_ref().unwrap().events[0].kind,
        LifecycleKind::Claimed
    );
    assert_eq!(
        detail.history.as_ref().unwrap().events[1].kind,
        LifecycleKind::Renewed
    );
    let detail = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::SessionGetInput {
            id: session.id,
            history: Some(graphit_records::task_store::SessionDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(detail.history.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(detail.session, renewed_session);
    assert_eq!(detail.history.as_ref().unwrap().events.len(), 2);
    assert_eq!(
        detail.history.as_ref().unwrap().events[0].kind,
        LifecycleKind::Claimed
    );
    assert_eq!(
        detail.history.as_ref().unwrap().events[1].kind,
        LifecycleKind::Renewed
    );

    // An actual capture in a different installation namespace cannot adopt the
    // participant even with the same physical fixture key and target scope.
    let installation = CapturedPublicationScope::installation(
        &paths,
        InstallationId::new("other-installation").unwrap(),
        [f.root.clone()],
        ScopeLimits::new(16, 4096, 128).unwrap(),
        ctx.snapshot(),
    )
    .unwrap();
    let record = installation.binding().unwrap();
    let mut other_scope = f.scope();
    other_scope.installation = graphit_auth::ScopeCapture {
        live: &installation,
        record: &record,
    };
    assert!(
        authority
            .admit_authenticated_write(&caller, &ctx, other_scope, ProjectWriteOperation::Task)
            .is_err()
    );
}
