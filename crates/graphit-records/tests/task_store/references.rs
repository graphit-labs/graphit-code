use super::*;
use graphit_records::{
    reference_projection::*,
    task_spec::{CheckStatus, FieldUpdate},
};
fn limits() -> TaskStoreReadLimits {
    read_limits()
}
fn query(focus: TaskReferenceRecord, size: usize) -> TaskReferencesQuery {
    TaskReferencesQuery {
        focus,
        page_size: NonZeroUsize::new(size).unwrap(),
        outgoing_cursor: None,
        incoming_cursor: None,
    }
}
fn candidate<'a>(kind: &'a str, id: &'a str, relation: &'a str) -> AuthoredReferenceEntry<'a> {
    AuthoredReferenceEntry::Tuple(ReferenceCandidate {
        target_type: kind,
        id,
        relation,
        scope: None,
        scope_id: None,
        context: None,
        field: None,
    })
}
fn pages(
    read: TaskReferencesRead,
) -> (
    TaskGeneration,
    TaskReferencePage,
    TaskReferencePage,
    TaskReferencesCoverage,
) {
    let TaskReferencesRead::Published {
        generation,
        outgoing,
        incoming,
        coverage,
        ..
    } = read
    else {
        panic!("published")
    };
    (generation, *outgoing, *incoming, coverage)
}
async fn refs(
    original: &Arc<SnapshotContext>,
    read: &graphit_auth::ProjectReadAdmission<'_>,
    focus: TaskReferenceRecord,
) -> (
    TaskGeneration,
    TaskReferencePage,
    TaskReferencePage,
    TaskReferencesCoverage,
) {
    pages(
        NativeTaskStore::references(original, read, query(focus, 100), limits())
            .await
            .unwrap(),
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_actual_writes_reopen_structural_authored_duplicate_literal_scopes_and_resolution()
 {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, registered_reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &registered_reference,
            f.scope(),
            &ctx,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let empty = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("empty"),
        AuthoredReferencesField::Sequence(&[]),
        write_limits(),
    )
    .await
    .unwrap();
    let (_, outgoing, incoming, coverage) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Session(empty.id),
    )
    .await;
    assert!(outgoing.complete && outgoing.results.is_empty() && incoming.complete);
    assert!(coverage.inbound_other_sources_not_qualified);
    let parent = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(empty.id, "parent"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let dependency = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(empty.id, "dependency"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let parent_id = serde_json::to_value(parent.specification.id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let foreign_id: graphit_core::project_identity::ProjectId =
        "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
    let foreign_text = serde_json::to_value(foreign_id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let mut input = task_input(empty.id, "child");
    input.parent_id = Some(parent.specification.id);
    input.depends_on = vec![dependency.specification.id];
    let entries = [
        candidate("task", &parent_id, "  literal\nrelation "),
        candidate("task", &parent_id, "  literal\nrelation "),
        AuthoredReferenceEntry::Tuple(ReferenceCandidate {
            target_type: "knowledge",
            id: "Wiki",
            relation: "documents",
            scope: Some("user"),
            scope_id: Some("UserLiteral"),
            context: Some("Import@1"),
            field: Some(" body "),
        }),
        candidate("memory", "MemoryOpaque", "supports"),
        candidate("task", "opaque", "unresolved"),
        AuthoredReferenceEntry::Tuple(ReferenceCandidate {
            target_type: "task",
            id: &parent_id,
            relation: "foreign-project",
            scope: Some("project"),
            scope_id: Some(&foreign_text),
            context: None,
            field: None,
        }),
        AuthoredReferenceEntry::Tuple(ReferenceCandidate {
            target_type: "task",
            id: &parent_id,
            relation: "user-target",
            scope: Some("user"),
            scope_id: Some("UserLiteral"),
            context: None,
            field: None,
        }),
    ];
    let child = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        input,
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(child.references.len(), 7);
    assert_eq!(child.reference_generation, 1);
    let (g, outgoing, _, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Task(child.specification.id),
    )
    .await;
    assert_eq!(outgoing.total, 10);
    assert!(outgoing.complete);
    assert_eq!(
        outgoing
            .results
            .iter()
            .map(|e| e.reference.relation.as_str())
            .collect::<Vec<_>>(),
        vec![
            "session",
            "parent",
            "dependency",
            "  literal\nrelation ",
            "  literal\nrelation ",
            "documents",
            "supports",
            "unresolved",
            "foreign-project",
            "user-target"
        ]
    );
    assert_eq!(
        outgoing.results[0].provenance,
        TaskReferenceProvenance::Structural
    );
    assert_eq!(
        outgoing.results[3].provenance,
        TaskReferenceProvenance::Authored
    );
    assert!(
        matches!(outgoing.results[3].target_resolution, TaskReferenceResolution::Task { id, .. } if id == parent.specification.id)
    );
    assert!(
        matches!(outgoing.results[0].target_resolution, TaskReferenceResolution::Session { id, .. } if id == empty.id)
    );
    for i in 5..10 {
        assert_eq!(
            outgoing.results[i].target_resolution,
            TaskReferenceResolution::Unresolved
        );
    }
    assert_eq!(
        outgoing.results[5].reference.field.as_deref(),
        Some(" body ")
    );
    assert!(
        outgoing
            .results
            .iter()
            .all(|e| e.source_snapshot == g && e.source_revision == 1)
    );
    let (_, _, incoming, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Task(parent.specification.id),
    )
    .await;
    assert_eq!(incoming.total, 3);
    assert_eq!(
        incoming
            .results
            .iter()
            .map(|e| e.ordinal)
            .collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
    // Recreate actual admissions and reopen the SDK, without a cached observation.
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    let (bearer, _run) = f.issue_registered(&issuer, &fresh, &registered_reference);
    let fresh_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &registered_reference,
            f.scope(),
            &fresh,
        )
        .unwrap();
    let fresh_read = authority
        .admit_read(&fresh_grant, &fresh, ProjectReadModule::Task)
        .unwrap();
    let (again, outgoing_again, _, _) = refs(
        fresh.snapshot(),
        &fresh_read,
        TaskReferenceRecord::Task(child.specification.id),
    )
    .await;
    assert_eq!(again, g);
    assert_eq!(outgoing_again, outgoing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_input_preflight_entire_batch_bounds_auth_and_no_birth() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let valid = candidate("memory", "opaque", "supports");
    let invalid = [valid, AuthoredReferenceEntry::InvalidShape];
    let invalid_kind = [candidate("unknown", "id", "supports")];
    for field in [
        AuthoredReferencesField::Null,
        AuthoredReferencesField::NonSequence,
        AuthoredReferencesField::Sequence(&invalid),
        AuthoredReferencesField::Sequence(&invalid_kind),
    ] {
        assert!(
            NativeTaskStore::create_session(
                ctx.snapshot(),
                &write,
                session_input("bad"),
                field,
                write_limits()
            )
            .await
            .is_err()
        );
        assert!(!f.project.runtime_root().join("task").exists());
    }
    let mut bounded = write_limits();
    bounded.references.max_entries = NonZeroUsize::new(1).unwrap();
    assert!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("entries"),
            AuthoredReferencesField::Sequence(&[valid, valid]),
            bounded
        )
        .await
        .is_err()
    );
    bounded = write_limits();
    bounded.references.max_text_bytes = NonZeroUsize::new(1).unwrap();
    assert!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("text"),
            AuthoredReferencesField::Sequence(&[valid]),
            bounded
        )
        .await
        .is_err()
    );
    let other = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ));
    assert!(matches!(
        NativeTaskStore::create_session(
            &other,
            &write,
            session_input("wrong"),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::DifferentRequest)
    ));
    assert_eq!(
        NativeTaskStore::references(
            ctx.snapshot(),
            &read,
            query(
                TaskReferenceRecord::Task(TaskId::new(NonZeroU128::new(1).unwrap())),
                20
            ),
            limits()
        )
        .await
        .unwrap(),
        TaskReferencesRead::NoCorpus
    );
    cancel.cancel();
    assert!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("cancelled"),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeTaskStore::references(
            ctx.snapshot(),
            &read,
            query(
                TaskReferenceRecord::Task(TaskId::new(NonZeroU128::new(1).unwrap())),
                20
            ),
            limits()
        )
        .await
        .is_err()
    );
    assert!(!f.project.runtime_root().join("task").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_retry_patch_identity_ref_only_revision_preserves_checks_and_older_clear_never_resurrects()
 {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let own = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let entries = [candidate("memory", "first", "supports")];
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("s"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input("s"),
            AuthoredReferencesField::Sequence(&[]),
            write_limits()
        )
        .await,
        Err(TaskStoreError::IdempotencyConflict)
    ));
    let create = task_input(session.id, "t");
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        create.clone(),
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap();
    let initial_references = task.references.clone();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let token = claim.token;
    let mut task = claim.record;
    task = NativeTaskStore::verify_task_check(
        ctx.snapshot(),
        &own,
        task.specification.id,
        CheckUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            check_id: task.specification.checks[0].id,
            passed: true,
            evidence: "Actual proof",
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let checks = task.specification.checks.clone();
    assert_eq!(checks[0].status, CheckStatus::Passed);
    let update = RevisionUpdate {
        claim: owner_input(&token, &task),
        lease: Duration::from_secs(120),
        reason: "Clear authored only",
        title: FieldUpdate::Keep,
        description: FieldUpdate::Keep,
        task_type: FieldUpdate::Keep,
        priority: None,
        schedule: FieldUpdate::Keep,
        add_checks: &[],
    };
    task = NativeTaskStore::revise_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        update,
        AuthoredReferencesField::Sequence(&[]),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(task.references.is_empty());
    assert_eq!(task.specification.checks, checks);
    let retry = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        create,
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(retry, task); // immutable creation identity, current head, no replay
    let comment = || TaskCommentUpdate {
        claim: owner_input(&token, &task),
        lease: Duration::from_secs(120),
        idempotency_key: "comment".into(),
        kind: TaskCommentKind::Note,
        body: "literal body".into(),
    };
    let TaskCommentOutcome::Created {
        task: mut current, ..
    } = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &own,
        task.specification.id,
        comment(),
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("created")
    };
    current = NativeTaskStore::progress_task(
        ctx.snapshot(),
        &own,
        current.specification.id,
        ProgressUpdate {
            claim: owner_input(&token, &current),
            lease: Duration::from_secs(120),
            summary: "Clear".into(),
            next_step: "Observe".into(),
        },
        AuthoredReferencesField::Sequence(&[]),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &own,
            current.specification.id,
            comment(),
            AuthoredReferencesField::Omitted,
            write_limits()
        )
        .await,
        Err(TaskStoreError::IdempotencyConflict)
    ));
    let TaskCommentOutcome::Existing { task: retry, .. } = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &own,
        current.specification.id,
        comment(),
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("existing")
    };
    assert_eq!(retry, current);
    assert!(retry.references.is_empty());
    let (_, outgoing, _, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Task(current.specification.id),
    )
    .await;
    assert_eq!(outgoing.total, 1);
    assert_eq!(outgoing.results[0].reference.relation, "session");
    assert!(outgoing.complete);
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(current.specification.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        detail.creation.references_update,
        TaskReferenceUpdate::Replace(initial_references)
    );
    assert_eq!(
        detail.last_event.as_ref().unwrap().references_update,
        TaskReferenceUpdate::Replace(vec![])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_closed_cursors_bind_pin_focus_direction_limits_and_allow_page_size_change() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let entries = (0..25)
        .map(|_| candidate("memory", "opaque", "duplicate"))
        .collect::<Vec<_>>();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("pages"),
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap();
    let focus = TaskReferenceRecord::Session(session.id);
    let (_, first, _, _) = pages(
        NativeTaskStore::references(ctx.snapshot(), &read, query(focus, 20), limits())
            .await
            .unwrap(),
    );
    assert_eq!(first.results.len(), 20);
    assert_eq!(first.total, 25);
    let cursor = first.next_cursor.unwrap();
    let mut next = query(focus, 100);
    next.outgoing_cursor = Some(cursor.clone());
    let (_, second, _, _) = pages(
        NativeTaskStore::references(ctx.snapshot(), &read, next.clone(), limits())
            .await
            .unwrap(),
    );
    assert_eq!(
        second.results.iter().map(|e| e.ordinal).collect::<Vec<_>>(),
        vec![20, 21, 22, 23, 24]
    );
    assert!(second.next_cursor.is_none());
    let mut wrong = query(focus, 20);
    wrong.incoming_cursor = Some(cursor.clone());
    assert!(matches!(
        NativeTaskStore::references(ctx.snapshot(), &read, wrong, limits()).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let mut encoded = serde_json::to_value(&cursor).unwrap();
    encoded["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TaskReferencesCursor>(encoded).is_err());
    let mut encoded = serde_json::to_value(&cursor).unwrap();
    encoded["binding"]["pin"]["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TaskReferencesCursor>(encoded).is_err());
    let mut encoded = serde_json::to_value(&cursor).unwrap();
    encoded["offset"] = serde_json::json!(25);
    let equal = serde_json::from_value(encoded.clone()).unwrap();
    let mut equalq = query(focus, 20);
    equalq.outgoing_cursor = Some(equal);
    let (_, equal, _, _) = pages(
        NativeTaskStore::references(ctx.snapshot(), &read, equalq, limits())
            .await
            .unwrap(),
    );
    assert!(equal.results.is_empty() && equal.complete);
    encoded["offset"] = serde_json::json!(26);
    next.outgoing_cursor = Some(serde_json::from_value(encoded).unwrap());
    assert!(matches!(
        NativeTaskStore::references(ctx.snapshot(), &read, next.clone(), limits()).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    next.outgoing_cursor = Some(cursor.clone());
    NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("advance"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::references(ctx.snapshot(), &read, next, limits()).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let other = Fixture::new(&ctx, "main");
    let otherissuer = other.issuer(&ctx);
    let (bearer, reference, _run) = other.issue(&otherissuer, &ctx);
    let othergrant = otherissuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            other.scope(),
            &ctx,
        )
        .unwrap();
    let otherauthority = ProjectRecordAuthority::new(&otherissuer);
    let otherread = otherauthority
        .admit_read(&othergrant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let mut absent = query(focus, 20);
    absent.outgoing_cursor = Some(cursor);
    assert!(matches!(
        NativeTaskStore::references(ctx.snapshot(), &otherread, absent, limits()).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    assert!(!other.project.runtime_root().join("task").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_full_eof_and_named_source_version() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "feature/records");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let entries = [candidate("task", "opaque", "incoming-unresolved")];
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("named"),
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap();
    let focus = TaskReferenceRecord::Session(session.id);
    let (g, _, _, _) = refs(ctx.snapshot(), &read, focus).await;
    assert!(g.physical_branch.is_some());
    assert!(matches!(
        NativeTaskStore::references(
            ctx.snapshot(),
            &read,
            query(
                TaskReferenceRecord::Task(TaskId::new(NonZeroU128::new(1).unwrap())),
                20
            ),
            limits()
        )
        .await,
        Err(TaskStoreError::MissingTask)
    ));
    assert!(matches!(
        NativeTaskStore::references(
            ctx.snapshot(),
            &read,
            query(
                TaskReferenceRecord::Session(SessionId::new(NonZeroU128::new(1).unwrap())),
                20
            ),
            limits()
        )
        .await,
        Err(TaskStoreError::MissingSession)
    ));
    assert!(
        NativeTaskStore::references(ctx.snapshot(), &read, query(focus, 101), limits())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_all_mutators_claim_renew_noop_and_audit_generations_use_actual_sdk() {
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
    let entries = [candidate("memory", "every-mutator", "supports")];
    let field = AuthoredReferencesField::Sequence(&entries);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("all"),
        field,
        write_limits(),
    )
    .await
    .unwrap();
    let original = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "task"),
        field,
        write_limits(),
    )
    .await
    .unwrap();
    let expected = original.references.clone();
    let parent = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "parent"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        original.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let mut token = claim.token;
    let mut task = claim.record;
    let (g, _, _, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Task(task.specification.id),
    )
    .await;
    let same = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(same.record, task);
    let (again, _, _, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Task(task.specification.id),
    )
    .await;
    assert_eq!(g, again);
    macro_rules! checked {
        ($future:expr) => {{
            let next = $future.await.unwrap();
            assert_eq!(next.references, expected);
            assert_eq!(next.reference_generation, next.specification.revision);
            let (_, page, _, _) = refs(
                ctx.snapshot(),
                &read,
                TaskReferenceRecord::Task(next.specification.id),
            )
            .await;
            assert!(page.complete);
            assert_eq!(page.results.last().unwrap().reference.id, "every-mutator");
            assert!(
                page.results
                    .iter()
                    .all(|e| e.source_revision == next.specification.revision)
            );
            next
        }};
    }
    task = checked!(NativeTaskStore::heartbeat_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        owner_input(&token, &task),
        Duration::from_secs(180),
        write_limits()
    ));
    task = checked!(NativeTaskStore::verify_task_check(
        ctx.snapshot(),
        &own,
        task.specification.id,
        CheckUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            check_id: task.specification.checks[0].id,
            passed: true,
            evidence: "Actual pass"
        },
        field,
        write_limits()
    ));
    task = checked!(NativeTaskStore::supersede_task_check(
        ctx.snapshot(),
        &own,
        task.specification.id,
        SupersessionUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            check_id: task.specification.checks[1].id,
            reason: "Replace evidence check",
            replacement: Some(ReplacementInput {
                text: "Replacement functional check",
                kind: None
            })
        },
        field,
        write_limits()
    ));
    task = checked!(NativeTaskStore::revise_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        RevisionUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            reason: "Update title",
            title: FieldUpdate::Set("Updated title"),
            description: FieldUpdate::Keep,
            task_type: FieldUpdate::Keep,
            priority: None,
            schedule: FieldUpdate::Keep,
            add_checks: &[]
        },
        field,
        write_limits()
    ));
    let TaskCommentOutcome::Created { task: next, .. } = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &own,
        task.specification.id,
        TaskCommentUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            idempotency_key: "comment".into(),
            kind: TaskCommentKind::Note,
            body: "Actual comment".into(),
        },
        field,
        write_limits(),
    )
    .await
    .unwrap() else {
        panic!("comment")
    };
    task = next;
    assert_eq!(task.references, expected);
    task = checked!(NativeTaskStore::progress_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        ProgressUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            summary: "Progress".into(),
            next_step: "Flag".into()
        },
        field,
        write_limits()
    ));
    task = checked!(NativeTaskStore::set_task_flag(
        ctx.snapshot(),
        &own,
        task.specification.id,
        TaskFlagUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            flagged: true,
            reason: "Flag"
        },
        field,
        write_limits()
    ));
    task = checked!(NativeTaskStore::set_task_flag(
        ctx.snapshot(),
        &own,
        task.specification.id,
        TaskFlagUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            flagged: false,
            reason: "Unflag"
        },
        field,
        write_limits()
    ));
    task = checked!(NativeTaskStore::update_task_links(
        ctx.snapshot(),
        &own,
        task.specification.id,
        TaskLinksUpdate {
            claim: owner_input(&token, &task),
            lease: Duration::from_secs(120),
            parent_id: FieldUpdate::Set(Some(parent.specification.id)),
            depends_on: None,
            reason: "Parent"
        },
        field,
        write_limits()
    ));
    task = checked!(NativeTaskStore::release_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        ReleaseUpdate {
            claim: owner_input(&token, &task),
            summary: "Release".into(),
            next_step: "Reclaim".into()
        },
        field,
        write_limits()
    ));
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    token = claim.token;
    task = claim.record;
    let ids = task
        .specification
        .checks
        .iter()
        .filter(|c| c.status != CheckStatus::Superseded)
        .map(|c| c.id)
        .collect::<Vec<_>>();
    for id in ids {
        task = checked!(NativeTaskStore::verify_task_check(
            ctx.snapshot(),
            &own,
            task.specification.id,
            CheckUpdate {
                claim: owner_input(&token, &task),
                lease: Duration::from_secs(120),
                check_id: id,
                passed: true,
                evidence: "Final actual pass"
            },
            field,
            write_limits()
        ));
    }
    task = checked!(NativeTaskStore::complete_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        owner_input(&token, &task),
        field,
        write_limits()
    ));
    let cancelled = NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        parent.specification.id,
        TaskCancellation {
            claim: None,
            reason: "Parent cancelled",
        },
        field,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(cancelled.references, expected);
    let (_, page, _, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Task(cancelled.specification.id),
    )
    .await;
    assert_eq!(page.results.last().unwrap().reference.id, "every-mutator");
    let claim = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        session.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let mut current = claim.record;
    let mut stoken = claim.token;
    current = NativeTaskStore::heartbeat_session(
        ctx.snapshot(),
        &coordinate,
        current.id,
        ClaimUpdate {
            token: &stoken,
            expected_revision: current.revision,
        },
        Duration::from_secs(180),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(current.references, expected);
    current = NativeTaskStore::checkpoint_session(
        ctx.snapshot(),
        &coordinate,
        current.id,
        ProgressUpdate {
            claim: ClaimUpdate {
                token: &stoken,
                expected_revision: current.revision,
            },
            lease: Duration::from_secs(120),
            summary: "Checkpoint".into(),
            next_step: "Release".into(),
        },
        field,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(current.references, expected);
    current = NativeTaskStore::release_session(
        ctx.snapshot(),
        &coordinate,
        current.id,
        ReleaseUpdate {
            claim: ClaimUpdate {
                token: &stoken,
                expected_revision: current.revision,
            },
            summary: "Release".into(),
            next_step: "Close".into(),
        },
        field,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(current.references, expected);
    let claim = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        current.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    current = claim.record;
    stoken = claim.token;
    current = NativeTaskStore::complete_session(
        ctx.snapshot(),
        &coordinate,
        current.id,
        ClaimUpdate {
            token: &stoken,
            expected_revision: current.revision,
        },
        field,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(current.references, expected);
    assert_eq!(current.reference_generation, current.revision);
    let (_, page, incoming, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Session(current.id),
    )
    .await;
    assert_eq!(page.total, 1);
    assert!(page.complete && incoming.complete);
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
    assert_eq!(
        detail.history.as_ref().unwrap().events.len() as u64 + 1,
        task.specification.revision
    );
    assert!(
        detail
            .history
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|e| e.record.reference_generation == e.record.specification.revision)
    );
    let detail = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::SessionGetInput {
            id: current.id,
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
    assert!(
        detail
            .history
            .as_ref()
            .unwrap()
            .events
            .iter()
            .all(|e| e.record.reference_generation == e.record.revision)
    );
    // A real persisted projection exists for every head/event revision, same SDK corpus.
    let pin = super::lexical_index::pin(&f).await;
    let mut scan = pin.scan();
    scan.filter("kind = 'reference_projection'").unwrap();
    use futures::TryStreamExt;
    let batches: Vec<_> = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        batches.iter().map(|b| b.num_rows()).sum::<usize>() as u64,
        task.specification.revision + current.revision + cancelled.specification.revision
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_incoming_stable_source_order_across_pages_same_actual_pin() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("s"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let focus = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "focus"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let id = serde_json::to_value(focus.specification.id)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let entries = [
        candidate("task", &id, "duplicate"),
        candidate("task", &id, "duplicate"),
    ];
    let mut expected = Vec::new();
    for key in ["one", "two"] {
        let task = NativeTaskStore::create_task(
            ctx.snapshot(),
            &write,
            task_input(session.id, key),
            AuthoredReferencesField::Sequence(&entries),
            write_limits(),
        )
        .await
        .unwrap();
        let source = TaskReferenceRecord::Task(task.specification.id);
        let sid = serde_json::to_value(task.specification.id)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        expected.push(((0, sid), source, vec![1, 2]));
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input(key),
            AuthoredReferencesField::Sequence(&entries),
            write_limits(),
        )
        .await
        .unwrap();
        let source = TaskReferenceRecord::Session(session.id);
        let sid = serde_json::to_value(session.id)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        expected.push(((1, sid), source, vec![0, 1]));
    }
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    let expected = expected
        .into_iter()
        .flat_map(|(_, s, ordinals)| ordinals.into_iter().map(move |ordinal| (s, ordinal)))
        .collect::<Vec<_>>();
    let mut actual = Vec::new();
    let mut q = query(TaskReferenceRecord::Task(focus.specification.id), 2);
    let mut pin = None;
    loop {
        let (g, _, incoming, _) = pages(
            NativeTaskStore::references(ctx.snapshot(), &read, q.clone(), limits())
                .await
                .unwrap(),
        );
        if let Some(pin) = &pin {
            assert_eq!(&g, pin)
        } else {
            pin = Some(g.clone())
        };
        assert_eq!(incoming.total, 8);
        assert!(incoming.complete);
        for e in incoming.results {
            assert_eq!(e.source_snapshot, g);
            actual.push((e.source, e.ordinal));
        }
        if let Some(cursor) = incoming.next_cursor {
            q.incoming_cursor = Some(cursor)
        } else {
            break;
        }
    }
    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_actual_expired_original_denies_before_read_and_cursor_root_and_focus_are_bound()
{
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    let issuer = f.issuer(&ctx);
    let (bearer, registered_reference, _run) = f.issue(&issuer, &ctx);
    let grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &registered_reference,
            f.scope(),
            &ctx,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let entries = (0..21)
        .map(|_| candidate("memory", "id", "supports"))
        .collect::<Vec<_>>();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("binding"),
        AuthoredReferencesField::Sequence(&entries),
        write_limits(),
    )
    .await
    .unwrap();
    let focus = TaskReferenceRecord::Session(session.id);
    let (_, page, _, _) = pages(
        NativeTaskStore::references(ctx.snapshot(), &read, query(focus, 20), limits())
            .await
            .unwrap(),
    );
    let cursor = page.next_cursor.unwrap();
    for kind in 0..4 {
        let mut encoded = serde_json::to_value(&cursor).unwrap();
        match kind {
            0 => {
                encoded["binding"]["root"] = serde_json::to_value(
                    graphit_core::native_path::NativePath::new(f.root.join("other")).unwrap(),
                )
                .unwrap()
            }
            1 => encoded["binding"]["project_version"] = serde_json::json!("other"),
            2 => {
                encoded["binding"]["focus"] = serde_json::to_value(TaskReferenceRecord::Session(
                    SessionId::new(NonZeroU128::new(1).unwrap()),
                ))
                .unwrap()
            }
            _ => encoded["binding"]["pin"]["physical_branch"] = serde_json::json!("other"),
        };
        let altered = serde_json::from_value(encoded).unwrap();
        let mut q = query(focus, 20);
        q.outgoing_cursor = Some(altered);
        assert!(matches!(
            NativeTaskStore::references(ctx.snapshot(), &read, q, limits()).await,
            Err(TaskStoreError::InvalidCursor)
        ));
    }
    let short_cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(1);
    let short = request_until(&short_cancel, deadline);
    let (bearer, _run) = f.issue_registered(&issuer, &short, &registered_reference);
    let short_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &registered_reference,
            f.scope(),
            &short,
        )
        .unwrap();
    let short_read = authority
        .admit_read(&short_grant, &short, ProjectReadModule::Task)
        .unwrap();
    tokio::time::sleep_until(deadline.into()).await;
    assert!(
        NativeTaskStore::references(short.snapshot(), &short_read, query(focus, 20), limits())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn references_competing_creation_and_fenced_writes_keep_one_head_audit_and_marker() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let one = [candidate("memory", "one", "supports")];
    let two = [candidate("memory", "two", "supports")];
    let field_one = AuthoredReferencesField::Sequence(&one);
    let field_two = AuthoredReferencesField::Sequence(&two);
    let input = session_input("race");
    let (a, b) = tokio::join!(
        NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            input.clone(),
            field_one,
            write_limits()
        ),
        NativeTaskStore::create_session(ctx.snapshot(), &write, input, field_two, write_limits())
    );
    let session = match (a, b) {
        (Ok(winner), Err(TaskStoreError::IdempotencyConflict))
        | (Err(TaskStoreError::IdempotencyConflict), Ok(winner)) => winner,
        _ => panic!("one creation only"),
    };
    let (g, page, _, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Session(session.id),
    )
    .await;
    assert_eq!(page.results.len(), 1);
    assert_eq!(page.results[0].reference, session.references[0]);
    let pin = super::lexical_index::pin(&f).await;
    use futures::TryStreamExt;
    let batches: Vec<_> = pin
        .scan()
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 3);
    assert_eq!(pin.version(), g.version);
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "task"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let own = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        task.specification.id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let current = claim.record;
    let update = || ProgressUpdate {
        claim: owner_input(&claim.token, &current),
        lease: Duration::from_secs(120),
        summary: "Race".into(),
        next_step: "Observe actual winner".into(),
    };
    let (a, b) = tokio::join!(
        NativeTaskStore::progress_task(
            ctx.snapshot(),
            &own,
            current.specification.id,
            update(),
            field_one,
            write_limits()
        ),
        NativeTaskStore::progress_task(
            ctx.snapshot(),
            &own,
            current.specification.id,
            update(),
            field_two,
            write_limits()
        )
    );
    let winner = match (a, b) {
        (Ok(winner), Err(TaskStoreError::RevisionConflict))
        | (Err(TaskStoreError::RevisionConflict), Ok(winner)) => winner,
        _ => panic!("one fenced write only"),
    };
    assert_eq!(
        winner.specification.revision,
        current.specification.revision + 1
    );
    let (_, page, _, _) = refs(
        ctx.snapshot(),
        &read,
        TaskReferenceRecord::Task(winner.specification.id),
    )
    .await;
    assert_eq!(page.total, 2);
    assert_eq!(page.results[1].reference, winner.references[0]);
    assert_eq!(
        page.results[1].source_revision,
        winner.specification.revision
    );
    let detail = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::TaskGetInput::current(winner.specification.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(detail.last_event.as_ref().unwrap().record, winner);
    assert_eq!(
        detail.last_event.as_ref().unwrap().references_update,
        TaskReferenceUpdate::Replace(winner.references.clone())
    );
    let pin = super::lexical_index::pin(&f).await;
    let mut scan = pin.scan();
    scan.filter("kind = 'reference_projection'").unwrap();
    let batches: Vec<_> = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        batches.iter().map(|b| b.num_rows()).sum::<usize>() as u64,
        winner.specification.revision + session.revision
    );
}
