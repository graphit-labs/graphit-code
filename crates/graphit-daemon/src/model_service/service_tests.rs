use super::*;
use crate::model_child as child_api;
use crate::model_service as model_api;
use crate::model_service::test_fixture as fixture;
use crate::model_transport as disconnect_api;
#[path = "../../tests/support/model_rpc.rs"]
mod rpc;
use crate::model_service::{CapturedModelInput, ModelSettings, TaskKind};
use fixture::{Fixture, context};
use std::time::{Duration, Instant};

fn assert_normalized(output: &EmbedOutput) {
    assert!(!output.vectors.is_empty());
    for vector in &output.vectors {
        assert_eq!(vector.len() as u64, output.dimensions);
        assert!(vector.iter().all(|x| x.is_finite()));
        let norm: f64 = vector.iter().map(|x| f64::from(*x).powi(2)).sum();
        assert!((norm - 1.0).abs() < 1e-4);
    }
}

#[test]
fn declared_configuration_matches_full_identity_and_reads_only_the_manifest() {
    let signal = Cancellation::default();
    let original = Arc::new(graphit_core::artifact_store::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        signal.clone(),
    ));
    for task in [TaskKind::Embedding, TaskKind::Rerank] {
        let fixture = Fixture::metadata(task);
        let full = fixture.config_with_original(&["a"], &original);
        let declared = || {
            ResolvedConfig::resolve_declared(
                fixture.settings(&["a"]),
                fixture::cpu_selection(),
                &original,
                &fixture::capture_policy(),
                graphit_core::artifact_store::SourceReadLimits::new(65536, 512, 65536).unwrap(),
                &fixture::manifest_limits(),
            )
            .unwrap()
        };
        let metadata = declared();
        assert_eq!(metadata.config_ids(), full.config_ids());
        assert_eq!(metadata.slots["a"].vector_id, full.slots["a"].vector_id);
        assert_eq!(metadata.slots["a"].device, full.slots["a"].device);
        // A declaration can exist before weights are installed or loaded.
        // This integrity-only fixture is never sent to a native inference API.
        std::fs::remove_file(fixture.directory.path().join("model.gguf")).unwrap();
        assert_eq!(declared().config_ids(), full.config_ids());
        let mut escaped = fixture.settings(&["a"]);
        escaped.slots[0].manifest_relative = "../model.json".into();
        assert_eq!(
            ResolvedConfig::resolve_declared(
                escaped,
                fixture::cpu_selection(),
                &original,
                &fixture::capture_policy(),
                graphit_core::artifact_store::SourceReadLimits::new(65536, 512, 65536).unwrap(),
                &fixture::manifest_limits(),
            )
            .err()
            .unwrap()
            .kind(),
            ModelErrorKind::Configuration
        );
        let mut extra = fixture.body.clone();
        let mut auxiliary = extra["artifacts"][0].clone();
        auxiliary["role"] = "unconsumed".into();
        auxiliary["path"] = "unconsumed.gguf".into();
        extra["artifacts"].as_array_mut().unwrap().push(auxiliary);
        std::fs::write(
            fixture.directory.path().join("model.json"),
            serde_json::to_vec(&extra).unwrap(),
        )
        .unwrap();
        assert_eq!(
            ResolvedConfig::resolve_declared(
                fixture.settings(&["a"]),
                fixture::cpu_selection(),
                &original,
                &fixture::capture_policy(),
                graphit_core::artifact_store::SourceReadLimits::new(65536, 512, 65536).unwrap(),
                &fixture::manifest_limits(),
            )
            .err()
            .unwrap()
            .kind(),
            ModelErrorKind::Configuration
        );
    }
    signal.cancel();
    assert_eq!(
        ResolvedConfig::resolve_declared(
            ModelSettings { slots: vec![] },
            fixture::cpu_selection(),
            &original,
            &fixture::capture_policy(),
            graphit_core::artifact_store::SourceReadLimits::new(65536, 512, 65536).unwrap(),
            &fixture::manifest_limits(),
        )
        .err()
        .unwrap()
        .kind(),
        ModelErrorKind::Cancelled
    );
}

#[tokio::test]
#[ignore = "requires pinned real BF16 GGUF; isolated native bootstrap"]
async fn declared_real_weights_tampering_is_integrity_error_before_any_output() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    use std::io::Write;
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let signal = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(180);
    let original = Arc::new(graphit_core::artifact_store::SnapshotContext::new(
        deadline,
        signal.clone(),
    ));
    let fixture = Fixture::actual(TaskKind::Embedding);
    let config = ResolvedConfig::resolve_declared(
        fixture.settings(&["a"]),
        fixture::cpu_selection(),
        &original,
        &fixture::capture_policy(),
        graphit_core::artifact_store::SourceReadLimits::new(65536, 512, 65536).unwrap(),
        &fixture::manifest_limits(),
    )
    .unwrap();
    let id = config.config_id("a").unwrap();
    // Corrupt only this private copy of the real pinned model. Configuration
    // remains declared; actual native staging must reject the complete digest.
    std::fs::OpenOptions::new()
        .write(true)
        .open(fixture.directory.path().join("model.gguf"))
        .unwrap()
        .write_all(&[0])
        .unwrap();
    let service = fixture::service(snapshot_parent.clone());
    let inference = || InferenceContext::new(&signal, deadline);
    service.install(config, inference()).unwrap();
    let error = service
        .embed("a", id, EmbedInput::Query, vec!["a".into()], inference())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ModelErrorKind::Integrity);
    assert!(std::error::Error::source(&error).is_some());
    let owner = service
        .admit("a", id, ModelTask::Embedding, inference())
        .unwrap()
        .0;
    assert!(owner.native.lock().await.is_none());
    drop(owner);
    service.shutdown(inference()).await.unwrap();
    println!("declared_real_model_pin_mismatch_rejected_before_output_and_joined=true");
}

#[path = "service_tests/project_generations.rs"]
mod project_generations;

#[tokio::test]
#[ignore = "requires pinned real BF16 GGUF; isolated native bootstrap"]
async fn equivalent_owners_lazy_distinct_and_retired_arc_lifetime() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let cancel = Cancellation::default();
    let fixture = Fixture::actual(TaskKind::Embedding);
    let config = fixture.config(&["a", "b"]);
    let id = config.config_id("a").unwrap();
    let service = fixture::service(snapshot_parent.clone());
    service.install(config, context(&cancel)).unwrap();
    let a = service
        .admit("a", id, ModelTask::Embedding, context(&cancel))
        .unwrap()
        .0;
    let b = service
        .admit("b", id, ModelTask::Embedding, context(&cancel))
        .unwrap()
        .0;
    assert!(Arc::ptr_eq(&a, &b));
    assert!(a.native.lock().await.is_none());
    let weak = Arc::downgrade(&a);
    let block = a.native.clone().lock_owned().await;
    let request = service.embed(
        "a",
        id,
        EmbedInput::Query,
        vec!["a".into()],
        context(&cancel),
    );
    tokio::pin!(request);
    tokio::select! { out=&mut request => panic!("request should await owner: {out:?}"),
    _=tokio::time::sleep(Duration::from_millis(5))=>{} }
    let distinct = Fixture::actual(TaskKind::Embedding);
    let config = distinct.config(&["a"]);
    let new_id = config.config_id("a").unwrap();
    assert_eq!(a.config.vector_id, config.slots["a"].vector_id);
    assert_ne!(id, new_id);
    let generation = service.install(config, context(&cancel)).unwrap();
    assert_eq!(generation, 2);
    let new = service
        .admit("a", new_id, ModelTask::Embedding, context(&cancel))
        .unwrap()
        .0;
    assert!(!Arc::ptr_eq(&a, &new));
    assert!(new.native.lock().await.is_none());
    drop(block);
    let old = request.await.unwrap();
    assert_eq!((old.config_id, old.generation), (id, 1));
    assert!(a.native.lock().await.is_some());
    drop(a);
    drop(b);
    // The request future is still pinned in this scope, but execute's owned
    // native worker and retired Owner have both reached terminal + join.
    assert!(weak.upgrade().is_none());
    let out = service
        .embed(
            "a",
            new_id,
            EmbedInput::Document,
            vec!["b".into()],
            context(&cancel),
        )
        .await
        .unwrap();
    assert_eq!(out.generation, 2);
    assert_normalized(&out);
    service.shutdown(context(&cancel)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires pinned real BF16 GGUF; isolated native bootstrap"]
async fn queued_cancel_deadline_and_retirement_do_not_interrupt_b() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let cancel = Cancellation::default();
    let fixture = Fixture::actual(TaskKind::Embedding);
    let service = fixture::service(snapshot_parent.clone());
    let config = fixture.config(&["a", "b"]);
    let id = config.config_id("a").unwrap();
    service.install(config, context(&cancel)).unwrap();
    let a = service
        .admit("a", id, ModelTask::Embedding, context(&cancel))
        .unwrap()
        .0;
    let block = a.native.clone().lock_owned().await;
    let cancelled = Cancellation::default();
    let request = service.embed(
        "a",
        id,
        EmbedInput::Query,
        vec!["a".into()],
        context(&cancelled),
    );
    tokio::pin!(request);
    tokio::select! {out=&mut request=>panic!("unexpected result: {out:?}"),_=tokio::time::sleep(Duration::from_millis(5))=>{}}
    let b_cancel = Cancellation::default();
    let b = service.embed(
        "b",
        id,
        EmbedInput::Query,
        vec!["a".into()],
        context(&b_cancel),
    );
    tokio::pin!(b);
    tokio::select! {
        out = &mut b => panic!("B should await the shared owner: {out:?}"),
        _ = tokio::time::sleep(Duration::from_millis(5)) => {}
    }
    cancelled.cancel();
    assert_eq!(request.await.unwrap_err().kind(), ModelErrorKind::Cancelled);
    let expired = service
        .embed(
            "a",
            id,
            EmbedInput::Query,
            vec!["a".into()],
            InferenceContext::new(&cancel, Instant::now() + Duration::from_millis(5)),
        )
        .await
        .unwrap_err();
    assert_eq!(expired.kind(), ModelErrorKind::Deadline);
    assert!(a.native.try_lock().is_err());
    assert!(service.0.state.lock().unwrap().workers.is_empty());
    service
        .install(fixture.config(&["b"]), context(&cancel))
        .unwrap();
    drop(block);
    let out = b.await.unwrap();
    assert!(!b_cancel.is_cancelled());
    assert_eq!((out.config_id, out.generation), (id, 1));
    assert_normalized(&out);
    let next = service
        .embed(
            "b",
            id,
            EmbedInput::Query,
            vec!["a".into()],
            context(&b_cancel),
        )
        .await
        .unwrap();
    assert_eq!(next.generation, 2);
    assert_normalized(&next);
    service.shutdown(context(&cancel)).await.unwrap();
}

#[tokio::test]
async fn refresh_only_reaps_completed_retirements_and_preserves_failure_for_drain() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let fixture = Fixture::metadata(TaskKind::Embedding);
    let cancel = Cancellation::default();
    let service = fixture::service(snapshot_parent.clone());
    let initial = fixture.config(&["a"]);
    service.install(initial.clone(), context(&cancel)).unwrap();
    for index in 0..8 {
        let mut settings = fixture.settings(&["a"]);
        let model_api::RuntimeSettings::EmbeddingGemma2 { threads, .. } =
            &mut settings.slots[0].runtime
        else {
            panic!("embedding metadata fixture");
        };
        *threads = std::num::NonZeroU32::new(3 + index % 2).unwrap();
        service
            .install(fixture.resolve(settings).unwrap(), context(&cancel))
            .unwrap();
        let done = {
            let state = service.0.state.lock().unwrap();
            assert_eq!(state.workers.len(), 1);
            state.workers[0].done.clone()
        };
        done.cancelled().await;
    }
    service.install(initial.clone(), context(&cancel)).unwrap();
    let done = service.0.state.lock().unwrap().workers[0].done.clone();
    done.cancelled().await;
    // Even equivalent-map refresh must reap, without an inference request.
    service.install(initial, context(&cancel)).unwrap();
    assert!(service.0.state.lock().unwrap().workers.is_empty());
    let failure = Arc::new(Worker {
        cancellation: Cancellation::default(),
        done: Cancellation::default(),
        join: Mutex::new(Some(tokio::spawn(async {
            Err(ModelServiceError::new(ModelErrorKind::Worker))
        }))),
    });
    service.0.state.lock().unwrap().workers.push(failure);
    service
        .install(fixture.config(&["a"]), context(&cancel))
        .unwrap();
    assert_eq!(service.0.state.lock().unwrap().workers.len(), 1);
    assert_eq!(
        service.shutdown(context(&cancel)).await.unwrap_err().kind(),
        ModelErrorKind::Worker
    );
}

#[tokio::test]
async fn idle_policy_updates_without_identity_generation_or_native_loading() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let snapshot =
        graphit_core::config::ConfigSnapshot::new(&graphit_core::brand::Brand::default(), [])
            .unwrap();
    assert_eq!(
        ModelSettings::idle_timeout(&snapshot).unwrap(),
        Duration::from_secs(300)
    );
    assert!(
        ResolvedConfig::empty()
            .with_idle_timeout(Duration::ZERO)
            .is_err()
    );
    assert!(
        ResolvedConfig::empty()
            .with_idle_timeout(Duration::MAX)
            .is_err()
    );
    let fixture = Fixture::metadata(TaskKind::Embedding);
    let signal = Cancellation::default();
    let original = Arc::new(graphit_core::artifact_store::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        signal.clone(),
    ));
    let config = fixture.config_with_original(&["a"], &original);
    let ids = config.config_ids();
    let service = fixture::service(snapshot_parent.clone());
    assert_eq!(
        service.install(config.clone(), context(&signal)).unwrap(),
        1
    );
    let owner = service
        .admit("a", ids["a"], ModelTask::Embedding, context(&signal))
        .unwrap()
        .0;
    let vector_id = owner.config.vector_id;
    let changed = config.with_idle_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(changed.config_ids(), ids);
    assert_eq!(service.install(changed, context(&signal)).unwrap(), 1);
    let after = service
        .admit("a", ids["a"], ModelTask::Embedding, context(&signal))
        .unwrap()
        .0;
    assert!(Arc::ptr_eq(&owner, &after));
    assert_eq!(after.config.vector_id, vector_id);
    assert_eq!(*service.0.idle_timeout.borrow(), Duration::from_secs(1));
    assert!(owner.native.lock().await.is_none());
    assert!(!owner.idle_started.load(Ordering::Acquire));
    assert!(service.0.state.lock().unwrap().workers.is_empty());
    service.shutdown(context(&signal)).await.unwrap();
}

// Hold an actual installed model's owned operation at a controlled service
// phase. This gates ownership, without fabricating any native model output.
fn held_owner_operation(
    service: ModelService,
    owner: Arc<Owner>,
    signal: Cancellation,
    deadline: Instant,
) -> (
    oneshot::Sender<()>,
    oneshot::Receiver<()>,
    tokio::task::JoinHandle<ModelResult<()>>,
) {
    let (release, wait) = oneshot::channel();
    let (entered, entry) = oneshot::channel();
    let task = tokio::spawn(async move {
        service
            .execute_owned(
                owner,
                InferenceContext::new(&signal, deadline),
                std::io::sink(),
                move |owner, native, _, _, _| async move {
                    let activity = ActiveOwner::new(&owner);
                    drop(native);
                    let _ = entered.send(());
                    let result = wait
                        .await
                        .map_err(|cause| ModelServiceError::caused(ModelErrorKind::Worker, cause));
                    drop(activity);
                    tokio::task::spawn_blocking(move || drop(owner))
                        .await
                        .map_err(|cause| {
                            ModelServiceError::caused(ModelErrorKind::Worker, cause)
                        })?;
                    result
                },
            )
            .await
    });
    (release, entry, task)
}
async fn actual_idle_ack(receiver: &mut watch::Receiver<u64>) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while *receiver.borrow_and_update() == 0 {
            receiver.changed().await.unwrap();
        }
    })
    .await
    .expect("real model unload and native join ACK");
}

#[tokio::test]
#[ignore = "requires both pinned real GGUF artifacts; optional GRAPHIT_TEST_MODELS_DEVICE=cuda"]
async fn actual_gguf_idle_release_reload_and_inflight_peer_independence() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let signal = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(600);
    let original = Arc::new(graphit_core::artifact_store::SnapshotContext::new(
        deadline,
        signal.clone(),
    ));
    let embedding = Fixture::actual(TaskKind::Embedding);
    let rerank = Fixture::actual(TaskKind::Rerank);
    let mut settings = embedding.settings(&["e"]);
    settings.slots.extend(rerank.settings(&["r"]).slots);
    let mut selection = fixture::cpu_selection();
    let (device, provider) = match std::env::var("GRAPHIT_TEST_MODELS_DEVICE").as_deref() {
        Err(_) | Ok("cpu") => (
            graphit_auth::profile::LocalDevice::Cpu,
            graphit_ai::local_execution::LocalProvider::CPU,
        ),
        Ok("cuda") => (
            graphit_auth::profile::LocalDevice::Cuda,
            graphit_ai::local_execution::LocalProvider::CUDA,
        ),
        Ok(_) => panic!("unsupported test device selector"),
    };
    selection.embedding.as_mut().unwrap().device = device;
    selection.rerank.as_mut().unwrap().device = device;
    let inputs = settings
        .slots
        .iter()
        .map(|slot| {
            CapturedModelInput::capture(
                &slot.name,
                &slot.bundle,
                &fixture::capture_policy(),
                graphit_core::artifact_store::SourceReadLimits::new(65536, 512, 65536).unwrap(),
                original.clone(),
            )
        })
        .collect::<ModelResult<Vec<_>>>()
        .unwrap();
    let config = ResolvedConfig::resolve(
        settings,
        selection,
        inputs,
        &original,
        &fixture::manifest_limits(),
    )
    .unwrap();
    let ids = config.config_ids();
    let service = fixture::service(snapshot_parent.clone());
    let inference = || InferenceContext::new(&signal, deadline);
    assert_eq!(service.install(config.clone(), inference()).unwrap(), 1);
    let e = service
        .admit("e", ids["e"], ModelTask::Embedding, inference())
        .unwrap()
        .0;
    let r = service
        .admit("r", ids["r"], ModelTask::Rerank, inference())
        .unwrap()
        .0;
    assert!(e.native.lock().await.is_none() && r.native.lock().await.is_none());
    let vector_id = e.config.vector_id;
    let mut e_idle = e.idle_unloaded.subscribe();
    let mut r_idle = r.idle_unloaded.subscribe();
    let output = service
        .embed(
            "e",
            ids["e"],
            EmbedInput::Query,
            vec!["What is the capital of France?".into()],
            inference(),
        )
        .await
        .unwrap();
    assert_normalized(&output);
    assert!(output.device.matches_request(provider, 0));
    let e_weak = match e.native.lock().await.as_ref().unwrap() {
        NativeClient::Gemma(worker) => Arc::downgrade(worker),
        NativeClient::Bge(_) => panic!("embedding owner"),
    };
    let (release_e, entry, held_e) =
        held_owner_operation(service.clone(), e.clone(), signal.clone(), deadline);
    entry.await.unwrap();
    assert_eq!(
        service
            .install(
                config
                    .clone()
                    .with_idle_timeout(Duration::from_secs(1))
                    .unwrap(),
                inference()
            )
            .unwrap(),
        1
    );
    let query = "What is the capital of France?";
    let docs = || {
        vec![
            "Paris is the capital of France.".into(),
            "Bananas grow in tropical climates.".into(),
        ]
    };
    let ranking = service
        .rerank("r", ids["r"], query.into(), docs(), inference())
        .await
        .unwrap();
    assert!(ranking.scores[0] > ranking.scores[1]);
    assert!(ranking.device.matches_request(provider, 0));
    assert!(e_weak.upgrade().is_some());
    let r_weak = match r.native.lock().await.as_ref().unwrap() {
        NativeClient::Bge(worker) => Arc::downgrade(worker),
        NativeClient::Gemma(_) => panic!("rerank owner"),
    };
    let (release_r, entry, held_r) =
        held_owner_operation(service.clone(), r.clone(), signal.clone(), deadline);
    entry.await.unwrap();
    release_e.send(()).unwrap();
    held_e.await.unwrap().unwrap();
    actual_idle_ack(&mut e_idle).await;
    assert!(e.native.lock().await.is_none());
    assert!(e_weak.upgrade().is_none());
    assert!(r.native.lock().await.is_some() && r_weak.upgrade().is_some());
    println!("actual_idle_embedding_unloaded_peer_active=true");
    release_r.send(()).unwrap();
    held_r.await.unwrap().unwrap();
    actual_idle_ack(&mut r_idle).await;
    assert!(r.native.lock().await.is_none() && r_weak.upgrade().is_none());
    println!("actual_idle_both_models_quiesced=true");
    assert_eq!(service.install(config, inference()).unwrap(), 1);
    let reloaded = service
        .embed(
            "e",
            ids["e"],
            EmbedInput::Query,
            vec!["What is the capital of France?".into()],
            inference(),
        )
        .await
        .unwrap();
    assert_normalized(&reloaded);
    assert_eq!((reloaded.config_id, reloaded.generation), (ids["e"], 1));
    assert_eq!(e.config.vector_id, vector_id);
    assert!(reloaded.device.matches_request(provider, 0));
    let e_reloaded = match e.native.lock().await.as_ref().unwrap() {
        NativeClient::Gemma(worker) => Arc::downgrade(worker),
        NativeClient::Bge(_) => panic!("embedding owner"),
    };
    let reloaded = service
        .rerank("r", ids["r"], query.into(), docs(), inference())
        .await
        .unwrap();
    assert_eq!((reloaded.config_id, reloaded.generation), (ids["r"], 1));
    assert!(reloaded.scores[0] > reloaded.scores[1]);
    assert!(reloaded.device.matches_request(provider, 0));
    let r_reloaded = match r.native.lock().await.as_ref().unwrap() {
        NativeClient::Bge(worker) => Arc::downgrade(worker),
        NativeClient::Gemma(_) => panic!("rerank owner"),
    };
    drop(e);
    drop(r);
    service.shutdown(inference()).await.unwrap();
    assert!(e_reloaded.upgrade().is_none() && r_reloaded.upgrade().is_none());
    println!("actual_idle_reload_same_identity_generation_device_and_joined_shutdown=true");
}

#[tokio::test]
#[ignore = "requires pinned real BF16 GGUF; isolated native bootstrap"]
async fn actual_idle_cleanup_deadline_rearms_after_real_join_and_preserves_failure() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let signal = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(600);
    let original = Arc::new(graphit_core::artifact_store::SnapshotContext::new(
        deadline,
        signal.clone(),
    ));
    let fixture = Fixture::actual(TaskKind::Embedding);
    let config = fixture
        .config_with_original(&["e"], &original)
        .with_idle_timeout(Duration::from_secs(1))
        .unwrap();
    let id = config.config_id("e").unwrap();
    let mut service = fixture::service(snapshot_parent.clone());
    // Inject an already expired cleanup context. Public configuration still
    // rejects zero; shutdown_retained must nevertheless perform the real join.
    Arc::get_mut(&mut service.0).unwrap().shutdown_timeout = Duration::ZERO;
    let inference = || InferenceContext::new(&signal, deadline);
    service.install(config, inference()).unwrap();
    let owner = service
        .admit("e", id, ModelTask::Embedding, inference())
        .unwrap()
        .0;
    let vector_id = owner.config.vector_id;
    let mut unloaded = owner.idle_unloaded.subscribe();
    for iteration in 0..2 {
        let output = service
            .embed(
                "e",
                id,
                EmbedInput::Query,
                vec!["What is the capital of France?".into()],
                inference(),
            )
            .await
            .unwrap();
        assert_normalized(&output);
        assert_eq!((output.config_id, output.generation), (id, 1));
        assert_eq!(owner.config.vector_id, vector_id);
        let weak = match owner.native.lock().await.as_ref().unwrap() {
            NativeClient::Gemma(worker) => Arc::downgrade(worker),
            NativeClient::Bge(_) => panic!("embedding owner"),
        };
        tokio::time::timeout(Duration::from_secs(30), unloaded.changed())
            .await
            .expect("cleanup must join even with an expired context")
            .unwrap();
        assert_eq!(*unloaded.borrow_and_update(), iteration + 1);
        assert!(owner.native.lock().await.is_none());
        assert!(weak.upgrade().is_none());
        assert!(!owner.idle_started.load(Ordering::Acquire));
    }
    drop(owner);
    let error = service.shutdown(inference()).await.unwrap_err();
    assert_eq!(error.kind(), ModelErrorKind::Worker);
    let failure = service
        .0
        .state
        .lock()
        .unwrap()
        .drain_result
        .clone()
        .unwrap();
    assert_eq!(failure.unwrap_err().kind(), ModelErrorKind::Deadline);
    println!("actual_idle_cleanup_error_rearmed_twice_after_join_and_retained_for_drain=true");
}

#[tokio::test]
async fn cancelled_owned_worker_remains_joined_before_shutdown_returns() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    for cancel_shutdown in [false, true] {
        let cancel = Cancellation::default();
        let fixture = Fixture::metadata(TaskKind::Embedding);
        let service = fixture::service(snapshot_parent.clone());
        let config = fixture.config(&["a"]);
        let id = config.config_id("a").unwrap();
        service.install(config, context(&cancel)).unwrap();
        let owner = service
            .admit("a", id, ModelTask::Embedding, context(&cancel))
            .unwrap()
            .0;
        let weak = Arc::downgrade(&owner);
        let (entered, phase) = oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let active = Cancellation::default();
        let work = service.execute_owned(
            owner,
            context(&active),
            std::io::sink(),
            move |owner, native, _, cancellation, deadline| async move {
                let _retained = (owner, native);
                entered.send(()).unwrap();
                tokio::task::spawn_blocking(move || wait.recv().unwrap())
                    .await
                    .unwrap();
                InferenceContext::new(&cancellation, deadline).check()?;
                Ok(())
            },
        );
        tokio::pin!(work);
        tokio::select! {out=&mut work=>panic!("unexpected: {out:?}"),out=phase=>out.unwrap()}
        let worker = service.0.state.lock().unwrap().workers[0].clone();
        active.cancel();
        assert_eq!(work.await.unwrap_err().kind(), ModelErrorKind::Cancelled);
        assert!(worker.cancellation.is_cancelled());
        assert!(!worker.done.is_cancelled());
        let shutdown_cancel = Cancellation::default();
        let shutdown_budget = if cancel_shutdown {
            Duration::from_secs(30)
        } else {
            Duration::from_millis(20)
        };
        let drain = service.shutdown(InferenceContext::new(
            &shutdown_cancel,
            Instant::now() + shutdown_budget,
        ));
        tokio::pin!(drain);
        if cancel_shutdown {
            assert!(futures::poll!(&mut drain).is_pending());
            shutdown_cancel.cancel();
        }
        assert_eq!(
            drain.await.unwrap_err().kind(),
            if cancel_shutdown {
                ModelErrorKind::Cancelled
            } else {
                ModelErrorKind::Deadline
            }
        );
        let joined = service.join_shutdown();
        tokio::pin!(joined);
        assert!(futures::poll!(&mut joined).is_pending());
        assert!(weak.upgrade().is_some());
        release.send(()).unwrap();
        joined.await.unwrap();
        assert!(worker.done.is_cancelled());
        assert!(worker.join.lock().unwrap().is_none());
        assert!(weak.upgrade().is_none());
        assert!(service.0.drained.is_cancelled());
    }
}

#[tokio::test]
#[ignore = "requires pinned real BF16 GGUF; isolated native bootstrap"]
async fn real_remote_eof_drops_queued_handler_and_b_connection_survives() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let _sdk = graphit_ai::native_embedding::NativeSdkRoot::new().unwrap();
    let cancel = Cancellation::default();
    let fixture = Fixture::actual(TaskKind::Embedding);
    let service = fixture::service(snapshot_parent.clone());
    let config = fixture.config(&["a", "b"]);
    let id = config.config_id("a").unwrap();
    service.install(config, context(&cancel)).unwrap();
    let owner = service
        .admit("a", id, ModelTask::Embedding, context(&cancel))
        .unwrap()
        .0;
    let block = owner.native.clone().lock_owned().await;
    let retained = Arc::strong_count(&owner);
    let secret =
        Arc::new(model_api::RuntimeSecret::new(zeroize::Zeroizing::new([b'a'; 64])).unwrap());
    let (address, connection, server) = rpc::server(service.clone(), secret, fixture::rpc_limits());
    {
        let stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let tarpc::client::NewClient { client, dispatch } = model_api::ModelRpcClient::new(
            tarpc::client::Config::default(),
            model_api::client_transport(stream, fixture::rpc_limits()).unwrap(),
        );
        let call = client.embed(
            tarpc::context::current(),
            model_api::RuntimeCredential::from_secret(&[b'a'; 64]).unwrap(),
            "a".into(),
            id,
            EmbedInput::Query,
            vec!["a".into()],
        );
        tokio::pin!(call, dispatch);
        tokio::select! {out=&mut call=>panic!("queued request ended: {out:?}"),out=&mut dispatch=>panic!("dispatch ended: {out:?}"),_=tokio::time::sleep(Duration::from_millis(10))=>{}}
        assert!(Arc::strong_count(&owner) > retained);
        assert!(service.0.state.lock().unwrap().workers.is_empty());
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    assert!(connection.is_cancelled());
    assert_eq!(Arc::strong_count(&owner), retained);
    drop(block);
    let out = service
        .embed(
            "b",
            id,
            EmbedInput::Query,
            vec!["b".into()],
            context(&cancel),
        )
        .await
        .unwrap();
    assert_normalized(&out);
    service.shutdown(context(&cancel)).await.unwrap();
}

#[tokio::test]
async fn first_invalid_payload_do_not_load_or_start_native_workers() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let cancel = Cancellation::default();
    for task in [TaskKind::Embedding, TaskKind::Rerank] {
        let mut fixture = Fixture::metadata(task);
        fixture.body["artifacts"][0]["minimum_bytes"] = serde_json::json!(
            std::fs::metadata(fixture.directory.path().join("model.gguf"))
                .unwrap()
                .len()
        );
        let config = fixture.config(&["a"]);
        let id = config.config_id("a").unwrap();
        let service = fixture::service(snapshot_parent.clone());
        service.install(config, context(&cancel)).unwrap();
        let task = task.native();
        let owner = service.admit("a", id, task, context(&cancel)).unwrap().0;
        std::fs::remove_file(fixture.directory.path().join("model.gguf")).unwrap();
        let error = if task == ModelTask::Embedding {
            service
                .embed(
                    "a",
                    id,
                    EmbedInput::Query,
                    vec!["a".repeat(65537)],
                    context(&cancel),
                )
                .await
                .unwrap_err()
        } else {
            service
                .rerank(
                    "a",
                    id,
                    "a".repeat(65537),
                    vec!["a".into()],
                    context(&cancel),
                )
                .await
                .unwrap_err()
        };
        assert_eq!(error.kind(), ModelErrorKind::Limit);
        assert!(owner.native.lock().await.is_none());
        assert!(service.0.state.lock().unwrap().workers.is_empty());
        let error = if task == ModelTask::Embedding {
            service
                .embed(
                    "a",
                    id,
                    EmbedInput::Query,
                    vec!["a".into()],
                    context(&cancel),
                )
                .await
                .unwrap_err()
        } else {
            service
                .rerank("a", id, "a".into(), vec!["a".into()], context(&cancel))
                .await
                .unwrap_err()
        };
        assert_eq!(error.kind(), ModelErrorKind::Artifact);
        assert!(owner.native.lock().await.is_none());
        std::fs::write(fixture.directory.path().join("model.gguf"), b"x").unwrap();
        let error = if task == ModelTask::Embedding {
            service
                .embed(
                    "a",
                    id,
                    EmbedInput::Query,
                    vec!["a".into()],
                    context(&cancel),
                )
                .await
                .unwrap_err()
        } else {
            service
                .rerank("a", id, "a".into(), vec!["a".into()], context(&cancel))
                .await
                .unwrap_err()
        };
        assert_eq!(error.kind(), ModelErrorKind::Artifact);
        assert!(owner.native.lock().await.is_none());
        service.shutdown(context(&cancel)).await.unwrap();
    }
}

#[tokio::test]
async fn deadline_observation_admission_and_expired_drop() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let fixture = Fixture::metadata(TaskKind::Embedding);
    for expired_drop in [false, true] {
        let cancel = Cancellation::default();
        let service = fixture::service(snapshot_parent.clone());
        let config = fixture.config(&["a"]);
        let id = config.config_id("a").unwrap();
        service.install(config, context(&cancel)).unwrap();
        let owner = service
            .admit("a", id, ModelTask::Embedding, context(&cancel))
            .unwrap()
            .0;
        let blocked = service
            .0
            .capacity
            .clone()
            .acquire_many_owned(4)
            .await
            .unwrap();
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut diagnostics = Vec::new();
        let mut request = Box::pin(service.execute_owned(
            owner,
            InferenceContext::new(&cancel, deadline),
            &mut diagnostics,
            |_, _, _, _, _| async { panic!("admission must not run") },
        ));
        if expired_drop {
            std::future::poll_fn(|cx| {
                assert!(request.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            tokio::time::sleep_until(deadline.into()).await;
        } else {
            let result: ModelResult<()> = request.as_mut().await;
            assert_eq!(result.unwrap_err().kind(), ModelErrorKind::Deadline);
        }
        drop(request);
        assert_eq!(
            diagnostics,
            b"model service deadline: operation=embedding phase=admission\n"
        );
        assert!(service.0.state.lock().unwrap().workers.is_empty());
        drop(blocked);
        service.join_shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn deadline_observation_execution_retains_drain_and_silences_unexpired_calls() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let fixture = Fixture::metadata(TaskKind::Embedding);
    let cancel = Cancellation::default();
    let service = fixture::service(snapshot_parent.clone());
    let config = fixture.config(&["a"]);
    let id = config.config_id("a").unwrap();
    service.install(config, context(&cancel)).unwrap();
    let owner = service
        .admit("a", id, ModelTask::Embedding, context(&cancel))
        .unwrap()
        .0;
    let (entered, started) = oneshot::channel();
    let (release, wait) = oneshot::channel();
    let mut diagnostics = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut request = Box::pin(service.execute_owned(
        owner.clone(),
        InferenceContext::new(&cancel, deadline),
        &mut diagnostics,
        move |owner, native, _, _, _| async move {
            let _retained = (owner, native);
            entered.send(()).unwrap();
            wait.await.unwrap();
            Ok(())
        },
    ));
    tokio::select! {
        result = &mut request => panic!("execution returned before barrier: {result:?}"),
        result = started => result.unwrap(),
    }
    let worker = service.0.state.lock().unwrap().workers[0].clone();
    assert_eq!(
        request.as_mut().await.unwrap_err().kind(),
        ModelErrorKind::Deadline
    );
    drop(request);
    assert_eq!(
        diagnostics,
        b"model service deadline: operation=embedding phase=execution\n"
    );
    assert!(worker.cancellation.is_cancelled());
    assert!(!worker.done.is_cancelled());
    let mut drain = Box::pin(service.join_shutdown());
    std::future::poll_fn(|cx| {
        assert!(drain.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    release.send(()).unwrap();
    drain.await.unwrap();
    assert!(worker.done.is_cancelled());
    assert!(worker.join.lock().unwrap().is_none());

    let service = fixture::service(snapshot_parent.clone());
    service
        .install(fixture.config(&["a"]), context(&cancel))
        .unwrap();
    let owner = service
        .admit("a", id, ModelTask::Embedding, context(&cancel))
        .unwrap()
        .0;
    diagnostics.clear();
    service
        .execute_owned(
            owner.clone(),
            context(&cancel),
            &mut diagnostics,
            |_, _, _, _, _| async { Ok(()) },
        )
        .await
        .unwrap();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let result: ModelResult<()> = service
        .execute_owned(
            owner,
            context(&cancelled),
            &mut diagnostics,
            |_, _, _, _, _| async { panic!("cancelled admission must not run") },
        )
        .await;
    assert_eq!(result.unwrap_err().kind(), ModelErrorKind::Cancelled);
    assert!(diagnostics.is_empty());
    service.join_shutdown().await.unwrap();
}
