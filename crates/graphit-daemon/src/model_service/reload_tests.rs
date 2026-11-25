use super::*;
use crate::model_service as model_api;
use crate::{
    model_child::{ChildOptions, GlobalModelConfig},
    model_transport::DisconnectTransport,
};
use model_api::TaskKind;
#[path = "../../tests/support/model_auth.rs"]
#[allow(
    dead_code,
    reason = "shared physical auth helpers cover other service tests"
)]
mod auth;
use crate::model_service::test_fixture as fixture;
use graphit_auth::profile::AiService;
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigStore},
    project_paths::InstallationPaths,
};
use std::time::{Duration, Instant};

fn write(root: &std::path::Path, value: serde_json::Value, cancel: &Cancellation) {
    let paths = InstallationPaths::resolve(Brand::default(), root, None, Some(root)).unwrap();
    ConfigStore::global(&paths)
        .unwrap()
        .set(
            ConfigKey::new("daemon.models").unwrap(),
            value,
            Duration::from_secs(5),
            cancel,
        )
        .unwrap();
}
fn credential() -> RuntimeCredential {
    RuntimeCredential::from_secret(&[b'a'; 64]).unwrap()
}
fn context() -> tarpc::context::Context {
    let mut ctx = tarpc::context::current();
    ctx.deadline = Instant::now() + Duration::from_secs(30);
    ctx
}

fn source(root: &std::path::Path) -> Arc<GlobalModelConfig> {
    let options = ChildOptions::standard().unwrap();
    Arc::new(
        GlobalModelConfig::new(
            root.to_owned(),
            options.capture,
            options.source,
            options.manifest,
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn refresh_installs_only_requested_slots_and_returns_canonical_ack() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::new(
        &request,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let embedding = fixture::Fixture::metadata(TaskKind::Embedding);
    let rerank = fixture::Fixture::metadata(TaskKind::Rerank);
    embedding.config(&["e"]);
    rerank.config(&["r"]);
    let mut settings = embedding.settings(&["e"]);
    settings.slots.extend(rerank.settings(&["r"]).slots);
    write(
        auth.installation_root(),
        serde_json::to_value(settings).unwrap(),
        &cancel,
    );
    let paths = InstallationPaths::resolve(
        Brand::default(),
        auth.installation_root(),
        None,
        Some(auth.installation_root()),
    )
    .unwrap();
    ConfigStore::global(&paths)
        .unwrap()
        .set(
            ConfigKey::new("search.rerank").unwrap(),
            serde_json::Value::Bool(true),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    let global = source(auth.installation_root());
    let config = global
        .resolve(request.snapshot().clone(), &["e".into(), "r".into()])
        .unwrap()
        .unwrap();
    let all = config.config_ids();
    let required = BTreeMap::from([("e".to_owned(), all["e"])]);
    let service = fixture::service(snapshot_parent.clone());
    let borrow = auth.runtime.check(request.snapshot()).unwrap();
    let secret = Arc::new(
        RuntimeSecret::new(Zeroizing::new(*borrow.secret(request.snapshot()).unwrap())).unwrap(),
    );
    let (client_stream, server_stream) = tokio::io::duplex(1024 * 1024);
    let connection = Cancellation::default();
    let transport = DisconnectTransport::new(
        server_transport(server_stream, fixture::rpc_limits()).unwrap(),
        connection.clone(),
    );
    let server = tokio::spawn(serve_connection(
        transport,
        service.clone(),
        secret,
        fixture::rpc_limits(),
        global.clone(),
        connection,
    ));
    let ack = model_api::ModelServiceClient::new(fixture::rpc_limits())
        .unwrap()
        .refresh(&auth.runtime, request.snapshot(), required.clone(), async {
            Ok(client_stream)
        })
        .await
        .unwrap();
    assert_eq!(ack.config_ids, required);
    assert_eq!(
        service.slot_names(fixture::context(&cancel)).unwrap(),
        ["e"]
    );
    assert_eq!(ack.generation, 1);
    let _ = server.await.unwrap();
    let wrong = BTreeMap::from([("e".to_owned(), ConfigId([0; 32]))]);
    assert_eq!(
        global
            .refresh(service.clone(), wrong, request.snapshot().clone())
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::ConfigChanged
    );
    assert!(
        service
            .has_model(
                "e",
                all["e"],
                graphit_ai::model_manifest::ModelTask::Embedding,
                fixture::context(&cancel)
            )
            .unwrap()
    );
    global.join().await.unwrap();
    service.shutdown(fixture::context(&cancel)).await.unwrap();
}

#[test]
fn inactive_rerank_skips_missing_bundle_before_capture() {
    let cancel = Cancellation::default();
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::new(
        &request,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let embedding = fixture::Fixture::metadata(TaskKind::Embedding);
    embedding.config(&["e"]);
    let mut settings = embedding.settings(&["e"]);
    let mut inactive = embedding.settings(&["r"]).slots.remove(0);
    inactive.selection.task = TaskKind::Rerank;
    inactive.bundle = auth.installation_root().join("absent-rerank-bundle");
    settings.slots.push(inactive);
    write(
        auth.installation_root(),
        serde_json::to_value(settings).unwrap(),
        &cancel,
    );
    let global = source(auth.installation_root());
    let selected = global
        .resolve(request.snapshot().clone(), &["e".into()])
        .unwrap()
        .unwrap();
    assert!(selected.config_id("e").is_some());
    assert!(selected.config_id("r").is_none());
    assert!(
        global
            .resolve(request.snapshot().clone(), &["r".into()])
            .is_err()
    );
}

#[test]
fn cold_local_rerank_needs_no_model_settings_and_no_active_provider_has_no_child() {
    for activate in [true, false] {
        let cancel = Cancellation::default();
        let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = auth::AuthFixture::new(
            &request,
            AiService::Disabled,
            AiService::Local(auth::cpu()),
            activate,
        );
        // This is intentionally invalid: the inactive purpose must not even
        // decode model settings, much less capture a bundle or load the SDK.
        write(
            auth.installation_root(),
            serde_json::json!({"slots": "invalid"}),
            &cancel,
        );
        let global = source(auth.installation_root());
        let selected = global.resolve(request.snapshot().clone(), &[]).unwrap();
        if activate {
            assert!(selected.unwrap().config_ids().is_empty());
            assert_eq!(
                global
                    .resolve(request.snapshot().clone(), &["r".into()])
                    .err()
                    .unwrap()
                    .kind(),
                ModelErrorKind::Configuration
            );
        } else {
            assert!(selected.is_none());
            assert!(
                global
                    .resolve(request.snapshot().clone(), &["r".into()])
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[tokio::test]
async fn demanded_slots_ignore_other_bundles_and_refresh_ignores_rerank_default() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    use graphit_ai::model_manifest::ModelTask;
    for (task, slot) in [(TaskKind::Embedding, "e"), (TaskKind::Rerank, "r")] {
        for rerank_default in [false, true] {
            let cancel = Cancellation::default();
            let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
            let auth = auth::AuthFixture::new(
                &request,
                AiService::Local(auth::cpu()),
                AiService::Local(auth::cpu()),
                true,
            );
            let selected = fixture::Fixture::metadata(task);
            let selected_config = selected.config(&[slot]);
            let id = selected_config.config_id(slot).unwrap();
            let mut settings = selected.settings(&[slot]);
            let other = match task {
                TaskKind::Embedding => TaskKind::Rerank,
                TaskKind::Rerank => TaskKind::Embedding,
            };
            let mut unused = selected.settings(&["unused"]).slots.remove(0);
            unused.selection.task = other;
            unused.bundle = auth.installation_root().join("absent-unused-bundle");
            settings.slots.push(unused);
            write(
                auth.installation_root(),
                serde_json::to_value(settings).unwrap(),
                &cancel,
            );
            let paths = InstallationPaths::resolve(
                Brand::default(),
                auth.installation_root(),
                None,
                Some(auth.installation_root()),
            )
            .unwrap();
            ConfigStore::global(&paths)
                .unwrap()
                .set(
                    ConfigKey::new("search.rerank").unwrap(),
                    serde_json::json!(rerank_default),
                    Duration::from_secs(5),
                    &cancel,
                )
                .unwrap();
            let global = source(auth.installation_root());
            assert!(
                global
                    .resolve(request.snapshot().clone(), &[])
                    .unwrap()
                    .unwrap()
                    .config_ids()
                    .is_empty()
            );
            let service = fixture::service(snapshot_parent.clone());
            let purpose = match task {
                TaskKind::Embedding => ModelTask::Embedding,
                TaskKind::Rerank => ModelTask::Rerank,
            };
            global
                .ensure_model(
                    service.clone(),
                    slot.into(),
                    id,
                    purpose,
                    request.snapshot().clone(),
                )
                .await
                .unwrap();
            let required = BTreeMap::from([(slot.to_owned(), id)]);
            let ack = global
                .refresh(
                    service.clone(),
                    required.clone(),
                    request.snapshot().clone(),
                )
                .await
                .unwrap();
            assert_eq!(ack.config_ids, required);
            assert_eq!(
                service.slot_names(fixture::context(&cancel)).unwrap(),
                [slot]
            );
            assert_eq!(
                global
                    .refresh(
                        service.clone(),
                        BTreeMap::from([(slot.to_owned(), ConfigId([0; 32]))]),
                        request.snapshot().clone(),
                    )
                    .await
                    .unwrap_err()
                    .kind(),
                ModelErrorKind::ConfigChanged
            );
            global
                .refresh(service.clone(), BTreeMap::new(), request.snapshot().clone())
                .await
                .unwrap();
            assert!(
                service
                    .slot_names(fixture::context(&cancel))
                    .unwrap()
                    .is_empty()
            );
            global.join().await.unwrap();
            service.shutdown(fixture::context(&cancel)).await.unwrap();
        }
    }
}

#[tokio::test]
async fn lazy_model_admission_preserves_both_orders_and_concurrent_requests() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    use graphit_ai::model_manifest::ModelTask;
    for order in 0..3 {
        let cancel = Cancellation::default();
        let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = auth::AuthFixture::new(
            &request,
            AiService::Local(auth::cpu()),
            AiService::Local(auth::cpu()),
            true,
        );
        let embedding = fixture::Fixture::metadata(TaskKind::Embedding);
        let rerank = fixture::Fixture::metadata(TaskKind::Rerank);
        let e = embedding.config(&["e"]).config_id("e").unwrap();
        let r = rerank.config(&["r"]).config_id("r").unwrap();
        let mut settings = embedding.settings(&["e"]);
        settings.slots.extend(rerank.settings(&["r"]).slots);
        write(
            auth.installation_root(),
            serde_json::to_value(settings).unwrap(),
            &cancel,
        );
        let service = fixture::service(snapshot_parent.clone());
        let global = source(auth.installation_root());
        let mut demands = [("e", e, ModelTask::Embedding), ("r", r, ModelTask::Rerank)];
        if order == 2 {
            let start = Arc::new(tokio::sync::Barrier::new(3));
            let requests = demands.map(|(slot, id, purpose)| {
                let global = global.clone();
                let service = service.clone();
                let original = request.snapshot().clone();
                let start = start.clone();
                tokio::spawn(async move {
                    start.wait().await;
                    global
                        .ensure_model(service, slot.into(), id, purpose, original)
                        .await
                })
            });
            start.wait().await;
            for request in requests {
                request.await.unwrap().unwrap();
            }
        } else {
            if order == 1 {
                demands.reverse();
            }
            for (slot, id, purpose) in demands {
                global
                    .ensure_model(
                        service.clone(),
                        slot.into(),
                        id,
                        purpose,
                        request.snapshot().clone(),
                    )
                    .await
                    .unwrap();
            }
        }
        assert_eq!(
            service.slot_names(fixture::context(&cancel)).unwrap(),
            ["e", "r"]
        );
        global
            .refresh(
                service.clone(),
                BTreeMap::from([("r".to_owned(), r)]),
                request.snapshot().clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            service.slot_names(fixture::context(&cancel)).unwrap(),
            ["r"]
        );
        global.join().await.unwrap();
        service.shutdown(fixture::context(&cancel)).await.unwrap();
    }
}

#[test]
fn global_idle_setting_is_positive_and_keeps_captured_slot_identity() {
    let signal = Cancellation::default();
    let request = auth::request(&signal, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::new(
        &request,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let fixture = fixture::Fixture::metadata(TaskKind::Embedding);
    write(
        auth.installation_root(),
        serde_json::to_value(fixture.settings(&["e"])).unwrap(),
        &signal,
    );
    let source = source(auth.installation_root());
    let initial = source
        .resolve(request.snapshot().clone(), &["e".into()])
        .unwrap()
        .unwrap();
    assert_eq!(initial.idle_timeout(), Duration::from_secs(300));
    let paths = InstallationPaths::resolve(
        Brand::default(),
        auth.installation_root(),
        None,
        Some(auth.installation_root()),
    )
    .unwrap();
    let store = ConfigStore::global(&paths).unwrap();
    let key = ConfigKey::new("models.idle_timeout_seconds").unwrap();
    store
        .set(
            key.clone(),
            serde_json::json!(2),
            Duration::from_secs(5),
            &signal,
        )
        .unwrap();
    let changed = source
        .resolve(request.snapshot().clone(), &["e".into()])
        .unwrap()
        .unwrap();
    assert_eq!(changed.idle_timeout(), Duration::from_secs(2));
    assert_eq!(changed.config_ids(), initial.config_ids());
    store
        .set(key, serde_json::json!(0), Duration::from_secs(5), &signal)
        .unwrap();
    assert_eq!(
        source
            .resolve(request.snapshot().clone(), &["e".into()])
            .err()
            .unwrap()
            .kind(),
        ModelErrorKind::Configuration
    );
}

#[tokio::test]
#[ignore = "requires pinned real v2-m3 F16 GGUF; isolated native bootstrap"]
async fn rerank_rpc_is_opt_in_and_checks_auth_limits_and_identity_before_lazy_load() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let cancel = Cancellation::default();
    let setup = auth::request(&cancel, Instant::now() + Duration::from_secs(180));
    let auth = auth::AuthFixture::new(
        &setup,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let embedding = fixture::Fixture::metadata(TaskKind::Embedding);
    let rerank = fixture::Fixture::actual_with_original(TaskKind::Rerank, setup.snapshot());
    let mut settings = embedding.settings(&["e"]);
    settings.slots.extend(rerank.settings(&["r"]).slots);
    write(
        auth.installation_root(),
        serde_json::to_value(settings).unwrap(),
        &cancel,
    );
    setup.snapshot().check().unwrap();
    drop(setup);
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(180));
    let global = source(auth.installation_root());
    let first = global
        .resolve(request.snapshot().clone(), &["e".into()])
        .unwrap()
        .unwrap();
    let e = first.config_id("e").unwrap();
    let expected = global
        .resolve(request.snapshot().clone(), &["r".into()])
        .unwrap()
        .unwrap()
        .config_id("r")
        .unwrap();
    let context = || {
        let mut context = tarpc::context::current();
        context.deadline = request.snapshot().required_deadline().unwrap();
        context
    };
    let service = fixture::service(snapshot_parent.clone());
    service.install(first, fixture::context(&cancel)).unwrap();
    assert!(
        !service
            .has_model(
                "r",
                expected,
                graphit_ai::model_manifest::ModelTask::Rerank,
                fixture::context(&cancel)
            )
            .unwrap()
    );
    let server = RpcServer {
        service: service.clone(),
        secret: Arc::new(RuntimeSecret::new(Zeroizing::new([b'a'; 64])).unwrap()),
        limits: fixture::rpc_limits(),
        global: global.clone(),
    };
    assert_eq!(
        server
            .clone()
            .rerank(
                context(),
                RuntimeCredential::from_secret(&[b'b'; 64]).unwrap(),
                "r".into(),
                expected,
                "a".into(),
                vec!["a".into()]
            )
            .await
            .unwrap_err(),
        ModelErrorKind::Authentication
    );
    assert_eq!(
        server
            .clone()
            .rerank(
                context(),
                credential(),
                "r".into(),
                expected,
                "a".into(),
                vec!["a".into(); 33]
            )
            .await
            .unwrap_err(),
        ModelErrorKind::Limit
    );
    assert!(
        !service
            .has_model(
                "r",
                expected,
                graphit_ai::model_manifest::ModelTask::Rerank,
                fixture::context(&cancel)
            )
            .unwrap()
    );
    let output = server
        .clone()
        .rerank(
            context(),
            credential(),
            "r".into(),
            expected,
            "a".into(),
            vec!["a".into(), "a b".into()],
        )
        .await
        .unwrap();
    assert_eq!(output.scores.len(), 2);
    assert!(output.scores.iter().all(|score| score.is_finite()));
    assert_eq!(output.generation, 2);
    assert!(
        output
            .device
            .matches_request(graphit_ai::local_execution::LocalProvider::CPU, 0)
    );
    assert!(
        service
            .has_model(
                "e",
                e,
                graphit_ai::model_manifest::ModelTask::Embedding,
                fixture::context(&cancel)
            )
            .unwrap()
    );
    let resident = server
        .clone()
        .rerank(
            context(),
            credential(),
            "r".into(),
            expected,
            "a".into(),
            vec!["a".into(), "a b".into()],
        )
        .await
        .unwrap();
    assert_eq!(resident.generation, output.generation);
    assert_eq!(resident.config_id, expected);
    assert_eq!(resident.scores, output.scores);
    assert_eq!(
        server
            .rerank(
                context(),
                credential(),
                "r".into(),
                ConfigId([0; 32]),
                "a".into(),
                vec!["a".into()]
            )
            .await
            .unwrap_err(),
        ModelErrorKind::ConfigChanged
    );
    global.join().await.unwrap();
    service.shutdown(fixture::context(&cancel)).await.unwrap();
}

#[tokio::test]
async fn lazy_rerank_existing_identity_and_context_stay_terminal() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let fixture = fixture::Fixture::metadata(TaskKind::Rerank);
    let config = fixture.config(&["r"]);
    let id = config.config_id("r").unwrap();
    let service = fixture::service(snapshot_parent.clone());
    service.install(config, fixture::context(&cancel)).unwrap();
    let absent = tempfile::tempdir().unwrap();
    let global = source(absent.path());
    // The existing exact slot needs no second Auth/config capture from this absent installation.
    global
        .ensure_model(
            service.clone(),
            "r".into(),
            id,
            graphit_ai::model_manifest::ModelTask::Rerank,
            request.snapshot().clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        global
            .ensure_model(
                service.clone(),
                "r".into(),
                ConfigId([0; 32]),
                graphit_ai::model_manifest::ModelTask::Rerank,
                request.snapshot().clone()
            )
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::ConfigChanged
    );
    let timeout = Arc::new(SnapshotContext::new(
        Instant::now(),
        Cancellation::default(),
    ));
    assert_eq!(
        global
            .ensure_model(
                service.clone(),
                "r".into(),
                id,
                graphit_ai::model_manifest::ModelTask::Rerank,
                timeout
            )
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::Deadline
    );
    let stopped = Cancellation::default();
    stopped.cancel();
    let stopped = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(1),
        stopped,
    ));
    assert_eq!(
        global
            .ensure_model(
                service.clone(),
                "r".into(),
                id,
                graphit_ai::model_manifest::ModelTask::Rerank,
                stopped
            )
            .await
            .unwrap_err()
            .kind(),
        ModelErrorKind::Cancelled
    );
    global.join().await.unwrap();
    service.shutdown(fixture::context(&cancel)).await.unwrap();
}

#[tokio::test]
async fn rerank_service_input_limits_precede_optional_config_capture() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let root = tempfile::tempdir().unwrap();
    let global = source(root.path());
    let mut limits = ChildOptions::standard().unwrap().service;
    limits.rerank =
        graphit_ai::native_rerank::NativeRerankLimits::new(1, 1, 8, 4096, 4096).unwrap();
    let service = ModelService::new(snapshot_parent.clone(), limits).unwrap();
    let server = RpcServer {
        service: service.clone(),
        secret: Arc::new(RuntimeSecret::new(Zeroizing::new([b'a'; 64])).unwrap()),
        limits: fixture::rpc_limits(),
        global: global.clone(),
    };
    // Both requests fit the RPC envelope; the actual service limit is smaller.
    for (query, docs) in [
        ("a", vec!["a".into(), "b".into()]),
        ("a too long query", vec!["a".into()]),
    ] {
        assert_eq!(
            server
                .clone()
                .rerank(
                    context(),
                    credential(),
                    "r".into(),
                    ConfigId([0; 32]),
                    query.into(),
                    docs
                )
                .await
                .unwrap_err(),
            ModelErrorKind::Limit
        );
    }
    global.join().await.unwrap();
    service
        .shutdown(fixture::context(&Cancellation::default()))
        .await
        .unwrap();
}

#[tokio::test]
async fn global_refresh_auth_identity_invalid_disable_reenable_and_client_ack() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(60));
    let auth = auth::AuthFixture::new(
        &request,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let root = auth.installation_root();
    let fixture = fixture::Fixture::metadata(TaskKind::Embedding);
    let first = fixture.config(&["e"]);
    let first_ids = first.config_ids();
    let service = fixture::service(snapshot_parent.clone());
    assert_eq!(
        service
            .install(first.clone(), fixture::context(&cancel))
            .unwrap(),
        1
    );
    write(
        root,
        serde_json::to_value(fixture.settings(&["e"])).unwrap(),
        &cancel,
    );
    let options = ChildOptions::standard().unwrap();
    let global = Arc::new(
        GlobalModelConfig::new(
            root.to_owned(),
            options.capture,
            options.source,
            options.manifest,
        )
        .unwrap(),
    );
    let secret = Arc::new(RuntimeSecret::new(Zeroizing::new([b'a'; 64])).unwrap());
    let server = RpcServer {
        service: service.clone(),
        secret,
        limits: fixture::rpc_limits(),
        global: global.clone(),
    };
    let wrong = RuntimeCredential::from_secret(&[b'b'; 64]).unwrap();
    assert_eq!(
        server
            .clone()
            .refresh(context(), wrong, BTreeMap::new())
            .await
            .unwrap_err(),
        ModelErrorKind::Authentication
    );
    assert_eq!(
        server
            .clone()
            .refresh(context(), credential(), first_ids.clone())
            .await
            .unwrap()
            .generation,
        1
    );

    let mut settings = fixture.settings(&["e"]);
    let model_api::RuntimeSettings::EmbeddingGemma2 { threads, .. } =
        &mut settings.slots[0].runtime
    else {
        panic!("GGUF embedding fixture")
    };
    *threads = std::num::NonZeroU32::new(3).unwrap();
    let second = fixture.resolve(settings).unwrap();
    let second_ids = second.config_ids();
    let mut settings = fixture.settings(&["e"]);
    let model_api::RuntimeSettings::EmbeddingGemma2 { threads, .. } =
        &mut settings.slots[0].runtime
    else {
        panic!("GGUF embedding fixture")
    };
    *threads = std::num::NonZeroU32::new(3).unwrap();
    write(root, serde_json::to_value(settings).unwrap(), &cancel);
    assert_eq!(
        server
            .clone()
            .refresh(context(), credential(), first_ids)
            .await
            .unwrap_err(),
        ModelErrorKind::ConfigChanged
    );
    assert_eq!(
        service.install(first, fixture::context(&cancel)).unwrap(),
        1
    );

    // Exercise the public runtime-authenticated client and the actual bounded codec.
    let borrow = auth.runtime.check(request.snapshot()).unwrap();
    let runtime_secret = Arc::new(
        RuntimeSecret::new(Zeroizing::new(*borrow.secret(request.snapshot()).unwrap())).unwrap(),
    );
    let (client_stream, server_stream) = tokio::io::duplex(65536);
    let connection = Cancellation::default();
    let task = tokio::spawn(serve_connection(
        DisconnectTransport::new(
            server_transport(server_stream, fixture::rpc_limits()).unwrap(),
            connection.clone(),
        ),
        service.clone(),
        runtime_secret,
        fixture::rpc_limits(),
        global.clone(),
        connection,
    ));
    let client = model_api::ModelServiceClient::new(fixture::rpc_limits()).unwrap();
    let ack = client
        .refresh(
            &auth.runtime,
            request.snapshot(),
            second_ids.clone(),
            async { Ok(client_stream) },
        )
        .await
        .unwrap();
    assert_eq!((ack.config_ids, ack.generation), (second_ids.clone(), 2));
    let _ = task.await.unwrap();

    write(
        root,
        serde_json::json!({"slots":[{"invalid":true}]}),
        &cancel,
    );
    assert_eq!(
        server
            .clone()
            .refresh(context(), credential(), second_ids.clone())
            .await
            .unwrap_err(),
        ModelErrorKind::Configuration
    );
    assert_eq!(
        service
            .install(second.clone(), fixture::context(&cancel))
            .unwrap(),
        2
    );
    write(root, serde_json::json!({"slots":[]}), &cancel);
    let disabled = server
        .clone()
        .refresh(context(), credential(), BTreeMap::new())
        .await
        .unwrap();
    assert!(disabled.config_ids.is_empty());
    assert_eq!(disabled.generation, 3);
    assert_eq!(
        server
            .clone()
            .refresh(context(), credential(), BTreeMap::new())
            .await
            .unwrap()
            .generation,
        3
    );
    let mut settings = fixture.settings(&["e"]);
    let model_api::RuntimeSettings::EmbeddingGemma2 { threads, .. } =
        &mut settings.slots[0].runtime
    else {
        panic!("GGUF embedding fixture")
    };
    *threads = std::num::NonZeroU32::new(3).unwrap();
    write(root, serde_json::to_value(settings).unwrap(), &cancel);
    assert_eq!(
        server
            .refresh(context(), credential(), second_ids)
            .await
            .unwrap()
            .generation,
        4
    );
    global.join().await.unwrap();
    service.shutdown(fixture::context(&cancel)).await.unwrap();
    service.join_shutdown().await.unwrap();
}

#[tokio::test]
async fn global_refresh_preserves_expired_and_cancelled_original_requests() {
    let cancel = Cancellation::default();
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::new(
        &request,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let client = model_api::ModelServiceClient::new(fixture::rpc_limits()).unwrap();
    for cancelled in [false, true] {
        let signal = Cancellation::default();
        let deadline = if cancelled {
            Instant::now() + Duration::from_secs(30)
        } else {
            Instant::now() - Duration::from_secs(1)
        };
        if cancelled {
            signal.cancel();
        }
        let original = SnapshotContext::new(deadline, signal);
        let error = client
            .refresh(
                &auth.runtime,
                &original,
                BTreeMap::new(),
                std::future::pending::<io::Result<tokio::io::DuplexStream>>(),
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.kind(),
            if cancelled {
                ModelErrorKind::Cancelled
            } else {
                ModelErrorKind::Deadline
            }
        );
    }
}
