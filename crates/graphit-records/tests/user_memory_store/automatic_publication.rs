use super::*;
use std::{cell::Cell, error::Error};
#[allow(
    dead_code,
    reason = "Real broker fixture is shared with the public integration journey."
)]
mod fixture {
    use crate as graphit_records;
    include!("fixture.rs");
}
use fixture::*;
use graphit_core::file_lock::Cancellation;

macro_rules! incoming_fenced {
    ($f:ident,$ctx:ident,$verifier:ident,$proof:ident,$read:ident,$write:ident) => {
        let $verifier = $f.server.verifier(true, $f.provider_revision(&$ctx));
        let raw = $f.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
        let $proof = $verifier
            .verify(
                &raw,
                graphit_auth::broker::BrokerRequestContext::new($ctx.snapshot()),
            )
            .await
            .unwrap();
        let $read = $f
            .issuer
            .admit_incoming_user_memory_read(&$proof, &$verifier, &$ctx, "service", $f.scope())
            .unwrap();
        let $write = $f
            .issuer
            .admit_incoming_user_memory_write(
                &$proof,
                &$verifier,
                &$ctx,
                "service",
                $f.scope(),
                graphit_auth::UserMemoryWriteOperation::FencedMemory,
            )
            .unwrap();
    };
}
async fn confirmed(fold: bool, output_failure: bool) {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    incoming_fenced!(f, ctx, v, p, read, write);
    let first = NativeUserMemoryStore::fenced_create(
        ctx.snapshot(),
        &write,
        input("confirmed-first"),
        write_limits(),
    )
    .await
    .unwrap();
    let removed = NativeUserMemoryStore::fenced_create(
        ctx.snapshot(),
        &write,
        input("confirmed-removed"),
        write_limits(),
    )
    .await
    .unwrap();
    let input = FoldMemories {
        kind: MemoryFoldKind::Merge,
        survivor: MemoryFoldMember {
            id: first.id,
            expected_revision: 1,
            idempotency_key: "confirmed-survivor".into(),
        },
        members: vec![MemoryFoldMember {
            id: removed.id,
            expected_revision: 1,
            idempotency_key: "confirmed-removal".into(),
        }],
        title: None,
        body: "actual folded body".into(),
        reason: "actual fold proof".into(),
    };
    let update = FencedUpdateMemory {
        expected_revision: 1,
        idempotency_key: "confirmed-fenced-update".into(),
        reason: "actual edit proof".into(),
        title: None,
        body: Some("actual updated body".into()),
        references: Some(vec![]),
    };
    let capture = native::acquire_with_check(
        ctx.snapshot(),
        &write,
        write_limits(),
        false,
        UserWritePolicy::Fenced,
        || check_pair(ctx.snapshot(), &read, &write),
    )
    .await
    .unwrap()
    .unwrap();
    let before = || {
        check_pair(ctx.snapshot(), &read, &write)?;
        capture.check(ctx.snapshot(), &write)
    };
    let a = store::chain(
        &capture.pin,
        ctx.snapshot(),
        &capture.owner,
        first.id,
        read_limits(),
        &before,
    )
    .await
    .unwrap()
    .unwrap();
    let b = store::chain(
        &capture.pin,
        ctx.snapshot(),
        &capture.owner,
        removed.id,
        read_limits(),
        &before,
    )
    .await
    .unwrap()
    .unwrap();
    let actor = native::actor(&write).unwrap();
    let at = store::now().unwrap();
    let operation = if fold {
        shared::fold_survivor(
            input.clone(),
            vec![
                (first.id, 1, first.attributes),
                (removed.id, 1, removed.attributes),
            ],
        )
    } else {
        FencedMemoryMutation::Update(update.clone()).into_mutation()
    };
    let anchor = event(
        ctx.snapshot(),
        &capture.owner,
        &actor,
        at,
        &first,
        operation,
    )
    .unwrap();
    let mut batches = vec![rows::mutation_batch(&a.creation, &anchor, write_limits()).unwrap()];
    let mut removed_events = Vec::new();
    if fold {
        let deletion = event(
            ctx.snapshot(),
            &capture.owner,
            &actor,
            at,
            &removed,
            shared::delete_operation(&input, &input.members[0]).unwrap(),
        )
        .unwrap();
        batches.push(rows::mutation_batch(&b.creation, &deletion, write_limits()).unwrap());
        removed_events.push(deletion);
    }
    let version = capture.pin.version();
    let pending = capture
        .pin
        .prepare_merge(batches, vec!["row_key".into()])
        .await
        .unwrap();
    let key = pending.recovery_key().clone();
    let calls = Cell::new(0);
    let result = store::publish(pending, &capture.owner, || {
        calls.set(calls.get() + 1);
        if calls.get() == 2 && !output_failure {
            cancel.cancel();
        }
        before()
    })
    .await;
    let error = if output_failure {
        let publication = result.unwrap();
        let receipt = MemoryFoldReceipt {
            survivor: effect(&anchor, Some(publication.generation.sdk_version)),
            removed: removed_events
                .iter()
                .map(|e| effect(e, Some(publication.generation.sdk_version)))
                .collect(),
        };
        assert_eq!(receipt.survivor.sdk_version, Some(version + 1));
        cancel.cancel();
        check_confirmed(&publication, before).unwrap_err()
    } else {
        result.unwrap_err()
    };
    assert!(error.source().is_some());
    let MemoryStoreError::UserConfirmed { publication, cause } = error else {
        panic!("actual committed USER delta");
    };
    assert!(cause.source().is_some());
    assert_eq!(calls.get(), 2);
    assert_eq!(publication.recovery_key, key);
    assert_eq!(publication.generation.sdk_version, version + 1);
    assert_eq!(publication.generation.owner, capture.owner);
    assert_eq!(publication.generation.physical_branch, None);
    assert_eq!(publication.generation.inherited_target, None);
    assert!(
        NativeUserMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &read,
            &write,
            input.clone(),
            write_limits()
        )
        .await
        .is_err()
    );
    drop(capture);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    incoming_fenced!(f, fresh, fv, fp, fr, fw);
    let detail = NativeUserMemoryStore::get(fresh.snapshot(), &fr, first.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.memory, anchor.after);
    assert_eq!(detail.generation, publication.generation);
    if fold {
        let observed = NativeUserMemoryStore::reobserve_automatic_fold(
            fresh.snapshot(),
            &fr,
            &fw,
            input.clone(),
            write_limits(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(observed.survivor.sdk_version, None);
        assert_eq!(observed.removed.len(), 1);
        assert_eq!(observed.removed[0].sdk_version, None);
        assert_eq!(
            NativeUserMemoryStore::automatic_fold(
                fresh.snapshot(),
                &fr,
                &fw,
                input,
                write_limits()
            )
            .await
            .unwrap(),
            observed
        );
        let archived =
            NativeUserMemoryStore::history(fresh.snapshot(), &fr, removed.id, read_limits())
                .await
                .unwrap()
                .unwrap();
        assert!(archived.deleted);
        assert_eq!(archived.revisions.len(), 1);
        assert!(archived.current.references.is_empty());
        assert_eq!(archived.generation, publication.generation);
    } else {
        assert_eq!(
            NativeUserMemoryStore::reobserve_fenced_mutation(
                fresh.snapshot(),
                &fw,
                first.id,
                FencedMemoryMutation::Update(update.clone()),
                write_limits()
            )
            .await
            .unwrap(),
            Some(anchor.clone())
        );
        assert_eq!(
            NativeUserMemoryStore::fenced_update(
                fresh.snapshot(),
                &fw,
                first.id,
                update,
                write_limits()
            )
            .await
            .unwrap(),
            anchor.after
        );
    }
    assert_eq!(
        NativeUserMemoryStore::get(fresh.snapshot(), &fr, first.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .generation,
        publication.generation
    );
    let UserMemoryReferencesRead::Published {
        generation,
        coverage,
        outgoing,
        ..
    } = NativeUserMemoryStore::references(
        fresh.snapshot(),
        &fr,
        UserMemoryReferencesQuery {
            outgoing: Some(first.id),
            incoming: None,
            page_size: std::num::NonZeroUsize::new(1).unwrap(),
            outgoing_cursor: None,
            incoming_cursor: None,
        },
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("current USER markers after known commit");
    };
    assert_eq!(generation, publication.generation);
    assert!(coverage.memory_user_source_complete);
    assert_eq!(outgoing.unwrap().total, 0);
}
#[tokio::test]
async fn actual_user_fenced_commit_preserves_confirmed_cause_and_fresh_historical_retry() {
    confirmed(false, false).await;
}
#[tokio::test]
async fn actual_user_automatic_group_commit_preserves_confirmed_cause_and_fresh_historical_retry() {
    confirmed(true, false).await;
}

#[tokio::test]
async fn actual_user_output_after_known_commit_retains_publication_and_fresh_group_retry() {
    confirmed(true, true).await;
}
