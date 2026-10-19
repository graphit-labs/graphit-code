use super::*;
use std::num::{NonZeroU32, NonZeroU64};

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
fn published(value: MemoryHybridSearchRead) -> (MemoryGeneration, usize, Vec<MemoryHybridHit>) {
    match value {
        MemoryHybridSearchRead::Published {
            generation,
            total_hits,
            hits,
        } => (generation, total_hits, hits),
        other => panic!("expected hybrid publication, got {other:?}"),
    }
}
async fn cover(
    original: &Arc<graphit_core::file_io::SnapshotContext>,
    read: &graphit_auth::ProjectReadAdmission<'_>,
    write: &graphit_auth::ProjectWriteAdmission<'_>,
    vector: impl Fn(&MemoryEmbeddingDocument) -> Vec<f32>,
) {
    let MemoryEmbeddingCorpusRead::Published(mut corpus) =
        NativeMemoryStore::capture_embeddings(original, read, identity(1), read_limits())
            .await
            .unwrap()
    else {
        panic!("actual corpus")
    };
    let mut attachments = Vec::new();
    while let Some(documents) = corpus.next_documents(original, read).await.unwrap() {
        for document in documents {
            attachments.push(MemoryEmbeddingAttachment {
                memory_id: document.memory_id(),
                revision: NonZeroU64::new(document.revision()).unwrap(),
                input_fingerprint: *document.fingerprint(),
                vector: vector(&document),
            });
        }
    }
    NativeMemoryStore::attach_embeddings(original, write, &corpus, attachments, write_limits())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hybrid_union_before_topk_uses_channel_order_one_current_head_and_semantic_only_ids() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
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
            NativeMemoryStore::create(ctx.snapshot(), &write, value, write_limits())
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
    cover(ctx.snapshot(), &read, &write, |document| {
        vectors[&document.memory_id()].clone()
    })
    .await;
    let pin = sdk::pin(&f).await;
    let (generation, total, hits) = published(
        NativeMemoryStore::search_hybrid(
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
    assert_eq!(generation.sdk_version, pin.version());
    assert_eq!(total, 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current, records[2]);
    assert!((hits[0].score - 2.0 / 62.0).abs() < f64::EPSILON);
    assert_eq!(hits[0].lexical.unwrap().matched_revision, NonZeroU64::MIN);
    assert_eq!(hits[0].semantic.unwrap().matched_revision, NonZeroU64::MIN);
    let (_, total, all) = published(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(5)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 5);
    assert_eq!(all.len(), 5);
    let semantic_only = all.iter().find(|h| h.current.id == records[1].id).unwrap();
    assert!(semantic_only.lexical.is_none());
    assert!(semantic_only.semantic.is_some());
    assert!(all.windows(2).all(|w| w[0].score > w[1].score
        || (w[0].score == w[1].score && w[0].current.id < w[1].current.id)));
    let (_, total, no_lexical) = published(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("absentword", options(5)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 5);
    assert!(
        no_lexical
            .iter()
            .all(|h| h.lexical.is_none() && h.semantic.is_some())
    );
    let mut excluded = options(2);
    excluded.filter.exclude_mandatory = true;
    excluded.filter.important = Some(true);
    let (_, total, hits) = published(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", excluded),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, records[2]);
    assert_eq!(sdk::pin(&f).await.version(), pin.version());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hybrid_channels_keep_different_archive_revisions_deleted_history_and_unavailable_without_fallback()
 {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let mut create = input("history");
    create.body = "needle old".into();
    let first = NativeMemoryStore::create(ctx.snapshot(), &write, create, write_limits())
        .await
        .unwrap();
    let second = NativeMemoryStore::update(
        ctx.snapshot(),
        &write,
        first.id,
        UpdateMemory {
            expected_revision: 1,
            idempotency_key: "history2".into(),
            reason: "new body".into(),
            title: None,
            body: Some("new unmatched".into()),
            classification: None,
            mandatory: None,
            important: None,
            tags: None,
            references: None,
        },
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        MemoryHybridSearchRead::Unavailable {
            reason: MemoryEmbeddingUnavailable::MissingCoverage,
            ..
        }
    ));
    cover(ctx.snapshot(), &read, &write, |d| {
        if d.revision() == 1 {
            vec![0., 1.]
        } else {
            vec![1., 0.]
        }
    })
    .await;
    let (_, total, hits) = published(
        NativeMemoryStore::search_hybrid(
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
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, second);
    assert_eq!(
        hits[0].lexical.unwrap().location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::MIN
        }
    );
    assert_eq!(
        hits[0].semantic.unwrap().location,
        MemoryMatchLocation::Current
    );
    assert_eq!(hits[0].semantic.unwrap().matched_revision.get(), 2);
    assert!(matches!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(2),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        MemoryHybridSearchRead::Unavailable {
            reason: MemoryEmbeddingUnavailable::IncompatibleIdentity,
            ..
        }
    ));
    NativeMemoryStore::delete(
        ctx.snapshot(),
        &write,
        first.id,
        DeleteMemory {
            expected_revision: 2,
            idempotency_key: "history-delete".into(),
            reason: "explicit delete".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    let (_, total, hits) = published(
        NativeMemoryStore::search_hybrid(
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
    assert_eq!(total, 0);
    assert!(hits.is_empty());
    let mut with_history = options(1);
    with_history.include_deleted_history = true;
    let (_, total, hits) = published(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", with_history),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].current.revision, 3);
    assert_eq!(
        hits[0].lexical.unwrap().location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::MIN
        }
    );
    assert_eq!(
        hits[0].semantic.unwrap().location,
        MemoryMatchLocation::Archived {
            revision: NonZeroU64::new(2).unwrap()
        }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hybrid_absence_empty_query_and_vector_guards_auth_cancel_and_late_corruption_are_causal() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    assert_eq!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        MemoryHybridSearchRead::NoCorpus
    );
    assert!(!f.project.runtime_root().join("memory").exists());
    let mut tiny = read_limits();
    tiny.max_query_bytes = NonZeroUsize::MIN;
    assert!(matches!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            tiny
        )
        .await,
        Err(MemoryStoreError::Limit("lexical expansion bytes"))
    ));
    assert!(MemoryHybridQuery::new(" ", options(1), read_limits()).is_err());
    for vector in [vec![1.], vec![f32::NAN, 0.]] {
        assert!(matches!(
            NativeMemoryStore::search_hybrid(
                ctx.snapshot(),
                &read,
                identity(1),
                prepared("needle", options(1)),
                &vector,
                read_limits()
            )
            .await,
            Err(MemoryStoreError::Invalid("Memory query vector"))
        ));
    }
    sdk::empty_with_schema(&f, &ctx, sdk::schema()).await;
    assert!(matches!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        MemoryHybridSearchRead::PublishedEmpty { .. }
    ));
    let empty_version = sdk::pin(&f).await.version();
    assert!(matches!(
        NativeMemoryStore::maintain_lexical_index(
            ctx.snapshot(),
            &write,
            MemoryLexicalIndexOperation::Refresh,
            write_limits()
        )
        .await,
        Err(MemoryStoreError::LexicalIndexUnavailable(
            graphit_data::lance_branch_store::JsonFtsIndexState::Missing
        ))
    ));
    assert_eq!(sdk::pin(&f).await.version(), empty_version);
    let mut value = input("guard");
    value.body = "needle".into();
    NativeMemoryStore::create(ctx.snapshot(), &write, value, write_limits())
        .await
        .unwrap();
    cover(ctx.snapshot(), &read, &write, |_| vec![1., 0.]).await;
    let before = sdk::pin(&f).await.version();
    assert!(matches!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        MemoryHybridSearchRead::Published { total_hits: 1, .. }
    ));
    assert_eq!(sdk::pin(&f).await.version(), before);
    let wrong = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &wrong,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await,
        Err(MemoryStoreError::WrongAdmission)
    ));
    let other = request(&cancel);
    assert!(matches!(
        NativeMemoryStore::search_hybrid(
            other.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await,
        Err(MemoryStoreError::DifferentRequest)
    ));
    let pin = sdk::pin(&f).await;
    let raw = sdk::all(&pin).await;
    let mut values = (0..raw.num_rows())
        .map(|row| sdk::string(&raw, 17, row).to_owned())
        .collect::<Vec<_>>();
    let last = values.len() - 1;
    values[last] = "{broken".into();
    sdk::replace(&pin, sdk::text_column(&raw, 17, values)).await;
    assert!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .is_err()
    );
    cancel.cancel();
    assert!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hybrid_named_inherited_query_keeps_one_observed_generation_and_isolation() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx, "main");
    admissions!(
        f, ctx, issuer, authority, grant, read, write, _bearer, _reference, _admin
    );
    let mut value = input("branch");
    value.body = "needle".into();
    let memory = NativeMemoryStore::create(ctx.snapshot(), &write, value, write_limits())
        .await
        .unwrap();
    cover(ctx.snapshot(), &read, &write, |_| vec![1., 0.]).await;
    let version = VersionContext::select(
        &graphit_core::project_paths::ProjectPaths::at(
            &f.root,
            &graphit_core::project_paths::InstallationPaths::resolve(
                f.installation.brand().clone(),
                &f.root,
                None,
                Some(f.installation.owner_root()),
            )
            .unwrap(),
        )
        .unwrap(),
        Checkout::NonGit,
        "hybrid-feature",
        None,
        "",
    )
    .unwrap();
    let mut scope = f.scope();
    scope.project.as_mut().unwrap().version = &version;
    let (bearer, reference, _run, _admin) = f.issue_for(&issuer, &ctx, scope);
    let feature_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, scope, &ctx)
        .unwrap();
    let feature_read = authority
        .admit_read(&feature_grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    let feature_write = authority
        .admit_write(&feature_grant, &ctx, ProjectWriteOperation::Memory)
        .unwrap();
    let main = sdk::pin(&f).await;
    let (generation, total, hits) = published(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &feature_read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, memory);
    assert!(generation.inherited_target.is_some());
    assert_eq!(generation.sdk_version, main.version());
    NativeMemoryStore::update(
        ctx.snapshot(),
        &feature_write,
        memory.id,
        UpdateMemory {
            expected_revision: 1,
            idempotency_key: "branch-edit".into(),
            reason: "branch body".into(),
            title: None,
            body: Some("needle changed".into()),
            classification: None,
            mandatory: None,
            important: None,
            tags: None,
            references: None,
        },
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        NativeMemoryStore::search_hybrid(
            ctx.snapshot(),
            &feature_read,
            identity(1),
            prepared("needle", options(1)),
            &[1., 0.],
            read_limits()
        )
        .await
        .unwrap(),
        MemoryHybridSearchRead::Published { total_hits: 1, .. }
    ));
    let (generation, total, hits) = published(
        NativeMemoryStore::search_hybrid(
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
    assert_eq!(generation.sdk_version, main.version());
    assert_eq!(total, 1);
    assert_eq!(hits[0].current, memory);
}
