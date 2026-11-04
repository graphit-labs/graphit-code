#[path = "support/model_auth.rs"]
mod auth;
#[path = "support/model_fixture.rs"]
#[allow(
    dead_code,
    reason = "remaining shared helpers are exercised by model_service tests"
)]
mod fixture;
#[path = "support/gemma_model.rs"]
#[allow(
    dead_code,
    reason = "shared configuration helpers are exercised by model_service tests"
)]
mod gemma;
#[path = "support/model_service_revocation.rs"]
mod model_service_revocation;
use graphit_auth::{ServicePurpose, profile::AiService};
use graphit_core::{
    artifact_store::SnapshotContext,
    brand::Brand,
    config::{ConfigKey, ConfigStore},
    file_lock::Cancellation,
    project_paths::InstallationPaths,
    tcp_listener::PortSelection,
};
use graphit_daemon::model_service as model_api;
use graphit_daemon::{model_child::*, model_service::*};
use graphit_process::{
    Chunk, Cleanup, HostLimits, HostShutdown, Limits, ProcessHost, Request, Retention, ServiceSink,
    SinkFailure, run_service, spawn_service,
};
use std::{
    io,
    net::{Ipv4Addr, TcpListener},
    num::NonZeroU16,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt, net::TcpStream};
use zeroize::Zeroizing;

fn model_environment(temporary: &Path) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    ["TMPDIR", "TEMP", "TMP"]
        .into_iter()
        .map(|key| (key.into(), temporary.as_os_str().to_owned()))
        .collect()
}
fn process_limits() -> Limits {
    Limits {
        stdin_bytes: 65,
        stdout_bytes: 512,
        stderr_bytes: 4096,
        termination_grace: Duration::from_secs(1),
        cleanup: Duration::from_secs(5),
    }
}
async fn host(service: &Cancellation) -> ProcessHost {
    let host = ProcessHost::install(service, HostLimits::new(4, 4).unwrap()).unwrap();
    assert!(matches!(
        host.ready(Instant::now() + Duration::from_secs(5)).await,
        graphit_process::HostReadiness::Ready
    ));
    host
}
fn installation(root: &Path) -> InstallationPaths {
    InstallationPaths::resolve(Brand::default(), root, None, Some(root)).unwrap()
}
fn configure(root: &Path, value: serde_json::Value, cancel: &Cancellation) {
    ConfigStore::global(&installation(root))
        .unwrap()
        .set(
            ConfigKey::new("daemon.models").unwrap(),
            value,
            Duration::from_secs(5),
            cancel,
        )
        .unwrap();
}
fn model_server_executable() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_graphit-model-server"))
}
fn executable_with_spaces(directory: &Path) -> PathBuf {
    let executable = directory.join("model server α with spaces");
    // CUDA Release tests consume the qualified complete package. Raw Cargo
    // executables do not carry the package's relative loader paths/runtime.
    let source = std::env::var_os("GRAPHIT_TEST_MODEL_SERVER")
        .map(PathBuf::from)
        .unwrap_or_else(model_server_executable);
    std::fs::copy(&source, &executable).unwrap();
    let runtime = source.parent().unwrap().join("graphit-runtime/lib");
    if runtime.is_dir() {
        let destination = directory.join("graphit-runtime/lib");
        std::fs::create_dir_all(&destination).unwrap();
        for entry in std::fs::read_dir(runtime).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if let Err(error) = std::fs::hard_link(entry.path(), &target) {
                if error.kind() == io::ErrorKind::CrossesDevices {
                    std::fs::copy(entry.path(), target).unwrap();
                } else {
                    panic!("fixture runtime link failed: {:?}", error.kind());
                }
            }
        }
    }
    executable
}
fn assert_finished(process: ChildProcessResult) {
    match process.unwrap() {
        Ok(outcome) => {
            assert_native_cleanup(outcome.cleanup);
            assert!(outcome.remaining.is_none());
        }
        Err(failure) => {
            assert_native_cleanup(failure.cleanup);
            assert!(failure.remaining.is_none());
        }
    }
}

fn assert_native_cleanup(cleanup: Cleanup) {
    #[cfg(unix)]
    assert_eq!(cleanup, Cleanup::Complete);
    #[cfg(windows)]
    assert_eq!(
        cleanup,
        Cleanup::Unconfirmed(graphit_process::UnconfirmedCleanup::TreeUnknownWindows {
            leader: graphit_process::LeaderCleanup::Reaped,
            control: graphit_process::ControlFailures::default(),
        })
    );
}

#[tokio::test]
#[ignore = "requires pinned BF16 embedding and F16 paired reranker artifacts"]
async fn native_child_uses_auth_real_models_literal_path_and_outlives_startup() {
    let setup_started = Instant::now();
    let cancel = Cancellation::default();
    let setup = auth::request(&cancel, setup_started + Duration::from_secs(180));
    let auth = auth::AuthFixture::new(
        &setup,
        AiService::Local(auth::cpu()),
        AiService::Local(auth::cpu()),
        true,
    );
    let embedding = fixture::Fixture::actual_with_original(TaskKind::Embedding, setup.snapshot());
    let rerank = fixture::Fixture::actual_with_original(TaskKind::Rerank, setup.snapshot());
    let mut settings = embedding.settings(&["e"]);
    settings.slots.extend(rerank.settings(&["r"]).slots);
    for (slot, fixture) in settings.slots.iter_mut().zip([&embedding, &rerank]) {
        let bundle = auth.installation_root().join("models").join(&slot.name);
        std::fs::create_dir_all(&bundle).unwrap();
        for file in ["model.gguf", "model.json"] {
            let source = fixture.directory.path().join(file);
            let destination = bundle.join(file);
            if let Err(error) = std::fs::hard_link(&source, &destination) {
                if error.kind() == io::ErrorKind::CrossesDevices {
                    std::fs::copy(&source, &destination).unwrap();
                } else {
                    panic!("fixture hard link failed: {:?}", error.kind());
                }
            }
        }
        slot.bundle = bundle;
    }
    let mut value = serde_json::to_value(&settings).unwrap();
    for (value, slot) in value["slots"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(&settings.slots)
    {
        value["bundle"] = serde_json::json!(format!("models/{}", slot.name));
    }
    configure(auth.installation_root(), value, &cancel);
    setup.snapshot().check().unwrap();
    drop(setup);
    // Installing test weights and seeding Auth are fixture preparation. The
    // request keeps one unchanged deadline through binding, startup,
    // both real inference clients, revocation and joined child shutdown.
    let request_started = Instant::now();
    let ctx = auth::request(&cancel, request_started + Duration::from_secs(180));
    let config = ResolvedConfig::resolve_declared(
        settings,
        fixture::cpu_selection(),
        ctx.snapshot(),
        &fixture::capture_policy(),
        graphit_core::artifact_store::SourceReadLimits::new(65536, 512, 65536).unwrap(),
        &fixture::manifest_limits(),
    )
    .unwrap();
    let service_signal = Cancellation::default();
    let process_host = host(&service_signal).await;
    let program = tempfile::tempdir().unwrap();
    let startup_cancel = Cancellation::default();
    let startup = Arc::new(SnapshotContext::new(
        ctx.snapshot().required_deadline().unwrap(),
        startup_cancel.clone(),
    ));
    let child = ModelChild::start(
        ChildLaunch {
            executable: executable_with_spaces(program.path()),
            installation_root: auth.installation_root().to_owned(),
            environment: model_environment(program.path()),
            limits: process_limits(),
        },
        &auth.runtime,
        &startup,
        &process_host,
    )
    .await
    .unwrap();
    startup_cancel.cancel();
    drop(startup);
    let address = child.address();
    let tarpc::client::NewClient {
        client: wrong_client,
        dispatch,
    } = ModelRpcClient::new(
        tarpc::client::Config::default(),
        client_transport(
            TcpStream::connect(address).await.unwrap(),
            fixture::rpc_limits(),
        )
        .unwrap(),
    );
    let call = wrong_client.embed(
        tarpc::context::current(),
        RuntimeCredential::from_secret(&[b'a'; 64]).unwrap(),
        "absent".into(),
        ConfigId([0; 32]),
        EmbedInput::Query,
        vec!["a".into()],
    );
    let wrong = {
        tokio::pin!(call, dispatch);
        tokio::select! {
            result=&mut call => result.unwrap(),
            result=&mut dispatch => panic!("dispatch ended: {result:?}"),
        }
    };
    let error = wrong.unwrap_err();
    assert_eq!(error.kind, ModelErrorKind::Authentication);
    assert_eq!(error.limit, None);
    drop(wrong_client);
    let client = ModelServiceClient::new(fixture::rpc_limits()).unwrap();
    let runtime = auth.runtime.check(ctx.snapshot()).unwrap();
    let issuer = auth.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(runtime.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let expected = ExpectedModel::new(&config, "e", "service").unwrap();
    let out = client
        .embed(
            &lease,
            &auth.runtime,
            &expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Document,
                texts: vec!["a b".into(), "a".into()],
                deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    assert_normalized(&out.vectors, 2, 768);
    assert_eq!(out.dimensions, 768);
    // EOF of A is real. B uses its own operation connection and remains usable.
    drop(TcpStream::connect(address).await.unwrap());
    let out = client
        .embed(
            &lease,
            &auth.runtime,
            &expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Query,
                texts: vec!["b".into()],
                deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    assert_normalized(&out.vectors, 1, 768);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank, auth::cpu());
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(runtime.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.local().unwrap();
    let expected = ExpectedModel::new(&config, "r", "service").unwrap();
    let out = client
        .rerank(
            &lease,
            &auth.runtime,
            &expected,
            "a".into(),
            vec!["a".into(), "a b".into()],
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    assert_eq!(out.scores.len(), 2);
    assert!(out.scores.iter().all(|score| score.is_finite()));
    assert_eq!(out.generation, 2); // Rerank was absent from the default startup map.
    assert!(
        out.device
            .matches_request(graphit_ai::local_execution::LocalProvider::CPU, 0)
    );
    auth.revoke_service(&ctx);
    let denied = client
        .rerank(
            &lease,
            &auth.runtime,
            &expected,
            "a".into(),
            vec!["b".into()],
            TcpStream::connect(address),
        )
        .await
        .unwrap_err();
    assert_eq!(denied.kind(), ModelErrorKind::Authentication);
    let shutdown = child.shutdown(&client, &auth.runtime, ctx.snapshot()).await;
    shutdown.drain.unwrap();
    assert_finished(shutdown.process);
    assert!(TcpStream::connect(address).await.is_err());
    assert!(auth.runtime.check(ctx.snapshot()).is_ok());
    assert!(matches!(
        process_host
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await,
        HostShutdown::Joined(_)
    ));
}

struct Discard;
impl ServiceSink for Discard {
    async fn push<'a>(&'a mut self, _: Chunk<'a>, _: &'a Cancellation) -> Result<(), SinkFailure> {
        Ok(())
    }
}
#[tokio::test]
async fn native_bootstrap_and_missing_auth_fail_without_ready_or_secret_output() {
    assert_bootstrap_rejects_invalid_input(&model_server_executable()).await;
}

async fn assert_bootstrap_rejects_invalid_input(executable: &Path) {
    let cancel = Cancellation::default();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        cancel.clone(),
    ));
    let directory = tempfile::tempdir().unwrap();
    let host = host(&cancel).await;
    for bytes in [
        vec![b'a'; 63],
        vec![b'a'; 65],
        vec![b'G'; 64],
        vec![b'a'; 64],
    ] {
        let process = spawn_service(
            Request {
                program: executable.to_owned(),
                args: vec![directory.path().as_os_str().to_owned()],
                cwd: directory.path().to_owned(),
                environment: vec![],
                stdin: Some(Zeroizing::new(bytes.clone())),
                limits: process_limits(),
            },
            &original,
            &host,
        )
        .await
        .unwrap();
        let result = run_service(process, Retention::Full, &mut Discard).await;
        let (status, transcript, cleanup) = match result {
            Ok(out) => (Some(out.status), out.transcript.unwrap(), out.cleanup),
            Err(err) => (err.status, err.transcript.unwrap(), err.cleanup),
        };
        assert!(!status.unwrap().success());
        assert_native_cleanup(cleanup);
        assert!(transcript.stdout().is_empty());
        assert!(
            !transcript
                .stderr()
                .windows(bytes.len())
                .any(|part| part == bytes)
        );
    }
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
}

#[tokio::test]
async fn cold_child_reports_model_errors_on_demand_and_pre_cancelled_start_has_no_child() {
    assert_cold_child_reports_model_errors(&model_server_executable()).await;
}

async fn assert_cold_child_reports_model_errors(executable: &Path) {
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let host = host(&cancel).await;
    let launch = || ChildLaunch {
        executable: executable.to_owned(),
        installation_root: auth.installation_root().to_owned(),
        environment: vec![],
        limits: process_limits(),
    };
    for (configuration, expected_error) in [
        (None, ModelErrorKind::Selection),
        (
            Some(serde_json::json!({"slots":[{}]})),
            ModelErrorKind::Configuration,
        ),
    ] {
        if let Some(configuration) = configuration {
            configure(auth.installation_root(), configuration, &cancel);
        }
        let child = ModelChild::start(launch(), &auth.runtime, ctx.snapshot(), &host)
            .await
            .unwrap();
        let snapshots = auth.installation_root().join("tmp");
        let captured = graphit_core::native_document_file::CapturedDocumentDirectory::capture(
            &snapshots,
            ChildOptions::standard().unwrap().bootstrap.native_limits(),
            ctx.snapshot(),
        )
        .unwrap();
        captured.private_handle(ctx.snapshot()).unwrap();
        assert_eq!(std::fs::read_dir(&snapshots).unwrap().count(), 0);
        let address = child.address();
        let client = ModelServiceClient::new(fixture::rpc_limits()).unwrap();
        let ack = client
            .refresh(
                &auth.runtime,
                ctx.snapshot(),
                Default::default(),
                TcpStream::connect(address),
            )
            .await
            .unwrap();
        assert!(ack.config_ids.is_empty());
        let credential = {
            let runtime = auth.runtime.check(ctx.snapshot()).unwrap();
            RuntimeCredential::from_secret(runtime.secret(ctx.snapshot()).unwrap()).unwrap()
        };
        let tarpc::client::NewClient {
            client: rpc,
            dispatch,
        } = ModelRpcClient::new(
            tarpc::client::Config::default(),
            client_transport(
                TcpStream::connect(address).await.unwrap(),
                fixture::rpc_limits(),
            )
            .unwrap(),
        );
        let mut context = tarpc::context::current();
        context.deadline = ctx.snapshot().required_deadline().unwrap();
        let call = rpc.embed(
            context,
            credential,
            "e".into(),
            ConfigId([0; 32]),
            EmbedInput::Query,
            vec!["bounded input".into()],
        );
        let result = {
            tokio::pin!(call, dispatch);
            tokio::select! {
                result = &mut call => result.unwrap(),
                result = &mut dispatch => panic!("dispatch ended: {result:?}"),
            }
        };
        assert_eq!(result.unwrap_err().kind, expected_error);
        drop(rpc);
        let shutdown = child.shutdown(&client, &auth.runtime, ctx.snapshot()).await;
        shutdown.drain.unwrap();
        assert_finished(shutdown.process);
        assert!(TcpStream::connect(address).await.is_err());
    }
    let service_original = Arc::new(SnapshotContext::service(Cancellation::default()));
    let Err(error) = ModelChild::start(launch(), &auth.runtime, &service_original, &host).await
    else {
        panic!("unbounded startup created a child");
    };
    assert_eq!(error.cause.kind(), io::ErrorKind::InvalidInput);
    assert!(error.process.is_none());
    let interrupted = Cancellation::default();
    interrupted.cancel();
    let original = Arc::new(SnapshotContext::new(
        Instant::now() + Duration::from_secs(5),
        interrupted,
    ));
    let Err(error) = ModelChild::start(launch(), &auth.runtime, &original, &host).await else {
        panic!("cancelled startup created a child");
    };
    assert_eq!(error.cause.kind(), io::ErrorKind::Interrupted);
    assert!(error.process.is_none());
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
}

#[tokio::test]
async fn actual_bind_failure_emits_no_readiness_and_stalled_bootstrap_expires() {
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let runtime = auth.runtime.check(ctx.snapshot()).unwrap();
    let key = runtime.secret(ctx.snapshot()).unwrap();
    configure(
        auth.installation_root(),
        serde_json::json!({"slots":[]}),
        &cancel,
    );
    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = NonZeroU16::new(occupied.local_addr().unwrap().port()).unwrap();
    let mut options = ChildOptions::standard().unwrap();
    options.listener.ports = PortSelection::Range {
        first: port,
        last: port,
    };
    let (mut reader, writer) = tokio::io::duplex(512);
    assert!(
        serve_child(
            auth.installation_root(),
            options,
            key.as_slice(),
            writer,
            Cancellation::default()
        )
        .await
        .is_err()
    );
    let mut output = Vec::new();
    reader.read_to_end(&mut output).await.unwrap();
    assert!(output.is_empty());
    let (bootstrap, _held_open) = tokio::io::duplex(128);
    let mut options = ChildOptions::standard().unwrap();
    options.startup_timeout = Duration::from_millis(20);
    let error = serve_child(
        auth.installation_root(),
        options,
        bootstrap,
        tokio::io::sink(),
        Cancellation::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[tokio::test]
#[ignore = "requires the pinned real BF16 model in GRAPHIT_TEST_GEMMA2_MODEL"]
async fn native_gemma_child_installed_relative_four_roles_and_configured_restart() {
    let model =
        PathBuf::from(std::env::var_os("GRAPHIT_TEST_GEMMA2_MODEL").expect("pinned BF16 fixture"));
    let model = std::fs::canonicalize(model).unwrap();
    let source = gemma::GemmaFixture::new(&model);
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(600));
    let auth = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let bundle = auth.installation_root().join("models/gemma nested α");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::hard_link(
        source.directory.path().join("model.gguf"),
        bundle.join("model.gguf"),
    )
    .unwrap();
    for dimensions in [768, 128] {
        std::fs::copy(
            source.directory.path().join(format!("{dimensions}.json")),
            bundle.join(format!("{dimensions}.json")),
        )
        .unwrap();
    }
    let host = host(&cancel).await;
    let program = tempfile::tempdir().unwrap();
    let executable = executable_with_spaces(program.path());
    let client = ModelServiceClient::new(ChildOptions::standard().unwrap().rpc).unwrap();
    let runtime = auth.runtime.check(ctx.snapshot()).unwrap();
    let issuer = auth.issuer(&ctx, ServicePurpose::Embedding, auth::cpu());
    let caller = issuer
        .authenticate_runtime(
            std::str::from_utf8(runtime.secret(ctx.snapshot()).unwrap()).unwrap(),
            &ctx,
        )
        .unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let mut previous_id = None;
    for dimensions in [768, 128] {
        let settings = ModelSettings {
            slots: vec![SlotSettings {
                name: "embedding".into(),
                selection: SelectionSettings {
                    id: "embeddinggemma2".into(),
                    revision: "bfcd298762cc34d0357ece5ebdd31791a3a374d8".into(),
                    task: TaskKind::Embedding,
                },
                bundle: bundle.clone(),
                manifest_relative: format!("{dimensions}.json").into(),
                runtime: RuntimeSettings::EmbeddingGemma2 {
                    threads: std::num::NonZeroU32::new(2).unwrap(),
                    queue_capacity: std::num::NonZeroUsize::new(8).unwrap(),
                },
            }],
        };
        let options = ChildOptions::standard().unwrap();
        let input = CapturedModelInput::capture(
            "embedding",
            &bundle,
            &options.capture,
            options.source,
            ctx.snapshot().clone(),
        )
        .unwrap();
        let mut value = serde_json::to_value(&settings).unwrap();
        let config = ResolvedConfig::resolve(
            settings,
            fixture::cpu_selection(),
            vec![input],
            ctx.snapshot(),
            &options.manifest,
        )
        .unwrap();
        let expected = ExpectedModel::new(&config, "embedding", "service").unwrap();
        if let Some(id) = previous_id {
            assert_ne!(config.config_id("embedding").unwrap(), id);
        }
        previous_id = config.config_id("embedding");
        value["slots"][0]["bundle"] = serde_json::json!("models/gemma nested α");
        assert!(value["slots"][0]["runtime"].get("native_library").is_none());
        configure(auth.installation_root(), value, &cancel);
        let child = ModelChild::start(
            ChildLaunch {
                executable: executable.clone(),
                installation_root: auth.installation_root().to_owned(),
                environment: model_environment(program.path()),
                limits: process_limits(),
            },
            &auth.runtime,
            ctx.snapshot(),
            &host,
        )
        .await
        .unwrap();
        let address = child.address();
        for role in [
            EmbedInput::Query,
            EmbedInput::Document,
            EmbedInput::CodeQuery,
            EmbedInput::CodeDocument,
        ] {
            let output = client
                .embed(
                    &lease,
                    &auth.runtime,
                    &expected,
                    graphit_daemon::model_service::EmbedOperation {
                        input: role,
                        texts: vec!["fn graphit() {}".into()],
                        deadline: ctx.snapshot().deadline().expect("finite fixture Original"),
                    },
                    TcpStream::connect(address),
                )
                .await
                .unwrap();
            assert_eq!(output.config_id, config.config_id("embedding").unwrap());
            assert_eq!(output.dimensions, dimensions);
            assert_eq!(output.model, "embeddinggemma2");
            assert_eq!(output.revision, "bfcd298762cc34d0357ece5ebdd31791a3a374d8");
            assert_eq!(output.vectors.len(), 1);
            assert_eq!(output.vectors[0].len(), dimensions as usize);
            assert!(output.vectors[0].iter().all(|v| v.is_finite()));
            let norm: f32 = output.vectors[0].iter().map(|v| v * v).sum();
            assert!((norm - 1.).abs() < 1e-4);
        }
        let stopping = Cancellation::default();
        if dimensions == 128 {
            stopping.cancel();
        }
        let original = SnapshotContext::new(Instant::now() + Duration::from_secs(30), stopping);
        let shutdown = child.shutdown(&client, &auth.runtime, &original).await;
        if dimensions == 768 {
            shutdown.drain.unwrap();
        } else {
            assert!(shutdown.drain.is_err());
        }
        let process = shutdown.process.unwrap();
        let status = match &process {
            Ok(outcome) => &outcome.status,
            Err(failure) => failure.status.as_ref().expect("observed child exit"),
        };
        assert!(
            status.success(),
            "SIGTERM must await native drain before exit"
        );
        assert_finished(Ok(process));
        assert!(TcpStream::connect(address).await.is_err());
    }
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
}

fn assert_normalized(vectors: &[Vec<f32>], count: usize, dimensions: usize) {
    assert_eq!(vectors.len(), count);
    for vector in vectors {
        assert_eq!(vector.len(), dimensions);
        assert!(vector.iter().all(|value| value.is_finite()));
        let norm = vector
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}

#[tokio::test]
async fn actual_child_completion_is_cancel_safe_and_retains_the_same_exit() {
    assert_child_completion_is_cancel_safe(&model_server_executable()).await;
}

async fn assert_child_completion_is_cancel_safe(executable: &Path) {
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    configure(
        auth.installation_root(),
        serde_json::json!({"slots":[]}),
        &cancel,
    );
    let host = host(&cancel).await;
    let mut child = ModelChild::start(
        ChildLaunch {
            executable: executable.to_owned(),
            installation_root: auth.installation_root().to_owned(),
            environment: vec![],
            limits: process_limits(),
        },
        &auth.runtime,
        ctx.snapshot(),
        &host,
    )
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), child.completed())
            .await
            .is_err()
    );
    let address = child.address();
    let client = ModelServiceClient::new(fixture::rpc_limits()).unwrap();
    client
        .shutdown(&auth.runtime, ctx.snapshot(), TcpStream::connect(address))
        .await
        .unwrap();
    // The RPC drains native workers; the existing host lifetime requests the
    // process stop independently of this ModelChild observation.
    cancel.cancel();
    for _ in 0..2 {
        let outcome = tokio::time::timeout(Duration::from_secs(5), child.completed())
            .await
            .unwrap();
        let (status, cleanup, remaining) = match outcome {
            Ok(Ok(outcome)) => (Some(&outcome.status), outcome.cleanup, &outcome.remaining),
            Ok(Err(failure)) => (failure.status.as_ref(), failure.cleanup, &failure.remaining),
            Err(error) => panic!("actual child runner failed: {error}"),
        };
        assert!(status.expect("real child status").success());
        assert_native_cleanup(cleanup);
        assert!(remaining.is_none());
    }
    assert_finished(child.join().await);
    assert!(TcpStream::connect(address).await.is_err());
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
}

#[tokio::test]
#[ignore = "requires GRAPHIT_TEST_MODEL_SERVER from the installed package"]
async fn packaged_model_child_preserves_bootstrap_errors_and_shutdown() {
    let executable = PathBuf::from(
        std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").expect("installed model-server executable"),
    );
    assert!(executable.is_absolute() && executable.is_file());
    assert_bootstrap_rejects_invalid_input(&executable).await;
    assert_cold_child_reports_model_errors(&executable).await;
    assert_child_completion_is_cancel_safe(&executable).await;
}

#[cfg(unix)]
#[tokio::test]
async fn actual_unsolicited_nonzero_child_exit_retains_status_and_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    let cancel = Cancellation::default();
    let ctx = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::new(
        &ctx,
        AiService::Local(auth::cpu()),
        AiService::Disabled,
        true,
    );
    let program = tempfile::tempdir().unwrap();
    let script = program.path().join("completed child");
    std::fs::write(
        &script,
        r#"#!/bin/sh
IFS= read -r credential
unset credential
printf '%s\n' '{"address":"127.0.0.1:9"}'
exit 23
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let host = host(&cancel).await;
    let mut child = ModelChild::start(
        ChildLaunch {
            executable: script,
            installation_root: auth.installation_root().to_owned(),
            environment: vec![],
            limits: process_limits(),
        },
        &auth.runtime,
        ctx.snapshot(),
        &host,
    )
    .await
    .unwrap();
    child.completed().await;
    let process = child.join().await.unwrap();
    let (status, cleanup, remaining) = match process {
        Ok(outcome) => (Some(outcome.status), outcome.cleanup, outcome.remaining),
        Err(failure) => (failure.status, failure.cleanup, failure.remaining),
    };
    assert_eq!(status.unwrap().code(), Some(23));
    assert_eq!(cleanup, Cleanup::Complete);
    assert!(remaining.is_none());
    assert!(matches!(
        host.shutdown(Instant::now() + Duration::from_secs(5)).await,
        HostShutdown::Joined(_)
    ));
}
