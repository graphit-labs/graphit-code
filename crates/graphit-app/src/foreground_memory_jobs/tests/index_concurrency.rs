use super::*;
use futures_util::FutureExt;
use graphit_core::artifact_store::ArtifactStore;

#[tokio::test]
async fn project_writer_contention_does_not_serialize_another_project_index() {
    let (mut fixture, mut foreground, tasks, source, raw, selected) =
        browser_ast_search_fixture().await;
    let cancellation = Cancellation::default();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let exercise = std::panic::AssertUnwindSafe(async {
        foreground.supervisors.shutdown().await.unwrap();
        create_memory(&foreground, 0, "writer-contention-a").await;
        create_memory(&foreground, 1, "writer-contention-b").await;
        let resources = foreground.server.as_ref().unwrap().semantic_model().unwrap().unwrap();
        let projects = foreground.projects.as_ref().unwrap();
        let scope = CapturedPublicationScope::project(&projects[0].0, projects[0].1, foreground.scope, original.snapshot()).unwrap();
        let store = ArtifactStore::new(scope, foreground.policy.artifacts());
        let writer = store.writer_with_context(original.snapshot()).unwrap();
        let a = graphit_daemon::supervisor::ProjectContext { id: projects[0].1, version: projects[0].2.clone() };
        let b = graphit_daemon::supervisor::ProjectContext { id: projects[1].1, version: projects[1].2.clone() };
        let mut pending = Box::pin(resources.index(&projects[0].0, &a, &cancellation));
        assert!(futures_util::poll!(pending.as_mut()).is_pending());
        let completed = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::select! {
                result = &mut pending => panic!("A completed while its actual publication writer was held: {result:?}"),
                result = resources.index(&projects[1].0, &b, &cancellation) => result.unwrap(),
            }
        }).await.expect("B indexes while A's project writer is held");
        assert!(matches!(completed, model_api::MemoryEmbeddingIndexRead::Complete { last_attachment: Some(_), .. }));
        assert!(futures_util::poll!(pending.as_mut()).is_pending());
        drop(writer);
        let completed = tokio::time::timeout(Duration::from_secs(30), pending).await.unwrap().unwrap();
        assert!(matches!(completed, model_api::MemoryEmbeddingIndexRead::Complete { last_attachment: Some(_), .. }));

        // The same PROJECT already has a real published Raw/Graph package. Keep
        // Knowledge's actual input pending on model RPC response bytes while
        // a fresh HTTP Graph request captures that publication.
        use graphit_data::{
            knowledge_document_projection::{
                AuthoredPageFields, AuthoredParameters, CompilationDigest32,
                PhysicalSourceDigest32, RecipeFields,
            },
            knowledge_embeddings::{
                KnowledgeEmbeddingInput, KnowledgeEmbeddingPublication, KnowledgeEmbeddingRead,
                KnowledgeEmbeddingRow, KnowledgeEmbeddingWriteMode,
                capture_knowledge_embedding_corpus, encode_native_knowledge_embedding_batch,
                native_knowledge_embedding_schema, publish_knowledge_embeddings_stream,
            },
            knowledge_index::KnowledgeIndexWriteLimits,
            lance_module_store::{DataFusionError, RecordBatchStreamAdapter},
        };
        use graphit_knowledge::embedding_projection::{
            prepare_knowledge_embedding_document, prepare_knowledge_embedding_projection,
        };
        use sha2::{Digest, Sha256};
        use tokio::sync::{mpsc, oneshot};

        let body = "a b";
        let digest = Sha256::digest(body.as_bytes()).into();
        let page = AuthoredPageFields {
            slug: Some("pending-model"), title: Some("a"), body: Some(body),
            kind: Some("guide"), summary: Some("a"), description: None,
            summary_truncated: false, source_path: "docs/pending-model.md",
            physical_digest: Some(PhysicalSourceDigest32::from_bytes(digest)),
            compilation_digest: CompilationDigest32::from_bytes(digest),
            recipe: RecipeFields {
                id: graphit_data::knowledge_document_projection::AUTHORED_RECIPE_ID,
                parameters: Some(AuthoredParameters {
                    fallback_title: "a", kind_on_absent: "guide",
                    max_summary_bytes: NonZeroUsize::new(512).unwrap(),
                }),
            },
            updated_at: None, stale_since: None, stale_after: None,
            resolved_wiki_targets: &[],
        };
        fs::create_dir_all(selected.0.root().join("docs")).unwrap();
        fs::write(selected.0.root().join(page.source_path), body).unwrap();
        crate::task_server::browser::knowledge_fixture::seed(&tasks, &raw, 0, &[page]).await;

        let auth = Arc::clone(&foreground.auth);
        let issuer = auth.issuer();
        let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
        let scope = CapturedPublicationScope::project(
            &selected.0, selected.1, foreground.scope, original.snapshot(),
        ).unwrap();
        let record = scope.binding().unwrap();
        let input = || ScopeInput {
            installation: auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture { live: &scope, record: &record },
                version: &selected.2,
            }),
            providers: &[], binding: NativeBinding::KiroV3, global_version: "",
        };
        let authority = ProjectRecordAuthority::new(issuer);
        let read = authority.admit_authenticated_read(
            &caller, &original, input(), ProjectReadModule::Knowledge,
        ).unwrap();
        let write = authority.admit_authenticated_write(
            &caller, &original, input(), ProjectWriteOperation::Knowledge,
        ).unwrap();
        let mut service = issuer.capture_service(
            &caller, graphit_auth::ServicePurpose::Embedding,
        ).unwrap();
        let lease = service.local().unwrap();
        let policy = crate::foreground_tasks::task_policy(
            &foreground.policy, foreground.scope, foreground.native, NativeBinding::KiroV3,
        ).unwrap();
        let read_limits = crate::foreground_knowledge::limits(&policy).unwrap();
        let write_limits = KnowledgeIndexWriteLimits {
            content: read_limits.content, native: foreground.native,
            artifacts: foreground.policy.artifacts(),
        };
        let capture = match capture_knowledge_embedding_corpus(
            original.snapshot(), &read, read_limits,
        ).await.unwrap() {
            KnowledgeEmbeddingRead::Published(capture) => capture,
            KnowledgeEmbeddingRead::NoCorpus => panic!("real authored Knowledge publication"),
        };
        let source_generation = capture.source_generation().clone();
        let identity = prepare_knowledge_embedding_projection(
            original.snapshot(), &capture, &[], resources.local.as_ref().unwrap().expected.vector_id(),
            resources.local.as_ref().unwrap().expected.dimensions().unwrap(),
        ).unwrap().identity().clone();
        let documents = capture.next_documents(original.snapshot(), &read).await.unwrap().unwrap();
        assert_eq!(documents.len(), 1);
        let projected = prepare_knowledge_embedding_document(
            original.snapshot(), &capture, &documents[0], &identity,
        ).unwrap();
        let slug = projected.slug().to_owned();
        let fingerprint = projected.input_fingerprint();
        let text = projected.into_text();
        assert!(capture.next_documents(original.snapshot(), &read).await.unwrap().is_none());

        let (input_entered, entered) = oneshot::channel();
        let (sender, receiver) = mpsc::channel::<Result<arrow_array::RecordBatch, DataFusionError>>(1);
        let stream = Box::pin(RecordBatchStreamAdapter::new(
            native_knowledge_embedding_schema(&identity),
            futures_util::stream::unfold((receiver, Some(input_entered)), |(mut receiver, first)| async move {
                // This event comes from the real publisher polling its input,
                // after any writer acquisition that precedes stream consumption.
                if let Some(entered) = first { let _ = entered.send(()); }
                receiver.recv().await.map(|frame| (frame, (receiver, None)))
            }),
        ));
        let (arrived, arrival) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let produce = async {
            let batch: io::Result<arrow_array::RecordBatch> = async {
                original.check()?;
                read.recheck().map_err(io::Error::other)?;
                let output = resources.local.as_ref().unwrap().client.embed(
                    &lease, auth.runtime(), &resources.local.as_ref().unwrap().expected,
                    model_api::EmbedOperation {
                        input: EmbedInput::Document, texts: vec![text],
                        deadline: original.snapshot().required_deadline()?,
                    },
                    async {
                        Ok::<_, io::Error>(super::model_reload::ModelResponseBoundary {
                            socket: TcpStream::connect(resources.local.as_ref().unwrap().address).await?,
                            arrived: Some(arrived), release: Some(released), buffered: Vec::new(),
                        })
                    },
                ).await.map_err(io::Error::other)?;
                lease.check().map_err(io::Error::other)?;
                read.recheck().map_err(io::Error::other)?;
                encode_native_knowledge_embedding_batch(
                    original.snapshot(), &write, &identity,
                    &[KnowledgeEmbeddingRow {
                        input: KnowledgeEmbeddingInput { slug: &slug, input_fingerprint: fingerprint },
                        vector: &output.vectors[0],
                    }],
                    write_limits,
                ).map_err(io::Error::other)
            }.await;
            let _ = sender.send(batch.map_err(DataFusionError::IoError)).await;
            drop(sender);
        };
        let publish = publish_knowledge_embeddings_stream(
            original.snapshot(), &write, &capture, &identity, stream,
            KnowledgeEmbeddingWriteMode::ReplaceComplete, write_limits,
        );
        let mut pending = Box::pin(async {
            let (produced, publication) = tokio::join!(
                std::panic::AssertUnwindSafe(produce).catch_unwind(),
                std::panic::AssertUnwindSafe(publish).catch_unwind(),
            );
            produced.unwrap();
            publication.unwrap()
        });
        let listener = foreground.listener.take().unwrap();
        let server = source.clone();
        let serving = tokio::spawn(async move { server.serve(listener).await });
        let mut completed = None;
        let consumer = std::panic::AssertUnwindSafe(async {
            tokio::time::timeout(Duration::from_secs(30), async {
                tokio::select! {
                    reached = async { entered.await.unwrap(); arrival.await.unwrap(); } => reached,
                    result = pending.as_mut() => {
                        completed = Some(result);
                        panic!("Knowledge completed before its real Models response was released");
                    },
                }
                let reads = async {
                    let selection = || crate::graph_entry::Selection {
                        project_dir: Some(selected.0.root().to_owned()),
                        project_version: Some(selected.2.version().to_owned()), context: None,
                    };
                    let commands = [
                        crate::graph_entry::Command::Schema(selection()),
                        crate::graph_entry::Command::Query(crate::graph_entry::Query {
                            selection: selection(),
                            query: "MATCH (f:AstFileObservation) WHERE f.path = $path RETURN f.path".into(),
                            parameters: vec![("path".into(), json!("browser_search.rs"))],
                            string_parameters: Vec::new(),
                        }),
                    ];
                    let client = reqwest::Client::new();
                    let mut results = Vec::<serde_json::Value>::new();
                    for command in commands {
                        let mut output = Vec::new();
                        crate::graph_entry::run(
                            &fixture.installation, &client,
                            command.prepare(source.policy().content).unwrap(),
                            (source.policy().index, source.policy().content), &original, &mut output,
                        ).await.unwrap();
                        results.push(serde_json::from_slice(&output).unwrap());
                    }
                    results
                };
                tokio::select! {
                    results = reads => results,
                    result = pending.as_mut() => {
                        completed = Some(result);
                        panic!("Knowledge ended before Graph captured the independent published Copy");
                    },
                }
            }).await.expect("Graph HTTP completes while Knowledge awaits real Models delivery")
        }).catch_unwind().await;
        let _ = release.send(());
        let publication = match completed {
            Some(publication) => Ok(publication),
            None => std::panic::AssertUnwindSafe(pending.as_mut()).catch_unwind().await,
        };
        drop(pending);
        let shutdown = std::panic::AssertUnwindSafe(source.shutdown()).catch_unwind().await;
        let served = serving.await;
        let results = match consumer {
            Ok(results) => results,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        let publication = match publication {
            Ok(publication) => publication,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        shutdown.unwrap().unwrap();
        served.unwrap().unwrap();
        assert!(serde_json::to_string(&results[0]).unwrap().contains("AstFileObservation"));
        assert_eq!(results[1]["rows"], json!([[{"type":"STRING","value":"browser_search.rs"}]]));
        assert!(matches!(publication.unwrap(), KnowledgeEmbeddingPublication::Committed { .. }));
        assert_eq!(capture.source_generation(), &source_generation);
    }).catch_unwind().await;
    stop_browser_model(&mut fixture, &mut foreground).await;
    let stopped = foreground.shutdown().await;
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    stopped.unwrap();
}
