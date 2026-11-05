//! Vector streams under genuine Core/Auth/SDK admissions. No Models claim.
include!("support/knowledge_embeddings_fixture.rs");
use futures::StreamExt;
use graphit_data::lance_module_store::{
    DataFusionError, RecordBatchStreamAdapter, SendableRecordBatchStream,
};

fn stream_input(
    identity: &KnowledgeEmbeddingIdentity,
    frames: Vec<Result<RecordBatch, DataFusionError>>,
) -> SendableRecordBatchStream {
    Box::pin(RecordBatchStreamAdapter::new(
        native_knowledge_embedding_schema(identity),
        futures::stream::iter(frames),
    ))
}

fn pending_input(
    identity: &KnowledgeEmbeddingIdentity,
    prefix: Vec<Result<RecordBatch, DataFusionError>>,
) -> (
    SendableRecordBatchStream,
    tokio::sync::mpsc::Sender<Result<RecordBatch, DataFusionError>>,
    tokio::sync::oneshot::Receiver<()>,
) {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let tail = futures::stream::unfold(
        (receiver, Some(entered)),
        |(mut receiver, mut entered)| async move {
            if let Some(entered) = entered.take() {
                let _ = entered.send(());
            }
            receiver
                .recv()
                .await
                .map(|frame| (frame, (receiver, entered)))
        },
    );
    (
        Box::pin(RecordBatchStreamAdapter::new(
            native_knowledge_embedding_schema(identity),
            futures::stream::iter(prefix).chain(tail),
        )),
        sender,
        waiting,
    )
}

async fn concurrent_writer(
    ctx: &RequestContext<'_>,
    f: &Fixture,
    cancel: &Cancellation,
) -> Option<graphit_core::artifact_store::ArtifactWriter> {
    let store = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let original = Arc::clone(ctx.snapshot());
    let mut acquisition = tokio::task::spawn_blocking(move || store.writer_with_context(&original));
    match tokio::time::timeout(Duration::from_secs(5), &mut acquisition).await {
        Ok(result) => Some(result.unwrap().unwrap()),
        Err(_) => {
            // Join even a failing oracle; never leave a lock waiter detached.
            cancel.cancel();
            let _ = acquisition.await.unwrap();
            None
        }
    }
}
fn batch(
    ctx: &RequestContext<'_>,
    write: &ProjectWriteAdmission<'_>,
    id: &KnowledgeEmbeddingIdentity,
    rows: &[KnowledgeEmbeddingRow<'_>],
) -> RecordBatch {
    encode_native_knowledge_embedding_batch(ctx.snapshot(), write, id, rows, write_limits())
        .unwrap()
}
fn semantic_uri(f: &Fixture) -> String {
    f.project
        .runtime_root()
        .join("knowledge/index.lance")
        .to_str()
        .unwrap()
        .to_owned()
}

async fn replace_source(
    ctx: &RequestContext<'_>,
    f: &Fixture,
    uri: &str,
    batches: Vec<RecordBatch>,
) {
    let store = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let writer = store.writer_with_context(ctx.snapshot()).unwrap();
    overwrite(Arc::new(Dataset::open(uri).await.unwrap()), batches).await;
    drop(writer);
}
fn input_error(error: KnowledgeEmbeddingError, kind: std::io::ErrorKind) {
    let KnowledgeEmbeddingError::InputStream(cause) = operation_cause(&error) else {
        panic!("{error:?}")
    };
    let DataFusionError::External(cause) = cause.as_ref() else {
        panic!("input error lost")
    };
    assert_eq!(cause.downcast_ref::<std::io::Error>().unwrap().kind(), kind);
}

fn operation_cause(error: &KnowledgeEmbeddingError) -> &KnowledgeEmbeddingError {
    match error {
        KnowledgeEmbeddingError::CorpusChanged { cause, .. } => operation_cause(cause),
        _ => error,
    }
}

#[tokio::test]
async fn named_branch_keeps_source_nn_history_export_and_retirement_in_one_dataset() {
    use graphit_data::knowledge_document_update::{
        KnowledgeDocumentAuthoredUpdate, KnowledgeDocumentReferenceUpdate,
        KnowledgeDocumentUpdateLimits, update_existing_native_knowledge_documents,
    };
    use graphit_data::knowledge_sync_history::{
        KnowledgeSyncHistoryQuery, KnowledgeSyncHistoryRead, read_native_knowledge_sync_history,
    };
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let main = publish_source(&ctx, &write, &["a"]).await;
    let named_version =
        VersionContext::select(&f.paths, Checkout::NonGit, "native-child", None, "").unwrap();
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
    let uri = semantic_uri(&f);
    let branch = named_version.lance_branch();
    let named = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        &uri,
        Some(branch.as_str()),
        None,
    )
    .await
    .unwrap()
    .materialize()
    .await
    .unwrap();
    let pages = [page("b", &[])];
    let prepared = h::prepare(ctx.snapshot(), &pages, &[], hlimits()).unwrap();
    let references = [KnowledgeDocumentReferenceUpdate {
        slug: "b",
        authored: KnowledgeDocumentAuthoredUpdate::Replace(&[]),
    }];
    let source_batch =
        graphit_data::knowledge_first_publication::encode_native_knowledge_source_batch(
            ctx.snapshot(),
            &named_write,
            &prepared,
            &references,
            write_limits(),
        )
        .unwrap();
    // Offline fixture publication on the real native child, sharing the one
    // canonical dataset. Main remains independently readable at its retained pin.
    let named = named
        .prepare_overwrite(vec![source_batch])
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    assert_eq!(named.physical_branch(), Some(branch.as_str()));
    assert_eq!(named.corpus_uri(), uri);
    for body in ["branch first update", "branch second update"] {
        let captured = capture(&ctx, &named_read).await;
        assert_eq!(captured.documents()[0].slug, "b");
        let mut changed = page("b", &[]);
        changed.body = Some(body);
        let pages = [changed];
        let prepared = h::prepare(ctx.snapshot(), &pages, &[], hlimits()).unwrap();
        update_existing_native_knowledge_documents(
            ctx.snapshot(),
            &named_read,
            &named_write,
            captured.source_generation(),
            &prepared,
            &references,
            KnowledgeDocumentUpdateLimits {
                read: read_limits(),
                write: write_limits(),
            },
        )
        .await
        .unwrap();
    }
    let history = |cursor| KnowledgeSyncHistoryQuery {
        limit: n(1),
        cursor,
    };
    let KnowledgeSyncHistoryRead::Published {
        entries,
        next_cursor: Some(cursor),
        ..
    } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &named_read,
        history(None),
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("two actual documentary transactions are required")
    };
    assert_eq!(entries[0].changes.updated, ["b"]);
    let captured = capture(&ctx, &named_read).await;
    assert_eq!(captured.source_generation().corpus_uri, uri);
    let context =
        capture_native_knowledge_context(ctx.snapshot(), &named_read, read_limits().native)
            .await
            .unwrap()
            .unwrap();
    let graphit_data::knowledge_search::KnowledgeSearchRead::Published(lexical) =
        graphit_data::knowledge_search::capture_native_knowledge_search(
            ctx.snapshot(),
            &named_read,
            read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("named lexical corpus must be published")
    };
    assert_eq!(context.generation().corpus_uri, uri);
    assert_eq!(lexical.generation().corpus_uri, uri);
    let graphit_data::knowledge_snapshot::KnowledgeSnapshotPage::Found { document, .. } = context
        .page(ctx.snapshot(), &named_read, "b", read_limits())
        .await
        .unwrap()
    else {
        panic!("retained context must read named document")
    };
    assert_eq!(document.body, "branch second update");
    let references = context
        .references(
            ctx.snapshot(),
            &named_read,
            graphit_data::knowledge_reference_query::KnowledgeReferenceQuery {
                outgoing: Some("b"),
                incoming: None,
                page_size: n(10),
                outgoing_cursor: None,
                incoming_cursor: None,
            },
            read_limits(),
        )
        .await
        .unwrap();
    assert_eq!(references.focused_source.unwrap().slug, "b");
    let hits = graphit_data::knowledge_search::query_native_knowledge_search(
        ctx.snapshot(),
        &named_read,
        &lexical,
        graphit_data::knowledge_search::KnowledgeLexicalQuery {
            text: "branch",
            top_k: 0,
            page_size: n(10),
            cursor: None,
            caller_binding: "native-child",
            preview: false,
        },
        graphit_data::knowledge_search::KnowledgeSearchLimits {
            read: read_limits(),
            max_query_bytes: n(4096),
            max_output_bytes: n(65536),
        },
        |_, _, _| panic!("preview disabled"),
    )
    .await
    .unwrap();
    assert_eq!(hits.hits.len(), 1);
    assert_eq!(hits.hits[0].source.slug, "b");
    assert_eq!(hits.generation.corpus_uri, uri);
    let KnowledgeSyncHistoryRead::Published {
        entries,
        next_cursor,
        ..
    } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &named_read,
        history(Some(cursor.clone())),
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("native child history continuation remains bound to the same dataset")
    };
    assert_eq!(entries.len(), 1);
    let mut expected_history = entries;
    let mut continuation = next_cursor;
    while let Some(next) = continuation {
        let KnowledgeSyncHistoryRead::Published {
            entries,
            next_cursor,
            ..
        } = read_native_knowledge_sync_history(
            ctx.snapshot(),
            &named_read,
            history(Some(next)),
            read_limits(),
        )
        .await
        .unwrap()
        else {
            panic!("native history must remain published through EOF");
        };
        expected_history.extend(entries);
        continuation = next_cursor;
    }
    assert_eq!(expected_history.len(), 2);
    assert_eq!(expected_history[0].changes.updated, ["b"]);
    assert_eq!(
        expected_history[1].changes.added,
        ["a"],
        "the branch Clone retains the referenced Main create transaction"
    );
    let id = identity(7, 2);
    let vectors = batch(&ctx, &named_write, &id, &[row("b", 7, &[1., 0.])]);
    let (generation, recovery) = committed(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &named_write,
            &captured,
            &id,
            stream_input(&id, vec![Ok(vectors)]),
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let KnowledgeEmbeddingRecoveryKey::Delta(key) = recovery;
    assert_eq!(
        key.corpus_uri(),
        uri,
        "a retained operation cannot retarget the default"
    );
    let KnowledgeIndexRead::Published {
        generation: current_main,
        documents,
    } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("default must remain published")
    };
    assert_eq!(current_main, main.default);
    assert_eq!(documents[0].slug, "a");
    let KnowledgeSyncHistoryRead::Published {
        entries,
        next_cursor,
        ..
    } = read_native_knowledge_sync_history(
        ctx.snapshot(),
        &named_read,
        history(Some(cursor)),
        read_limits(),
    )
    .await
    .unwrap()
    else {
        panic!("same-branch history must survive vector-only commits")
    };
    let mut retained_history = entries;
    let mut continuation = next_cursor;
    while let Some(next) = continuation {
        let KnowledgeSyncHistoryRead::Published {
            entries,
            next_cursor,
            ..
        } = read_native_knowledge_sync_history(
            ctx.snapshot(),
            &named_read,
            history(Some(next)),
            read_limits(),
        )
        .await
        .unwrap()
        else {
            panic!("retained history must remain published through EOF");
        };
        retained_history.extend(entries);
        continuation = next_cursor;
    }
    assert_eq!(
        retained_history, expected_history,
        "vector-only commits preserve every functional history event and cursor cutoff"
    );
    let current = capture(&ctx, &named_read).await;
    assert_eq!(current.semantic_generation().unwrap(), generation);
    let cloned = current
        .spawn_source_clone(ctx.snapshot(), &named_read, tempfile::tempdir().unwrap())
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    let exported = Dataset::open(
        url::Url::from_file_path(cloned.directory())
            .unwrap()
            .as_str(),
    )
    .await
    .unwrap();
    assert_eq!(
        exported
            .count_rows(Some("slug = 'b' AND vector IS NOT NULL".into()))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        exported
            .count_rows(Some("slug = 'a'".into()))
            .await
            .unwrap(),
        0
    );
    assert!(matches!(
        retire_native_knowledge_index(ctx.snapshot(), &named_write, KnowledgeRetirementLimits {
            native: read_limits().native, artifacts: write_limits().artifacts, }).await.unwrap(),
        KnowledgeBranchRetirement::Retired { generation } if generation.corpus_uri == uri
    ));
    let KnowledgeIndexRead::Published {
        generation: current_main,
        documents,
    } = read_native_knowledge_index(ctx.snapshot(), &read, read_limits())
        .await
        .unwrap()
    else {
        panic!("retiring a child cannot remove the canonical Main dataset")
    };
    assert_eq!(current_main, main.default);
    assert_eq!(documents[0].slug, "a");
    let reopened = Dataset::open(&uri).await.unwrap();
    assert!(
        !reopened
            .list_branches()
            .await
            .unwrap()
            .contains_key(branch.as_str())
    );
    assert!(
        f.project
            .runtime_root()
            .join("knowledge/index.lance")
            .is_dir()
    );
    assert!(!f.project.runtime_root().join("knowledge/lineages").exists());
    assert!(!f.project.runtime_root().join("corpora.json").exists());
}

#[tokio::test]
async fn inherited_equal_vectors_and_index_are_unchanged_without_branch_materialization() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a"]).await;
    let id = identity(7, 2);
    let first = capture(&ctx, &read).await;
    let (parent_generation, _) = committed(
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
    let named_version =
        VersionContext::select(&f.paths, Checkout::NonGit, "unchanged-child", None, "").unwrap();
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
    assert_eq!(
        inherited.source_generation().version,
        parent_generation.version
    );
    assert!(inherited.source_generation().physical_branch.is_none());
    assert_eq!(
        inherited.source_generation().inherited_target.as_deref(),
        Some(named_version.lance_branch().as_str())
    );
    let vectors = batch(&ctx, &named_write, &id, &[row("a", 1, &[1., 0.])]);
    assert!(matches!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &named_write,
            &inherited,
            &id,
            stream_input(&id, vec![Ok(vectors)]),
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
        KnowledgeEmbeddingPublication::Unchanged
    ));
    let observed = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        &semantic_uri(&f),
        Some(named_version.lance_branch().as_str()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(observed.version(), parent_generation.version);
    assert!(observed.physical_branch().is_none());
    assert_eq!(
        observed.inherited_target(),
        Some(named_version.lance_branch().as_str())
    );
    let current_parent = capture(&ctx, &read).await;
    assert_eq!(
        current_parent.semantic_generation(),
        Some(parent_generation)
    );
    assert!(matches!(
        query_knowledge_embeddings(
            ctx.snapshot(),
            &named_read,
            &inherited,
            &id,
            &[input("a", 1)],
            KnowledgeEmbeddingQuery {
                vector: &[1., 0.],
                top_k: n(1),
                metric: KnowledgeEmbeddingMetric::L2,
            },
        )
        .await
        .unwrap(),
        KnowledgeSemanticQuery::Hits { hits, .. } if hits.len() == 1
    ));
}

#[tokio::test]
async fn native_index_io_failure_preserves_vectors_and_reuse_retries_only_the_index() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let initial = capture(&ctx, &read).await;
    let blocked = f
        .project
        .runtime_root()
        .join("knowledge/index.lance/_indices");
    assert!(!blocked.exists());
    std::fs::write(&blocked, b"owned fixture blocks native index directory").unwrap();
    let vectors = batch(
        &ctx,
        &write,
        &id,
        &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
    );
    let error = publish_knowledge_embeddings_stream(
        ctx.snapshot(),
        &write,
        &initial,
        &id,
        stream_input(&id, vec![Ok(vectors.clone())]),
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap_err();
    let KnowledgeEmbeddingError::IndexFailed { publication, cause } = operation_cause(&error)
    else {
        panic!("native index IO failure must retain the merge: {error:?}")
    };
    assert!(std::error::Error::source(cause.as_ref()).is_some());
    let KnowledgeEmbeddingPublication::Committed { generation, .. } = publication.as_ref() else {
        panic!("vector merge must be an actual confirmed publication")
    };
    let committed_version = generation.version;
    let pending_index = capture(&ctx, &read).await;
    assert_eq!(pending_index.source_generation().version, committed_version);
    assert_eq!(pending_index.stored_inputs().count(), 2);
    assert!(
        matches!(query_knowledge_embeddings(ctx.snapshot(), &read, &pending_index,
        &id, &[input("a", 1), input("b", 2)], KnowledgeEmbeddingQuery {
            vector: &[1., 0.], top_k: n(2), metric: KnowledgeEmbeddingMetric::L2
        }).await.unwrap(), KnowledgeSemanticQuery::Hits { hits, .. } if hits.len() == 2)
    );
    let before = pending_index
        .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &[input("a", 1), input("b", 2)])
        .await
        .unwrap();
    std::fs::remove_file(&blocked).unwrap();
    let (indexed, _) = committed(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &pending_index,
            &id,
            stream_input(&id, vec![Ok(vectors)]),
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        indexed.version,
        committed_version + 1,
        "only the native index commit is needed for already confirmed vectors"
    );
    let current = capture(&ctx, &read).await;
    assert_eq!(
        current
            .read_reusable_embedding_batch(
                ctx.snapshot(),
                &read,
                &id,
                &[input("a", 1), input("b", 2)]
            )
            .await
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn stream_pending_input_allows_project_writer_between_batches() {
    for (existing, before_first) in [(false, true), (false, false), (true, false)] {
        setup!(cancel, ctx, f, issuer, authority, read, write, grant);
        publish_source(&ctx, &write, &["a", "b"]).await;
        let id = identity(7, 2);
        if existing {
            let captured = capture(&ctx, &read).await;
            publish_knowledge_embeddings(
                ctx.snapshot(),
                &write,
                &captured,
                &id,
                &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
                KnowledgeEmbeddingWriteMode::ReplaceComplete,
                write_limits(),
            )
            .await
            .unwrap();
        }
        let captured = capture(&ctx, &read).await;
        let a = batch(&ctx, &write, &id, &[row("a", 3, &[0.8, 0.2])]);
        let b = batch(&ctx, &write, &id, &[row("b", 2, &[0., 1.])]);
        let prefix = if before_first {
            vec![]
        } else {
            vec![Ok(a.clone())]
        };
        let (input, sender, waiting) = pending_input(&id, prefix);
        let (publication, progressed) = tokio::join!(
            publish_knowledge_embeddings_stream(
                ctx.snapshot(),
                &write,
                &captured,
                &id,
                input,
                KnowledgeEmbeddingWriteMode::ReplaceComplete,
                write_limits(),
            ),
            async {
                waiting.await.unwrap();
                let Some(writer) = concurrent_writer(&ctx, &f, &cancel).await else {
                    return false;
                };
                let pin = Dataset::open(&semantic_uri(&f)).await.unwrap();
                let schema = arrow_schema::Schema::from(pin.schema());
                assert_eq!(
                    native_knowledge_embedding_dimensions(&schema)
                        .unwrap()
                        .is_some(),
                    !before_first
                );
                drop(writer);
                if before_first {
                    sender.send(Ok(a)).await.unwrap();
                }
                sender.send(Ok(b)).await.unwrap();
                drop(sender);
                true
            },
        );
        assert!(progressed, "PROJECT writer blocked by pending producer");
        let (generation, _) = committed(publication.unwrap());
        assert_eq!(
            capture(&ctx, &read).await.semantic_generation(),
            Some(generation)
        );
    }
}

#[tokio::test]
async fn stream_equal_pending_input_allows_unrelated_native_head_advance() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let source = publish_source(&ctx, &write, &["a"]).await;
    let id = identity(7, 2);
    let captured = capture(&ctx, &read).await;
    let (first, _) = committed(
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
        .unwrap(),
    );
    let captured = capture(&ctx, &read).await;
    let (input, sender, waiting) = pending_input(
        &id,
        vec![Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])]))],
    );
    let (result, progressed) = tokio::join!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            input,
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        ),
        async {
            waiting.await.unwrap();
            let Some(writer) = concurrent_writer(&ctx, &f, &cancel).await else {
                return false;
            };
            let dataset = Arc::new(Dataset::open(&source.default.corpus_uri).await.unwrap());
            let batches = sdk_batches(&dataset).await;
            overwrite(dataset, batches).await;
            drop(writer);
            drop(sender);
            true
        },
    );
    assert!(progressed, "equal input retained PROJECT writer");
    assert!(matches!(
        result.unwrap(),
        KnowledgeEmbeddingPublication::Unchanged
    ));
    let after = capture(&ctx, &read).await;
    assert!(after.source_generation().version > first.version);
    assert_eq!(after.stored_inputs().count(), 1);
}

#[tokio::test]
async fn stream_batches_preserve_other_writes_and_reject_incompatible_model_changes() {
    for dimensions in [2, 3] {
        setup!(cancel, ctx, f, issuer, authority, read, write, grant);
        publish_source(&ctx, &write, &["a", "b"]).await;
        let old_id = identity(7, 2);
        let captured = capture(&ctx, &read).await;
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &captured,
            &old_id,
            &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap();
        let captured = capture(&ctx, &read).await;
        let id = identity(7, dimensions);
        let vector = vec![0.5; dimensions as usize];
        let (input, sender, waiting) = pending_input(
            &id,
            vec![Ok(batch(&ctx, &write, &id, &[row("a", 3, &vector)]))],
        );
        let tail = batch(&ctx, &write, &id, &[row("b", 2, &vector)]);
        let (result, winner) = tokio::join!(
            publish_knowledge_embeddings_stream(
                ctx.snapshot(),
                &write,
                &captured,
                &id,
                input,
                KnowledgeEmbeddingWriteMode::ReplaceComplete,
                write_limits(),
            ),
            async {
                waiting.await.unwrap();
                let writer = concurrent_writer(&ctx, &f, &cancel).await?;
                drop(writer);
                let fresh = capture(&ctx, &read).await;
                let (generation, _) = committed(
                    publish_knowledge_embeddings(
                        ctx.snapshot(),
                        &write,
                        &fresh,
                        &old_id,
                        &[row("a", 4, &[0.2, 0.8]), row("b", 2, &[0., 1.])],
                        KnowledgeEmbeddingWriteMode::ReplaceComplete,
                        write_limits(),
                    )
                    .await
                    .unwrap(),
                );
                sender.send(Ok(tail)).await.unwrap();
                drop(sender);
                Some(generation)
            },
        );
        let winner = winner.expect("pending input retained PROJECT writer");
        if dimensions == 2 {
            let (generation, _) = committed(result.unwrap());
            let dataset = Dataset::open(&semantic_uri(&f)).await.unwrap();
            assert!(generation.version > captured.source_generation().version);
            let mut scan = dataset.scan();
            scan.filter("slug = 'a'").unwrap();
            let rows = scan
                .try_into_stream()
                .await
                .unwrap()
                .try_collect::<Vec<_>>()
                .await
                .unwrap();
            let fingerprints = rows[0]
                .column(18)
                .as_any()
                .downcast_ref::<arrow_array::FixedSizeBinaryArray>()
                .unwrap();
            assert_eq!(
                fingerprints.value(0),
                &[4; 32],
                "later B embedding must preserve concurrent A"
            );
        } else {
            let error = result.unwrap_err();
            assert!(
                matches!(operation_cause(&error), KnowledgeEmbeddingError::Conflict),
                "{error:?}"
            );
            assert_eq!(
                capture(&ctx, &read).await.semantic_generation(),
                Some(winner)
            );
        }
    }
}

#[tokio::test]
async fn stream_named_retirement_between_batches_preserves_default_and_reports_real_failure() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let captured = capture(&ctx, &read).await;
    let (default, _) = committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let named = VersionContext::select(&f.paths, Checkout::NonGit, "stage", None, "").unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let admin = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &admin).unwrap();
    let (bearer, reference) = f.issue_for(&issuer, &parent, &named);
    let named_grant = issuer
        .verify(
            bearer.expose_for_transport(),
            &reference,
            f.scope_for(&named),
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
    let (input, sender, waiting) = pending_input(
        &id,
        vec![Ok(batch(
            &ctx,
            &named_write,
            &id,
            &[row("a", 3, &[0.8, 0.2])],
        ))],
    );
    let tail = batch(&ctx, &named_write, &id, &[row("b", 2, &[0., 1.])]);
    let (result, retired) = tokio::join!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &named_write,
            &captured,
            &id,
            input,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        ),
        async {
            waiting.await.unwrap();
            let writer = concurrent_writer(&ctx, &f, &cancel).await?;
            drop(writer);
            let w = write_limits();
            let retired = retire_native_knowledge_index(
                ctx.snapshot(),
                &named_write,
                KnowledgeRetirementLimits {
                    native: w.native,
                    artifacts: w.artifacts,
                },
            )
            .await
            .unwrap();
            // Retirement is terminal for the in-flight branch publication.
            let _ = sender.send(Ok(tail)).await;
            drop(sender);
            Some(retired)
        },
    );
    assert!(matches!(
        retired,
        Some(KnowledgeBranchRetirement::Retired { .. })
    ));
    let error = result.unwrap_err();
    let KnowledgeEmbeddingError::CorpusChanged { generation, cause } = &error else {
        panic!("the confirmed batch must remain observable after retirement: {error:?}")
    };
    assert_eq!(generation.corpus_uri, semantic_uri(&f));
    assert!(generation.version > captured.source_generation().version);
    assert_eq!(
        generation.physical_branch.as_deref(),
        Some(named.lance_branch().as_str())
    );
    assert!(generation.inherited_target.is_none());
    assert!(
        matches!(
            cause.as_ref(),
            KnowledgeEmbeddingError::Conflict | KnowledgeEmbeddingError::Branch(_)
        ),
        "{error:?}"
    );
    assert_eq!(
        capture(&ctx, &read).await.semantic_generation(),
        Some(default)
    );
    let selected = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        &semantic_uri(&f),
        Some(&named.lance_branch()),
        None,
    )
    .await
    .unwrap();
    assert!(
        selected.physical_branch().is_none(),
        "retired named branch was republished"
    );
}

#[tokio::test]
async fn stream_cancel_keeps_source_and_already_published_batch() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let captured = capture(&ctx, &read).await;
    let sentinel = f.project.runtime_root().join("knowledge/foreign-marker");
    std::fs::write(&sentinel, b"preserved").unwrap();
    let (input, sender, waiting) = pending_input(
        &id,
        vec![Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])]))],
    );
    let (result, progressed) = tokio::join!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            input,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        ),
        async {
            waiting.await.unwrap();
            let Some(writer) = concurrent_writer(&ctx, &f, &cancel).await else {
                return false;
            };
            assert!(
                f.project
                    .runtime_root()
                    .join("knowledge/index.lance")
                    .is_dir()
            );
            cancel.cancel();
            drop(sender);
            drop(writer);
            true
        },
    );
    assert!(progressed, "pending input retained PROJECT writer");
    let error = result.unwrap_err();
    assert!(matches!(
        &error,
        KnowledgeEmbeddingError::CorpusChanged { .. }
    ));
    let mut cause: &(dyn std::error::Error + 'static) = &error;
    let interrupted = loop {
        if cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::Interrupted)
        {
            break true;
        }
        match cause.source() {
            Some(next) => cause = next,
            None => break false,
        }
    };
    assert!(interrupted, "{error:?}");
    assert_eq!(
        Dataset::open(&semantic_uri(&f))
            .await
            .unwrap()
            .count_rows(Some("vector IS NOT NULL".into()))
            .await
            .unwrap(),
        1,
        "the earlier validated batch remains available"
    );
    assert_eq!(std::fs::read(sentinel).unwrap(), b"preserved");
}

#[tokio::test]
async fn stream_cancelled_batch_writer_acquisition_preserves_prefix_and_lock_cause() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let captured = capture(&ctx, &read).await;
    let (input, sender, waiting) = pending_input(
        &id,
        vec![Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])]))],
    );
    let tail = batch(&ctx, &write, &id, &[row("b", 2, &[0., 1.])]);
    let (finished, joined) = tokio::sync::oneshot::channel();
    let (result, progressed) = tokio::join!(
        async {
            let result = publish_knowledge_embeddings_stream(
                ctx.snapshot(),
                &write,
                &captured,
                &id,
                input,
                KnowledgeEmbeddingWriteMode::ReplaceComplete,
                write_limits(),
            )
            .await;
            let _ = finished.send(());
            result
        },
        async {
            waiting.await.unwrap();
            let Some(writer) = concurrent_writer(&ctx, &f, &cancel).await else {
                return false;
            };
            sender.send(Ok(tail)).await.unwrap();
            drop(sender);
            // Trigger cancellation while the final section is unavailable. The
            // typed Lock error below, not elapsed time, proves that stage ended.
            tokio::time::sleep(Duration::from_secs(1)).await;
            cancel.cancel();
            joined.await.unwrap();
            drop(writer);
            true
        },
    );
    assert!(progressed, "pending input retained PROJECT writer");
    let error = result.unwrap_err();
    assert!(matches!(
        &error,
        KnowledgeEmbeddingError::CorpusChanged { .. }
    ));
    let KnowledgeEmbeddingError::Index(KnowledgeIndexError::Artifact(cause)) =
        operation_cause(&error)
    else {
        panic!("final writer acquisition cause was lost: {error:?}");
    };
    assert_eq!(
        cause.stage(),
        graphit_core::artifact_store::ArtifactStage::Lock
    );
    assert_eq!(cause.kind(), std::io::ErrorKind::Interrupted);
    assert_eq!(
        Dataset::open(&semantic_uri(&f))
            .await
            .unwrap()
            .count_rows(Some("vector IS NOT NULL".into()))
            .await
            .unwrap(),
        1,
        "the earlier validated batch remains available"
    );
}

#[tokio::test]
async fn stream_first_exact_signed_zero_noop_attach_conflict_and_replace_change() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let captured = capture(&ctx, &read).await;
    assert_eq!(captured.documents().len(), 2);
    let first = stream_input(
        &id,
        vec![
            Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])])),
            Ok(batch(&ctx, &write, &id, &[row("b", 2, &[0., 1.])])),
        ],
    );
    let (generation, _) = committed(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            first,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let current = capture(&ctx, &read).await;
    let equal = stream_input(
        &id,
        vec![
            Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., -0.])])),
            Ok(batch(&ctx, &write, &id, &[row("b", 2, &[-0., 1.])])),
        ],
    );
    assert!(matches!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &current,
            &id,
            equal,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits()
        )
        .await
        .unwrap(),
        KnowledgeEmbeddingPublication::Unchanged
    ));
    assert_eq!(
        capture(&ctx, &read).await.semantic_generation(),
        Some(generation.clone())
    );
    let conflict = stream_input(
        &id,
        vec![Ok(batch(&ctx, &write, &id, &[row("a", 1, &[2., 0.])]))],
    );
    assert!(matches!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &current,
            &id,
            conflict,
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidInput("already embedded"))
    ));
    let changed = stream_input(
        &id,
        vec![
            Ok(batch(&ctx, &write, &id, &[row("a", 1, &[2., 0.])])),
            Ok(batch(&ctx, &write, &id, &[row("b", 2, &[0., 1.])])),
        ],
    );
    let (next, _) = committed(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &current,
            &id,
            changed,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(next.version, generation.version + 1);
    let stored = Dataset::open(&semantic_uri(&f)).await.unwrap();
    assert_eq!(stored.count_rows(None).await.unwrap(), 2);
}

#[tokio::test]
async fn stream_equal_survivor_deletion_only_then_delete_all() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let source = publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let captured = capture(&ctx, &read).await;
    let (generation, _) = committed(
        publish_knowledge_embeddings(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let mut source_data = Dataset::open(&source.default.corpus_uri).await.unwrap();
    let store = ArtifactStore::new(f.project.clone(), write_limits().artifacts);
    let writer = store.writer_with_context(ctx.snapshot()).unwrap();
    // Delete through the real SDK so the surviving native index remains intact.
    // Overwrite would remove that index and correctly require a later repair commit.
    source_data.delete("slug = 'b'").await.unwrap();
    drop(writer);
    let current = capture(&ctx, &read).await;
    let equal = stream_input(
        &id,
        vec![Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])]))],
    );
    let unchanged = publish_knowledge_embeddings_stream(
        ctx.snapshot(),
        &write,
        &current,
        &id,
        equal,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap();
    assert!(matches!(
        unchanged,
        KnowledgeEmbeddingPublication::Unchanged
    ));
    assert!(current.source_generation().version > generation.version);
    let stored = Dataset::open(&semantic_uri(&f)).await.unwrap();
    assert_eq!(stored.count_rows(None).await.unwrap(), 1);
    assert_eq!(stored.version_id(), current.source_generation().version);
    let writer = store.writer_with_context(ctx.snapshot()).unwrap();
    source_data.delete("slug = 'a'").await.unwrap();
    drop(writer);
    let empty = capture(&ctx, &read).await;
    let input = stream_input(&id, vec![Ok(batch(&ctx, &write, &id, &[]))]);
    assert!(matches!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &empty,
            &id,
            input,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits()
        )
        .await
        .unwrap(),
        KnowledgeEmbeddingPublication::Unchanged
    ));
    assert_eq!(
        Dataset::open(&semantic_uri(&f))
            .await
            .unwrap()
            .count_rows(None)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn stream_late_error_preserves_already_published_prefix_and_real_cause() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let captured = capture(&ctx, &read).await;
    let id = identity(7, 2);
    let input = stream_input(
        &id,
        vec![
            Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])])),
            Err(DataFusionError::External(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "tail fixture",
            )))),
        ],
    );
    let error = publish_knowledge_embeddings_stream(
        ctx.snapshot(),
        &write,
        &captured,
        &id,
        input,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap_err();
    input_error(error, std::io::ErrorKind::InvalidData);
    assert_eq!(
        Dataset::open(&semantic_uri(&f))
            .await
            .unwrap()
            .count_rows(Some("vector IS NOT NULL".into()))
            .await
            .unwrap(),
        1,
        "the earlier validated batch remains available"
    );
}

#[tokio::test]
async fn stream_equal_prefix_does_not_hide_late_error_or_touch_existing_head() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a"]).await;
    let captured = capture(&ctx, &read).await;
    let id = identity(7, 2);
    let (first, _) = committed(
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
        .unwrap(),
    );
    let current = capture(&ctx, &read).await;
    let input = stream_input(
        &id,
        vec![
            Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])])),
            Err(DataFusionError::External(Box::new(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "tail fixture",
            )))),
        ],
    );
    let error = publish_knowledge_embeddings_stream(
        ctx.snapshot(),
        &write,
        &current,
        &id,
        input,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap_err();
    input_error(error, std::io::ErrorKind::ConnectionReset);
    assert_eq!(
        capture(&ctx, &read).await.semantic_generation(),
        Some(first)
    );
}

#[tokio::test]
async fn stream_pending_producer_observes_original_cancel_before_birth() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a"]).await;
    let captured = capture(&ctx, &read).await;
    let id = identity(7, 2);
    let (input, _sender, waiting) = pending_input(&id, Vec::new());
    let (result, ()) = tokio::join!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            input,
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        ),
        async {
            waiting
                .await
                .expect("producer must be polled before cancellation");
            cancel.cancel();
        },
    );
    let error = result.unwrap_err();
    let mut cause: &(dyn std::error::Error + 'static) = &error;
    let mut cancelled = false;
    loop {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            cancelled |= io.kind() == std::io::ErrorKind::Interrupted;
        }
        let Some(next) = cause.source() else { break };
        cause = next;
    }
    assert!(cancelled, "{error:?}");
    assert_source_without_vectors(&f).await;
}

#[tokio::test]
async fn stream_rejects_duplicate_incomplete_identity_null_and_nonfinite() {
    for case in 0..5 {
        setup!(cancel, ctx, f, issuer, authority, read, write, grant);
        publish_source(&ctx, &write, &["a", "b"]).await;
        let captured = capture(&ctx, &read).await;
        let id = identity(7, 2);
        let first = batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])]);
        let frames = match case {
            0 => vec![Ok(first.clone()), Ok(first)],
            1 => vec![Ok(first)],
            _ => {
                let mut columns = first.columns().to_vec();
                if case == 2 {
                    columns[4] = Arc::new(arrow_array::StringArray::from(vec!["Query"]));
                } else {
                    let values = if case == 3 {
                        arrow_array::Float32Array::from(vec![Some(1.), None])
                    } else {
                        arrow_array::Float32Array::from(vec![1., f32::NAN])
                    };
                    columns[5] = Arc::new(
                        arrow_array::FixedSizeListArray::try_new(
                            Arc::new(arrow_schema::Field::new(
                                "item",
                                arrow_schema::DataType::Float32,
                                true,
                            )),
                            2,
                            Arc::new(values),
                            None,
                        )
                        .unwrap(),
                    );
                }
                vec![Ok(RecordBatch::try_new(first.schema(), columns).unwrap())]
            }
        };
        let error = publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            stream_input(&id, frames),
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                operation_cause(&error),
                KnowledgeEmbeddingError::InvalidInput(_)
            ),
            "case {case}: {error:?}"
        );
        if case <= 1 {
            assert_eq!(
                Dataset::open(&semantic_uri(&f))
                    .await
                    .unwrap()
                    .count_rows(Some("vector IS NOT NULL".into()))
                    .await
                    .unwrap(),
                1
            );
        } else {
            assert_source_without_vectors(&f).await;
        }
    }
}

#[tokio::test]
async fn stream_reports_missing_source_eof_without_reverting_published_prefix() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a"]).await;
    let KnowledgeEmbeddingRead::Published(captured) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("published")
    };
    let id = identity(7, 2);
    let input = stream_input(
        &id,
        vec![Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])]))],
    );
    let error = publish_knowledge_embeddings_stream(
        ctx.snapshot(),
        &write,
        &captured,
        &id,
        input,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        write_limits(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            operation_cause(&error),
            KnowledgeEmbeddingError::InvalidInput("document cursor not complete")
        ),
        "{error:?}"
    );
    assert!(matches!(
        error,
        KnowledgeEmbeddingError::CorpusChanged { .. }
    ));
    let current = Dataset::open(&semantic_uri(&f)).await.unwrap();
    assert_eq!(
        current
            .count_rows(Some("vector IS NOT NULL".into()))
            .await
            .unwrap(),
        1,
    );
}

#[tokio::test]
async fn stream_publishes_while_document_producer_advances_its_cursor() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let KnowledgeEmbeddingRead::Published(captured) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("published")
    };
    let first = captured
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.len(), 1);
    let id = identity(7, 2);
    let first_batch = batch(&ctx, &write, &id, &[row(&first[0].slug, 1, &[1., 0.])]);
    let (input, sender, waiting) = pending_input(&id, vec![Ok(first_batch)]);
    let (publication, ()) = tokio::join!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            input,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        ),
        async {
            // Asking for the next vector frame happens after the first native
            // commit, while the document cursor still has an unread record.
            waiting.await.unwrap();
            let current = Dataset::open(&semantic_uri(&f)).await.unwrap();
            assert_eq!(
                current
                    .count_rows(Some("vector IS NOT NULL".into()))
                    .await
                    .unwrap(),
                1,
            );
            let next = captured
                .next_documents(ctx.snapshot(), &read)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(next.len(), 1);
            let second = batch(&ctx, &write, &id, &[row(&next[0].slug, 2, &[0., 1.])]);
            sender.send(Ok(second)).await.unwrap();
            assert!(
                captured
                    .next_documents(ctx.snapshot(), &read)
                    .await
                    .unwrap()
                    .is_none()
            );
            drop(sender);
        },
    );
    committed(publication.unwrap());
    let current = Dataset::open(&semantic_uri(&f)).await.unwrap();
    assert_eq!(
        current
            .count_rows(Some("vector IS NOT NULL".into()))
            .await
            .unwrap(),
        2,
    );
}

#[tokio::test]
async fn reusable_batch_preserves_order_pin_and_independent_source_eof() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b", "c"]).await;
    let id = identity(7, 2);
    let initial = capture(&ctx, &read).await;
    let stored = batch(
        &ctx,
        &write,
        &id,
        &[row("a", 1, &[1., 0.]), row("b", 2, &[0., 1.])],
    );
    publish_knowledge_embeddings_stream(
        ctx.snapshot(),
        &write,
        &initial,
        &id,
        stream_input(&id, vec![Ok(stored)]),
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap();
    let KnowledgeEmbeddingRead::Published(captured) =
        capture_knowledge_embedding_corpus(ctx.snapshot(), &read, read_limits())
            .await
            .unwrap()
    else {
        panic!("published")
    };
    let pin = captured.semantic_generation().unwrap();
    let inputs = [input("b", 2), input("a", 1)];
    let expected = batch(
        &ctx,
        &write,
        &id,
        &[row("b", 2, &[0., 1.]), row("a", 1, &[1., 0.])],
    );
    assert_eq!(
        captured
            .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &inputs)
            .await
            .unwrap(),
        expected,
    );
    assert_eq!(captured.semantic_generation().unwrap(), pin);
    assert!(matches!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            stream_input(&id, vec![Ok(expected.clone())]),
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await,
        Err(KnowledgeEmbeddingError::InvalidInput(
            "document cursor not complete"
        )),
    ));
    // Vector merge may reorder physical rows. Reusable reads must leave the
    // independent documentary cursor untouched, including all rows and EOF.
    let mut seen = std::collections::BTreeSet::new();
    while let Some(documents) = captured
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
    {
        for document in documents {
            assert!(seen.insert(document.slug));
        }
    }
    assert_eq!(
        seen,
        ["a", "b", "c"].into_iter().map(str::to_owned).collect()
    );

    // A real newer head must not cause this retained reader to adopt its vectors.
    let newer = batch(
        &ctx,
        &write,
        &id,
        &[row("a", 1, &[8., 0.]), row("b", 2, &[0., 8.])],
    );
    let actual = Dataset::open(&semantic_uri(&f)).await.unwrap();
    let documents = sdk_batches(&actual).await;
    let names = newer
        .column(0)
        .as_any()
        .downcast_ref::<arrow_array::StringArray>()
        .unwrap();
    let mut replaced = Vec::new();
    for at in 0..newer.num_rows() {
        let mut found = None;
        for batch in &documents {
            let slugs = batch
                .column(0)
                .as_any()
                .downcast_ref::<arrow_array::StringArray>()
                .unwrap();
            for row in 0..batch.num_rows() {
                if slugs.value(row) == names.value(at) {
                    assert!(found.is_none());
                    found = Some(batch.slice(row, 1));
                }
            }
        }
        let document = found.unwrap();
        let mut columns = document.columns()[..18].to_vec();
        columns.extend(newer.slice(at, 1).columns()[1..].iter().cloned());
        replaced.push(RecordBatch::try_new(document.schema(), columns).unwrap());
    }
    replace_source(&ctx, &f, &semantic_uri(&f), replaced).await;
    assert_eq!(
        captured
            .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &inputs)
            .await
            .unwrap(),
        expected,
    );
    let current = capture(&ctx, &read).await;
    assert!(current.semantic_generation().unwrap().version > pin.version);
    assert_eq!(
        current
            .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &inputs)
            .await
            .unwrap(),
        batch(
            &ctx,
            &write,
            &id,
            &[row("b", 2, &[0., 8.]), row("a", 1, &[8., 0.])]
        ),
    );
}

#[tokio::test]
async fn reusable_batch_rejects_uncertified_inputs_and_preserves_controls() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let initial = capture(&ctx, &read).await;
    assert_eq!(
        initial
            .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &[])
            .await
            .unwrap(),
        batch(&ctx, &write, &id, &[]),
    );
    assert!(matches!(
        initial
            .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &[input("a", 1)])
            .await,
        Err(KnowledgeEmbeddingError::InvalidInput(_)),
    ));
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &initial,
        &id,
        &[row("a", 1, &[1., 0.])],
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap();
    let captured = capture(&ctx, &read).await;
    for inputs in [
        vec![input("a", 1), input("a", 1)],
        vec![input("a", 9)],
        vec![input("b", 2)],
        vec![input("absent", 1)],
    ] {
        assert!(matches!(
            captured
                .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &inputs)
                .await,
            Err(KnowledgeEmbeddingError::InvalidInput(_)),
        ));
    }
    for other in [
        identity(8, 2),
        identity(7, 3),
        KnowledgeEmbeddingIdentity::new(
            [7; 32],
            NonZeroU32::new(2).unwrap(),
            "other-recipe".into(),
        )
        .unwrap(),
    ] {
        assert!(matches!(
            captured
                .read_reusable_embedding_batch(ctx.snapshot(), &read, &other, &[input("a", 1)])
                .await,
            Err(KnowledgeEmbeddingError::InvalidInput(_)),
        ));
    }
    let foreign_cancel = Cancellation::default();
    let foreign = request(&foreign_cancel);
    assert!(matches!(
        captured
            .read_reusable_embedding_batch(foreign.snapshot(), &read, &id, &[input("a", 1)])
            .await,
        Err(KnowledgeEmbeddingError::Index(
            KnowledgeIndexError::DifferentRequest
        )),
    ));
    let wrong = authority
        .admit_read(&grant, &ctx, ProjectReadModule::Task)
        .unwrap();
    assert!(matches!(
        captured
            .read_reusable_embedding_batch(ctx.snapshot(), &wrong, &id, &[input("a", 1)])
            .await,
        Err(KnowledgeEmbeddingError::Index(
            KnowledgeIndexError::WrongAdmission
        )),
    ));
    cancel.cancel();
    let error = captured
        .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &[input("a", 1)])
        .await
        .unwrap_err();
    assert!(
        matches!(error, KnowledgeEmbeddingError::Index(KnowledgeIndexError::Request(ref cause)) if cause.kind() == std::io::ErrorKind::Interrupted)
    );
}

#[tokio::test]
async fn reusable_batch_preserves_real_revocation() {
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
    let id = identity(7, 2);
    let initial = capture(&ctx, &read).await;
    publish_knowledge_embeddings(
        ctx.snapshot(),
        &write,
        &initial,
        &id,
        &[row("a", 1, &[1., 0.])],
        KnowledgeEmbeddingWriteMode::Attach,
        write_limits(),
    )
    .await
    .unwrap();
    let captured = capture(&ctx, &read).await;
    issuer.revoke(&parent, &bearer).unwrap();
    assert!(matches!(
        captured
            .read_reusable_embedding_batch(ctx.snapshot(), &read, &id, &[input("a", 1)])
            .await,
        Err(KnowledgeEmbeddingError::Index(KnowledgeIndexError::Auth(
            AuthError::Denied
        ))),
    ));
}

#[tokio::test]
async fn selected_batches_survive_other_source_files_and_reset_only_incompatible_vector_spaces() {
    use arrow_array::StringArray;
    use std::collections::BTreeSet;
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    let source = publish_source(&ctx, &write, &["a", "b"]).await;
    let id = identity(7, 2);
    let slugs = BTreeSet::from(["a".to_owned()]);
    let KnowledgeEmbeddingRead::Published(a) =
        capture_knowledge_embedding_selection(ctx.snapshot(), &read, &slugs, read_limits())
            .await
            .unwrap()
    else {
        panic!("Source missing")
    };
    let mut observed = Vec::new();
    while let Some(documents) = a.next_documents(ctx.snapshot(), &read).await.unwrap() {
        observed.extend(documents);
    }
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].slug, "a");
    // Commit B while A's inference is in flight; the complete A row remains genuine.
    let source_batches =
        sdk_batches(&Dataset::open(&source.default.corpus_uri).await.unwrap()).await;
    let updated: Vec<_> = source_batches
        .into_iter()
        .map(|batch| {
            let names = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let body = batch
                .column(2)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let values: Vec<_> = (0..batch.num_rows())
                .map(|row| {
                    if names.value(row) == "b" {
                        "new B during A inference"
                    } else {
                        body.value(row)
                    }
                })
                .collect();
            let mut columns = batch.columns().to_vec();
            columns[2] = Arc::new(StringArray::from(values));
            RecordBatch::try_new(batch.schema(), columns).unwrap()
        })
        .collect();
    replace_source(&ctx, &f, &source.default.corpus_uri, updated).await;
    let encoded = batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])]);
    committed(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &a,
            &id,
            stream_input(&id, vec![Ok(encoded)]),
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let fresh = capture(&ctx, &read).await;
    let result = query_knowledge_embeddings(
        ctx.snapshot(),
        &read,
        &fresh,
        &id,
        &[input("a", 1), input("b", 2)],
        KnowledgeEmbeddingQuery {
            vector: &[1., 0.],
            top_k: n(2),
            metric: KnowledgeEmbeddingMetric::L2,
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(result, KnowledgeSemanticQuery::Hits { hits, .. } if hits.len() == 1 && hits[0].slug == "a")
    );
    // A genuinely different model replaces the old space with its first real batch.
    let new_id = identity(8, 3);
    let KnowledgeEmbeddingRead::Published(next) = capture_knowledge_embedding_selection(
        ctx.snapshot(),
        &read,
        &BTreeSet::from(["b".to_owned()]),
        read_limits(),
    )
    .await
    .unwrap() else {
        panic!("Source missing")
    };
    assert!(next.requires_reindex(&new_id).unwrap());
    while next
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .is_some()
    {}
    let encoded = batch(&ctx, &write, &new_id, &[row("b", 3, &[0., 1., 0.])]);
    committed(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &next,
            &new_id,
            stream_input(&new_id, vec![Ok(encoded)]),
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits(),
        )
        .await
        .unwrap(),
    );
    let final_capture = capture(&ctx, &read).await;
    assert_eq!(final_capture.stored_inputs().count(), 1);
    assert_eq!(final_capture.stored_inputs().next().unwrap().0.slug, "b");
    assert!(!final_capture.requires_reindex(&new_id).unwrap());
    // Documentary removal withdraws the vector atomically, without Models.
    let source_batches =
        sdk_batches(&Dataset::open(&source.default.corpus_uri).await.unwrap()).await;
    let mut without_b = Vec::new();
    for batch in source_batches {
        let names = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for at in 0..batch.num_rows() {
            if names.value(at) != "b" {
                without_b.push(batch.slice(at, 1));
            }
        }
    }
    replace_source(&ctx, &f, &source.default.corpus_uri, without_b).await;
    assert_eq!(capture(&ctx, &read).await.stored_inputs().count(), 0);
    // Changing A itself invalidates an already captured A batch.
    let KnowledgeEmbeddingRead::Published(stale_a) =
        capture_knowledge_embedding_selection(ctx.snapshot(), &read, &slugs, read_limits())
            .await
            .unwrap()
    else {
        panic!("Source missing")
    };
    while stale_a
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .is_some()
    {}
    let source_batches =
        sdk_batches(&Dataset::open(&source.default.corpus_uri).await.unwrap()).await;
    let updated: Vec<_> = source_batches
        .into_iter()
        .map(|batch| {
            let names = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let body = batch
                .column(2)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let values: Vec<_> = (0..batch.num_rows())
                .map(|row| {
                    if names.value(row) == "a" {
                        "new A invalidates captured inference"
                    } else {
                        body.value(row)
                    }
                })
                .collect();
            let mut columns = batch.columns().to_vec();
            columns[2] = Arc::new(StringArray::from(values));
            RecordBatch::try_new(batch.schema(), columns).unwrap()
        })
        .collect();
    replace_source(&ctx, &f, &source.default.corpus_uri, updated).await;
    let encoded = batch(&ctx, &write, &new_id, &[row("a", 1, &[1., 0., 0.])]);
    assert!(matches!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &stale_a,
            &new_id,
            stream_input(&new_id, vec![Ok(encoded)]),
            KnowledgeEmbeddingWriteMode::Attach,
            write_limits()
        )
        .await,
        Ok(KnowledgeEmbeddingPublication::Unchanged)
    ));
}

#[tokio::test]
async fn progressive_batches_preserve_unrelated_vectors_and_drop_changed_or_deleted_documents() {
    setup!(cancel, ctx, f, issuer, authority, read, write, grant);
    publish_source(&ctx, &write, &["a", "b", "c"]).await;
    let captured = capture(&ctx, &read).await;
    let id = identity(7, 2);
    let (stream, sender, waiting) = pending_input(
        &id,
        vec![Ok(batch(&ctx, &write, &id, &[row("a", 1, &[1., 0.])]))],
    );
    let tail = batch(
        &ctx,
        &write,
        &id,
        &[row("b", 2, &[0., 1.]), row("c", 3, &[0.5, 0.5])],
    );
    let (result, ()) = tokio::join!(
        publish_knowledge_embeddings_stream(
            ctx.snapshot(),
            &write,
            &captured,
            &id,
            stream,
            KnowledgeEmbeddingWriteMode::ReplaceComplete,
            write_limits()
        ),
        async {
            waiting.await.unwrap();
            // The first batch is searchable before the producer reaches EOF.
            let partial = capture(&ctx, &read).await;
            assert!(
                matches!(query_knowledge_embeddings(ctx.snapshot(), &read, &partial, &id,
                &[input("a", 1), input("b", 2), input("c", 3)], KnowledgeEmbeddingQuery {
                    vector: &[1., 0.], top_k: n(3), metric: KnowledgeEmbeddingMetric::L2,
                }).await.unwrap(), KnowledgeSemanticQuery::Hits { hits, .. } if hits.len() == 1 && hits[0].slug == "a")
            );
            let writer = concurrent_writer(&ctx, &f, &cancel).await.unwrap();
            let dataset = Arc::new(Dataset::open(&semantic_uri(&f)).await.unwrap());
            let rows = sdk_batches(&dataset).await;
            let mut updated = Vec::new();
            for batch in rows {
                let slugs = batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<arrow_array::StringArray>()
                    .unwrap();
                for at in 0..batch.num_rows() {
                    match slugs.value(at) {
                        "c" => {}
                        "b" => {
                            let row = batch.slice(at, 1);
                            let mut columns = row.columns().to_vec();
                            columns[2] = Arc::new(arrow_array::StringArray::from(vec![
                                "updated while inference waited",
                            ]));
                            for (at, column) in columns.iter_mut().enumerate().skip(18) {
                                *column = arrow_array::new_null_array(
                                    row.schema().field(at).data_type(),
                                    1,
                                );
                            }
                            updated.push(RecordBatch::try_new(row.schema(), columns).unwrap());
                        }
                        _ => updated.push(batch.slice(at, 1)),
                    }
                }
            }
            overwrite(dataset, updated).await;
            drop(writer);
            sender.send(Ok(tail)).await.unwrap();
            drop(sender);
        },
    );
    committed(result.unwrap());
    let dataset = Dataset::open(&semantic_uri(&f)).await.unwrap();
    assert_eq!(dataset.count_rows(None).await.unwrap(), 2);
    assert_eq!(
        dataset
            .count_rows(Some("slug = 'a' AND vector IS NOT NULL".into()))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        dataset
            .count_rows(Some("slug = 'b' AND vector IS NOT NULL".into()))
            .await
            .unwrap(),
        0
    );
    let current = capture(&ctx, &read).await;
    assert_eq!(
        current
            .documents()
            .iter()
            .find(|document| document.slug == "b")
            .unwrap()
            .body,
        "updated while inference waited"
    );
    assert_eq!(current.stored_inputs().count(), 1);
}
