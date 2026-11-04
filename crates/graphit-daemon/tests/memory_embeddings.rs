#![cfg(target_os = "linux")]
#[path = "support/memory_model.rs"]
mod auth;
#[path = "support/memory_gemma.rs"]
mod gemma;
#[path = "support/model_fixture.rs"]
mod model_fixture;
#[path = "support/model_service_revocation.rs"]
mod model_service_revocation;
#[path = "support/model_rpc.rs"]
mod rpc;
use graphit_ai::{
    context::InferenceContext, embedding::EmbeddingLimits, native_embedding::NativeSdkRoot,
    native_rerank::NativeRerankLimits,
};
use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ServicePurpose,
};
use graphit_core::file_lock::Cancellation;
use graphit_daemon::model_child as child_api;
use graphit_daemon::model_service as model_api;
use graphit_daemon::model_transport as disconnect_api;
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store as memory_api,
    memory_store::*,
};
use model_api::*;
use std::{
    io,
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

fn input(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: key.into(),
        body: "Paris is the capital of France.".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: false,
            important: false,
        },
        tags: vec!["geography".into()],
        references: None,
    }
}
fn options() -> MemorySemanticSearchOptions {
    MemorySemanticSearchOptions {
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(2).unwrap(),
    }
}
fn secret(
    f: &auth::Fixture,
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> Arc<RuntimeSecret> {
    let key = f.runtime().check(original).unwrap();
    Arc::new(RuntimeSecret::new(Zeroizing::new(*key.secret(original).unwrap())).unwrap())
}

// Keep each real consumer future's unoptimized temporaries in its own poll frame.
// The long journey otherwise reserves over 1 MiB before entering Lance/Arrow.
fn boxed_phase<'a, T: 'a, F: std::future::Future<Output = T> + 'a>(
    make: impl FnOnce() -> F,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + 'a>> {
    Box::pin(make())
}

#[tokio::test]
#[ignore = "requires the pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn actual_memory_gemma_rpc_index_query_identity_and_partial_failure() {
    let (_snapshot_root, snapshot_parent) = model_fixture::snapshot_parent();
    let model_path = std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL").expect("pinned BF16 GGUF");
    let model = gemma::GemmaFixture::new(std::path::Path::new(&model_path));
    let cancel = Cancellation::default();
    let ctx = auth::request_until(&cancel, Instant::now() + Duration::from_secs(600));
    let f = auth::Fixture::new(&ctx, "main");
    let (issuer, store) = f.issuer(&ctx);
    let key = f.runtime().check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let config = model
        .config(
            &[("a", 768), ("mrl", 128)],
            ctx.snapshot(),
            NonZeroU32::new(2).unwrap(),
        )
        .inspect_err(|error| {
            let mut cause = Some(error as &dyn std::error::Error);
            while let Some(error) = cause {
                eprintln!("Memory GGUF configuration cause: {error}");
                if let Some(error) = error.downcast_ref::<io::Error>() {
                    eprintln!(
                        "Memory GGUF configuration IO: {:?} {:?}",
                        error.kind(),
                        error.raw_os_error()
                    );
                }
                cause = error.source();
            }
        })
        .unwrap();
    let a = ExpectedModel::new(&config, "a", "service").unwrap();
    let mrl = ExpectedModel::new(&config, "mrl", "service").unwrap();
    let sdk = NativeSdkRoot::new().unwrap();
    let service = ModelService::new(
        snapshot_parent.clone(),
        ServiceLimits {
            max_active_workers: NonZeroUsize::new(16).unwrap(),
            embedding: EmbeddingLimits::new(32, 65536, 768, 24576, 8, 131072, 131072).unwrap(),
            rerank: NativeRerankLimits::new(32, 8, 65536, 131072, 131072).unwrap(),
        },
    )
    .unwrap();
    service
        .install(
            config,
            InferenceContext::new(&cancel, ctx.snapshot().required_deadline().unwrap()),
        )
        .unwrap();
    let limits = RpcLimits::new(131072, 32, 65536, 24576).unwrap();
    let client = ModelServiceClient::new(limits).unwrap();
    let consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: f.runtime(),
        expected: &a,
    };
    let consumer = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    assert_eq!(consumer.identity().vector_id(), &a.vector_id());
    let mut calls = 0;
    let absent = boxed_phase(|| {
        consumer.index(
            &read,
            &write,
            auth::read_limits(),
            auth::write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("unexpected connection")) }
                })
            },
        )
    })
    .await
    .unwrap();
    assert!(matches!(absent, MemoryEmbeddingIndexRead::NoCorpus));
    assert_eq!(calls, 0);
    assert!(matches!(
        boxed_phase(|| consumer.query(
            &read,
            "Paris".into(),
            options(),
            auth::read_limits(),
            |operation| consumer_inference.infer(operation, async {
                Err::<TcpStream, _>(io::Error::other("unexpected query connection"))
            })
        ))
        .await
        .unwrap(),
        MemorySemanticSearchRead::NoCorpus
    ));
    let other = auth::request_until(&cancel, ctx.snapshot().required_deadline().unwrap());
    assert!(matches!(
        MemoryEmbeddingModel::new(other.snapshot(), consumer_inference.identity(), std::time::Duration::from_secs(120))
            .unwrap()
            .index(&read, &write, auth::read_limits(), auth::write_limits(), consumer_inference.limits(), |operation| {
                consumer_inference.infer(operation, async { Err::<TcpStream, _>(io::Error::other("foreign request must not connect")) })
            }).await
            .err()
            .unwrap()
            .cause,
        MemoryEmbeddingCause::Store(ref e) if matches!(**e, MemoryStoreError::DifferentRequest)
    ));
    let fresh_key = f.runtime().check(other.snapshot()).unwrap();
    let fresh_caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(fresh_key.secret(other.snapshot()).unwrap()).unwrap(),
            &other,
        )
        .unwrap();
    let fresh_write = authority
        .admit_authenticated_write(
            &fresh_caller,
            &other,
            f.scope(),
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    // Obtain schema through the real Records codec and real native model output.
    // Slice to zero rows before publishing: no probe Memory or embedding is persisted.
    let schema_memory = Memory {
        id: "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap(),
        revision: 1,
        title: "schema probe".into(),
        body: "Paris".into(),
        attributes: input("schema").attributes,
        tags: vec![],
        references: vec![],
        reference_generation: 0,
        created_by: "fixture".into(),
        updated_by: "fixture".into(),
        created_at: 1,
        updated_at: 1,
    };
    let schema_document =
        prepare_memory_embedding_document(&schema_memory, consumer.identity()).unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
    let output = client
        .embed(
            &lease,
            f.runtime(),
            &a,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Document,
                texts: vec![schema_document.text().to_owned()],
                deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    task.await.unwrap().unwrap();
    let schema_probe = encode_memory_embedding_row(
        "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
        &schema_document,
        &output.vectors[0],
        auth::write_limits(),
    )
    .unwrap()
    .slice(0, 0);
    let artifact_store = graphit_core::artifact_store::ArtifactStore::new(
        f.project.clone(),
        auth::write_limits().artifacts,
    );
    let original = Arc::clone(ctx.snapshot());
    let mut directory =
        tokio::task::spawn_blocking(move || artifact_store.writer_with_context(&original))
            .await
            .unwrap()
            .unwrap();
    directory
        .create_record_directory(
            graphit_core::artifact_store::ProjectRecordDirectory::Memory,
            auth::read_limits().native,
        )
        .unwrap();
    let uri = url::Url::from_file_path(f.project.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    graphit_data::lance_module_store::LanceModuleStore::create(
        &uri,
        schema_probe.schema(),
        vec![schema_probe],
    )
    .await
    .unwrap();
    drop(directory);
    let mut empty_calls = 0;
    assert!(matches!(
        boxed_phase(|| consumer.index(
            &read,
            &write,
            auth::read_limits(),
            auth::write_limits(),
            consumer_inference.limits(),
            |operation| consumer_inference.infer(operation, {
                empty_calls += 1;
                async { Err::<TcpStream, _>(io::Error::other("unexpected empty connection")) }
            })
        ))
        .await
        .unwrap(),
        MemoryEmbeddingIndexRead::Complete {
            last_attachment: None,
            ..
        }
    ));
    assert_eq!(empty_calls, 0);
    assert!(matches!(
        boxed_phase(|| consumer.query(
            &read,
            "Paris".into(),
            options(),
            auth::read_limits(),
            |operation| consumer_inference.infer(operation, async {
                Err::<TcpStream, _>(io::Error::other("unexpected empty query connection"))
            })
        ))
        .await
        .unwrap(),
        MemorySemanticSearchRead::PublishedEmpty { .. }
    ));
    let first =
        NativeMemoryStore::create(ctx.snapshot(), &write, input("Paris"), auth::write_limits())
            .await
            .unwrap();
    for expected in [&mrl, &a] {
        let consumer_inference = auth::LocalInference {
            client: &client,
            lease: &lease,
            runtime: f.runtime(),
            expected,
        };
        let consumer = MemoryEmbeddingModel::new(
            ctx.snapshot(),
            consumer_inference.identity(),
            std::time::Duration::from_secs(120),
        )
        .unwrap();
        let mut tasks = Vec::new();
        let indexed = boxed_phase(|| {
            consumer.index(
                &read,
                &write,
                auth::read_limits(),
                auth::write_limits(),
                consumer_inference.limits(),
                |operation| {
                    consumer_inference.infer(operation, {
                        let (address, _, task) =
                            rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
                        tasks.push(task);
                        TcpStream::connect(address)
                    })
                },
            )
        })
        .await
        .unwrap();
        let MemoryEmbeddingIndexRead::Complete {
            last_attachment: Some(attachment),
            ..
        } = indexed
        else {
            panic!("actual SDK publication");
        };
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        let MemoryEmbeddingAttachOutcome::Published(publication) = *attachment else {
            panic!("actual SDK publication");
        };
        assert!(matches!(
            NativeMemoryStore::resolve_embeddings(
                other.snapshot(),
                &fresh_write,
                &publication.recovery_key,
                auth::write_limits()
            )
            .await
            .unwrap(),
            graphit_data::lance_branch_store::DeltaResolution::Applied { .. }
        ));
        let MemoryEmbeddingCorpusRead::Published(mut covered) =
            NativeMemoryStore::capture_embeddings(
                ctx.snapshot(),
                &read,
                consumer.identity().clone(),
                auth::read_limits(),
            )
            .await
            .unwrap()
        else {
            panic!("published covered corpus");
        };
        assert!(!covered.is_empty());
        assert!(
            covered
                .next_documents(ctx.snapshot(), &read)
                .await
                .unwrap()
                .is_none()
        );
        drop(covered);
        let (address, _, task) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
        let result = boxed_phase(|| {
            consumer.query(
                &read,
                "capital of France".into(),
                options(),
                auth::read_limits(),
                |operation| consumer_inference.infer(operation, TcpStream::connect(address)),
            )
        })
        .await
        .unwrap();
        let MemorySemanticSearchRead::Published {
            total_hits, hits, ..
        } = result
        else {
            panic!("semantic result");
        };
        assert_eq!(total_hits, 1);
        assert_eq!(hits[0].current.id, first.id);
        assert!(hits[0].distance.is_finite());
        task.await.unwrap().unwrap();
    }
    let manifest_path = model.directory.path().join("768.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["recipe"]["max_tokens"] = serde_json::json!(4096);
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let changed = model
        .config(&[("a", 768)], ctx.snapshot(), NonZeroU32::new(1).unwrap())
        .unwrap();
    let b = ExpectedModel::new(&changed, "a", "service").unwrap();
    assert_eq!(a.dimensions(), b.dimensions());
    assert_ne!(a.config_id(), b.config_id());
    assert_ne!(a.vector_id(), b.vector_id());
    service
        .install(
            changed,
            InferenceContext::new(&cancel, ctx.snapshot().required_deadline().unwrap()),
        )
        .unwrap();
    let consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: f.runtime(),
        expected: &b,
    };
    let consumer = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
    assert!(matches!(
        boxed_phase(|| consumer.query(
            &read,
            "Paris".into(),
            options(),
            auth::read_limits(),
            |operation| consumer_inference.infer(operation, TcpStream::connect(address))
        ))
        .await
        .unwrap(),
        MemorySemanticSearchRead::Unavailable {
            reason: MemoryEmbeddingUnavailable::IncompatibleIdentity,
            ..
        }
    ));
    task.await.unwrap().unwrap();
    NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("France"),
        auth::write_limits(),
    )
    .await
    .unwrap();
    let mut read_limits = auth::read_limits();
    read_limits.batch_rows = NonZeroU32::new(1).unwrap();
    let memory_path = f.project.runtime_root().join("memory");
    let saved_memory_path = f.project.runtime_root().join("memory-saved");
    let mut store_attempts = 0;
    let mut store_tasks = Vec::new();
    let store_failed = boxed_phase(|| {
        consumer.index(
            &read,
            &write,
            read_limits,
            auth::write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, {
                    store_attempts += 1;
                    if store_attempts == 2 {
                        std::fs::rename(&memory_path, &saved_memory_path).unwrap();
                        std::fs::create_dir(&memory_path).unwrap();
                    }
                    let (address, _, task) =
                        rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
                    store_tasks.push(task);
                    TcpStream::connect(address)
                })
            },
        )
    })
    .await
    .unwrap_err();
    std::fs::remove_dir(&memory_path).unwrap();
    std::fs::rename(&saved_memory_path, &memory_path).unwrap();
    for task in store_tasks {
        task.await.unwrap().unwrap();
    }
    assert!(matches!(store_failed.cause, MemoryEmbeddingCause::Store(_)));
    assert!(matches!(store_failed.last_attachment,
        Some(ref attachment) if matches!(**attachment, MemoryEmbeddingAttachOutcome::Published(_))));
    assert_eq!(store_attempts, 2);
    // The first Memory is already covered after the partial store failure.
    // Keep two real pending documents for the partial transport-failure case.
    NativeMemoryStore::create(ctx.snapshot(), &write, input("Lyon"), auth::write_limits())
        .await
        .unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
    let mut attempts = 0;
    let failed = boxed_phase(|| {
        consumer.index(
            &read,
            &write,
            read_limits,
            auth::write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, {
                    attempts += 1;
                    let attempt = attempts;
                    async move {
                        if attempt == 1 {
                            TcpStream::connect(address).await
                        } else {
                            Err(io::Error::new(
                                io::ErrorKind::ConnectionRefused,
                                "deliberate transport failure",
                            ))
                        }
                    }
                })
            },
        )
    })
    .await
    .unwrap_err();
    assert!(matches!(failed.cause, MemoryEmbeddingCause::Service(_)));
    assert!(failed.last_attachment.is_some());
    assert_eq!(attempts, 2);
    task.await.unwrap().unwrap();
    let mut tasks = Vec::new();
    boxed_phase(|| {
        consumer.index(
            &read,
            &write,
            auth::read_limits(),
            auth::write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, {
                    let (address, _, task) =
                        rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
                    tasks.push(task);
                    TcpStream::connect(address)
                })
            },
        )
    })
    .await
    .unwrap();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    let (address, _, task) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
    assert!(matches!(
        boxed_phase(|| consumer.query(
            &read,
            "Paris".into(),
            options(),
            auth::read_limits(),
            |operation| consumer_inference.infer(operation, TcpStream::connect(address))
        ))
        .await
        .unwrap(),
        MemorySemanticSearchRead::Published { total_hits: 3, .. }
    ));
    task.await.unwrap().unwrap();
    let short_cancel = Cancellation::default();
    let short = auth::request_until(&short_cancel, Instant::now() + Duration::from_secs(2));
    let short_key = f.runtime().check(short.snapshot()).unwrap();
    let short_caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(short_key.secret(short.snapshot()).unwrap()).unwrap(),
            &short,
        )
        .unwrap();
    let short_read = authority
        .admit_authenticated_read(&short_caller, &short, f.scope(), ProjectReadModule::Memory)
        .unwrap();
    let mut short_selection = issuer
        .capture_service(&short_caller, ServicePurpose::Embedding)
        .unwrap();
    let short_lease = short_selection.local().unwrap();
    let short_consumer_inference = auth::LocalInference {
        client: &client,
        lease: &short_lease,
        runtime: f.runtime(),
        expected: &b,
    };
    let short_consumer = MemoryEmbeddingModel::new(
        short.snapshot(),
        short_consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let error = boxed_phase(|| {
        short_consumer.query(
            &short_read,
            "Paris".into(),
            options(),
            auth::read_limits(),
            |operation| {
                short_consumer_inference.infer(operation, async {
                    Err::<TcpStream, _>(io::Error::other("must not connect after deadline"))
                })
            },
        )
    })
    .await
    .unwrap_err();
    assert!(
        matches!(error.cause, MemoryEmbeddingCause::Store(ref e) if matches!(**e, MemoryStoreError::Request(ref request) if request.kind() == io::ErrorKind::TimedOut))
    );
    auth::revoke_service(&store, &ctx);
    let denied_connect = std::sync::atomic::AtomicBool::new(false);
    let error = boxed_phase(|| {
        consumer.query(
            &read,
            "Paris".into(),
            options(),
            auth::read_limits(),
            |operation| {
                consumer_inference.infer(operation, async {
                    denied_connect.store(true, std::sync::atomic::Ordering::SeqCst);
                    Err::<TcpStream, _>(io::Error::other("must not connect after denial"))
                })
            },
        )
    })
    .await
    .unwrap_err();
    assert!(
        matches!(
            error.cause,
            MemoryEmbeddingCause::Service(ref e) if e.kind() == ModelErrorKind::Authentication
        ),
        "{error:?}"
    );
    assert!(!denied_connect.load(std::sync::atomic::Ordering::SeqCst));
    cancel.cancel();
    let error = boxed_phase(|| {
        consumer.index(
            &read,
            &write,
            auth::read_limits(),
            auth::write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, async {
                    Err::<TcpStream, _>(io::Error::other("must not connect after cancellation"))
                })
            },
        )
    })
    .await
    .unwrap_err();
    assert!(
        matches!(error.cause, MemoryEmbeddingCause::Store(ref e) if matches!(**e, MemoryStoreError::Request(ref request) if request.kind() == io::ErrorKind::Interrupted))
    );
    let shutdown = Cancellation::default();
    service
        .shutdown(InferenceContext::new(
            &shutdown,
            Instant::now() + Duration::from_secs(180),
        ))
        .await
        .unwrap();
    drop(sdk);
}

async fn partition_memories(
    ctx: &graphit_hub::policy::RequestContext<'_>,
) -> (auth::Fixture, graphit_auth::RoleIssuer, Vec<Memory>) {
    let fixture = auth::Fixture::new(ctx, "main");
    let (issuer, _) = fixture.issuer(ctx);
    let key = fixture.runtime().check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            ctx,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let write = authority
        .admit_authenticated_write(&caller, ctx, fixture.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    let mut memories = Vec::new();
    for (key, title) in [
        ("partition-one", "ação A"),
        ("partition-two", "ação B"),
        ("partition-three", "ação C"),
    ] {
        let mut item = input(key);
        item.title = title.into();
        memories.push(
            NativeMemoryStore::create(ctx.snapshot(), &write, item, auth::write_limits())
                .await
                .unwrap(),
        );
    }
    drop(write);
    drop(caller);
    drop(key);
    (fixture, issuer, memories)
}

#[tokio::test]
#[ignore = "requires the pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn actual_memory_rpc_partition_boundaries() {
    let (_snapshot_root, snapshot_parent) = model_fixture::snapshot_parent();
    let model_path = std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL").expect("pinned BF16 GGUF");
    let model = gemma::GemmaFixture::new(std::path::Path::new(&model_path));
    let cancel = Cancellation::default();
    let ctx = auth::request_until(&cancel, Instant::now() + Duration::from_secs(600));
    let f = auth::Fixture::new(&ctx, "main");
    let (issuer, _store) = f.issuer(&ctx);
    let key = f.runtime().check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let config = model
        .config(
            &[("a", 768), ("mrl", 128)],
            ctx.snapshot(),
            NonZeroU32::new(2).unwrap(),
        )
        .unwrap();
    let a = ExpectedModel::new(&config, "mrl", "service").unwrap();
    let sdk = NativeSdkRoot::new().unwrap();
    let service = ModelService::new(
        snapshot_parent.clone(),
        ServiceLimits {
            max_active_workers: NonZeroUsize::new(16).unwrap(),
            embedding: EmbeddingLimits::new(32, 65536, 768, 24576, 8, 131072, 131072).unwrap(),
            rerank: NativeRerankLimits::new(32, 8, 65536, 131072, 131072).unwrap(),
        },
    )
    .unwrap();
    service
        .install(
            config,
            InferenceContext::new(&cancel, ctx.snapshot().required_deadline().unwrap()),
        )
        .unwrap();
    let limits = RpcLimits::new(131072, 32, 65536, 24576).unwrap();
    let client = ModelServiceClient::new(limits).unwrap();
    let consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: f.runtime(),
        expected: &a,
    };
    let consumer = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();

    let mut ids = Vec::new();
    for (key, title) in [
        ("partition-one", "ação A"),
        ("partition-two", "ação B"),
        ("partition-three", "ação C"),
    ] {
        let mut item = input(key);
        item.title = title.into();
        ids.push(
            NativeMemoryStore::create(ctx.snapshot(), &write, item, auth::write_limits())
                .await
                .unwrap()
                .id,
        );
    }
    let MemoryEmbeddingCorpusRead::Published(mut corpus) = NativeMemoryStore::capture_embeddings(
        ctx.snapshot(),
        &read,
        consumer.identity().clone(),
        auth::read_limits(),
    )
    .await
    .unwrap() else {
        panic!("actual corpus");
    };
    let documents = corpus
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(documents.len(), 3);
    let text_bytes = documents[0].text().len();
    assert!(
        documents
            .iter()
            .all(|document| document.text().len() == text_bytes)
    );
    assert!(
        documents
            .iter()
            .all(|document| document.text().contains("ação"))
    );
    assert!(
        corpus
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    let metadata: Vec<_> = documents
        .iter()
        .map(|document| {
            (
                document.memory_id(),
                document.revision(),
                *document.fingerprint(),
            )
        })
        .collect();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
    let baseline = client
        .embed(
            &lease,
            f.runtime(),
            &a,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Document,
                texts: documents
                    .into_iter()
                    .map(MemoryEmbeddingDocument::into_text)
                    .collect(),
                deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    task.await.unwrap().unwrap();
    drop(corpus);

    let bounds = RpcLimits::new(131072, 2, 65536, 24576).unwrap();
    let bounded_client = ModelServiceClient::new(bounds).unwrap();
    let bounded_inference = auth::LocalInference {
        client: &bounded_client,
        lease: &lease,
        runtime: f.runtime(),
        expected: &a,
    };
    let bounded = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        bounded_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), bounds);
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_address = closed.local_addr().unwrap();
    drop(closed);
    let mut calls = 0;
    let failed = bounded
        .index(
            &read,
            &write,
            auth::read_limits(),
            auth::write_limits(),
            bounds,
            |operation| {
                bounded_inference.infer(operation, {
                    calls += 1;
                    let first = calls == 1;
                    async move {
                        if first {
                            TcpStream::connect(address).await
                        } else {
                            TcpStream::connect(closed_address).await
                        }
                    }
                })
            },
        )
        .await
        .unwrap_err();
    assert_eq!(calls, 2);
    assert!(
        matches!(failed.cause, MemoryEmbeddingCause::Service(ref error) if error.kind() == ModelErrorKind::Transport)
    );
    assert!(
        matches!(failed.last_attachment, Some(ref outcome) if matches!(**outcome, MemoryEmbeddingAttachOutcome::Published(_)))
    );
    task.await.unwrap().unwrap();

    // The real scan chunk contains three documents; each client admits exactly two.
    for bounds in [
        RpcLimits::new(131072, 2, 65536, 24576).unwrap(),
        RpcLimits::new(131072, 32, text_bytes * 2, 24576).unwrap(),
        RpcLimits::new(131072, 32, 65536, 128 * 2).unwrap(),
    ] {
        // Each ceiling must see pending documents, independent of preceding CAS effects.
        let (corpus_owner, corpus_issuer, memories) = partition_memories(&ctx).await;
        let corpus_key = corpus_owner.runtime().check(ctx.snapshot()).unwrap();
        let corpus_caller = corpus_issuer
            .authenticate_runtime(
                std::str::from_utf8(corpus_key.secret(ctx.snapshot()).unwrap()).unwrap(),
                &ctx,
            )
            .unwrap();
        let corpus_authority = ProjectRecordAuthority::new(&corpus_issuer);
        let read = corpus_authority
            .admit_authenticated_read(
                &corpus_caller,
                &ctx,
                corpus_owner.scope(),
                ProjectReadModule::Memory,
            )
            .unwrap();
        let write = corpus_authority
            .admit_authenticated_write(
                &corpus_caller,
                &ctx,
                corpus_owner.scope(),
                ProjectWriteOperation::Memory,
            )
            .unwrap();
        let ids = memories.iter().map(|memory| memory.id).collect::<Vec<_>>();
        let bounded_client = ModelServiceClient::new(bounds).unwrap();
        let bounded_inference = auth::LocalInference {
            client: &bounded_client,
            lease: &lease,
            runtime: f.runtime(),
            expected: &a,
        };
        let bounded = MemoryEmbeddingModel::new(
            ctx.snapshot(),
            bounded_inference.identity(),
            std::time::Duration::from_secs(120),
        )
        .unwrap();
        let mut tasks = Vec::new();
        let indexed = bounded
            .index(
                &read,
                &write,
                auth::read_limits(),
                auth::write_limits(),
                bounds,
                |operation| {
                    bounded_inference.infer(operation, {
                        let (address, _, task) =
                            rpc::server(service.clone(), secret(&f, ctx.snapshot()), bounds);
                        tasks.push(task);
                        TcpStream::connect(address)
                    })
                },
            )
            .await
            .unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(
            matches!(indexed, MemoryEmbeddingIndexRead::Complete { last_attachment: Some(ref outcome), .. }
            if matches!(**outcome, MemoryEmbeddingAttachOutcome::Published(_) | MemoryEmbeddingAttachOutcome::Existing { .. }))
        );
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        let (address, _, task) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), bounds);
        let mut all = options();
        all.top_k = NonZeroUsize::new(3).unwrap();
        let MemorySemanticSearchRead::Published {
            total_hits, hits, ..
        } = bounded
            .query(
                &read,
                "capital of France".into(),
                all,
                auth::read_limits(),
                |operation| bounded_inference.infer(operation, TcpStream::connect(address)),
            )
            .await
            .unwrap()
        else {
            panic!("complete actual semantic coverage");
        };
        assert_eq!(total_hits, 3);
        let mut actual: Vec<_> = hits.iter().map(|hit| hit.current.id).collect();
        actual.sort();
        let mut expected = ids.clone();
        expected.sort();
        assert_eq!(actual, expected);
        assert!(
            hits.iter()
                .all(|hit| hit.current.revision == 1 && hit.distance.is_finite())
        );
        task.await.unwrap().unwrap();
        for memory in &memories {
            let document = prepare_memory_embedding_document(memory, bounded.identity()).unwrap();
            let index = metadata
                .iter()
                .position(|(_, _, fingerprint)| fingerprint == document.fingerprint())
                .expect("same literal embedding document");
            let vector = &baseline.vectors[index];
            let MemorySemanticSearchRead::Published { hits, .. } =
                NativeMemoryStore::search_semantic(
                    ctx.snapshot(),
                    &read,
                    bounded.identity().clone(),
                    vector,
                    options(),
                    auth::read_limits(),
                )
                .await
                .unwrap()
            else {
                panic!("actual vector identity");
            };
            assert_eq!(hits[0].current.id, memory.id);
            assert_eq!(hits[0].current.revision, memory.revision);
            assert!(hits[0].distance.abs() < 1e-5);
        }
    }
    for bounds in [
        RpcLimits::new(131072, 32, text_bytes - 1, 24576).unwrap(),
        RpcLimits::new(131072, 32, 65536, 127).unwrap(),
    ] {
        let (corpus_owner, corpus_issuer, _) = partition_memories(&ctx).await;
        let corpus_key = corpus_owner.runtime().check(ctx.snapshot()).unwrap();
        let corpus_caller = corpus_issuer
            .authenticate_runtime(
                std::str::from_utf8(corpus_key.secret(ctx.snapshot()).unwrap()).unwrap(),
                &ctx,
            )
            .unwrap();
        let corpus_authority = ProjectRecordAuthority::new(&corpus_issuer);
        let read = corpus_authority
            .admit_authenticated_read(
                &corpus_caller,
                &ctx,
                corpus_owner.scope(),
                ProjectReadModule::Memory,
            )
            .unwrap();
        let write = corpus_authority
            .admit_authenticated_write(
                &corpus_caller,
                &ctx,
                corpus_owner.scope(),
                ProjectWriteOperation::Memory,
            )
            .unwrap();
        let bounded_client = ModelServiceClient::new(bounds).unwrap();
        let bounded_inference = auth::LocalInference {
            client: &bounded_client,
            lease: &lease,
            runtime: f.runtime(),
            expected: &a,
        };
        let bounded = MemoryEmbeddingModel::new(
            ctx.snapshot(),
            bounded_inference.identity(),
            std::time::Duration::from_secs(120),
        )
        .unwrap();
        let mut calls = 0;
        let failed = bounded
            .index(
                &read,
                &write,
                auth::read_limits(),
                auth::write_limits(),
                bounds,
                |operation| {
                    bounded_inference.infer(operation, {
                        calls += 1;
                        async {
                            Err::<TcpStream, _>(io::Error::other(
                                "indivisible document must not connect",
                            ))
                        }
                    })
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(failed.cause, MemoryEmbeddingCause::Store(ref error) if matches!(**error, MemoryStoreError::Limit("Memory embedding batch capacity")))
        );
        assert!(failed.last_attachment.is_none());
        assert_eq!(calls, 0);
    }
    let (corpus_owner, corpus_issuer, memories) = partition_memories(&ctx).await;
    let corpus_key = corpus_owner.runtime().check(ctx.snapshot()).unwrap();
    let corpus_caller = corpus_issuer
        .authenticate_runtime(
            std::str::from_utf8(corpus_key.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let corpus_authority = ProjectRecordAuthority::new(&corpus_issuer);
    let read = corpus_authority
        .admit_authenticated_read(
            &corpus_caller,
            &ctx,
            corpus_owner.scope(),
            ProjectReadModule::Memory,
        )
        .unwrap();
    let write = corpus_authority
        .admit_authenticated_write(
            &corpus_caller,
            &ctx,
            corpus_owner.scope(),
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    let ids = memories.iter().map(|memory| memory.id).collect::<Vec<_>>();
    let mut large = input("partition-oversized-after-prefix");
    large.body = "z".repeat(text_bytes * 3);
    let large = NativeMemoryStore::create(ctx.snapshot(), &write, large, auth::write_limits())
        .await
        .unwrap();
    assert!(ids.iter().all(|id| *id < large.id));
    let bounds = RpcLimits::new(131072, 32, text_bytes * 3, 24576).unwrap();
    let bounded_client = ModelServiceClient::new(bounds).unwrap();
    let bounded_inference = auth::LocalInference {
        client: &bounded_client,
        lease: &lease,
        runtime: f.runtime(),
        expected: &a,
    };
    let bounded = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        bounded_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let mut tasks = Vec::new();
    let failed = bounded
        .index(
            &read,
            &write,
            auth::read_limits(),
            auth::write_limits(),
            bounds,
            |operation| {
                bounded_inference.infer(operation, {
                    let (address, _, task) =
                        rpc::server(service.clone(), secret(&f, ctx.snapshot()), bounds);
                    tasks.push(task);
                    TcpStream::connect(address)
                })
            },
        )
        .await
        .unwrap_err();
    assert_eq!(tasks.len(), 1);
    assert!(
        matches!(failed.cause, MemoryEmbeddingCause::Store(ref error) if matches!(**error, MemoryStoreError::Limit("Memory embedding batch capacity")))
    );
    assert!(
        matches!(failed.last_attachment, Some(ref outcome) if matches!(**outcome,
        MemoryEmbeddingAttachOutcome::Published(_) | MemoryEmbeddingAttachOutcome::Existing { .. }))
    );
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    let shutdown = Cancellation::default();
    service
        .shutdown(InferenceContext::new(
            &shutdown,
            Instant::now() + Duration::from_secs(180),
        ))
        .await
        .unwrap();
    drop(sdk);
}

#[tokio::test]
#[ignore = "requires the pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn actual_gguf_memory_hybrid_rpc_uses_borrowed_owners_and_queries_fully_covered_corpus() {
    let (_snapshot_root, snapshot_parent) = model_fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let setup = auth::request_until(&cancel, Instant::now() + Duration::from_secs(120));
    let f = auth::Fixture::new(&setup, "main");
    let (issuer, store) = f.issuer(&setup);
    let ctx =
        graphit_hub::policy::RequestContext::service(setup.content_limits(), &cancel).unwrap();
    let original = Arc::clone(ctx.snapshot());
    assert_eq!(original.deadline(), None);
    let key = f.runtime().check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(&issuer);
    let read = authority
        .admit_authenticated_read(&caller, &ctx, f.scope(), ProjectReadModule::Memory)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &ctx, f.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let mut fixture = model_fixture::Fixture::actual(model_api::TaskKind::Embedding);
    fixture.body["recipe"]["dimensions"] = serde_json::json!(128);
    let config = fixture.config(&["tiny"]);
    let expected = ExpectedModel::new(&config, "tiny", "service").unwrap();
    let sdk = NativeSdkRoot::new().unwrap();
    let service = model_fixture::service(snapshot_parent.clone());
    service
        .install(config, model_fixture::context(&cancel))
        .unwrap();
    let mut limits = model_fixture::rpc_limits();
    limits.max_texts = 1;
    let client = ModelServiceClient::new(limits).unwrap();
    let rpc_timeout = Duration::from_secs(5);
    let consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: f.runtime(),
        expected: &expected,
    };
    let consumer =
        MemoryEmbeddingModel::new(ctx.snapshot(), consumer_inference.identity(), rpc_timeout)
            .unwrap();
    let opts = || MemoryHybridSearchOptions {
        filter: MemoryListFilter {
            mandatory: Some(false),
            ..MemoryListFilter::default()
        },
        include_deleted_history: false,
        top_k: NonZeroUsize::new(2).unwrap(),
    };
    let polled = std::sync::atomic::AtomicBool::new(false);
    let absent = consumer
        .query_hybrid(
            &read,
            "Paris".into(),
            opts(),
            auth::read_limits(),
            |operation| {
                consumer_inference.infer(operation, async {
                    polled.store(true, std::sync::atomic::Ordering::SeqCst);
                    Err::<TcpStream, _>(io::Error::other("absent must not connect"))
                })
            },
        )
        .await
        .unwrap();
    assert_eq!(absent, MemoryHybridSearchRead::NoCorpus);
    assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
    let mut tiny_row = auth::write_limits();
    tiny_row.max_row_bytes = NonZeroUsize::MIN;
    assert!(matches!(
        NativeMemoryStore::create(ctx.snapshot(), &write, input("empty-birth"), tiny_row).await,
        Err(MemoryStoreError::Limit(_))
    ));
    let empty = consumer
        .query_hybrid(
            &read,
            "Paris".into(),
            opts(),
            auth::read_limits(),
            |operation| {
                consumer_inference.infer(operation, async {
                    polled.store(true, std::sync::atomic::Ordering::SeqCst);
                    Err::<TcpStream, _>(io::Error::other("empty must not connect"))
                })
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        empty,
        MemoryHybridSearchRead::PublishedEmpty { .. }
    ));
    assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
    let memory = NativeMemoryStore::create(
        ctx.snapshot(),
        &write,
        input("tiny-paris"),
        auth::write_limits(),
    )
    .await
    .unwrap();
    let mut second_input = input("tiny-second");
    second_input.attributes.mandatory = true;
    let second =
        NativeMemoryStore::create(ctx.snapshot(), &write, second_input, auth::write_limits())
            .await
            .unwrap();
    // This case tests fresh five-second Memory operations, independently of cold SDK loading.
    let (address, _, prepared) = rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
    let warmed = client
        .embed(
            &lease,
            f.runtime(),
            &expected,
            EmbedOperation {
                input: EmbedInput::Document,
                texts: vec![
                    prepare_memory_embedding_document(&memory, consumer.identity())
                        .unwrap()
                        .into_text(),
                ],
                deadline: Instant::now() + Duration::from_secs(30),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    assert_eq!(warmed.dimensions, 128);
    assert!(warmed.vectors[0].iter().all(|value| value.is_finite()));
    prepared.await.unwrap().unwrap();
    let mut first_rpc_started = None;
    let mut rpc_owners = Vec::new();
    let indexed = consumer
        .index(
            &read,
            &write,
            auth::read_limits(),
            auth::write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, {
                    first_rpc_started.get_or_insert_with(Instant::now);
                    let (address, connection, task) =
                        rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
                    rpc_owners.push((connection, task));
                    TcpStream::connect(address)
                })
            },
        )
        .await;
    let rpc_count = rpc_owners.len();
    let mut joined = Vec::new();
    for (connection, task) in rpc_owners {
        connection.cancel();
        joined.push(task.await);
    }
    if indexed.is_err() || rpc_count != 2 || joined.iter().any(|join| !matches!(join, Ok(Ok(())))) {
        let cleanup = Cancellation::default();
        service
            .shutdown(model_fixture::context(&cleanup))
            .await
            .unwrap();
    }
    indexed.unwrap();
    assert_eq!(rpc_count, 2, "one real RPC for each pending document");
    assert!(
        joined.iter().all(|join| matches!(join, Ok(Ok(())))),
        "{joined:?}"
    );
    // Expiration of the first operation does not expire this service Original.
    tokio::time::sleep_until(
        (first_rpc_started.unwrap() + rpc_timeout + Duration::from_millis(10)).into(),
    )
    .await;
    original.check().unwrap();
    assert!(Arc::ptr_eq(&original, lease.original().unwrap().snapshot()));
    let MemoryEmbeddingCorpusRead::Published(mut covered) = NativeMemoryStore::capture_embeddings(
        ctx.snapshot(),
        &read,
        consumer.identity().clone(),
        auth::read_limits(),
    )
    .await
    .unwrap() else {
        panic!("covered corpus")
    };
    assert!(!covered.is_empty());
    assert!(
        covered
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    drop(covered);
    let (address, connection, task) =
        rpc::server(service.clone(), secret(&f, ctx.snapshot()), limits);
    let MemoryHybridSearchRead::Published {
        total_hits, hits, ..
    } = consumer
        .query_hybrid(
            &read,
            "Paris".into(),
            opts(),
            auth::read_limits(),
            |operation| consumer_inference.infer(operation, TcpStream::connect(address)),
        )
        .await
        .unwrap()
    else {
        panic!("real hybrid result")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current, memory);
    assert_ne!(hits[0].current.id, second.id);
    assert!(hits[0].lexical.is_some() && hits[0].semantic.is_some());
    assert_eq!(hits[0].score, 2.0 / 61.0);
    connection.cancel();
    task.await.unwrap().unwrap();
    assert!(Arc::ptr_eq(&original, lease.original().unwrap().snapshot()));
    assert_eq!(original.deadline(), None);
    auth::revoke_service(&store, &setup);
    let denied_connect = std::sync::atomic::AtomicBool::new(false);
    let denied = consumer
        .query(
            &read,
            "Paris".into(),
            options(),
            auth::read_limits(),
            |operation| {
                consumer_inference.infer(operation, async {
                    denied_connect.store(true, std::sync::atomic::Ordering::SeqCst);
                    Err::<TcpStream, _>(io::Error::other("revoked lease must not connect"))
                })
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(denied.cause, MemoryEmbeddingCause::Service(ref error)
        if error.kind() == ModelErrorKind::Authentication)
    );
    assert!(!denied_connect.load(std::sync::atomic::Ordering::SeqCst));
    cancel.cancel();
    let cancelled = consumer
        .query(
            &read,
            "Paris".into(),
            options(),
            auth::read_limits(),
            |operation| {
                consumer_inference.infer(operation, async {
                    denied_connect.store(true, std::sync::atomic::Ordering::SeqCst);
                    Err::<TcpStream, _>(io::Error::other("cancelled Original must not connect"))
                })
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(cancelled.cause, MemoryEmbeddingCause::Store(ref error)
        if matches!(**error, MemoryStoreError::Request(ref cause) if cause.kind() == io::ErrorKind::Interrupted))
    );
    assert!(!denied_connect.load(std::sync::atomic::Ordering::SeqCst));
    let cleanup = Cancellation::default();
    service
        .shutdown(model_fixture::context(&cleanup))
        .await
        .unwrap();
    drop(sdk);
}

#[tokio::test]
async fn cancelled_unconnected_model_rpc_server_joins() {
    let (_snapshot_root, snapshot_parent) = model_fixture::snapshot_parent();
    let service = model_fixture::service(snapshot_parent.clone());
    let secret = Arc::new(RuntimeSecret::new(Zeroizing::new([b'a'; 64])).unwrap());
    let (_, cancellation, server) =
        rpc::server(service.clone(), secret, model_fixture::rpc_limits());
    cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let cleanup = Cancellation::default();
    service
        .shutdown(model_fixture::context(&cleanup))
        .await
        .unwrap();
}
