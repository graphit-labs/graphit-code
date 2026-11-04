#![cfg(target_os = "linux")]
#[allow(dead_code)]
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
#[allow(dead_code)]
#[path = "../../graphit-records/tests/user_memory_store/fixture.rs"]
mod user_fixture;

use graphit_ai::{
    context::InferenceContext, embedding::EmbeddingLimits, native_embedding::NativeSdkRoot,
    native_rerank::NativeRerankLimits,
};
use graphit_auth::{
    Permission, ServicePurpose, UserMemoryWriteOperation, broker::BrokerRequestContext,
};
use graphit_core::{artifact_store::StoreOwner, file_lock::Cancellation};
use graphit_daemon::model_child as child_api;
use graphit_daemon::model_service as model_api;
use graphit_daemon::model_transport as disconnect_api;
use graphit_records::{memory_store as memory_api, memory_store::*, user_memory_store::*};
use model_api::*;
use std::{
    io,
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::net::TcpStream;
use user_fixture::{Fixture, RESOURCE, SUBJECT, input, now_seconds, read_limits, write_limits};
use zeroize::Zeroizing;

macro_rules! incoming {
    ($f:ident,$ctx:ident,$verifier:ident,$proof:ident,$read:ident,$write:ident) => {
        let $verifier = $f.server.verifier(true, $f.provider_revision(&$ctx));
        let raw = $f.server.token(SUBJECT, RESOURCE, now_seconds() + 900);
        let $proof = $verifier
            .verify(&raw, BrokerRequestContext::new($ctx.snapshot()))
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
                UserMemoryWriteOperation::Memory,
            )
            .unwrap();
    };
}

fn options() -> MemorySemanticSearchOptions {
    MemorySemanticSearchOptions {
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(8).unwrap(),
    }
}
fn secret(
    f: &auth::Fixture,
    original: &Arc<graphit_core::file_io::SnapshotContext>,
) -> Arc<RuntimeSecret> {
    let key = f.runtime().check(original).unwrap();
    Arc::new(RuntimeSecret::new(Zeroizing::new(*key.secret(original).unwrap())).unwrap())
}
fn model() -> gemma::GemmaFixture {
    let path =
        std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL").expect("existing pinned BF16 GGUF");
    gemma::GemmaFixture::new(std::path::Path::new(&path))
}
fn publication(result: UserMemoryEmbeddingIndexRead) -> UserMemoryEmbeddingPublication {
    let UserMemoryEmbeddingIndexRead::Complete {
        last_attachment: Some(attachment),
        ..
    } = result
    else {
        panic!("actual USER attachment");
    };
    let UserMemoryEmbeddingAttachOutcome::Published(p) = *attachment else {
        panic!("actual new USER publication");
    };
    p
}

async fn pin(f: &Fixture) -> graphit_data::lance_branch_store::BranchSnapshot {
    let uri = url::Url::from_file_path(f.user.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    graphit_data::lance_branch_store::BranchSnapshot::capture_observed(&uri, None, None)
        .await
        .unwrap()
}

fn incompatible_dimension<'a>(
    consumer: &'a MemoryEmbeddingModel<'_>,
    consumer_inference: &'a auth::LocalInference<'_, '_>,
    read: &'a graphit_auth::UserReadAdmission<'_>,
    user: &'a Fixture,
    service: ModelService,
    secret: Arc<RuntimeSecret>,
    limits: RpcLimits,
) -> std::pin::Pin<Box<impl std::future::Future<Output = ()> + 'a>> {
    // Keep this independent RPC phase out of the long journey's inline future.
    Box::pin(async move {
        let before = pin(user).await.version();
        let (address, _, task) = rpc::server(service, secret, limits);
        let failure = consumer
            .query_user(
                read,
                "Paris".into(),
                options(),
                read_limits(),
                |operation| consumer_inference.infer(operation, TcpStream::connect(address)),
            )
            .await
            .unwrap_err();
        task.await.unwrap().unwrap();
        assert!(
            matches!(failure.cause, MemoryEmbeddingCause::Service(ref e) if e.kind() == ModelErrorKind::ConfigChanged)
        );
        assert!(failure.last_attachment.is_none());
        assert_eq!(pin(user).await.version(), before);
    })
}

#[tokio::test]
#[ignore = "requires the existing pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn actual_user_gemma_rpc_index_query_identity_and_partial_effects() {
    let (_snapshot_root, snapshot_parent) = model_fixture::snapshot_parent();
    let model = model();
    let cancel = Cancellation::default();
    let ctx = auth::request_until(&cancel, Instant::now() + Duration::from_secs(900));
    let model_owner = auth::Fixture::new(&ctx, "main");
    let (model_issuer, _) = model_owner.issuer(&ctx);
    let key = model_owner.runtime().check(ctx.snapshot()).unwrap();
    let model_caller = model_issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selected = model_issuer
        .capture_service(&model_caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selected.local().unwrap();
    let config = model
        .config(&[("a", 128)], ctx.snapshot(), NonZeroU32::new(2).unwrap())
        .unwrap();
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let sdk_root = NativeSdkRoot::new().unwrap();
    let service = ModelService::new(
        snapshot_parent.clone(),
        ServiceLimits {
            max_active_workers: NonZeroUsize::new(2).unwrap(),
            embedding: EmbeddingLimits::new(32, 65536, 768, 24576, 8, 131072, 131072).unwrap(),
            rerank: NativeRerankLimits::new(32, 8, 65536, 131072, 131072).unwrap(),
        },
    )
    .unwrap();
    service
        .install(
            config,
            InferenceContext::new(&cancel, ctx.deadline().expect("finite fixture deadline")),
        )
        .unwrap();
    let limits = RpcLimits::new(131072, 32, 65536, 24576).unwrap();
    let client = ModelServiceClient::new(limits).unwrap();
    let consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: model_owner.runtime(),
        expected: &expected,
    };
    let consumer = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let user = Fixture::new(&ctx).await;
    incoming!(user, ctx, verifier, proof, read, write);
    assert!(
        model_issuer
            .admit_authenticated_user_memory_read(&model_caller, &ctx, user.scope())
            .is_err()
    );
    let mut calls = 0;
    assert!(matches!(
        consumer
            .index_user(
                &read,
                &write,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("absent must not connect")) }
                })
            )
            .await
            .unwrap(),
        UserMemoryEmbeddingIndexRead::NoCorpus
    ));
    assert_eq!(calls, 0);
    assert!(matches!(
        consumer
            .query_user(
                &read,
                "Paris".into(),
                options(),
                read_limits(),
                |operation| consumer_inference.infer(operation, async {
                    Err::<TcpStream, _>(io::Error::other("absent query must not connect"))
                })
            )
            .await
            .unwrap(),
        UserMemorySemanticSearchRead::NoCorpus
    ));
    assert!(!user.user.runtime_root().exists());
    // The existing model consumer fixture derives an empty schema from the real
    // codec and actual model output. The probe is sliced away before SDK creation.
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
    let document = prepare_memory_embedding_document(&schema_memory, consumer.identity()).unwrap();
    let (address, _, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    let output = client
        .embed(
            &lease,
            model_owner.runtime(),
            &expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Document,
                texts: vec![document.text().to_owned()],
                deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    task.await.unwrap().unwrap();
    let StoreOwner::User {
        installation_id,
        identity,
    } = user.user.owner()
    else {
        panic!("USER");
    };
    let owner = UserMemoryOwner {
        installation_id: installation_id.clone(),
        identity: identity.clone(),
    };
    let empty =
        encode_user_memory_embedding_row(&owner, &document, &output.vectors[0], write_limits())
            .unwrap()
            .slice(0, 0);
    let l = write_limits();
    let artifacts =
        graphit_core::artifact_store::ArtifactStore::new(user.user.clone(), l.artifacts);
    let original = Arc::clone(ctx.snapshot());
    let mut writer = tokio::task::spawn_blocking(move || artifacts.writer_with_context(&original))
        .await
        .unwrap()
        .unwrap();
    writer.create_user_memory_directory(l.read.native).unwrap();
    let uri = url::Url::from_file_path(user.user.runtime_root().join("memory/index.lance"))
        .unwrap()
        .to_string();
    graphit_data::lance_module_store::LanceModuleStore::create(&uri, empty.schema(), vec![empty])
        .await
        .unwrap();
    drop(writer);
    let empty_version = pin(&user).await.version();
    assert!(matches!(
        consumer
            .index_user(
                &read,
                &write,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("empty must not connect")) }
                })
            )
            .await
            .unwrap(),
        UserMemoryEmbeddingIndexRead::Complete {
            last_attachment: None,
            ..
        }
    ));
    assert_eq!(calls, 0);
    assert!(matches!(
        consumer
            .query_user(
                &read,
                "Paris".into(),
                options(),
                read_limits(),
                |operation| consumer_inference.infer(operation, async {
                    Err::<TcpStream, _>(io::Error::other("empty query must not connect"))
                })
            )
            .await
            .unwrap(),
        UserMemorySemanticSearchRead::PublishedEmpty { .. }
    ));
    assert_eq!(pin(&user).await.version(), empty_version);
    let mut first_input = input("Paris");
    first_input.body = "Paris is the capital of France.".into();
    let first = NativeUserMemoryStore::create(ctx.snapshot(), &write, first_input, write_limits())
        .await
        .unwrap();
    let mut tasks = Vec::new();
    let indexed = consumer
        .index_user(
            &read,
            &write,
            read_limits(),
            write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, {
                    let (address, _, task) = rpc::server(
                        service.clone(),
                        secret(&model_owner, ctx.snapshot()),
                        limits,
                    );
                    tasks.push(task);
                    TcpStream::connect(address)
                })
            },
        )
        .await
        .unwrap();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    let first_publication = publication(indexed);
    assert_eq!(first_publication.generation.physical_branch, None);
    assert_eq!(first_publication.generation.inherited_target, None);
    assert_eq!(
        first_publication.generation.owner.identity.issuer(),
        user.server.origin
    );
    assert_eq!(
        first_publication.generation.owner.identity.subject(),
        SUBJECT
    );
    let StoreOwner::User {
        installation_id, ..
    } = user.user.owner()
    else {
        panic!("USER");
    };
    assert_eq!(
        &first_publication.generation.owner.installation_id,
        installation_id
    );
    let (address, _, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    let UserMemorySemanticSearchRead::Published {
        total_hits, hits, ..
    } = consumer
        .query_user(
            &read,
            "capital of France".into(),
            options(),
            read_limits(),
            |operation| consumer_inference.infer(operation, TcpStream::connect(address)),
        )
        .await
        .unwrap()
    else {
        panic!("actual USER semantic result");
    };
    task.await.unwrap().unwrap();
    assert_eq!(total_hits, 1);
    assert_eq!(hits[0].current.id, first.id);
    assert_eq!(hits[0].matched_revision.get(), 1);
    assert!(hits[0].distance.is_finite());
    let detail = NativeUserMemoryStore::get(ctx.snapshot(), &read, first.id, read_limits())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.memory, first);

    // An incompatible dimension changes the real ConfigId. Reject that expectation
    // against the running 128-dimensional service; no second model is installed.
    let wrong_config = model
        .config(&[("a", 768)], ctx.snapshot(), NonZeroU32::new(2).unwrap())
        .unwrap();
    let wrong_expected = ExpectedModel::new(&wrong_config, "a", "service").unwrap();
    assert_ne!(wrong_expected.dimensions(), expected.dimensions());
    let wrong_consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: model_owner.runtime(),
        expected: &wrong_expected,
    };
    let wrong_consumer = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        wrong_consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    incompatible_dimension(
        &wrong_consumer,
        &wrong_consumer_inference,
        &read,
        &user,
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    )
    .await;

    let manifest_path = model.directory.path().join("128.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["recipe"]["max_tokens"] = serde_json::json!(4096);
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let changed = model
        .config(&[("a", 128)], ctx.snapshot(), NonZeroU32::new(1).unwrap())
        .unwrap();
    let different = ExpectedModel::new(&changed, "a", "service").unwrap();
    assert_eq!(different.dimensions(), expected.dimensions());
    assert_ne!(different.config_id(), expected.config_id());
    assert_ne!(different.vector_id(), expected.vector_id());
    service
        .install(
            changed,
            InferenceContext::new(&cancel, ctx.deadline().expect("finite fixture deadline")),
        )
        .unwrap();
    let changed_consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: model_owner.runtime(),
        expected: &different,
    };
    let changed_consumer = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        changed_consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let (address, _, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    assert!(matches!(
        changed_consumer
            .query_user(
                &read,
                "Paris".into(),
                options(),
                read_limits(),
                |operation| changed_consumer_inference
                    .infer(operation, TcpStream::connect(address))
            )
            .await
            .unwrap(),
        UserMemorySemanticSearchRead::Unavailable {
            reason: MemoryEmbeddingUnavailable::IncompatibleIdentity,
            ..
        }
    ));
    task.await.unwrap().unwrap();
    NativeUserMemoryStore::update(
        ctx.snapshot(),
        &write,
        first.id,
        user_fixture::update("archive", 1),
        write_limits(),
    )
    .await
    .unwrap();
    let second =
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("France"), write_limits())
            .await
            .unwrap();
    let mut bounded = read_limits();
    bounded.batch_rows = NonZeroU32::new(1).unwrap();
    let memory_path = user.user.runtime_root().join("memory");
    let saved_path = user.user.runtime_root().join("memory-saved");
    let mut attempts = 0;
    let mut tasks = Vec::new();
    let store_failure = changed_consumer
        .index_user(
            &read,
            &write,
            bounded,
            write_limits(),
            changed_consumer_inference.limits(),
            |operation| {
                changed_consumer_inference.infer(operation, {
                    attempts += 1;
                    if attempts == 2 {
                        std::fs::rename(&memory_path, &saved_path).unwrap();
                        std::fs::create_dir(&memory_path).unwrap();
                    }
                    let (address, _, task) = rpc::server(
                        service.clone(),
                        secret(&model_owner, ctx.snapshot()),
                        limits,
                    );
                    tasks.push(task);
                    TcpStream::connect(address)
                })
            },
        )
        .await
        .unwrap_err();
    std::fs::remove_dir(&memory_path).unwrap();
    std::fs::rename(&saved_path, &memory_path).unwrap();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    assert_eq!(attempts, 2);
    assert!(matches!(
        store_failure.cause,
        MemoryEmbeddingCause::Store(_)
    ));
    let Some(previous) = store_failure.last_attachment else {
        panic!("retain prior effect");
    };
    let UserMemoryEmbeddingAttachOutcome::Published(previous) = *previous else {
        panic!("prior SDK publication");
    };
    let fresh_ctx = auth::request_until(&cancel, ctx.deadline().expect("finite fixture deadline"));
    incoming!(
        user,
        fresh_ctx,
        fresh_verifier,
        fresh_proof,
        _fresh_read,
        fresh_write
    );
    let version = pin(&user).await.version();
    assert!(matches!(
        NativeUserMemoryStore::resolve_embeddings(
            fresh_ctx.snapshot(),
            &fresh_write,
            &previous.recovery_key,
            write_limits()
        )
        .await
        .unwrap(),
        graphit_data::lance_branch_store::DeltaResolution::Applied { .. }
    ));
    assert_eq!(pin(&user).await.version(), version);

    let (address, _, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    let mut attempts = 0;
    let transport_failure = changed_consumer
        .index_user(
            &read,
            &write,
            bounded,
            write_limits(),
            changed_consumer_inference.limits(),
            |operation| {
                changed_consumer_inference.infer(operation, {
                    attempts += 1;
                    let n = attempts;
                    async move {
                        if n == 1 {
                            TcpStream::connect(address).await
                        } else {
                            Err(io::Error::new(
                                io::ErrorKind::ConnectionRefused,
                                "actual transport failure",
                            ))
                        }
                    }
                })
            },
        )
        .await
        .unwrap_err();
    task.await.unwrap().unwrap();
    assert_eq!(attempts, 2);
    assert!(matches!(
        transport_failure.cause,
        MemoryEmbeddingCause::Service(_)
    ));
    assert!(transport_failure.last_attachment.is_some());
    let mut tasks = Vec::new();
    changed_consumer
        .index_user(
            &read,
            &write,
            read_limits(),
            write_limits(),
            changed_consumer_inference.limits(),
            |operation| {
                changed_consumer_inference.infer(operation, {
                    let (address, _, task) = rpc::server(
                        service.clone(),
                        secret(&model_owner, ctx.snapshot()),
                        limits,
                    );
                    tasks.push(task);
                    TcpStream::connect(address)
                })
            },
        )
        .await
        .unwrap();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    let (address, _, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    let UserMemorySemanticSearchRead::Published {
        total_hits, hits, ..
    } = changed_consumer
        .query_user(
            &read,
            "Paris".into(),
            options(),
            read_limits(),
            |operation| changed_consumer_inference.infer(operation, TcpStream::connect(address)),
        )
        .await
        .unwrap()
    else {
        panic!("current USER coverage");
    };
    task.await.unwrap().unwrap();
    assert_eq!(total_hits, 2);
    assert!(hits.iter().any(|hit| hit.current.id == first.id));
    assert!(hits.iter().any(|hit| hit.current.id == second.id));
    assert!(hits.iter().all(|hit| hit.distance.is_finite()));
    let mut filter = options();
    filter.filter.mandatory = Some(false);
    let (address, _, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    assert!(matches!(
        changed_consumer
            .query_user(&read, "Paris".into(), filter, read_limits(), |operation| {
                changed_consumer_inference.infer(operation, TcpStream::connect(address))
            })
            .await
            .unwrap(),
        UserMemorySemanticSearchRead::Published { total_hits: 0, .. }
    ));
    task.await.unwrap().unwrap();
    let deleted = NativeUserMemoryStore::delete(
        ctx.snapshot(),
        &write,
        second.id,
        DeleteMemory {
            expected_revision: second.revision,
            idempotency_key: "consumer-delete".into(),
            reason: "explicit deleted history".into(),
        },
        write_limits(),
    )
    .await
    .unwrap();
    for include_deleted_history in [false, true] {
        let mut query = options();
        query.include_deleted_history = include_deleted_history;
        let (address, _, task) = rpc::server(
            service.clone(),
            secret(&model_owner, ctx.snapshot()),
            limits,
        );
        let UserMemorySemanticSearchRead::Published {
            total_hits, hits, ..
        } = changed_consumer
            .query_user(&read, "Paris".into(), query, read_limits(), |operation| {
                changed_consumer_inference.infer(operation, TcpStream::connect(address))
            })
            .await
            .unwrap()
        else {
            panic!("explicit deleted-history selection");
        };
        task.await.unwrap().unwrap();
        assert_eq!(total_hits, if include_deleted_history { 2 } else { 1 });
        assert!(hits.iter().any(|hit| hit.current.id == first.id));
        if include_deleted_history {
            assert!(
                hits.iter()
                    .any(|hit| hit.current == deleted
                        && hit.matched_revision.get() == second.revision)
            );
        } else {
            assert!(hits.iter().all(|hit| hit.current.id != second.id));
        }
    }
    assert_eq!(user.server.count("/userinfo"), 0);
    assert_eq!(user.server.count("/token"), 0);
    service
        .shutdown(InferenceContext::new(
            &cancel,
            ctx.deadline().expect("finite fixture deadline"),
        ))
        .await
        .unwrap();
    drop(sdk_root);
}

#[tokio::test]
#[ignore = "requires the existing pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn actual_user_admissions_original_and_live_guards_prevent_rpc() {
    let model = model();
    let cancel = Cancellation::default();
    let ctx = auth::request_until(&cancel, Instant::now() + Duration::from_secs(120));
    let f = auth::Fixture::new(&ctx, "main");
    let (issuer, store) = f.issuer(&ctx);
    let key = f.runtime().check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let config = model
        .config(&[("a", 128)], ctx.snapshot(), NonZeroU32::new(2).unwrap())
        .unwrap();
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let client =
        ModelServiceClient::new(RpcLimits::new(131072, 32, 65536, 24576).unwrap()).unwrap();
    let consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: f.runtime(),
        expected: &expected,
    };
    let consumer = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let user = Fixture::new(&ctx).await;
    incoming!(user, ctx, verifier, proof, read, write);
    let other_user = Fixture::new(&ctx).await;
    incoming!(
        other_user,
        ctx,
        other_verifier,
        other_proof,
        _other_read,
        other_write
    );
    let mut calls = 0;
    let failure = consumer
        .index_user(
            &read,
            &other_write,
            read_limits(),
            write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("foreign owner")) }
                })
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(failure.cause, MemoryEmbeddingCause::Store(ref e) if matches!(**e, MemoryStoreError::WrongAdmission))
    );
    assert_eq!(calls, 0);
    let foreign = auth::request_until(&cancel, ctx.deadline().expect("finite fixture deadline"));
    incoming!(
        user,
        foreign,
        foreign_verifier,
        foreign_proof,
        _foreign_read,
        foreign_write
    );
    let failure = consumer
        .index_user(
            &read,
            &foreign_write,
            read_limits(),
            write_limits(),
            consumer_inference.limits(),
            |operation| {
                consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("foreign Original")) }
                })
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(failure.cause, MemoryEmbeddingCause::Store(ref e) if matches!(**e, MemoryStoreError::DifferentRequest))
    );
    assert_eq!(calls, 0);
    for permissions in [vec![Permission::ReadUser], vec![Permission::MutateUser]] {
        let restricted = Fixture::with_permissions(&ctx, &permissions).await;
        let v = restricted
            .server
            .verifier(true, restricted.provider_revision(&ctx));
        let raw = restricted
            .server
            .token(SUBJECT, RESOURCE, now_seconds() + 600);
        let p = v
            .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        assert!(
            restricted
                .issuer
                .admit_incoming_user_memory_read(&p, &v, &ctx, "service", restricted.scope())
                .is_err()
        );
        assert!(
            restricted
                .issuer
                .admit_incoming_user_memory_write(
                    &p,
                    &v,
                    &ctx,
                    "service",
                    restricted.scope(),
                    UserMemoryWriteOperation::Memory
                )
                .is_err()
        );
        assert!(!restricted.user.runtime_root().exists());
    }
    let fenced = Fixture::fenced(&ctx).await;
    let v = fenced.server.verifier(true, fenced.provider_revision(&ctx));
    let raw = fenced.server.token(SUBJECT, RESOURCE, now_seconds() + 600);
    let p = v
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let r = fenced
        .issuer
        .admit_incoming_user_memory_read(&p, &v, &ctx, "service", fenced.scope())
        .unwrap();
    let w = fenced
        .issuer
        .admit_incoming_user_memory_write(
            &p,
            &v,
            &ctx,
            "service",
            fenced.scope(),
            UserMemoryWriteOperation::FencedMemory,
        )
        .unwrap();
    assert!(
        matches!(consumer.index_user(&r, &w, read_limits(), write_limits(), consumer_inference.limits(), |operation| consumer_inference.infer(operation, {
        calls += 1; async { Err::<TcpStream, _>(io::Error::other("fenced is not ordinary")) }
    })).await.unwrap_err().cause, MemoryEmbeddingCause::Store(ref e) if matches!(**e, MemoryStoreError::WrongAdmission))
    );
    assert_eq!(calls, 0);
    assert!(
        issuer
            .admit_authenticated_user_memory_read(&caller, &ctx, user.scope())
            .is_err()
    );

    let native = Fixture::native(&ctx).await;
    native.install_native_profile(&ctx);
    let v = native
        .server
        .verifier(false, native.provider_revision(&ctx));
    let secret = native.secret();
    let native_caller = native
        .issuer
        .authenticate_runtime(&secret, &ctx)
        .unwrap()
        .authenticate_native(&v)
        .await
        .unwrap();
    let nr = native
        .issuer
        .admit_authenticated_user_memory_read(&native_caller, &ctx, native.scope())
        .unwrap();
    let nw = native
        .issuer
        .admit_authenticated_user_memory_write(
            &native_caller,
            &ctx,
            native.scope(),
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    assert!(matches!(
        consumer
            .index_user(
                &nr,
                &nw,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("native absent")) }
                })
            )
            .await
            .unwrap(),
        UserMemoryEmbeddingIndexRead::NoCorpus
    ));
    let logout = native
        .store
        .mutate(native.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            doc.logout(None, doc.revision(), ctx)
        })
        .unwrap();
    assert!(logout.failure().is_none());
    assert!(
        consumer
            .index_user(
                &nr,
                &nw,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("native logout")) }
                })
            )
            .await
            .is_err()
    );
    assert_eq!(calls, 0);
    let deadline = Instant::now() + Duration::from_secs(1);
    let short = auth::request_until(&cancel, deadline);
    let key = f.runtime().check(short.snapshot()).unwrap();
    let short_caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(short.snapshot()).unwrap()).unwrap(),
            &short,
        )
        .unwrap();
    let mut short_selection = issuer
        .capture_service(&short_caller, ServicePurpose::Embedding)
        .unwrap();
    let short_lease = short_selection.local().unwrap();
    let short_consumer_inference = auth::LocalInference {
        client: &client,
        lease: &short_lease,
        runtime: f.runtime(),
        expected: &expected,
    };
    let short_consumer = MemoryEmbeddingModel::new(
        short.snapshot(),
        short_consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    incoming!(
        user,
        short,
        short_verifier,
        short_proof,
        short_read,
        short_write
    );
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        deadline + Duration::from_millis(10),
    ))
    .await;
    assert!(
        short_consumer
            .index_user(
                &short_read,
                &short_write,
                read_limits(),
                write_limits(),
                short_consumer_inference.limits(),
                |operation| short_consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("expired Original deadline")) }
                })
            )
            .await
            .is_err()
    );
    assert_eq!(calls, 0);
    auth::revoke(&store, &ctx);
    // No corpus still requires only USER authority; the independent model lease is used at RPC.
    assert!(matches!(
        consumer
            .index_user(
                &read,
                &write,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async {
                        Err::<TcpStream, _>(io::Error::other(
                            "no corpus after model profile changed",
                        ))
                    }
                })
            )
            .await
            .unwrap(),
        UserMemoryEmbeddingIndexRead::NoCorpus
    ));
    assert_eq!(calls, 0);
    let published = user
        .store
        .mutate(user.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            let p = doc.provider("service").unwrap();
            doc.update_provider(
                "service",
                p.config().clone(),
                doc.revision(),
                p.revision(),
                p.updated_at() + 1,
                ctx,
            )
        })
        .unwrap();
    assert!(published.failure().is_none());
    assert!(
        consumer
            .index_user(
                &read,
                &write,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("changed USER provider")) }
                })
            )
            .await
            .is_err()
    );
    assert_eq!(calls, 0);
    assert_eq!(user.server.count("/userinfo"), 0);

    let expiry_user = Fixture::new(&ctx).await;
    let v = expiry_user
        .server
        .verifier(true, expiry_user.provider_revision(&ctx));
    let expiry = now_seconds() + 1;
    let raw = expiry_user.server.token(SUBJECT, RESOURCE, expiry);
    let p = v
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let r = expiry_user
        .issuer
        .admit_incoming_user_memory_read(&p, &v, &ctx, "service", expiry_user.scope())
        .unwrap();
    let w = expiry_user
        .issuer
        .admit_incoming_user_memory_write(
            &p,
            &v,
            &ctx,
            "service",
            expiry_user.scope(),
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let until = std::time::UNIX_EPOCH + Duration::from_secs(expiry);
    tokio::time::sleep(
        until
            .duration_since(std::time::SystemTime::now())
            .unwrap_or_default()
            + Duration::from_millis(20),
    )
    .await;
    assert!(
        consumer
            .index_user(
                &r,
                &w,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("expired signed USER token")) }
                })
            )
            .await
            .is_err()
    );
    assert_eq!(calls, 0);

    let physical = Fixture::new(&ctx).await;
    incoming!(physical, ctx, pv, pp, pr, pw);
    NativeUserMemoryStore::create(ctx.snapshot(), &pw, input("physical"), write_limits())
        .await
        .unwrap();
    let root = physical.user.user_root().unwrap();
    std::fs::rename(root, root.with_extension("saved")).unwrap();
    std::fs::create_dir(root).unwrap();
    assert!(
        consumer
            .index_user(
                &pr,
                &pw,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("replaced physical USER scope")) }
                })
            )
            .await
            .is_err()
    );
    assert_eq!(calls, 0);
    cancel.cancel();
    assert!(
        consumer
            .index_user(
                &read,
                &write,
                read_limits(),
                write_limits(),
                consumer_inference.limits(),
                |operation| consumer_inference.infer(operation, {
                    calls += 1;
                    async { Err::<TcpStream, _>(io::Error::other("cancelled Original")) }
                })
            )
            .await
            .is_err()
    );
    assert_eq!(calls, 0);
}

#[tokio::test]
#[ignore = "requires the pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn actual_gguf_user_memory_hybrid_rpc() {
    let (_snapshot_root, snapshot_parent) = model_fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let ctx = auth::request_until(&cancel, Instant::now() + Duration::from_secs(120));
    let model_owner = auth::Fixture::new(&ctx, "main");
    let (model_issuer, _store) = model_owner.issuer(&ctx);
    let key = model_owner.runtime().check(ctx.snapshot()).unwrap();
    let model_caller = model_issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selected = model_issuer
        .capture_service(&model_caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selected.local().unwrap();
    let mut fixture = model_fixture::Fixture::actual(model_api::TaskKind::Embedding);
    fixture.body["recipe"]["dimensions"] = serde_json::json!(128);
    let config = fixture.config(&["tiny"]);
    let expected = ExpectedModel::new(&config, "tiny", "service").unwrap();
    let sdk = NativeSdkRoot::new().unwrap();
    let service = model_fixture::service(snapshot_parent.clone());
    service
        .install(config, model_fixture::context(&cancel))
        .unwrap();
    let limits = model_fixture::rpc_limits();
    let client = ModelServiceClient::new(limits).unwrap();
    let consumer_inference = auth::LocalInference {
        client: &client,
        lease: &lease,
        runtime: model_owner.runtime(),
        expected: &expected,
    };
    let consumer = MemoryEmbeddingModel::new(
        ctx.snapshot(),
        consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let user = Fixture::new(&ctx).await;
    incoming!(user, ctx, verifier, proof, read, write);
    assert!(
        model_issuer
            .admit_authenticated_user_memory_read(&model_caller, &ctx, user.scope())
            .is_err()
    );
    let opts = || MemoryHybridSearchOptions {
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(2).unwrap(),
    };
    let connected = std::sync::atomic::AtomicBool::new(false);
    let absent = consumer
        .query_user_hybrid(&read, "Paris".into(), opts(), read_limits(), |operation| {
            consumer_inference.infer(operation, async {
                connected.store(true, std::sync::atomic::Ordering::SeqCst);
                Err::<TcpStream, _>(io::Error::other("absent must not connect"))
            })
        })
        .await
        .unwrap();
    assert_eq!(absent, UserMemoryHybridSearchRead::NoCorpus);
    assert!(!connected.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!user.user.runtime_root().exists());
    let mut tiny = write_limits();
    tiny.max_row_bytes = NonZeroUsize::MIN;
    assert!(matches!(
        NativeUserMemoryStore::create(ctx.snapshot(), &write, input("empty-birth"), tiny).await,
        Err(MemoryStoreError::Limit(_))
    ));
    let empty = consumer
        .query_user_hybrid(&read, "Paris".into(), opts(), read_limits(), |operation| {
            consumer_inference.infer(operation, async {
                connected.store(true, std::sync::atomic::Ordering::SeqCst);
                Err::<TcpStream, _>(io::Error::other("empty must not connect"))
            })
        })
        .await
        .unwrap();
    assert!(matches!(
        empty,
        UserMemoryHybridSearchRead::PublishedEmpty { .. }
    ));
    assert!(!connected.load(std::sync::atomic::Ordering::SeqCst));
    let mut value = input("tiny-user-paris");
    value.title = "Paris".into();
    value.body = "Paris".into();
    let memory = NativeUserMemoryStore::create(ctx.snapshot(), &write, value, write_limits())
        .await
        .unwrap();
    let (address, connection, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    let index = consumer
        .index_user(
            &read,
            &write,
            read_limits(),
            write_limits(),
            consumer_inference.limits(),
            |operation| consumer_inference.infer(operation, TcpStream::connect(address)),
        )
        .await
        .unwrap();
    let publication = publication(index);
    assert_eq!(publication.generation.owner.identity.subject(), SUBJECT);
    connection.cancel();
    task.await.unwrap().unwrap();
    let UserMemoryEmbeddingCorpusRead::Published(mut covered) =
        NativeUserMemoryStore::capture_embeddings(
            ctx.snapshot(),
            &read,
            consumer.identity().clone(),
            read_limits(),
        )
        .await
        .unwrap()
    else {
        panic!("covered USER")
    };
    assert!(!covered.is_empty());
    // USER Document iteration may emit covered sources; exhaustion is separate from corpus existence.
    while covered
        .next_documents(ctx.snapshot(), &read)
        .await
        .unwrap()
        .is_some()
    {}
    assert!(!covered.is_empty());
    assert!(
        covered
            .next_documents(ctx.snapshot(), &read)
            .await
            .unwrap()
            .is_none()
    );
    drop(covered);
    let before = pin(&user).await.version();
    let (address, connection, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    let UserMemoryHybridSearchRead::Published {
        generation,
        total_hits,
        hits,
    } = consumer
        .query_user_hybrid(&read, "Paris".into(), opts(), read_limits(), |operation| {
            consumer_inference.infer(operation, async {
                connected.store(true, std::sync::atomic::Ordering::SeqCst);
                TcpStream::connect(address).await
            })
        })
        .await
        .unwrap()
    else {
        panic!("actual USER hybrid RPC result")
    };
    assert!(connected.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(generation.sdk_version, before);
    assert_eq!(generation.owner, publication.generation.owner);
    assert_eq!(generation.physical_branch, None);
    assert_eq!(generation.inherited_target, None);
    assert_eq!(total_hits, 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current, memory);
    assert!(hits[0].lexical.is_some() && hits[0].semantic.is_some());
    assert_eq!(hits[0].score, 2.0 / 61.0);
    assert_eq!(pin(&user).await.version(), before);
    connection.cancel();
    task.await.unwrap().unwrap();
    connected.store(false, std::sync::atomic::Ordering::SeqCst);
    let failure = consumer
        .query_user_hybrid(&read, " ".into(), opts(), read_limits(), |operation| {
            consumer_inference.infer(operation, async {
                connected.store(true, std::sync::atomic::Ordering::SeqCst);
                Err::<TcpStream, _>(io::Error::other("invalid must not connect"))
            })
        })
        .await
        .unwrap_err();
    assert!(
        matches!(failure.cause,MemoryEmbeddingCause::Store(ref cause) if matches!(cause.as_ref(),MemoryStoreError::Invalid(_)))
    );
    assert!(!connected.load(std::sync::atomic::Ordering::SeqCst));
    let (address, connection, task) = rpc::server(
        service.clone(),
        secret(&model_owner, ctx.snapshot()),
        limits,
    );
    let failure = consumer
        .query_user_hybrid(&read, "Paris".into(), opts(), read_limits(), |operation| {
            consumer_inference.infer(operation, async {
                let stream = TcpStream::connect(address).await?;
                connected.store(true, std::sync::atomic::Ordering::SeqCst);
                let mutation = user
                    .store
                    .mutate(user.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                        let provider = doc.provider("service").unwrap();
                        let revision = provider.revision();
                        let at = provider.updated_at() + 1;
                        let config = provider.config().clone();
                        doc.update_provider("service", config, doc.revision(), revision, at, ctx)
                    })
                    .unwrap();
                assert!(mutation.failure().is_none());
                Ok::<TcpStream, io::Error>(stream)
            })
        })
        .await
        .unwrap_err();
    assert!(connected.load(std::sync::atomic::Ordering::SeqCst));
    assert!(matches!(failure.cause, MemoryEmbeddingCause::Store(_)));
    assert!(failure.last_attachment.is_none());
    assert_eq!(pin(&user).await.version(), before);
    connection.cancel();
    task.await.unwrap().unwrap();
    connected.store(false, std::sync::atomic::Ordering::SeqCst);
    let failure = consumer
        .query_user_hybrid(&read, "Paris".into(), opts(), read_limits(), |operation| {
            consumer_inference.infer(operation, async {
                connected.store(true, std::sync::atomic::Ordering::SeqCst);
                Err::<TcpStream, _>(io::Error::other("retired admission must not connect"))
            })
        })
        .await
        .unwrap_err();
    assert!(matches!(failure.cause, MemoryEmbeddingCause::Store(_)));
    assert!(failure.last_attachment.is_none());
    assert!(!connected.load(std::sync::atomic::Ordering::SeqCst));
    service
        .shutdown(model_fixture::context(&cancel))
        .await
        .unwrap();
    drop(sdk);
}
