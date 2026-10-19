//! Regression on the unmodified test harness stack and actual Task SDK authority.
#![cfg(target_os = "linux")]
#[path = "task_store/fixture.rs"]
mod fixture;
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::file_lock::Cancellation;
use graphit_records::{
    gates::TaskStatus,
    reference_projection::{AuthoredReferenceEntry, AuthoredReferencesField, ReferenceCandidate},
    task_spec::{CheckKind, TaskId},
    task_store::*,
};
use std::{future::Future, num::NonZeroU128, time::Duration};

fn future_bytes<F: Future>(_: impl FnOnce() -> F) -> usize {
    std::mem::size_of::<F>()
}
fn result<T>(result: Result<T, TaskStoreError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "stack-phase: error-returned {:?}",
                std::mem::discriminant(&error)
            );
            std::panic::panic_any(error)
        }
    }
}
fn session_input() -> CreateSession {
    CreateSession {
        idempotency_key: "stack-session".into(),
        title: "Task stack regression".into(),
        description: "Actual SDK".into(),
        strategy: "Default harness stack".into(),
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
        description: "Actual claim".into(),
        task_type: "task".into(),
        priority: 2,
        schedule: None,
        checks: vec![
            CreateCheck {
                kind: CheckKind::Acceptance,
                text: "Claim completes".into(),
            },
            CreateCheck {
                kind: CheckKind::Test,
                text: "Preserve the actual fence and audit".into(),
            },
        ],
    }
}
#[test]
fn observe_public_claim_future_without_constructing_or_polling_it() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    let id = TaskId::new(NonZeroU128::new(1).unwrap());
    eprintln!(
        "stack-observation: claim_future_bytes={}",
        future_bytes(|| NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write,
            id,
            Duration::from_secs(120),
            write_limits()
        ))
    );
}
#[tokio::test]
async fn native_claim_on_default_harness_preserves_fencing_audit_and_reference_generation() {
    eprintln!("stack-phase: entered");
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    eprintln!("stack-phase: session-create");
    let session = result(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input(),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await,
    );
    let sid = serde_json::to_value(session.id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let entries = [AuthoredReferenceEntry::Tuple(ReferenceCandidate {
        target_type: "session",
        id: &sid,
        scope: None,
        scope_id: None,
        context: None,
        relation: "supports",
        field: Some("specification"),
    })];
    eprintln!("stack-phase: task-create");
    let task = result(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, "stack-task"),
            AuthoredReferencesField::Sequence(&entries),
            write_limits(),
        )
        .await,
    );
    eprintln!(
        "stack-observation: claim_future_bytes={}",
        future_bytes(|| NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            Duration::from_secs(120),
            write_limits()
        ))
    );
    eprintln!("stack-phase: claim-call");
    let future = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    );
    eprintln!("stack-phase: claim-poll");
    let claim = future.await;
    eprintln!("stack-phase: claim-returned");
    let claim = result(claim);
    assert_eq!(claim.record.status, TaskStatus::InProgress);
    assert_eq!(claim.record.ownership.claim_epoch, 1);
    assert_eq!(
        claim.record.reference_generation,
        claim.record.specification.revision
    );
    assert_eq!(claim.record.references, task.references);
    let current = result(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            ClaimUpdate {
                token: &claim.token,
                expected_revision: claim.record.specification.revision,
            },
            Duration::from_secs(120),
            write_limits(),
        )
        .await,
    );
    assert_eq!(current.reference_generation, current.specification.revision);
    assert_eq!(current.references, task.references);
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            ClaimUpdate {
                token: &claim.token,
                expected_revision: claim.record.specification.revision
            },
            Duration::from_secs(120),
            write_limits()
        )
        .await,
        Err(TaskStoreError::RevisionConflict)
    ));
    let same = result(
        NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            Duration::from_secs(120),
            write_limits(),
        )
        .await,
    );
    assert!(same.token == claim.token);
    assert_eq!(same.record, current);
    let detail = result(
        NativeTaskStore::read_task(
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
        .await,
    )
    .unwrap();
    assert_eq!(detail.task, current);
    assert_eq!(detail.history.as_ref().unwrap().events.len(), 2);
    assert_eq!(
        detail.history.as_ref().unwrap().events[0].kind,
        LifecycleKind::Claimed
    );
    assert_eq!(
        detail.history.as_ref().unwrap().events[1].kind,
        LifecycleKind::Renewed
    );
    assert!(
        detail
            .history
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(
                |e| e.record.reference_generation == e.record.specification.revision
                    && e.record.references == task.references
            )
    );
    let next = result(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, "stack-next"),
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await,
    );
    let next_claim = result(
        NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write,
            next.specification.id,
            Duration::from_secs(120),
            write_limits(),
        )
        .await,
    );
    assert_eq!(next_claim.record.ownership.owner, current.ownership.owner);
    assert!(next_claim.token != claim.token);
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            ClaimUpdate {
                token: &next_claim.token,
                expected_revision: current.specification.revision,
            },
            Duration::from_secs(120),
            write_limits(),
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    // Another real signed run is admitted, but its actor cannot reuse the Native fence.
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let other = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    assert!(matches!(
        NativeTaskStore::claim_task(
            ctx.snapshot(),
            &other,
            task.specification.id,
            Duration::from_secs(120),
            write_limits(),
        )
        .await,
        Err(TaskStoreError::AlreadyClaimed)
    ));
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &other,
            task.specification.id,
            ClaimUpdate {
                token: &claim.token,
                expected_revision: current.specification.revision
            },
            Duration::from_secs(120),
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let (registered, _registered_run) = f.issue_registered(&issuer, &ctx, &reference);
    let verified = issuer
        .verify(
            registered.expose_for_transport(),
            &reference,
            f.scope(),
            &ctx,
        )
        .unwrap();
    let coordinate = authority
        .admit_write(&verified, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let session_claim = result(
        NativeTaskStore::claim_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            Duration::from_secs(120),
            write_limits(),
        )
        .await,
    );
    assert_eq!(session_claim.record.status, TaskStatus::InProgress);
    eprintln!("stack-phase: claim-audit-fence-complete");
}

#[tokio::test]
async fn native_empty_authored_claim_matches_runtime_seed_batch_one() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Task)
        .unwrap();
    let mut limits = write_limits();
    limits.read.batch_rows = std::num::NonZeroU32::new(1).unwrap();
    let session = result(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            CreateSession {
                idempotency_key: "first-session".into(),
                title: "first".into(),
                description: "PRIVATE_SESSION_BODY".into(),
                strategy: "actual native".into(),
            },
            AuthoredReferencesField::Omitted,
            limits,
        )
        .await,
    );
    let mut input = task_input(session.id, "first-task");
    input.title = "first".into();
    input.description = "PRIVATE_TASK_BODY".into();
    input.checks[0].text = "Publish actual fixture head".into();
    input.checks[1].text = "Observe the complete fixture head".into();
    let saved = result(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            input,
            AuthoredReferencesField::Omitted,
            limits,
        )
        .await,
    );
    assert!(saved.references.is_empty());
    eprintln!("stack-phase: runtime-empty-claim-poll");
    let returned = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        saved.specification.id,
        Duration::from_secs(120),
        limits,
    )
    .await;
    eprintln!("stack-phase: runtime-empty-claim-returned");
    let claim = result(returned);
    assert!(claim.record.references.is_empty());
    assert_eq!(
        claim.record.reference_generation,
        claim.record.specification.revision
    );
    assert_eq!(claim.record.status, TaskStatus::InProgress);
}
