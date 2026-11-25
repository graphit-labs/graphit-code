use super::*;
use graphit_records::{reference_projection::AuthoredReferencesField, task_spec::FieldUpdate};

fn page(cursor: Option<SessionDetailCursor>, size: usize) -> SessionDetailPageInput {
    SessionDetailPageInput {
        page_size: NonZeroUsize::new(size).unwrap(),
        cursor,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_and_history_keep_the_captured_session_with_replaceable_page_seams() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let coordinate = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let absent = SessionId::new(NonZeroU128::new(1).unwrap());
    assert!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            SessionGetInput::current(absent),
            read_limits()
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(!f.project.runtime_root().join("task").exists());
    let mut input = session_input("selected-session");
    input.description = "A durable demand preserved at every observed transition. "
        .repeat(150)
        .trim_end()
        .to_owned();
    let created = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        input,
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let initial = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        SessionGetInput::current(created.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(initial.session, created);
    assert_eq!(initial.creation.record, created);
    assert!(initial.last_event.is_none() && initial.history.is_none());
    let empty = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        SessionGetInput {
            id: created.id,
            history: Some(page(Some(initial.history_start_cursor.clone()), 1)),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    let empty = empty.history.unwrap();
    assert_eq!(empty.before, created);
    assert!(empty.events.is_empty() && empty.next_cursor.is_none());
    let claim = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        created.id,
        Duration::from_secs(300),
        write_limits(),
    )
    .await
    .unwrap();
    let mut current = claim.record;
    for index in 0..5 {
        current = NativeTaskStore::checkpoint_session(
            ctx.snapshot(),
            &coordinate,
            created.id,
            ProgressUpdate {
                claim: ClaimUpdate {
                    token: &claim.token,
                    expected_revision: current.revision,
                },
                lease: Duration::from_secs(300),
                summary: format!("Confirmed checkpoint {index}"),
                next_step: "Continue the admitted work".into(),
            },
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    current = NativeTaskStore::revise_session(
        ctx.snapshot(),
        &coordinate,
        created.id,
        SessionRevisionUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: current.revision,
            },
            lease: Duration::from_secs(300),
            reason: "Refine the admitted demand",
            title: FieldUpdate::Set("Revised Session"),
            description: FieldUpdate::Keep,
            strategy: FieldUpdate::Keep,
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let captured = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        SessionGetInput::current(created.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(captured.session, current);
    assert_eq!(captured.last_event.as_ref().unwrap().record, current);
    assert!(captured.history.is_none());
    let audit = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::SessionGetInput {
            id: created.id,
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
    assert!(audit.history.as_ref().unwrap().next_cursor.is_none());
    assert!(
        serde_json::to_vec(&audit).unwrap().len() > serde_json::to_vec(&captured).unwrap().len()
    );
    NativeTaskStore::checkpoint_session(
        ctx.snapshot(),
        &coordinate,
        created.id,
        ProgressUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: current.revision,
            },
            lease: Duration::from_secs(300),
            summary: "A later publication".into(),
            next_step: "Keep the old page pin".into(),
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let latest = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        SessionGetInput::current(created.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(latest.session.revision > captured.session.revision);
    let mut next = Some(captured.history_start_cursor.clone());
    let mut before = created;
    let mut events = Vec::new();
    while let Some(cursor) = next {
        let detail = NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            SessionGetInput {
                id: current.id,
                history: Some(page(Some(cursor), 2)),
            },
            read_limits(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(detail.session, captured.session);
        assert_eq!(detail.generation, captured.generation);
        let history = detail.history.unwrap();
        assert_eq!(history.before, before);
        assert!(!history.events.is_empty());
        before = history.events.last().unwrap().record.clone();
        events.extend(history.events);
        next = history.next_cursor;
    }
    assert_eq!(before, captured.session);
    assert_eq!(events, audit.history.as_ref().unwrap().events);
    let other = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("other-session"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            SessionGetInput {
                id: other.id,
                history: Some(page(Some(captured.history_start_cursor.clone()), 1))
            },
            read_limits()
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
    let task = NativeTaskStore::create_task(
        ctx.snapshot(),
        &write,
        task_input(current.id, "cursor-kind"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let task = NativeTaskStore::read_task(
        ctx.snapshot(),
        &read,
        TaskGetInput::current(task.specification.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        serde_json::from_value::<SessionDetailCursor>(
            serde_json::to_value(task.history_start_cursor).unwrap()
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<TaskDetailCursor>(
            serde_json::to_value(captured.history_start_cursor.clone()).unwrap()
        )
        .is_err()
    );
    {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let encoded = serde_json::to_value(&captured.history_start_cursor).unwrap();
        let text = encoded.as_str().unwrap();
        assert!(
            serde_json::from_value::<SessionDetailCursor>(serde_json::json!(format!("{text}=")))
                .is_err()
        );
        let mut fields: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(text).unwrap()).unwrap();
        fields["binding"]["generation"]["extra"] = serde_json::json!(true);
        let changed = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&fields).unwrap());
        assert!(serde_json::from_value::<SessionDetailCursor>(serde_json::json!(changed)).is_err());
    }
    cancel.cancel();
    assert!(
        matches!(NativeTaskStore::read_session(ctx.snapshot(), &read, SessionGetInput::current(current.id), read_limits()).await, Err(TaskStoreError::Request(error)) if error.kind()==std::io::ErrorKind::Interrupted)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inherited_session_history_survives_parent_writes_and_child_materialization() {
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
    let coordinate = authority
        .admit_write(&main_grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let main_read = authority
        .admit_read(&main_grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let created = NativeTaskStore::create_session(
        ctx.snapshot(),
        &main_write,
        session_input("inherited-session-page"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        created.id,
        Duration::from_secs(300),
        write_limits(),
    )
    .await
    .unwrap();
    let captured = NativeTaskStore::read_session(
        ctx.snapshot(),
        &feature_read,
        SessionGetInput::current(created.id),
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
    NativeTaskStore::checkpoint_session(
        ctx.snapshot(),
        &coordinate,
        created.id,
        ProgressUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: claim.record.revision,
            },
            lease: Duration::from_secs(300),
            summary: "Parent advanced after capture".into(),
            next_step: "Materialize the child".into(),
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let continued = NativeTaskStore::read_session(
        ctx.snapshot(),
        &feature_read,
        SessionGetInput {
            id: created.id,
            history: Some(page(Some(captured.history_start_cursor.clone()), 1)),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(continued.session, captured.session);
    assert!(continued.history.unwrap().next_cursor.is_none());
    NativeTaskStore::create_session(
        ctx.snapshot(),
        &feature_write,
        session_input("materialize-session-child"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let promoted = NativeTaskStore::read_session(
        ctx.snapshot(),
        &feature_read,
        SessionGetInput {
            id: created.id,
            history: Some(page(Some(captured.history_start_cursor.clone()), 1)),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(promoted.session, captured.session);
    assert_eq!(promoted.generation, captured.generation);
    assert_ne!(promoted.history_start_cursor, captured.history_start_cursor);
    let promoted_again = NativeTaskStore::read_session(
        ctx.snapshot(),
        &feature_read,
        SessionGetInput {
            id: created.id,
            history: Some(page(Some(promoted.history_start_cursor.clone()), 1)),
        },
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        promoted_again.history_start_cursor,
        promoted.history_start_cursor
    );
    let child_now = NativeTaskStore::read_session(
        ctx.snapshot(),
        &feature_read,
        SessionGetInput::current(created.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        child_now.generation.physical_branch.as_deref(),
        Some("feature")
    );
    assert!(matches!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &main_read,
            SessionGetInput {
                id: created.id,
                history: Some(page(Some(promoted.history_start_cursor.clone()), 1))
            },
            read_limits()
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
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
    for cursor in [
        child_now.history_start_cursor,
        promoted.history_start_cursor,
    ] {
        assert!(
            NativeTaskStore::read_session(
                ctx.snapshot(),
                &feature_read,
                SessionGetInput {
                    id: created.id,
                    history: Some(page(Some(cursor), 1))
                },
                read_limits()
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_skips_old_corruption_but_selected_pages_and_last_transition_reject_it() {
    use arrow_array::{RecordBatch, StringArray};
    use futures::TryStreamExt;
    use graphit_data::lance_branch_store::LanceBranchStore;
    async fn merge(uri: &str, batches: Vec<RecordBatch>) {
        LanceBranchStore::open(uri, batches[0].schema(), None)
            .await
            .unwrap()
            .capture_target(None)
            .await
            .unwrap()
            .prepare_merge(batches, vec!["row_key".into()])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
    }
    async fn corrupt(
        uri: &str,
        revision: u64,
        alter: fn(&mut serde_json::Value),
    ) -> Vec<RecordBatch> {
        let dataset = lance::Dataset::open(uri).await.unwrap();
        let mut scan = dataset.scan();
        scan.filter(&format!("kind = 'session_event' AND revision = {revision}"))
            .unwrap();
        let saved: Vec<RecordBatch> = scan
            .try_into_stream()
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();
        assert_eq!(saved.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        let bad = saved
            .iter()
            .map(|batch| {
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
                        alter(&mut value);
                        serde_json::to_string(&value).unwrap()
                    })
                    .collect::<Vec<_>>();
                let mut columns = batch.columns().to_vec();
                columns[batch.schema().index_of("payload").unwrap()] =
                    Arc::new(StringArray::from(strings));
                RecordBatch::try_new(batch.schema(), columns).unwrap()
            })
            .collect::<Vec<_>>();
        merge(uri, bad).await;
        saved
    }
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let coordinate = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let created = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("session-corruption"),
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let claim = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        created.id,
        Duration::from_secs(300),
        write_limits(),
    )
    .await
    .unwrap();
    let mut current = claim.record;
    for index in 0..3 {
        current = NativeTaskStore::checkpoint_session(
            ctx.snapshot(),
            &coordinate,
            created.id,
            ProgressUpdate {
                claim: ClaimUpdate {
                    token: &claim.token,
                    expected_revision: current.revision,
                },
                lease: Duration::from_secs(300),
                summary: format!("Checkpoint {index}"),
                next_step: "Verify bounded audit".into(),
            },
            AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
    }
    let uri = f.project.runtime_root().join("task/index.lance");
    let uri = uri.to_str().unwrap();
    let saved = corrupt(uri, 2, |value| {
        value["record"]["record"]["input"]["description"] =
            serde_json::json!("Unexpected specification change");
    })
    .await;
    let head = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        SessionGetInput::current(created.id),
        read_limits(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(head.session, current);
    assert!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            SessionGetInput {
                id: created.id,
                history: Some(page(Some(head.history_start_cursor), 1))
            },
            read_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::SessionGetInput {
                id: created.id,
                history: Some(graphit_records::task_store::SessionDetailPageInput {
                    page_size: std::num::NonZeroUsize::new(64).unwrap(),
                    cursor: None
                })
            },
            read_limits()
        )
        .await
        .is_err()
    );
    merge(uri, saved).await;
    let revised = NativeTaskStore::revise_session(
        ctx.snapshot(),
        &coordinate,
        created.id,
        SessionRevisionUpdate {
            claim: ClaimUpdate {
                token: &claim.token,
                expected_revision: current.revision,
            },
            lease: Duration::from_secs(300),
            reason: "Refine the selected demand",
            title: FieldUpdate::Set("Revised corruption Session"),
            description: FieldUpdate::Keep,
            strategy: FieldUpdate::Keep,
        },
        AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            SessionGetInput::current(created.id),
            read_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .session,
        revised
    );
    corrupt(uri, revised.revision, |value| {
        let first = ulid::Ulid::from(10u128).to_string();
        let other = if value["record"]["actor"].as_str() == Some(first.as_str()) {
            ulid::Ulid::from(11u128).to_string()
        } else {
            first
        };
        value["record"]["actor"] = serde_json::json!(other);
    })
    .await;
    assert!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            SessionGetInput::current(created.id),
            read_limits()
        )
        .await
        .is_err()
    );
    assert!(
        NativeTaskStore::read_session(
            ctx.snapshot(),
            &read,
            graphit_records::task_store::SessionGetInput {
                id: created.id,
                history: Some(graphit_records::task_store::SessionDetailPageInput {
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
