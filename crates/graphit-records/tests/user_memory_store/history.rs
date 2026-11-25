use super::*;
use graphit_core::file_lock::Cancellation;
use graphit_records::reference_projection::ReferenceTargetKind;

#[tokio::test]
async fn revisions_keep_literal_audit_refs_generations_retry_before_cas_and_tombstone_history() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let owner = UserMemoryOwner {
        installation_id: graphit_core::installation_identity::InstallationId::new(
            "user-records-fixture",
        )
        .unwrap(),
        identity: graphit_core::artifact_store::UserIdentity::new(
            f.server.origin.clone(),
            SUBJECT.into(),
        )
        .unwrap(),
    };
    let scope = UserMemoryScopeId::from_owner(&owner).unwrap();
    let refs = vec![MemoryReference {
        target_kind: ReferenceTargetKind::Memory,
        field: Some("literal provenance".into()),
        scope: MemoryReferenceScope::User(scope.as_str().into()),
        id: "literal unresolved ' β".into(),
        context: None,
        relation: " supports ".into(),
    }];
    let mut create = input("history-create");
    create.references = Some(refs.clone());
    let first =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, create.clone(), write_limits())
            .await
            .unwrap();
    assert_eq!(first.references, refs);
    let edit = update("history-edit", 1);
    let second = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        first.id,
        edit.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(second.reference_generation, 1);
    assert_eq!(second.references, refs);
    let promote = ChangeMemoryImportance {
        expected_revision: 2,
        idempotency_key: "promote".into(),
        reason: " importance only ".into(),
    };
    let third = NativeUserMemoryStore::promote(
        ctx.snapshot(),
        &write,
        first.id,
        promote.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(third.attributes.important);
    assert!(third.attributes.mandatory);
    assert_eq!(third.reference_generation, 1);
    let before_noop = NativeUserMemoryStore::get(ctx.snapshot(), &read, first.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .generation;
    let noop = ChangeMemoryImportance {
        expected_revision: 3,
        idempotency_key: "no-op".into(),
        reason: "already important".into(),
    };
    assert_eq!(
        NativeUserMemoryStore::promote(ctx.snapshot(), &write, first.id, noop, write_limits())
            .await
            .unwrap(),
        third
    );
    assert_eq!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, first.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .generation,
        before_noop
    );
    let demote = ChangeMemoryImportance {
        expected_revision: 3,
        idempotency_key: "demote".into(),
        reason: "demote".into(),
    };
    let fourth = NativeUserMemoryStore::demote(
        ctx.snapshot(),
        &write,
        first.id,
        demote.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(!fourth.attributes.important);
    assert!(fourth.attributes.mandatory);
    let mut clear = update("clear", 4);
    clear.body = None;
    clear.references = Some(vec![]);
    let fifth = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        first.id,
        clear.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(fifth.reference_generation, 2);
    assert!(fifth.references.is_empty());
    let deletion = DeleteMemory {
        expected_revision: 5,
        idempotency_key: "delete".into(),
        reason: "literal deletion".into(),
    };
    let deleted = NativeUserMemoryStore::delete(
        ctx.snapshot(),
        &write,
        first.id,
        deletion.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(deleted.revision, 6);
    assert_eq!(deleted.reference_generation, 3);
    assert!(deleted.references.is_empty());
    assert_eq!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, first.id, read_limits())
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        NativeUserMemoryStore::update(
            ctx.snapshot(),
            &write,
            first.id,
            edit.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        second
    );
    assert_eq!(
        NativeUserMemoryStore::promote(
            ctx.snapshot(),
            &write,
            first.id,
            promote.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        third
    );
    assert_eq!(
        NativeUserMemoryStore::demote(
            ctx.snapshot(),
            &write,
            first.id,
            demote.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        fourth
    );
    assert_eq!(
        NativeUserMemoryStore::delete(
            ctx.snapshot(),
            &write,
            first.id,
            deletion.clone(),
            write_limits()
        )
        .await
        .unwrap(),
        deleted
    );
    let history = NativeUserMemoryStore::history(ctx.snapshot(), &read, first.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert!(history.deleted);
    assert_eq!(history.current, deleted);
    assert_eq!(history.creation.owner, owner);
    assert_eq!(history.creation.input, create);
    assert_eq!(history.creation.memory, first);
    assert_eq!(history.revisions.len(), 5);
    // Each publication carries its head-revision marker, independently of ref generation.
    let physical = super::integrity::physical_rows(&f).await;
    let mut markers: Vec<_> = physical
        .iter()
        .filter(|b| super::integrity::text(b, "kind") == "references")
        .map(|b| {
            serde_json::from_str::<serde_json::Value>(&super::integrity::text(b, "payload"))
                .unwrap()
        })
        .collect();
    markers.sort_by_key(|m| m["revision"].as_u64().unwrap());
    assert_eq!(markers.len(), 6);
    for (offset, marker) in markers.iter().enumerate() {
        assert_eq!(marker["revision"], (offset + 1) as u64);
        assert_eq!(marker["reference_generation"], [1, 1, 1, 1, 2, 3][offset]);
        assert_eq!(marker["owner"], serde_json::to_value(&owner).unwrap());
        assert_eq!(marker["tombstone"], offset == 5);
        assert_eq!(
            marker["references"],
            serde_json::to_value(if offset < 4 { refs.as_slice() } else { &[] }).unwrap()
        );
    }

    assert_eq!(history.revisions[0].before, first);
    assert_eq!(history.revisions[0].after, second);
    assert_eq!(
        history.revisions[0].operation,
        MemoryMutation::Update(edit.clone())
    );
    for event in &history.revisions {
        assert_eq!(event.owner, owner);
        assert_eq!(event.actor, event.after.updated_by);
        assert_eq!(event.after.revision, event.before.revision + 1);
    }
    assert_eq!(
        NativeUserMemoryStore::reobserve_mutation(
            ctx.snapshot(),
            &write,
            first.id,
            MemoryMutation::Update(edit),
            write_limits()
        )
        .await
        .unwrap(),
        Some(history.revisions[0].clone())
    );
    assert_eq!(history.generation.sdk_version, before_noop.sdk_version + 3);
    assert!(
        matches!(NativeUserMemoryStore::list(ctx.snapshot(),&read,MemoryListFilter::default(),read_limits()).await.unwrap(),UserMemoryListRead::Published{memories,..} if memories.is_empty())
    );
    assert!(
        matches!(NativeUserMemoryStore::observe_heads(ctx.snapshot(),&read,head_limits()).await.unwrap(),UserMemoryHeadRead::Published{heads,..} if heads.is_empty())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_expected_revision_has_one_winner_and_changed_attempt_conflicts() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("cas"), write_limits())
            .await
            .unwrap();
    let (a, b) = tokio::join!(
        NativeUserMemoryStore::update(
            ctx.snapshot(),
            &write,
            memory.id,
            update("a", 1),
            write_limits()
        ),
        NativeUserMemoryStore::update(
            ctx.snapshot(),
            &write,
            memory.id,
            update("b", 1),
            write_limits()
        )
    );
    let (winner, committed) = match (a, b) {
        (
            Ok(committed),
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
        ) => ("a", committed),
        (
            Err(MemoryStoreError::RevisionConflict {
                expected: 1,
                actual: 2,
            }),
            Ok(committed),
        ) => ("b", committed),
        outcomes => panic!("one CAS winner and one exact revision conflict required: {outcomes:?}"),
    };
    let mut changed = update(winner, 1);
    changed.body = Some("changed same key".into());
    assert!(matches!(
        NativeUserMemoryStore::update(ctx.snapshot(), &write, memory.id, changed, write_limits())
            .await,
        Err(MemoryStoreError::IdempotencyConflict)
    ));
    let history = NativeUserMemoryStore::history(ctx.snapshot(), &read, memory.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(history.current, committed);
    assert_eq!(history.current.revision, 2);
    assert_eq!(history.revisions.len(), 1);
}
