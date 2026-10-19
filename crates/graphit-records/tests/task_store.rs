#![cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "task_store/fixture.rs"]
mod fixture;
#[path = "task_store/parallel_sessions.rs"]
mod parallel_sessions;
#[path = "task_store/restart.rs"]
mod restart;
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_records::{
    task_spec::{CheckKind, TaskId},
    task_store::*,
};
use std::{
    num::{NonZeroU128, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};

fn session_input(key: &str) -> CreateSession {
    CreateSession {
        idempotency_key: key.into(),
        title: "Session".into(),
        description: "Current demand".into(),
        strategy: "Implement actual product".into(),
    }
}
fn task_input(session: SessionId, key: &str) -> CreateTask {
    CreateTask {
        commit: None,
        idempotency_key: key.into(),
        session_id: session,
        parent_id: None,
        depends_on: vec![],
        title: format!("Task {key}"),
        description: "Actual work".into(),
        task_type: "feature".into(),
        priority: 2,
        schedule: None,
        checks: vec![
            CreateCheck {
                kind: CheckKind::Acceptance,
                text: "System produces the requested result".into(),
            },
            CreateCheck {
                kind: CheckKind::Test,
                text: "Execute the affected test with expected result".into(),
            },
        ],
    }
}
macro_rules! admissions {
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident) => {
        let $issuer = $f.issuer(&$ctx);
        let (bearer, reference, _run) = $f.issue(&$issuer, &$ctx);
        let $grant = $issuer
            .verify(bearer.expose_for_transport(), &reference, $f.scope(), &$ctx)
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $read = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Task)
            .unwrap();
        let $write = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::Task)
            .unwrap();
    };
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persisted_dates_dependencies_creation_and_session_heads() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    assert_eq!(
        NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        TaskReadView::NoCorpus
    );
    assert!(!f.project.runtime_root().join("task").exists());
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("session"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let repeated = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("session"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(session, repeated);
    let mut conflict = session_input("session");
    conflict.title = "Another".into();
    assert!(matches!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            conflict,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::IdempotencyConflict)
    ));
    let mut tasks = vec![];
    for (i, schedule) in [
        "null",
        r#"{"start_date":"2028-02-29"}"#,
        r#"{"start_date":"2026-01-31","end_date":"2026-02-02","due_date":"2026-02-05"}"#,
        r#"{"due_date":"2026-12-31"}"#,
    ]
    .iter()
    .enumerate()
    {
        let mut input = task_input(session.id, &format!("t{i}"));
        input.schedule = serde_json::from_str(schedule).unwrap();
        if let Some(previous) = tasks.first() {
            input.depends_on.push(*previous);
        }
        let task = NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            input.clone(),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        assert_eq!(task.specification.schedule, input.schedule);
        assert_eq!(
            task,
            NativeTaskStore::create_task(
                ctx.snapshot(),
                &write,
                input,
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits()
            )
            .await
            .unwrap()
        );
        tasks.push(task.specification.id);
        let detail = NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput::current(task.specification.id),
            read_limits(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(detail.task, task);
        assert_eq!(detail.creation.record, task);
        assert_eq!(detail.creation.actor.len(), 64);
    }
    let TaskReadView::Published {
        generation,
        tasks: heads,
        sessions,
    } = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published view")
    };
    assert!(generation.version > 1);
    assert_eq!(heads.len(), 4);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].total_tasks, 4);
    assert_eq!(sessions[0].completed_tasks, 0);
    assert_eq!(heads.iter().filter(|t| t.ready).count(), 1);
    let first = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        CatalogueFilter::default(),
        NonZeroUsize::new(2).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(first.results.len(), 2);
    let cursor: CatalogueCursor =
        serde_json::from_str(&serde_json::to_string(first.next_cursor.as_ref().unwrap()).unwrap())
            .unwrap();
    let second = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        CatalogueFilter::default(),
        NonZeroUsize::new(2).unwrap(),
        Some(&cursor),
    )
    .await
    .unwrap();
    assert_eq!(second.results.len(), 2);
    assert!(second.next_cursor.is_none());
    assert!(
        first
            .results
            .iter()
            .all(|x| !second.results.iter().any(|y| x.id == y.id))
    );
    let extra = task_input(session.id, "new");
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        extra,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::catalogue(
            ctx.snapshot(),
            &read,
            read_limits(),
            CatalogueFilter::default(),
            NonZeroUsize::new(2).unwrap(),
            first.next_cursor.as_ref()
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let detail = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::SessionGetInput::current(session.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(detail.session, session);
    assert_eq!(detail.creation.record, session);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn named_first_publication_leaves_default_empty() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "feature");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("feature"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "feature-task"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claimed = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let mut revision = revision_input(&claimed.token, &claimed.record, "Explicit branch calendar");
    revision.schedule = graphit_records::task_spec::FieldUpdate::Set(
        serde_json::from_str(r#"{ "due_date":"2028-02-29" }"#).unwrap(),
    );
    let revised = NativeTaskStore::revise_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        revision,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let reopened = NativeTaskStore::read_task(
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
    assert!(reopened.history.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(reopened.task, revised);
    assert_eq!(reopened.history.as_ref().unwrap().events.len(), 2);
    let TaskReadView::Published {
        generation, tasks, ..
    } = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published")
    };
    assert_eq!(tasks.len(), 1);
    assert_eq!(generation.physical_branch.as_deref(), Some("feature"));
    let install = graphit_core::project_paths::InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = graphit_core::project_paths::ProjectPaths::at(&f.root, &install).unwrap();
    let version = graphit_core::version_context::VersionContext::select(
        &paths,
        graphit_core::version_context::Checkout::NonGit,
        "main",
        None,
        "",
    )
    .unwrap();
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().version = &version;
    let (other_bearer, other_reference, _other_run) = f.issue_for(&issuer, &ctx, scope);
    let other_grant = issuer
        .verify(
            other_bearer.expose_for_transport(),
            &other_reference,
            scope,
            &ctx,
        )
        .unwrap();
    let other_read = authority
        .admit_read(&other_grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let TaskReadView::Published {
        generation,
        tasks,
        sessions,
    } = NativeTaskStore::read_view(ctx.snapshot(), &other_read, read_limits())
        .await
        .unwrap()
    else {
        panic!("available default")
    };
    assert!(generation.physical_branch.is_none());
    assert!(tasks.is_empty());
    assert!(sessions.is_empty());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_shape_relations_context_and_cancellation_are_rejected() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let different = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(120),
        Cancellation::default(),
    ));
    assert!(matches!(
        NativeTaskStore::read_view(&different, &read, read_limits()).await,
        Err(TaskStoreError::DifferentRequest)
    ));
    for bad in [
        r#"{}"#,
        r#"{"start_date":"2027-02-29"}"#,
        r#"{"start_date":"2026-03-01","end_date":"2026-02-28"}"#,
        r#"{"due_date":"2026-01-01","other":1}"#,
    ] {
        assert!(serde_json::from_str::<graphit_records::task_schedule::TaskSchedule>(bad).is_err());
    }
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("s"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let before = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    let mut bad = task_input(session.id, "invalid");
    bad.priority = 5;
    assert!(matches!(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            bad,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::Specification(_))
    ));
    let mut bad = task_input(session.id, "missing");
    bad.depends_on
        .push(TaskId::new(NonZeroU128::new(1).unwrap()));
    assert!(matches!(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            bad,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::MissingTask)
    ));
    let parent = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "parent"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut cyclic = task_input(session.id, "cycle");
    cyclic.parent_id = Some(parent.specification.id);
    cyclic.depends_on.push(parent.specification.id);
    assert!(matches!(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            cyclic,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::Gate(_))
    ));
    let TaskReadView::Published { tasks, .. } = before else {
        panic!("published")
    };
    assert!(tasks.is_empty());
    let another = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("other"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut wrong = task_input(another.id, "wrong");
    wrong.parent_id = Some(parent.specification.id);
    assert!(matches!(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            wrong,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::Invalid(_))
    ));
    cancel.cancel();
    assert!(
        NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
            .await
            .is_err()
    );
    assert!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("cancelled"),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_request_deadline_denies_observation_and_creation() {
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let ctx = request_until(&cancel, deadline);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    assert!(matches!(
        NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        TaskReadView::NoCorpus
    ));
    tokio::time::sleep_until(deadline.into()).await;
    assert!(ctx.remaining().is_err());
    assert!(
        NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits())
            .await
            .is_err()
    );
    assert!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("expired-request"),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_directory_is_never_adopted_and_symlink_read_is_error() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let limits = write_limits();
    let artifacts =
        graphit_core::artifact_store::ArtifactStore::new(f.project.clone(), limits.artifacts);
    let mut writer = artifacts.writer_with_context(ctx.snapshot()).unwrap();
    writer
        .create_record_directory(
            graphit_core::artifact_store::ProjectRecordDirectory::Task,
            limits.read.native,
        )
        .unwrap();
    drop(writer);
    let task_dir = f.project.runtime_root().join("task");
    let partial = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("partial"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await;
    assert!(
        matches!(partial, Err(TaskStoreError::IncompleteCorpus)),
        "{partial:?}"
    );
    assert!(!task_dir.join("index.lance").exists());
    std::fs::remove_dir(&task_dir).unwrap();
    std::os::unix::fs::symlink(&f.root, &task_dir).unwrap();
    assert!(
        NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_same_key_has_one_atomic_current_and_creation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let input = session_input("concurrent");
    let (a, b) = tokio::join!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input.clone(),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        ),
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a, b);
    let heads = NativeTaskStore::session_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    assert_eq!(heads.len(), 1);
    assert_eq!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::SessionGetInput::current(a.id),
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .session,
        a
    );
}
#[test]
fn wire_identifiers_and_dates_are_canonical() {
    let nonzero = NonZeroU128::new(1).unwrap();
    let id = TaskId::new(nonzero);
    let encoded = serde_json::to_string(&id).unwrap();
    assert_eq!(serde_json::from_str::<TaskId>(&encoded).unwrap(), id);
    for invalid in [
        r#""00000000000000000000000000""#,
        r#""tsk-old""#,
        r#""01ksh1crffg8z74b5zs78ww808""#,
    ] {
        assert!(serde_json::from_str::<TaskId>(invalid).is_err());
    }
    let input = task_input(SessionId::new(nonzero), "dates");
    let mut wire = serde_json::to_value(&input).unwrap();
    wire.as_object_mut().unwrap().remove("schedule");
    assert!(
        serde_json::from_value::<CreateTask>(wire.clone())
            .unwrap()
            .schedule
            .is_none()
    );
    wire["schedule"] = serde_json::Value::Null;
    assert!(
        serde_json::from_value::<CreateTask>(wire.clone())
            .unwrap()
            .schedule
            .is_none()
    );
    wire["schedule"] = serde_json::json!({"due_date":"2028-02-29"});
    assert!(
        serde_json::from_value::<CreateTask>(wire.clone())
            .unwrap()
            .schedule
            .is_some()
    );
    wire["schedule"] = serde_json::json!({});
    assert!(serde_json::from_value::<CreateTask>(wire).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ownership_progress_history_and_heads_use_real_private_fences() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let own = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let coordinate = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("owned"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "owned-task"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let other = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "second"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &own,
            task_input(session.id, "unauthorized-create"),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::WrongAdmission)
    ));
    let claimed = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        Duration::from_secs(30),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(claimed.record.ownership.claim_epoch, 1);
    assert_eq!(
        claimed.record.status,
        graphit_records::gates::TaskStatus::InProgress
    );
    let repeated = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        Duration::from_secs(1),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(claimed.record, repeated.record);
    assert!(claimed.token == repeated.token);
    let other_claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        other.specification.id,
        Duration::from_secs(30),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        other_claim.record.ownership.owner,
        claimed.record.ownership.owner
    );
    assert!(other_claim.token != claimed.token);
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &own,
            task.specification.id,
            ClaimUpdate {
                token: &other_claim.token,
                expected_revision: claimed.record.specification.revision,
            },
            Duration::from_secs(30),
            write_limits(),
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let coordinated = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        Duration::from_secs(30),
        write_limits(),
    )
    .await
    .unwrap();
    let renewed = NativeTaskStore::heartbeat_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        ClaimUpdate {
            token: &claimed.token,
            expected_revision: claimed.record.specification.revision,
        },
        Duration::from_secs(1),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(renewed.ownership.lease_expires_at >= claimed.record.ownership.lease_expires_at);
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &own,
            task.specification.id,
            ClaimUpdate {
                token: &claimed.token,
                expected_revision: claimed.record.specification.revision
            },
            Duration::from_secs(1),
            write_limits()
        )
        .await,
        Err(TaskStoreError::RevisionConflict)
    ));
    let incorrect = ClaimToken::from_bytes([0; 32]);
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &own,
            task.specification.id,
            ClaimUpdate {
                token: &incorrect,
                expected_revision: renewed.specification.revision
            },
            Duration::from_secs(1),
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let progress = NativeTaskStore::progress_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        ProgressUpdate {
            claim: ClaimUpdate {
                token: &claimed.token,
                expected_revision: renewed.specification.revision,
            },
            lease: Duration::from_secs(30),
            summary: "private progress content".into(),
            next_step: "private continuation".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let checkpoint = NativeTaskStore::checkpoint_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        ProgressUpdate {
            claim: ClaimUpdate {
                token: &coordinated.token,
                expected_revision: coordinated.record.revision,
            },
            lease: Duration::from_secs(30),
            summary: "private checkpoint".into(),
            next_step: "private strategy".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let TaskHeadObservation::Published {
        generation,
        tasks,
        sessions,
    } = NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published")
    };
    let tasks = tasks.unwrap();
    let sessions = sessions.unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(sessions[0].total_tasks, 2);
    let head = tasks
        .iter()
        .find(|t| t.id == task.specification.id)
        .unwrap();
    assert_eq!(head.session_id, session.id);
    assert_eq!(head.ownership, progress.ownership);
    assert_eq!(head.progress.as_ref().unwrap().sequence, 1);
    assert_eq!(sessions[0].ownership, checkpoint.ownership);
    assert_eq!(
        sessions[0].progress.as_ref().unwrap().kind,
        ProgressKind::Checkpoint
    );
    let serialized = serde_json::to_string(&tasks).unwrap();
    assert!(!serialized.contains("private progress"));
    assert!(!serialized.contains("summary"));
    assert!(!serialized.contains("next_step"));
    assert!(!serialized.contains("fence"));
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
    assert_eq!(detail.task, progress);
    assert_eq!(detail.history.as_ref().unwrap().events.len(), 3);
    assert!(!serde_json::to_string(&detail).unwrap().contains("fence"));
    let TaskHeadObservation::Published {
        generation: after, ..
    } = NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published")
    };
    assert_eq!(generation, after);
    let released = NativeTaskStore::release_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        ReleaseUpdate {
            claim: ClaimUpdate {
                token: &claimed.token,
                expected_revision: progress.specification.revision,
            },
            summary: "handoff".into(),
            next_step: "continue".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(released.status, graphit_records::gates::TaskStatus::Open);
    assert!(released.ownership.owner.is_none());
    assert_eq!(released.ownership.claim_epoch, 1);
    assert_eq!(released.progress.as_ref().unwrap().observation.sequence, 2);
    let reclaimed = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        Duration::from_secs(30),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(reclaimed.record.ownership.claim_epoch, 2);
    assert!(reclaimed.token != claimed.token);
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            &own,
            task.specification.id,
            ClaimUpdate {
                token: &claimed.token,
                expected_revision: reclaimed.record.specification.revision
            },
            Duration::from_secs(1),
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let released_session = NativeTaskStore::release_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        ReleaseUpdate {
            claim: ClaimUpdate {
                token: &coordinated.token,
                expected_revision: checkpoint.revision,
            },
            summary: "session handoff".into(),
            next_step: "continue session".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(released_session.ownership.owner.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn competing_actor_and_expired_lease_reclaim_rotate_epoch() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let (bearer2, _run2) = f.issue_registered(&issuer, &ctx, &reference);
    let grant2 = issuer
        .verify(bearer2.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let write2 = authority
        .admit_write(&grant2, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("expire"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let coordinate = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let coordinate2 = authority
        .admit_write(&grant2, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    assert!(matches!(
        NativeTaskStore::claim_session(
            ctx.snapshot(),
            &write,
            session.id,
            Duration::from_secs(30),
            write_limits()
        )
        .await,
        Err(TaskStoreError::WrongAdmission)
    ));
    let (left_session, right_session) = tokio::join!(
        NativeTaskStore::claim_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            Duration::from_secs(30),
            write_limits()
        ),
        NativeTaskStore::claim_session(
            ctx.snapshot(),
            &coordinate2,
            session.id,
            Duration::from_secs(30),
            write_limits()
        )
    );
    let (session_claim, session_owner, session_foreign) = match (left_session, right_session) {
        (Ok(c), Err(TaskStoreError::AlreadyClaimed)) => (c, &coordinate, &coordinate2),
        (Err(TaskStoreError::AlreadyClaimed), Ok(c)) => (c, &coordinate2, &coordinate),
        _ => panic!("one session coordinator wins"),
    };
    assert!(matches!(
        NativeTaskStore::heartbeat_session(
            ctx.snapshot(),
            session_foreign,
            session.id,
            ClaimUpdate {
                token: &session_claim.token,
                expected_revision: session_claim.record.revision
            },
            Duration::from_secs(30),
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let renewed_session = NativeTaskStore::heartbeat_session(
        ctx.snapshot(),
        session_owner,
        session.id,
        ClaimUpdate {
            token: &session_claim.token,
            expected_revision: session_claim.record.revision,
        },
        Duration::from_secs(30),
        write_limits(),
    )
    .await
    .unwrap();
    NativeTaskStore::release_session(
        ctx.snapshot(),
        session_owner,
        session.id,
        ReleaseUpdate {
            claim: ClaimUpdate {
                token: &session_claim.token,
                expected_revision: renewed_session.revision,
            },
            summary: "release coordinator".into(),
            next_step: "task ownership".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let session_detail = NativeTaskStore::read_session(
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
    assert!(
        session_detail
            .history
            .as_ref()
            .unwrap()
            .next_cursor
            .is_none()
    );
    assert_eq!(session_detail.history.as_ref().unwrap().events.len(), 3);
    assert!(
        !serde_json::to_string(&session_detail)
            .unwrap()
            .contains("fence")
    );
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "expire-task"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let (left, right) = tokio::join!(
        NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write,
            task.specification.id,
            Duration::from_secs(30),
            write_limits()
        ),
        NativeTaskStore::claim_task(
            ctx.snapshot(),
            &write2,
            task.specification.id,
            Duration::from_secs(30),
            write_limits()
        )
    );
    let (claimed, owner, foreign) = match (left, right) {
        (Ok(c), Err(TaskStoreError::AlreadyClaimed)) => (c, &write, &write2),
        (Err(TaskStoreError::AlreadyClaimed), Ok(c)) => (c, &write2, &write),
        _ => panic!("one owner must win"),
    };
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            foreign,
            task.specification.id,
            ClaimUpdate {
                token: &claimed.token,
                expected_revision: claimed.record.specification.revision
            },
            Duration::from_secs(1),
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let released = NativeTaskStore::release_task(
        ctx.snapshot(),
        owner,
        task.specification.id,
        ReleaseUpdate {
            claim: ClaimUpdate {
                token: &claimed.token,
                expected_revision: claimed.record.specification.revision,
            },
            summary: "release".into(),
            next_step: "expiry case".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let short = NativeTaskStore::claim_task(
        ctx.snapshot(),
        owner,
        task.specification.id,
        Duration::from_millis(1),
        write_limits(),
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(matches!(
        NativeTaskStore::heartbeat_task(
            ctx.snapshot(),
            owner,
            task.specification.id,
            ClaimUpdate {
                token: &short.token,
                expected_revision: short.record.specification.revision
            },
            Duration::from_secs(1),
            write_limits()
        )
        .await,
        Err(TaskStoreError::LeaseExpired)
    ));
    let next = NativeTaskStore::claim_task(
        ctx.snapshot(),
        foreign,
        task.specification.id,
        Duration::from_secs(30),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        next.record.ownership.claim_epoch,
        released.ownership.claim_epoch + 2
    );
    assert!(next.token != short.token);
    let observed = NativeTaskStore::read_task(
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
    assert!(observed.history.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(observed.task, next.record);
    assert!(
        observed
            .history
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|e| !serde_json::to_string(e).unwrap().contains("fence"))
    );
}

/// Inject an actual persisted header defect using the SDK's loaded schema.
/// This does not duplicate the product codec or expose a test-only service API.
async fn replace_current_text(f: &Fixture, key: &str, changes: &[(&str, &str)]) {
    use arrow_array::{RecordBatch, StringArray};
    use arrow_schema::Schema;
    use futures::TryStreamExt;
    use graphit_data::lance_branch_store::LanceBranchStore;
    let uri = url::Url::from_file_path(f.project.runtime_root().join("task/index.lance"))
        .unwrap()
        .to_string();
    let dataset = lance::dataset::builder::DatasetBuilder::from_uri(&uri)
        .load()
        .await
        .unwrap();
    let schema = Arc::new(Schema::from(dataset.schema()));
    let store = LanceBranchStore::open(&uri, schema, None).await.unwrap();
    let pin = store.capture_target(None).await.unwrap();
    let mut scan = pin.scan();
    scan.filter(&format!("row_key = '{key}'")).unwrap();
    let mut stream = scan.try_into_stream().await.unwrap();
    let batch = stream.try_next().await.unwrap().unwrap();
    assert_eq!(batch.num_rows(), 1);
    assert!(stream.try_next().await.unwrap().is_none());
    let schema = batch.schema();
    let mut columns = batch.columns().to_vec();
    for (name, value) in changes {
        let index = schema.index_of(name).unwrap();
        columns[index] = Arc::new(StringArray::from(vec![*value]));
    }
    let patch = RecordBatch::try_new(schema, columns).unwrap();
    pin.prepare_merge(vec![patch], vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn activity_section_errors_keep_valid_tasks_and_global_identity_fails() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    assert!(matches!(
        NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        TaskHeadObservation::NoCorpus
    ));
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("sections"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "section-task"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let other = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "section-other"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task_id = serde_json::to_value(task.specification.id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let session_id = serde_json::to_value(session.id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let other_id = serde_json::to_value(other.specification.id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let task_key = format!("task/{task_id}");
    let session_key = format!("session/{session_id}");
    let other_key = format!("task/{other_id}");
    replace_current_text(&f, &session_key, &[("title", "")]).await;
    let TaskHeadObservation::Published {
        tasks, sessions, ..
    } = NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published")
    };
    assert_eq!(tasks.unwrap().len(), 2);
    assert!(matches!(sessions, Err(ref e) if matches!(e.as_ref(),TaskStoreError::Invalid(_))));
    assert!(
        NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
            .await
            .is_err()
    );
    replace_current_text(&f, &session_key, &[("title", "Session")]).await;
    replace_current_text(&f, &task_key, &[("status", "invalid_status")]).await;
    let TaskHeadObservation::Published {
        tasks, sessions, ..
    } = NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published")
    };
    assert!(tasks.is_err());
    assert!(
        matches!(sessions, Err(ref e) if matches!(e.as_ref(),TaskStoreError::CountsUnavailable))
    );
    replace_current_text(&f, &task_key, &[("status", "cancelled")]).await;
    replace_current_text(&f, &other_key, &[("status", "completed")]).await;
    let TaskHeadObservation::Published {
        tasks, sessions, ..
    } = NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published")
    };
    let tasks = tasks.unwrap();
    assert_eq!(tasks.len(), 2);
    assert!(
        tasks
            .iter()
            .any(|t| t.status == graphit_records::gates::TaskStatus::Cancelled)
    );
    let sessions = sessions.unwrap();
    assert_eq!(sessions[0].total_tasks, 1);
    assert_eq!(sessions[0].completed_tasks, 1);
    let missing = ulid::Ulid::generate().to_string();
    replace_current_text(&f, &task_key, &[("session_id", &missing)]).await;
    assert!(matches!(
        NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits()).await,
        Err(TaskStoreError::MissingSession)
    ));
    replace_current_text(&f, &task_key, &[("session_id", &session_id)]).await;
    replace_current_text(
        &f,
        &session_key,
        &[("record_id", &task_id), ("session_id", &task_id)],
    )
    .await;
    assert!(matches!(
        NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits()).await,
        Err(TaskStoreError::Invalid(_))
    ));
}

fn owner_input<'a>(token: &'a ClaimToken, task: &Task) -> ClaimUpdate<'a> {
    ClaimUpdate {
        token,
        expected_revision: task.specification.revision,
    }
}
fn revision_input<'a>(token: &'a ClaimToken, task: &Task, reason: &'a str) -> RevisionUpdate<'a> {
    RevisionUpdate {
        claim: owner_input(token, task),
        lease: Duration::from_secs(60),
        reason,
        title: graphit_records::task_spec::FieldUpdate::Keep,
        description: graphit_records::task_spec::FieldUpdate::Keep,
        task_type: graphit_records::task_spec::FieldUpdate::Keep,
        priority: None,
        schedule: graphit_records::task_spec::FieldUpdate::Keep,
        add_checks: &[],
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn specification_calendar_checks_and_history_are_one_native_revision() {
    use graphit_records::task_spec::{CheckStatus, FieldUpdate};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("domain"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let initial = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "domain"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        initial.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let token = claim.token;
    let mut task = claim.record;
    let id = task.specification.id;
    let check = task.specification.checks[0].id;
    task = NativeTaskStore::verify_task_check(
        ctx.snapshot(),
        &write,
        id,
        CheckUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(60),
            check_id: check,
            passed: true,
            evidence: "Actual SDK result passes",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let passed = task.specification.checks[0].verification.clone();
    let mut update = revision_input(&token, &task, "Set explicit calendar interval");
    update.schedule = FieldUpdate::Set(
        serde_json::from_str(r#"{ "start_date":"2028-02-29", "end_date":"2028-03-01" }"#).unwrap(),
    );
    task = NativeTaskStore::revise_task(
        ctx.snapshot(),
        &write,
        id,
        update,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(task.specification.revision, 4);
    assert_eq!(task.specification.checks[0].verification, passed);
    let snapshot = task.clone();
    assert!(
        NativeTaskStore::revise_task(
            ctx.snapshot(),
            &write,
            id,
            revision_input(&token, &task, "No effective change"),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    let stale = ClaimUpdate {
        token: &token,
        expected_revision: 2,
    };
    assert!(matches!(
        NativeTaskStore::verify_task_check(
            ctx.snapshot(),
            &write,
            id,
            CheckUpdate {
                claim: stale,
                lease: Duration::from_secs(60),
                check_id: check,
                passed: false,
                evidence: "Stale"
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::RevisionConflict)
    ));
    assert_eq!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput::current(id),
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .task,
        snapshot
    );
    let mut update = revision_input(&token, &task, "Effective description requires revalidation");
    update.description = FieldUpdate::Set("Changed functional contract");
    task = NativeTaskStore::revise_task(
        ctx.snapshot(),
        &write,
        id,
        update,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(
        task.specification
            .checks
            .iter()
            .all(|c| c.status == CheckStatus::Pending && c.verification.is_none())
    );
    task = NativeTaskStore::verify_task_check(
        ctx.snapshot(),
        &write,
        id,
        CheckUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(60),
            check_id: check,
            passed: false,
            evidence: "Known failure observed",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    task = NativeTaskStore::supersede_task_check(
        ctx.snapshot(),
        &write,
        id,
        SupersessionUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(60),
            check_id: check,
            reason: "Replace obsolete acceptance criterion",
            replacement: Some(ReplacementInput {
                text: "Current acceptance criterion",
                kind: None,
            }),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let old = &task.specification.checks[0];
    assert_eq!(old.status, CheckStatus::Superseded);
    assert_eq!(
        old.verification.as_ref().unwrap().evidence,
        "Known failure observed"
    );
    let replacement = old.supersession.as_ref().unwrap().replacement.unwrap();
    assert_ne!(replacement, check);
    assert!(
        task.specification
            .checks
            .iter()
            .any(|c| c.id == replacement && c.kind == CheckKind::Acceptance)
    );
    let additions = [CreateCheck {
        kind: CheckKind::Test,
        text: "New explicit regression check".into(),
    }];
    let mut update = revision_input(&token, &task, "Clear calendar");
    update.add_checks = &additions;
    update.schedule = FieldUpdate::Set(None);
    task = NativeTaskStore::revise_task(
        ctx.snapshot(),
        &write,
        id,
        update,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(task.specification.schedule.is_none());
    assert_eq!(
        task.specification.checks.last().unwrap().text,
        "New explicit regression check"
    );
    assert_eq!(
        task.specification.checks.last().unwrap().status,
        CheckStatus::Pending
    );
    assert_eq!(task.specification.checks[0].status, CheckStatus::Superseded);
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id,
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
    assert_eq!(detail.task, task);
    assert_eq!(
        detail.history.as_ref().unwrap().events.len() as u64,
        task.specification.revision - 1
    );
    assert!(matches!(
        detail.history.as_ref().unwrap().events[2].kind,
        LifecycleKind::Revised { .. }
    ));
    let encoded = serde_json::to_string(&detail).unwrap();
    assert!(!encoded.contains("fence"));
    assert!(!encoded.contains("token"));
    assert!(encoded.contains("Clear calendar"));
}

async fn pass_task_checks(
    original: &Arc<SnapshotContext>,
    admission: &graphit_auth::ProjectWriteAdmission<'_>,
    token: &ClaimToken,
    mut task: Task,
) -> Task {
    let checks: Vec<_> = task
        .specification
        .checks
        .iter()
        .filter(|c| c.status != graphit_records::task_spec::CheckStatus::Superseded)
        .map(|c| c.id)
        .collect();
    for check in checks {
        task = NativeTaskStore::verify_task_check(
            original,
            admission,
            task.specification.id,
            CheckUpdate {
                claim: owner_input(token, &task),
                lease: Duration::from_secs(60),
                check_id: check,
                passed: true,
                evidence: "Affected test and behavior pass",
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    task
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_links_flags_cancellation_and_session_closure_use_actual_graph() {
    use graphit_records::{gates::TaskStatus, task_spec::FieldUpdate};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let own = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let coordinate = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("closure"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let initial = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "parent"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let other_session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("prerequisite-session"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let other = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(other_session.id, "other"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let pending = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "unfinished"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        initial.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let token = claim.token;
    let id = initial.specification.id;
    let mut task = pass_task_checks(ctx.snapshot(), &own, &token, claim.record).await;
    task = NativeTaskStore::set_task_flag(
        ctx.snapshot(),
        &own,
        id,
        TaskFlagUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(60),
            flagged: true,
            reason: "Confirmed outstanding blocker",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(
        NativeTaskStore::complete_task(
            ctx.snapshot(),
            &own,
            id,
            owner_input(&token, &task),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeTaskStore::set_task_flag(
            ctx.snapshot(),
            &own,
            id,
            TaskFlagUpdate {
                claim: owner_input(&token, &task),
                lease: Duration::from_secs(60),
                flagged: true,
                reason: "No change",
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    task = NativeTaskStore::set_task_flag(
        ctx.snapshot(),
        &own,
        id,
        TaskFlagUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(60),
            flagged: false,
            reason: "Blocker resolved",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let baseline = task.clone();
    let cycle = [id];
    assert!(
        NativeTaskStore::update_task_links(
            ctx.snapshot(),
            &own,
            id,
            TaskLinksUpdate {
                claim: owner_input(&token, &task),
                lease: Duration::from_secs(60),
                parent_id: FieldUpdate::Keep,
                depends_on: Some(&cycle),
                reason: "Self dependency rejected",
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    let deps = [other.specification.id];
    task = NativeTaskStore::update_task_links(
        ctx.snapshot(),
        &own,
        id,
        TaskLinksUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(60),
            parent_id: FieldUpdate::Keep,
            depends_on: Some(&deps),
            reason: "Real prerequisite",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        task.specification.revision,
        baseline.specification.revision + 1
    );
    assert!(
        NativeTaskStore::complete_task(
            ctx.snapshot(),
            &own,
            id,
            owner_input(&token, &task),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    task = NativeTaskStore::update_task_links(
        ctx.snapshot(),
        &own,
        id,
        TaskLinksUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(60),
            parent_id: FieldUpdate::Keep,
            depends_on: Some(&[]),
            reason: "Remove erroneous prerequisite",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let completed = NativeTaskStore::complete_task(
        ctx.snapshot(),
        &own,
        id,
        owner_input(&token, &task),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    assert_eq!(completed.ownership.owner, None);
    assert_eq!(completed.ownership.claim_epoch, 1);
    assert!(
        NativeTaskStore::cancel_task(
            ctx.snapshot(),
            &write,
            id,
            TaskCancellation {
                claim: None,
                reason: "Terminal"
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(matches!(
        NativeTaskStore::cancel_task(
            ctx.snapshot(),
            &own,
            other.specification.id,
            TaskCancellation {
                claim: None,
                reason: "Not owned"
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::WrongAdmission)
    ));
    let coordinated = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let session_claim = || ClaimUpdate {
        token: &coordinated.token,
        expected_revision: coordinated.record.revision,
    };
    assert!(
        NativeTaskStore::complete_session(
            ctx.snapshot(),
            &write,
            session.id,
            session_claim(),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeTaskStore::complete_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            session_claim(),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    let cancelled = NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        other.specification.id,
        TaskCancellation {
            claim: None,
            reason: "No longer requested",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        pending.specification.id,
        TaskCancellation {
            claim: None,
            reason: "Remaining work removed from demand",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let closed = NativeTaskStore::complete_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        session_claim(),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(closed.status, TaskStatus::Completed);
    assert_eq!(closed.ownership.owner, None);
    let empty = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("empty-closure"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let empty_claim = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        empty.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let empty_closed = NativeTaskStore::complete_session(
        ctx.snapshot(),
        &coordinate,
        empty.id,
        ClaimUpdate {
            token: &empty_claim.token,
            expected_revision: empty_claim.record.revision,
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(empty_closed.status, TaskStatus::Completed);
    let absent = SessionId::new(NonZeroU128::new(123).unwrap());
    assert!(matches!(
        NativeTaskStore::complete_session(
            ctx.snapshot(),
            &coordinate,
            absent,
            ClaimUpdate {
                token: &empty_claim.token,
                expected_revision: 1
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::MissingSession)
    ));
    assert!(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, "late"),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    let task_detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        task_detail.last_event.as_ref().unwrap().kind,
        LifecycleKind::Completed
    );
    let other_detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(other.specification.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        other_detail.last_event.as_ref().unwrap().kind,
        LifecycleKind::Cancelled {
            reason: "No longer requested".into()
        }
    );
    let session_detail = NativeTaskStore::read_session(
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
    assert!(
        session_detail
            .history
            .as_ref()
            .unwrap()
            .next_cursor
            .is_none()
    );
    assert_eq!(session_detail.history.as_ref().unwrap().events.len(), 2);
    assert!(
        !serde_json::to_string(&session_detail)
            .unwrap()
            .contains("token")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn specification_mutations_reject_wrong_fences_expiry_and_cancelled_original() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let (foreign_bearer, _foreign_run) = f.issue_registered(&issuer, &ctx, &reference);
    let foreign_grant = issuer
        .verify(
            foreign_bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &ctx,
        )
        .unwrap();
    let foreign = authority
        .admit_write(&foreign_grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("rejected"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "rejected"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let id = task.specification.id;
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        id,
        Duration::from_secs(3),
        write_limits(),
    )
    .await
    .unwrap();
    let task = claim.record;
    let wrong = ClaimToken::from_bytes([0; 32]);
    let update = |token| CheckUpdate {
        claim: owner_input(token, &task),
        lease: Duration::from_secs(60),
        check_id: task.specification.checks[0].id,
        passed: true,
        evidence: "Observed",
    };
    assert!(matches!(
        NativeTaskStore::verify_task_check(
            ctx.snapshot(),
            &write,
            id,
            update(&wrong),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    assert!(matches!(
        NativeTaskStore::verify_task_check(
            ctx.snapshot(),
            &foreign,
            id,
            update(&claim.token),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let view = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    tokio::time::sleep(Duration::from_millis(
        task.ownership.lease_expires_at.unwrap().saturating_sub(now) + 1,
    ))
    .await;
    assert!(matches!(
        NativeTaskStore::verify_task_check(
            ctx.snapshot(),
            &write,
            id,
            update(&claim.token),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::LeaseExpired)
    ));
    assert_eq!(
        NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        view
    );
    cancel.cancel();
    assert!(
        NativeTaskStore::cancel_task(
            ctx.snapshot(),
            &write,
            id,
            TaskCancellation {
                claim: Some(owner_input(&claim.token, &task)),
                reason: "Cancelled original"
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_reparent_cycles_terminal_parent_and_owned_cancellation_are_enforced() {
    use graphit_records::{gates::TaskStatus, task_spec::FieldUpdate};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, _read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("relations"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let first = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "first"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut second_input = task_input(session.id, "second");
    second_input.parent_id = Some(first.specification.id);
    let second = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        second_input,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let first_claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        first.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let first = pass_task_checks(
        ctx.snapshot(),
        &write,
        &first_claim.token,
        first_claim.record,
    )
    .await;
    assert!(
        NativeTaskStore::complete_task(
            ctx.snapshot(),
            &write,
            first.specification.id,
            owner_input(&first_claim.token, &first),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeTaskStore::update_task_links(
            ctx.snapshot(),
            &write,
            first.specification.id,
            TaskLinksUpdate {
                claim: owner_input(&first_claim.token, &first),
                lease: Duration::from_secs(60),
                parent_id: FieldUpdate::Set(Some(second.specification.id)),
                depends_on: None,
                reason: "Parent cycle"
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    NativeTaskStore::release_task(
        ctx.snapshot(),
        &write,
        first.specification.id,
        ReleaseUpdate {
            claim: owner_input(&first_claim.token, &first),
            summary: "Release parent".into(),
            next_step: "Resolve child".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        second.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let detached = NativeTaskStore::update_task_links(
        ctx.snapshot(),
        &write,
        second.specification.id,
        TaskLinksUpdate {
            claim: owner_input(&claim.token, &claim.record),
            lease: Duration::from_secs(60),
            parent_id: FieldUpdate::Set(None),
            depends_on: None,
            reason: "Independent work",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(detached.parent_id, None);
    assert!(matches!(
        NativeTaskStore::cancel_task(
            ctx.snapshot(),
            &write,
            second.specification.id,
            TaskCancellation {
                claim: None,
                reason: "Missing claim"
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let cancelled = NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        second.specification.id,
        TaskCancellation {
            claim: Some(owner_input(&claim.token, &detached)),
            reason: "Child no longer needed",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    let mut invalid = task_input(session.id, "terminal-parent");
    invalid.parent_id = Some(second.specification.id);
    assert!(
        NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            invalid,
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    let first_claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        first.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let completed = NativeTaskStore::complete_task(
        ctx.snapshot(),
        &write,
        first.specification.id,
        owner_input(&first_claim.token, &first_claim.record),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
}

#[path = "task_store/comments.rs"]
mod comments;

#[path = "task_store/catalogue_filters.rs"]
mod catalogue_filters;

#[path = "task_store/lexical_index.rs"]
mod lexical_index;

#[path = "task_store/lexical_search.rs"]
mod lexical_search;

#[path = "task_store/references.rs"]
mod references;

#[path = "task_store/reference_backlinks.rs"]
mod reference_backlinks;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_specification_revision_preserves_creation_progress_and_rejects_invalid_writers() {
    use graphit_records::{
        gates::TaskStatus, reference_projection::AuthoredReferencesField, task_spec::FieldUpdate,
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let coordinate = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let initial = session_input("revision-session");
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        initial.clone(),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let child = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "retained-child"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claimed = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        Duration::from_secs(300),
        write_limits(),
    )
    .await
    .unwrap();
    let checkpoint = NativeTaskStore::checkpoint_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        ProgressUpdate {
            claim: ClaimUpdate {
                token: &claimed.token,
                expected_revision: claimed.record.revision,
            },
            lease: Duration::from_secs(300),
            summary: "Confirmed progress".into(),
            next_step: "Continue current work".into(),
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let update = |revision, title| SessionRevisionUpdate {
        claim: ClaimUpdate {
            token: &claimed.token,
            expected_revision: revision,
        },
        lease: Duration::from_secs(300),
        reason: "Refine the accepted demand",
        title,
        description: FieldUpdate::Keep,
        strategy: FieldUpdate::Keep,
    };
    let revised = NativeTaskStore::revise_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        update(checkpoint.revision, FieldUpdate::Set("Revised session")),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(revised.revision, checkpoint.revision + 1);
    assert_eq!(revised.input.title, "Revised session");
    assert_eq!(revised.input.description, initial.description);
    assert_eq!(revised.input.strategy, initial.strategy);
    assert_eq!(revised.input.idempotency_key, initial.idempotency_key);
    assert_eq!(revised.progress, checkpoint.progress);
    assert_eq!(revised.ownership.owner, checkpoint.ownership.owner);
    assert_eq!(
        revised.ownership.claim_epoch,
        checkpoint.ownership.claim_epoch
    );
    assert_eq!(revised.created_at, session.created_at);
    assert_eq!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            initial.clone(),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .unwrap(),
        revised
    );
    assert!(matches!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            revised.input.clone(),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::IdempotencyConflict)
    ));
    let before = NativeTaskStore::read_session(
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
    assert!(before.history.as_ref().unwrap().next_cursor.is_none());
    let before_view = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    assert_eq!(before.creation.record, session);
    assert_eq!(before.history.as_ref().unwrap().events.len(), 3);
    assert!(
        matches!(&before.history.as_ref().unwrap().events.last().unwrap().kind,LifecycleKind::Revised{reason} if reason=="Refine the accepted demand")
    );
    assert!(matches!(
        NativeTaskStore::revise_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            update(checkpoint.revision, FieldUpdate::Set("Stale")),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::RevisionConflict)
    ));
    for title in [
        FieldUpdate::Keep,
        FieldUpdate::Set("Revised session"),
        FieldUpdate::Set(""),
        FieldUpdate::Set(" padded "),
    ] {
        assert!(matches!(
            NativeTaskStore::revise_session(
                ctx.snapshot(),
                &coordinate,
                session.id,
                update(revised.revision, title),
                AuthoredReferencesField::Omitted,
                write_limits()
            )
            .await,
            Err(TaskStoreError::Invalid(_))
        ));
    }
    let wrong = ClaimToken::from_bytes([7; 32]);
    let mut wrong_update = update(revised.revision, FieldUpdate::Set("Wrong fence"));
    wrong_update.claim.token = &wrong;
    assert!(matches!(
        NativeTaskStore::revise_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            wrong_update,
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    assert!(matches!(
        NativeTaskStore::revise_session(
            ctx.snapshot(),
            &write,
            session.id,
            update(revised.revision, FieldUpdate::Set("Wrong authority")),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::WrongAdmission)
    ));
    let (other_bearer, _other_run) = f.issue_registered(&issuer, &ctx, &reference);
    let other_grant = issuer
        .verify(
            other_bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &ctx,
        )
        .unwrap();
    let other = authority
        .admit_write(&other_grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    assert!(matches!(
        NativeTaskStore::revise_session(
            ctx.snapshot(),
            &other,
            session.id,
            update(revised.revision, FieldUpdate::Set("Wrong actor")),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    assert_eq!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::SessionGetInput {
                id: session.id,
                history: Some(graphit_records::task_store::SessionDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .unwrap()
        .unwrap(),
        before
    );
    assert_eq!(
        NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        before_view
    );
    cancel.cancel();
    assert!(
        NativeTaskStore::revise_session(
            ctx.snapshot(),
            &coordinate,
            session.id,
            update(revised.revision, FieldUpdate::Set("Cancelled")),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    // A fresh Original verifies the same bearer using its original issuer.
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    let fresh_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let fresh_read = authority
        .admit_read(&fresh_grant, &fresh, ProjectReadModule::Task)
        .unwrap();
    let fresh_coordinate = authority
        .admit_write(&fresh_grant, &fresh, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    assert_eq!(
        NativeTaskStore::read_session(
            fresh.snapshot(),
            &fresh_read,
            graphit_records::task_store::SessionGetInput {
                id: session.id,
                history: Some(graphit_records::task_store::SessionDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .unwrap()
        .unwrap(),
        before
    );
    assert_eq!(
        NativeTaskStore::read_task(
            fresh.snapshot(),
            &fresh_read,
            graphit_records::task_store::TaskGetInput::current(child.specification.id),
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .task,
        child
    );
    NativeTaskStore::release_session(
        fresh.snapshot(),
        &fresh_coordinate,
        session.id,
        ReleaseUpdate {
            claim: ClaimUpdate {
                token: &claimed.token,
                expected_revision: revised.revision,
            },
            summary: "Revision verified and retained".into(),
            next_step: "Complete the separate terminal fixture".into(),
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    // A terminal Session has no children blocking completion.
    let fresh_write = authority
        .admit_write(&fresh_grant, &fresh, ProjectWriteOperation::Task)
        .unwrap();
    let terminal = NativeTaskStore::create_session(
        fresh.snapshot(),
        &fresh_write,
        session_input("terminal-revision"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let terminal_claim = NativeTaskStore::claim_session(
        fresh.snapshot(),
        &fresh_coordinate,
        terminal.id,
        Duration::from_secs(300),
        write_limits(),
    )
    .await
    .unwrap();
    let completed = NativeTaskStore::complete_session(
        fresh.snapshot(),
        &fresh_coordinate,
        terminal.id,
        ClaimUpdate {
            token: &terminal_claim.token,
            expected_revision: terminal_claim.record.revision,
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    let mut terminal_update = update(completed.revision, FieldUpdate::Set("Cannot reopen"));
    terminal_update.claim.token = &terminal_claim.token;
    assert!(
        NativeTaskStore::revise_session(
            fresh.snapshot(),
            &fresh_coordinate,
            terminal.id,
            terminal_update,
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(
        NativeTaskStore::read_session(
            fresh.snapshot(),
            &fresh_read,
            graphit_records::task_store::SessionGetInput::current(terminal.id),
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .session,
        completed
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_progress_excludes_cancelled_without_removing_inventory() {
    use graphit_records::{gates::TaskStatus, reference_projection::AuthoredReferencesField};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let mut session_ids = Vec::new();
    for key in ["mixed-progress", "cancelled-progress", "empty-progress"] {
        session_ids.push(
            NativeTaskStore::create_session(
                ctx.snapshot(),
                &write,
                session_input(key),
                AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap()
            .id,
        );
    }
    let mut task_ids = Vec::new();
    for (session, key) in [
        (session_ids[0], "done"),
        (session_ids[0], "open"),
        (session_ids[0], "cancelled"),
        (session_ids[1], "only-cancelled"),
    ] {
        task_ids.push(
            NativeTaskStore::create_task(
                ctx.snapshot(),
                &write,
                task_input(session, key),
                AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap()
            .specification
            .id,
        );
    }
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        task_ids[0],
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let passed = pass_task_checks(ctx.snapshot(), &write, &claim.token, claim.record).await;
    NativeTaskStore::complete_task(
        ctx.snapshot(),
        &write,
        task_ids[0],
        owner_input(&claim.token, &passed),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let before = NativeTaskStore::session_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    let mixed = before
        .iter()
        .find(|head| head.id == session_ids[0])
        .unwrap();
    assert_eq!((mixed.completed_tasks, mixed.total_tasks), (1, 3));
    for id in [task_ids[2], task_ids[3]] {
        NativeTaskStore::cancel_task(
            ctx.snapshot(),
            &write,
            id,
            TaskCancellation {
                claim: None,
                reason: "No longer required",
            },
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let TaskReadView::Published {
        tasks, sessions, ..
    } = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published")
    };
    assert_eq!(tasks.len(), 4);
    assert_eq!(
        tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Cancelled)
            .count(),
        2
    );
    let heads = NativeTaskStore::session_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    let TaskHeadObservation::Published {
        tasks: observed_tasks,
        sessions: observed_sessions,
        ..
    } = NativeTaskStore::observe_heads(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published")
    };
    assert_eq!(observed_tasks.unwrap().len(), 4);
    for view in [sessions, heads, observed_sessions.unwrap()] {
        for (id, expected) in session_ids.iter().zip([(1, 2), (0, 0), (0, 0)]) {
            let head = view.iter().find(|head| head.id == *id).unwrap();
            assert_eq!((head.completed_tasks, head.total_tasks), expected);
        }
    }
}

#[path = "task_store/query.rs"]
mod query;

#[path = "task_store/get_pages.rs"]
mod get_pages;

#[path = "task_store/session_get.rs"]
mod session_get;

#[path = "task_store/embeddings.rs"]
mod embeddings;
