use super::*;
use graphit_daemon::model_service::{MemoryEmbeddingIndexRead, MemoryEmbeddingModel, RpcLimits};
use graphit_records::memory_store::{
    MEMORY_DOCUMENT_RECIPE, MemoryEmbeddingCorpusRead, MemoryEmbeddingIdentity, MemoryListFilter,
    MemorySemanticSearchOptions, MemorySemanticSearchRead, MemoryStoreReadLimits,
    MemoryStoreWriteLimits, NativeMemoryStore,
};
use graphit_records::task_store::TaskLexicalSearchRead;
use std::num::{NonZeroU32, NonZeroUsize};

// Leave room for SDK opens; the inference callback deliberately outlasts STS.
const MEMORY_STS_TTL: Duration = Duration::from_secs(10);
const MEMORY_INFERENCE_DELAY: Duration =
    Duration::from_millis(MEMORY_STS_TTL.as_millis() as u64 + 100);

async fn index_and_query(
    client: &Client,
    broker: &Broker,
    change_during_query: Option<(&Client, &str)>,
) -> (usize, MemorySemanticSearchRead) {
    let policy = &client.server.0.policy;
    let cancel = Cancellation::default();
    // The actual background worker deliberately keeps a continuous corpus request.
    // Storage discovery/STS must still use bounded transport subrequests.
    let request = RequestContext::service(policy.content, &cancel).unwrap();
    let snapshot = client
        .server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("selected project"));
    let (paths, id, version) = &snapshot.projects[0];
    let scope =
        CapturedPublicationScope::project(paths, *id, policy.scope, request.snapshot()).unwrap();
    let binding = scope.binding().unwrap();
    let scope_input = ScopeInput {
        installation: client.server.0.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &scope,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: policy.binding,
        global_version: "",
    };
    let issuer = client.server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&client.raw, &request).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(&caller, &request, scope_input, ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_authenticated_write(
            &caller,
            &request,
            scope_input,
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    let read_limits = MemoryStoreReadLimits {
        native: policy.read.native,
        batch_rows: NonZeroU32::new(1).unwrap(),
        max_query_bytes: NonZeroUsize::new(policy.content.max_input_bytes).unwrap(),
    };
    let write_limits = MemoryStoreWriteLimits {
        read: read_limits,
        artifacts: policy.write.artifacts,
        max_input_bytes: NonZeroUsize::new(policy.content.max_input_bytes).unwrap(),
        max_row_bytes: NonZeroUsize::new(policy.content.max_input_bytes).unwrap(),
    };
    let identity = MemoryEmbeddingIdentity::new(
        [19; 32],
        NonZeroU32::new(2).unwrap(),
        MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let model = MemoryEmbeddingModel::new(request.snapshot(), identity, policy.timeout).unwrap();
    let mut documents = 0;
    let indexed = model
        .index(
            &read,
            &write,
            read_limits,
            write_limits,
            RpcLimits::new(65536, 2, 32768, 16).unwrap(),
            |operation| {
                let first_batch = documents == 0;
                documents += operation.texts.len();
                async move {
                    // The issued STS session expires while expensive inference is
                    // in flight. Preserve that calculated batch and renew only IO.
                    if first_batch {
                        tokio::time::sleep(MEMORY_INFERENCE_DELAY).await;
                    }
                    Ok::<_, io::Error>(vec![vec![1.0, 0.0]; operation.texts.len()])
                }
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!("Memory indexing failed after {documents} inferred documents: {error:?}")
        });
    assert!(matches!(indexed, MemoryEmbeddingIndexRead::Complete { .. }));
    let MemoryEmbeddingCorpusRead::Published(before_query) = NativeMemoryStore::capture_embeddings(
        request.snapshot(),
        &read,
        model.identity().clone(),
        read_limits,
    )
    .await
    .unwrap() else {
        panic!("created remote Memory corpus")
    };
    let query_generation = before_query.generation().clone();
    drop(before_query);
    let calls_after_inference = AtomicUsize::new(0);
    let results =
        model
            .query(
                &read,
                "shared document".into(),
                MemorySemanticSearchOptions {
                    filter: MemoryListFilter::default(),
                    include_deleted_history: false,
                    top_k: NonZeroUsize::new(10).unwrap(),
                },
                read_limits,
                |operation| {
                    let calls_after_inference = &calls_after_inference;
                    // The in-flight mutation is a real nested storage consumer.
                    // Keep its large debug future out of the generic model's
                    // inline state rather than increasing thread stacks.
                    Box::pin(async move {
                        assert!(matches!(
                            operation.input,
                            graphit_daemon::model_service::EmbedInput::Query
                        ));
                        if let Some((other, id)) = change_during_query {
                            let updated = other
                                .memory(
                                    "memory_update",
                                    json!({"id":id,"expected_revision":1,
                                        "idempotency_key":"query-inflight-attributes",
                                        "reason":"concurrent remote revision during query inference",
                                        "important":true}),
                                    0,
                                )
                                .await
                                .unwrap();
                            assert_eq!(updated["native"]["revision"], 2);
                        }
                        // Renew transport credentials after inference without
                        // replacing the already captured Lance version with latest.
                        tokio::time::sleep(MEMORY_INFERENCE_DELAY).await;
                        calls_after_inference
                            .store(broker.state.calls.load(Ordering::SeqCst), Ordering::SeqCst);
                        Ok::<_, io::Error>(vec![vec![1.0, 0.0]; operation.texts.len()])
                    })
                },
            )
            .await
            .unwrap();
    assert!(
        broker.state.calls.load(Ordering::SeqCst) > calls_after_inference.load(Ordering::SeqCst),
        "expired query credentials must be renewed after inference"
    );
    let MemorySemanticSearchRead::Published {
        generation, hits, ..
    } = &results
    else {
        panic!("remote semantic coverage after query credential renewal")
    };
    assert_eq!(
        generation, &query_generation,
        "credential renewal must preserve the captured project, version, branch and SDK snapshot"
    );
    if let Some((other, id)) = change_during_query {
        let hit = hits
            .iter()
            .find(|hit| hit.current.id.to_string() == id)
            .unwrap();
        assert_eq!(hit.current.revision, 1);
        assert_eq!(hit.matched_revision.get(), 1);
        assert!(!hit.current.attributes.important);
        let latest = other
            .memory("memory_get", json!({"id":id}), 0)
            .await
            .unwrap();
        assert_eq!(latest["native"]["memory"]["revision"], 2);
        assert_eq!(latest["native"]["memory"]["attributes"]["important"], true);
        assert!(
            latest["native"]["generation"]["sdk_version"]
                .as_u64()
                .unwrap()
                > query_generation.sdk_version,
            "the independent write must advance latest without changing the captured query snapshot"
        );
    }
    (documents, results)
}

async fn task_index_and_query(client: &Client) -> (usize, TaskLexicalSearchRead) {
    use graphit_records::task_store::{
        NativeTaskStore, TASK_DOCUMENT_RECIPE, TaskEmbeddingAttachment, TaskEmbeddingCorpusRead,
        TaskEmbeddingIdentity, TaskLexicalQuery, TaskSearchFilter, TaskSearchMode,
    };
    let policy = &client.server.0.policy;
    let cancel = Cancellation::default();
    let request = RequestContext::service(policy.content, &cancel).unwrap();
    let snapshot = client
        .server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("selected project"));
    let (paths, id, version) = &snapshot.projects[0];
    let captured =
        CapturedPublicationScope::project(paths, *id, policy.scope, request.snapshot()).unwrap();
    let binding = captured.binding().unwrap();
    let scope = ScopeInput {
        installation: client.server.0.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: policy.binding,
        global_version: "",
    };
    let issuer = client.server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&client.raw, &request).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Task)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Task)
        .unwrap();
    let identity = TaskEmbeddingIdentity::new(
        [23; 32],
        NonZeroU32::new(2).unwrap(),
        TASK_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let corpus = NativeTaskStore::capture_embeddings(
        request.snapshot(),
        &read,
        identity.clone(),
        policy.read,
    )
    .await
    .unwrap();
    let TaskEmbeddingCorpusRead::Published(mut corpus) = corpus else {
        panic!("created remote Task corpus")
    };
    let mut processed = 0;
    while let Some(documents) = corpus
        .next_documents(request.snapshot(), &read, NonZeroUsize::new(1).unwrap())
        .await
        .unwrap()
    {
        processed += documents.len();
        let attachments = documents
            .into_iter()
            .map(|document| TaskEmbeddingAttachment {
                document,
                vector: vec![1., 0.],
            })
            .collect();
        let published = NativeTaskStore::attach_embeddings(
            request.snapshot(),
            &write,
            attachments,
            policy.write,
        )
        .await
        .unwrap();
        assert_eq!(published.attached, 1);
    }
    let result = NativeTaskStore::search_semantic(
        request.snapshot(),
        &read,
        identity,
        &[1., 0.],
        TaskLexicalQuery {
            mode: TaskSearchMode::Semantic,
            text: "remote Task meaning".into(),
            filter: TaskSearchFilter::default(),
            page_size: NonZeroUsize::new(8).unwrap(),
            top_k: NonZeroUsize::new(8).unwrap(),
            cursor: None,
        },
        policy.read,
        NonZeroUsize::new(policy.content.max_input_bytes).unwrap(),
    )
    .await
    .unwrap();
    (processed, result)
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn background_embedding_consumer_uses_remote_table_and_reuses_published_vectors() {
    let broker = Broker::start(S3Fixture::load()).await;
    let a = Client::new(&broker, None);
    let b = Client::new(&broker, Some(&a));
    // The authoritative remote corpus remains remote even before its first
    // publication; consumers must not create a local module.
    let mut local_roots = Vec::new();
    for client in [&a, &b] {
        let snapshot = client
            .server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live storage test selection"));
        for (paths, project_id, _) in snapshot.projects.iter() {
            let runtime = paths.project_runtime(*project_id, &[]).unwrap();
            std::fs::create_dir_all(&runtime).unwrap();
            local_roots.push(runtime);
        }
    }
    let search = json!({"text":"bypass","filter":{},"page_size":8,"top_k":8,"cursor":null});
    let empty_task = task_value(a.task("search", search.clone(), false).await);
    assert!(matches!(
        serde_json::from_value::<TaskLexicalSearchRead>(empty_task["result"].clone()).unwrap(),
        TaskLexicalSearchRead::NoCorpus
    ));
    let session = task_value(a.task("session_create", json!({
        "idempotency_key":"background-remote-session", "title":"Remote corpus bypass",
        "description":"Authoritative S3 records", "strategy":"existing authenticated storage consumers"
    }), true).await);
    let task = task_value(
        a.task(
            "create",
            json!({
                "idempotency_key":"background-remote-task", "session_id":session["result"]["id"],
                "title":"Remote corpus bypass", "description":"Authoritative S3 Task",
                "task_type":"TASK", "priority":1, "checks":[
                    {"kind":"acceptance","text":"Use the admitted remote corpus"},
                    {"kind":"test","text":"Read it despite malformed local bindings"}
                ]
            }),
            true,
        )
        .await,
    );
    let task_id = task["result"]["specification"]["id"].clone();
    let read = task_value(b.task("get", json!({"id":task_id}), false).await);
    assert_eq!(read["result"]["task"]["specification"]["id"], task_id);
    let expected_task_id: graphit_records::task_spec::TaskId =
        serde_json::from_value(task_id).unwrap();
    let found = task_value(b.task("search", search, false).await);
    let TaskLexicalSearchRead::Published { results, .. } =
        serde_json::from_value(found["result"].clone()).unwrap()
    else {
        panic!("published authoritative S3 Task must remain searchable")
    };
    assert!(results.iter().any(|hit| hit.task.id == expected_task_id));
    let (task_processed, task_semantic) = task_index_and_query(&a).await;
    assert_eq!(task_processed, 1);
    let TaskLexicalSearchRead::Published { results, .. } = task_semantic else {
        panic!("remote Task vectors published")
    };
    assert_eq!(results[0].task.id, expected_task_id);
    let (task_processed, task_semantic) = task_index_and_query(&b).await;
    assert_eq!(
        task_processed, 0,
        "independent installation must reuse typed vectors from the S3 Task table"
    );
    let TaskLexicalSearchRead::Published { results, .. } = task_semantic else {
        panic!("independent Task semantic read")
    };
    assert_eq!(results[0].task.id, expected_task_id);
    // These product readers reopen the same remote table after native schema evolution.
    let reopened = task_value(b.task("get", json!({"id":expected_task_id}), false).await);
    assert_eq!(
        reopened["result"]["task"]["specification"]["id"],
        serde_json::to_value(expected_task_id).unwrap()
    );
    let lexical = task_value(
        b.task(
            "search",
            json!({"text":"bypass","filter":{},"page_size":8,"top_k":8}),
            false,
        )
        .await,
    );
    let TaskLexicalSearchRead::Published { results, .. } =
        serde_json::from_value(lexical["result"].clone()).unwrap()
    else {
        panic!("independent remote lexical read after vector schema evolution")
    };
    assert!(results.iter().any(|hit| hit.task.id == expected_task_id));
    // Only Memory inference exercises expiry here; its first credential cache
    // entry must already be short-lived, without expiring unrelated Task reads.
    broker
        .state
        .ttl
        .store(MEMORY_STS_TTL.as_secs(), Ordering::SeqCst);
    let empty_memory = a
        .memory(
            "memory_search",
            json!({"text":"bypass","mode":"fts","top_k":8}),
            0,
        )
        .await
        .unwrap();
    assert_eq!(empty_memory["native"]["state"], "no_corpus");
    let created = a
        .memory("memory_create", memory_input("background-vectors"), 0)
        .await
        .unwrap();
    let id = created["native"]["id"].as_str().unwrap();
    a.memory(
        "memory_create",
        memory_input("background-vectors-second-batch"),
        0,
    )
    .await
    .unwrap();
    let (processed, first) = index_and_query(&a, &broker, Some((&b, id))).await;
    assert_eq!(processed, 2);
    let MemorySemanticSearchRead::Published { hits, .. } = first else {
        panic!("newly committed remote embeddings must be queryable")
    };
    assert!(hits.iter().any(|hit| hit.current.id.to_string() == id));
    let (processed, second) = index_and_query(&b, &broker, None).await;
    assert_eq!(
        processed, 0,
        "another client must reuse embeddings from the authoritative S3 table"
    );
    let MemorySemanticSearchRead::Published { hits, .. } = second else {
        panic!("another client must see remote semantic coverage")
    };
    assert!(hits.iter().any(|hit| hit.current.id.to_string() == id));
    a.assert_no_local_tables();
    b.assert_no_local_tables();
    for runtime in local_roots {
        for module in ["memory", "task"] {
            assert!(
                !runtime.join(module).exists(),
                "S3 consumers must not create a local module"
            );
        }
    }
    a.server.shutdown().await.unwrap();
    b.server.shutdown().await.unwrap();
}
