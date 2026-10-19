use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_actor_coordinates_distinct_sessions_with_independent_fences() {
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
    let coordinate = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::CoordinateTask)
        .unwrap();
    let mut claims = Vec::new();
    for key in ["parallel-session-a", "parallel-session-b"] {
        let session = NativeTaskStore::create_session(
            ctx.snapshot(),
            &write,
            session_input(key),
            graphit_records::reference_projection::AuthoredReferencesField::Omitted,
            write_limits(),
        )
        .await
        .unwrap();
        claims.push(
            NativeTaskStore::claim_session(
                ctx.snapshot(),
                &coordinate,
                session.id,
                Duration::from_secs(30),
                write_limits(),
            )
            .await
            .unwrap(),
        );
    }
    assert_eq!(
        claims[0].record.ownership.owner,
        claims[1].record.ownership.owner
    );
    assert!(claims[0].token != claims[1].token);
    let first = &claims[0];
    let before = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::SessionGetInput {
            id: first.record.id,
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
    let repeated = NativeTaskStore::claim_session(
        ctx.snapshot(),
        &coordinate,
        first.record.id,
        Duration::from_secs(1),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(first.record, repeated.record);
    assert!(first.token == repeated.token);
    assert!(matches!(
        NativeTaskStore::heartbeat_session(
            ctx.snapshot(),
            &coordinate,
            first.record.id,
            ClaimUpdate {
                token: &claims[1].token,
                expected_revision: first.record.revision,
            },
            Duration::from_secs(30),
            write_limits(),
        )
        .await,
        Err(TaskStoreError::FenceMismatch)
    ));
    let after = NativeTaskStore::read_session(
        ctx.snapshot(),
        &read,
        graphit_records::task_store::SessionGetInput {
            id: first.record.id,
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
    assert!(after.history.as_ref().unwrap().next_cursor.is_none());
    assert_eq!(before, after);
    assert_eq!(after.history.as_ref().unwrap().events.len(), 1);
}
