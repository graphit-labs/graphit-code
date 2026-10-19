#![cfg(target_os = "linux")]
#[path = "memory_store/fixture.rs"]
mod fixture;
#[path = "memory_store/embeddings_sdk.rs"]
mod sdk;
use fixture::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation};
use graphit_core::{
    file_lock::Cancellation,
    version_context::{Checkout, VersionContext},
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store as memory_api,
    memory_store::*,
};
use std::{
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};

macro_rules! admissions {
    ($f:ident,$ctx:ident,$issuer:ident,$authority:ident,$grant:ident,$read:ident,$write:ident,$bearer:ident,$reference:ident,$admin:ident) => {
        let $issuer = $f.issuer(&$ctx);
        let ($bearer, $reference, _run, $admin) = $f.issue(&$issuer, &$ctx);
        let $grant = $issuer
            .verify(
                $bearer.expose_for_transport(),
                &$reference,
                $f.scope(),
                &$ctx,
            )
            .unwrap();
        let $authority = ProjectRecordAuthority::new(&$issuer);
        let $read = $authority
            .admit_read(&$grant, &$ctx, ProjectReadModule::Memory)
            .unwrap();
        let $write = $authority
            .admit_write(&$grant, &$ctx, ProjectWriteOperation::Memory)
            .unwrap();
    };
}
fn input(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: format!(" Title {key} \n"),
        body: " literal body \nβ".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: false,
            important: false,
        },
        tags: vec!["literal tag".into()],
        references: None,
    }
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
fn corpus(value: MemoryEmbeddingCorpusRead) -> MemoryEmbeddingCorpus {
    match value {
        MemoryEmbeddingCorpusRead::Published(c) => c,
        _ => panic!("published capture"),
    }
}
fn attachment(document: &MemoryEmbeddingDocument, vector: Vec<f32>) -> MemoryEmbeddingAttachment {
    MemoryEmbeddingAttachment {
        memory_id: document.memory_id(),
        revision: NonZeroU64::new(document.revision()).unwrap(),
        input_fingerprint: *document.fingerprint(),
        vector,
    }
}
fn publication(value: MemoryEmbeddingAttachOutcome) -> MemoryEmbeddingPublication {
    match value {
        MemoryEmbeddingAttachOutcome::Published(p) => p,
        _ => panic!("actual publication"),
    }
}
fn hits(value: MemorySemanticSearchRead) -> (usize, Vec<MemorySemanticHit>) {
    match value {
        MemorySemanticSearchRead::Published {
            total_hits, hits, ..
        } => (total_hits, hits),
        other => panic!("hits expected: {other:?}"),
    }
}
fn update(memory: &Memory, key: &str, body: &str) -> UpdateMemory {
    UpdateMemory {
        expected_revision: memory.revision,
        idempotency_key: key.into(),
        reason: "literal reason".into(),
        title: None,
        body: Some(body.into()),
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    }
}

fn assert_reference_rows(batch: &arrow_array::RecordBatch, expected: &[(MemoryId, u64)]) {
    assert_eq!(batch.schema().fields().len(), 24);
    assert_eq!(
        batch
            .schema()
            .metadata()
            .get("graphit.memory.layout")
            .map(String::as_str),
        Some("project-v2-embedding-list")
    );
    let mut actual = Vec::new();
    for row in 0..batch.num_rows() {
        if sdk::string(batch, 1, row) == "references" {
            assert!(
                batch
                    .columns()
                    .iter()
                    .skip(18)
                    .all(|column| column.is_null(row))
            );
            let id: MemoryId = sdk::string(batch, 2, row).parse().unwrap();
            let revision = batch
                .column(8)
                .as_any()
                .downcast_ref::<arrow_array::UInt64Array>()
                .unwrap()
                .value(row);
            actual.push((id, revision));
        }
    }
    actual.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn no_corpus_empty_eof_and_query_states_have_no_birth() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    assert!(matches!(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
        MemoryEmbeddingCorpusRead::NoCorpus
    ));
    assert_eq!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits()
        )
        .await
        .unwrap(),
        MemorySemanticSearchRead::NoCorpus
    );
    assert!(!f.project.runtime_root().join("memory").exists());
    sdk::empty_with_schema(&f, &ctx, sdk::schema()).await;
    let old = sdk::pin(&f).await;
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    assert!(
        c.next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits()
        )
        .await
        .unwrap(),
        MemorySemanticSearchRead::PublishedEmpty { .. }
    ));
    assert!(
        NativeMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, vec![], write_limits())
            .await
            .is_err()
    );
    assert_eq!(sdk::pin(&f).await.version(), old.version());
}

#[tokio::test]
async fn chunk_partial_coverage_complete_existing_key_and_live_logical_history() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let a2 = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        update(&a, "u", "new literal"),
        write_limits(),
    )
    .await
    .unwrap();
    let b = NativeMemoryStore::create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    let old = sdk::pin(&f).await;
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::new(1).unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), limits)
            .await
            .unwrap(),
    );
    let first = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.len(), 1);
    let p1 = publication(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(&first[0], vec![0., 1.])],
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(p1.recovery_key.base_version(), old.version());
    assert!(matches!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(10),
            read_limits()
        )
        .await
        .unwrap(),
        MemorySemanticSearchRead::Published { total_hits: 1, .. }
    ));
    let mut docs = first;
    while let Some(chunk) = c.next_documents(ctx.snapshot(), &read).await.unwrap() {
        assert_eq!(chunk.len(), 1);
        docs.extend(chunk);
    }
    assert_eq!(docs.len(), 3);
    assert_eq!(docs.iter().filter(|d| d.memory_id() == a.id).count(), 2);
    let batches = docs.iter().map(|d| attachment(d, vec![1., 0.])).collect();
    let p2 = publication(
        NativeMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, batches, write_limits())
            .await
            .unwrap(),
    );
    assert_eq!(p2.recovery_key.base_version(), p1.generation.sdk_version);
    assert_eq!(
        NativeMemoryStore::resolve_embeddings(
            ctx.snapshot(),
            &write,
            &p2.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        graphit_data::lance_branch_store::DeltaResolution::Applied {
            version: p2.generation.sdk_version
        }
    );
    let same = docs.iter().map(|d| attachment(d, vec![1., 0.])).collect();
    assert_eq!(
        NativeMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, same, write_limits())
            .await
            .unwrap(),
        MemoryEmbeddingAttachOutcome::Existing {
            generation: p2.generation.clone()
        }
    );
    let (total, result) = hits(
        NativeMemoryStore::search_semantic(
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
    assert_eq!(result[0].distance, 0.);
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        a2
    );
    let heads = match NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
        .await
        .unwrap()
    {
        MemoryHeadRead::Published { generation, heads } => {
            assert_eq!(generation, p2.generation);
            heads
        }
        _ => panic!("published heads"),
    };
    assert_eq!(heads.len(), 2);
    assert!(
        heads
            .iter()
            .any(|head| head.id == a2.id && head.title == a2.title)
    );
    assert!(
        heads
            .iter()
            .any(|head| head.id == b.id && head.title == b.title)
    );
    let physical = sdk::all(&old).await;
    assert_eq!(physical.num_rows(), 8); // 2 heads,2 creations,1revision,3 actual reference markers.
    assert_reference_rows(&physical, &[(a.id, 1), (a.id, 2), (b.id, 1)]);
    assert!((0..physical.num_rows()).all(|row| sdk::string(&physical, 1, row) != "embedding"));
}

#[tokio::test]
async fn best_archive_beyond_cap_current_tie_high_revision_and_head_filters() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let a2 = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        update(&a, "u2", "second"),
        write_limits(),
    )
    .await
    .unwrap();
    let a3 = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        update(&a2, "u3", "third"),
        write_limits(),
    )
    .await
    .unwrap();
    let b = NativeMemoryStore::create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let docs = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
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
    NativeMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, values, write_limits())
        .await
        .unwrap();
    let (total, result) = hits(
        NativeMemoryStore::search_semantic(
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
    assert_eq!(result[0].current, a3);
    assert_eq!(result[0].matched_revision.get(), 2);
    assert_eq!(
        result[0].location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::new(2).unwrap()
        }
    );
    // All predicates are on the current head, even when its archive is the best score.
    for filter in [
        MemoryListFilter {
            classification: Some(MemoryClass::Fact),
            ..Default::default()
        },
        MemoryListFilter {
            tag: Some("other".into()),
            ..Default::default()
        },
        MemoryListFilter {
            mandatory: Some(true),
            ..Default::default()
        },
        MemoryListFilter {
            important: Some(true),
            ..Default::default()
        },
    ] {
        let mut opt = options(1);
        opt.filter = filter;
        assert_eq!(
            hits(
                NativeMemoryStore::search_semantic(
                    ctx.snapshot(),
                    &read,
                    identity(1, 2),
                    &[1., 0.],
                    opt,
                    read_limits()
                )
                .await
                .unwrap()
            )
            .0,
            0
        );
    }
    let mut changed = update(&a3, "meta", "third");
    changed.body = None;
    changed.mandatory = Some(true);
    changed.important = Some(true);
    changed.tags = Some(vec!["other".into()]);
    NativeMemoryStore::update(ctx.snapshot(), &write, a.id, changed, write_limits())
        .await
        .unwrap();
    let mut opt = options(10);
    opt.filter.exclude_mandatory = true;
    let (_, result) = hits(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            opt,
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].current.id, b.id);
    assert!(matches!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits()
        )
        .await
        .unwrap(),
        MemorySemanticSearchRead::Published { total_hits: 2, .. }
    ));
}

#[tokio::test]
async fn concurrent_source_update_fresh_capture_and_config_dimension_recompute() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let doc = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    let a2 = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        a.id,
        update(&a, "u", "newsource"),
        write_limits(),
    )
    .await
    .unwrap();
    NativeMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        vec![attachment(&doc, vec![1., 0.])],
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            read_limits()
        )
        .await
        .unwrap(),
        MemorySemanticSearchRead::Published { total_hits: 1, .. }
    ));
    for (config, dim) in [(1, 2), (2, 2), (2, 3), (3, 3)] {
        let ident = identity(config, dim);
        let mut fresh = corpus(
            NativeMemoryStore::capture_embeddings(
                ctx.snapshot(),
                &read,
                ident.clone(),
                read_limits(),
            )
            .await
            .unwrap(),
        );
        let docs = fresh
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(docs.len(), if config == 1 { 1 } else { 2 });
        if config == 1 {
            assert_eq!(docs[0].revision(), a2.revision);
        }
        let mut vector = vec![0.; dim as usize];
        vector[0] = 1.;
        let values = docs.iter().map(|d| attachment(d, vector.clone())).collect();
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &fresh,
            values,
            write_limits(),
        )
        .await
        .unwrap();
        let (_, result) = hits(
            NativeMemoryStore::search_semantic(
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
        assert_eq!(result[0].current, a2);
        assert_eq!(result[0].location, MemoryMatchLocation::Current);
        if config > 1 {
            assert!(matches!(
                NativeMemoryStore::search_semantic(
                    ctx.snapshot(),
                    &read,
                    identity(1, 2),
                    &[1., 0.],
                    options(1),
                    read_limits()
                )
                .await
                .unwrap(),
                MemorySemanticSearchRead::Unavailable {
                    reason: MemoryEmbeddingUnavailable::IncompatibleIdentity,
                    ..
                }
            ));
        }
    }
}

#[tokio::test]
async fn deleted_history_emits_no_tombstone_duplicate_and_is_explicit_only() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let deleted = NativeMemoryStore::delete(
        ctx.snapshot(),
        &write,
        a.id,
        DeleteMemory {
            expected_revision: 1,
            idempotency_key: "delete".into(),
            reason: "literal delete".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let docs = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].revision(), 1);
    assert!(
        c.next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    // An excluded deleted record does not demand embedding coverage.
    assert_eq!(
        hits(
            NativeMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &[1., 0.],
                options(1),
                read_limits()
            )
            .await
            .unwrap()
        )
        .0,
        0
    );
    let mut opt = options(1);
    opt.include_deleted_history = true;
    assert!(matches!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            opt.clone(),
            read_limits()
        )
        .await
        .unwrap(),
        MemorySemanticSearchRead::Unavailable { .. }
    ));
    NativeMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        vec![attachment(&docs[0], vec![1., 0.])],
        write_limits(),
    )
    .await
    .unwrap();
    let (_, result) = hits(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            opt,
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(result[0].current, deleted);
    assert_eq!(
        result[0].location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::new(1).unwrap()
        }
    );
    assert!(
        NativeMemoryStore::get(ctx.snapshot(), &read, a.id, read_limits())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn attachment_shape_binding_unemitted_duplicate_and_budget_fail_before_effect() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let old = sdk::pin(&f).await;
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let doc = prepare_memory_embedding_document(&a, &identity(1, 2)).unwrap();
    assert!(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits()
        )
        .await
        .is_err()
    );
    let doc = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    let good = attachment(&doc, vec![1., 0.]);
    let mut forged = good.clone();
    forged.input_fingerprint[0] ^= 1;
    let mut missing = good.clone();
    missing.revision = NonZeroU64::new(2).unwrap();
    for bad in [
        forged,
        missing,
        attachment(&doc, vec![1.]),
        attachment(&doc, vec![f32::NAN, 0.]),
        attachment(&doc, vec![f32::INFINITY, 0.]),
    ] {
        assert!(
            NativeMemoryStore::attach_embeddings(
                ctx.snapshot(),
                &write,
                &c,
                vec![bad],
                write_limits()
            )
            .await
            .is_err()
        );
    }
    assert!(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![good.clone(), good.clone()],
            write_limits()
        )
        .await
        .is_err()
    );
    let mut limits = write_limits();
    limits.max_input_bytes = NonZeroUsize::new(63).unwrap();
    assert!(matches!(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![good.clone()],
            limits
        )
        .await,
        Err(MemoryStoreError::Limit("embedding input bytes"))
    ));
    limits.max_input_bytes = NonZeroUsize::new(64).unwrap();
    assert_eq!(sdk::pin(&f).await.version(), old.version());
    NativeMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, vec![good], limits)
        .await
        .unwrap();
    for vector in [vec![1.], vec![f32::NAN, 0.]] {
        assert!(
            NativeMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &vector,
                options(1),
                read_limits()
            )
            .await
            .is_err()
        );
    }
    assert!(matches!(
        NativeMemoryStore::search_semantic(
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
async fn original_wrong_admission_revocation_cancellation_and_fresh_request_require_new_capture() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let doc = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    let other_cancel = Cancellation::default();
    let fresh = request(&other_cancel);
    let freshgrant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &fresh)
        .unwrap();
    let freshread = authority
        .admit_read(&freshgrant, &fresh, ProjectReadModule::Memory)
        .unwrap();
    let freshwrite = authority
        .admit_write(&freshgrant, &fresh, ProjectWriteOperation::Memory)
        .unwrap();
    assert!(matches!(
        c.next_documents(fresh.snapshot(), &freshread).await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    assert!(matches!(
        NativeMemoryStore::attach_embeddings(
            fresh.snapshot(),
            &freshwrite,
            &c,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits()
        )
        .await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    let mut cnew = corpus(
        NativeMemoryStore::capture_embeddings(
            fresh.snapshot(),
            &freshread,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        cnew.next_documents(fresh.snapshot(), &freshread)
            .await
            .unwrap()
            .unwrap()
            .len(),
        1
    );
    let wrongread = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    let wrongwrite = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::FencedMemory)
        .unwrap();
    assert!(matches!(
        c.next_documents(ctx.snapshot(), &wrongread).await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    assert!(matches!(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &wrongwrite,
            &c,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    other_cancel.cancel();
    assert!(
        cnew.next_documents(fresh.snapshot(), &freshread)
            .await
            .is_err()
    );
    let secret =
        std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
    let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(c.next_documents(ctx.snapshot(), &read).await.is_err());
    assert!(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits()
        )
        .await
        .is_err()
    );
    // A fresh bearer restores ordinary authority, but never renews a different Original.
    let (renewed, _run2) = f.issue_registered_for(&issuer, &ctx, f.scope(), &reference);
    let renewedgrant = issuer
        .verify(renewed.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let renewedread = authority
        .admit_read(&renewedgrant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let deadline_cancel = Cancellation::default();
    let deadline = request_until(&deadline_cancel, Instant::now() + Duration::from_secs(5));
    let dg = issuer
        .verify(
            renewed.expose_for_transport(),
            &reference,
            f.scope(),
            &deadline,
        )
        .unwrap();
    let dr = authority
        .admit_read(&dg, &deadline, ProjectReadModule::Memory)
        .unwrap();
    let mut dc = corpus(
        NativeMemoryStore::capture_embeddings(
            deadline.snapshot(),
            &dr,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        deadline
            .snapshot()
            .deadline()
            .expect("finite fixture deadline")
            + Duration::from_millis(10),
    ))
    .await;
    assert!(dc.next_documents(deadline.snapshot(), &dr).await.is_err());
    assert!(
        NativeMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &renewedread,
            identity(1, 2),
            read_limits()
        )
        .await
        .is_ok()
    );
}

#[tokio::test]
async fn complete_embedding_eof_and_repeated_reads_are_independent_of_query_bound() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let doc = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    NativeMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        vec![attachment(&doc, vec![1., 0.])],
        write_limits(),
    )
    .await
    .unwrap();
    let all = sdk::all(&sdk::pin(&f).await).await;
    assert_eq!(all.num_rows(), 4); // head,creation,reference marker,derived embedding.
    assert_reference_rows(&all, &[(doc.memory_id(), 1)]);
    assert_eq!(
        (0..all.num_rows())
            .filter(|row| sdk::string(&all, 1, *row) == "embedding")
            .count(),
        1
    );
    let mut limits = read_limits();
    limits.max_query_bytes = NonZeroUsize::MIN;
    limits.batch_rows = NonZeroU32::new(1).unwrap();
    let mut exact = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), limits)
            .await
            .unwrap(),
    );
    assert!(
        exact
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        exact
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        hits(
            NativeMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                identity(1, 2),
                &[1., 0.],
                options(1),
                limits
            )
            .await
            .unwrap()
        )
        .0,
        1
    );
}

#[tokio::test]
async fn inherited_existing_does_not_birth_publish_isolates_branch_and_recovery_binding() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let mut f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let doc = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    let mainpub = publication(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let mainpin = sdk::pin(&f).await;
    drop(write);
    drop(read);
    drop(grant);
    drop(issuer);
    let paths = graphit_core::project_paths::ProjectPaths::at(
        &f.root,
        &graphit_core::project_paths::InstallationPaths::resolve(
            f.installation.brand().clone(),
            &f.root,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap(),
    )
    .unwrap();
    f.version =
        VersionContext::select(&paths, Checkout::NonGit, "feature/semantic", None, "").unwrap();
    admissions!(
        f, ctx, issuer2, authority2, grant2, read2, write2, bearer2, reference2, _admin2
    );
    let mut named = corpus(
        NativeMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read2,
            identity(1, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert!(named.generation().inherited_target.is_some());
    assert!(
        named
            .next_documents(ctx.snapshot(), &read2)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write2,
            &named,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits()
        )
        .await,
        Err(MemoryStoreError::Invalid(
            "unemitted Memory embedding attachment"
        ))
    ));
    assert!(sdk::pin(&f).await.inherited_target().is_some());
    assert!(
        NativeMemoryStore::resolve_embeddings(
            ctx.snapshot(),
            &write2,
            &mainpub.recovery_key,
            write_limits()
        )
        .await
        .is_err()
    );
    // A new model identity legitimately requires a new derived row on this branch.
    let mut changed = corpus(
        NativeMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read2,
            identity(2, 2),
            read_limits(),
        )
        .await
        .unwrap(),
    );
    let ndoc = changed
        .next_documents(ctx.snapshot(), &read2)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    let npub = publication(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write2,
            &changed,
            vec![attachment(&ndoc, vec![0., 1.])],
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert!(npub.generation.inherited_target.is_none());
    assert!(npub.generation.physical_branch.is_some());
    assert_eq!(
        npub.recovery_key.physical_branch(),
        npub.generation.physical_branch.as_deref()
    );
    assert_eq!(
        hits(
            NativeMemoryStore::search_semantic(
                ctx.snapshot(),
                &read2,
                identity(2, 2),
                &[1., 0.],
                options(1),
                read_limits()
            )
            .await
            .unwrap()
        )
        .1[0]
            .distance,
        1.
    );
    assert_eq!(mainpin.version(), mainpub.generation.sdk_version);
    let values = sdk::all(&mainpin).await;
    assert_eq!(values.num_rows(), 4); // main head,creation,reference marker,derived embedding.
    assert_reference_rows(&values, &[(a.id, 1)]);
    assert_eq!(
        (0..values.num_rows())
            .filter(|row| sdk::string(&values, 1, *row) == "embedding")
            .count(),
        1
    );
    assert_eq!(
        NativeMemoryStore::get(ctx.snapshot(), &read2, a.id, read_limits())
            .await
            .unwrap()
            .unwrap()
            .memory,
        a
    );
}

#[tokio::test]
async fn global_project_corpus_reopens_text_history_and_vectors_from_another_checkout() {
    use graphit_auth::{ProjectCapture, ScopeCapture};
    use graphit_core::{
        artifact_store::{CapturedPublicationScope, ScopeLimits, StoreOwner},
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::{InstallationPaths, ProjectPaths},
    };

    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let created = NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("global-checkout"),
        write_limits(),
    )
    .await
    .unwrap();
    let current = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        created.id,
        update(&created, "global-edit", "globalcheckout revision two"),
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeMemoryStore::search_lexical(
            ctx.snapshot(),
            &read,
            MemoryLexicalQuery {
                text: "globalcheckout".into(),
                filter: MemoryListFilter::default(),
                include_deleted_history: false,
                top_k: NonZeroUsize::new(1).unwrap(),
            },
            read_limits(),
        )
        .await
        .unwrap(),
        MemoryLexicalSearchRead::Published { hits, .. }
            if hits.len() == 1 && hits[0].current == current
    ));
    let mut captured = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let documents = captured
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(documents.len(), 2);
    let published = publication(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &captured,
            documents
                .iter()
                .map(|document| attachment(document, vec![1., 0.]))
                .collect(),
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let main_pin = sdk::pin(&f).await;
    assert_eq!(published.recovery_key.corpus_uri(), main_pin.corpus_uri());
    let history = NativeMemoryStore::history(ctx.snapshot(), &read, current.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(history.creation.memory, created);
    assert_eq!(history.current, current);
    assert_eq!(history.revisions.len(), 1);

    let StoreOwner::Project { project_id } = f.project.owner() else {
        panic!("PROJECT fixture required")
    };
    for selected_id in [*project_id, "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap()] {
        let checkout = f
            .root
            .parent()
            .unwrap()
            .join(format!("other checkout β {selected_id}"));
        std::fs::create_dir_all(checkout.join(f.installation.brand().directory())).unwrap();
        let installation = InstallationPaths::resolve(
            f.installation.brand().clone(),
            &checkout,
            None,
            Some(f.installation.owner_root()),
        )
        .unwrap();
        let paths = ProjectPaths::at(&checkout, &installation).unwrap();
        ProjectIdentityStore::at(&paths)
            .writer(Duration::from_secs(5), &cancel)
            .unwrap()
            .ensure_with_id(
                &ProjectMetadata::new("another-checkout").unwrap(),
                selected_id,
            )
            .unwrap();
        let project = CapturedPublicationScope::project(
            &paths,
            selected_id,
            ScopeLimits::new(16, 4096, 128).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let record = project.binding().unwrap();
        let version = VersionContext::select(&paths, Checkout::NonGit, "main", None, "").unwrap();
        let mut scope = f.scope();
        scope.project = Some(ProjectCapture {
            scope: ScopeCapture {
                live: &project,
                record: &record,
            },
            version: &version,
        });
        // Admit this exact checkout through the authenticated runtime API.
        let secret =
            std::fs::read_to_string(f.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, &ctx).unwrap();
        let admin = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        assert!(matches!(
            issuer.start_run(&parent, &scope, Duration::from_secs(3600)),
            Err(graphit_auth::AuthError::Denied)
        ));
        issuer
            .trust_project_root(&caller, &ctx, scope.project.unwrap().scope)
            .unwrap();
        let (selected_bearer, _run) = f.issue_registered_for(&issuer, &ctx, scope, &reference);
        let selected_grant = issuer
            .verify(
                selected_bearer.expose_for_transport(),
                &reference,
                scope,
                &ctx,
            )
            .unwrap();
        let selected_read = authority
            .admit_read(&selected_grant, &ctx, ProjectReadModule::Memory)
            .unwrap();
        assert_eq!(
            project.runtime_root(),
            installation
                .root()
                .join("projects")
                .join(selected_id.to_string())
        );
        if selected_id == *project_id {
            assert_eq!(project.runtime_root(), f.project.runtime_root());
            assert_eq!(
                NativeMemoryStore::history(
                    ctx.snapshot(),
                    &selected_read,
                    current.id,
                    read_limits(),
                )
                .await
                .unwrap()
                .unwrap(),
                history
            );
            let (total, found) = hits(
                NativeMemoryStore::search_semantic(
                    ctx.snapshot(),
                    &selected_read,
                    identity(1, 2),
                    &[1., 0.],
                    options(1),
                    read_limits(),
                )
                .await
                .unwrap(),
            );
            assert_eq!(total, 1);
            assert_eq!(found[0].current, current);
            let mut reused = corpus(
                NativeMemoryStore::capture_embeddings(
                    ctx.snapshot(),
                    &selected_read,
                    identity(1, 2),
                    read_limits(),
                )
                .await
                .unwrap(),
            );
            assert!(
                reused
                    .next_documents(ctx.snapshot(), &selected_read)
                    .await
                    .unwrap()
                    .is_none()
            );
        } else {
            assert!(matches!(
                NativeMemoryStore::capture_embeddings(
                    ctx.snapshot(),
                    &selected_read,
                    identity(1, 2),
                    read_limits(),
                )
                .await
                .unwrap(),
                MemoryEmbeddingCorpusRead::NoCorpus
            ));
            assert!(!project.runtime_root().join("memory").exists());
        }
        assert!(!paths.brand_directory().join("runtime/memory").exists());
    }
    assert!(
        !f.root
            .join(f.installation.brand().directory())
            .join("runtime/memory")
            .exists()
    );
    assert_eq!(sdk::pin(&f).await.version(), main_pin.version());
}

#[tokio::test]
async fn late_forged_source_orphan_foreign_and_duplicate_are_errors_not_partial_hits() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let doc = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    NativeMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        vec![attachment(&doc, vec![1., 0.])],
        write_limits(),
    )
    .await
    .unwrap();
    let pin = sdk::pin(&f).await;
    let all = sdk::all(&pin).await;
    let erow = (0..all.num_rows())
        .find(|r| sdk::string(&all, 1, *r) == "embedding")
        .unwrap();
    let embedding = all.slice(erow, 1);
    let mut columns = embedding.columns().to_vec();
    columns[19] = Arc::new(
        arrow_array::FixedSizeBinaryArray::try_from_iter([[0u8; 32].as_slice()].into_iter())
            .unwrap(),
    );
    let forged = arrow_array::RecordBatch::try_new(embedding.schema(), columns).unwrap();
    sdk::replace(&pin, forged).await;
    let mut small = read_limits();
    small.batch_rows = NonZeroU32::new(1).unwrap();
    assert!(matches!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            small
        )
        .await,
        Err(MemoryStoreError::Invalid(
            "Memory embedding source fingerprint"
        ))
    ));
    let mut apparently_covered = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), small)
            .await
            .unwrap(),
    );
    assert!(matches!(
        apparently_covered
            .next_documents(ctx.snapshot(), &read)
            .await,
        Err(MemoryStoreError::Invalid(
            "Memory embedding source fingerprint"
        ))
    ));
    // Restore the valid row, then inject another physical identity with no logical source.
    sdk::replace(&sdk::pin(&f).await, embedding.clone()).await;
    let mut orphan = a.clone();
    orphan.id = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
    let odoc = prepare_memory_embedding_document(&orphan, &identity(1, 2)).unwrap();
    let orphanrow = encode_memory_embedding_row(
        "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
        &odoc,
        &[1., 0.],
        write_limits(),
    )
    .unwrap();
    sdk::replace(&sdk::pin(&f).await, orphanrow).await;
    assert!(matches!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(1),
            small
        )
        .await,
        Err(MemoryStoreError::Invalid("orphan Memory embedding"))
    ));
    let mut orphan_capture = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), small)
            .await
            .unwrap(),
    );
    assert!(matches!(
        orphan_capture.next_documents(ctx.snapshot(), &read).await,
        Err(MemoryStoreError::Invalid("orphan Memory embedding"))
    ));
    let foreign = sdk::text_column(&embedding, 3, vec!["01KSH1CRFFG8Z74B5ZS78WW809".into()]);
    sdk::replace(&sdk::pin(&f).await, foreign).await;
    assert!(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), small)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn physical_duplicates_and_late_shape_corruption_never_return_prefix() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("source"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let doc = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    NativeMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &c,
        vec![attachment(&doc, vec![1., 0.])],
        write_limits(),
    )
    .await
    .unwrap();
    let all = sdk::all(&sdk::pin(&f).await).await;
    let row = (0..all.num_rows())
        .find(|r| sdk::string(&all, 1, *r) == "embedding")
        .unwrap();
    let embedding = all.slice(row, 1);
    let mut cols = embedding.columns().to_vec();
    cols[22] = Arc::new(arrow_array::UInt32Array::from(vec![Some(3)]));
    sdk::replace(
        &sdk::pin(&f).await,
        arrow_array::RecordBatch::try_new(embedding.schema(), cols).unwrap(),
    )
    .await;
    assert!(
        NativeMemoryStore::search_semantic(
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
    let duplicate = Fixture::new(&ctx, "main");
    let i = duplicate.issuer(&ctx);
    let (b, r, _, _) = duplicate.issue(&i, &ctx);
    let g = i
        .verify(b.expose_for_transport(), &r, duplicate.scope(), &ctx)
        .unwrap();
    let authority2 = ProjectRecordAuthority::new(&i);
    let read2 = authority2
        .admit_read(&g, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let _writer = sdk::directory_only(&duplicate, &ctx).await;
    let uri = url::Url::from_file_path(duplicate.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    graphit_data::lance_module_store::LanceModuleStore::create(
        &uri,
        sdk::schema(),
        vec![all.clone(), all],
    )
    .await
    .unwrap();
    let mut small = read_limits();
    small.batch_rows = NonZeroU32::new(1).unwrap();
    assert!(matches!(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read2, identity(1, 2), small).await,
        Err(MemoryStoreError::Invalid("duplicate Memory row key"))
    ));
    assert!(matches!(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read2,
            identity(1, 2),
            &[1., 0.],
            options(1),
            small
        )
        .await,
        Err(MemoryStoreError::Invalid("duplicate Memory row key"))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_disjoint_batches_take_current_writer_pin_and_preserve_both_vectors() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("a"), write_limits())
        .await
        .unwrap();
    let b = NativeMemoryStore::create(ctx.snapshot(), &write, input("b"), write_limits())
        .await
        .unwrap();
    let mut c = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let docs = c
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(docs.len(), 2);
    let aa = attachment(
        docs.iter().find(|d| d.memory_id() == a.id).unwrap(),
        vec![1., 0.],
    );
    let bb = attachment(
        docs.iter().find(|d| d.memory_id() == b.id).unwrap(),
        vec![0., 1.],
    );
    let (first, second) = tokio::join!(
        NativeMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, vec![aa], write_limits()),
        NativeMemoryStore::attach_embeddings(ctx.snapshot(), &write, &c, vec![bb], write_limits())
    );
    let first = publication(first.unwrap());
    let second = publication(second.unwrap());
    let (earlier, later) = if first.generation.sdk_version < second.generation.sdk_version {
        (&first, &second)
    } else {
        (&second, &first)
    };
    assert_eq!(
        later.recovery_key.base_version(),
        earlier.generation.sdk_version
    );
    let (total, results) = hits(
        NativeMemoryStore::search_semantic(
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
    assert_eq!(total, 2);
    assert_eq!(results[0].current.id, a.id);
    assert_eq!(results[0].distance, 0.);
    assert_eq!(results[1].current.id, b.id);
    assert_eq!(results[1].distance, 1.);
}

#[tokio::test]
async fn fresh_capture_omits_complete_coverage_and_preserves_emitted_allowlist() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    NativeMemoryStore::create(ctx.snapshot(), &write, input("covered"), write_limits())
        .await
        .unwrap();
    let mut initial = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let doc = initial
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap()
        .remove(0);
    let published = publication(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &initial,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let mut covered = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    assert_eq!(
        covered.generation().sdk_version,
        published.generation.sdk_version
    );
    assert!(
        covered
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        covered
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &covered,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits(),
        )
        .await,
        Err(MemoryStoreError::Invalid(
            "unemitted Memory embedding attachment"
        ))
    ));
    assert_eq!(
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &initial,
            vec![attachment(&doc, vec![1., 0.])],
            write_limits(),
        )
        .await
        .unwrap(),
        MemoryEmbeddingAttachOutcome::Existing {
            generation: published.generation.clone()
        },
    );
    assert_eq!(
        sdk::pin(&f).await.version(),
        published.generation.sdk_version
    );
    cancel.cancel();
    assert!(covered.next_documents(ctx.snapshot(), &read).await.is_err());
}

#[tokio::test]
async fn partial_coverage_advances_batch_one_and_emits_new_revision_with_same_fingerprint() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    for key in ["covered-first", "missing-middle", "covered-last"] {
        NativeMemoryStore::create(ctx.snapshot(), &write, input(key), write_limits())
            .await
            .unwrap();
    }
    let mut initial = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), read_limits())
            .await
            .unwrap(),
    );
    let docs = initial
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(docs.len(), 3);
    NativeMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &initial,
        vec![
            attachment(&docs[0], vec![1., 0.]),
            attachment(&docs[2], vec![1., 0.]),
        ],
        write_limits(),
    )
    .await
    .unwrap();
    let mut small = read_limits();
    small.batch_rows = NonZeroU32::new(1).unwrap();
    let mut partial = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), small)
            .await
            .unwrap(),
    );
    let only = partial
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(
        (only[0].memory_id(), only[0].revision()),
        (docs[1].memory_id(), 1)
    );
    assert!(
        partial
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    let source = NativeMemoryStore::get(ctx.snapshot(), &read, docs[0].memory_id(), read_limits())
        .await
        .unwrap()
        .unwrap()
        .memory;
    let mut flags = update(&source, "flag-only-revision", &source.body);
    flags.mandatory = Some(!source.attributes.mandatory);
    let updated =
        NativeMemoryStore::update(ctx.snapshot(), &write, source.id, flags, write_limits())
            .await
            .unwrap();
    let projected = prepare_memory_embedding_document(&updated, &identity(1, 2)).unwrap();
    assert_eq!(projected.fingerprint(), docs[0].fingerprint());
    let mut fresh = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), small)
            .await
            .unwrap(),
    );
    let mut actual = vec![];
    while let Some(batch) = fresh.next_documents(ctx.snapshot(), &read).await.unwrap() {
        actual.extend(batch.into_iter().map(|mut d| {
            if d.memory_id() == updated.id {
                assert_eq!(d.take_reusable_vector(), Some(vec![1., 0.]));
            } else {
                assert!(d.take_reusable_vector().is_none());
            }
            (d.memory_id(), d.revision())
        }));
    }
    let mut expected = vec![(updated.id, updated.revision), (docs[1].memory_id(), 1)];
    expected.sort();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn fragmented_memory_streams_preserve_all_revisions_and_release_readers_between_batches() {
    use arrow_array::RecordBatchIterator;
    use lance::{
        Dataset,
        dataset::{WriteMode, WriteParams},
    };
    use std::collections::{BTreeMap, BTreeSet};
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let mut expected = BTreeMap::new();
    let mut revisions = BTreeSet::new();
    let mut first = None;
    for number in 0..16 {
        let memory = NativeMemoryStore::create(
            ctx.snapshot(),
            &write,
            input(&format!("fragment-{number}")),
            write_limits(),
        )
        .await
        .unwrap();
        expected.insert(memory.id, memory.revision);
        revisions.insert((memory.id, memory.revision));
        first.get_or_insert(memory);
    }
    let first = first.unwrap();
    let updated = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        first.id,
        update(
            &first,
            "fragment-update",
            "new revision on another fragment",
        ),
        write_limits(),
    )
    .await
    .unwrap();
    expected.insert(updated.id, updated.revision);
    revisions.insert((updated.id, updated.revision));
    // Split only this private fixture's valid public-CRUD records into many
    // physical files. Product data and migration/import code are not involved.
    let pin = sdk::pin(&f).await;
    let batch = sdk::all(&pin).await;
    let rows = batch.num_rows();
    Dataset::write(
        RecordBatchIterator::new(vec![Ok(batch)], sdk::schema()),
        pin.corpus_uri(),
        Some(WriteParams {
            mode: WriteMode::Overwrite,
            max_rows_per_file: 1,
            max_rows_per_group: 1,
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    drop(pin);
    let pin = sdk::pin(&f).await;
    let dataset = Dataset::open(pin.corpus_uri()).await.unwrap();
    assert_eq!(dataset.get_fragments().len(), rows);
    assert!(
        rows > 32,
        "exercise more fragments than the previous decode fanout"
    );
    drop(dataset);
    let version = pin.version();
    drop(pin);
    let mut limits = read_limits();
    limits.batch_rows = NonZeroU32::new(1).unwrap();
    let heads = NativeMemoryStore::observe_heads(ctx.snapshot(), &read, head_limits())
        .await
        .unwrap();
    let MemoryHeadRead::Published { generation, heads } = heads else {
        panic!("published heads")
    };
    assert_eq!(generation.sdk_version, version);
    assert_eq!(
        heads
            .into_iter()
            .map(|head| head.id)
            .collect::<BTreeSet<_>>(),
        expected.keys().copied().collect()
    );
    let listed =
        NativeMemoryStore::list(ctx.snapshot(), &read, MemoryListFilter::default(), limits)
            .await
            .unwrap();
    let MemoryListRead::Published {
        generation,
        memories,
    } = listed
    else {
        panic!("published catalogue")
    };
    assert_eq!(generation.sdk_version, version);
    assert_eq!(
        memories
            .into_iter()
            .map(|memory| (memory.id, memory.revision))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
    let mut captured = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), limits)
            .await
            .unwrap(),
    );
    assert_eq!(captured.generation().sdk_version, version);
    let mut documents = Vec::new();
    while let Some(batch) = captured
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
    {
        assert_eq!(batch.len(), 1);
        documents.extend(batch);
    }
    assert_eq!(
        documents
            .iter()
            .map(|doc| (doc.memory_id(), doc.revision()))
            .collect::<BTreeSet<_>>(),
        revisions
    );
    // Keep the authoritative captured pin alive: EOF must still close physical
    // data readers while retaining its snapshot and generation for attachment.
    let data = f.project.runtime_root().join("memory/index.lance/data");
    let open_data = std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.path().read_link().ok())
        .filter(|path| path.starts_with(&data))
        .count();
    assert_eq!(
        open_data, 0,
        "captured generation must not retain completed scan readers"
    );
    let vectors = documents
        .iter()
        .map(|document| attachment(document, vec![1., 0.]))
        .collect();
    NativeMemoryStore::attach_embeddings(
        ctx.snapshot(),
        &write,
        &captured,
        vectors,
        write_limits(),
    )
    .await
    .unwrap();
    let (total, found) = hits(
        NativeMemoryStore::search_semantic(
            ctx.snapshot(),
            &read,
            identity(1, 2),
            &[1., 0.],
            options(16),
            limits,
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, expected.len());
    assert_eq!(
        found
            .into_iter()
            .map(|hit| (hit.current.id, hit.matched_revision.get()))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
    let mut fresh = corpus(
        NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, identity(1, 2), limits)
            .await
            .unwrap(),
    );
    assert!(
        fresh
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn partial_semantic_ignores_other_model_without_losing_compatible_hits() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, bearer, reference, _admin
    );
    let a = NativeMemoryStore::create(ctx.snapshot(), &write, input("compatible"), write_limits())
        .await
        .unwrap();
    let b = NativeMemoryStore::create(ctx.snapshot(), &write, input("other-model"), write_limits())
        .await
        .unwrap();
    NativeMemoryStore::create(ctx.snapshot(), &write, input("pending"), write_limits())
        .await
        .unwrap();
    for (id, space, vector) in [
        (a.id, identity(1, 2), vec![1., 0.]),
        (b.id, identity(2, 2), vec![0., 1.]),
    ] {
        let mut c = corpus(
            NativeMemoryStore::capture_embeddings(ctx.snapshot(), &read, space, read_limits())
                .await
                .unwrap(),
        );
        let docs = c
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .unwrap();
        let doc = docs.iter().find(|doc| doc.memory_id() == id).unwrap();
        NativeMemoryStore::attach_embeddings(
            ctx.snapshot(),
            &write,
            &c,
            vec![attachment(doc, vector)],
            write_limits(),
        )
        .await
        .unwrap();
    }
    for (id, space, query) in [
        (a.id, identity(1, 2), [1., 0.]),
        (b.id, identity(2, 2), [0., 1.]),
    ] {
        let (total, found) = hits(
            NativeMemoryStore::search_semantic(
                ctx.snapshot(),
                &read,
                space,
                &query,
                options(10),
                read_limits(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(total, 1);
        assert_eq!(found[0].current.id, id);
        assert_eq!(found[0].location, MemoryMatchLocation::Current);
    }
}
