#![cfg(target_os = "linux")]

use graphit_app::{
    model_provision::{
        DownloadHop, InstallSource, MODEL_BYTES, MODEL_FILE, MODEL_ID, MODEL_REVISION,
        ModelProvisioner, PinnedModel, ProvisionError, ProvisionLimits, RERANK_MODEL_BYTES,
        RERANK_MODEL_FILE, RERANK_MODEL_ID, RERANK_MODEL_REVISION, StageError, receive_pinned,
    },
    models::{default_rerank_runtime, default_runtime, pinned_manifest, pinned_rerank_manifest},
};
use graphit_core::{
    artifact_store::{CapturedSourceReader, SnapshotContext},
    brand::Brand,
    config::{ConfigKey, ConfigMutation},
    file_lock::Cancellation,
    project_paths::InstallationPaths,
};
use graphit_daemon::model_service::RuntimeSettings;
use graphit_http::client::{
    BoundedClient, ClientPolicy, NetworkEffect, NetworkError, NetworkStreamError,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

fn context(duration: Duration) -> Arc<SnapshotContext> {
    Arc::new(SnapshotContext::new(
        Instant::now() + duration,
        Cancellation::default(),
    ))
}
fn run<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}
fn allowed() -> Result<(), io::Error> {
    Ok(())
}
struct Server {
    origin: String,
    release: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(response: Vec<u8>, wait: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (release, released) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut block = [0; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let read = stream.read(&mut block).unwrap();
                assert_ne!(read, 0);
                request.extend_from_slice(&block[..read]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(!request.contains("authorization:"));
            assert!(!request.contains("cookie:"));
            stream.write_all(&response).unwrap();
            if wait {
                let _ = released.recv_timeout(Duration::from_secs(5));
            }
        });
        Self {
            origin,
            release: Some(release),
            worker: Some(worker),
        }
    }
    fn client(&self) -> BoundedClient {
        BoundedClient::new(
            ClientPolicy {
                origin: &self.origin,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
            ProvisionLimits::product().unwrap().http,
        )
        .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        self.worker.take().unwrap().join().unwrap();
    }
}
fn receive(response: Vec<u8>) -> (Result<DownloadHop, ProvisionError<io::Error>>, u64) {
    let server = Server::new(response, false);
    let client = server.client();
    let mut stage = tempfile::tempfile().unwrap();
    let original = context(Duration::from_secs(5));
    let result = run(receive_pinned(
        PinnedModel::EmbeddingGemma2,
        &client,
        &server.origin,
        &mut stage,
        &original,
        &mut allowed,
    ));
    (result, stage.metadata().unwrap().len())
}

#[test]
fn bad_status_and_short_declared_length_never_write_payload() {
    let (result, bytes) =
        receive(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 3\r\n\r\nabc".to_vec());
    assert!(matches!(
        result,
        Err(ProvisionError::Network(NetworkStreamError::Sink {
            source: StageError::Status(403),
            accepted_bytes: 0,
            ..
        }))
    ));
    assert_eq!(bytes, 0);
    let (result, bytes) = receive(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc".to_vec());
    assert!(matches!(
        result,
        Err(ProvisionError::Network(NetworkStreamError::Sink {
            source: StageError::Integrity,
            accepted_bytes: 0,
            ..
        }))
    ));
    assert_eq!(bytes, 0);
}
#[test]
fn oversized_and_truncated_responses_return_actual_transport_failures() {
    let (result, bytes) = receive(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            RERANK_MODEL_BYTES + 1
        )
        .into_bytes(),
    );
    assert!(matches!(
        result,
        Err(ProvisionError::Network(NetworkStreamError::Network {
            source: NetworkError::Limit,
            ..
        }))
    ));
    assert_eq!(bytes, 0);
    let (result, bytes) = receive(
        format!("HTTP/1.1 200 OK\r\nContent-Length: {MODEL_BYTES}\r\n\r\nabc").into_bytes(),
    );
    assert!(matches!(
        result,
        Err(ProvisionError::Network(NetworkStreamError::Network {
            accepted_bytes: 3,
            ..
        }))
    ));
    assert_eq!(bytes, 3);
    let (result, bytes) = receive(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n".to_vec(),
    );
    assert!(matches!(result, Err(ProvisionError::Integrity)));
    assert_eq!(bytes, 3);
}
#[test]
fn redirects_keep_only_exact_https_origins_and_do_not_write_bodies() {
    for location in [
        "https://huggingface.co/pinned?signature=test-private-canary",
        "https://us.aws.cdn.hf.co/pinned?signature=test-private-canary",
    ] {
        let (result, bytes) = receive(
            format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 3\r\n\r\nabc")
                .into_bytes(),
        );
        let DownloadHop::Redirect(url) = result.unwrap() else {
            panic!("expected redirect")
        };
        assert_eq!(url.as_str(), location);
        assert_eq!(bytes, 0);
    }
    for location in [
        "http://huggingface.co/pinned",
        "https://example.invalid/pinned",
        "https://user:password@huggingface.co/pinned",
        "https://huggingface.co:8443/pinned",
    ] {
        let (result, bytes) = receive(
            format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 3\r\n\r\nabc")
                .into_bytes(),
        );
        let error = result.err().unwrap();
        assert!(matches!(
            error,
            ProvisionError::Network(NetworkStreamError::Sink {
                source: StageError::RedirectDenied,
                ..
            })
        ));
        assert!(!format!("{error:?} {error}").contains(location));
        assert_eq!(bytes, 0);
    }
    let (result, bytes) = receive(b"HTTP/1.1 302 Found\r\nLocation: https://huggingface.co/a\r\nLocation: https://huggingface.co/b\r\n\r\n".to_vec());
    assert!(matches!(
        result,
        Err(ProvisionError::Network(NetworkStreamError::Sink {
            source: StageError::RedirectDenied,
            ..
        }))
    ));
    assert_eq!(bytes, 0);
}
#[test]
fn cancellation_and_deadline_after_partial_write_keep_effect_and_count() {
    for cancel in [true, false] {
        let server = Server::new(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n".to_vec(),
            true,
        );
        let client = server.client();
        let mut stage = tempfile::tempfile().unwrap();
        let observed = stage.try_clone().unwrap();
        let original = context(if cancel {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(150)
        });
        let mut gate = || {
            if cancel && observed.metadata()?.len() >= 3 {
                original.cancellation().cancel();
            }
            Ok::<(), io::Error>(())
        };
        let result = run(receive_pinned(
            PinnedModel::EmbeddingGemma2,
            &client,
            &server.origin,
            &mut stage,
            &original,
            &mut gate,
        ));
        let Err(ProvisionError::Network(NetworkStreamError::Network {
            source,
            effect,
            accepted_bytes,
        })) = result
        else {
            panic!("expected original interruption")
        };
        assert!(matches!(
            (cancel, source),
            (true, NetworkError::Cancelled) | (false, NetworkError::Deadline)
        ));
        assert_eq!(effect, NetworkEffect::MayHaveApplied);
        assert_eq!(accepted_bytes, 3);
        assert_eq!(stage.metadata().unwrap().len(), 3);
    }
}
#[test]
fn denied_gate_preserves_cause_and_redacts_diagnostics() {
    let server = Server::new(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n".to_vec(),
        true,
    );
    let client = server.client();
    let mut stage = tempfile::tempfile().unwrap();
    let observed = stage.try_clone().unwrap();
    let original = context(Duration::from_secs(5));
    let mut gate = || {
        if observed.metadata().unwrap().len() >= 3 {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private-cause-canary",
            ))
        } else {
            Ok(())
        }
    };
    let error = run(receive_pinned(
        PinnedModel::EmbeddingGemma2,
        &client,
        &server.origin,
        &mut stage,
        &original,
        &mut gate,
    ))
    .err()
    .unwrap();
    assert!(!format!("{error:?} {error}").contains("private-cause-canary"));
    let ProvisionError::Network(NetworkStreamError::Admission {
        source,
        effect,
        accepted_bytes,
    }) = error
    else {
        panic!("expected denial")
    };
    assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(source.to_string(), "private-cause-canary");
    assert_eq!(effect, NetworkEffect::MayHaveApplied);
    assert_eq!(accepted_bytes, 3);
}
#[test]
fn post_head_denial_is_not_hidden_by_redirect_control_flow() {
    let response = b"HTTP/1.1 302 Found\r\nLocation: https://huggingface.co/pinned?signature=test-private-canary\r\nContent-Length: 3\r\n\r\nabc";
    let original = context(Duration::from_secs(5));
    let mut calls = 0;
    {
        let server = Server::new(response.to_vec(), false);
        let mut stage = tempfile::tempfile().unwrap();
        let mut gate = || {
            calls += 1;
            Ok::<(), io::Error>(())
        };
        assert!(matches!(
            run(receive_pinned(
                PinnedModel::EmbeddingGemma2,
                &server.client(),
                &server.origin,
                &mut stage,
                &original,
                &mut gate
            )),
            Ok(DownloadHop::Redirect(_))
        ));
    }
    let server = Server::new(response.to_vec(), false);
    let mut stage = tempfile::tempfile().unwrap();
    let mut count = 0;
    let mut gate = || {
        count += 1;
        if count == calls {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            Ok(())
        }
    };
    let result = run(receive_pinned(
        PinnedModel::EmbeddingGemma2,
        &server.client(),
        &server.origin,
        &mut stage,
        &original,
        &mut gate,
    ));
    assert!(matches!(
        result,
        Err(ProvisionError::Network(NetworkStreamError::Admission {
            accepted_bytes: 0,
            ..
        }))
    ));
    assert_eq!(stage.metadata().unwrap().len(), 0);
}
fn make_producer(temp: &tempfile::TempDir, original: Arc<SnapshotContext>) -> ModelProvisioner {
    let paths = InstallationPaths::resolve(
        Brand::default(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )
    .unwrap();
    ModelProvisioner::capture(&paths, original, ProvisionLimits::product().unwrap()).unwrap()
}
fn get(producer: &ModelProvisioner, key: &str) -> Option<Value> {
    producer
        .config()
        .writer(Duration::from_secs(5), producer.original().cancellation())
        .unwrap()
        .load()
        .unwrap()
        .get(&ConfigKey::new(key).unwrap())
        .unwrap()
}
fn set(producer: &ModelProvisioner, key: &str, value: Option<Value>) {
    let key = ConfigKey::new(key).unwrap();
    let mutation = match value {
        Some(value) => ConfigMutation::Set { key, value },
        None => ConfigMutation::Unset { key },
    };
    producer
        .config()
        .writer(Duration::from_secs(5), producer.original().cancellation())
        .unwrap()
        .apply([mutation])
        .unwrap();
}
fn no_stages(producer: &ModelProvisioner) {
    assert!(
        fs::read_dir(producer.paths().models_directory())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".model-stage-"))
    );
}
#[test]
fn capture_requires_finite_deadline_before_installation_changes() {
    for cancelled in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("installation");
        let paths =
            InstallationPaths::resolve(Brand::default(), temp.path(), None, Some(&root)).unwrap();
        let cancellation = Cancellation::default();
        if cancelled {
            cancellation.cancel();
        }
        let original = Arc::new(SnapshotContext::service(cancellation));
        let expected = if cancelled {
            io::ErrorKind::Interrupted
        } else {
            io::ErrorKind::InvalidInput
        };
        assert!(matches!(
            ModelProvisioner::capture(&paths, original, ProvisionLimits::product().unwrap()),
            Err(ProvisionError::Io { kind, .. }) if kind == expected
        ));
        assert!(
            !root.exists(),
            "finite admission must precede bootstrap writes"
        );
    }
}

#[test]
fn phase_admission_rejects_replaced_or_nonprivate_installation() {
    use std::os::unix::fs::PermissionsExt;

    for replace in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let producer = make_producer(&temp, context(Duration::from_secs(20)));
        let installation = temp.path().join("installation");
        let mut gate = || {
            if replace {
                fs::rename(&installation, temp.path().join("original-installation"))?;
                graphit_fs::create_private_directory(&installation)?;
            } else {
                fs::set_permissions(&installation, fs::Permissions::from_mode(0o755))?;
            }
            Ok::<(), io::Error>(())
        };
        assert!(matches!(
            producer.check(&mut gate),
            Err(ProvisionError::Bootstrap(_))
        ));
    }
}

#[test]
fn invalid_paths_missing_manifests_and_cancel_before_capture_are_real_errors() {
    let temp = tempfile::tempdir().unwrap();
    let original = context(Duration::from_secs(20));
    let producer = make_producer(&temp, original.clone());
    for path in ["", "../model.json", "/model.json", "bundle/../model.json"] {
        assert!(matches!(
            producer.select(Path::new(path), None, &mut allowed),
            Err(ProvisionError::Configuration)
        ));
    }
    assert!(
        producer
            .select(Path::new("missing/model.json"), None, &mut allowed)
            .is_err()
    );
    original.cancellation().cancel();
    assert!(matches!(
        producer.select(Path::new("missing/model.json"), None, &mut allowed),
        Err(ProvisionError::Io {
            kind: io::ErrorKind::Interrupted,
            ..
        })
    ));
    let paths = producer.paths().clone();
    assert!(
        ModelProvisioner::capture(&paths, original, ProvisionLimits::product().unwrap()).is_err()
    );
    no_stages(&producer);
}
#[test]
fn malformed_captured_payload_and_denied_install_leave_no_bundle_or_stage() {
    let temp = tempfile::tempdir().unwrap();
    let original = context(Duration::from_secs(20));
    let producer = make_producer(&temp, original.clone());
    let source = temp.path().join("short.gguf");
    fs::write(&source, b"bad").unwrap();
    let reader = CapturedSourceReader::capture(
        &source,
        graphit_fs::NodeKind::File,
        &producer.limits().snapshot,
        producer.limits().read,
        original,
    )
    .unwrap();
    let relative = reader.nodes().next().unwrap().relative().to_path_buf();
    for model in [PinnedModel::EmbeddingGemma2, PinnedModel::BgeRerankerV2M3] {
        assert!(matches!(
            run(producer.install(
                model,
                InstallSource::Captured {
                    reader: &reader,
                    relative: &relative
                },
                &mut allowed
            )),
            Err(ProvisionError::Integrity)
        ));
        let mut denied = || Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(matches!(
            run(producer.install(model, InstallSource::Remote, &mut denied)),
            Err(ProvisionError::Admission(_))
        ));
    }
    no_stages(&producer);
    assert_eq!(get(&producer, "daemon.models"), None);
}
#[test]
fn explicit_defaults_and_recipes_use_strict_existing_schema() {
    let RuntimeSettings::EmbeddingGemma2 {
        threads,
        queue_capacity,
    } = default_runtime().unwrap()
    else {
        panic!("expected GGUF")
    };
    assert_eq!(
        threads.get(),
        u32::try_from(
            std::thread::available_parallelism()
                .unwrap()
                .get()
                .div_ceil(2)
        )
        .unwrap()
    );
    assert_eq!(queue_capacity.get(), 8);
    let explicit_threads =
        u32::try_from(std::thread::available_parallelism().unwrap().get()).unwrap();
    let explicit: RuntimeSettings = serde_json::from_value(json!({
        "backend":"gguf-embeddinggemma2","threads":explicit_threads,"queue_capacity":8
    }))
    .unwrap();
    assert!(matches!(
        explicit,
        RuntimeSettings::EmbeddingGemma2 { threads, .. } if threads.get() == explicit_threads
    ));
    for dimensions in [128, 256, 512, 768] {
        let manifest: Value =
            serde_json::from_slice(&pinned_manifest(dimensions).unwrap()).unwrap();
        assert_eq!(manifest["recipe"]["dimensions"], dimensions);
        assert_eq!(manifest["recipe"]["max_tokens"], 8192);
        assert_eq!(manifest["recipe"]["precision"], "bf16");
        assert_eq!(manifest["artifacts"][0]["minimum_bytes"], MODEL_BYTES);
    }
    assert!(pinned_manifest(384).is_err());
    for runtime in [
        json!({"backend":"onnx","native_library":"obsolete.so","intra_threads":2,"inter_threads":1,"padding":null}),
        json!({"backend":"gguf-bge-reranker-v2-m3","threads":0,"queue_capacity":8}),
        json!({"backend":"gguf-embeddinggemma2","threads":0,"queue_capacity":8}),
        json!({"backend":"gguf-embeddinggemma2","threads":2}),
        json!({"backend":"gguf-embeddinggemma2","threads":2,"queue_capacity":8,"provider":"cpu"}),
    ] {
        assert!(serde_json::from_value::<RuntimeSettings>(runtime).is_err());
    }
}

#[test]
fn pinned_rerank_manifest_uses_the_original_classifier_and_canonical_slot() {
    let manifest: Value = serde_json::from_slice(&pinned_rerank_manifest().unwrap()).unwrap();
    assert_eq!(manifest["id"], RERANK_MODEL_ID);
    assert_eq!(manifest["revision"], RERANK_MODEL_REVISION);
    assert_eq!(manifest["runtime"], "gguf-bge-reranker-v2-m3");
    assert_eq!(manifest["task"], "rerank");
    assert_eq!(manifest["artifacts"][0]["path"], RERANK_MODEL_FILE);
    assert_eq!(
        manifest["artifacts"][0]["minimum_bytes"],
        RERANK_MODEL_BYTES
    );
    assert_eq!(
        manifest["recipe"],
        json!({
            "architecture":"bert", "tokenizer":"embedded", "attention":"noncausal",
            "pooling":"rank", "pair_template":"xlm_roberta", "truncation":"longest_first_right",
            "max_tokens":8192, "precision":"f16", "score":"raw_logit", "outputs":1
        })
    );
    let RuntimeSettings::BgeRerankerV2M3 {
        threads,
        queue_capacity,
    } = default_rerank_runtime().unwrap()
    else {
        panic!("expected GGUF reranker")
    };
    assert_eq!(
        threads.get() as usize,
        std::thread::available_parallelism()
            .unwrap()
            .get()
            .div_ceil(2)
    );
    assert_eq!(queue_capacity.get(), 8);
    assert_eq!(graphit_app::models::DEFAULT_RERANK_SLOT, "rerank");
    graphit_ai::model_manifest::decode_manifest(
        &pinned_rerank_manifest().unwrap(),
        &ProvisionLimits::product().unwrap().manifest,
        &mut allowed,
    )
    .unwrap();
}

#[test]
fn published_rerank_uses_immutable_f16_source_and_admits_its_pinned_length() {
    let model = PinnedModel::BgeRerankerV2M3;
    assert_eq!(
        model.url(),
        "https://huggingface.co/sinjab/bge-reranker-v2-m3-F16-GGUF/resolve/1458e3304258a8a11773000c7d0c192abfe78291/bge-reranker-v2-m3-F16.gguf"
    );
    // The distribution commit is distinct from the original model identity.
    assert_eq!(model.revision(), RERANK_MODEL_REVISION);
    assert_eq!(model.file(), RERANK_MODEL_FILE);
    assert_eq!(model.bytes(), RERANK_MODEL_BYTES);
    let server = Server::new(
        format!("HTTP/1.1 200 OK\r\nContent-Length: {RERANK_MODEL_BYTES}\r\n\r\nabc").into_bytes(),
        false,
    );
    let mut stage = tempfile::tempfile().unwrap();
    let result = run(receive_pinned(
        model,
        &server.client(),
        &server.origin,
        &mut stage,
        &context(Duration::from_secs(5)),
        &mut allowed,
    ));
    // The complete F16 length is admitted, but the truncated transport still fails.
    assert!(matches!(
        result,
        Err(ProvisionError::Network(NetworkStreamError::Network {
            accepted_bytes: 3,
            ..
        }))
    ));
    assert_eq!(stage.metadata().unwrap().len(), 3);
}

#[test]
#[ignore = "requires GRAPHIT_BGE_RERANK_TEST_FILE with the actual pinned F16 artifact"]
fn actual_pinned_rerank_installs_reuses_and_selects_its_canonical_slot() {
    let path = PathBuf::from(
        std::env::var_os("GRAPHIT_BGE_RERANK_TEST_FILE")
            .expect("actual pinned fixture is required"),
    )
    .canonicalize()
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let original = context(Duration::from_secs(1200));
    let producer = make_producer(&temp, original.clone());
    // Exercise the download sink with the real pinned bytes over local HTTP,
    // without a second external download or an in-memory model-sized buffer.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let payload = path.clone();
    let download = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        {
            let mut request = io::BufReader::new(&mut stream);
            let mut line = String::new();
            loop {
                line.clear();
                assert_ne!(io::BufRead::read_line(&mut request, &mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                assert!(!lower.starts_with("authorization:"));
                assert!(!lower.starts_with("cookie:"));
            }
        }
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {RERANK_MODEL_BYTES}\r\n\r\n"
        )
        .unwrap();
        let mut file = fs::File::open(payload).unwrap();
        assert_eq!(
            io::copy(&mut file, &mut stream).unwrap(),
            RERANK_MODEL_BYTES
        );
    });
    let client = BoundedClient::new(
        ClientPolicy {
            origin: &origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        producer.limits().http,
    )
    .unwrap();
    let mut received = tempfile::tempfile().unwrap();
    let transferred = run(receive_pinned(
        PinnedModel::BgeRerankerV2M3,
        &client,
        &origin,
        &mut received,
        &original,
        &mut allowed,
    ));
    download.join().unwrap();
    match transferred {
        Ok(DownloadHop::Complete) => {}
        Ok(DownloadHop::Redirect(_)) => panic!("unexpected local redirect"),
        Err(error) => panic!("pinned F16 download failed: {error}"),
    }
    assert_eq!(received.metadata().unwrap().len(), RERANK_MODEL_BYTES);
    drop(received);
    let source = CapturedSourceReader::capture(
        &path,
        graphit_fs::NodeKind::File,
        &producer.limits().snapshot,
        producer.limits().read,
        original,
    )
    .unwrap();
    let relative = source.nodes().next().unwrap().relative().to_path_buf();
    let embedding = json!({"name":"embedding", "selection":{"id":MODEL_ID,"revision":MODEL_REVISION,"task":"embedding"},
        "bundle":"existing-embedding", "manifest_relative":"model.json",
        "runtime":{"backend":"gguf-embeddinggemma2","threads":2,"queue_capacity":8}});
    set(
        &producer,
        "daemon.models",
        Some(json!({"slots":[embedding.clone()]})),
    );
    set(&producer, "search.rerank", Some(json!(false)));
    let installed = run(producer.install(
        PinnedModel::BgeRerankerV2M3,
        InstallSource::Captured {
            reader: &source,
            relative: &relative,
        },
        &mut allowed,
    ))
    .unwrap();
    assert!(installed.selected);
    assert_eq!(get(&producer, "search.rerank"), Some(json!(false)));
    let selection = get(&producer, "daemon.models").unwrap();
    assert_eq!(selection["slots"][0], embedding);
    assert_eq!(selection["slots"][1]["name"], "rerank");
    assert_eq!(selection["slots"][1]["selection"]["id"], RERANK_MODEL_ID);
    assert_eq!(selection["slots"][1]["selection"]["task"], "rerank");
    assert_eq!(
        selection["slots"][1]["runtime"]["backend"],
        "gguf-bge-reranker-v2-m3"
    );
    let repeated = run(producer.install(
        PinnedModel::BgeRerankerV2M3,
        InstallSource::Remote,
        &mut allowed,
    ))
    .unwrap();
    assert!(!repeated.selected);
    assert_eq!(repeated.bundle, installed.bundle);
    assert_eq!(get(&producer, "daemon.models").unwrap(), selection);
    no_stages(&producer);
    // An explicit empty selection remains intentional for a different installation.
    set(&producer, "daemon.models", Some(json!({"slots":[]})));
    assert!(
        !run(producer.install(
            PinnedModel::BgeRerankerV2M3,
            InstallSource::Remote,
            &mut allowed
        ))
        .unwrap()
        .selected
    );
    let relative_manifest = PathBuf::from(format!(
        "{}-{}/model.json",
        RERANK_MODEL_ID, RERANK_MODEL_REVISION
    ));
    assert!(
        producer
            .select(&relative_manifest, None, &mut allowed)
            .unwrap()
    );
    let selected = get(&producer, "daemon.models").unwrap();
    assert_eq!(selected["slots"].as_array().unwrap().len(), 1);
    assert_eq!(selected["slots"][0]["name"], "rerank");
}

#[test]
#[ignore = "requires GRAPHIT_GEMMA2_TEST_FILE with the actual pinned BF16 artifact"]
fn actual_pinned_bundle_and_current_selection_matrix() {
    let source = PathBuf::from(
        std::env::var_os("GRAPHIT_GEMMA2_TEST_FILE").expect("actual pinned fixture is required"),
    )
    .canonicalize()
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let original = context(Duration::from_secs(1200));
    let producer = make_producer(&temp, original.clone());
    let reader = CapturedSourceReader::capture(
        &source,
        graphit_fs::NodeKind::File,
        &producer.limits().snapshot,
        producer.limits().read,
        original,
    )
    .unwrap();
    let relative = reader.nodes().next().unwrap().relative().to_path_buf();
    set(
        &producer,
        "daemon.models",
        Some(json!("invalid model settings")),
    );
    assert!(
        run(producer.install(
            PinnedModel::EmbeddingGemma2,
            InstallSource::Captured {
                reader: &reader,
                relative: &relative,
            },
            &mut allowed,
        ))
        .is_err()
    );
    let published = producer
        .paths()
        .models_directory()
        .join(format!("{MODEL_ID}-{MODEL_REVISION}"));
    assert_eq!(
        fs::metadata(published.join(MODEL_FILE)).unwrap().len(),
        MODEL_BYTES
    );
    assert_eq!(
        get(&producer, "daemon.models"),
        Some(json!("invalid model settings"))
    );
    no_stages(&producer);
    set(&producer, "daemon.models", None);
    let installed = run(producer.install(
        PinnedModel::EmbeddingGemma2,
        InstallSource::Remote,
        &mut allowed,
    ))
    .unwrap();
    assert!(installed.selected);
    assert_eq!(
        fs::metadata(installed.bundle.join(MODEL_FILE))
            .unwrap()
            .len(),
        MODEL_BYTES
    );
    let selected = get(&producer, "daemon.models").unwrap();
    assert_eq!(selected["slots"][0]["selection"]["id"], MODEL_ID);
    assert_eq!(
        selected["slots"][0]["selection"]["revision"],
        MODEL_REVISION
    );
    assert_eq!(
        selected["slots"][0]["runtime"]["backend"],
        "gguf-embeddinggemma2"
    );
    assert_eq!(selected["slots"][0]["runtime"]["queue_capacity"], 8);
    set(&producer, "search.rerank", Some(json!(false)));
    let rerank = json!({"name":"reranking","selection":{"id":"bge-reranker-v2-m3","revision":"custom","task":"rerank"},"bundle":"custom-rerank","manifest_relative":"model.json","runtime":{"backend":"gguf-bge-reranker-v2-m3","threads":2,"queue_capacity":8}});
    for (value, expected_selected) in [
        (None, true),
        (Some(json!({"slots":[]})), false),
        (Some(json!({"slots":[rerank.clone()]})), true),
        (Some(selected.clone()), false),
    ] {
        set(&producer, "daemon.models", value.clone());
        // A verified installed bundle is reused; this must never contact the network.
        let repeated = run(producer.install(
            PinnedModel::EmbeddingGemma2,
            InstallSource::Remote,
            &mut allowed,
        ))
        .unwrap();
        assert_eq!(repeated.selected, expected_selected);
        let current = get(&producer, "daemon.models").unwrap();
        if !expected_selected {
            assert_eq!(current, value.unwrap());
        }
        if current["slots"].as_array().unwrap().len() == 2 {
            assert_eq!(current["slots"][0], rerank);
        }
        assert_eq!(get(&producer, "search.rerank").unwrap(), json!(false));
    }
    set(&producer, "daemon.models", Some(json!({"slots":[]})));
    let name = installed.bundle.file_name().unwrap();
    for dimensions in [128, 256, 512, 768] {
        let relative =
            Path::new(name).join(graphit_app::model_provision::manifest_name(dimensions));
        producer.select(&relative, None, &mut allowed).unwrap();
        let selected = get(&producer, "daemon.models").unwrap();
        assert_eq!(
            selected["slots"][0]["manifest_relative"],
            graphit_app::model_provision::manifest_name(dimensions)
        );
    }
    let public_relative = Path::new(name).join("model-256.json");
    assert_eq!(
        graphit_app::models::run(
            graphit_app::models::Arguments {
                command: graphit_app::models::ModelCommand::Select {
                    manifest_relative: public_relative,
                },
            },
            producer.paths(),
            producer.original().clone(),
        ),
        std::process::ExitCode::SUCCESS,
    );
    let public_selection = get(&producer, "daemon.models");
    assert_eq!(
        graphit_app::models::run(
            graphit_app::models::Arguments {
                command: graphit_app::models::ModelCommand::Install {
                    model: MODEL_ID.to_owned(),
                    file: None,
                },
            },
            producer.paths(),
            producer.original().clone(),
        ),
        std::process::ExitCode::SUCCESS,
    );
    assert_eq!(get(&producer, "daemon.models"), public_selection);
    let relative = Path::new(name).join("model.json");
    let invalid: RuntimeSettings = serde_json::from_value(
        json!({"backend":"gguf-embeddinggemma2","threads":u32::MAX,"queue_capacity":8}),
    )
    .unwrap();
    let before = get(&producer, "daemon.models");
    assert!(
        producer
            .select(&relative, Some(invalid), &mut allowed)
            .is_err()
    );
    assert_eq!(get(&producer, "daemon.models"), before);
    let incompatible: RuntimeSettings = serde_json::from_value(rerank["runtime"].clone()).unwrap();
    assert!(
        producer
            .select(&relative, Some(incompatible), &mut allowed)
            .is_err()
    );
    assert_eq!(get(&producer, "daemon.models"), before);
    fs::write(installed.bundle.join("model.json"), b"conflicting content").unwrap();
    assert!(matches!(
        run(producer.install(
            PinnedModel::EmbeddingGemma2,
            InstallSource::Remote,
            &mut allowed
        )),
        Err(ProvisionError::Conflict)
    ));
    assert_eq!(
        fs::read(installed.bundle.join("model.json")).unwrap(),
        b"conflicting content"
    );
    assert_eq!(get(&producer, "daemon.models"), before);
    no_stages(&producer);
    fs::write(
        installed.bundle.join("model.json"),
        pinned_manifest(768).unwrap(),
    )
    .unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(installed.bundle.join(MODEL_FILE))
        .unwrap()
        .write_all(b"BAD!")
        .unwrap();
    assert!(matches!(
        run(producer.install(
            PinnedModel::EmbeddingGemma2,
            InstallSource::Remote,
            &mut allowed
        )),
        Err(ProvisionError::Binding(_))
    ));
    let mut prefix = [0; 4];
    fs::File::open(installed.bundle.join(MODEL_FILE))
        .unwrap()
        .read_exact(&mut prefix)
        .unwrap();
    assert_eq!(&prefix, b"BAD!");
    assert_eq!(get(&producer, "daemon.models"), before);

    let cancelled_temp = tempfile::tempdir().unwrap();
    let cancelled_context = context(Duration::from_secs(60));
    let cancelled = make_producer(&cancelled_temp, cancelled_context.clone());
    let source = CapturedSourceReader::capture(
        &source,
        graphit_fs::NodeKind::File,
        &cancelled.limits().snapshot,
        cancelled.limits().read,
        cancelled_context.clone(),
    )
    .unwrap();
    let relative = source.nodes().next().unwrap().relative().to_path_buf();
    let mut gate = || {
        let partial = fs::read_dir(cancelled.paths().models_directory())?.any(|entry| {
            let Ok(entry) = entry else { return false };
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".model-stage-")
                && fs::metadata(entry.path().join(MODEL_FILE))
                    .is_ok_and(|metadata| metadata.len() > 0)
        });
        if partial {
            cancelled_context.cancellation().cancel();
        }
        Ok::<(), io::Error>(())
    };
    assert!(matches!(
        run(cancelled.install(
            PinnedModel::EmbeddingGemma2,
            InstallSource::Captured {
                reader: &source,
                relative: &relative
            },
            &mut gate
        )),
        Err(ProvisionError::Io {
            kind: io::ErrorKind::Interrupted,
            ..
        })
    ));
    no_stages(&cancelled);
    assert!(
        !cancelled
            .paths()
            .models_directory()
            .join(format!("{MODEL_ID}-{MODEL_REVISION}"))
            .exists()
    );
}
