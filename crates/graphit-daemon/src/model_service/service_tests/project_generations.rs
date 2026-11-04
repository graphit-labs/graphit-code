//! PROJECT consumers retain admitted model owners while the selected config changes.
#![cfg(target_os = "linux")]

use super::{fixture, rpc};
use crate::model_service as model_api;
use crate::model_service::{
    CapturedModelInput, EmbedInput, ExpectedModel, MemoryEmbeddingCause, MemoryEmbeddingIndexRead,
    MemoryEmbeddingModel, ModelErrorKind, ModelResult, ModelService, ModelServiceClient,
    ResolvedConfig, RuntimeSecret,
};
use graphit_ai::{model_manifest::ModelTask, native_embedding::NativeSdkRoot};
use graphit_auth::{
    ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ServicePurpose,
};
use graphit_core::{
    artifact_store::SourceReadLimits, file_io::SnapshotContext, file_lock::Cancellation,
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{
        self as memory_api, CreateMemory, MemoryHybridSearchOptions, MemoryHybridSearchRead,
        MemoryListFilter, NativeMemoryStore,
    },
};
use std::{
    fmt::Debug,
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpStream, task::JoinHandle};
use zeroize::Zeroizing;

#[allow(
    dead_code,
    reason = "shared physical PROJECT fixture also serves other consumer cases"
)]
#[path = "../../../tests/support/memory_model.rs"]
mod project;

fn memory(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: key.into(),
        body: "Paris is the capital of France.".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: false,
            important: false,
        },
        tags: vec![],
        references: None,
    }
}

fn secret(project: &project::Fixture, original: &Arc<SnapshotContext>) -> Arc<RuntimeSecret> {
    let key = project.runtime().check(original).unwrap();
    Arc::new(RuntimeSecret::new(Zeroizing::new(*key.secret(original).unwrap())).unwrap())
}

async fn join_rpc(connection: Cancellation, server: JoinHandle<ModelResult<()>>) {
    connection.cancel();
    // serve_connection returns Cancelled if this token wins, or Ok if the
    // completed transport reaches EOF first. Transport errors still fail.
    if let Err(error) = server.await.expect("RPC task must join") {
        assert_eq!(error.kind(), ModelErrorKind::Cancelled);
    }
}

// The permit is acquired after the real slot/owner admission and before the
// existing native mutex. No new production hook or fabricated admission is used.
async fn admitted<F: Future>(
    mut request: Pin<&mut F>,
    service: &ModelService,
    remaining: usize,
    deadline: Instant,
) where
    F::Output: Debug,
{
    tokio::time::timeout_at(deadline.into(), async {
        loop {
            tokio::select! {
                out = request.as_mut() => panic!("request completed before the held native owner: {out:?}"),
                () = tokio::task::yield_now() => {}
            }
            if service.0.capacity.available_permits() == remaining {
                return;
            }
        }
    }).await.expect("real RPC must acquire the existing service permit");
}

#[tokio::test]
async fn configuration_change_preserves_admitted_owner_and_unchanged_sibling() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let cancel = Cancellation::default();
    let context = fixture::context(&cancel);
    let fixture = fixture::Fixture::metadata(model_api::TaskKind::Embedding);
    let initial = fixture.config(&["a", "b"]);
    let a_id = initial.config_id("a").unwrap();
    let b_id = initial.config_id("b").unwrap();
    let service = fixture::service(snapshot_parent);
    assert_eq!(service.install(initial, context).unwrap(), 1);
    let (old_a, generation) = service
        .admit("a", a_id, ModelTask::Embedding, context)
        .unwrap();
    let (old_b, _) = service
        .admit("b", b_id, ModelTask::Embedding, context)
        .unwrap();
    assert_eq!(generation, 1);

    let (entered, mut phase) = tokio::sync::oneshot::channel();
    let (release, wait) = tokio::sync::oneshot::channel();
    let work = service.execute_owned(
        old_a.clone(),
        context,
        std::io::sink(),
        move |owner, native, _, cancellation, deadline| async move {
            let _native = native;
            entered.send(()).unwrap();
            wait.await.unwrap();
            graphit_ai::context::InferenceContext::new(&cancellation, deadline).check()?;
            Ok(owner.config.id)
        },
    );
    tokio::pin!(work);
    tokio::select! {
        outcome = &mut work => panic!("operation completed before release: {outcome:?}"),
        phase = &mut phase => phase.unwrap(),
    }

    let mut settings = fixture.settings(&["a", "b"]);
    let model_api::RuntimeSettings::EmbeddingGemma2 { threads, .. } =
        &mut settings.slots[0].runtime
    else {
        panic!("embedding fixture")
    };
    *threads = std::num::NonZeroU32::new(3).unwrap();
    let next = fixture.resolve(settings).unwrap();
    let next_id = next.config_id("a").unwrap();
    assert_ne!(next_id, a_id);
    assert_eq!(next.config_id("b"), Some(b_id));
    assert_eq!(service.install(next, context).unwrap(), 2);
    let (new_a, generation) = service
        .admit("a", next_id, ModelTask::Embedding, context)
        .unwrap();
    let (same_b, _) = service
        .admit("b", b_id, ModelTask::Embedding, context)
        .unwrap();
    assert_eq!(generation, 2);
    assert!(!Arc::ptr_eq(&old_a, &new_a));
    assert!(Arc::ptr_eq(&old_b, &same_b));
    assert_eq!(
        service
            .admit("a", a_id, ModelTask::Embedding, context)
            .err()
            .unwrap()
            .kind(),
        ModelErrorKind::ConfigChanged
    );
    release.send(()).unwrap();
    assert_eq!(work.await.unwrap(), a_id);
    service.shutdown(context).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the pinned BF16 GGUF in GRAPHIT_EMBEDDINGGEMMA2_MODEL"]
async fn project_profile_change_keeps_borrowed_generation_and_sibling_independent() {
    let (_snapshot_root, snapshot_parent) = fixture::snapshot_parent();
    let setup_cancel = Cancellation::default();
    let end = Instant::now() + Duration::from_secs(120);
    let setup = project::request_until(&setup_cancel, end);
    let a = project::Fixture::new_for(&setup, "main", "01D39ZY06FGSCTVN4T2V9PKHFZ");
    let b = project::Fixture::new_for(&setup, "main", "01D39ZY06FGSCTVN4T2V9PKHG0");
    assert_ne!(a.project.runtime_root(), b.project.runtime_root());
    let (issuer_a, _auth_a) = a.issuer(&setup);
    let (issuer_b, _auth_b) = b.issuer(&setup);
    let sdk = NativeSdkRoot::new().unwrap();

    let old_cancel = Cancellation::default();
    let old_ctx = project::request_until(&old_cancel, end);
    let key_a = a.runtime().check(old_ctx.snapshot()).unwrap();
    let caller_a = issuer_a
        .authenticate_runtime(
            std::str::from_utf8(key_a.secret(old_ctx.snapshot()).unwrap()).unwrap(),
            &old_ctx,
        )
        .unwrap();
    let authority_a = ProjectRecordAuthority::new(&issuer_a);
    let read_a = authority_a
        .admit_authenticated_read(&caller_a, &old_ctx, a.scope(), ProjectReadModule::Memory)
        .unwrap();
    let write_a = authority_a
        .admit_authenticated_write(
            &caller_a,
            &old_ctx,
            a.scope(),
            ProjectWriteOperation::Memory,
        )
        .unwrap();
    let mut selection_a = issuer_a
        .capture_service(&caller_a, ServicePurpose::Embedding)
        .unwrap();
    let lease_a = selection_a.local().unwrap();

    let b_cancel = Cancellation::default();
    let b_ctx = project::request_until(&b_cancel, end);
    let key_b = b.runtime().check(b_ctx.snapshot()).unwrap();
    let caller_b = issuer_b
        .authenticate_runtime(
            std::str::from_utf8(key_b.secret(b_ctx.snapshot()).unwrap()).unwrap(),
            &b_ctx,
        )
        .unwrap();
    let authority_b = ProjectRecordAuthority::new(&issuer_b);
    let read_b = authority_b
        .admit_authenticated_read(&caller_b, &b_ctx, b.scope(), ProjectReadModule::Memory)
        .unwrap();
    let write_b = authority_b
        .admit_authenticated_write(&caller_b, &b_ctx, b.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    let mut selection_b = issuer_b
        .capture_service(&caller_b, ServicePurpose::Embedding)
        .unwrap();
    let lease_b = selection_b.local().unwrap();
    let row_a = NativeMemoryStore::create(
        old_ctx.snapshot(),
        &write_a,
        memory("project-a"),
        project::write_limits(),
    )
    .await
    .unwrap();
    let row_b = NativeMemoryStore::create(
        b_ctx.snapshot(),
        &write_b,
        memory("project-b"),
        project::write_limits(),
    )
    .await
    .unwrap();

    let mut initial = fixture::Fixture::actual(model_api::TaskKind::Embedding);
    initial.body["recipe"]["dimensions"] = serde_json::json!(128);
    let config = initial.config(&["a", "b"]);
    let old_id = config.config_id("a").unwrap();
    let expected_old_a = ExpectedModel::new(&config, "a", "service").unwrap();
    let expected_b = ExpectedModel::new(&config, "b", "service").unwrap();
    let service = fixture::service(snapshot_parent.clone());
    assert_eq!(
        service
            .install(config, fixture::context(&setup_cancel))
            .unwrap(),
        1
    );
    let old_owner = service
        .admit(
            "a",
            old_id,
            ModelTask::Embedding,
            fixture::context(&setup_cancel),
        )
        .unwrap()
        .0;
    let old_weak = Arc::downgrade(&old_owner);
    let blocked = old_owner.native.clone().lock_owned().await;
    let limits = fixture::rpc_limits();
    let client = ModelServiceClient::new(limits).unwrap();
    let old_consumer_inference = project::LocalInference {
        client: &client,
        lease: &lease_a,
        runtime: a.runtime(),
        expected: &expected_old_a,
    };
    let old_consumer = MemoryEmbeddingModel::new(
        old_ctx.snapshot(),
        old_consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let b_consumer_inference = project::LocalInference {
        client: &client,
        lease: &lease_b,
        runtime: b.runtime(),
        expected: &expected_b,
    };
    let b_consumer = MemoryEmbeddingModel::new(
        b_ctx.snapshot(),
        b_consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let (old_address, old_connection, old_server) =
        rpc::server(service.clone(), secret(&a, old_ctx.snapshot()), limits);
    let old_request = old_consumer.index(
        &read_a,
        &write_a,
        project::read_limits(),
        project::write_limits(),
        old_consumer_inference.limits(),
        |operation| old_consumer_inference.infer(operation, TcpStream::connect(old_address)),
    );
    tokio::pin!(old_request);
    admitted(old_request.as_mut(), &service, 3, end).await;
    let (b_address, b_connection, b_server) =
        rpc::server(service.clone(), secret(&b, b_ctx.snapshot()), limits);
    let b_request = b_consumer.index(
        &read_b,
        &write_b,
        project::read_limits(),
        project::write_limits(),
        b_consumer_inference.limits(),
        |operation| b_consumer_inference.infer(operation, TcpStream::connect(b_address)),
    );
    tokio::pin!(b_request);
    admitted(b_request.as_mut(), &service, 2, end).await;
    // Observe the actual RPC generation too: the Records consumer deliberately
    // stores portable vector identity rather than the service generation number.
    let (pin_address, pin_connection, pin_server) =
        rpc::server(service.clone(), secret(&b, b_ctx.snapshot()), limits);
    let pinned_b = client.embed(
        &lease_b,
        b.runtime(),
        &expected_b,
        crate::model_service::EmbedOperation {
            input: EmbedInput::Query,
            texts: vec!["a".into()],
            deadline: b_ctx
                .snapshot()
                .deadline()
                .expect("finite fixture Original"),
        },
        TcpStream::connect(pin_address),
    );
    tokio::pin!(pinned_b);
    admitted(pinned_b.as_mut(), &service, 1, end).await;

    let mut changed = fixture::Fixture::actual(model_api::TaskKind::Embedding);
    changed.body["recipe"]["dimensions"] = serde_json::json!(128);
    changed.body["revision"] = serde_json::json!("fixture-2");
    std::fs::write(
        changed.directory.path().join("model.json"),
        serde_json::to_vec(&changed.body).unwrap(),
    )
    .unwrap();
    let mut next = initial.settings(&["b"]);
    let mut changed_a = changed.settings(&["a"]);
    changed_a.slots[0].selection.revision = "fixture-2".into();
    next.slots.extend(changed_a.slots);
    let inputs = next
        .slots
        .iter()
        .map(|slot| {
            CapturedModelInput::capture(
                &slot.name,
                &slot.bundle,
                &fixture::capture_policy(),
                SourceReadLimits::new(65536, 512, 65536).unwrap(),
                setup.snapshot().clone(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let next = ResolvedConfig::resolve(
        next,
        fixture::cpu_selection(),
        inputs,
        setup.snapshot(),
        &fixture::manifest_limits(),
    )
    .unwrap();
    let expected_new_a = ExpectedModel::new(&next, "a", "service").unwrap();
    assert_ne!(expected_old_a.config_id(), expected_new_a.config_id());
    assert_ne!(expected_old_a.vector_id(), expected_new_a.vector_id());
    assert_eq!(next.config_id("b").unwrap(), old_id);
    assert_eq!(
        service
            .install(next, fixture::context(&setup_cancel))
            .unwrap(),
        2
    );
    let new_owner = service
        .admit(
            "a",
            expected_new_a.config_id(),
            ModelTask::Embedding,
            fixture::context(&setup_cancel),
        )
        .unwrap()
        .0;
    let new_weak = Arc::downgrade(&new_owner);
    assert!(!Arc::ptr_eq(&old_owner, &new_owner));
    let retained_b = service
        .admit(
            "b",
            old_id,
            ModelTask::Embedding,
            fixture::context(&setup_cancel),
        )
        .unwrap()
        .0;
    assert!(Arc::ptr_eq(&old_owner, &retained_b));
    drop(retained_b);

    old_cancel.cancel();
    let failure = old_request.await.unwrap_err();
    assert!(
        matches!(failure.cause, MemoryEmbeddingCause::Service(ref error) if error.kind() == ModelErrorKind::Cancelled)
    );
    assert!(failure.last_attachment.is_none());
    join_rpc(old_connection, old_server).await;
    assert!(!b_cancel.is_cancelled());

    let fresh_cancel = Cancellation::default();
    let fresh = project::request_until(&fresh_cancel, end);
    let key_a = a.runtime().check(fresh.snapshot()).unwrap();
    let caller_a = issuer_a
        .authenticate_runtime(
            std::str::from_utf8(key_a.secret(fresh.snapshot()).unwrap()).unwrap(),
            &fresh,
        )
        .unwrap();
    let read_a = authority_a
        .admit_authenticated_read(&caller_a, &fresh, a.scope(), ProjectReadModule::Memory)
        .unwrap();
    let write_a = authority_a
        .admit_authenticated_write(&caller_a, &fresh, a.scope(), ProjectWriteOperation::Memory)
        .unwrap();
    let mut selection_a = issuer_a
        .capture_service(&caller_a, ServicePurpose::Embedding)
        .unwrap();
    let lease_a = selection_a.local().unwrap();
    let new_consumer_inference = project::LocalInference {
        client: &client,
        lease: &lease_a,
        runtime: a.runtime(),
        expected: &expected_new_a,
    };
    let new_consumer = MemoryEmbeddingModel::new(
        fresh.snapshot(),
        new_consumer_inference.identity(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let (address, connection, server) =
        rpc::server(service.clone(), secret(&a, fresh.snapshot()), limits);
    let result = new_consumer
        .index(
            &read_a,
            &write_a,
            project::read_limits(),
            project::write_limits(),
            new_consumer_inference.limits(),
            |operation| new_consumer_inference.infer(operation, TcpStream::connect(address)),
        )
        .await
        .unwrap();
    assert!(matches!(
        result,
        MemoryEmbeddingIndexRead::Complete {
            last_attachment: Some(_),
            ..
        }
    ));
    join_rpc(connection, server).await;
    assert!(
        old_owner.native.try_lock().is_err(),
        "new A completed while old B remains borrowed"
    );

    let (address, connection, server) =
        rpc::server(service.clone(), secret(&a, fresh.snapshot()), limits);
    let stale = client
        .embed(
            &lease_a,
            a.runtime(),
            &expected_old_a,
            crate::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["a".into()],
                deadline: fresh
                    .snapshot()
                    .deadline()
                    .expect("finite fixture Original"),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap_err();
    assert_eq!(stale.kind(), ModelErrorKind::ConfigChanged);
    join_rpc(connection, server).await;
    drop(blocked);
    let result = b_request.await.unwrap();
    assert!(matches!(
        result,
        MemoryEmbeddingIndexRead::Complete {
            last_attachment: Some(_),
            ..
        }
    ));
    let retained = pinned_b.await.unwrap();
    assert_eq!((retained.config_id, retained.generation), (old_id, 1));
    assert!(!b_cancel.is_cancelled());
    join_rpc(b_connection, b_server).await;
    join_rpc(pin_connection, pin_server).await;

    let options = || MemoryHybridSearchOptions {
        filter: MemoryListFilter::default(),
        include_deleted_history: false,
        top_k: NonZeroUsize::new(2).unwrap(),
    };
    let (address, connection, server) =
        rpc::server(service.clone(), secret(&a, fresh.snapshot()), limits);
    let MemoryHybridSearchRead::Published {
        total_hits, hits, ..
    } = new_consumer
        .query_hybrid(
            &read_a,
            "Paris".into(),
            options(),
            project::read_limits(),
            |operation| new_consumer_inference.infer(operation, TcpStream::connect(address)),
        )
        .await
        .unwrap()
    else {
        panic!("A corpus must be published")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current, row_a);
    join_rpc(connection, server).await;
    let (address, connection, server) =
        rpc::server(service.clone(), secret(&b, b_ctx.snapshot()), limits);
    let MemoryHybridSearchRead::Published {
        total_hits, hits, ..
    } = b_consumer
        .query_hybrid(
            &read_b,
            "Paris".into(),
            options(),
            project::read_limits(),
            |operation| b_consumer_inference.infer(operation, TcpStream::connect(address)),
        )
        .await
        .unwrap()
    else {
        panic!("B corpus must remain published")
    };
    assert_eq!(total_hits, 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].current, row_b);
    join_rpc(connection, server).await;
    drop(old_owner);
    drop(new_owner);
    service
        .shutdown(fixture::context(&setup_cancel))
        .await
        .unwrap();
    assert!(old_weak.upgrade().is_none());
    assert!(new_weak.upgrade().is_none());
    drop(sdk);
}
