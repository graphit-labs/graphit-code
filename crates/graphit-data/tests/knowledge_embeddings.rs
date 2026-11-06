//! Domain NN tests: real Core/Auth admissions, FULL18 and SDK storage.
//! Deterministic fixture vectors qualify storage, not Models inference.
include!("support/knowledge_embeddings_fixture.rs");
use arrow_array::RecordBatchIterator;

#[tokio::test]
async fn complete_capture_partial_attach_nn_noop_and_model_reindex() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    assert!(matches!(
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap(),
        KnowledgeEmbeddingRead::NoCorpus
    ));
    assert!(!f.project.runtime_root().join("knowledge").exists());
    let source = publish_source(&ctx, &write, &["a", "b"]).await;
    let fresh = capture(&ctx, &read).await;
    assert_eq!(fresh.documents().len(), 2);
    assert_eq!(fresh.source_generation(), &source.default);
    assert!(
        fresh
            .documents()
            .iter()
            .all(|v| v.body == "---\ntitle: body literal\n---\ntext\n")
    );
    let id = identity(7, 2);
    let inputs = [input("a", 1), input("b", 2)];
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &fresh,
            &id,
            &inputs,
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(2),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Unavailable(KnowledgeEmbeddingUnavailable::MissingSemantic)
    ));
    let (first, key) = committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &fresh,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert!(first.version > source.default.version);
    assert!(matches!(
        resolve_knowledge_embedding_publication(ctx.snapshot(), &read, &key, read_limits())
            .await
            .unwrap(),
        KnowledgeEmbeddingResolution::Delta(
            graphit_data::lance_branch_store::DeltaResolution::Applied { version }
        ) if version == first.version
    ));
    let partial = capture(&ctx, &read).await;
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &partial,
            &id,
            &inputs,
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(2),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { hits, .. } if hits.len() == 1 && hits[0].slug == "a"
    ));
    let (second, delta) = committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &partial,
            &id,
            &[row("b", 2, &[2., 1.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert!(second.version > first.version);
    assert!(matches!(
        resolve_knowledge_embedding_publication(ctx.snapshot(), &read, &delta, read_limits())
            .await
            .unwrap(),
        KnowledgeEmbeddingResolution::Delta(
            graphit_data::lance_branch_store::DeltaResolution::Applied { version }
        ) if version == second.version
    ));
    let complete = capture(&ctx, &read).await;
    assert_eq!(complete.source_generation().version, second.version);
    for (metric, slugs, distances) in [
        (KnowledgeEmbeddingMetric::L2, ["a", "b"], [0., 2.]),
        (
            KnowledgeEmbeddingMetric::Cosine,
            ["a", "b"],
            [0., 1. - 2. / 5f32.sqrt()],
        ),
        (KnowledgeEmbeddingMetric::Dot, ["b", "a"], [-1., 0.]),
    ] {
        let result = query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &complete,
            &id,
            &inputs,
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(2),
                metric,
            },
        )
        .await
        .unwrap();
        let KnowledgeSemanticQuery::Hits { hits, .. } = result else {
            panic!("expected hits")
        };
        assert_eq!(hits.len(), 2);
        for ((hit, slug), distance) in hits.iter().zip(slugs).zip(distances) {
            assert_eq!(hit.slug, slug);
            assert!((hit.distance - distance).abs() < 1e-5);
        }
    }
    let no_hits = query_knowledge_embeddings(
        ctx.snapshot(),
        &read,
        &complete,
        &id,
        &inputs,
        KnowledgeEmbeddingQuery {
            vector: &[0., 0.],
            top_k: n(2),
            metric: KnowledgeEmbeddingMetric::Cosine,
        },
    )
    .await
    .unwrap();
    assert!(matches!(no_hits, KnowledgeSemanticQuery::Hits { hits, .. } if hits.is_empty()));
    let rows = [row("a", 1, &[1., 0.]), row("b", 2, &[2., 1.])];
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &complete,
            &id,
            &rows,
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await
        .unwrap(),
        KnowledgeEmbeddingPublication::Unchanged
    ));
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &partial,
            &id,
            &[row("b", 2, &[2., 1.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await,
        Ok(KnowledgeEmbeddingPublication::Unchanged)
    ));
    let new = identity(8, 2);
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &complete,
            &new,
            &inputs,
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(2),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Unavailable(KnowledgeEmbeddingUnavailable::IncompatibleIdentity)
    ));
    let larger = identity(8, 3);
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &complete,
            &larger,
            &inputs,
            KnowledgeEmbeddingQuery {
                vector: &[1., 0., 0.],
                top_k: n(2),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Unavailable(KnowledgeEmbeddingUnavailable::IncompatibleDimensions)
    ));
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &complete,
            &new,
            &rows,
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidInput("reindex required"))
    ));
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &complete,
            &new,
            &rows,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &complete,
            &id,
            &rows,
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await,
        Err(KnowledgeEmbeddingError::Conflict)
    ));
    // Old pin remains an honest snapshot after latest model identity advances.
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &complete,
            &id,
            &inputs,
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(2),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { .. }
    ));
    let current = capture(&ctx, &read).await;
    assert!(current.source_generation().version > second.version);
    assert_eq!(current.documents(), fresh.documents());
    assert_eq!(
        current.source_generation().version,
        current.semantic_generation().unwrap().version
    );
}

#[tokio::test]
async fn named_vector_publication_preserves_default_and_binds_delta_recovery() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let source = publish_source(&ctx, &write, &["a"]).await;
    let named_version =
        VersionContext::select(&f.paths, Checkout::NonGit, "first", None, "").unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue_for(&issuer, &parent, &named_version);
    let named_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            f.scope_for(&named_version),
            &ctx,
        )
        .unwrap();
    let named_read = authority
        .admit_read(&named_grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let named_write = authority
        .admit_write(&named_grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let captured = capture(&ctx, &named_read).await;
    assert!(captured.semantic_generation().is_none());
    let id = identity(7, 2);
    let publication = publish_knowledge_embeddings(
        ctx.snapshot(),
        &named_write,
        &captured,
        &id,
        &[row("a", 1, &[1., 0.])],
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap();
    let KnowledgeEmbeddingPublication::Committed {
        generation,
        recovery,
    } = publication
    else {
        panic!("expected named delta");
    };
    assert!(generation.version > source.default.version);
    assert_eq!(
        generation.physical_branch.as_deref(),
        Some(named_version.lance_branch().as_str())
    );
    assert!(generation.inherited_target.is_none());
    assert!(matches!(resolve_knowledge_embedding_publication(
        ctx.snapshot(), &named_read, &recovery, read_limits()).await.unwrap(),
        KnowledgeEmbeddingResolution::Delta(
            graphit_data::lance_branch_store::DeltaResolution::Applied { version })
            if version == generation.version));
    assert!(matches!(
        resolve_knowledge_embedding_publication(ctx.snapshot(), &read, &recovery, read_limits())
            .await,
        Err(KnowledgeEmbeddingError::InvalidInput("recovery binding"))
    ));
    let main = capture(&ctx, &read).await;
    assert_eq!(main.source_generation(), &source.default);
    assert!(main.semantic_generation().is_none());
    let named = capture(&ctx, &named_read).await;
    assert_eq!(named.semantic_generation().unwrap(), generation);
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &named_read,
            &named,
            &id,
            &[input("a", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            },
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { .. }
    ));
}

#[tokio::test]
async fn independent_branch_widths_keep_main_and_old_pins() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a"]).await;
    let main0 = capture(&ctx, &read).await;
    let id2 = identity(7, 2);
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &main0,
            &id2,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let named_version =
        VersionContext::select(&f.paths, Checkout::NonGit, "development", None, "").unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue_for(&issuer, &parent, &named_version);
    let named_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            f.scope_for(&named_version),
            &ctx,
        )
        .unwrap();
    let named_read = authority
        .admit_read(&named_grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let named_write = authority
        .admit_write(&named_grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let inherited = capture(&ctx, &named_read).await;
    assert!(
        inherited
            .semantic_generation()
            .unwrap()
            .inherited_target
            .is_some()
    );
    let id4 = identity(9, 4);
    let (child, key) = committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &named_write,
            &inherited,
            &id4,
            &[row("a", 3, &[1., 0., 0., 0.])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert!(child.physical_branch.is_some());
    assert!(child.inherited_target.is_none());
    let main = capture(&ctx, &read).await;
    let id3 = identity(8, 3);
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &main,
            &id3,
            &[row("a", 2, &[1., 0., 0.])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let named = capture(&ctx, &named_read).await;
    assert_eq!(
        named.stored_inputs().next().unwrap().1.dimensions().get(),
        4
    );
    assert_eq!(
        capture(&ctx, &read)
            .await
            .stored_inputs()
            .next()
            .unwrap()
            .1
            .dimensions()
            .get(),
        3
    );
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &named_read,
            &named,
            &id4,
            &[input("a", 3)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0., 0., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { .. }
    ));
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &named_read,
            &inherited,
            &id2,
            &[input("a", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { .. }
    ));
    assert!(matches!(
        resolve_knowledge_embedding_publication(ctx.snapshot(), &named_read, &key, read_limits())
            .await
            .unwrap(),
        KnowledgeEmbeddingResolution::Delta(
            graphit_data::lance_branch_store::DeltaResolution::Applied { .. }
        )
    ));
    assert!(matches!(
        resolve_knowledge_embedding_publication(ctx.snapshot(), &read, &key, read_limits()).await,
        Err(KnowledgeEmbeddingError::InvalidInput("recovery binding"))
    ));
}

#[tokio::test]
async fn reset_same_path_version_rows_invalidates_capture_physically() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let _source = publish_source(&ctx, &write, &["a"]).await;
    let first = capture(&ctx, &read).await;
    let id = identity(7, 2);
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &first,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let captured = capture(&ctx, &read).await;
    let path = f.project.runtime_root().join("knowledge/index.lance");
    let uri = url::Url::from_file_path(&path).unwrap().to_string();
    let old = Dataset::open(&uri).await.unwrap();
    let rows = sdk_batches(&old).await;
    let store = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let writer = store.writer_with_context(ctx.snapshot()).unwrap();
    std::fs::rename(&path, path.with_file_name("semantic-quarantine")).unwrap();
    let iterator = RecordBatchIterator::new(
        rows.clone().into_iter().map(Ok),
        Arc::new(arrow_schema::Schema::from(old.schema())),
    );
    let mut recreated = Dataset::write(iterator, &uri, None).await.unwrap();
    while recreated.version_id() < old.version_id() {
        recreated = overwrite(Arc::new(recreated), rows.clone()).await;
    }
    assert_eq!(recreated.version_id(), old.version_id());
    drop(writer);
    assert!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &captured,
            &id,
            &[input("a", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .is_err()
    );
    assert!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await
        .is_err()
    );
    let new = capture(&ctx, &read).await;
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &new,
            &id,
            &[input("a", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { .. }
    ));
}

#[tokio::test]
async fn source_change_removed_slug_reindex_and_competing_writers() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let source = publish_source(&ctx, &write, &["a", "b"]).await;
    let initial = capture(&ctx, &read).await;
    let id = identity(7, 2);
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &initial,
            &id,
            &[row("a", 1, &[1., 0.]), row("b", 2, &[2., 1.])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let stale = capture(&ctx, &read).await;
    let dataset = Arc::new(Dataset::open(&source.default.corpus_uri).await.unwrap());
    let batches = sdk_batches(&dataset).await;
    let store = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let writer = store.writer_with_context(ctx.snapshot()).unwrap();
    overwrite(dataset, vec![batches[0].slice(0, 1)]).await;
    drop(writer);
    assert!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &stale,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await
        .is_ok()
    );
    let fresh = capture(&ctx, &read).await;
    assert_eq!(fresh.documents().len(), 1);
    let slug = fresh.documents()[0].slug.as_str();
    let fp = if slug == "a" { 1 } else { 2 };
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &fresh,
            &id,
            &[input(slug, fp)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { hits, .. } if hits.len() == 1 && hits[0].slug == slug
    ));
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &fresh,
            &id,
            &[row(slug, fp, &[0.5, 0.5])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let current = capture(&ctx, &read).await;
    let ida = identity(8, 3);
    let idb = identity(9, 4);
    let rowsa = [row(slug, 3, &[1., 0., 0.])];
    let rowsb = [row(slug, 4, &[1., 0., 0., 0.])];
    let (a, b) = tokio::join!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &current,
            &ida,
            &rowsa,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits()
        ),
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &current,
            &idb,
            &rowsb,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits()
        )
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let loser = if a.is_err() {
        a.err().unwrap()
    } else {
        b.err().unwrap()
    };
    assert!(matches!(loser, KnowledgeEmbeddingError::Conflict));
    let winner = capture(&ctx, &read).await;
    assert_eq!(winner.stored_inputs().count(), 1);
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &current,
            &id,
            &[input(slug, fp)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { .. }
    ));
}

#[tokio::test]
async fn empty_corpus_invalid_inputs_distance_and_original_request() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &[]).await;
    let empty = capture(&ctx, &read).await;
    let id = identity(7, 2);
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &empty,
            &id,
            &[],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::PublishedEmpty
    ));
    assert!(
        KnowledgeEmbeddingIdentity::new([0; 32], NonZeroU32::new(u32::MAX).unwrap(), "r".into())
            .is_err()
    );
    let foreign_cancel = Cancellation::default();
    let foreign = request(&foreign_cancel);
    assert!(matches!(
        capture_knowledge_embedding_corpus(foreign.snapshot(), &read, read_limits()).await,
        Err(KnowledgeEmbeddingError::Index(
            KnowledgeIndexError::DifferentRequest
        ))
    ));
    let wrong = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Memory)
        .unwrap();
    assert!(matches!(
        capture_knowledge_embedding_corpus(ctx.snapshot(), &wrong, read_limits()).await,
        Err(KnowledgeEmbeddingError::Index(
            KnowledgeIndexError::WrongAdmission
        ))
    ));
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &empty,
            &id,
            &[row("ghost", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidInput("slugs"))
    ));
    cancel.cancel();
    assert!(
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn functional_write_bounds_and_real_nonfinite_l2_result() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["aa", "bb"]).await;
    let captured = capture(&ctx, &read).await;
    let id = identity(7, 2);
    let rows = [row("aa", 1, &[1., 0.]), row("bb", 2, &[2., 1.])];
    {
        let mut limits = write_limits();
        limits.content.max_identifier_bytes = n(1);
        let selected =
            KnowledgeEmbeddingIdentity::new([7; 32], NonZeroU32::new(2).unwrap(), "r".into())
                .unwrap();
        assert!(matches!(
            publish_knowledge_embeddings(
                ctx.snapshot(),
                &write,
                &captured,
                &selected,
                &rows,
                KnowledgeEmbeddingWriteMode::ReplaceComplete,
                limits
            )
            .await,
            Err(KnowledgeEmbeddingError::InvalidInput("slug"))
        ));
        assert_source_without_vectors(&f).await;
    }
    for bad in [&[f32::NAN, 0.][..], &[f32::INFINITY, 0.][..], &[1.][..]] {
        assert!(matches!(
            publish_knowledge_embeddings(
                ctx.snapshot(),
                &write,
                &captured,
                &id,
                &[row("aa", 1, bad)],
                KnowledgeEmbeddingWriteMode::Attach,
                write_limits()
            )
            .await,
            Err(KnowledgeEmbeddingError::InvalidInput("vector"))
        ));
    }
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            &[row("aa", 1, &[1.0e20, 0.]), row("bb", 2, &[1.0e20, 0.])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let complete = capture(&ctx, &read).await;
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &complete,
            &id,
            &[input("aa", 1), input("bb", 2)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidStored("distance"))
    ));
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &complete,
            &id,
            &[input("aa", 1), input("aa", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidInput("slugs"))
    ));
}

#[tokio::test]
async fn last_source_slug_removed_is_empty_without_another_semantic_cleanup() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let source = publish_source(&ctx, &write, &["a"]).await;
    let id = identity(7, 2);
    let first = capture(&ctx, &read).await;
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &first,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let dataset = Arc::new(Dataset::open(&source.default.corpus_uri).await.unwrap());
    let empty = RecordBatch::new_empty(Arc::new(arrow_schema::Schema::from(dataset.schema())));
    let store = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let writer = store.writer_with_context(ctx.snapshot()).unwrap();
    let emptied = overwrite(dataset, vec![empty]).await;
    drop(writer);
    let captured = capture(&ctx, &read).await;
    assert!(captured.documents().is_empty());
    assert_eq!(captured.stored_inputs().count(), 0);
    assert_eq!(captured.source_generation().version, emptied.version_id());
    for model in [&id, &identity(8, 3)] {
        // This low-level query accepts an explicit vector, whose input contract
        // remains valid even for an empty corpus. Optional-vector readiness is
        // exercised through the vector-search API in its own regression.
        let query_vector = vec![1.; model.dimensions().get() as usize];
        assert!(matches!(
            query_knowledge_embeddings(
                ctx.snapshot(),
                &read,
                &captured,
                model,
                &[],
                KnowledgeEmbeddingQuery {
                    vector: &query_vector,
                    top_k: n(1),
                    metric: KnowledgeEmbeddingMetric::L2
                }
            )
            .await
            .unwrap(),
            KnowledgeSemanticQuery::PublishedEmpty
        ));
        assert!(matches!(
            publish_knowledge_embeddings(
                ctx.snapshot(),
                &write,
                &captured,
                model,
                &[],
                KnowledgeEmbeddingWriteMode::ReplaceComplete,
                write_limits()
            )
            .await
            .unwrap(),
            KnowledgeEmbeddingPublication::Unchanged
        ));
    }
    assert_eq!(
        capture(&ctx, &read).await.source_generation(),
        captured.source_generation()
    );
}

#[tokio::test]
async fn revoked_admissions_deny_capture_publication_and_query() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    publish_source(&ctx, &write, &["a"]).await;
    let captured = capture(&ctx, &read).await;
    let id = identity(7, 2);
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        read_native_knowledge_heads(
            ctx.snapshot(),
            &read,
            KnowledgeHeadReadLimits {
                native: read_limits().native,
                batch_rows: NonZeroU32::new(1).unwrap(),
                max_slug_bytes: n(256),
            },
        )
        .await,
        Err(KnowledgeIndexError::Auth(AuthError::Denied))
    ));
    assert!(matches!(
        graphit_data::knowledge_search::capture_native_knowledge_search(
            ctx.snapshot(),
            &read,
            read_limits(),
        )
        .await,
        Err(graphit_data::knowledge_search::KnowledgeSearchError::Data(
            KnowledgeIndexError::Auth(AuthError::Denied)
        ))
    ));
    assert!(matches!(
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits()).await,
        Err(KnowledgeEmbeddingError::Index(KnowledgeIndexError::Auth(
            AuthError::Denied
        )))
    ));
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await,
        Err(KnowledgeEmbeddingError::Index(KnowledgeIndexError::Auth(
            AuthError::Denied
        )))
    ));
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &captured,
            &id,
            &[input("a", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            }
        )
        .await,
        Err(KnowledgeEmbeddingError::Index(KnowledgeIndexError::Auth(
            AuthError::Denied
        )))
    ));
    assert_source_without_vectors(&f).await;
}

#[tokio::test]
async fn recovery_observes_absent_transaction_then_foreign_winner_and_rejects_corruption() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a"]).await;
    let id = identity(7, 2);
    let first = capture(&ctx, &read).await;
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &first,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let path = f.project.runtime_root().join("knowledge/index.lance");
    let uri = path.to_str().unwrap().to_owned();
    let dataset = Arc::new(Dataset::open(&uri).await.unwrap());
    let rows = sdk_batches(&dataset).await;
    let store = graphit_data::lance_branch_store::LanceBranchStore::open(
        &uri,
        Arc::new(arrow_schema::Schema::from(dataset.schema())),
        None,
    )
    .await
    .unwrap();
    let pin = store.capture_target(None).await.unwrap();
    let artifact = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let writer = artifact.writer_with_context(ctx.snapshot()).unwrap();
    let pending = pin
        .prepare_merge(rows.clone(), vec!["slug".into()])
        .await
        .unwrap();
    let recovery = KnowledgeEmbeddingRecoveryKey::Delta(pending.recovery_key().clone());
    drop(writer);
    assert!(matches!(
        resolve_knowledge_embedding_publication(ctx.snapshot(), &read, &recovery, read_limits())
            .await
            .unwrap(),
        KnowledgeEmbeddingResolution::Delta(
            graphit_data::lance_branch_store::DeltaResolution::Unresolved
        )
    ));
    let writer = artifact.writer_with_context(ctx.snapshot()).unwrap();
    let winner = overwrite(Arc::clone(&dataset), rows.clone()).await;
    drop(writer);
    assert!(matches!(
        resolve_knowledge_embedding_publication(ctx.snapshot(), &read, &recovery, read_limits())
            .await
            .unwrap(),
        KnowledgeEmbeddingResolution::Delta(
            graphit_data::lance_branch_store::DeltaResolution::Conflicted { version }
        ) if version == winner.version_id()
    ));
    let mut columns = rows[0].columns().to_vec();
    columns[21] = Arc::new(arrow_array::StringArray::from(vec!["Query"]));
    let corrupt = RecordBatch::try_new(rows[0].schema(), columns).unwrap();
    let writer = artifact.writer_with_context(ctx.snapshot()).unwrap();
    overwrite(Arc::new(winner), vec![corrupt]).await;
    drop(writer);
    assert!(matches!(
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits()).await,
        Err(KnowledgeEmbeddingError::InvalidStored("content"))
    ));
}

#[tokio::test]
async fn read_only_grant_queries_without_authorizing_publication() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = f.issuer(&ctx);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    publish_source(&ctx, &write, &["a"]).await;
    let id = identity(7, 2);
    let before = capture(&ctx, &read).await;
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &before,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let bearer = issuer
        .issue(
            &parent,
            &run,
            &reference,
            &[Permission::Read(Module::Knowledge)],
            &f.scope(),
        )
        .unwrap();
    let read_grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let readonly = authority
        .admit_read(&read_grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    assert!(matches!(
        authority.admit_write(&read_grant, &ctx, ProjectWriteOperation::Knowledge),
        Err(AuthError::Denied)
    ));
    let captured = capture(&ctx, &readonly).await;
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &readonly,
            &captured,
            &id,
            &[input("a", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2
            },
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { .. }
    ));
}

#[tokio::test]
async fn persisted_malformed_vectors_duplicates_and_identity_never_become_absence() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let before = capture(&ctx, &read).await;
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &before,
            &id,
            &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let uri = url::Url::from_file_path(f.project.runtime_root().join("knowledge/index.lance"))
        .unwrap()
        .to_string();
    let initial = Dataset::open(&uri).await.unwrap();
    let valid = sdk_batches(&initial).await;
    assert_eq!(valid.len(), 1);
    assert_eq!(valid[0].num_rows(), 2);
    let vector_field = Arc::new(arrow_schema::Field::new(
        "item",
        arrow_schema::DataType::Float32,
        true,
    ));
    let mut invalid = Vec::new();
    for values in [
        vec![Some(1.), None, Some(0.), Some(1.)],
        vec![Some(f32::NAN), Some(0.), Some(0.), Some(1.)],
        vec![Some(f32::INFINITY), Some(0.), Some(0.), Some(1.)],
    ] {
        let mut columns = valid[0].columns().to_vec();
        columns[22] = Arc::new(
            arrow_array::FixedSizeListArray::try_new(
                Arc::clone(&vector_field),
                2,
                Arc::new(arrow_array::Float32Array::from(values)),
                None,
            )
            .unwrap(),
        );
        invalid.push(RecordBatch::try_new(valid[0].schema(), columns).unwrap());
    }
    let mut columns = valid[0].columns().to_vec();
    columns[0] = Arc::new(arrow_array::StringArray::from(vec!["a", "a"]));
    invalid.push(RecordBatch::try_new(valid[0].schema(), columns).unwrap());
    let mut columns = valid[0].columns().to_vec();
    columns[19] = Arc::new(
        arrow_array::FixedSizeBinaryArray::try_from_iter([[7u8; 32], [8u8; 32]].into_iter())
            .unwrap(),
    );
    invalid.push(RecordBatch::try_new(valid[0].schema(), columns).unwrap());
    let store = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    for corrupt in invalid {
        let dataset = Arc::new(Dataset::open(&uri).await.unwrap());
        let writer = store.writer_with_context(ctx.snapshot()).unwrap();
        overwrite(dataset, vec![corrupt]).await;
        drop(writer);
        assert!(matches!(
            capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits()).await,
            Err(KnowledgeEmbeddingError::InvalidStored(_))
        ));
    }
}

#[tokio::test]
async fn original_deadline_is_preserved_on_domain_boundaries() {
    // Bootstrap/Auth/source publication are fixture preparation, outside the
    // immutable request whose original deadline is exercised below.
    let preparation_cancel = Cancellation::default();
    let preparation = request(&preparation_cancel);
    let f = Fixture::new(&preparation);
    let issuer = f.issuer(&preparation);
    let caller = issuer
        .authenticate_runtime(&f.secret(), &preparation)
        .unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue(&issuer, &parent);
    let grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            f.scope(),
            &preparation,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_write(&grant, &preparation, ProjectWriteOperation::Knowledge)
        .unwrap();
    publish_source(&preparation, &write, &["a"]).await;
    let cancel = Cancellation::default();
    let now = Instant::now();
    let deadline = now + Duration::from_secs(3);
    let clock = Arc::new(std::sync::Mutex::new(now));
    let ctx = RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        },
        &cancel,
        deadline,
    )
    .unwrap();
    ctx.snapshot().set_test_clock(Arc::clone(&clock)).unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    let read = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
        .unwrap();
    let write = authority
        .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
        .unwrap();
    let captured = capture(&ctx, &read).await;
    let id = identity(7, 2);
    assert_eq!(ctx.snapshot().deadline(), Some(deadline));
    ctx.check().unwrap();
    // The real capture must finish before the immutable Original expires.
    *clock.lock().unwrap() = deadline;
    let errors = [
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .err()
            .unwrap(),
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            &[row("a", 1, &[1., 0.])],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .err()
        .unwrap(),
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &captured,
            &id,
            &[input("a", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2,
            },
        )
        .await
        .err()
        .unwrap(),
    ];
    for error in errors {
        let KnowledgeEmbeddingError::Index(KnowledgeIndexError::Request(cause)) = error else {
            panic!("original request timeout required")
        };
        assert_eq!(cause.kind(), std::io::ErrorKind::TimedOut);
    }
    assert_source_without_vectors(&f).await;
}

#[tokio::test]
async fn semantic_cursor_requires_eof_and_emits_only_active_bounded_batches() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<KnowledgeEmbeddingCapture>();
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    use graphit_data::knowledge_references::{
        KnowledgeDocumentState, KnowledgeReferenceFields, KnowledgeReferenceNamespace,
        KnowledgeReferenceScope, prepare_knowledge_references,
    };
    let pages = [page("a", &[]), page("b", &[]), page("c", &[])];
    let prepared = h::prepare(ctx.snapshot(), &pages, &[], hlimits()).unwrap();
    let graphit_core::artifact_store::StoreOwner::Project { project_id } = f.project.owner() else {
        panic!("actual PROJECT fixture")
    };
    let namespace = KnowledgeReferenceNamespace {
        scope: KnowledgeReferenceScope::Project(*project_id),
        context: None,
    };
    let fields: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|slug| KnowledgeReferenceFields {
            slug,
            state: if *slug == "b" {
                KnowledgeDocumentState::Removed
            } else {
                KnowledgeDocumentState::Active
            },
            namespace,
            authored: &[],
        })
        .collect();
    let references =
        prepare_knowledge_references(ctx.snapshot(), &prepared, &fields, write_limits().content)
            .unwrap();
    publish_native_authored_with_references(
        ctx.snapshot(),
        &write,
        &prepared,
        &references,
        write_limits(),
    )
    .await
    .unwrap();
    let KnowledgeEmbeddingRead::Published(corpus) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("actual source corpus")
    };
    let id = identity(7, 2);
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &corpus,
            &id,
            &[],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidInput(
            "document cursor not complete"
        ))
    ));
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &read,
            &corpus,
            &id,
            &[],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2,
            },
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidInput(
            "document cursor not complete"
        ))
    ));
    assert_source_without_vectors(&f).await;
    let mut slugs = Vec::new();
    let mut saw_empty_removed = false;
    while let Some(batch) = corpus.next_documents(ctx.snapshot(), &read).await.unwrap() {
        assert!(batch.len() <= read_limits().batch_rows.get() as usize);
        saw_empty_removed |= batch.is_empty();
        slugs.extend(batch.into_iter().map(|doc| doc.slug));
    }
    slugs.sort();
    assert_eq!(slugs, ["a", "c"]);
    assert!(saw_empty_removed);
    assert!(
        corpus
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &corpus,
            &id,
            &[],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
        KnowledgeEmbeddingPublication::Unchanged
    ));
    committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &corpus,
            &id,
            &[row("a", 1, &[1., 0.]), row("c", 3, &[0., 1.])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
}

#[tokio::test]
async fn semantic_cursor_metadata_preflight_rejects_late_corruption_and_duplicates() {
    for defect in ["state", "duplicate", "complete"] {
        setup!(cancel, ctx, f, issuer, authority, read, write, grant);
        let source = publish_source(&ctx, &write, &["a", "b", "c"]).await;
        let limits = read_limits();
        if defect != "complete" {
            let dataset = Arc::new(Dataset::open(&source.default.corpus_uri).await.unwrap());
            let mut batches = sdk_batches(&dataset).await;
            for batch in &mut batches {
                let slugs = batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<arrow_array::StringArray>()
                    .unwrap();
                let mut columns = batch.columns().to_vec();
                if defect == "state" {
                    columns[16] = Arc::new(arrow_array::UInt8Array::from_iter_values(
                        (0..batch.num_rows())
                            .map(|row| if slugs.value(row) == "c" { 3 } else { 1 }),
                    ));
                } else {
                    columns[0] = Arc::new(arrow_array::StringArray::from_iter_values(
                        (0..batch.num_rows()).map(|row| {
                            if slugs.value(row) == "c" {
                                "a"
                            } else {
                                slugs.value(row)
                            }
                        }),
                    ));
                }
                *batch = RecordBatch::try_new(batch.schema(), columns).unwrap();
            }
            overwrite(dataset, batches).await;
        }
        let result = capture_knowledge_embedding_corpus(ctx.snapshot(), &read, limits).await;
        if defect == "complete" {
            let exact = read_limits();
            let KnowledgeEmbeddingRead::Published(corpus) =
                capture_knowledge_embedding_corpus(ctx.snapshot(), &read, exact)
                    .await
                    .unwrap()
            else {
                panic!("complete corpus")
            };
            let mut count = 0;
            while let Some(batch) = corpus.next_documents(ctx.snapshot(), &read).await.unwrap() {
                count += batch.len();
            }
            assert_eq!(count, 3);
        } else {
            assert!(result.is_err(), "late {defect} was accepted");
        }
        assert_source_without_vectors(&f).await;
    }
}

#[tokio::test]
async fn semantic_cursor_between_batches_preserves_cancel_and_real_revoke_causes() {
    for revoke in [false, true] {
        let cancel = Cancellation::default();
        let ctx = request(&cancel);
        let f = Fixture::new(&ctx);
        let issuer = f.issuer(&ctx);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let admin = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &admin).unwrap();
        let (bearer, reference) = f.issue(&issuer, &parent);
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let authority = ProjectRecordAuthority::new(&issuer);
        let read = authority
            .admit_read(&grant, &ctx, ProjectReadModule::Knowledge)
            .unwrap();
        let write = authority
            .admit_write(&grant, &ctx, ProjectWriteOperation::Knowledge)
            .unwrap();
        publish_source(&ctx, &write, &["a", "b"]).await;
        let KnowledgeEmbeddingRead::Published(corpus) =
            capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!("actual source")
        };
        let first = corpus
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.len(), 1); // prove a real first batch was delivered
        if revoke {
            issuer.revoke(&parent, &bearer).unwrap();
        } else {
            cancel.cancel();
        }
        let error = corpus
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap_err();
        if revoke {
            assert!(matches!(
                error,
                KnowledgeEmbeddingError::Index(KnowledgeIndexError::Auth(AuthError::Denied))
            ));
        } else {
            assert!(
                matches!(error, KnowledgeEmbeddingError::Index(KnowledgeIndexError::Request(ref cause))
                if cause.kind() == std::io::ErrorKind::Interrupted)
            );
        }
        assert!(std::error::Error::source(&error).is_some());
        assert_source_without_vectors(&f).await;
    }
}

#[tokio::test]
async fn semantic_cursor_dropped_inflight_read_cannot_resume_or_publish_a_prefix() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let KnowledgeEmbeddingRead::Published(corpus) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("actual source cursor")
    };
    let mut pending = Box::pin(corpus.next_documents(ctx.snapshot(), &read));
    assert!(futures::poll!(pending.as_mut()).is_pending());
    drop(pending);
    assert!(matches!(
        corpus.next_documents(ctx.snapshot(), &read).await,
        Err(KnowledgeEmbeddingError::InvalidInput(
            "invalid document cursor"
        ))
    ));
    assert!(matches!(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &corpus,
            &identity(7, 2),
            &[],
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidInput(
            "document cursor not complete"
        ))
    ));
    assert_source_without_vectors(&f).await;
}

#[tokio::test]
async fn vector_search_fuses_complete_pinned_rankings_before_top_k_and_binds_nn_cursor() {
    use graphit_data::knowledge_search::*;
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b", "c", "d"]).await;
    let empty_nn = capture(&ctx, &read).await;
    let id = identity(7, 2);
    let inputs = [input("a", 1), input("b", 2), input("c", 3), input("d", 4)];
    let limits = KnowledgeSearchLimits {
        read: read_limits(),
        max_query_bytes: n(4096),
        max_output_bytes: n(65536),
    };
    let page = KnowledgeLexicalQuery {
        text: "literal",
        top_k: 0,
        page_size: n(1),
        cursor: None,
        caller_binding: "actual selected PROJECT",
        preview: false,
    };
    let query = KnowledgeVectorSearchQuery {
        page,
        mode: KnowledgeVectorSearchMode::Hybrid,
        vector: None,
        metric: KnowledgeEmbeddingMetric::L2,
    };
    let no_preview =
        |_: &str, _: &str, _: usize| -> Result<KnowledgeSearchPreview, KnowledgeSearchError> {
            panic!("preview false called formatter")
        };
    let unavailable = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &empty_nn,
        &id,
        &inputs,
        query,
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(unavailable.state, KnowledgeVectorSearchState::Partial);
    assert_eq!(
        unavailable.readiness,
        KnowledgeEmbeddingSearchReadiness::Unavailable(
            KnowledgeEmbeddingUnavailable::MissingSemantic
        )
    );
    assert_eq!(unavailable.total, 4);
    assert!(unavailable.hits[0].semantic_distance.is_none());
    assert!(unavailable.hits[0].rrf_score.is_none());
    let partial_query = KnowledgeVectorSearchQuery {
        page: KnowledgeLexicalQuery { top_k: 5, ..page },
        mode: KnowledgeVectorSearchMode::Semantic,
        vector: Some(&[0., 0.]),
        ..query
    };
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &empty_nn,
        &id,
        &[row("a", 1, &[4., 0.])],
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap();
    let one_nn = capture(&ctx, &read).await;
    let one = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &one_nn,
        &id,
        &inputs,
        partial_query,
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(one.state, KnowledgeVectorSearchState::Hits);
    assert_eq!(one.readiness, KnowledgeEmbeddingSearchReadiness::Ready);
    assert_eq!(&one.generation, one_nn.source_generation());
    assert_eq!(one.total, 1);
    assert_eq!(one.hits[0].source.slug, "a");
    assert_eq!(one.hits[0].semantic_distance, Some(16.));
    assert!(one.next_cursor.is_none());
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &one_nn,
        &id,
        &[row("b", 2, &[2., 0.]), row("c", 3, &[3., 0.])],
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap();
    let two_nn = capture(&ctx, &read).await;
    // c has a stored vector for an obsolete input; d has not been processed yet. Both remain
    // lexical candidates, but only a/b belong to this pin's compatible semantic ranking.
    let with_stale = [input("a", 1), input("b", 2), input("c", 33), input("d", 4)];
    let first_partial = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &two_nn,
        &id,
        &with_stale,
        partial_query,
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(first_partial.total, 2);
    assert_eq!(first_partial.hits[0].source.slug, "b");
    assert_eq!(first_partial.hits[0].semantic_distance, Some(4.));
    let partial_cursor = first_partial.next_cursor.as_ref().unwrap();
    let second_partial = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &two_nn,
        &id,
        &with_stale,
        KnowledgeVectorSearchQuery {
            page: KnowledgeLexicalQuery {
                cursor: Some(partial_cursor),
                ..partial_query.page
            },
            ..partial_query
        },
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(second_partial.total, 2);
    assert_eq!(second_partial.hits[0].source.slug, "a");
    assert_eq!(second_partial.hits[0].semantic_distance, Some(16.));
    assert!(second_partial.next_cursor.is_none());
    let partial_hybrid = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &two_nn,
        &id,
        &with_stale,
        KnowledgeVectorSearchQuery {
            page: KnowledgeLexicalQuery {
                page_size: n(4),
                ..partial_query.page
            },
            mode: KnowledgeVectorSearchMode::Hybrid,
            ..partial_query
        },
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(partial_hybrid.total, 4);
    assert_eq!(partial_hybrid.hits.len(), 4);
    for hit in &partial_hybrid.hits {
        assert!(hit.lexical_score.is_some());
        assert!(hit.rrf_score.is_some());
        assert_eq!(
            hit.semantic_distance,
            match hit.source.slug.as_str() {
                "a" => Some(16.),
                "b" => Some(4.),
                "c" | "d" => None,
                slug => panic!("unexpected partial result {slug}"),
            }
        );
    }
    let none_compatible = [
        input("a", 11),
        input("b", 22),
        input("c", 33),
        input("d", 4),
    ];
    let incomplete = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &two_nn,
        &id,
        &none_compatible,
        KnowledgeVectorSearchQuery {
            vector: None,
            ..partial_query
        },
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(incomplete.state, KnowledgeVectorSearchState::Unavailable);
    assert_eq!(
        incomplete.readiness,
        KnowledgeEmbeddingSearchReadiness::Unavailable(KnowledgeEmbeddingUnavailable::Incomplete)
    );
    assert!(incomplete.hits.is_empty());
    assert_eq!(incomplete.total, 0);
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &two_nn,
        &id,
        &[
            row("a", 1, &[4., 0.]),
            row("b", 2, &[2., 0.]),
            row("c", 3, &[3., 0.]),
            row("d", 4, &[1., 0.]),
        ],
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap();
    let complete = capture(&ctx, &read).await;
    assert_eq!(
        knowledge_embedding_search_readiness(ctx.snapshot(), &read, &complete, &id, &inputs)
            .unwrap(),
        KnowledgeEmbeddingSearchReadiness::Ready
    );
    let query = KnowledgeVectorSearchQuery {
        vector: Some(&[0., 0.]),
        ..query
    };
    assert!(matches!(
        query_native_knowledge_vector_search(
            ctx.snapshot(),
            &read,
            &complete,
            &id,
            &inputs,
            KnowledgeVectorSearchQuery {
                vector: None,
                ..query
            },
            limits,
            no_preview
        )
        .await,
        Err(KnowledgeSearchError::InvalidQuery("missing_query_vector"))
    ));
    let best = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &complete,
        &id,
        &inputs,
        KnowledgeVectorSearchQuery {
            page: KnowledgeLexicalQuery { top_k: 1, ..page },
            ..query
        },
        limits,
        no_preview,
    )
    .await
    .unwrap();
    // b is rank2 in BOTH channels, outside either independent top1. It wins
    // only when the full rankings are fused before top_k, not their top1 union.
    assert_eq!(best.hits[0].source.slug, "b");
    assert_eq!(best.total, 1);
    assert!((best.hits[0].rrf_score.unwrap() - 2.0 / 62.0).abs() < 1e-12);
    assert_eq!(best.hits[0].semantic_distance, Some(4.));
    assert!(best.hits[0].lexical_score.is_some());
    assert_eq!(&best.generation, complete.source_generation());
    let first = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &complete,
        &id,
        &inputs,
        query,
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(first.total, 4);
    let cursor = first.next_cursor.as_ref().unwrap();
    let encoded = serde_json::to_vec(cursor).unwrap();
    let cursor: KnowledgeSearchCursor = serde_json::from_slice(&encoded).unwrap();
    let continuation = KnowledgeVectorSearchQuery {
        page: KnowledgeLexicalQuery {
            cursor: Some(&cursor),
            ..page
        },
        ..query
    };
    let next = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &complete,
        &id,
        &inputs,
        continuation,
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_ne!(first.hits[0].source.slug, next.hits[0].source.slug);
    for changed in [
        KnowledgeVectorSearchQuery {
            vector: Some(&[1., 0.]),
            ..continuation
        },
        KnowledgeVectorSearchQuery {
            mode: KnowledgeVectorSearchMode::Semantic,
            ..continuation
        },
        KnowledgeVectorSearchQuery {
            metric: KnowledgeEmbeddingMetric::Dot,
            ..continuation
        },
    ] {
        assert!(matches!(
            query_native_knowledge_vector_search(
                ctx.snapshot(),
                &read,
                &complete,
                &id,
                &inputs,
                changed,
                limits,
                no_preview
            )
            .await,
            Err(KnowledgeSearchError::InvalidCursor)
        ));
    }
    let semantic = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &complete,
        &id,
        &inputs,
        KnowledgeVectorSearchQuery {
            mode: KnowledgeVectorSearchMode::Semantic,
            ..query
        },
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(semantic.hits[0].source.slug, "d");
    assert!(semantic.hits[0].lexical_score.is_none());
    assert!(semantic.hits[0].rrf_score.is_none());
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &complete,
        &id,
        &[
            row("a", 1, &[4., 0.]),
            row("b", 2, &[2., 0.]),
            row("c", 3, &[3., 0.]),
            row("d", 4, &[5., 0.]),
        ],
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap();
    let new_nn = capture(&ctx, &read).await;
    assert!(new_nn.source_generation().version > complete.source_generation().version);
    assert!(matches!(
        query_native_knowledge_vector_search(
            ctx.snapshot(),
            &read,
            &new_nn,
            &id,
            &inputs,
            continuation,
            limits,
            no_preview
        )
        .await,
        Err(KnowledgeSearchError::GenerationChanged)
    ));
    cancel.cancel();
    let error = query_native_knowledge_vector_search(
        ctx.snapshot(),
        &read,
        &complete,
        &id,
        &inputs,
        query,
        limits,
        no_preview,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, KnowledgeSearchError::Capture(KnowledgeEmbeddingError::Index(KnowledgeIndexError::Request(cause))) if cause.kind()==std::io::ErrorKind::Interrupted)
    );
}

#[tokio::test]
async fn vector_search_requires_real_source_eof_and_verified_empty_without_query_vector() {
    use graphit_data::knowledge_search::*;
    for empty in [false, true] {
        setup!(cancel, ctx, f, issuer, authority, read, write, grant);
        publish_source(&ctx, &write, if empty { &[] } else { &["a"] }).await;
        let KnowledgeEmbeddingRead::Published(raw) =
            capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let id = identity(7, 2);
        let inputs = if empty { vec![] } else { vec![input("a", 1)] };
        let limits = KnowledgeSearchLimits {
            read: read_limits(),
            max_query_bytes: n(4096),
            max_output_bytes: n(65536),
        };
        let query = KnowledgeVectorSearchQuery {
            page: KnowledgeLexicalQuery {
                text: "literal",
                top_k: 0,
                page_size: n(1),
                cursor: None,
                caller_binding: "actual PROJECT",
                preview: false,
            },
            mode: KnowledgeVectorSearchMode::Semantic,
            vector: None,
            metric: KnowledgeEmbeddingMetric::L2,
        };
        let no_preview =
            |_: &str, _: &str, _: usize| -> Result<KnowledgeSearchPreview, KnowledgeSearchError> {
                panic!()
            };
        assert!(matches!(
            query_native_knowledge_vector_search(
                ctx.snapshot(),
                &read,
                &raw,
                &id,
                &inputs,
                query,
                limits,
                no_preview
            )
            .await,
            Err(KnowledgeSearchError::Capture(
                KnowledgeEmbeddingError::InvalidInput(_)
            ))
        ));
        while raw
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_some()
        {}
        let observed = query_native_knowledge_vector_search(
            ctx.snapshot(),
            &read,
            &raw,
            &id,
            &inputs,
            query,
            limits,
            no_preview,
        )
        .await
        .unwrap();
        if empty {
            assert_eq!(observed.state, KnowledgeVectorSearchState::PublishedEmpty);
            assert_eq!(
                observed.readiness,
                KnowledgeEmbeddingSearchReadiness::PublishedEmpty
            );
        } else {
            assert_eq!(observed.state, KnowledgeVectorSearchState::Unavailable);
            assert_eq!(
                observed.readiness,
                KnowledgeEmbeddingSearchReadiness::Unavailable(
                    KnowledgeEmbeddingUnavailable::MissingSemantic
                )
            );
        }
        assert_eq!(observed.total, 0);
        assert!(observed.hits.is_empty());
        assert!(observed.next_cursor.is_none());
    }
}

#[tokio::test]
async fn native_capture_single_clone_preserves_pin_and_requires_source_eof() {
    use graphit_data::lance_branch_store::BranchSnapshot;
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let initial = capture(&ctx, &read).await;
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &initial,
        &id,
        &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap();
    let KnowledgeEmbeddingRead::Published(raw) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        raw.spawn_source_clone(ctx.snapshot(), &read, tempfile::tempdir().unwrap()),
        Err(KnowledgeEmbeddingError::InvalidInput(
            "document cursor not complete"
        ))
    ));
    while raw
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .is_some()
    {}
    let observed = raw.source_generation().clone();
    assert_eq!(raw.semantic_generation().unwrap().version, observed.version);
    let job = raw
        .spawn_source_clone(ctx.snapshot(), &read, tempfile::tempdir().unwrap())
        .unwrap();
    let latest = capture(&ctx, &read).await;
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &latest,
        &identity(8, 3),
        &[row("a", 1, &[1., 0., 0.]), row("b", 2, &[0., 1., 0.])],
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap();
    let cloned = job.await.unwrap().unwrap();
    assert_eq!(raw.source_generation(), &observed);
    let uri = url::Url::from_file_path(cloned.directory())
        .unwrap()
        .to_string();
    let pin = BranchSnapshot::capture_observed(&uri, None, None)
        .await
        .unwrap();
    assert_eq!(
        native_knowledge_embedding_dimensions(pin.schema())
            .unwrap()
            .unwrap()
            .get(),
        2
    );
    assert_eq!(
        cloned
            .native_metadata(ctx.snapshot())
            .await
            .unwrap()
            .manifest()
            .version,
        pin.version()
    );
    let mut scan = pin.scan();
    scan.project(&["slug"]).unwrap();
    scan.nearest("vector", &arrow_array::Float32Array::from(vec![1., 0.]), 2)
        .unwrap()
        .use_index(true)
        .distance_metric(lance_linalg::distance::DistanceType::Cosine);
    let rows = scan
        .try_into_stream()
        .await
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
    assert_eq!(
        rows[0]
            .column_by_name("slug")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow_array::StringArray>()
            .unwrap()
            .value(0),
        "a"
    );
    cancel.cancel();
    assert!(
        raw.spawn_source_clone(ctx.snapshot(), &read, tempfile::tempdir().unwrap())
            .is_err()
    );
}

#[tokio::test]
async fn embedding_source_search_uses_retained_pin_without_model_identity_and_binds_nn_generation()
{
    use graphit_data::knowledge_search::*;
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let KnowledgeEmbeddingRead::Published(raw) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!()
    };
    let limits = KnowledgeSearchLimits {
        read: read_limits(),
        max_query_bytes: n(4096),
        max_output_bytes: n(65536),
    };
    let query = KnowledgeLexicalQuery {
        text: "body",
        top_k: 0,
        page_size: n(1),
        cursor: None,
        caller_binding: "Hybrid Models unavailable observed by caller",
        preview: false,
    };
    let no_preview =
        |_: &str, _: &str, _: usize| -> Result<KnowledgeSearchPreview, KnowledgeSearchError> {
            panic!("preview false")
        };
    // Source was audited to EOF by capture. No Model identity/currentFP or a
    // second source capture is required for this lexical channel.
    let first = query_native_knowledge_embedding_source_search(
        ctx.snapshot(),
        &read,
        &raw,
        query,
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.hits.len(), 1);
    assert!(first.hits[0].score.is_finite());
    let continued = KnowledgeLexicalQuery {
        cursor: first.next_cursor.as_ref(),
        ..query
    };
    let second = query_native_knowledge_embedding_source_search(
        ctx.snapshot(),
        &read,
        &raw,
        continued,
        limits,
        no_preview,
    )
    .await
    .unwrap();
    assert_ne!(first.hits[0].source.slug, second.hits[0].source.slug);
    assert_eq!(first.generation, second.generation);
    assert!(second.next_cursor.is_none());
    let narrow = KnowledgeSearchLimits {
        max_output_bytes: n(1),
        ..limits
    };
    assert!(
        query_native_knowledge_embedding_source_search(
            ctx.snapshot(),
            &read,
            &raw,
            query,
            narrow,
            no_preview
        )
        .await
        .is_err()
    );
    while raw
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .is_some()
    {}
    let id = identity(7, 2);
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &raw,
        &id,
        &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap();
    let KnowledgeEmbeddingRead::Published(with_nn) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!()
    };
    assert!(with_nn.source_generation().version > raw.source_generation().version);
    assert!(with_nn.semantic_generation().is_some());
    assert!(matches!(
        query_native_knowledge_embedding_source_search(
            ctx.snapshot(),
            &read,
            &with_nn,
            continued,
            limits,
            no_preview
        )
        .await,
        Err(KnowledgeSearchError::GenerationChanged)
    ));
    cancel.cancel();
    assert!(
        query_native_knowledge_embedding_source_search(
            ctx.snapshot(),
            &read,
            &with_nn,
            query,
            limits,
            no_preview
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn native_source_clone_handles_empty_corpus_without_model_identity() {
    for empty in [true, false] {
        setup!(cancel, ctx, f, issuer, authority, read, write, grant);
        publish_source(&ctx, &write, if empty { &[] } else { &["a"] }).await;
        let KnowledgeEmbeddingRead::Published(raw) =
            capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        assert!(raw.semantic_generation().is_none());
        assert!(matches!(
            raw.spawn_source_clone(ctx.snapshot(), &read, tempfile::tempdir().unwrap()),
            Err(KnowledgeEmbeddingError::InvalidInput(
                "document cursor not complete"
            ))
        ));
        while raw
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_some()
        {}
        let foreign_cancel = Cancellation::default();
        let foreign = request(&foreign_cancel);
        assert!(matches!(
            raw.spawn_source_clone(foreign.snapshot(), &read, tempfile::tempdir().unwrap()),
            Err(KnowledgeEmbeddingError::Index(
                KnowledgeIndexError::DifferentRequest
            ))
        ));
        let cloned = raw
            .spawn_source_clone(ctx.snapshot(), &read, tempfile::tempdir().unwrap())
            .unwrap()
            .await
            .unwrap()
            .unwrap();
        let uri = url::Url::from_file_path(cloned.directory())
            .unwrap()
            .to_string();
        let pin = Dataset::open(&uri).await.unwrap();
        assert_eq!(pin.count_rows(None).await.unwrap(), usize::from(!empty));
        assert!(
            arrow_schema::Schema::from(pin.schema())
                .field_with_name("vector")
                .is_err()
        );
        cancel.cancel();
        assert!(
            raw.spawn_source_clone(ctx.snapshot(), &read, tempfile::tempdir().unwrap())
                .is_err()
        );
    }
}

#[tokio::test]
async fn selected_documents_share_pins_keep_literal_selection_and_leave_current_inputs_unread() {
    use graphit_data::knowledge_references::{
        KnowledgeDocumentState, KnowledgeReferenceFields, KnowledgeReferenceNamespace,
        KnowledgeReferenceScope, prepare_knowledge_references,
    };
    use graphit_data::knowledge_search::{
        KnowledgeSearchRead, KnowledgeSelectedDocumentsState as State,
        capture_native_knowledge_search,
    };
    for all_removed in [false, true] {
        setup!(cancel, ctx, f, issuer, authority, read, write, grant);
        let literal = "# Selected\r\n\r\n**ação** 日本語 🦀\nbody-tail\n";
        let mut pages = [page("a", &[]), page("b", &[]), page("c", &[])];
        for page in &mut pages {
            page.body = Some(literal);
        }
        let prepared = h::prepare(ctx.snapshot(), &pages, &[], hlimits()).unwrap();
        let graphit_core::artifact_store::StoreOwner::Project { project_id } = f.project.owner()
        else {
            panic!()
        };
        let namespace = KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(*project_id),
            context: None,
        };
        let fields: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|slug| KnowledgeReferenceFields {
                slug,
                state: if all_removed || *slug == "b" {
                    KnowledgeDocumentState::Removed
                } else {
                    KnowledgeDocumentState::Active
                },
                namespace,
                authored: &[],
            })
            .collect();
        let references = prepare_knowledge_references(
            ctx.snapshot(),
            &prepared,
            &fields,
            write_limits().content,
        )
        .unwrap();
        let source = publish_native_authored_with_references(
            ctx.snapshot(),
            &write,
            &prepared,
            &references,
            write_limits(),
        )
        .await
        .unwrap();
        let KnowledgeSearchRead::Published(lexical) =
            capture_native_knowledge_search(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let KnowledgeEmbeddingRead::Published(embedding) =
            capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let wanted = ["c", "a"];
        for selected in [
            lexical
                .read_selected_documents(ctx.snapshot(), &read, &wanted)
                .await
                .unwrap(),
            embedding
                .read_selected_documents(ctx.snapshot(), &read, &wanted)
                .await
                .unwrap(),
        ] {
            assert_eq!(selected.generation, source.default);
            assert_eq!(
                selected.state,
                if all_removed {
                    State::PublishedEmpty
                } else {
                    State::Selected
                }
            );
            assert_eq!(
                selected
                    .documents
                    .iter()
                    .map(|doc| doc.slug.as_str())
                    .collect::<Vec<_>>(),
                if all_removed { vec![] } else { vec!["a", "c"] }
            );
            assert!(selected.documents.iter().all(|doc| doc.body == literal));
        }
        for selected in [
            lexical
                .read_selected_documents(ctx.snapshot(), &read, &[])
                .await
                .unwrap(),
            embedding
                .read_selected_documents(ctx.snapshot(), &read, &[])
                .await
                .unwrap(),
        ] {
            assert_eq!(
                selected.state,
                if all_removed {
                    State::PublishedEmpty
                } else {
                    State::EmptySelection
                }
            );
            assert!(selected.documents.is_empty());
        }
        for wanted in [&["missing"][..], &["b"][..]] {
            for selected in [
                lexical
                    .read_selected_documents(ctx.snapshot(), &read, wanted)
                    .await,
                embedding
                    .read_selected_documents(ctx.snapshot(), &read, wanted)
                    .await,
            ] {
                if all_removed {
                    assert_eq!(selected.unwrap().state, State::PublishedEmpty);
                } else {
                    assert!(matches!(
                        selected,
                        Err(
                            graphit_data::knowledge_search::KnowledgeSearchError::InvalidQuery(
                                "selected_slugs"
                            )
                        )
                    ));
                }
            }
        }
        for wanted in [&[""][..], &["a", "a"][..]] {
            assert!(matches!(
                lexical
                    .read_selected_documents(ctx.snapshot(), &read, wanted)
                    .await,
                Err(
                    graphit_data::knowledge_search::KnowledgeSearchError::InvalidQuery(
                        "selected_slugs"
                    )
                )
            ));
            assert!(matches!(
                embedding
                    .read_selected_documents(ctx.snapshot(), &read, wanted)
                    .await,
                Err(
                    graphit_data::knowledge_search::KnowledgeSearchError::InvalidQuery(
                        "selected_slugs"
                    )
                )
            ));
        }
        // Selected-body reading has its own EOF and must not certify NN inputs.
        assert!(matches!(
            embedding.spawn_source_clone(ctx.snapshot(), &read, tempfile::tempdir().unwrap()),
            Err(KnowledgeEmbeddingError::InvalidInput(
                "document cursor not complete"
            ))
        ));
        let foreign_cancel = Cancellation::default();
        let foreign = request(&foreign_cancel);
        assert!(
            lexical
                .read_selected_documents(foreign.snapshot(), &read, &wanted)
                .await
                .is_err()
        );
        assert!(
            embedding
                .read_selected_documents(foreign.snapshot(), &read, &wanted)
                .await
                .is_err()
        );
        // A genuine SDK replacement publishes a new default; both retained
        // captures continue reading their own documentary generation.
        let store = graphit_data::lance_module_store::LanceModuleStore::open(
            &source.default.corpus_uri,
            physical_schema(),
        )
        .await
        .unwrap();
        let changed = store
            .snapshot()
            .prepare_replace(vec![RecordBatch::new_empty(physical_schema())])
            .await
            .unwrap()
            .commit()
            .await
            .unwrap();
        assert!(changed.snapshot().version() > source.default.version);
        for selected in [
            lexical
                .read_selected_documents(ctx.snapshot(), &read, &wanted)
                .await
                .unwrap(),
            embedding
                .read_selected_documents(ctx.snapshot(), &read, &wanted)
                .await
                .unwrap(),
        ] {
            assert_eq!(selected.generation, source.default);
            assert_eq!(selected.documents.len(), if all_removed { 0 } else { 2 });
        }
        let KnowledgeSearchRead::Published(current) =
            capture_native_knowledge_search(ctx.snapshot(), &read, read_limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            current
                .read_selected_documents(ctx.snapshot(), &read, &["missing"])
                .await
                .unwrap()
                .state,
            State::PublishedEmpty
        );
        cancel.cancel();
        assert!(
            lexical
                .read_selected_documents(ctx.snapshot(), &read, &wanted)
                .await
                .is_err()
        );
        assert!(
            embedding
                .read_selected_documents(ctx.snapshot(), &read, &wanted)
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn selected_documents_enforce_actual_identifier_boundary_in_both_captures() {
    use graphit_data::knowledge_search::{
        KnowledgeSearchError as E, KnowledgeSearchRead, capture_native_knowledge_search,
    };
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let slug = "s".repeat(128);
    publish_source(&ctx, &write, &[&slug]).await;
    let mut limits = read_limits();
    limits.content.max_identifier_bytes = n(128);
    let KnowledgeSearchRead::Published(lexical) =
        capture_native_knowledge_search(ctx.snapshot(), &read, limits)
            .await
            .unwrap()
    else {
        panic!()
    };
    let KnowledgeEmbeddingRead::Published(embedding) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, limits)
            .await
            .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        lexical
            .read_selected_documents(ctx.snapshot(), &read, &[&slug])
            .await
            .unwrap()
            .documents[0]
            .slug,
        slug
    );
    assert_eq!(
        embedding
            .read_selected_documents(ctx.snapshot(), &read, &[&slug])
            .await
            .unwrap()
            .documents[0]
            .slug,
        slug
    );
    let excess = "s".repeat(129);
    assert!(matches!(
        lexical
            .read_selected_documents(ctx.snapshot(), &read, &[&excess])
            .await,
        Err(E::Data(KnowledgeIndexError::LimitExceeded(
            "identifier_bytes"
        )))
    ));
    assert!(matches!(
        embedding
            .read_selected_documents(ctx.snapshot(), &read, &[&excess])
            .await,
        Err(E::Data(KnowledgeIndexError::LimitExceeded(
            "identifier_bytes"
        )))
    ));
}
