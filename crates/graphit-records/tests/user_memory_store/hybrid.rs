use super::*;
use graphit_core::file_lock::Cancellation;
use std::{
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    sync::Arc,
};

fn identity(config: u8) -> MemoryEmbeddingIdentity {
    MemoryEmbeddingIdentity::new(
        [config; 32],
        NonZeroU32::new(2).unwrap(),
        MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap()
}
fn options(k: usize) -> MemoryHybridSearchOptions {
    MemoryHybridSearchOptions {
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(k).unwrap(),
    }
}
fn prepared(text: &str, options: MemoryHybridSearchOptions) -> MemoryHybridQuery {
    MemoryHybridQuery::new(text, options, read_limits()).unwrap()
}
fn published(
    value: UserMemoryHybridSearchRead,
) -> (UserMemoryGeneration, usize, Vec<MemoryHybridHit>) {
    match value {
        UserMemoryHybridSearchRead::Published {
            generation,
            total_hits,
            hits,
        } => (generation, total_hits, hits),
        other => panic!("USER hybrid publication expected: {other:?}"),
    }
}
async fn cover(
    original: &Arc<graphit_core::file_io::SnapshotContext>,
    read: &graphit_auth::UserReadAdmission<'_>,
    write: &graphit_auth::UserWriteAdmission<'_>,
    vector: impl Fn(&MemoryEmbeddingDocument) -> Vec<f32>,
) {
    let UserMemoryEmbeddingCorpusRead::Published(mut corpus) =
        NativeUserMemoryStore::capture_embeddings(original, read, identity(1), read_limits())
            .await
            .unwrap()
    else {
        panic!("actual USER corpus")
    };
    assert!(!corpus.is_empty());
    let mut attachments = Vec::new();
    while let Some(documents) = corpus.next_documents(original, read).await.unwrap() {
        attachments.extend(
            documents
                .into_iter()
                .map(|document| MemoryEmbeddingAttachment {
                    memory_id: document.memory_id(),
                    revision: NonZeroU64::new(document.revision()).unwrap(),
                    input_fingerprint: *document.fingerprint(),
                    vector: vector(&document),
                }),
        );
    }
    NativeUserMemoryStore::attach_embeddings(original, write, &corpus, attachments, write_limits())
        .await
        .unwrap();
}

#[tokio::test]
async fn hybrid_user_absent_empty_zero_hits_and_two_channels_keep_user_generation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    assert_eq!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryHybridSearchRead::NoCorpus
    );
    assert!(!f.user.runtime_root().exists());
    let mut tiny = write_limits();
    tiny.max_row_bytes = NonZeroUsize::MIN;
    assert!(matches!(
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("birth"), tiny).await,
        Err(MemoryStoreError::Limit(_))
    ));
    assert!(matches!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryHybridSearchRead::PublishedEmpty { .. }
    ));
    let UserMemoryEmbeddingCorpusRead::Published(empty) =
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            identity(1),
            read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("published empty")
    };
    assert!(empty.is_empty());
    drop(empty);
    let memory =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("live"), write_limits())
            .await
            .unwrap();
    cover(ctx.snapshot(), &read, &write, |_| vec![1., 0.]).await;
    let (generation, total, hits) = published(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let detail = NativeUserMemoryStore::get(ctx.snapshot(), &read, memory.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(generation, detail.generation);
    assert_eq!(generation.owner.identity.subject(), SUBJECT);
    assert_eq!(generation.owner.identity.issuer(), f.server.origin);
    assert_eq!(generation.physical_branch, None);
    assert_eq!(generation.inherited_target, None);
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, memory);
    assert_eq!(hits[0].score, 2.0 / 61.0);
    assert_eq!(hits[0].lexical.unwrap().matched_revision, NonZeroU64::MIN);
    assert_eq!(hits[0].semantic.unwrap().distance, 0.0);
    let mut filtered = options(1);
    filtered.filter.mandatory = Some(false);
    let (_, total, hits) = published(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", filtered),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 0);
    assert!(hits.is_empty());
}

#[tokio::test]
async fn hybrid_user_complete_channels_union_before_topk_find_below_channel_winner() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let mut records = Vec::new();
    for (key, body, mandatory, important) in [
        ("a", "needle", true, false),
        ("b", "other", false, false),
        ("c", "needle", false, true),
        ("d", "needle", false, false),
        ("e", "needle", false, false),
    ] {
        let mut value = input(key);
        value.title = key.into();
        value.body = body.into();
        value.attributes.mandatory = mandatory;
        value.attributes.important = important;
        records.push(
            NativeUserMemoryStore::create(ctx.snapshot(), &write, value, write_limits())
                .await
                .unwrap(),
        );
    }
    let vectors = std::collections::HashMap::from([
        (records[0].id, vec![0., 1.]),
        (records[1].id, vec![1., 0.]),
        (records[2].id, vec![0.9, 0.1]),
        (records[3].id, vec![0.7, 0.3]),
        (records[4].id, vec![0.6, 0.4]),
    ]);
    cover(ctx.snapshot(), &read, &write, |d| {
        vectors[&d.memory_id()].clone()
    })
    .await;
    let (_, total, hits) = published(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current, records[2]);
    assert!((hits[0].score - 2.0 / 62.0).abs() < f64::EPSILON);
    assert!(hits[0].lexical.is_some() && hits[0].semantic.is_some());
    let (_, total, all) = published(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(10)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 5);
    assert_eq!(all.len(), 5);
    assert!(
        all.iter()
            .any(|h| h.current == records[1] && h.lexical.is_none() && h.semantic.is_some())
    );
}

#[tokio::test]
async fn hybrid_user_equal_rrf_scores_sort_by_literal_memory_id() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    for key in ["one", "two"] {
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input(key), write_limits())
            .await
            .unwrap();
    }
    let UserMemoryLexicalSearchRead::Published { hits: lexical, .. } =
        NativeUserMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            MemoryLexicalQuery {
                text: "quartz".into(),
                filter: MemoryListFilter::default(),
                include_deleted_history: false,
                top_k: NonZeroUsize::new(2).unwrap(),
            },
            read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("FTS")
    };
    let first = lexical[0].current.id;
    cover(ctx.snapshot(), &read, &write, |d| {
        if d.memory_id() == first {
            vec![0., 1.]
        } else {
            vec![1., 0.]
        }
    })
    .await;
    let (_, total, hits) = published(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(2)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 2);
    assert_eq!(hits[0].score, hits[1].score);
    assert!(hits[0].current.id < hits[1].current.id);
    assert_eq!(hits[0].score, 1.0 / 61.0 + 1.0 / 62.0);
}

#[tokio::test]
async fn hybrid_user_archive_channel_revisions_share_current_head_and_deleted_filters() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    let first =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("archive"), write_limits())
            .await
            .unwrap();
    let second = NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        first.id,
        update("edit", 1),
        write_limits(),
    )
    .await
    .unwrap();
    cover(ctx.snapshot(), &read, &write, |d| {
        if d.revision() == 1 {
            vec![1., 0.]
        } else {
            vec![0., 1.]
        }
    })
    .await;
    let (_, _, hits) = published(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("topaz", options(1)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(hits[0].current, second);
    assert_eq!(hits[0].lexical.unwrap().matched_revision.get(), 2);
    assert_eq!(
        hits[0].lexical.unwrap().location,
        MemoryMatchLocation::Current
    );
    assert_eq!(hits[0].semantic.unwrap().matched_revision.get(), 1);
    assert!(
        matches!(hits[0].semantic.unwrap().location,MemoryMatchLocation::Archived { revision } if revision.get()==1)
    );
    let deleted = NativeUserMemoryStore::delete(
        ctx.snapshot(),
        &write,
        first.id,
        DeleteMemory {
            expected_revision: 2,
            idempotency_key: "delete".into(),
            reason: "archive".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    let (_, total, hits) = published(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 0);
    assert!(hits.is_empty());
    let mut deleted_options = options(1);
    deleted_options.include_deleted_history = true;
    let (_, total, hits) = published(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", deleted_options),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, deleted);
    assert_eq!(hits[0].lexical.unwrap().matched_revision.get(), 1);
    assert_eq!(hits[0].semantic.unwrap().matched_revision.get(), 1);
}

#[tokio::test]
async fn hybrid_user_missing_coverage_identity_and_fts_have_causal_outcomes() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    NativeUserMemoryStore::create(ctx.snapshot(), &write, input("cause"), write_limits())
        .await
        .unwrap();
    assert!(matches!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryHybridSearchRead::Unavailable {
            reason: MemoryEmbeddingUnavailable::MissingCoverage,
            ..
        }
    ));
    cover(ctx.snapshot(), &read, &write, |_| vec![1., 0.]).await;
    assert!(matches!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryHybridSearchRead::Published { total_hits: 1, .. }
    ));
    assert!(matches!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(2),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        UserMemoryHybridSearchRead::Unavailable {
            reason: MemoryEmbeddingUnavailable::IncompatibleIdentity,
            ..
        }
    ));
    for vector in [&[1.][..], &[f32::NAN, 0.][..]] {
        assert!(matches!(
            NativeUserMemoryStore::search_hybrid(
                ctx.snapshot(),
                &read,
                identity(1),
                prepared("quartz", options(1)),
                vector,
                read_limits()
            )
            .await,
            Err(MemoryStoreError::Invalid(_))
        ));
    }
}

#[tokio::test]
async fn hybrid_user_foreign_original_owner_and_late_corruption_are_errors() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, write);
    NativeUserMemoryStore::create(ctx.snapshot(), &write, input("integrity"), write_limits())
        .await
        .unwrap();
    let foreign = request(&cancel);
    assert!(matches!(
        NativeUserMemoryStore::search_hybrid(
            foreign.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    let good = integrity::physical_rows(&f).await;
    let mut foreign_owner = good.clone();
    foreign_owner[0] = integrity::replace_text(&good[0], "user_subject", "another principal");
    integrity::replace(&f, foreign_owner).await;
    assert!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .is_err()
    );
    let mut late = good.clone();
    late.push(integrity::replace_text(&good[0], "payload", "{broken"));
    integrity::replace(&f, late).await;
    assert!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .is_err()
    );
    cancel.cancel();
    assert!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn hybrid_user_guard_retired_provider_and_physical_scope_never_return_healthy_empty() {
    for retired in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx).await;
        incoming!(f, ctx, verifier, proof, read, write);
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("retire"), write_limits())
            .await
            .unwrap();
        if retired {
            let mutation = f
                .store
                .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                    let p = doc.provider("service").unwrap();
                    let rev = p.revision();
                    let at = p.updated_at() + 1;
                    let config = p.config().clone();
                    doc.update_provider("service", config, doc.revision(), rev, at, ctx)
                })
                .unwrap();
            assert!(mutation.failure().is_none());
        } else {
            let root = f.user.user_root().unwrap();
            std::fs::rename(root, root.with_extension("replaced")).unwrap();
            std::fs::create_dir(root).unwrap();
        }
        assert!(
            NativeUserMemoryStore::search_hybrid(
                ctx.snapshot(),
                &read,
                identity(1),
                prepared("quartz", options(1)),
                &[1., 0.],
                read_limits()
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn hybrid_user_guard_expired_original_has_no_corpus_io() {
    let cancel = Cancellation::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let ctx = request_until(&cancel, deadline);
    let f = Fixture::new(&ctx).await;
    incoming!(f, ctx, verifier, proof, read, _write);
    tokio::time::sleep(
        deadline.saturating_duration_since(std::time::Instant::now())
            + std::time::Duration::from_millis(20),
    )
    .await;
    assert!(
        NativeUserMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("quartz", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .is_err()
    );
    assert!(!f.user.runtime_root().exists());
}
