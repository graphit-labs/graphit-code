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
macro_rules! incoming {
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
                graphit_auth::UserMemoryWriteOperation::Memory,
            )
            .unwrap();
    };
}
#[tokio::test]
async fn committed_user_delta_retains_actual_key_cause_and_fresh_reobserve_without_second_commit() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let first = NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("before-confirmed"),
        write_limits(),
    )
    .await
    .unwrap();
    let UserMemoryLexicalIndexOutcome::Published(index) =
        NativeUserMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Refresh,
            write_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("actual USER index publication");
    };
    let capture = native::acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let chain = chain(
        &capture.pin,
        ctx.snapshot(),
        &capture.owner,
        first.id,
        read_limits(),
        || capture.check(ctx.snapshot(), &write),
    )
    .await
    .unwrap()
    .unwrap();
    let operation = MemoryMutation::Update(update("confirmed-update", 1));
    let sid = UserMemoryScopeId::from_owner(&capture.owner).unwrap();
    let after = revision::derive_in_scope(
        ctx.snapshot(),
        LogicalScope::User(sid.as_str()),
        &first,
        &operation,
        &native::actor(&write).unwrap(),
        now().unwrap(),
    )
    .unwrap();
    let event = UserMemoryRevision {
        owner: capture.owner.clone(),
        actor: native::actor(&write).unwrap(),
        operation: operation.clone(),
        before: first.clone(),
        after: after.clone(),
    };
    let batch = rows::mutation_batch(&chain.creation, &event, write_limits()).unwrap();
    let pending = capture
        .pin
        .prepare_merge(vec![batch], vec!["row_key".into()])
        .await
        .unwrap();
    let key = pending.recovery_key().clone();
    let calls = Cell::new(0);
    let error = preserve_index_publication(
        publish(pending, &capture.owner, || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                cancel.cancel();
            }
            capture.check(ctx.snapshot(), &write)
        })
        .await,
        Some(index.clone()),
    )
    .unwrap_err();
    assert!(error.source().is_some());
    let MemoryStoreError::UserConfirmed { publication, cause } = error else {
        panic!("actual known publication")
    };
    assert!(cause.source().is_some());
    let MemoryStoreError::UserIndexConfirmed {
        publication: confirmed_index,
        cause,
    } = *cause
    else {
        panic!("retained USER index effect");
    };
    assert_eq!(*confirmed_index, index);
    assert!(matches!(cause.as_ref(), MemoryStoreError::Request(_)));
    assert_eq!(calls.get(), 2);
    assert_eq!(publication.recovery_key, key);
    assert_eq!(publication.generation.owner, capture.owner);
    assert_eq!(publication.generation.physical_branch, None);
    assert!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, first.id, read_limits())
            .await
            .is_err()
    );
    drop(capture);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    incoming!(f, fresh, fv, fp, fr, fw);
    let detail = NativeUserMemoryStore::get(fresh.snapshot(), &fr, first.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.memory, after);
    assert_eq!(detail.generation, publication.generation);
    assert_eq!(
        NativeUserMemoryStore::reobserve_mutation(
            fresh.snapshot(),
            &fw,
            first.id,
            operation,
            write_limits()
        )
        .await
        .unwrap(),
        Some(event.clone())
    );
    assert_eq!(
        NativeUserMemoryStore::update(
            fresh.snapshot(),
            &fw,
            first.id,
            update("confirmed-update", 1),
            write_limits()
        )
        .await
        .unwrap(),
        after
    );
    assert_eq!(
        NativeUserMemoryStore::get(fresh.snapshot(), &fr, first.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .generation,
        detail.generation
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
        panic!("actual current USER reference source after confirmed")
    };
    assert_eq!(generation, detail.generation);
    assert!(coverage.memory_user_source_complete);
    assert_eq!(outgoing.unwrap().total, 0);
    assert_eq!(
        NativeUserMemoryStore::history(fresh.snapshot(), &fr, first.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .revisions,
        vec![event]
    );
}
#[tokio::test]
async fn committed_user_index_retains_actual_key_and_fresh_resolution_without_rebuild() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let first = NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("before-index-confirmed"),
        write_limits(),
    )
    .await
    .unwrap();
    let capture = native::acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let prepared = capture
        .pin
        .prepare_json_fts_index(
            "payload",
            "memory-json-content-v1",
            graphit_data::lance_branch_store::JsonFtsIndexMode::Replace,
        )
        .await
        .unwrap();
    let key = prepared.recovery_key().clone();
    let calls = Cell::new(0);
    let error = publish_index(prepared, &capture.owner, || {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            cancel.cancel();
        }
        capture.check(ctx.snapshot(), &write)
    })
    .await
    .map(|(_, publication)| publication)
    .unwrap_err();
    assert!(error.source().is_some());
    let MemoryStoreError::UserIndexConfirmed { publication, .. } = error else {
        panic!("actual known index publication")
    };
    assert_eq!(calls.get(), 2);
    assert_eq!(publication.recovery_key, key);
    assert_eq!(publication.generation.owner, capture.owner);
    assert!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, first.id, read_limits())
            .await
            .is_err()
    );
    drop(capture);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    incoming!(f, fresh, fv, fp, fr, fw);
    assert!(matches!(
        NativeUserMemoryStore::resolve_lexical_index(fresh.snapshot(), &fw, &key, write_limits())
            .await
            .unwrap(),
        DeltaResolution::Applied { .. }
    ));
    let UserMemoryLexicalSearchRead::Published {
        generation,
        hits,
        total_hits,
    } = NativeUserMemoryStore::search_lexical(
        fresh.snapshot(),
        &fr,
        MemoryLexicalQuery {
            text: "quartz".into(),
            filter: Default::default(),
            include_deleted_history: false,
            top_k: std::num::NonZeroUsize::new(10).unwrap(),
        },
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("actual index")
    };
    assert_eq!(generation, publication.generation);
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current, first);
    assert_eq!(
        NativeUserMemoryStore::get(fresh.snapshot(), &fr, first.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .generation,
        generation
    );
}
