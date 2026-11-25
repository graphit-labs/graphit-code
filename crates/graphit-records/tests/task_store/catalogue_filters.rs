use super::*;
use graphit_records::gates::TaskStatus;

#[test]
fn boolean_catalogue_filters_are_typed_and_optional() {
    let empty: CatalogueFilter = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, CatalogueFilter::default());
    let values: CatalogueFilter =
        serde_json::from_str(r#"{"flagged":false,"blocked":true}"#).unwrap();
    assert_eq!(values.flagged, Some(false));
    assert_eq!(values.blocked, Some(true));
    assert_eq!(
        serde_json::from_str::<CatalogueFilter>(r#"{"flagged":null,"blocked":null}"#).unwrap(),
        empty
    );
    assert!(serde_json::from_str::<CatalogueFilter>(r#"{"blocked":"true"}"#).is_err());
    assert!(serde_json::from_str::<CatalogueFilter>(r#"{"flagged":1}"#).is_err());
    assert!(serde_json::from_str::<CatalogueFilter>(r#"{"unknown":true}"#).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn global_boolean_filters_precede_paging_and_bind_cursor() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(f, ctx, issuer, authority, grant, read, write);
    let own = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::OwnTask)
        .unwrap();
    let session = NativeTaskStore::create_session(
        ctx.snapshot(),
        &write,
        session_input("filters"),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let mut records: Vec<Task> = Vec::new();
    for key in [
        "base",
        "flagged",
        "dependent",
        "completed",
        "cancelled",
        "active",
    ] {
        let mut input = task_input(session.id, key);
        if key == "dependent" {
            input.depends_on.push(records[0].specification.id);
        }
        records.push(
            NativeTaskStore::create_task(
                ctx.snapshot(),
                &write,
                input,
                graphit_records::reference_projection::AuthoredReferencesField::Omitted,
                write_limits(),
            )
            .await
            .unwrap(),
        );
    }
    let base = records[0].clone();
    let flagged = &records[1];
    let claimed = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        flagged.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let flagged = NativeTaskStore::set_task_flag(
        ctx.snapshot(),
        &own,
        flagged.specification.id,
        TaskFlagUpdate {
            claim: owner_input(&claimed.token, &claimed.record),
            lease: Duration::from_secs(60),
            flagged: true,
            reason: "Follow up",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let flagged = NativeTaskStore::update_task_links(
        ctx.snapshot(),
        &own,
        flagged.specification.id,
        TaskLinksUpdate {
            claim: owner_input(&claimed.token, &flagged),
            lease: Duration::from_secs(60),
            parent_id: graphit_records::task_spec::FieldUpdate::Keep,
            depends_on: Some(&[base.specification.id]),
            reason: "Needs base",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let flagged = NativeTaskStore::release_task(
        ctx.snapshot(),
        &own,
        flagged.specification.id,
        ReleaseUpdate {
            claim: owner_input(&claimed.token, &flagged),
            summary: "Await base".into(),
            next_step: "Finish base".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let complete_claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        records[3].specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let checked = pass_task_checks(
        ctx.snapshot(),
        &own,
        &complete_claim.token,
        complete_claim.record,
    )
    .await;
    let completed = NativeTaskStore::complete_task(
        ctx.snapshot(),
        &own,
        checked.specification.id,
        owner_input(&complete_claim.token, &checked),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let cancelled = NativeTaskStore::cancel_task(
        ctx.snapshot(),
        &write,
        records[4].specification.id,
        TaskCancellation {
            claim: None,
            reason: "No longer needed",
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let active = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        records[5].specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert_eq!(active.record.status, TaskStatus::InProgress);

    let first = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        CatalogueFilter::default(),
        NonZeroUsize::new(1).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(first.results[0].id, active.record.specification.id);
    let blocked = CatalogueFilter {
        blocked: Some(true),
        ..Default::default()
    };
    let page = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        blocked.clone(),
        NonZeroUsize::new(1).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert!(page.results[0].status == TaskStatus::Open && !page.results[0].ready);
    let cursor = page.next_cursor.as_ref().unwrap();
    let rest = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        blocked.clone(),
        NonZeroUsize::new(1).unwrap(),
        Some(cursor),
    )
    .await
    .unwrap();
    let ids = [page.results[0].id, rest.results[0].id];
    assert!(ids.contains(&flagged.specification.id));
    assert!(ids.contains(&records[2].specification.id));
    assert!(rest.next_cursor.is_none());
    for filter in [
        CatalogueFilter {
            blocked: Some(false),
            ..Default::default()
        },
        CatalogueFilter {
            flagged: Some(true),
            ..blocked.clone()
        },
    ] {
        assert!(matches!(
            NativeTaskStore::catalogue(
                ctx.snapshot(),
                &read,
                read_limits(),
                filter,
                NonZeroUsize::new(1).unwrap(),
                Some(cursor)
            )
            .await,
            Err(TaskStoreError::InvalidCursor)
        ));
    }
    let flagged_blocked = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        CatalogueFilter {
            flagged: Some(true),
            blocked: Some(true),
            query: Some("flagged".into()),
            status: Some(TaskStatus::Open),
            session_id: Some(session.id),
        },
        NonZeroUsize::new(20).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(flagged_blocked.results.len(), 1);
    assert_eq!(flagged_blocked.results[0].id, flagged.specification.id);
    let not_flagged = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        CatalogueFilter {
            flagged: Some(false),
            blocked: Some(true),
            ..Default::default()
        },
        NonZeroUsize::new(20).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(not_flagged.results.len(), 1);
    assert_eq!(not_flagged.results[0].id, records[2].specification.id);
    let not_blocked = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        CatalogueFilter {
            blocked: Some(false),
            ..Default::default()
        },
        NonZeroUsize::new(20).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(not_blocked.results.len(), 4);
    assert!(
        not_blocked
            .results
            .iter()
            .all(|t| t.status != TaskStatus::Open || t.ready)
    );
    assert!(
        not_blocked
            .results
            .iter()
            .any(|t| t.id == completed.specification.id)
    );
    assert!(
        not_blocked
            .results
            .iter()
            .any(|t| t.id == cancelled.specification.id)
    );
    assert!(
        not_blocked
            .results
            .iter()
            .any(|t| t.id == active.record.specification.id)
    );
    let impossible = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        CatalogueFilter {
            blocked: Some(true),
            status: Some(TaskStatus::Completed),
            ..Default::default()
        },
        NonZeroUsize::new(20).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert!(impossible.results.is_empty());

    // The complete view is an independent oracle in this fixture, never a catalogue adapter.
    let TaskReadView::Published {
        tasks: expected_heads,
        ..
    } = NativeTaskStore::read_view(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("published fixture")
    };
    let mut small_batches = read_limits();
    small_batches.batch_rows = std::num::NonZeroU32::new(1).unwrap();
    let mut paged = Vec::new();
    let mut continuation = None;
    for size in [1, 3, 2] {
        let page = NativeTaskStore::catalogue(
            ctx.snapshot(),
            &read,
            small_batches,
            CatalogueFilter::default(),
            NonZeroUsize::new(size).unwrap(),
            continuation.as_ref(),
        )
        .await
        .unwrap();
        assert!(page.results.len() <= size);
        paged.extend(page.results);
        continuation = page
            .next_cursor
            .map(|cursor| serde_json::from_value(serde_json::to_value(cursor).unwrap()).unwrap());
    }
    assert_eq!(paged, expected_heads);
    assert!(continuation.is_none());
    let unbounded_representable_page = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        small_batches,
        CatalogueFilter::default(),
        NonZeroUsize::new(usize::MAX).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(unbounded_representable_page.results, expected_heads);
    assert!(unbounded_representable_page.next_cursor.is_none());

    NativeTaskStore::release_task(
        ctx.snapshot(),
        &own,
        active.record.specification.id,
        ReleaseUpdate {
            claim: owner_input(&active.token, &active.record),
            summary: "Pause independent work".into(),
            next_step: "Finish base".into(),
        },
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let base_claim = NativeTaskStore::claim_task(
        ctx.snapshot(),
        &own,
        base.specification.id,
        Duration::from_secs(60),
        write_limits(),
    )
    .await
    .unwrap();
    let base = pass_task_checks(ctx.snapshot(), &own, &base_claim.token, base_claim.record).await;
    NativeTaskStore::complete_task(
        ctx.snapshot(),
        &own,
        base.specification.id,
        owner_input(&base_claim.token, &base),
        graphit_records::reference_projection::AuthoredReferencesField::Omitted,
        write_limits(),
    )
    .await
    .unwrap();
    let after = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        blocked.clone(),
        NonZeroUsize::new(20).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert!(after.results.is_empty());
    let still_flagged = NativeTaskStore::catalogue(
        ctx.snapshot(),
        &read,
        read_limits(),
        CatalogueFilter {
            flagged: Some(true),
            blocked: Some(false),
            ..Default::default()
        },
        NonZeroUsize::new(20).unwrap(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(still_flagged.results.len(), 1);
    assert!(still_flagged.results[0].ready);
    assert!(matches!(
        NativeTaskStore::catalogue(
            ctx.snapshot(),
            &read,
            read_limits(),
            blocked,
            NonZeroUsize::new(1).unwrap(),
            Some(cursor)
        )
        .await,
        Err(TaskStoreError::InvalidCursor)
    ));
}
