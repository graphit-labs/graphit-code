use super::*;
use futures_util::FutureExt;

#[test]
fn broker_services_skip_local_model_settings_even_when_they_are_invalid() {
    use graphit_auth::profile::{AuthDocument, BrokerService, ProfileLimits};
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    ConfigStore::global(&fixture.installation)
        .unwrap()
        .set(
            ConfigKey::new("daemon.models").unwrap(),
            json!({"slots": "invalid-local-settings"}),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let mut document = AuthDocument::new(
        ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        &original,
    )
    .unwrap();
    document
        .add_provider(
            "managed".into(),
            ProviderConfig {
                kind: ProviderKind::Broker(BrokerService {
                    endpoint: "https://broker.example".into(),
                    mcp_resource: None,
                    allow_anonymous: false,
                }),
                embedding: AiService::Broker,
                rerank: AiService::Broker,
                s3: S3Location {
                    bucket: None,
                    region: None,
                    endpoint: None,
                    prefix: None,
                    source: S3Source::BrokerScope,
                },
            },
            document.revision(),
            1,
            &original,
        )
        .unwrap();
    assert!(
        crate::foreground_memory_jobs::expected_selection(
            &fixture.installation,
            document.provider("managed"),
            original.snapshot(),
        )
        .unwrap()
        .is_none()
    );
    let global = ConfigSnapshot::new(
        fixture.installation.brand(),
        [ConfigStore::global(&fixture.installation)
            .unwrap()
            .load()
            .unwrap()],
    )
    .unwrap();
    assert!(
        model_api::ModelSettings::read(&global).is_err(),
        "the unused local settings really are invalid"
    );
}

fn task_only_fixture(rerank: bool) -> Fixture {
    configure_task_only(Fixture::with_services(true, rerank))
}

fn configure_task_only(mut fixture: Fixture) -> Fixture {
    // Browser policy is captured by the real owner; these tests use local
    // Runtime authority and never make a Broker request.
    fixture.configure_browser("http://127.0.0.1:9", false);
    for project in &fixture.projects {
        for module in [Module::Ast, Module::Memory, Module::Knowledge] {
            ConfigStore::project(project)
                .unwrap()
                .set(
                    module.key(),
                    json!(false),
                    Duration::from_secs(5),
                    &Cancellation::default(),
                )
                .unwrap();
        }
    }
    fixture
}

#[tokio::test]
async fn remote_embedding_keeps_resources_after_last_local_rerank_slot_retires() {
    let fixture = configure_task_only(Fixture::with_ai_services(
        AiService::Direct {
            protocol: graphit_auth::profile::DirectProtocol::Cohere,
            endpoint: "http://127.0.0.1:9".into(),
            model: "declared-remote-embedding".into(),
            model_revision: "r1".into(),
            recipe: "remote-document-query-v1".into(),
            dimensions: 128,
            request_dimensions: None,
            batch_endpoint: None,
        },
        AiService::Local(LocalExecution {
            device: LocalDevice::Cpu,
            device_id: 0,
        }),
    ));
    fixture.install_rerank_model().await;
    let cancellation = Cancellation::default();
    ConfigStore::global(&fixture.installation)
        .unwrap()
        .set(
            ConfigKey::new("search.rerank").unwrap(),
            json!(false),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    set_embedding(&fixture, true, &cancellation);
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let mut foreground = start_task_source(&fixture, &original);
    let exercise = std::panic::AssertUnwindSafe(async {
        foreground.project_control.as_mut().unwrap().environment = fixture.environment();
        foreground
            .refresh_model_selection(&fixture.installation, &fixture.config, &[], &original)
            .await
            .unwrap();
        let (response, _received) = tokio::sync::oneshot::channel();
        let pending = crate::foreground_project_control::PendingLocalModel {
            original: Arc::clone(original.snapshot()),
            provider: "cpu".into(),
            purpose: ServicePurpose::Rerank,
            execution: LocalExecution {
                device: LocalDevice::Cpu,
                device_id: 0,
            },
            response,
        };
        let (_, _, address) = foreground.ensure_local_model(&pending).await.unwrap();
        ConfigStore::global(&fixture.installation)
            .unwrap()
            .set(
                ConfigKey::new("daemon.models").unwrap(),
                json!({"slots": []}),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
        foreground
            .refresh_model_selection(&fixture.installation, &fixture.config, &[], &original)
            .await
            .unwrap();
        let jobs = foreground.memory_jobs.as_ref().unwrap();
        assert!(jobs.retirement_complete());
        assert!(foreground.requested_local_rerank.is_none());
        assert!(TcpStream::connect(address).await.is_err());
        let resources = foreground
            .server
            .as_ref()
            .unwrap()
            .semantic_model()
            .unwrap()
            .unwrap();
        assert!(
            resources.local.is_none(),
            "remote resources remain selected"
        );
        assert!(
            foreground
                .server
                .as_ref()
                .unwrap()
                .task_server()
                .unwrap()
                .memory_model()
                .unwrap()
                .is_some()
        );
        drop(resources);
    })
    .catch_unwind()
    .await;
    let cleanup = foreground.shutdown().await;
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    cleanup.unwrap();
}

fn set_embedding(fixture: &Fixture, enabled: bool, cancellation: &Cancellation) {
    ConfigStore::global(&fixture.installation)
        .unwrap()
        .set(
            ConfigKey::new("modules.embedding").unwrap(),
            json!(enabled),
            Duration::from_secs(5),
            cancellation,
        )
        .unwrap();
}

fn start_task_source(fixture: &Fixture, original: &RequestContext<'_>) -> ForegroundSource {
    let provider = active_provider(&fixture.installation, &fixture.config, original)
        .unwrap()
        .unwrap();
    let foreground = ForegroundSource::start_with_inventory(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        original,
        Some(&provider),
    )
    .unwrap();
    assert!(foreground.tasks.is_some(), "actual Browser Task owner");
    foreground
}

#[tokio::test]
async fn global_user_memory_gate_is_independent_of_task_browser_and_project_embedding() {
    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    let global = ConfigStore::global(&fixture.installation).unwrap();
    for (module, enabled) in [
        (Module::Task, false),
        (Module::Memory, true),
        (Module::Embedding, true),
    ] {
        global
            .set(
                module.key(),
                json!(enabled),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    for project in &fixture.projects {
        ConfigStore::project(project)
            .unwrap()
            .set(
                Module::Embedding.key(),
                json!(false),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    for override_source in [
        None,
        Some(ConfigSource::Environment),
        Some(ConfigSource::Command),
    ] {
        let base = ConfigSnapshot::new(
            fixture.installation.brand(),
            override_source.map(|source| {
                ConfigDocument::new(
                    fixture.installation.brand(),
                    source,
                    [(Module::Embedding.key(), json!(false))],
                )
                .unwrap()
            }),
        )
        .unwrap();
        let provider = active_provider(&fixture.installation, &base, &original).unwrap();
        let foreground = ForegroundSource::start_with_inventory(
            &fixture.installation,
            &fixture.inputs(),
            0,
            &base,
            NativeBinding::KiroV3,
            &original,
            provider.as_ref(),
        )
        .unwrap();
        assert!(
            foreground.tasks.is_none(),
            "Source serves USER without Browser"
        );
        let modules = foreground
            .server
            .as_ref()
            .unwrap()
            .project_modules(&original)
            .unwrap();
        assert!(modules.iter().all(|m| !m.enabled(Module::Embedding)));
        assert_eq!(
            foreground
                .capture_model_selection(
                    &fixture.installation,
                    &base,
                    provider.as_ref(),
                    &original,
                    &modules,
                )
                .unwrap()
                .is_some(),
            override_source.is_none()
        );
        foreground.shutdown().await.unwrap();
    }
    global
        .set(
            Module::Memory.key(),
            json!(false),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    global
        .set(
            Module::Knowledge.key(),
            json!(true),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    let provider = active_provider(&fixture.installation, &fixture.config, &original).unwrap();
    let foreground = ForegroundSource::start_with_inventory(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        &original,
        provider.as_ref(),
    )
    .unwrap();
    let modules = foreground
        .server
        .as_ref()
        .unwrap()
        .project_modules(&original)
        .unwrap();
    assert!(
        !foreground
            .embedding_requested(
                &fixture.installation,
                &fixture.config,
                provider.as_ref(),
                &original,
                &modules,
            )
            .unwrap(),
        "Global Knowledge alone does not invent a consumer"
    );
    foreground.shutdown().await.unwrap();
}

async fn assert_task_create_get_and_fts(
    foreground: &ForegroundSource,
    original: &RequestContext<'_>,
) {
    use crate::task_server::request::{Credential, Origin};
    let tasks = foreground.server.as_ref().unwrap().task_server().unwrap();
    let runtime = foreground
        .auth
        .runtime()
        .check(original.snapshot())
        .unwrap();
    let raw = zeroize::Zeroizing::new(
        std::str::from_utf8(runtime.secret(original.snapshot()).unwrap())
            .unwrap()
            .to_owned(),
    );
    let call = |operation: &'static str, input: serde_json::Value, write| {
        let tasks = tasks.clone();
        let credential = Credential {
            role_binding: None,
            bearer: raw.clone(),
            origin: Origin::Runtime,
        };
        async move {
            tasks
                .call(
                    serde_json::from_value(json!({"input": input})).unwrap(),
                    operation,
                    write,
                    credential,
                    CancellationToken::new(),
                )
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "Task {operation}: {}",
                        serde_json::to_string(&error).unwrap()
                    )
                })
        }
    };
    let session = call(
        "create_session",
        json!({"idempotency_key":"embedding-off-session", "title":"Lexical Task", "description":"Embedding disabled", "strategy":"Use native records"}),
        true,
    )
    .await;
    let created = call(
        "create_task",
        json!({"idempotency_key":"embedding-off-task", "session_id": session["result"]["id"],
            "title":"LexicalOrchid", "description":"Native lexical journey with Embedding disabled", "task_type":"TASK", "priority":1,
            "checks":[{"kind":"acceptance", "text":"Retain the record"}, {"kind":"test", "text":"Find its lexical title"}]}),
        true,
    )
    .await;
    let id = created["result"]["specification"]["id"].as_str().unwrap();
    let read = call("get_task", json!({"id":id}), false).await;
    assert_eq!(read["result"]["task"]["specification"]["id"], id);
    let search = call(
        "search",
        json!({"text":"LexicalOrchid", "filter":{}, "page_size":5, "top_k":5}),
        false,
    )
    .await;
    assert!(search["result"].to_string().contains(id));
}

#[tokio::test]
async fn task_only_embedding_gate_respects_effective_modules_at_capture_and_refresh() {
    let fixture = task_only_fixture(true);
    fixture.install_model();
    let cancellation = Cancellation::default();
    set_embedding(&fixture, false, &cancellation);
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let mut foreground = start_task_source(&fixture, &original);
    let provider = active_provider(&fixture.installation, &fixture.config, &original).unwrap();
    let server = foreground.server.as_ref().unwrap().clone();
    let modules = server.project_modules(&original).unwrap();
    assert!(modules.iter().all(|m| m.enabled(Module::Task)));
    assert!(
        foreground
            .capture_model_selection(
                &fixture.installation,
                &fixture.config,
                provider.as_ref(),
                &original,
                &modules,
            )
            .unwrap()
            .is_none()
    );
    assert!(
        foreground
            .create_memory_jobs(
                &fixture.installation,
                &fixture.config,
                provider.as_ref(),
                &original,
                &modules,
            )
            .unwrap()
            .is_none()
    );
    // Configured local Embedding and Rerank are not acquired by refresh.
    foreground
        .refresh_model_selection(&fixture.installation, &fixture.config, &[], &original)
        .await
        .unwrap();
    assert!(foreground.memory_jobs.is_none());
    assert!(foreground.requested_local_rerank.is_none());
    assert!(server.semantic_model().unwrap().is_none());
    assert!(
        server
            .task_server()
            .unwrap()
            .memory_model()
            .unwrap()
            .is_none()
    );
    assert_task_create_get_and_fts(&foreground, &original).await;
    assert!(foreground.memory_jobs.is_none());

    set_embedding(&fixture, true, &cancellation);
    let modules = server.project_modules(&original).unwrap();
    assert!(
        foreground
            .capture_model_selection(
                &fixture.installation,
                &fixture.config,
                provider.as_ref(),
                &original,
                &modules,
            )
            .unwrap()
            .is_some(),
        "Task and the enabled global USER Memory consumer both require embeddings"
    );
    foreground.shutdown().await.unwrap();
}

#[tokio::test]
async fn task_without_project_uses_fresh_global_embedding_gate_and_command_override() {
    let mut fixture = task_only_fixture(false);
    fixture.projects.clear();
    fixture.install_model();
    let cancellation = Cancellation::default();
    set_embedding(&fixture, false, &cancellation);
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = start_task_source(&fixture, &original);
    let provider = active_provider(&fixture.installation, &fixture.config, &original).unwrap();
    for enabled in [false, true, false] {
        set_embedding(&fixture, enabled, &cancellation);
        assert_eq!(
            foreground
                .embedding_requested(
                    &fixture.installation,
                    &fixture.config,
                    provider.as_ref(),
                    &original,
                    &[],
                )
                .unwrap(),
            enabled
        );
    }
    set_embedding(&fixture, true, &cancellation);
    let command = ConfigSnapshot::new(
        fixture.installation.brand(),
        [ConfigDocument::new(
            fixture.installation.brand(),
            ConfigSource::Command,
            [(ConfigKey::new("modules.embedding").unwrap(), json!(false))],
        )
        .unwrap()],
    )
    .unwrap();
    assert!(
        !foreground
            .embedding_requested(
                &fixture.installation,
                &command,
                provider.as_ref(),
                &original,
                &[],
            )
            .unwrap()
    );
    foreground.shutdown().await.unwrap();
}

#[tokio::test]
async fn task_only_embedding_off_retires_actual_child_and_semantic_resources() {
    let fixture = task_only_fixture(false);
    fixture.install_model();
    let cancellation = Cancellation::default();
    set_embedding(&fixture, true, &cancellation);
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let mut foreground = start_task_source(&fixture, &original);
    let exercise = std::panic::AssertUnwindSafe(async {
        foreground
            .refresh_model_selection(
                &fixture.installation,
                &fixture.config,
                &fixture.environment(),
                &original,
            )
            .await
            .unwrap();
        let server = foreground.server.as_ref().unwrap().clone();
        let resources = server.semantic_model().unwrap().unwrap();
        let address = resources.local.as_ref().unwrap().address;
        let borrowed = Arc::downgrade(&resources);
        assert!(TcpStream::connect(address).await.is_ok());
        drop(resources);
        set_embedding(&fixture, false, &cancellation);
        foreground
            .refresh_model_selection(&fixture.installation, &fixture.config, &[], &original)
            .await
            .unwrap();
        assert!(foreground.memory_jobs.is_none());
        assert!(server.semantic_model().unwrap().is_none());
        assert!(
            server
                .task_server()
                .unwrap()
                .memory_model()
                .unwrap()
                .is_none()
        );
        assert!(borrowed.upgrade().is_none());
        assert!(TcpStream::connect(address).await.is_err());
        assert!(foreground.tasks.is_some());
    })
    .catch_unwind()
    .await;
    let cleanup = foreground.shutdown().await;
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    cleanup.unwrap();
}

#[tokio::test]
async fn acquired_rerank_survives_embedding_off_and_retires_missing_slot_or_provider() {
    let fixture = task_only_fixture(true);
    fixture.install_rerank_model().await;
    let cancellation = Cancellation::default();
    ConfigStore::global(&fixture.installation)
        .unwrap()
        .set(
            ConfigKey::new("search.rerank").unwrap(),
            json!(false),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    set_embedding(&fixture, false, &cancellation);
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let mut foreground = start_task_source(&fixture, &original);
    let exercise = std::panic::AssertUnwindSafe(async {
        foreground.project_control.as_mut().unwrap().environment = fixture.environment();
        foreground
            .refresh_model_selection(&fixture.installation, &fixture.config, &[], &original)
            .await
            .unwrap();
        assert!(
            foreground.memory_jobs.is_none(),
            "configuration alone is not demand"
        );
        let (response, _received) = tokio::sync::oneshot::channel();
        let pending = crate::foreground_project_control::PendingLocalModel {
            original: Arc::clone(original.snapshot()),
            provider: "cpu".into(),
            purpose: ServicePurpose::Rerank,
            execution: LocalExecution {
                device: LocalDevice::Cpu,
                device_id: 0,
            },
            response,
        };
        let (_, expected, address) = foreground.ensure_local_model(&pending).await.unwrap();
        assert_eq!(
            foreground.requested_local_rerank,
            Some(("cpu".into(), expected.config_id()))
        );
        foreground
            .refresh_model_selection(&fixture.installation, &fixture.config, &[], &original)
            .await
            .unwrap();
        assert_eq!(
            foreground
                .memory_jobs
                .as_ref()
                .unwrap()
                .child
                .as_ref()
                .unwrap()
                .address(),
            address
        );
        assert!(
            foreground
                .server
                .as_ref()
                .unwrap()
                .semantic_model()
                .unwrap()
                .is_none()
        );
        foreground.retire_unused_model(&original).await.unwrap();
        assert!(TcpStream::connect(address).await.is_ok());
        let global = ConfigStore::global(&fixture.installation).unwrap();
        let snapshot =
            ConfigSnapshot::new(fixture.installation.brand(), [global.load().unwrap()]).unwrap();
        let settings = model_api::ModelSettings::read(&snapshot).unwrap().unwrap();
        let mut missing_bundle = serde_json::to_value(&settings).unwrap();
        missing_bundle["slots"][0]["bundle"] =
            json!(fixture.installation.root().join("absent-rerank-bundle"));
        global
            .set(
                ConfigKey::new("daemon.models").unwrap(),
                missing_bundle,
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
        assert!(
            foreground
                .refresh_model_selection(&fixture.installation, &fixture.config, &[], &original)
                .await
                .is_err(),
            "a declared slot with missing bundle remains an error"
        );
        assert!(foreground.requested_local_rerank.is_some());
        global
            .set(
                ConfigKey::new("daemon.models").unwrap(),
                json!({"slots": []}),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
        foreground
            .refresh_model_selection(&fixture.installation, &fixture.config, &[], &original)
            .await
            .unwrap();
        assert!(foreground.memory_jobs.is_none());
        assert!(foreground.requested_local_rerank.is_none());
        assert!(TcpStream::connect(address).await.is_err());
        assert_eq!(
            foreground
                .ensure_local_model(&pending)
                .await
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::NotFound,
            "an explicit acquisition still requires a declared slot"
        );
        global
            .set(
                ConfigKey::new("daemon.models").unwrap(),
                serde_json::to_value(settings).unwrap(),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
        let (_, _, address) = foreground.ensure_local_model(&pending).await.unwrap();
        fixture.remove_provider(&original);
        foreground.retire_unused_model(&original).await.unwrap();
        assert!(foreground.memory_jobs.is_none());
        assert!(foreground.requested_local_rerank.is_none());
        assert!(TcpStream::connect(address).await.is_err());
    })
    .catch_unwind()
    .await;
    let cleanup = foreground.shutdown().await;
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
    cleanup.unwrap();
}
