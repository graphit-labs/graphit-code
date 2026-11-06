#[path = "support/model_service_revocation.rs"]
mod model_service_revocation;
use graphit_daemon::model_child as child_api;
#[path = "support/model_auth.rs"]
mod auth;
#[path = "support/model_fixture.rs"]
mod fixture;
#[path = "support/gemma_model.rs"]
mod gemma_model;
#[path = "support/read_hook.rs"]
mod read_hook;
#[path = "support/model_rpc.rs"]
mod rpc;
use fixture::*;
use graphit_auth::{ServicePurpose, profile::AiService};
use graphit_core::file_lock::Cancellation;
use graphit_daemon::model_service as model_api;
use graphit_daemon::model_service::*;
use graphit_daemon::model_transport as disconnect_api;
use std::num::NonZeroUsize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::net::TcpStream;
use zeroize::Zeroizing;

fn assert_normalized(output: &EmbedOutput) {
    assert!(!output.vectors.is_empty());
    for vector in &output.vectors {
        assert_eq!(vector.len() as u64, output.dimensions);
        assert!(vector.iter().all(|x| x.is_finite()));
        let norm: f64 = vector.iter().map(|x| f64::from(*x).powi(2)).sum();
        assert!((norm - 1.0).abs() < 1e-4);
    }
}

fn secret(
    f: &auth::AuthFixture,
    ctx: &graphit_hub::policy::RequestContext<'_>,
) -> Arc<RuntimeSecret> {
    let borrow = f.runtime.check(ctx.snapshot()).unwrap();
    Arc::new(RuntimeSecret::new(Zeroizing::new(*borrow.secret(ctx.snapshot()).unwrap())).unwrap())
}

fn service_request(signal: &Cancellation) -> graphit_hub::policy::RequestContext<'_> {
    graphit_hub::policy::RequestContext::service(
        graphit_hub::native::ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        signal,
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "requires pinned real GGUF artifacts; isolated native bootstrap"]
async fn real_sdk_embedding_query_document_rerank_and_shutdown() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let cancel = Cancellation::default();
    let fixture = Fixture::actual(TaskKind::Embedding);
    let config = fixture.config(&["a", "b"]);
    let id = config.config_id("a").unwrap();
    assert_eq!(config.config_id("b"), Some(id));
    let service = service(snapshot_parent.clone());
    assert_eq!(
        service.install(config.clone(), context(&cancel)).unwrap(),
        1
    );
    assert_eq!(service.install(config, context(&cancel)).unwrap(), 1);
    let out = service
        .embed(
            "a",
            id,
            EmbedInput::Document,
            vec!["a b".into(), "a".into(), "b".into()],
            context(&cancel),
        )
        .await
        .unwrap();
    assert_normalized(&out);
    assert_eq!((out.generation, out.dimensions), (1, 768));
    let out = service
        .embed(
            "b",
            id,
            EmbedInput::Query,
            vec!["a".into()],
            context(&cancel),
        )
        .await
        .unwrap();
    assert_normalized(&out);
    let rank = Fixture::actual(TaskKind::Rerank);
    let config = rank.config(&["r"]);
    let id = config.config_id("r").unwrap();
    assert_eq!(service.install(config, context(&cancel)).unwrap(), 2);
    let out = service
        .rerank(
            "r",
            id,
            "a".into(),
            vec!["a".into(), "a b".into()],
            context(&cancel),
        )
        .await
        .unwrap();
    assert_eq!(out.scores.len(), 2);
    assert!(out.scores.iter().all(|score| score.is_finite()));
    service.shutdown(context(&cancel)).await.unwrap();
    assert_eq!(
        service
            .rerank("r", id, "a".into(), vec!["a".into()], context(&cancel))
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::Shutdown
    );
}

#[tokio::test]
async fn configuration_changes_with_same_metadata_reject_before_native() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let fixture = Fixture::metadata(TaskKind::Embedding);
    let original = fixture.config(&["a"]);
    let old = original.config_id("a").unwrap();
    let service = service(snapshot_parent.clone());
    service.install(original, context(&cancel)).unwrap();
    for provider in [
        graphit_auth::profile::LocalDevice::Cpu,
        graphit_auth::profile::LocalDevice::Cuda,
    ] {
        let mut selection = fixture::cpu_selection();
        selection.embedding.as_mut().unwrap().device = provider;
        let mut settings = fixture.settings(&["a"]);
        let RuntimeSettings::EmbeddingGemma2 { threads, .. } = &mut settings.slots[0].runtime
        else {
            panic!("fixture must use GGUF embedding");
        };
        *threads = std::num::NonZeroU32::new(3).unwrap();
        let config = fixture.resolve_with_selection(settings, selection).unwrap();
        let new = config.config_id("a").unwrap();
        assert_ne!(old, new);
        service.install(config, context(&cancel)).unwrap();
        assert_eq!(
            service
                .embed(
                    "a",
                    old,
                    EmbedInput::Query,
                    vec!["a".into()],
                    context(&cancel)
                )
                .await
                .unwrap_err()
                .kind(),
            ModelErrorKind::ConfigChanged
        );
    }
    let mut changed = Fixture::metadata(TaskKind::Embedding);
    changed.body["recipe"]["dimensions"] = serde_json::json!(128);
    let config = changed.config(&["a"]);
    let new = config.config_id("a").unwrap();
    assert_ne!(old, new);
    service.install(config, context(&cancel)).unwrap();
    assert_eq!(
        service
            .embed(
                "a",
                old,
                EmbedInput::Query,
                vec!["a".into()],
                context(&cancel)
            )
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::ConfigChanged
    );
    service.shutdown(context(&cancel)).await.unwrap();
}

#[test]
fn strict_settings_and_missing_are_distinct() {
    let empty =
        graphit_core::config::ConfigSnapshot::new(&graphit_core::brand::Brand::default(), [])
            .unwrap();
    assert!(ModelSettings::read(&empty).unwrap().is_none());
    let settings: ModelSettings = serde_json::from_str("{\"slots\":[]}").unwrap();
    assert!(settings.slots.is_empty());
    assert!(serde_json::from_str::<ModelSettings>("{\"slots\":[],\"unknown\":1}").is_err());
    assert!(serde_json::from_str::<ModelSettings>("{}").is_err());
    let fixture = Fixture::metadata(TaskKind::Embedding);
    assert_eq!(
        fixture
            .resolve(fixture.settings(&["a", "a"]))
            .err()
            .unwrap()
            .kind(),
        ModelErrorKind::Configuration
    );
}

#[tokio::test]
#[ignore = "requires pinned real GGUF artifacts; isolated native bootstrap"]
async fn actual_auth_and_runtime_owner_drive_real_rpc_embedding_rerank_shutdown() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(180));
    let f = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let native = Fixture::actual(TaskKind::Embedding);
    let config = native.config_with_original(&["a"], ctx.snapshot());
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let service = service(snapshot_parent.clone());
    service.install(config, context(&cancel)).unwrap();
    let embedding_ctx = service_request(&cancel);
    let issuer = f.issuer(&embedding_ctx, ServicePurpose::Embedding, auth::cpu());
    let borrow = f.runtime.check(embedding_ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(borrow.secret(embedding_ctx.snapshot()).unwrap()).unwrap(),
            &embedding_ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, &embedding_ctx), rpc_limits());
    let client = ModelServiceClient::new(rpc_limits()).unwrap();
    let out = client
        .embed(
            &lease,
            &f.runtime,
            &expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["a".into()],
                deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    assert_normalized(&out);
    assert!(embedding_ctx.snapshot().deadline().is_none());
    embedding_ctx.check().unwrap();
    assert!(Arc::ptr_eq(
        embedding_ctx.snapshot(),
        lease.original().unwrap().snapshot()
    ));
    let _ = task.await.unwrap();
    let native = Fixture::actual(TaskKind::Rerank);
    let config = native.config_with_original(&["r"], ctx.snapshot());
    let expected = ExpectedModel::new(&config, "r", "service").unwrap();
    service.install(config, context(&cancel)).unwrap();
    let issuer = f.issuer(&ctx, ServicePurpose::Rerank, auth::cpu());
    let borrow = f.runtime.check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.local().unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, &ctx), rpc_limits());
    let out = client
        .rerank(
            &lease,
            &f.runtime,
            &expected,
            "a".into(),
            vec!["a".into(), "a b".into()],
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    assert_eq!(out.scores.len(), 2);
    assert!(out.scores.iter().all(|score| score.is_finite()));
    let _ = task.await.unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, &ctx), rpc_limits());
    client
        .shutdown(&f.runtime, ctx.snapshot(), TcpStream::connect(address))
        .await
        .unwrap();
    let _ = task.await.unwrap();
}

#[tokio::test]
async fn stale_local_lease_denies_before_connection_and_while_waiting() {
    let started = Instant::now();
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let f = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let native = Fixture::metadata(TaskKind::Embedding);
    let config = native.config(&["a"]);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let borrow = f.runtime.check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let client = ModelServiceClient::new(rpc_limits()).unwrap();
    let (entered, phase) = tokio::sync::oneshot::channel();
    let (release, wait) = tokio::sync::oneshot::channel();
    let connect = async {
        entered.send(()).unwrap();
        wait.await.unwrap();
        Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
    };
    let call = client.embed(
        &lease,
        &f.runtime,
        &expected,
        graphit_daemon::model_service::EmbedOperation {
            input: EmbedInput::Query,
            texts: vec!["a".into()],
            deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
        },
        connect,
    );
    tokio::pin!(call);
    tokio::select! {out=&mut call=>panic!("should await connect: {out:?}"),out=phase=>out.unwrap()}
    f.revoke_service(&ctx);
    release.send(()).unwrap();
    let error = call.await.unwrap_err();
    assert_eq!(
        error.kind(),
        ModelErrorKind::Authentication,
        "elapsed={:?} error={error:?}",
        started.elapsed()
    );
    assert!(std::error::Error::source(&error).is_some());
    let polled = std::sync::atomic::AtomicBool::new(false);
    let connect = async {
        polled.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
    };
    assert_eq!(
        client
            .embed(
                &lease,
                &f.runtime,
                &expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec!["a".into()],
                    deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
                },
                connect
            )
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::Authentication
    );
    assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn malformed_trailing_truncated_oversized_frames_are_real_transport_errors() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    use bincode::Options;
    use tokio::io::AsyncWriteExt;
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let f = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let borrow = f.runtime.check(ctx.snapshot()).unwrap();
    let message = tarpc::ClientMessage::Request(tarpc::Request {
        id: 0,
        context: tarpc::context::current(),
        message: ModelRpcRequest::Embed {
            credential: RuntimeCredential::from_secret(borrow.secret(ctx.snapshot()).unwrap())
                .unwrap(),
            slot: "a".into(),
            expected_config_id: ConfigId([0; 32]),
            input: EmbedInput::Query,
            texts: vec!["a".into()],
        },
    });
    let mut trailing = bincode::DefaultOptions::new().serialize(&message).unwrap();
    trailing.push(0);
    for (length, body) in [
        (8, vec![0xff; 8]),
        (trailing.len() as u32, trailing),
        (10, vec![0]),
        ((rpc_limits().max_frame_bytes + 1) as u32, vec![]),
    ] {
        let (address, connection, task) = rpc::server(
            service(snapshot_parent.clone()),
            secret(&f, &ctx),
            rpc_limits(),
        );
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(&length.to_be_bytes()).await.unwrap();
        stream.write_all(&body).await.unwrap();
        stream.shutdown().await.unwrap();
        let error = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind(), ModelErrorKind::Transport);
        assert!(std::error::Error::source(&error).is_some());
        assert!(connection.is_cancelled());
    }
    for length in [0, 63, 65] {
        let bytes = bincode::DefaultOptions::new()
            .serialize(&vec![b'a'; length])
            .unwrap();
        assert!(
            bincode::DefaultOptions::new()
                .deserialize::<RuntimeCredential>(&bytes)
                .is_err()
        );
    }
}

#[tokio::test]
async fn wrong_runtime_secret_rejected_before_configuration_and_native() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let f = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let (address, _, task) = rpc::server(
        service(snapshot_parent.clone()),
        secret(&f, &ctx),
        rpc_limits(),
    );
    let stream = TcpStream::connect(address).await.unwrap();
    let tarpc::client::NewClient { client, dispatch } = ModelRpcClient::new(
        tarpc::client::Config::default(),
        client_transport(stream, rpc_limits()).unwrap(),
    );
    let call = client.embed(
        tarpc::context::current(),
        RuntimeCredential::from_secret(&[b'a'; 64]).unwrap(),
        "absent".into(),
        ConfigId([0; 32]),
        EmbedInput::Query,
        vec!["a".into()],
    );
    let result = {
        tokio::pin!(call, dispatch);
        tokio::select! {result=&mut call=>result.unwrap(),result=&mut dispatch=>panic!("dispatch ended: {result:?}")}
    };
    let error = result.unwrap_err();
    assert_eq!(error.kind, ModelErrorKind::Authentication);
    assert_eq!(error.limit, None);
    drop(client);
    let _ = task.await.unwrap();

    // Both preflights reject before a model is loaded. Use the real Auth/IPC producers.
    let native = Fixture::metadata(TaskKind::Embedding);
    let expected = ExpectedModel::new(&native.config(&["a"]), "a", "service").unwrap();
    let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let borrow = f.runtime.check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let client = ModelServiceClient::new(rpc_limits()).unwrap();
    let polled = std::sync::atomic::AtomicBool::new(false);
    let local = client
        .embed(
            &lease,
            &f.runtime,
            &expected,
            EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["private-model-input-secret-canary".repeat(3000)],
                deadline: ctx.snapshot().required_deadline().unwrap(),
            },
            async {
                polled.store(true, std::sync::atomic::Ordering::SeqCst);
                std::future::pending::<std::io::Result<TcpStream>>().await
            },
        )
        .await
        .unwrap_err();
    assert_eq!(local.kind(), ModelErrorKind::Limit);
    assert!(format!("{local:?}").contains("LocalInputLimit"));
    assert!(!format!("{local:?}").contains("canary"));
    assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
    let remote_limits = RpcLimits::new(65536, 32, 1, 128).unwrap();
    let (address, _, task) = rpc::server(
        service(snapshot_parent.clone()),
        secret(&f, &ctx),
        remote_limits,
    );
    let remote = client
        .embed(
            &lease,
            &f.runtime,
            &expected,
            EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["a b".into()],
                deadline: ctx.snapshot().required_deadline().unwrap(),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap_err();
    assert_eq!(remote.kind(), ModelErrorKind::Limit);
    assert!(format!("{remote:?}").contains("RemoteLimit(InputLimit)"));
    assert!(Arc::ptr_eq(
        lease.original().unwrap().snapshot(),
        ctx.snapshot()
    ));
    ctx.check().unwrap();
    let _ = task.await.unwrap();
}

#[tokio::test]
async fn invalid_recipe_and_input_limits_return_no_output_before_native() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let mut invalid = Fixture::metadata(TaskKind::Embedding);
    invalid.body["recipe"]["dimensions"] = serde_json::json!(3);
    assert!(invalid.resolve(invalid.settings(&["a"])).is_err());
    let fixture = Fixture::metadata(TaskKind::Embedding);
    let config = fixture.config(&["a"]);
    let id = config.config_id("a").unwrap();
    let service = service(snapshot_parent.clone());
    service.install(config, context(&cancel)).unwrap();
    let error = service
        .embed(
            "a",
            id,
            EmbedInput::Document,
            vec!["a".repeat(65537)],
            context(&cancel),
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ModelErrorKind::Limit);
    service.shutdown(context(&cancel)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires pinned real GGUF artifacts; isolated native bootstrap"]
async fn actual_auth_revocation_after_compute_before_delivery_suppresses_output() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(180));
    let f = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let fixture = Fixture::actual(TaskKind::Embedding);
    let config = fixture.config_with_original(&["a"], ctx.snapshot());
    let id = config.config_id("a").unwrap();
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let service = service(snapshot_parent.clone());
    service.install(config, context(&cancel)).unwrap();
    let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let borrow = f.runtime.check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, &ctx), rpc_limits());
    let invoked = std::sync::atomic::AtomicBool::new(false);
    let connect = async {
        Ok::<_, std::io::Error>(read_hook::ReadHook {
            stream: TcpStream::connect(address).await?,
            hook: Some(|| {
                f.revoke_service(&ctx);
                invoked.store(true, std::sync::atomic::Ordering::SeqCst);
            }),
        })
    };
    let error = ModelServiceClient::new(rpc_limits())
        .unwrap()
        .embed(
            &lease,
            &f.runtime,
            &expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["a".into()],
                deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
            },
            connect,
        )
        .await
        .unwrap_err();
    assert!(invoked.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(error.kind(), ModelErrorKind::Authentication);
    let _ = task.await.unwrap();
    // The original wire response required real native compute. Its model remains
    // usable independently, while the revoked caller receives no output.
    let out = service
        .embed(
            "a",
            id,
            EmbedInput::Query,
            vec!["a".into()],
            context(&cancel),
        )
        .await
        .unwrap();
    assert_normalized(&out);
    service.shutdown(context(&cancel)).await.unwrap();
}

#[tokio::test]
async fn actual_local_no_active_disabled_purpose_execution_and_device_id_are_preserved() {
    use graphit_auth::{
        ServiceUseError,
        profile::{LocalDevice, LocalExecution, ProfileError},
    };
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    for (activate, embedding) in [
        (false, AiService::Local(auth::cpu())),
        (true, AiService::Disabled),
    ] {
        let f = auth::AuthFixture::new(&ctx, embedding, AiService::Disabled, activate);
        let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
        let borrow = f.runtime.check(ctx.snapshot()).unwrap();
        let caller = issuer
            .authenticate_runtime(
                std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
                &ctx,
            )
            .unwrap();
        let result = issuer.capture_service(&caller, ServicePurpose::Embedding);
        if activate {
            assert!(matches!(result, Err(ServiceUseError::Disabled)));
        } else {
            assert!(matches!(
                result,
                Err(ServiceUseError::Profile(ProfileError::NoActive))
            ));
        }
    }
    let native = Fixture::metadata(TaskKind::Embedding);
    let config = native.config(&["a"]);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    for (execution, purpose, kind) in [
        (
            LocalExecution {
                device: LocalDevice::Cuda,
                device_id: 0,
            },
            ServicePurpose::Embedding,
            ModelErrorKind::Authentication,
        ),
        (
            LocalExecution {
                device: LocalDevice::Cpu,
                device_id: 1,
            },
            ServicePurpose::Embedding,
            ModelErrorKind::Authentication,
        ),
        (
            auth::cpu(),
            ServicePurpose::Rerank,
            ModelErrorKind::Authentication,
        ),
    ] {
        let f = auth::AuthFixture::new(
            &ctx,
            AiService::Local(execution),
            AiService::Local(execution),
            true,
        );
        let issuer = f.issuer(&ctx, purpose, execution);
        let borrow = f.runtime.check(ctx.snapshot()).unwrap();
        let caller = issuer
            .authenticate_runtime(
                std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
                &ctx,
            )
            .unwrap();
        let mut selection = issuer.capture_service(&caller, purpose).unwrap();
        let lease = selection.local().unwrap();
        let polled = std::sync::atomic::AtomicBool::new(false);
        let connect = async {
            polled.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
        };
        assert_eq!(
            ModelServiceClient::new(rpc_limits())
                .unwrap()
                .embed(
                    &lease,
                    &f.runtime,
                    &expected,
                    graphit_daemon::model_service::EmbedOperation {
                        input: EmbedInput::Query,
                        texts: vec!["a".into()],
                        deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
                    },
                    connect
                )
                .await
                .unwrap_err()
                .kind(),
            kind
        );
        assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
    }
}

#[test]
fn captured_bundle_binding_preserves_root_request_capacity_and_selection_errors() {
    use graphit_ai::{
        model_bindings::bind_captured_model,
        model_manifest::{ModelSelection, ModelTask},
    };
    use graphit_core::artifact_store::{SnapshotContext, SourceReadLimits};
    let fixture = Fixture::metadata(TaskKind::Embedding);
    let other = Fixture::metadata(TaskKind::Embedding);
    std::fs::write(
        fixture.directory.path().join("model.json"),
        serde_json::to_vec(&fixture.body).unwrap(),
    )
    .unwrap();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    let capture = || {
        CapturedModelInput::capture(
            "a",
            fixture.directory.path(),
            &capture_policy(),
            SourceReadLimits::new(65536, 512, 65536).unwrap(),
            original.clone(),
        )
        .unwrap()
    };
    let mut settings = fixture.settings(&["a"]);
    settings.slots[0].bundle = other.directory.path().into();
    assert_eq!(
        ResolvedConfig::resolve(
            settings,
            fixture::cpu_selection(),
            [capture()],
            &original,
            &manifest_limits()
        )
        .err()
        .unwrap()
        .kind(),
        ModelErrorKind::Configuration
    );
    let mut small = manifest_limits();
    small.max_manifest_bytes = 1;
    assert_eq!(
        ResolvedConfig::resolve(
            fixture.settings(&["a"]),
            fixture::cpu_selection(),
            [capture()],
            &original,
            &small
        )
        .err()
        .unwrap()
        .kind(),
        ModelErrorKind::Limit
    );
    let mut settings = fixture.settings(&["a"]);
    settings.slots[0].selection.revision = "different".into();
    assert_eq!(
        ResolvedConfig::resolve(
            settings,
            fixture::cpu_selection(),
            [capture()],
            &original,
            &manifest_limits()
        )
        .err()
        .unwrap()
        .kind(),
        ModelErrorKind::Selection
    );
    let reader = fixture::capture(fixture.directory.path(), &original);
    original.cancellation().cancel();
    let binding = bind_captured_model(
        &reader,
        &original,
        std::path::Path::new("model.json"),
        &ModelSelection {
            id: "embeddinggemma2",
            revision: "bfcd298762cc34d0357ece5ebdd31791a3a374d8",
            task: ModelTask::Embedding,
        },
        &manifest_limits(),
    )
    .err()
    .unwrap();
    assert_eq!(binding.kind, std::io::ErrorKind::Interrupted);
    assert_eq!(
        ModelServiceError::from(binding).kind(),
        ModelErrorKind::Cancelled
    );
    let expired = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    ));
    let reader = fixture::capture(fixture.directory.path(), &expired);
    std::thread::sleep(Duration::from_millis(25));
    let binding = bind_captured_model(
        &reader,
        &expired,
        std::path::Path::new("model.json"),
        &ModelSelection {
            id: "embeddinggemma2",
            revision: "bfcd298762cc34d0357ece5ebdd31791a3a374d8",
            task: ModelTask::Embedding,
        },
        &manifest_limits(),
    )
    .err()
    .unwrap();
    assert_eq!(binding.kind, std::io::ErrorKind::TimedOut);
    assert_eq!(
        ModelServiceError::from(binding).kind(),
        ModelErrorKind::Deadline
    );
}

#[tokio::test]
async fn original_cancellation_and_deadline_preserve_typed_cause_during_connect_and_rpc() {
    let setup_cancel = Cancellation::default();
    let setup = auth::request(&setup_cancel, Instant::now() + Duration::from_secs(30));
    let f = auth::AuthFixture::new(
        &setup,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let native = Fixture::metadata(TaskKind::Embedding);
    let config = native.config(&["a"]);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    for expire in [false, true] {
        for rpc_wait in [false, true] {
            let cancel = Cancellation::default();
            let ctx = auth::request(
                &cancel,
                Instant::now()
                    + if expire {
                        Duration::from_secs(5)
                    } else {
                        Duration::from_secs(30)
                    },
            );
            let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
            let borrow = f.runtime.check(ctx.snapshot()).unwrap();
            let caller = issuer
                .authenticate_runtime(
                    std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
                    &ctx,
                )
                .unwrap();
            let mut selection = issuer
                .capture_service(&caller, ServicePurpose::Embedding)
                .unwrap();
            let lease = selection.local().unwrap();
            let client = ModelServiceClient::new(rpc_limits()).unwrap();
            let (ready, phase) = tokio::sync::oneshot::channel();
            let (stream, mut peer) = tokio::io::duplex(65536);
            let connect = async {
                if rpc_wait {
                    Ok::<_, std::io::Error>(stream)
                } else {
                    ready.send(()).unwrap();
                    std::future::pending::<std::io::Result<tokio::io::DuplexStream>>().await
                }
            };
            let call = client.embed(
                &lease,
                &f.runtime,
                &expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec!["a".into()],
                    deadline: ctx.snapshot().deadline().expect("finite fixture Original")
                        + Duration::from_secs(30),
                },
                connect,
            );
            tokio::pin!(call);
            if rpc_wait {
                use tokio::io::AsyncReadExt;
                let mut byte = [0; 1];
                tokio::select! { out=&mut call => panic!("call completed before response: {out:?}"), out=peer.read_exact(&mut byte) => {out.unwrap();} }
            } else {
                tokio::select! { out=&mut call => panic!("call completed before connection: {out:?}"), out=phase => out.unwrap() }
            }
            if !expire {
                cancel.cancel();
            }
            let error = tokio::time::timeout(Duration::from_secs(7), call)
                .await
                .unwrap()
                .unwrap_err();
            assert_eq!(
                error.kind(),
                if expire {
                    ModelErrorKind::Deadline
                } else {
                    ModelErrorKind::Cancelled
                },
                "expire={expire} rpc_wait={rpc_wait} error={error:?}"
            );
            assert!(
                std::error::Error::source(&error)
                    .unwrap()
                    .downcast_ref::<graphit_auth::ServiceUseError>()
                    .is_some()
            );
        }
    }
}

#[tokio::test]
async fn interruption_before_first_client_poll_preserves_context_error_without_connect() {
    let setup_cancel = Cancellation::default();
    let setup = auth::request(&setup_cancel, Instant::now() + Duration::from_secs(30));
    let f = auth::AuthFixture::new(
        &setup,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let fixture = Fixture::metadata(TaskKind::Embedding);
    let config = fixture.config(&["a"]);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    // Issuer construction and key I/O are setup; the tested Original starts below.
    let issuer = f.issuer(&setup, ServicePurpose::Embedding, auth::cpu());
    let borrow = f.runtime.check(setup.snapshot()).unwrap();
    let runtime_secret = Zeroizing::new(*borrow.secret(setup.snapshot()).unwrap());
    drop(borrow);
    for expire in [false, true] {
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_millis(500);
        let ctx = auth::request(&cancel, deadline);
        let caller = issuer
            .authenticate_runtime(std::str::from_utf8(runtime_secret.as_ref()).unwrap(), &ctx)
            .unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selection.local().unwrap();
        if expire {
            tokio::time::sleep_until(deadline.into()).await;
        } else {
            cancel.cancel();
        }
        let polled = std::sync::atomic::AtomicBool::new(false);
        let connect = async {
            polled.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
        };
        let error = ModelServiceClient::new(rpc_limits())
            .unwrap()
            .embed(
                &lease,
                &f.runtime,
                &expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec!["a".into()],
                    deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
                },
                connect,
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.kind(),
            if expire {
                ModelErrorKind::Deadline
            } else {
                ModelErrorKind::Cancelled
            }
        );
        assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<graphit_auth::ServiceUseError>()
                .is_some()
        );
    }
}

#[tokio::test]
async fn embedding_operation_expiry_preserves_service_original_and_next_rpc() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let setup_cancel = Cancellation::default();
    let setup = auth::request(&setup_cancel, Instant::now() + Duration::from_secs(30));
    let f = auth::AuthFixture::new(
        &setup,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let native = Fixture::metadata(TaskKind::Embedding);
    let config = native.config(&["a"]);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let signal = Cancellation::default();
    let ctx = service_request(&signal);
    let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let borrow = f.runtime.check(ctx.snapshot()).unwrap();
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let client = ModelServiceClient::new(rpc_limits()).unwrap();
    let polled = AtomicBool::new(false);
    let expired = client
        .embed(
            &lease,
            &f.runtime,
            &expected,
            EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["a".into()],
                deadline: Instant::now(),
            },
            async {
                polled.store(true, Ordering::SeqCst);
                Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
            },
        )
        .await
        .unwrap_err();
    assert_eq!(expired.kind(), ModelErrorKind::Deadline);
    assert!(!polled.load(Ordering::SeqCst));
    assert!(ctx.snapshot().deadline().is_none());
    ctx.check().unwrap();
    assert!(Arc::ptr_eq(
        ctx.snapshot(),
        lease.original().unwrap().snapshot()
    ));
    let connected = client
        .embed(
            &lease,
            &f.runtime,
            &expected,
            EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["a".into()],
                deadline: Instant::now() + Duration::from_secs(30),
            },
            async {
                polled.store(true, Ordering::SeqCst);
                Err::<tokio::io::DuplexStream, _>(std::io::Error::from(
                    std::io::ErrorKind::ConnectionRefused,
                ))
            },
        )
        .await
        .unwrap_err();
    assert!(polled.load(Ordering::SeqCst));
    assert_eq!(connected.kind(), ModelErrorKind::Transport);
    assert_eq!(
        std::error::Error::source(&connected)
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::ConnectionRefused
    );
    assert!(ctx.snapshot().deadline().is_none());
    ctx.check().unwrap();
    assert!(Arc::ptr_eq(
        ctx.snapshot(),
        lease.original().unwrap().snapshot()
    ));
}

#[tokio::test]
async fn embedding_operation_service_original_cancel_and_rpc_deadline_close_connect_and_rpc() {
    use tokio::io::AsyncReadExt;
    let setup_cancel = Cancellation::default();
    let setup = auth::request(&setup_cancel, Instant::now() + Duration::from_secs(30));
    let f = auth::AuthFixture::new(
        &setup,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let native = Fixture::metadata(TaskKind::Embedding);
    let config = native.config(&["a"]);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let client = ModelServiceClient::new(rpc_limits()).unwrap();
    for expire in [false, true] {
        for rpc_wait in [false, true] {
            let signal = Cancellation::default();
            let ctx = service_request(&signal);
            let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
            let borrow = f.runtime.check(ctx.snapshot()).unwrap();
            let caller = issuer
                .authenticate_runtime(
                    std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
                    &ctx,
                )
                .unwrap();
            let mut selection = issuer
                .capture_service(&caller, ServicePurpose::Embedding)
                .unwrap();
            let lease = selection.local().unwrap();
            let (ready, phase) = tokio::sync::oneshot::channel();
            let (stream, mut peer) = tokio::io::duplex(65536);
            let connect = async {
                if rpc_wait {
                    Ok::<_, std::io::Error>(stream)
                } else {
                    ready.send(()).unwrap();
                    std::future::pending::<std::io::Result<tokio::io::DuplexStream>>().await
                }
            };
            // Prepare real admissions before freezing only the RPC operation clock.
            // The phase barrier below uses in-runtime duplex/oneshot I/O.
            tokio::time::pause();
            let operation_started = tokio::time::Instant::now();
            let operation_deadline = operation_started + Duration::from_secs(5);
            let call = client.embed(
                &lease,
                &f.runtime,
                &expected,
                EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec!["a".into()],
                    deadline: operation_deadline.into_std(),
                },
                connect,
            );
            tokio::pin!(call);
            if rpc_wait {
                let mut byte = [0; 1];
                tokio::select! {
                    out = &mut call => panic!("call completed before response: {out:?}"),
                    out = peer.read_exact(&mut byte) => { out.unwrap(); }
                }
            } else {
                tokio::select! {
                    out = &mut call => panic!("call completed before connection: {out:?}"),
                    out = phase => out.unwrap()
                }
            }
            assert_eq!(tokio::time::Instant::now(), operation_started);
            if expire {
                tokio::time::advance(operation_deadline - operation_started).await;
            } else {
                signal.cancel();
            }
            let error = tokio::time::timeout(Duration::from_secs(7), call)
                .await
                .unwrap()
                .unwrap_err();
            assert_eq!(
                error.kind(),
                if expire {
                    ModelErrorKind::Deadline
                } else {
                    ModelErrorKind::Cancelled
                }
            );
            assert!(ctx.snapshot().deadline().is_none());
            if expire {
                assert_eq!(
                    std::error::Error::source(&error).unwrap().to_string(),
                    "DeadlineLocalOperation"
                );
                ctx.check().unwrap();
                assert!(Arc::ptr_eq(
                    ctx.snapshot(),
                    lease.original().unwrap().snapshot()
                ));
            } else {
                assert!(
                    std::error::Error::source(&error)
                        .unwrap()
                        .downcast_ref::<graphit_auth::ServiceUseError>()
                        .is_some()
                );
            }
            if rpc_wait {
                tokio::time::timeout(Duration::from_secs(2), peer.read_to_end(&mut Vec::new()))
                    .await
                    .unwrap()
                    .unwrap();
            }
            tokio::time::resume();
        }
    }
}

#[tokio::test]
async fn service_context_is_rejected_before_finite_model_rpc_connections() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let setup_cancel = Cancellation::default();
    let setup = auth::request(&setup_cancel, Instant::now() + Duration::from_secs(30));
    let f = auth::AuthFixture::new(
        &setup,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let signal = Cancellation::default();
    let ctx = service_request(&signal);
    let client = ModelServiceClient::new(rpc_limits()).unwrap();
    {
        let purpose = ServicePurpose::Rerank;
        let slot = "r";
        let fixture = Fixture::metadata(TaskKind::Rerank);
        let config = fixture.config(&[slot]);
        let expected = ExpectedModel::new(&config, slot, "service").unwrap();
        let issuer = f.issuer(&ctx, purpose, auth::cpu());
        let borrow = f.runtime.check(ctx.snapshot()).unwrap();
        let caller = issuer
            .authenticate_runtime(
                std::str::from_utf8(borrow.secret(ctx.snapshot()).unwrap()).unwrap(),
                &ctx,
            )
            .unwrap();
        let mut selection = issuer.capture_service(&caller, purpose).unwrap();
        let lease = selection.local().unwrap();
        let polled = AtomicBool::new(false);
        let connect = async {
            polled.store(true, Ordering::SeqCst);
            Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
        };
        let error = client
            .rerank(
                &lease,
                &f.runtime,
                &expected,
                "q".into(),
                vec!["a".into()],
                connect,
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.kind(), ModelErrorKind::Configuration);
        assert_eq!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
        assert!(!polled.load(Ordering::SeqCst));
    }
    let polled = AtomicBool::new(false);
    let connect = async {
        polled.store(true, Ordering::SeqCst);
        Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
    };
    let error = client
        .shutdown(&f.runtime, ctx.snapshot(), connect)
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind(), ModelErrorKind::Configuration);
    assert_eq!(
        std::error::Error::source(&error)
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert!(!polled.load(Ordering::SeqCst));
}

#[tokio::test]
#[ignore = "requires the pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn actual_gemma_service_rpc_single_query_768() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    use graphit_ai::{
        embedding::EmbeddingLimits, native_embedding::NativeSdkRoot,
        native_rerank::NativeRerankLimits,
    };
    let model = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL").expect("pinned BF16 fixture"),
    );
    let native = gemma_model::GemmaFixture::new(&model);
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(180));
    let f = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let config = native.config(&[("a", 768)], &cancel);
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
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
    let inference = || {
        graphit_ai::context::InferenceContext::new(
            &cancel,
            ctx.snapshot().required_deadline().unwrap(),
        )
    };
    service.install(config, inference()).unwrap();
    let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let key = f.runtime.check(ctx.snapshot()).unwrap();
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
    let limits = RpcLimits::new(131072, 32, 65536, 24576).unwrap();
    let client = ModelServiceClient::new(limits).unwrap();
    let (address, _, task) = rpc::server(service.clone(), secret(&f, &ctx), limits);
    let output = client
        .embed(
            &lease,
            &f.runtime,
            &expected,
            EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["Paris is the capital of France.".into()],
                deadline: ctx.snapshot().required_deadline().unwrap(),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    assert_eq!(output.model, "embeddinggemma2");
    assert_eq!(output.revision, "bfcd298762cc34d0357ece5ebdd31791a3a374d8");
    assert_eq!(output.config_id, expected.config_id());
    assert_eq!(output.dimensions, 768);
    assert_eq!(output.vectors.len(), 1);
    assert_eq!(output.vectors[0].len(), 768);
    assert!(output.vectors[0].iter().all(|value| value.is_finite()));
    let norm: f32 = output.vectors[0].iter().map(|value| value * value).sum();
    assert!((norm - 1.).abs() < 1e-4);
    println!(
        "Gemma2 RPC: model={} revision={} dimensions={} rows=1 finite=true normalized=true",
        output.model, output.revision, output.dimensions
    );
    task.await.unwrap().unwrap();
    service.shutdown(inference()).await.unwrap();
    drop(service);
    drop(sdk);
}

#[tokio::test]
#[ignore = "requires the pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn actual_gemma_service_rpc_four_roles_and_retirement() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    use graphit_ai::{
        embedding::EmbeddingLimits, native_embedding::NativeSdkRoot,
        native_rerank::NativeRerankLimits,
    };
    let model = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL").expect("pinned BF16 fixture"),
    );
    let native = gemma_model::GemmaFixture::new(&model);
    let cancel = Cancellation::default();
    let manifest_path = native.directory.path().join("768.json");
    let manifest_bytes = std::fs::read(&manifest_path).unwrap();
    let mut bad_manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    bad_manifest["artifacts"][0]["digest"][0] = serde_json::json!(0);
    std::fs::write(&manifest_path, serde_json::to_vec(&bad_manifest).unwrap()).unwrap();
    let error = native.try_config(&[("a", 768)], &cancel).err().unwrap();
    assert_eq!(error.kind(), ModelErrorKind::Artifact);
    assert!(std::error::Error::source(&error).is_some());
    std::fs::write(&manifest_path, manifest_bytes).unwrap();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(180));
    let f = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let config = native.config(&[("a", 768), ("alias", 768), ("b", 128)], &cancel);
    assert_eq!(config.config_id("a"), config.config_id("alias"));
    assert_ne!(config.config_id("a"), config.config_id("b"));
    let expected = ExpectedModel::new(&config, "a", "service").unwrap();
    let b = ExpectedModel::new(&config, "b", "service").unwrap();
    use graphit_auth::profile::{LocalDevice, LocalExecution};
    for execution in [
        LocalExecution {
            device: LocalDevice::Auto,
            device_id: 0,
        },
        LocalExecution {
            device: LocalDevice::Cuda,
            device_id: 0,
        },
        #[cfg(target_os = "macos")]
        LocalExecution {
            device: LocalDevice::Metal,
            device_id: 0,
        },
        LocalExecution {
            device: LocalDevice::Cpu,
            device_id: 1,
        },
    ] {
        let bad =
            auth::AuthFixture::new(&ctx, AiService::Local(execution), AiService::Disabled, true);
        let issuer = bad.issuer(&ctx, ServicePurpose::Embedding, execution);
        let key = bad.runtime.check(ctx.snapshot()).unwrap();
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
        let polled = std::sync::atomic::AtomicBool::new(false);
        let connect = async {
            polled.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
        };
        assert_eq!(
            ModelServiceClient::new(rpc_limits())
                .unwrap()
                .embed(
                    &lease,
                    &bad.runtime,
                    &expected,
                    graphit_daemon::model_service::EmbedOperation {
                        input: EmbedInput::Query,
                        texts: vec!["request".into()],
                        deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
                    },
                    connect,
                )
                .await
                .unwrap_err()
                .kind(),
            ModelErrorKind::Authentication
        );
        assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
    }
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
    let inference = || {
        graphit_ai::context::InferenceContext::new(
            &cancel,
            ctx.snapshot().required_deadline().unwrap(),
        )
    };
    service.install(config, inference()).unwrap();
    let issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let key = f.runtime.check(ctx.snapshot()).unwrap();
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
    let client =
        ModelServiceClient::new(RpcLimits::new(131072, 32, 65536, 24576).unwrap()).unwrap();
    for input in [
        EmbedInput::Query,
        EmbedInput::Document,
        EmbedInput::CodeQuery,
        EmbedInput::CodeDocument,
    ] {
        let (address, _, task) = rpc::server(
            service.clone(),
            secret(&f, &ctx),
            RpcLimits::new(131072, 32, 65536, 24576).unwrap(),
        );
        let output = client
            .embed(
                &lease,
                &f.runtime,
                &expected,
                graphit_daemon::model_service::EmbedOperation {
                    input,
                    texts: vec!["Paris is the capital of France.".into()],
                    deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
                },
                TcpStream::connect(address),
            )
            .await
            .unwrap();
        assert_eq!(output.dimensions, 768);
        assert_eq!(output.config_id, expected.config_id());
        assert_eq!(output.vectors[0].len(), 768);
        let norm: f32 = output.vectors[0].iter().map(|v| v * v).sum();
        assert!((norm - 1.).abs() < 1e-4);
        task.await.unwrap().unwrap();
    }
    f.revoke_service(&ctx);
    let polled = std::sync::atomic::AtomicBool::new(false);
    let connect = async {
        polled.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok::<_, std::io::Error>(tokio::io::duplex(1024).0)
    };
    assert_eq!(
        client
            .embed(
                &lease,
                &f.runtime,
                &expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec!["old binding".into()],
                    deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
                },
                connect
            )
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::Authentication
    );
    assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
    // Revocation disabled the provider and staled its profile. A new issuer
    // alone cannot restore authority: explicitly enable the service and rebind
    // the profile through the normal public Auth operations before recapturing.
    let restored = f
        .store
        .mutate(f.store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            let provider = doc.provider("service").unwrap();
            let mut config = provider.config().clone();
            config.embedding = AiService::Local(auth::cpu());
            let now = provider.updated_at() + 1;
            doc.update_provider(
                "service",
                config,
                doc.revision(),
                provider.revision(),
                now,
                ctx,
            )?;
            let profile = doc.profile("fixture").unwrap();
            let expected = graphit_auth::profile::ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("service").unwrap().revision(),
                profile_revision: Some(profile.revision()),
            };
            doc.replace_profile_data(auth::profile("fixture"), expected, now + 1, ctx)
        })
        .unwrap();
    assert!(restored.failure().is_none());
    assert_eq!(
        restored.effect(),
        graphit_core::auth_document::publication::AuthPublicationEffect::Applied
    );
    let renewed_issuer = f.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let renewed_caller = renewed_issuer
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut renewed_selection = renewed_issuer
        .capture_service(&renewed_caller, ServicePurpose::Embedding)
        .unwrap();
    let renewed_lease = renewed_selection.local().unwrap();
    let (address, _, task) = rpc::server(
        service.clone(),
        secret(&f, &ctx),
        RpcLimits::new(131072, 32, 65536, 24576).unwrap(),
    );
    let output = client
        .embed(
            &renewed_lease,
            &f.runtime,
            &b,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::CodeQuery,
                texts: vec!["fn main() {}".into()],
                deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(output.dimensions, 128);
    service
        .install(native.config(&[("b", 128)], &cancel), inference())
        .unwrap();
    assert_eq!(
        service
            .embed(
                "a",
                expected.config_id(),
                EmbedInput::Query,
                vec!["retired".into()],
                inference()
            )
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::Selection
    );
    assert_eq!(
        service
            .embed(
                "b",
                b.config_id(),
                EmbedInput::CodeDocument,
                vec!["fn main() {}".into()],
                inference()
            )
            .await
            .unwrap()
            .dimensions,
        128
    );
    service.shutdown(inference()).await.unwrap();
    drop(service);
    drop(sdk);
}
