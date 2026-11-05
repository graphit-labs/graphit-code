#![cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "user_memory_store/fixture.rs"]
mod fixture;
#[path = "user_memory_embeddings/sdk.rs"]
mod sdk;
use fixture::*;
use graphit_core::{artifact_store::StoreOwner, file_lock::Cancellation};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::*,
    reference_projection::ReferenceTargetKind,
    user_memory_store::*,
};
use std::{
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};
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
fn identity(config: u8, dimensions: u32) -> MemoryEmbeddingIdentity {
    MemoryEmbeddingIdentity::new(
        [config; 32],
        NonZeroU32::new(dimensions).unwrap(),
        MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap()
}
fn options(k: usize) -> MemorySemanticSearchOptions {
    MemorySemanticSearchOptions {
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(k).unwrap(),
    }
}
fn corpus(read: UserMemoryEmbeddingCorpusRead) -> UserMemoryEmbeddingCorpus {
    match read {
        UserMemoryEmbeddingCorpusRead::Published(c) => c,
        _ => panic!("published USER capture"),
    }
}
fn attachment(d: &MemoryEmbeddingDocument, vector: Vec<f32>) -> MemoryEmbeddingAttachment {
    MemoryEmbeddingAttachment {
        memory_id: d.memory_id(),
        revision: NonZeroU64::new(d.revision()).unwrap(),
        input_fingerprint: *d.fingerprint(),
        vector,
    }
}
fn publication(result: UserMemoryEmbeddingAttachOutcome) -> UserMemoryEmbeddingPublication {
    match result {
        UserMemoryEmbeddingAttachOutcome::Published(p) => p,
        _ => panic!("new USER metadata publication"),
    }
}
fn hits(read: UserMemorySemanticSearchRead) -> (usize, Vec<MemorySemanticHit>) {
    match read {
        UserMemorySemanticSearchRead::Published {
            total_hits, hits, ..
        } => (total_hits, hits),
        other => panic!("USER hits expected: {other:?}"),
    }
}
fn owner(f: &Fixture) -> UserMemoryOwner {
    match f.user.owner() {
        StoreOwner::User {
            installation_id,
            identity,
        } => UserMemoryOwner {
            installation_id: installation_id.clone(),
            identity: identity.clone(),
        },
        _ => panic!("USER"),
    }
}
async fn documents(
    c: &mut UserMemoryEmbeddingCorpus,
    ctx: &graphit_hub::policy::RequestContext<'_>,
    read: &graphit_auth::UserReadAdmission<'_>,
) -> Vec<MemoryEmbeddingDocument> {
    let mut all = vec![];
    while let Some(batch) = c.next_documents(ctx.snapshot(), read).await.unwrap() {
        all.extend(batch);
    }
    all
}

#[tokio::test]
async fn user_semantic_four_states_default_pin_no_birth_and_recovery_binding() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    assert!(matches!(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryEmbeddingCorpusRead::NoCorpus
    ));
    assert_eq!(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemorySemanticSearchRead::NoCorpus
    );
    assert!(!f.user.runtime_root().exists());
    sdk::empty(&f, &ctx).await;
    let mut empty = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert!(
        empty
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemorySemanticSearchRead::PublishedEmpty { .. }
    ));
    assert!(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &empty,
            vec![],
            write_limits()
        )
        .await
        .is_err()
    );
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("state-live"), write_limits())
            .await
            .unwrap();
    assert!(matches!(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemorySemanticSearchRead::Unavailable {
            reason: MemoryEmbeddingUnavailable::MissingCoverage,
            ..
        }
    ));
    let mut c = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let docs = documents(&mut c, &ctx, &read).await;
    assert_eq!(docs.len(), 1);
    let before = sdk::pin(&f).await.version();
    let p = publication(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(&docs[0], vec![1., 0.])],
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(p.generation.owner, owner(&f));
    assert_eq!(p.generation.sdk_version, before + 1);
    assert_eq!(p.generation.physical_branch, None);
    assert_eq!(p.generation.inherited_target, None);
    assert_eq!(
        NativeUserMemoryStore::resolve_embeddings(
            ctx.snapshot(),
            &write,
            &p.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        graphit_data::lance_branch_store::DeltaResolution::Applied {
            version: p.generation.sdk_version
        }
    );
    let (total, result) = hits(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(result[0].current, memory);
    assert_eq!(result[0].distance, 0.);
    let absent = Fixture::new(&ctx).await;
    incoming!(absent, ctx, av, ap, _ar, aw);
    assert!(
        NativeUserMemoryStore::resolve_embeddings(
            ctx.snapshot(),
            &aw,
            &p.recovery_key,
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(!absent.user.runtime_root().exists());
    assert_eq!(sdk::pin(&f).await.version(), p.generation.sdk_version);
}

#[tokio::test]
async fn user_emitted_chunks_partial_coverage_existing_retry_and_exact_owner25_rows() {
    use arrow_array::{Array, ListArray};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let a = NativeUserMemoryStore::create(ctx.snapshot(), &write, input("chunk-a"), write_limits())
        .await
        .unwrap();
    let a2 = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        update("chunk-edit", 1),
        write_limits(),
    )
    .await
    .unwrap();
    let b = NativeUserMemoryStore::create(ctx.snapshot(), &write, input("chunk-b"), write_limits())
        .await
        .unwrap();
    let before = sdk::pin(&f).await.version();
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::new(1).unwrap();
    let mut c = corpus(
        NativeUserMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), limits)
            .await
            .unwrap(),
    );
    assert_eq!(c.generation().sdk_version, before);
    assert_eq!(c.identity(), &identity(1, 2));
    let first = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.len(), 1);
    NativeUserMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        vec![attachment(&first[0], vec![1., 0.])],
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(10),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemorySemanticSearchRead::Published { total_hits: 1, .. }
    ));
    let mut docs = first;
    docs.extend(documents(&mut c, &ctx, &read).await);
    assert_eq!(docs.len(), 3);
    assert_eq!(docs.iter().filter(|d| d.memory_id() == a.id).count(), 2);
    assert!(docs.iter().all(|d| d.identity() == &identity(1, 2)));
    for d in &docs {
        let m = if d.memory_id() == b.id {
            &b
        } else if d.revision() == 1 {
            &a
        } else {
            &a2
        };
        assert_eq!(
            d,
            &prepare_memory_embedding_document(m, &identity(1, 2)).unwrap()
        );
    }
    let values = docs.iter().map(|d| attachment(d, vec![1., 0.])).collect();
    let p = publication(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            values,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(p.generation.sdk_version, before + 2);
    assert_eq!(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            docs.iter().map(|d| attachment(d, vec![1., 0.])).collect(),
            write_limits()
        )
        .await
        .unwrap(),
        UserMemoryEmbeddingAttachOutcome::Existing {
            generation: p.generation.clone()
        }
    );
    assert_eq!(sdk::pin(&f).await.version(), p.generation.sdk_version);
    let rows = sdk::rows(&f).await;
    let embeddings = rows
        .iter()
        .filter(|b| sdk::text(b, "kind") == "embedding")
        .collect::<Vec<_>>();
    assert_eq!(embeddings.len(), 3);
    for row in embeddings {
        assert_eq!(row.schema().fields().len(), 25);
        assert_eq!(
            sdk::text(row, "installation_id"),
            owner(&f).installation_id.as_str()
        );
        assert_eq!(sdk::text(row, "user_issuer"), f.server.origin);
        assert_eq!(sdk::text(row, "user_subject"), SUBJECT);
        assert_eq!(sdk::text(row, "embedding_recipe"), MEMORY_DOCUMENT_RECIPE);
        assert_eq!(sdk::text(row, "embedding_role"), "Document");
        let vectors = row
            .column_by_name("embedding_vector")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();
        assert!(!vectors.is_null(0));
        assert_eq!(vectors.value_length(0), 2);
    }
    let (total, result) = hits(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 2);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].current.id, a.id.min(b.id));
    assert_eq!(result[0].location, MemoryMatchLocation::Current);
    assert_eq!(
        NativeUserMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        a2
    );
}

#[tokio::test]
async fn user_archive_distance_ties_collapse_filters_and_deleted_history_are_explicit() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let mut authored = input("ranking-a");
    authored.attributes.mandatory = false;
    let a = NativeUserMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let _a2 = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        update("rank2", 1),
        write_limits(),
    )
    .await
    .unwrap();
    let a3 = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        {
            let mut patch = update("rank3", 2);
            patch.body = Some("third current body".into());
            patch
        },
        write_limits(),
    )
    .await
    .unwrap();
    let mut authored = input("ranking-b");
    authored.attributes.mandatory = false;
    let b = NativeUserMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let docs = documents(&mut c, &ctx, &read).await;
    assert_eq!(docs.len(), 4);
    let values = docs
        .iter()
        .map(|d| {
            attachment(
                d,
                if d.memory_id() == a.id && d.revision() < 3 {
                    vec![1., 0.]
                } else if d.memory_id() == b.id {
                    vec![1., 1.]
                } else {
                    vec![0., 1.]
                },
            )
        })
        .collect();
    NativeUserMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, values, write_limits())
        .await
        .unwrap();
    let (total, current) = hits(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 2);
    // Live memories always contribute their immutable archive; this flag only
    // controls histories whose current head is deleted.
    assert_eq!(current[0].current, a3);
    assert_eq!(current[0].matched_revision.get(), 2);
    assert_eq!(current[0].distance, 0.);
    assert!(
        matches!(current[0].location, MemoryMatchLocation::Archived { revision } if revision.get() == 2)
    );
    let (_, ranked) = hits(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(ranked.len(), 2);
    assert_eq!(ranked[0].current, a3);
    assert_eq!(ranked[0].matched_revision.get(), 2);
    assert_eq!(ranked[0].distance, 0.);
    assert_eq!(ranked[1].current, b);
    assert_eq!(ranked[1].matched_revision.get(), 1);
    assert_eq!(ranked[1].location, MemoryMatchLocation::Current);
    assert!((ranked[1].distance - (1. - std::f32::consts::FRAC_1_SQRT_2)).abs() < 1e-6);
    let mut archive = options(1);
    archive.include_deleted_history = true;
    let (total, best) = hits(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            archive.clone(),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 2);
    assert_eq!(best[0].current, a3);
    assert_eq!(best[0].matched_revision.get(), 2);
    assert!(
        matches!(best[0].location,MemoryMatchLocation::Archived{revision} if revision.get()==2)
    );
    NativeUserMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        docs.iter().map(|d| attachment(d, vec![1., 0.])).collect(),
        write_limits(),
    )
    .await
    .unwrap();
    let mut tie_options = archive.clone();
    tie_options.top_k = NonZeroUsize::new(2).unwrap();
    let (total, ties) = hits(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            tie_options,
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 2);
    assert_eq!(ties.len(), 2);
    let mut expected_ids = vec![a.id, b.id];
    expected_ids.sort();
    assert_eq!(
        ties.iter().map(|hit| hit.current.id).collect::<Vec<_>>(),
        expected_ids
    );
    for hit in &ties {
        assert_eq!(hit.distance, 0.);
        assert_eq!(hit.location, MemoryMatchLocation::Current);
        if hit.current.id == a.id {
            assert_eq!(hit.current, a3);
            assert_eq!(hit.matched_revision.get(), 3);
        } else {
            assert_eq!(hit.current, b);
            assert_eq!(hit.matched_revision.get(), 1);
        }
    }
    archive.filter.tag = Some(" tag ".into());
    assert_eq!(
        hits(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &[1., 0.],
                archive.clone(),
                read_limits()
            )
            .await
            .unwrap()
        )
        .0,
        2
    );
    archive.filter.tag = Some("tag".into());
    assert_eq!(
        hits(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &[1., 0.],
                archive,
                read_limits()
            )
            .await
            .unwrap()
        )
        .0,
        0
    );
    let deleted = NativeUserMemoryStore::delete(
        ctx.snapshot(),
        &write,
        a.id,
        DeleteMemory {
            expected_revision: a3.revision,
            idempotency_key: "rank-delete".into(),
            reason: "archive only".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    let (total, live) = hits(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(10),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(live[0].current, b);
    let mut archived = options(10);
    archived.include_deleted_history = true;
    let (total, all) = hits(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            archived,
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 2);
    assert!(
        all.iter()
            .any(|h| h.current == deleted && h.matched_revision.get() == a3.revision)
    );
    let mut after = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let docs = documents(&mut after, &ctx, &read).await;
    assert!(
        docs.iter()
            .all(|d| d.memory_id() != a.id || d.revision() < deleted.revision)
    );
    assert!(
        docs.is_empty(),
        "all non-delete revisions already have compatible vectors"
    );
}

#[tokio::test]
async fn user_attachment_emission_fingerprint_shape_duplicates_and_exact_vector_budget_precede_effects()
 {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("allowlist"), write_limits())
            .await
            .unwrap();
    let mut c = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let d = prepare_memory_embedding_document(&memory, &identity(1, 2)).unwrap();
    let before = sdk::pin(&f).await.version();
    assert!(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(&d, vec![1., 0.])],
            write_limits()
        )
        .await
        .is_err()
    );
    let d = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    let good = attachment(&d, vec![1., 0.]);
    let mut fingerprint = good.clone();
    fingerprint.input_fingerprint[0] ^= 1;
    let mut revision = good.clone();
    revision.revision = NonZeroU64::new(2).unwrap();
    let mut unknown = good.clone();
    unknown.memory_id = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
    for bad in [
        fingerprint,
        revision,
        unknown,
        attachment(&d, vec![1.]),
        attachment(&d, vec![f32::NAN, 0.]),
        attachment(&d, vec![f32::INFINITY, 0.]),
    ] {
        assert!(
            NativeUserMemoryStore::attach_embeddings(
                ctx.snapshot(),
                &write,
                &c,
                vec![bad],
                write_limits()
            )
            .await
            .is_err()
        );
        assert_eq!(sdk::pin(&f).await.version(), before);
    }
    assert!(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![good.clone(), good.clone()],
            write_limits()
        )
        .await
        .is_err()
    );
    let mut bound = write_limits();
    bound.max_input_bytes = NonZeroUsize::new(63).unwrap();
    assert!(matches!(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![good.clone()],
            bound
        )
        .await,
        Err(MemoryStoreError::Limit(_))
    ));
    assert_eq!(sdk::pin(&f).await.version(), before);
    assert!(matches!(
        encode_user_memory_embedding_row(&owner(&f), &d, &[1., 0.], bound),
        Err(MemoryStoreError::Limit(_))
    ));
    bound.max_input_bytes = NonZeroUsize::new(64).unwrap();
    let row = encode_user_memory_embedding_row(&owner(&f), &d, &[1., 0.], bound).unwrap();
    assert_eq!(row.schema().fields().len(), 25);
    assert_eq!(sdk::text(&row, "kind"), "embedding");
    assert_eq!(sdk::text(&row, "user_subject"), SUBJECT);
    for vector in [&[1.][..], &[f32::NAN, 0.][..]] {
        assert!(encode_user_memory_embedding_row(&owner(&f), &d, vector, write_limits()).is_err());
        assert!(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                vector,
                options(1),
                read_limits()
            )
            .await
            .is_err()
        );
    }
    NativeUserMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, vec![good], bound)
        .await
        .unwrap();
    assert_eq!(sdk::pin(&f).await.version(), before + 1);
    assert!(matches!(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[0., 0.],
            options(1),
            read_limits()
        )
        .await,
        Err(MemoryStoreError::VectorDistance(_))
    ));
}

#[tokio::test]
async fn user_immutable_old_sources_and_new_config_dimensions_require_current_coverage() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let first = NativeUserMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("source-change"),
        write_limits(),
    )
    .await
    .unwrap();
    let mut old = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let old_document = old
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    let second = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        first.id,
        update("source-changed", 1),
        write_limits(),
    )
    .await
    .unwrap();
    NativeUserMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &old,
        vec![attachment(&old_document, vec![1., 0.])],
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeUserMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits()
        )
        .await
        .unwrap(),
        UserMemorySemanticSearchRead::Published { total_hits: 1, .. }
    ));
    for (config, dim) in [(1, 2), (2, 2), (3, 3)] {
        let ident = identity(config, dim);
        let mut c = corpus(
            NativeUserMemoryStore::capture_embeddings(
                ctx.snapshot(),
                &read,
                ident.clone(),
                read_limits(),
            )
            .await
            .unwrap(),
        );
        let docs = documents(&mut c, &ctx, &read).await;
        assert_eq!(docs.len(), if config == 1 { 1 } else { 2 });
        let mut vector = vec![0.; dim as usize];
        vector[0] = 1.;
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            docs.iter().map(|d| attachment(d, vector.clone())).collect(),
            write_limits(),
        )
        .await
        .unwrap();
        let (_, result) = hits(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                ident,
                &vector,
                options(1),
                read_limits(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(result[0].current, second);
        assert_eq!(result[0].location, MemoryMatchLocation::Current);
        if config > 1 {
            assert!(matches!(
                NativeUserMemoryStore::search_semantic(
                    ctx.snapshot(),
                    &read,
                    identity(1, 2),
                    &[1., 0.],
                    options(1),
                    read_limits()
                )
                .await
                .unwrap(),
                UserMemorySemanticSearchRead::Unavailable {
                    reason: MemoryEmbeddingUnavailable::IncompatibleIdentity,
                    ..
                }
            ));
        }
    }
}

#[tokio::test]
async fn user_capture_guards_owner_original_config_cancel_deadline_path_and_fenced_write_deny() {
    use graphit_auth::broker::BrokerRequestContext;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::fenced(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    NativeUserMemoryStore::create(ctx.snapshot(), &write, input("guards"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let foreign = request(&cancel);
    assert!(c.next_documents(foreign.snapshot(), &read).await.is_err());
    let docs = documents(&mut c, &ctx, &read).await;
    let before = sdk::pin(&f).await.version();
    assert!(
        NativeUserMemoryStore::attach_embeddings(
            foreign.snapshot(),
            &write,
            &c,
            vec![attachment(&docs[0], vec![1., 0.])],
            write_limits()
        )
        .await
        .is_err()
    );
    let other = Fixture::new(&ctx).await;
    incoming!(other, ctx, ov, op, or, ow);
    assert!(c.next_documents(ctx.snapshot(), &or).await.is_err());
    assert!(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &ow,
            &c,
            vec![attachment(&docs[0], vec![1., 0.])],
            write_limits()
        )
        .await
        .is_err()
    );
    assert!(!other.user.runtime_root().exists());
    let fenced = f
        .issuer
        .admit_incoming_user_memory_write(
            &proof,
            &verifier,
            &ctx,
            "service",
            f.scope(),
            graphit_auth::UserMemoryWriteOperation::FencedMemory,
        )
        .unwrap();
    assert!(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &fenced,
            &c,
            vec![attachment(&docs[0], vec![1., 0.])],
            write_limits()
        )
        .await
        .is_err()
    );
    assert_eq!(sdk::pin(&f).await.version(), before);
    let denied = Fixture::with_permissions(&ctx, &[ALL[0]]).await;
    let dv = denied.server.verifier(true, denied.provider_revision(&ctx));
    let raw = denied.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
    let dp = dv
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    assert!(
        denied
            .issuer
            .admit_incoming_user_memory_read(&dp, &dv, &ctx, "service", denied.scope())
            .is_err()
    );
    assert!(!denied.user.runtime_root().exists());
    for ending in 0..4 {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx).await;
        let verifier = f.server.verifier(true, f.provider_revision(&ctx));
        let seed_expiry = now_seconds() + 600;
        let raw = f.server.token(SUBJECT, RESOURCE, seed_expiry);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = f
            .issuer
            .admit_incoming_user_memory_read(&proof, &verifier, &ctx, "service", f.scope())
            .unwrap();
        let write = f
            .issuer
            .admit_incoming_user_memory_write(
                &proof,
                &verifier,
                &ctx,
                "service",
                f.scope(),
                graphit_auth::UserMemoryWriteOperation::Memory,
            )
            .unwrap();
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("invalidate"), write_limits())
            .await
            .unwrap();
        let mut c = corpus(
            NativeUserMemoryStore::capture_embeddings(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                read_limits(),
            )
            .await
            .unwrap(),
        );
        let d = c
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .unwrap()
            .remove(0);
        let before = sdk::pin(&f).await.version();
        // Publish and capture with a durable proof before starting the expiry boundary.
        let expiry = now_seconds() + if ending == 3 { 5 } else { 600 };
        let raw = f.server.token(SUBJECT, RESOURCE, expiry);
        let proof = verifier
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = f
            .issuer
            .admit_incoming_user_memory_read(&proof, &verifier, &ctx, "service", f.scope())
            .unwrap();
        let write = f
            .issuer
            .admit_incoming_user_memory_write(
                &proof,
                &verifier,
                &ctx,
                "service",
                f.scope(),
                graphit_auth::UserMemoryWriteOperation::Memory,
            )
            .unwrap();
        assert!(
            c.next_documents(ctx.snapshot(), &read)
                .await
                .unwrap()
                .is_none(),
            "the refreshed proof must remain valid for the original capture before invalidation"
        );
        match ending {
            0 => {
                let result = f
                    .store
                    .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                        let p = doc.provider("service").unwrap();
                        let revision = p.revision();
                        let at = p.updated_at() + 1;
                        let config = p.config().clone();
                        doc.update_provider("service", config, doc.revision(), revision, at, ctx)
                    })
                    .unwrap();
                assert!(result.failure().is_none());
            }
            1 => cancel.cancel(),
            2 => {
                let root = f.user.user_root().unwrap();
                std::fs::rename(root, root.with_extension("replaced")).unwrap();
                std::fs::create_dir(root).unwrap();
            }
            3 => {
                let end = std::time::UNIX_EPOCH + Duration::from_secs(expiry);
                tokio::time::sleep(
                    end.duration_since(std::time::SystemTime::now())
                        .unwrap_or_default()
                        + Duration::from_millis(20),
                )
                .await;
            }
            _ => unreachable!(),
        };
        assert!(c.next_documents(ctx.snapshot(), &read).await.is_err());
        assert!(
            NativeUserMemoryStore::attach_embeddings(
                ctx.snapshot(),
                &write,
                &c,
                vec![attachment(&d, vec![1., 0.])],
                write_limits()
            )
            .await
            .is_err()
        );
        assert!(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &[1., 0.],
                options(1),
                read_limits()
            )
            .await
            .is_err()
        );
        if ending != 2 {
            assert_eq!(sdk::pin(&f).await.version(), before);
        } else {
            assert!(!f.user.runtime_root().join("memory").exists());
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let cancel = Cancellation::default();
    let timed = request_until(&cancel, deadline);
    let f = Fixture::new(&timed).await;
    incoming!(f, timed, tv, tp, tr, _tw);
    tokio::time::sleep(
        deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(20),
    )
    .await;
    assert!(
        NativeUserMemoryStore::capture_embeddings(
            timed.snapshot(),
            &tr,
            identity(1, 2),
            read_limits()
        )
        .await
        .is_err()
    );
    assert!(!f.user.runtime_root().exists());
}

#[tokio::test]
async fn user_semantic_eof_duplicate_foreign_orphan_fingerprint_vector_and_exact_scan_caps() {
    use arrow_array::{
        FixedSizeBinaryArray, RecordBatch,
        builder::{Float32Builder, ListBuilder},
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    NativeUserMemoryStore::create(ctx.snapshot(), &write, input("eof-source"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let d = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    NativeUserMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        vec![attachment(&d, vec![1., 0.])],
        write_limits(),
    )
    .await
    .unwrap();
    let good = sdk::rows(&f).await;
    assert_eq!(good.len(), 4);
    let embedding = good
        .iter()
        .find(|b| sdk::text(b, "kind") == "embedding")
        .unwrap();
    let marker = good
        .iter()
        .find(|b| sdk::text(b, "kind") == "references")
        .unwrap();
    let mut duplicate = good.clone();
    duplicate.push(embedding.clone());
    let foreign = sdk::text_column(embedding, "user_issuer", "foreign issuer");
    let orphan = sdk::text_column(
        embedding,
        "payload",
        "{\"memory_id\":\"01KSH1CRFFG8Z74B5ZS78WW809\",\"memory_revision\":1}",
    );
    let role = sdk::text_column(embedding, "embedding_role", "Query");
    let mut columns = embedding.columns().to_vec();
    columns[embedding
        .schema()
        .index_of("embedding_fingerprint")
        .unwrap()] =
        Arc::new(FixedSizeBinaryArray::try_from_iter([[7u8; 32].as_slice()].into_iter()).unwrap());
    let fingerprint = RecordBatch::try_new(embedding.schema(), columns).unwrap();
    let mut list = ListBuilder::new(Float32Builder::new());
    list.values().append_slice(&[f32::NAN, 0.]);
    list.append(true);
    let mut columns = embedding.columns().to_vec();
    columns[embedding.schema().index_of("embedding_vector").unwrap()] = Arc::new(list.finish());
    let vector = RecordBatch::try_new(embedding.schema(), columns).unwrap();
    let mut cases = vec![duplicate];
    for bad in [foreign, orphan, role, fingerprint, vector] {
        cases.push(
            good.iter()
                .map(|b| {
                    if sdk::text(b, "kind") == "embedding" {
                        bad.clone()
                    } else {
                        b.clone()
                    }
                })
                .collect(),
        );
    }
    let corrupt_marker = sdk::text_column(marker, "payload", "{broken marker");
    cases.push(
        good.iter()
            .map(|b| {
                if sdk::text(b, "kind") == "references" {
                    corrupt_marker.clone()
                } else {
                    b.clone()
                }
            })
            .collect(),
    );
    let mut bounds = read_limits();
    bounds.batch_rows = NonZeroU32::new(1).unwrap();
    for (index, rows) in cases.into_iter().enumerate() {
        sdk::replace(&f, rows).await;
        assert!(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &[1., 0.],
                options(1),
                bounds
            )
            .await
            .is_err(),
            "late semantic fixture {index}"
        );
        let captured = NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            bounds,
        )
        .await;
        match captured {
            Err(_) => (),
            Ok(UserMemoryEmbeddingCorpusRead::Published(mut c)) => {
                assert!(
                    c.next_documents(ctx.snapshot(), &read).await.is_err(),
                    "capture EOF fixture {index}"
                );
            }
            Ok(_) => panic!("corruption cannot hide as NoCorpus"),
        }
    }
    sdk::replace(&f, good.clone()).await;
    let mut exact = read_limits();
    exact.max_query_bytes = NonZeroUsize::MIN;
    exact.batch_rows = NonZeroU32::new(1).unwrap();
    let mut exact_capture = corpus(
        NativeUserMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), exact)
            .await
            .unwrap(),
    );
    assert!(documents(&mut exact_capture, &ctx, &read).await.is_empty());
    assert_eq!(
        hits(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &[1., 0.],
                options(1),
                exact
            )
            .await
            .unwrap()
        )
        .0,
        1
    );
}

#[tokio::test]
async fn user_embeddings_coexist_with_native_web_reads_crud_lexical_refs_and_fenced_automatic() {
    use graphit_auth::{
        UserMemoryWriteOperation,
        broker::{
            BrokerRequestContext,
            oauth::{OAuthLimits, web::WebOAuth},
        },
    };
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::native_fenced(&ctx).await;
    f.install_native_profile(&ctx);
    let verifier = f.server.verifier(false, f.provider_revision(&ctx));
    let secret = f.secret();
    let caller = f
        .issuer
        .authenticate_runtime(&secret, &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let read = f
        .issuer
        .admit_authenticated_user_memory_read(&caller, &ctx, f.scope())
        .unwrap();
    let write = f
        .issuer
        .admit_authenticated_user_memory_write(
            &caller,
            &ctx,
            f.scope(),
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let fenced = f
        .issuer
        .admit_authenticated_user_memory_write(
            &caller,
            &ctx,
            f.scope(),
            UserMemoryWriteOperation::FencedMemory,
        )
        .unwrap();
    let r = MemoryReference {
        target_kind: ReferenceTargetKind::Task,
        scope: MemoryReferenceScope::User(
            UserMemoryScopeId::from_owner(&owner(&f))
                .unwrap()
                .as_str()
                .into(),
        ),
        id: "semantic-source-refs-only".into(),
        context: None,
        relation: "supports".into(),
        field: None,
    };
    let mut authored = input("interop-a");
    authored.attributes = MemoryAttributes {
        classification: MemoryClass::Fact,
        mandatory: false,
        important: false,
    };
    authored.references = Some(vec![r.clone()]);
    let a = NativeUserMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let mut authored = input("interop-b");
    authored.attributes.mandatory = false;
    let b = NativeUserMemoryStore::create(ctx.snapshot(), &write, authored, write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let docs = documents(&mut c, &ctx, &read).await;
    assert_eq!(docs.len(), 2);
    NativeUserMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        docs.iter().map(|d| attachment(d, vec![1., 0.])).collect(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        NativeUserMemoryStore::history(ctx.snapshot(), &read, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .current,
        a
    );
    assert!(
        matches!(NativeUserMemoryStore::observe_heads(ctx.snapshot(),&read,head_limits()).await.unwrap(),UserMemoryHeadRead::Published{heads,..} if heads.len()==2)
    );
    assert!(
        matches!(NativeUserMemoryStore::list(ctx.snapshot(),&read,MemoryListFilter::default(),read_limits()).await.unwrap(),UserMemoryListRead::Published{memories,..} if memories.len()==2)
    );
    let flow = WebOAuth::new(
        &verifier,
        OAuthLimits {
            authorization_url_bytes: 8192,
            callback_bytes: 8192,
            callback_query_entries: 8,
            form_body_bytes: 8192,
            token_bytes: 8192,
            response_nodes: 32,
            response_scalar_bytes: 8192,
        },
        "https://workspace.example/callback",
        false,
    )
    .unwrap();
    let registered = flow
        .register(BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let raw=f.server.sign(&serde_json::json!({"iss":f.server.origin,"sub":SUBJECT,"aud":["broker-api",registered.resource()],"client_id":"web-client","jti":"web-user-semantics","token_use":"access","iat":now_seconds().saturating_sub(1),"exp":now_seconds()+600}));
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let web = f
        .issuer
        .admit_web_user_memory_read(&proof, &verifier, &ctx, "service", f.scope())
        .unwrap();
    assert_eq!(
        hits(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &web,
                identity(1, 2),
                &[1., 0.],
                options(10),
                read_limits()
            )
            .await
            .unwrap()
        )
        .0,
        2
    );
    let lexical = MemoryLexicalQuery {
        text: "quartz".into(),
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(10).unwrap(),
    };
    assert!(matches!(
        NativeUserMemoryStore::search_lexical(ctx.snapshot(), &web, lexical, read_limits())
            .await
            .unwrap(),
        UserMemoryLexicalSearchRead::Published { total_hits: 2, .. }
    ));
    let refs = UserMemoryReferencesQuery {
        outgoing: Some(a.id),
        incoming: Some(MemoryReferenceTarget {
            target_kind: r.target_kind,
            scope: r.scope.clone(),
            id: r.id.clone(),
            context: None,
        }),
        page_size: NonZeroUsize::new(100).unwrap(),
        outgoing_cursor: None,
        incoming_cursor: None,
    };
    let ref_limits = read_limits();
    assert!(
        matches!(NativeUserMemoryStore::references(ctx.snapshot(),&web,refs.clone(),ref_limits).await.unwrap(),UserMemoryReferencesRead::Published{incoming:Some(p),..} if p.total==1)
    );
    let mut edit = update("interop-edit", 1);
    edit.body = None;
    edit.references = Some(vec![]);
    let updated =
        NativeUserMemoryStore::update(ctx.snapshot(), &write, a.id, edit.clone(), write_limits())
            .await
            .unwrap();
    assert_eq!(updated.reference_generation, 2);
    assert!(
        matches!(NativeUserMemoryStore::references(ctx.snapshot(),&web,refs,ref_limits).await.unwrap(),UserMemoryReferencesRead::Published{incoming:Some(p),..} if p.total==0)
    );
    assert_eq!(
        NativeUserMemoryStore::reobserve_mutation(
            ctx.snapshot(),
            &write,
            a.id,
            MemoryMutation::Update(edit),
            write_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .after,
        updated
    );
    let folded = FoldMemories {
        kind: MemoryFoldKind::Merge,
        survivor: MemoryFoldMember {
            id: a.id,
            expected_revision: updated.revision,
            idempotency_key: "interop-fold-a".into(),
        },
        members: vec![MemoryFoldMember {
            id: b.id,
            expected_revision: b.revision,
            idempotency_key: "interop-fold-b".into(),
        }],
        title: None,
        body: "preserved aggregate facts".into(),
        reason: "actual automatic after embedding metadata".into(),
    };
    let receipt = NativeUserMemoryStore::automatic_fold(
        ctx.snapshot(),
        &read,
        &fenced,
        folded.clone(),
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(receipt.removed.len(), 1);
    assert!(
        NativeUserMemoryStore::reobserve_automatic_fold(
            ctx.snapshot(),
            &read,
            &fenced,
            folded,
            write_limits()
        )
        .await
        .unwrap()
        .unwrap()
        .survivor
        .sdk_version
        .is_none()
    );
    let latest = NativeUserMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
        .await
        .unwrap()
        .unwrap()
        .memory;
    assert_eq!(latest.revision, 3);
    assert_eq!(latest.reference_generation, 2);
    let mut capture = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let documents = documents(&mut capture, &ctx, &read).await;
    assert_eq!(
        documents.iter().filter(|d| d.memory_id() == a.id).count(),
        2
    );
    assert_eq!(
        documents.iter().filter(|d| d.memory_id() == b.id).count(),
        0
    );
    assert_eq!(documents.len(), 2);
    assert!(
        documents
            .iter()
            .all(|d| d.memory_id() != a.id || d.revision() > 1)
    );
    NativeUserMemoryStore::fenced_delete(
        ctx.snapshot(),
        &fenced,
        a.id,
        DeleteMemory {
            expected_revision: 3,
            idempotency_key: "interop-final-delete".into(),
            reason: "direct conditional archive".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    assert_eq!(
        NativeUserMemoryStore::history(ctx.snapshot(), &web, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .revisions
            .len(),
        3
    );
    assert_eq!(f.server.count("/userinfo"), 0);
    assert_eq!(f.server.count("/token"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_user_disjoint_embedding_batches_publish_current_writer_pin_and_preserve_both() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::native(&ctx).await;
    f.install_native_profile(&ctx);
    let verifier = f.server.verifier(false, f.provider_revision(&ctx));
    let secret = f.secret();
    let caller = f
        .issuer
        .authenticate_runtime(&secret, &ctx)
        .unwrap()
        .authenticate_native(&verifier)
        .await
        .unwrap();
    let read = f
        .issuer
        .admit_authenticated_user_memory_read(&caller, &ctx, f.scope())
        .unwrap();
    let write = f
        .issuer
        .admit_authenticated_user_memory_write(
            &caller,
            &ctx,
            f.scope(),
            graphit_auth::UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    NativeUserMemoryStore::create(ctx.snapshot(), &write, input("batch-a"), write_limits())
        .await
        .unwrap();
    NativeUserMemoryStore::create(ctx.snapshot(), &write, input("batch-b"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let docs = documents(&mut c, &ctx, &read).await;
    assert_eq!(docs.len(), 2);
    let before = sdk::pin(&f).await.version();
    let (left, right) = tokio::join!(
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(&docs[0], vec![1., 0.])],
            write_limits()
        ),
        NativeUserMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(&docs[1], vec![0., 1.])],
            write_limits()
        )
    );
    let a = publication(left.unwrap());
    let b = publication(right.unwrap());
    assert_ne!(a.generation.sdk_version, b.generation.sdk_version);
    assert_eq!(
        a.generation.sdk_version.min(b.generation.sdk_version),
        before + 1
    );
    assert_eq!(
        a.generation.sdk_version.max(b.generation.sdk_version),
        before + 2
    );
    assert_eq!(sdk::pin(&f).await.version(), before + 2);
    assert_eq!(
        hits(
            NativeUserMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &[1., 0.],
                options(10),
                read_limits()
            )
            .await
            .unwrap()
        )
        .0,
        2
    );
}
