use super::*;
use std::{
    cell::Cell,
    error::Error,
    num::{NonZeroU32, NonZeroUsize},
};
#[allow(
    dead_code,
    reason = "Real broker fixture shared with public USER integration journeys."
)]
mod fixture {
    use crate as graphit_records;
    include!("../user_memory_store/fixture.rs");
}
use fixture::*;
use graphit_core::file_lock::Cancellation;

macro_rules! incoming {
    ($f:ident,$ctx:ident,$v:ident,$p:ident,$r:ident,$w:ident) => {
        let $v = $f.server.verifier(true, $f.provider_revision(&$ctx));
        let token = $f.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
        let $p = $v
            .verify(
                &token,
                graphit_auth::broker::BrokerRequestContext::new($ctx.snapshot()),
            )
            .await
            .unwrap();
        let $r = $f
            .issuer
            .admit_incoming_user_memory_read(&$p, &$v, &$ctx, "service", $f.scope())
            .unwrap();
        let $w = $f
            .issuer
            .admit_incoming_user_memory_write(
                &$p,
                &$v,
                &$ctx,
                "service",
                $f.scope(),
                graphit_auth::UserMemoryWriteOperation::Memory,
            )
            .unwrap();
    };
}
#[tokio::test]
async fn actual_user_embedding_commit_preserves_key_cause_and_fresh_resolution_without_rewrite() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, v, p, read, write);
    let first = NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("embedding-confirmed-source"),
        write_limits(),
    )
    .await
    .unwrap();
    let identity = MemoryEmbeddingIdentity::new(
        [7; 32],
        NonZeroU32::new(2).unwrap(),
        MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let UserMemoryEmbeddingCorpusRead::Published(mut capture) =
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity.clone(),
            read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("actual source corpus");
    };
    let docs = capture
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(docs.len(), 1);
    let fresh_cancel = Cancellation::default();
    let fresh = request(&fresh_cancel);
    incoming!(f, fresh, fv, fp, fr, fw);
    let UserMemoryEmbeddingCorpusRead::Published(mut retry) =
        NativeUserMemoryStore::capture_embeddings(
            fresh.snapshot(),
            &fr,
            identity.clone(),
            read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("published source before concurrent attachment");
    };
    let retry_documents = retry
        .next_documents(fresh.snapshot(), &fr)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry_documents.len(), 1);
    assert_eq!(retry_documents[0].fingerprint(), docs[0].fingerprint());
    let writer = native::acquire(ctx.snapshot(), &write, write_limits(), false)
        .await
        .unwrap()
        .unwrap();
    let batch =
        encode_user_memory_embedding_row(&writer.owner, &docs[0], &[1., 0.], write_limits())
            .unwrap();
    let prepared = writer
        .pin
        .prepare_merge(vec![batch], vec!["row_key".into()])
        .await
        .unwrap();
    let key = prepared.recovery_key().clone();
    let version = writer.pin.version();
    let calls = Cell::new(0);
    let error = commit(prepared, &writer.owner, || {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            cancel.cancel();
        }
        writer.check(ctx.snapshot(), &write)?;
        capture.state.read.guard.revalidate(ctx.snapshot())?;
        Ok(())
    })
    .await
    .unwrap_err();
    assert!(error.source().is_some());
    let MemoryStoreError::UserEmbeddingConfirmed { publication, cause } = error else {
        panic!("known actual USER embedding publication");
    };
    assert!(cause.source().is_some());
    assert_eq!(calls.get(), 2);
    assert_eq!(publication.recovery_key, key);
    assert_eq!(publication.generation.sdk_version, version + 1);
    assert_eq!(publication.generation.owner, writer.owner);
    assert_eq!(publication.generation.physical_branch, None);
    assert_eq!(publication.generation.inherited_target, None);
    assert!(
        NativeUserMemoryStore::resolve_embeddings(ctx.snapshot(), &write, &key, write_limits())
            .await
            .is_err()
    );
    drop(writer);
    assert_eq!(
        NativeUserMemoryStore::resolve_embeddings(fresh.snapshot(), &fw, &key, write_limits())
            .await
            .unwrap(),
        DeltaResolution::Applied {
            version: publication.generation.sdk_version
        }
    );
    let detail = NativeUserMemoryStore::get(fresh.snapshot(), &fr, first.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.memory, first);
    assert_eq!(detail.generation, publication.generation);
    let UserMemoryEmbeddingCorpusRead::Published(mut current) =
        NativeUserMemoryStore::capture_embeddings(
            fresh.snapshot(),
            &fr,
            identity.clone(),
            read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("current metadata corpus");
    };
    assert!(
        current
            .next_documents(fresh.snapshot(), &fr)
            .await
            .unwrap()
            .is_none()
    );
    let outcome = NativeUserMemoryStore::attach_embeddings(
        fresh.snapshot(),
        &fw,
        &retry,
        vec![MemoryEmbeddingAttachment {
            memory_id: first.id,
            revision: std::num::NonZeroU64::MIN,
            input_fingerprint: *retry_documents[0].fingerprint(),
            vector: vec![1., 0.],
        }],
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        outcome,
        UserMemoryEmbeddingAttachOutcome::Existing {
            generation: publication.generation.clone()
        }
    );
    let UserMemorySemanticSearchRead::Published {
        generation,
        total_hits,
        hits,
    } = NativeUserMemoryStore::search_semantic(
        fresh.snapshot(),
        &fr,
        identity,
        &[1., 0.],
        MemorySemanticSearchOptions {
            filter: MemoryListFilter::default(),
            include_deleted_history: false,
            top_k: NonZeroUsize::new(1).unwrap(),
        },
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("current actual vector");
    };
    assert_eq!(generation, publication.generation);
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current, first);
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
            page_size: NonZeroUsize::new(1).unwrap(),
            outgoing_cursor: None,
            incoming_cursor: None,
        },
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("physical embedding coexists with current marker");
    };
    assert_eq!(generation, publication.generation);
    assert!(coverage.memory_user_source_complete);
    assert_eq!(outgoing.unwrap().total, 0);
    assert_eq!(
        NativeUserMemoryStore::get(fresh.snapshot(), &fr, first.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .generation,
        publication.generation
    );
}
