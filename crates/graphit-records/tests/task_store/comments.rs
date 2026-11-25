use super::*;
use graphit_auth::ProjectWriteAdmission;
use graphit_records::gates::TaskStatus;

macro_rules! ordinary {
    ($f:ident,$ctx:ident,$issuer:ident,$caller:ident,$authority:ident,$read:ident,$write:ident) => {
        let $issuer = $f.issuer(&$ctx);
        authenticated!($f, $ctx, $issuer, $caller, $authority, $read, $write);
    };
}
macro_rules! authenticated {
    ($f:ident,$ctx:ident,$issuer:ident,$caller:ident,$authority:ident,$read:ident,$write:ident) => {
        let secret =
            std::fs::read_to_string($f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let $caller = $issuer.authenticate_runtime(&secret, &$ctx).unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $read = $authority
            .admit_authenticated_read(&$caller, &$ctx, $f.scope(), ProjectReadModule::Task)
            .unwrap();
        let $write = $authority
            .admit_authenticated_write(&$caller, &$ctx, $f.scope(), ProjectWriteOperation::Task)
            .unwrap();
    };
}
fn input<'a>(
    token: &'a ClaimToken,
    revision: u64,
    key: &str,
    kind: TaskCommentKind,
    body: &str,
) -> TaskCommentUpdate<'a> {
    TaskCommentUpdate {
        claim: ClaimUpdate {
            token,
            expected_revision: revision,
        },
        lease: Duration::from_secs(30),
        idempotency_key: key.into(),
        kind,
        body: body.into(),
    }
}
async fn seed(
    original: &Arc<SnapshotContext>,
    write: &ProjectWriteAdmission<'_>,
    key: &str,
) -> Claim<Task> {
    let session = NativeTaskStore::create_session(
        original,
        write,
        session_input(key),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        original,
        write,
        task_input(session.id, key),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    NativeTaskStore::claim_task(
        original,
        write,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap()
}
fn created(outcome: TaskCommentOutcome) -> (Task, TaskComment) {
    let TaskCommentOutcome::Created { task, comment } = outcome else {
        panic!("new comment")
    };
    (task, comment)
}
async fn generation(
    original: &Arc<SnapshotContext>,
    read: &graphit_auth::ProjectReadAdmission<'_>,
) -> TaskGeneration {
    let TaskReadView::Published { generation, .. } =
        NativeTaskStore::read_view(original, read, read_limits())
            .await
            .unwrap()
    else {
        panic!("published")
    };
    generation
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_literal_order_retry_reclaim_and_terminal_history() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    ordinary!(f, ctx, issuer, caller, authority, read, write);
    let claim = seed(ctx.snapshot(), &write, "comments").await;
    let id = claim.record.specification.id;
    let original_revision = claim.record.specification.revision;
    let body = "  **Decision**\n\n    preserve indentation\n";
    let (first, c1) = created(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                original_revision,
                "key",
                TaskCommentKind::Decision,
                body,
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(c1.body, body);
    assert_eq!(c1.sequence.get(), 1);
    assert_eq!(c1.revision, original_revision + 1);
    assert_eq!(
        first.ownership.lease_expires_at,
        claim.record.ownership.lease_expires_at
    );
    assert_eq!(c1.actor, first.ownership.owner.as_ref().unwrap().as_str());
    let before = generation(ctx.snapshot(), &read).await;
    let TaskCommentOutcome::Existing { task, comment } = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &write,
        id,
        input(
            &claim.token,
            original_revision,
            "key",
            TaskCommentKind::Decision,
            body,
        ),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("replay")
    };
    assert_eq!(task, first);
    assert_eq!(comment, c1);
    assert_eq!(generation(ctx.snapshot(), &read).await, before);
    for (kind, other) in [
        (TaskCommentKind::Note, body),
        (TaskCommentKind::Decision, "changed"),
    ] {
        assert!(matches!(
            NativeTaskStore::add_task_comment(
                ctx.snapshot(),
                &write,
                id,
                input(
                    &claim.token,
                    first.specification.revision,
                    "key",
                    kind,
                    other
                ),
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits()
            )
            .await,
            Err(TaskStoreError::IdempotencyConflict)
        ));
    }
    let (second, c2) = created(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                first.specification.revision,
                "key2",
                TaskCommentKind::Problem,
                "Keep rationale",
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_ne!(c1.id, c2.id);
    assert_eq!(c2.sequence.get(), 2);
    let current = NativeTaskStore::heartbeat_task(
        ctx.snapshot(),
        &write,
        id,
        ClaimUpdate {
            token: &claim.token,
            expected_revision: second.specification.revision,
        },
        Duration::from_secs(1),
        write_limits(),
    )
    .await
    .unwrap();
    let before = generation(ctx.snapshot(), &read).await;
    let TaskCommentOutcome::Existing { task, .. } = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &write,
        id,
        input(
            &claim.token,
            current.specification.revision,
            "key",
            TaskCommentKind::Decision,
            body,
        ),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("current replay")
    };
    assert_eq!(task, current);
    assert_eq!(generation(ctx.snapshot(), &read).await, before);
    assert!(matches!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                second.specification.revision,
                "key",
                TaskCommentKind::Decision,
                body
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::RevisionConflict)
    ));
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id,
            history: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
            comments: Some(graphit_records::task_store::TaskDetailPageInput {
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
    assert!(detail.comments.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(
        detail.comments.as_ref().unwrap().comments,
        vec![c1.clone(), c2]
    );
    assert_eq!(
        detail
            .history
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter(|e| matches!(e.kind, LifecycleKind::CommentAdded { .. }))
            .count(),
        2
    );
    assert!(!serde_json::to_string(&detail).unwrap().contains("token"));
    let released = NativeTaskStore::release_task(
        ctx.snapshot(),
        &write,
        id,
        ReleaseUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: current.specification.revision,
            },
            summary: "Preserve context".into(),
            next_step: "Continue".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(released.status, TaskStatus::Open);
    let new_claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(new_claim.record.ownership.claim_epoch > claim.record.ownership.claim_epoch);
    let before = generation(ctx.snapshot(), &read).await;
    for token in [&claim.token, &new_claim.token] {
        assert!(matches!(
            NativeTaskStore::add_task_comment(
                ctx.snapshot(),
                &write,
                id,
                input(
                    token,
                    new_claim.record.specification.revision,
                    "key",
                    TaskCommentKind::Decision,
                    body
                ),
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits()
            )
            .await,
            Err(TaskStoreError::FenceMismatch)
        ));
    }
    assert_eq!(generation(ctx.snapshot(), &read).await, before);
    let (latest, c3) = created(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &new_claim.token,
                new_claim.record.specification.revision,
                "key3",
                TaskCommentKind::Knowledge,
                "New claim",
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(c3.sequence.get(), 3);
    let cancelled = NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        id,
        TaskCancellation {
            claim: Some(ClaimUpdate {
                token: &new_claim.token,
                expected_revision: latest.specification.revision,
            }),
            reason: "Scope withdrawn",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert_eq!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput {
                id,
                history: None,
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .comments
        .as_ref()
        .unwrap()
        .comments
        .len(),
        3
    );
    let heads = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap();
    let TaskReadView::Published {
        tasks, sessions, ..
    } = heads
    else {
        panic!("heads")
    };
    assert_eq!(tasks.len(), 1);
    assert_eq!(sessions[0].total_tasks, 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_concurrent_same_key_and_invalid_authority_have_no_duplicate() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    ordinary!(f, ctx, issuer, caller, authority, read, write);
    let claim = seed(ctx.snapshot(), &write, "race").await;
    let id = claim.record.specification.id;
    let revision = claim.record.specification.revision;
    let (a, b) = tokio::join!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                revision,
                "race",
                TaskCommentKind::Lesson,
                "Concurrent exact attempt"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        ),
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                revision,
                "race",
                TaskCommentKind::Lesson,
                "Concurrent exact attempt"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
    );
    let values = [a.unwrap(), b.unwrap()];
    assert_eq!(
        values
            .iter()
            .filter(|x| matches!(x, TaskCommentOutcome::Created { .. }))
            .count(),
        1
    );
    assert_eq!(
        values
            .iter()
            .filter(|x| matches!(x, TaskCommentOutcome::Existing { .. }))
            .count(),
        1
    );
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id,
            history: None,
            comments: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(detail.comments.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(detail.comments.as_ref().unwrap().comments.len(), 1);
    assert_eq!(detail.task.specification.revision, revision + 1);
    let current = detail.task.specification.revision;
    let before = generation(ctx.snapshot(), &read).await;
    for (key, body, lease) in [
        ("invalid", " \n ", Duration::from_secs(1)),
        (" bad", "body", Duration::from_secs(1)),
        ("invalid", "body", Duration::ZERO),
    ] {
        let mut update = input(&claim.token, current, key, TaskCommentKind::Note, body);
        update.lease = lease;
        assert!(matches!(
            NativeTaskStore::add_task_comment(
                ctx.snapshot(),
                &write,
                id,
                update,
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits()
            )
            .await,
            Err(TaskStoreError::Invalid(_))
        ));
    }
    assert!(matches!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                revision,
                "new",
                TaskCommentKind::Note,
                "stale"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::RevisionConflict)
    ));
    let wrong = ClaimToken::from_bytes([7; 32]);
    assert!(matches!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(&wrong, current, "new", TaskCommentKind::Note, "wrong"),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let foreign_cancel = Cancellation::default();
    let foreign = request(&foreign_cancel);
    assert!(matches!(
        NativeTaskStore::add_task_comment(
            foreign.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                current,
                "new",
                TaskCommentKind::Note,
                "foreign original"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::DifferentRequest)
    ));
    assert!(
        authority
            .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)
            .is_err()
    );
    let wrong_module = authority
        .admit_authenticated_write(
            &caller,
            &ctx,
            f.scope(),
            ProjectWriteOperation::CoordinateTask,
        )
        .unwrap();
    assert!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &wrong_module,
            id,
            input(
                &claim.token,
                current,
                "new",
                TaskCommentKind::Note,
                "wrong module"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    let (bearer, reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let foreign_actor = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    assert!(matches!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &foreign_actor,
            id,
            input(
                &claim.token,
                current,
                "new",
                TaskCommentKind::Note,
                "foreign actor"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    assert_eq!(generation(ctx.snapshot(), &read).await, before);
    cancel.cancel();
    assert!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                current,
                "new",
                TaskCommentKind::Note,
                "cancelled"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    authenticated!(f, fresh, issuer, caller2, authority2, read2, write2);
    let TaskCommentOutcome::Existing { comment, .. } = NativeTaskStore::add_task_comment(
        fresh.snapshot(),
        &write2,
        id,
        input(
            &claim.token,
            revision,
            "race",
            TaskCommentKind::Lesson,
            "Concurrent exact attempt",
        ),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("fresh real authority replay")
    };
    assert_eq!(
        comment.actor,
        detail.comments.as_ref().unwrap().comments[0].actor
    );
    assert_eq!(generation(fresh.snapshot(), &read2).await, before);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_inherited_replay_has_no_branch_birth_then_insert_isolated() {
    use graphit_core::{
        project_paths::{InstallationPaths, ProjectPaths},
        version_context::{Checkout, VersionContext},
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (claim, main, comment) = {
        authenticated!(f, ctx, issuer, caller, authority, read, write);
        let claim = seed(ctx.snapshot(), &write, "branch").await;
        let (main, comment) = created(
            NativeTaskStore::add_task_comment(
                ctx.snapshot(),
                &write,
                claim.record.specification.id,
                input(
                    &claim.token,
                    claim.record.specification.revision,
                    "key",
                    TaskCommentKind::Note,
                    "Main comment",
                ),
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(
            NativeTaskStore::read_task(
                ctx.snapshot(),
                &read,
                graphit_records::task_store::TaskGetInput {
                    id: main.specification.id,
                    history: None,
                    comments: Some(graphit_records::task_store::TaskDetailPageInput {
                        page_size: std::num::NonZeroUsize::new(64).unwrap(),
                        cursor: None
                    })
                },
                read_limits()
            )
            .await
            .unwrap()
            .unwrap()
            .comments
            .as_ref()
            .unwrap()
            .comments,
            vec![comment.clone()]
        );
        (claim, main, comment)
    };
    let install = InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = ProjectPaths::at(&f.root, &install).unwrap();
    f.version = VersionContext::select(&paths, Checkout::NonGit, "feature", None, "").unwrap();
    {
        authenticated!(f, ctx, issuer, caller, authority, read, write);
        let before = generation(ctx.snapshot(), &read).await;
        assert!(before.physical_branch.is_none());
        assert_eq!(before.inherited_target.as_deref(), Some("feature"));
        let TaskCommentOutcome::Existing {
            task,
            comment: observed,
        } = NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            main.specification.id,
            input(
                &claim.token,
                claim.record.specification.revision,
                "key",
                TaskCommentKind::Note,
                "Main comment",
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap()
        else {
            panic!("inherited replay")
        };
        assert_eq!(task, main);
        assert_eq!(observed, comment);
        assert_eq!(generation(ctx.snapshot(), &read).await, before);
        let (feature, added) = created(
            NativeTaskStore::add_task_comment(
                ctx.snapshot(),
                &write,
                main.specification.id,
                input(
                    &claim.token,
                    main.specification.revision,
                    "feature",
                    TaskCommentKind::Note,
                    "Feature comment",
                ),
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(added.sequence.get(), 2);
        let after = generation(ctx.snapshot(), &read).await;
        assert_eq!(after.physical_branch.as_deref(), Some("feature"));
        assert!(after.inherited_target.is_none());
        assert_eq!(
            NativeTaskStore::read_task(
                ctx.snapshot(),
                &read,
                graphit_records::task_store::TaskGetInput {
                    id: feature.specification.id,
                    history: None,
                    comments: Some(graphit_records::task_store::TaskDetailPageInput {
                        page_size: std::num::NonZeroUsize::new(64).unwrap(),
                        cursor: None
                    })
                },
                read_limits()
            )
            .await
            .unwrap()
            .unwrap()
            .comments
            .as_ref()
            .unwrap()
            .comments
            .len(),
            2
        );
    }
    f.version = VersionContext::select(&paths, Checkout::NonGit, "main", None, "").unwrap();
    authenticated!(f, ctx, issuer, caller, authority, read, _write);
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id: main.specification.id,
            history: None,
            comments: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(detail.comments.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(detail.task, main);
    assert_eq!(detail.comments.as_ref().unwrap().comments, vec![comment]);
    let other_cancel = Cancellation::default();
    let other_ctx = request(&other_cancel);
    let other = Fixture::new(&other_ctx, "main");
    ordinary!(
        other,
        other_ctx,
        other_issuer,
        other_caller,
        other_authority,
        other_read,
        other_write
    );
    assert!(
        NativeTaskStore::read_task(
            other_ctx.snapshot(),
            &other_read,
            graphit_records::task_store::TaskGetInput::current(main.specification.id),
            read_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        NativeTaskStore::add_task_comment(
            other_ctx.snapshot(),
            &other_write,
            main.specification.id,
            input(
                &claim.token,
                main.specification.revision,
                "key",
                TaskCommentKind::Note,
                "Main comment"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(
        NativeTaskStore::read_view(other_ctx.snapshot(), &other_read, read_limits())
            .await
            .unwrap(),
        TaskReadView::NoCorpus
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_expired_claim_and_deadline_deny_without_publication() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    ordinary!(f, ctx, issuer, caller, authority, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("expired"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "expired"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        task.specification.id,
        Duration::from_millis(500),
        write_limits(),
    )
    .await
    .unwrap();
    wait_for_task_lease_expiry(&claim.record).await;
    let before = generation(ctx.snapshot(), &read).await;
    assert!(matches!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            task.specification.id,
            input(
                &claim.token,
                claim.record.specification.revision,
                "new",
                TaskCommentKind::Note,
                "expired"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::LeaseExpired)
    ));
    assert_eq!(generation(ctx.snapshot(), &read).await, before);
    let expired_cancel = Cancellation::default();
    let expired = fixture::request_until(&expired_cancel, Instant::now() + Duration::from_secs(2));
    let expired_issuer = f.issuer(&expired);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let expired_caller = expired_issuer
        .authenticate_runtime(&secret, &expired)
        .unwrap();
    let expired_authority = ProjectRecordAuthority::new(&expired_issuer);
    let expired_write = expired_authority
        .admit_authenticated_write(
            &expired_caller,
            &expired,
            f.scope(),
            ProjectWriteOperation::OwnTask,
        )
        .unwrap();
    tokio::time::sleep_until(expired.snapshot().deadline().unwrap().into()).await;
    assert!(
        NativeTaskStore::add_task_comment(
            expired.snapshot(),
            &expired_write,
            task.specification.id,
            input(
                &claim.token,
                claim.record.specification.revision,
                "new",
                TaskCommentKind::Note,
                "deadline"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(generation(ctx.snapshot(), &read).await, before);
    assert!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput {
                id: task.specification.id,
                history: None,
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .comments
        .as_ref()
        .unwrap()
        .comments
        .is_empty()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_corrupt_projection_and_missing_body_fail_entire_detail() {
    use arrow_array::{RecordBatch, StringArray};
    use futures::TryStreamExt;
    use graphit_data::lance_branch_store::LanceBranchStore;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    ordinary!(f, ctx, issuer, caller, authority, read, write);
    let claim = seed(ctx.snapshot(), &write, "corrupt").await;
    let id = claim.record.specification.id;
    let (first, first_comment) = created(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                claim.record.specification.revision,
                "one",
                TaskCommentKind::Note,
                "One",
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let (_, last) = created(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                first.specification.revision,
                "two",
                TaskCommentKind::Decision,
                "Two",
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let uri = f.project.runtime_root().join("task/index.lance");
    let uri = uri.to_str().unwrap();
    let dataset = lance::Dataset::open(uri).await.unwrap();
    let mut scan = dataset.scan();
    scan.filter("kind = 'task_comment'").unwrap();
    let saved: Vec<RecordBatch> = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(saved.iter().map(|b| b.num_rows()).sum::<usize>(), 2);
    let batch = &saved[0];
    let mut bad = Vec::new();
    for batch in &saved {
        let payload = batch
            .column_by_name("payload")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let mut strings = Vec::new();
        for row in 0..batch.num_rows() {
            let mut value: serde_json::Value = serde_json::from_str(payload.value(row)).unwrap();
            if value["record"]["record"]["id"] == serde_json::to_value(last.id).unwrap() {
                value["record"]["record"]["sequence"] = serde_json::json!(99);
            }
            strings.push(serde_json::to_string(&value).unwrap());
        }
        let mut columns = batch.columns().to_vec();
        columns[batch.schema().index_of("payload").unwrap()] = Arc::new(StringArray::from(strings));
        bad.push(RecordBatch::try_new(batch.schema(), columns).unwrap());
    }
    let store = LanceBranchStore::open(uri, batch.schema(), None)
        .await
        .unwrap();
    store
        .capture_target(None)
        .await
        .unwrap()
        .prepare_merge(bad, vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    assert!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput {
                id,
                history: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                }),
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .is_err()
    );
    let store = LanceBranchStore::open(uri, batch.schema(), None)
        .await
        .unwrap();
    store
        .capture_target(None)
        .await
        .unwrap()
        .prepare_merge(saved.clone(), vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    assert_eq!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput {
                id,
                history: None,
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .comments
        .as_ref()
        .unwrap()
        .comments
        .len(),
        2
    );
    // Corrupt an intermediate event with coherent body/header session values.
    // This reaches history attribution rather than the projection codec.
    let dataset = lance::Dataset::open(uri).await.unwrap();
    let mut scan = dataset.scan();
    scan.filter("kind = 'task_event'").unwrap();
    let events: Vec<RecordBatch> = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    let mut bad = Vec::new();
    for event_batch in &events {
        let payload = event_batch
            .column_by_name("payload")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let sessions = event_batch
            .column_by_name("session_id")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let foreign = serde_json::to_value(SessionId::new(NonZeroU128::new(999).unwrap())).unwrap();
        let mut bodies = Vec::new();
        let mut headers = Vec::new();
        for row in 0..event_batch.num_rows() {
            let mut value: serde_json::Value = serde_json::from_str(payload.value(row)).unwrap();
            if value["record"]["record"]["specification"]["revision"]
                == serde_json::json!(first_comment.revision)
            {
                value["record"]["record"]["session_id"] = foreign.clone();
                headers.push(foreign.as_str().unwrap().to_owned());
            } else {
                headers.push(sessions.value(row).to_owned());
            }
            bodies.push(serde_json::to_string(&value).unwrap());
        }
        let mut columns = event_batch.columns().to_vec();
        columns[event_batch.schema().index_of("payload").unwrap()] =
            Arc::new(StringArray::from(bodies));
        columns[event_batch.schema().index_of("session_id").unwrap()] =
            Arc::new(StringArray::from(headers));
        bad.push(RecordBatch::try_new(event_batch.schema(), columns).unwrap());
    }
    let store = LanceBranchStore::open(uri, events[0].schema(), None)
        .await
        .unwrap();
    store
        .capture_target(None)
        .await
        .unwrap()
        .prepare_merge(bad, vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    assert!(matches!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput {
                id,
                history: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                }),
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await,
        Err(TaskStoreError::Invalid("selected Task transition"))
    ));
    let store = LanceBranchStore::open(uri, events[0].schema(), None)
        .await
        .unwrap();
    store
        .capture_target(None)
        .await
        .unwrap()
        .prepare_merge(events, vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let mut dataset = lance::Dataset::open(uri).await.unwrap();
    dataset.delete("kind = 'task_comment'").await.unwrap();
    assert!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput {
                id,
                history: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                }),
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .is_err()
    );
    let store = LanceBranchStore::open(uri, batch.schema(), None)
        .await
        .unwrap();
    store
        .capture_target(None)
        .await
        .unwrap()
        .prepare_merge(saved, vec!["row_key".into()])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    let old = lance::Dataset::open(uri).await.unwrap();
    let old_version = old.version().version;
    let mut current = lance::Dataset::open(uri).await.unwrap();
    current.delete("kind = 'task'").await.unwrap();
    assert!(current.version().version > old_version);
    assert_eq!(
        old.count_rows(Some("kind = 'task'".into())).await.unwrap(),
        1
    );
    assert!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput {
                id,
                history: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                }),
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_distinct_concurrent_keys_cas_completed_history_and_task_local_keys() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    ordinary!(f, ctx, issuer, caller, authority, read, write);
    let claim = seed(ctx.snapshot(), &write, "distinct").await;
    let id = claim.record.specification.id;
    let (a, b) = tokio::join!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                claim.record.specification.revision,
                "one",
                TaskCommentKind::Note,
                "One"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        ),
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                claim.record.specification.revision,
                "two",
                TaskCommentKind::Note,
                "Two"
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits()
        )
    );
    let success = match (a, b) {
        (Ok(a), Err(TaskStoreError::RevisionConflict))
        | (Err(TaskStoreError::RevisionConflict), Ok(a)) => a,
        _ => panic!("exactly one CAS winner"),
    };
    let (task, comment) = created(success);
    let task = pass_task_checks(ctx.snapshot(), &write, &claim.token, task).await;
    let task = NativeTaskStore::complete_task(
        ctx.snapshot(),
        &write,
        id,
        owner_input(&claim.token, &task),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(task.status, TaskStatus::Completed);
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput {
            id,
            history: None,
            comments: Some(graphit_records::task_store::TaskDetailPageInput {
                page_size: std::num::NonZeroUsize::new(64).unwrap(),
                cursor: None,
            }),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(detail.comments.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(
        detail.comments.as_ref().unwrap().comments,
        vec![comment.clone()]
    );
    let other = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(task.session_id, "other"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        other.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let (_, fresh) = created(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            other.specification.id,
            input(
                &claim.token,
                claim.record.specification.revision,
                &comment.idempotency_key,
                TaskCommentKind::Lesson,
                "Separate Task",
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(fresh.sequence.get(), 1);
    assert_ne!(fresh.id, comment.id);
    assert_eq!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::TaskGetInput {
                id,
                history: None,
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .comments
        .as_ref()
        .unwrap()
        .comments,
        vec![comment]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_revoked_real_role_admission_cannot_publish_or_replay() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let authority = ProjectRecordAuthority::new(&issuer);
    let (_, reference, _) = f.issue(&issuer, &ctx);
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(60))
        .unwrap();
    let permissions = [
        graphit_auth::Permission::Read(graphit_core::module_settings::Module::Task),
        graphit_auth::Permission::WriteTask,
        graphit_auth::Permission::WriteOwnTask,
    ];
    let bearer = issuer
        .issue(&parent, &run, &reference, &permissions, &f.scope())
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let claim = seed(ctx.snapshot(), &write, "revoked").await;
    let id = claim.record.specification.id;
    let (task, comment) = created(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            input(
                &claim.token,
                claim.record.specification.revision,
                "one",
                TaskCommentKind::Note,
                "One",
            ),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let ordinary_read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Task)
        .unwrap();
    let before = generation(ctx.snapshot(), &ordinary_read).await;
    issuer.revoke(&parent, &bearer).unwrap();
    for key in ["one", "new"] {
        assert!(matches!(
            NativeTaskStore::add_task_comment(
                ctx.snapshot(),
                &write,
                id,
                input(
                    &claim.token,
                    task.specification.revision,
                    key,
                    TaskCommentKind::Note,
                    "One"
                ),
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits()
            )
            .await,
            Err(TaskStoreError::Auth(_))
        ));
    }
    assert_eq!(generation(ctx.snapshot(), &ordinary_read).await, before);
    assert_eq!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &ordinary_read,
            graphit_records::task_store::TaskGetInput {
                id,
                history: None,
                comments: Some(graphit_records::task_store::TaskDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .comments
        .as_ref()
        .unwrap()
        .comments,
        vec![comment]
    );
}
