use super::*;
use graphit_records::task_comment::TaskCommentKind;

fn page(cursor: Option<TaskDetailCursor>, size: usize) -> TaskDetailPageInput {
    TaskDetailPageInput {
        page_size: NonZeroUsize::new(size).unwrap(),
        cursor,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inherited_detail_continues_after_parent_writes_and_child_materialization() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "feature");
    admissions!(
        f,
        ctx,
        issuer,
        authority,
        grant,
        feature_read,
        feature_write
    );
    let install = graphit_core::project_paths::InstallationPaths::resolve(
        f.installation.brand().clone(),
        &f.root,
        None,
        Some(f.installation.owner_root()),
    )
    .unwrap();
    let paths = graphit_core::project_paths::ProjectPaths::at(&f.root, &install).unwrap();
    let main = graphit_core::version_context::VersionContext::select(
        &paths,
        graphit_core::version_context::Checkout::NonGit,
        "main",
        None,
        "",
    )
    .unwrap();
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().version = &main;
    let (bearer, reference, _run) = f.issue_for(&issuer, &ctx, scope);
    let main_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let main_write = authority
        .admit_write(&main_grant, &ctx, ProjectWriteOperation::Task)
        .unwrap();
    let main_read = authority
        .admit_read(&main_grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &main_write,
        session_input("inherited-detail"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let created = NativeTaskStore::create_task(
        ctx.snapshot(),
        &main_write,
        task_input(session.id, "inherited-detail"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let id = created.specification.id;
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &main_write,
        id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let captured = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(captured.generation.physical_branch.is_none());
    assert_eq!(
        captured.generation.inherited_target.as_deref(),
        Some("feature")
    );
    let next = NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &main_write,
        id,
        TaskCommentUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: claim.record.specification.revision,
            },
            lease: Duration::from_secs(120),
            idempotency_key: "parent-before-fork".into(),
            kind: TaskCommentKind::Note,
            body: "Parent before materialization".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let TaskCommentOutcome::Created {
        task: parent_at_fork,
        ..
    } = next
    else {
        panic!("created")
    };
    let continued = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput {
            id,
            history: Some(page(Some(captured.history_start_cursor.clone()), 1)),
            comments: None,
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(continued.task, captured.task);
    let fresh_inherited = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(fresh_inherited.generation.physical_branch.is_none());
    assert_eq!(
        fresh_inherited.generation.inherited_target.as_deref(),
        Some("feature")
    );
    assert_eq!(fresh_inherited.task, parent_at_fork);
    // A real child write materializes its own branch, never the parent.
    NativeTaskStore::create_task(
        ctx.snapshot(),
        &feature_write,
        task_input(session.id, "child-only"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let continued = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput {
            id,
            history: Some(page(Some(captured.history_start_cursor.clone()), 1)),
            comments: None,
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(continued.task, captured.task);
    assert_eq!(continued.generation, captured.generation);
    // Both sections can continue together while the absent-child binding gains
    // SDK provenance. Every cursor returned after materialization carries it.
    let promoted = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput {
            id,
            history: Some(page(Some(captured.history_start_cursor.clone()), 1)),
            comments: Some(page(Some(captured.comments_start_cursor.clone()), 1)),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let encoded = serde_json::to_value(&promoted.history_start_cursor).unwrap();
        let fields: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded.as_str().unwrap()).unwrap())
                .unwrap();
        assert!(fields["binding"]["branch_provenance"].is_object());
        let mut unknown = fields;
        unknown["binding"]["branch_provenance"]["extra"] = serde_json::json!(true);
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&unknown).unwrap());
        assert!(serde_json::from_value::<TaskDetailCursor>(serde_json::json!(encoded)).is_err());
        assert_ne!(promoted.history_start_cursor, captured.history_start_cursor);
    }
    let promoted_again = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput {
            id,
            history: Some(page(Some(promoted.history_start_cursor.clone()), 1)),
            comments: Some(page(Some(promoted.comments_start_cursor.clone()), 1)),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(promoted_again.task, captured.task);
    let child_now = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(child_now.task, parent_at_fork);
    NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &main_write,
        id,
        TaskCommentUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: parent_at_fork.specification.revision,
            },
            lease: Duration::from_secs(120),
            idempotency_key: "parent-after-fork".into(),
            kind: TaskCommentKind::Note,
            body: "Not part of the child lineage".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let parent_now = NativeTaskStore::read_task(
        ctx.snapshot(),
        &main_read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let encoded = serde_json::to_value(&captured.history_start_cursor).unwrap();
        let mut fields: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded.as_str().unwrap()).unwrap())
                .unwrap();
        fields["binding"]["generation"]["version"] =
            serde_json::json!(parent_now.generation.version);
        let forged: TaskDetailCursor = serde_json::from_value(serde_json::json!(
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&fields).unwrap())
        ))
        .unwrap();
        assert!(
            NativeTaskStore::read_task(
                ctx.snapshot(),
                &feature_read,
                TaskGetInput {
                    id,
                    history: Some(page(Some(forged), 1)),
                    comments: None,
                },
                read_limits()
            )
            .await
            .is_err()
        );
    }
    let child_after = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(child_after.task, parent_at_fork);
    // A child-bound cursor cannot be presented under the parent's admission.
    assert!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &main_read,
            TaskGetInput {
                id,
                history: Some(page(Some(child_after.history_start_cursor.clone()), 1)),
                comments: None,
            },
            read_limits()
        )
        .await
        .is_err()
    );
    // Reusing the same named branch/version must not admit its old namespace
    // identity, even though the current authorized branch itself is readable.
    let uri = f.project.runtime_root().join("task/index.lance");
    let uri = uri.to_str().unwrap();
    let old = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        uri,
        Some("feature"),
        None,
    )
    .await
    .unwrap();
    old.retire_named().await.unwrap();
    graphit_data::lance_branch_store::BranchSnapshot::capture_observed(uri, Some("feature"), None)
        .await
        .unwrap()
        .materialize()
        .await
        .unwrap();
    let recreated = NativeTaskStore::read_task(
        ctx.snapshot(),
        &feature_read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(recreated.generation, child_after.generation);
    assert_eq!(recreated.task, parent_now.task);
    for cursor in [
        child_after.history_start_cursor,
        promoted.history_start_cursor,
    ] {
        assert!(matches!(
            NativeTaskStore::read_task(
                ctx.snapshot(),
                &feature_read,
                TaskGetInput {
                    id,
                    history: Some(page(Some(cursor), 1)),
                    comments: None
                },
                read_limits(),
            )
            .await,
            Err(TaskStoreError::InvalidCursor)
        ));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn old_corruption_is_checked_by_its_page_and_explicit_audit_not_every_current_read() {
    use arrow_array::{RecordBatch, StringArray};
    use futures::TryStreamExt;
    use graphit_data::lance_branch_store::LanceBranchStore;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("old-corruption"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let created = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(session.id, "old-corruption"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let id = created.specification.id;
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let mut current = claim.record;
    for n in 0..3 {
        let outcome = NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            TaskCommentUpdate {
                claim: ClaimUpdate {
                    token: &claim.token,
                    expected_revision: current.specification.revision,
                },
                lease: Duration::from_secs(120),
                idempotency_key: format!("old-{n}"),
                kind: TaskCommentKind::Note,
                body: format!("Recorded comment {n}"),
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let TaskCommentOutcome::Created { task, .. } = outcome else {
            panic!("created")
        };
        current = task;
    }
    let uri = f.project.runtime_root().join("task/index.lance");
    let uri = uri.to_str().unwrap();
    let dataset = lance::Dataset::open(uri).await.unwrap();
    let mut scan = dataset.scan();
    scan.filter("kind = 'task_event' AND revision = 2").unwrap();
    let saved: Vec<RecordBatch> = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    let mut bad = Vec::new();
    for batch in &saved {
        let payload = batch
            .column_by_name("payload")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let strings = (0..batch.num_rows())
            .map(|row| {
                let mut value: serde_json::Value =
                    serde_json::from_str(payload.value(row)).unwrap();
                value["record"]["record"]["reference_generation"] = serde_json::json!(999);
                serde_json::to_string(&value).unwrap()
            })
            .collect::<Vec<_>>();
        let mut columns = batch.columns().to_vec();
        columns[batch.schema().index_of("payload").unwrap()] = Arc::new(StringArray::from(strings));
        bad.push(RecordBatch::try_new(batch.schema(), columns).unwrap());
    }
    let store = LanceBranchStore::open(uri, saved[0].schema(), None)
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
    let head = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(head.task, current);
    assert!(
        NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            TaskGetInput {
                id,
                history: Some(page(Some(head.history_start_cursor), 1)),
                comments: None,
            },
            read_limits()
        )
        .await
        .is_err()
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
async fn current_and_replaceable_pages_keep_their_authorized_snapshot_after_writes() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("detail-pages"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut input = task_input(session.id, "detail-pages");
    input.description = "A literal specification preserved across each revision. "
        .repeat(200)
        .trim_end()
        .to_owned();
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        input,
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let id = task.specification.id;
    let initial = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(initial.task, task);
    assert!(
        initial.last_event.is_none() && initial.history.is_none() && initial.comments.is_none()
    );
    let empty = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        TaskGetInput {
            id,
            history: Some(page(Some(initial.history_start_cursor), 2)),
            comments: Some(page(Some(initial.comments_start_cursor), 1)),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(empty.history.unwrap().events.is_empty());
    assert!(empty.comments.unwrap().comments.is_empty());
    let claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &write,
        id,
        Duration::from_secs(120),
        write_limits(),
    )
    .await
    .unwrap();
    let mut current = claim.record;
    let mut expected_comments = Vec::new();
    for n in 0..3 {
        let outcome = NativeTaskStore::add_task_comment(
            ctx.snapshot(),
            &write,
            id,
            TaskCommentUpdate {
                claim: ClaimUpdate {
                    token: &claim.token,
                    expected_revision: current.specification.revision,
                },
                lease: Duration::from_secs(120),
                idempotency_key: format!("detail-{n}"),
                kind: TaskCommentKind::Note,
                body: format!("literal **comment** {n}"),
            },
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        let TaskCommentOutcome::Created { task, comment } = outcome else {
            panic!("created")
        };
        current = task;
        expected_comments.push(comment);
    }
    let captured = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    // The opaque continuation remains closed inside its nested generation too.
    {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let encoded = serde_json::to_value(&captured.history_start_cursor).unwrap();
        let mut fields: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded.as_str().unwrap()).unwrap())
                .unwrap();
        fields["binding"]["generation"]["extra"] = serde_json::json!(true);
        let changed = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&fields).unwrap());
        assert!(
            serde_json::from_value::<TaskDetailCursor>(serde_json::Value::String(changed)).is_err()
        );
    }
    assert_eq!(captured.last_event.as_ref().unwrap().record, current);
    // A later real publication must not invalidate the captured detail cursors.
    NativeTaskStore::add_task_comment(
        ctx.snapshot(),
        &write,
        id,
        TaskCommentUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: current.specification.revision,
            },
            lease: Duration::from_secs(120),
            idempotency_key: "later".into(),
            kind: TaskCommentKind::Note,
            body: "Later publication".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let latest = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        TaskGetInput::current(id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(latest.task.specification.revision > captured.task.specification.revision);
    let mut cursor = Some(captured.history_start_cursor.clone());
    let mut revisions = Vec::new();
    let mut before = 1;
    while let Some(next) = cursor {
        let detail = NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            TaskGetInput {
                id,
                history: Some(page(Some(next), 2)),
                comments: None,
            },
            read_limits(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(detail.task, captured.task);
        assert_eq!(detail.generation, captured.generation);
        assert!(detail.comments.is_none());
        let history = detail.history.unwrap();
        assert_eq!(history.before.specification.revision, before);
        for event in &history.events {
            before += 1;
            assert_eq!(event.record.specification.revision, before);
            revisions.push(before);
        }
        cursor = history.next_cursor;
    }
    assert_eq!(
        revisions,
        (2..=captured.task.specification.revision).collect::<Vec<_>>()
    );
    let mut cursor = Some(captured.comments_start_cursor.clone());
    let mut comments = Vec::new();
    while let Some(next) = cursor {
        let detail = NativeTaskStore::read_task(
            ctx.snapshot(),
            &read,
            TaskGetInput {
                id,
                history: None,
                comments: Some(page(Some(next), 1)),
            },
            read_limits(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(detail.generation, captured.generation);
        let selected = detail.comments.unwrap();
        comments.extend(selected.comments);
        cursor = selected.next_cursor;
    }
    assert_eq!(comments, expected_comments);
    let wrong_section = TaskGetInput {
        id,
        history: Some(page(Some(captured.comments_start_cursor.clone()), 1)),
        comments: None,
    };
    assert!(matches!(
        NativeTaskStore::read_task(ctx.snapshot(), &read, wrong_section, read_limits()).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let mixed = TaskGetInput {
        id,
        history: Some(page(Some(captured.history_start_cursor.clone()), 1)),
        comments: Some(page(Some(latest.comments_start_cursor.clone()), 1)),
    };
    assert!(matches!(
        NativeTaskStore::read_task(ctx.snapshot(), &read, mixed, read_limits()).await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let audit = NativeTaskStore::read_task(
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
    assert!(audit.history.as_ref().unwrap().next_cursor.is_none());
    assert!(audit.comments.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(audit.comments.as_ref().unwrap().comments.len(), 4);
    assert_eq!(
        audit
            .history
            .as_ref()
            .unwrap()
            .events
            .last()
            .unwrap()
            .record,
        latest.task
    );
    assert!(serde_json::to_vec(&audit).unwrap().len() > 65536);
    assert!(serde_json::to_vec(&latest).unwrap().len() < 65536);
    cancel.cancel();
    assert!(
        matches!(NativeTaskStore::read_task(ctx.snapshot(), &read, TaskGetInput::current(id), read_limits()).await, Err(TaskStoreError::Request(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
}
