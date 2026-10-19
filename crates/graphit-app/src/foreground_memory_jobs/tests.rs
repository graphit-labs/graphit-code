#[path = "tests/index_concurrency.rs"]
mod index_concurrency;
#[path = "tests/model_gate.rs"]
mod model_gate;
#[path = "tests/model_reload.rs"]
mod model_reload;
#[path = "tests/native_gguf_source.rs"]
mod native_gguf_source;
#[path = "tests/task_local_gguf.rs"]
mod task_local_gguf;
#[path = "tests/task_semantic.rs"]
mod task_semantic;
pub(crate) use super::dream_tests::dream_shutdown_fixture;
use super::*;
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput,
    profile::{
        AiService, LocalCredentials, LocalDevice, LocalExecution, ProfileCredentials, ProfileData,
        ProfileExpectation, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
    },
};
use graphit_core::{
    artifact_store::CapturedSourceReader,
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSource, ConfigStore},
    project_identity::ProjectMetadata,
};
use graphit_daemon::model_child::ChildOptions;
use graphit_daemon::model_service as model_api;
use graphit_daemon::model_service::{EmbedInput, MemoryEmbeddingModel, ModelServiceClient};
use graphit_records::memory_store::{
    Memory, MemoryListFilter, MemorySemanticSearchOptions, MemorySemanticSearchRead,
    prepare_memory_embedding_document,
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{
        CreateMemory, MemoryStoreReadLimits, MemoryStoreWriteLimits, NativeMemoryStore,
    },
};
use secrecy::SecretString;
use serde_json::json;
use std::{
    fs,
    num::{NonZeroU32, NonZeroUsize},
    path::Path,
};
use tokio::net::TcpStream;

fn read_limits(foreground: &ForegroundSource) -> MemoryStoreReadLimits {
    let content = foreground.policy.content();
    MemoryStoreReadLimits {
        native: foreground.native,
        batch_rows: NonZeroU32::new(16).unwrap(),
        max_query_bytes: NonZeroUsize::new(content.max_output_bytes).unwrap(),
    }
}

fn job_resources(
    foreground: &ForegroundSource,
) -> crate::foreground_memory_jobs::MemoryJobResources {
    let resources = foreground
        .memory_jobs
        .as_ref()
        .unwrap()
        .resources(foreground.scope, foreground.native, NativeBinding::KiroV3)
        .unwrap()
        .unwrap();
    match Arc::try_unwrap(resources) {
        Ok(resources) => resources,
        Err(_) => panic!("fresh resource observation must have one borrower"),
    }
}

#[tokio::test]
async fn foreground_memory_jobs_no_corpus_cancel_and_foreign_checkout_fail_closed() {
    use graphit_daemon::{model_service::MemoryEmbeddingIndexRead, supervisor::ProjectContext};
    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let mut foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &request,
    )
    .await
    .unwrap();
    let mut resources = job_resources(&foreground);
    // The RPC policy must not become a deadline for the background corpus.
    resources.policy = HubResourcePolicy::resolve(
        &ConfigSnapshot::new(
            fixture.installation.brand(),
            [ConfigDocument::new(
                fixture.installation.brand(),
                ConfigSource::Command,
                [(
                    ConfigKey::new("hub.limits.request.deadline_ms").unwrap(),
                    json!(1),
                )],
            )
            .unwrap()],
        )
        .unwrap(),
    )
    .unwrap();
    foreground.supervisors.shutdown().await.unwrap();
    foreground
        .memory_jobs
        .as_mut()
        .unwrap()
        .shutdown()
        .await
        .unwrap();
    assert!(
        TcpStream::connect(resources.local.as_ref().unwrap().address)
            .await
            .is_err()
    );
    // The actual endpoint is closed: NoCorpus success proves that this job did not
    // open a model connection. Runtime authentication stays live until Foreground shutdown.
    let projects = foreground.projects.as_ref().unwrap();
    for (paths, id, version) in projects.iter() {
        let context = ProjectContext {
            id: *id,
            version: version.clone(),
        };
        assert!(matches!(
            resources
                .index(paths, &context, &cancellation)
                .await
                .unwrap(),
            MemoryEmbeddingIndexRead::NoCorpus
        ));
    }
    let foreign = ProjectContext {
        id: projects[1].1,
        version: projects[1].2.clone(),
    };
    {
        let cancelled = Cancellation::default();
        cancelled.cancel();
        let error = resources
            .index(&projects[1].0, &foreign, &cancelled)
            .await
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    }
    assert_eq!(
        resources
            .index(&projects[0].0, &foreign, &cancellation)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let stopped = Cancellation::default();
    stopped.cancel();
    assert_eq!(
        resources
            .index(&projects[1].0, &foreign, &stopped)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    fixture.remove_provider(&request);
    assert!(
        resources
            .index(&projects[1].0, &foreign, &cancellation)
            .await
            .is_err()
    );
    foreground.shutdown().await.unwrap();
}

#[tokio::test]
async fn native_mcp_stdio_startup_reuses_live_checkout_and_canonical_selection() {
    use crate::foreground_project_control::{ProjectCommand, send};
    use graphit_core::native_path::NativePath;
    use rmcp::ClientServiceExt;
    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    for project in &fixture.projects {
        ConfigStore::project(project)
            .unwrap()
            .set(
                ConfigKey::new("modules.ast").unwrap(),
                json!(false),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    fs::write(
        fixture.projects[0].root().join("source.odd-extension"),
        "native source through canonical root\n",
    )
    .unwrap();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let inputs: Vec<_> = fixture
        .projects
        .iter()
        .map(|project| ProjectBootstrapInput {
            project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        })
        .collect();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &inputs,
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &original,
    )
    .await
    .unwrap();
    let (project, id, version) = foreground.projects.as_ref().unwrap()[0].clone();
    let address = foreground.address().unwrap();
    let native = foreground.native;
    let content = fixture.policy.content();
    let service = Cancellation::default();
    let run_signal = service.clone();
    let running = tokio::spawn(async move { foreground.run(&run_signal).await });
    let command = ProjectCommand::MaintainCheckout {
        project_id: id,
        project_dir: NativePath::new(project.root().to_owned()).unwrap(),
        version: version.version().to_owned(),
        detached_base: String::new(),
        select: false,
    };
    let http = reqwest::Client::new();
    let (status, observation) = send(
        &fixture.installation,
        address,
        &http,
        (native, content),
        &command,
        &original,
    )
    .await
    .unwrap();
    assert!(status.is_success(), "{observation}");
    assert_eq!(observation["phase"], "unchanged");
    assert!(observation["inventory_registered"].is_null());
    assert!(observation["trusted_root"].is_null());
    assert!(observation["broker_effect"].is_null());
    let (client_io, server_io) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(server_io);
    let installation = fixture.installation.clone();
    let policy = fixture.policy.clone();
    let proxy_signal = Cancellation::default();
    let proxy_root = project.root().join(".");
    let selected_version = version.version().to_owned();
    let stdio = tokio::spawn(async move {
        let original = policy
            .request_context(Instant::now(), &proxy_signal)
            .unwrap();
        crate::source_entry::stdio(
            &installation,
            &http,
            (native, policy.content(), Duration::from_secs(5)),
            Some((&proxy_root, Some(&selected_version))),
            &original,
            &proxy_signal,
            read,
            write,
        )
        .await
    });
    let client = ()
        .serve_with_lifecycle(
            client_io,
            rmcp::service::ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap();
    let tools = client.peer().list_all_tools().await.unwrap();
    assert!(tools.iter().any(|tool| tool.name == "graphit_ast_source"));
    let answer = client
        .peer()
        .call_tool(
            rmcp::model::CallToolRequestParams::new("graphit_ast_source").with_arguments(
                json!({"path":"source.odd-extension"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(answer.is_error, Some(true), "{answer:?}");
    let source: String = answer
        .content
        .iter()
        .map(|content| {
            content
                .as_text()
                .expect("Source returns text")
                .text
                .as_str()
        })
        .collect();
    assert_eq!(source, "native source through canonical root\n");
    client.cancel().await.unwrap();
    stdio.await.unwrap().unwrap();
    service.cancel();
    running.await.unwrap().unwrap();
}

fn sdk_task_role(
    auth: &graphit_daemon::auth_startup::DaemonAuth,
    selected: &(
        ProjectPaths,
        graphit_core::project_identity::ProjectId,
        graphit_core::version_context::VersionContext,
    ),
    scope: ScopeLimits,
    original: &RequestContext<'_>,
) -> (
    graphit_auth::AdminSessionHandle,
    graphit_auth::RunHandle,
    graphit_hub::native::RoleCredentialReference,
    graphit_auth::RoleBearer,
) {
    use graphit_auth::{IssuerLimits, RoleDefinition};
    let issuer = auth.issuer();
    let key = auth.runtime().check(original.snapshot()).unwrap();
    let raw = std::str::from_utf8(key.secret(original.snapshot()).unwrap()).unwrap();
    let caller = issuer.authenticate_runtime(raw, original).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let permissions = [
        graphit_auth::Permission::Read(Module::Task),
        graphit_auth::Permission::WriteOwnTask,
    ];
    let definition = RoleDefinition::tool_owned(
        graphit_hub::native::Role::Scribe,
        selected.0.brand(),
        NativeBinding::KiroV3,
        &permissions,
        [37; 32],
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
    )
    .unwrap();
    let reference = issuer
        .configure_tool_owned_definition(&parent, definition)
        .unwrap();
    let captured =
        CapturedPublicationScope::project(&selected.0, selected.1, scope, original.snapshot())
            .unwrap();
    let record = captured.binding().unwrap();
    let input = ScopeInput {
        installation: auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version: &selected.2,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let run = issuer
        .start_run(&parent, &input, Duration::from_secs(120))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &permissions, &input)
        .unwrap();
    (session, run, reference, bearer)
}

async fn sdk_task_client(
    address: std::net::SocketAddr,
    role: &(
        graphit_auth::AdminSessionHandle,
        graphit_auth::RunHandle,
        graphit_hub::native::RoleCredentialReference,
        graphit_auth::RoleBearer,
    ),
    content: graphit_hub::native::ContentLimits,
    original: &RequestContext<'_>,
) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    use rmcp::{
        ClientServiceExt,
        transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        },
    };
    let mut headers = std::collections::HashMap::new();
    let mut bearer =
        axum::http::HeaderValue::from_str(&format!("Bearer {}", role.3.expose_for_transport()))
            .unwrap();
    bearer.set_sensitive(true);
    headers.insert(axum::http::header::AUTHORIZATION, bearer);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_static("role"),
    );
    let reference = zeroize::Zeroizing::new(
        String::from_utf8(role.2.encode(content.max_input_bytes, original).unwrap()).unwrap(),
    );
    let mut header = axum::http::HeaderValue::from_str(&reference).unwrap();
    header.set_sensitive(true);
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-role-reference"),
        header,
    );
    ().serve_with_lifecycle(
        StreamableHttpClientTransport::with_client(
            reqwest::Client::new(),
            StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/roles/mcp"))
                .custom_headers(headers),
        ),
        rmcp::service::ClientLifecycleMode::Discover {
            preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
        },
    )
    .await
    .unwrap()
}

async fn live_task_call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &str,
    input: serde_json::Value,
    error: bool,
) -> serde_json::Value {
    let result = client
        .peer()
        .call_tool(
            rmcp::model::CallToolRequestParams::new(name.to_owned())
                .with_arguments(input.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap();
    assert_eq!(result.is_error, Some(error), "{name}: {value}");
    value
}

#[tokio::test]
async fn live_role_task_consumer_follows_switch_and_retirement() {
    use crate::foreground_project_control::{ProjectCommand, send};
    use graphit_core::{
        git_checkout::GitCheckout, native_path::NativePath, version_context::VersionContext,
    };
    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    for project in &fixture.projects {
        for key in ["modules.ast", "modules.knowledge"] {
            ConfigStore::project(project)
                .unwrap()
                .set(
                    ConfigKey::new(key).unwrap(),
                    json!(false),
                    Duration::from_secs(5),
                    &cancellation,
                )
                .unwrap();
        }
    }
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &original,
    )
    .await
    .unwrap();
    let auth = Arc::clone(&foreground.auth);
    let projects = Arc::clone(foreground.projects.as_ref().unwrap());
    let address = foreground.address().unwrap();
    let scope = foreground.scope;
    let native = foreground.native;
    let service = Cancellation::default();
    let run_signal = service.clone();
    let running = tokio::spawn(async move { foreground.run(&run_signal).await });
    let role_main = sdk_task_role(&auth, &projects[0], scope, &original);
    let main = sdk_task_client(address, &role_main, fixture.policy.content(), &original).await;
    let main_view = live_task_call(&main, "graphit_task_query", json!({"input":{"selection":{"table":"tasks"},"columns":["id","status"],"page_size":1,"top_k":1}}), false).await;
    assert_eq!(
        main_view,
        json!({"operation":"query","result":{"state":"no_corpus"}})
    );
    let http = reqwest::Client::new();
    let (status, observed) = send(
        &fixture.installation,
        address,
        &http,
        (native, fixture.policy.content()),
        &ProjectCommand::MaintainCheckout {
            project_id: projects[0].1,
            project_dir: NativePath::new(projects[0].0.root().to_owned()).unwrap(),
            version: "feature/live".into(),
            detached_base: String::new(),
            select: true,
        },
        &original,
    )
    .await
    .unwrap();
    assert!(status.is_success(), "{observed}");
    let stale = live_task_call(
        &main,
        "graphit_task_query",
        json!({"selection":{"project_id":projects[0].1,"project_version":"main"},"input":{"selection":{"table":"tasks"},"columns":["id","status"],"page_size":1,"top_k":1}}),
        true,
    )
    .await;
    assert_eq!(stale["code"], "selection_unavailable");
    let checkout =
        GitCheckout::inspect(projects[0].0.root(), Duration::from_secs(5), &cancellation).unwrap();
    let version = VersionContext::select(
        &projects[0].0,
        checkout.as_checkout(),
        "feature/live",
        None,
        "",
    )
    .unwrap();
    let selected = (projects[0].0.clone(), projects[0].1, version);
    let role_feature = sdk_task_role(&auth, &selected, scope, &original);
    let feature =
        sdk_task_client(address, &role_feature, fixture.policy.content(), &original).await;
    // The signed Role scope is feature/live. A stale main default would fail Auth.
    let feature_view = live_task_call(&feature, "graphit_task_query", json!({"input":{"selection":{"table":"tasks"},"columns":["id","status"],"page_size":1,"top_k":1}}), false).await;
    assert_eq!(
        feature_view,
        json!({"operation":"query","result":{"state":"no_corpus"}})
    );
    let (status, observed) = send(
        &fixture.installation,
        address,
        &http,
        (native, fixture.policy.content()),
        &ProjectCommand::RetireCheckout {
            project_id: projects[0].1,
            project_dir: NativePath::new(projects[0].0.root().to_owned()).unwrap(),
        },
        &original,
    )
    .await
    .unwrap();
    assert!(status.is_success(), "{observed}");
    let retired = live_task_call(
        &feature,
        "graphit_task_query",
        json!({"selection":{"project_id":projects[0].1},"input":{"selection":{"table":"tasks"},"columns":["id","status"],"page_size":1,"top_k":1}}),
        true,
    )
    .await;
    assert_eq!(retired["code"], "selection_unavailable");
    let role_sibling = sdk_task_role(&auth, &projects[1], scope, &original);
    let sibling =
        sdk_task_client(address, &role_sibling, fixture.policy.content(), &original).await;
    // This credential is scoped to the sibling, not the retired checkout.
    let sibling_view = live_task_call(&sibling, "graphit_task_query", json!({"input":{"selection":{"table":"tasks"},"columns":["id","status"],"page_size":1,"top_k":1}}), false).await;
    assert_eq!(
        sibling_view,
        json!({"operation":"query","result":{"state":"no_corpus"}})
    );
    for client in [main, feature, sibling] {
        client.cancel().await.unwrap();
    }
    let issuer = auth.issuer();
    let key = auth.runtime().check(original.snapshot()).unwrap();
    let raw = std::str::from_utf8(key.secret(original.snapshot()).unwrap()).unwrap();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    for role in [role_main, role_feature, role_sibling] {
        let parent = issuer.admin_parent(&caller, &role.0).unwrap();
        issuer.close_run(&parent, &role.1).unwrap();
    }
    service.cancel();
    running.await.unwrap().unwrap();
}

async fn assert_indexed(foreground: &ForegroundSource, index: usize, memory: &Memory) {
    let cancellation = Cancellation::default();
    let request = foreground
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let projects = foreground.projects.as_ref().unwrap();
    let (paths, id, version) = &projects[index];
    let installation = InstallationPaths::resolve(
        paths.brand().clone(),
        paths.root(),
        None,
        Some(foreground.auth.installation().live.owner_root()),
    )
    .unwrap();
    let provider = active_provider(
        &installation,
        &ConfigSnapshot::new(paths.brand(), []).unwrap(),
        &request,
    )
    .unwrap();
    let expected = crate::foreground_memory_jobs::expected_model(
        &installation,
        provider.as_ref(),
        request.snapshot(),
        &foreground
            .server
            .as_ref()
            .unwrap()
            .project_modules(&request)
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    let captured = CapturedPublicationScope::project(
        paths,
        *id,
        ScopeLimits::new(16, 65536, 256).unwrap(),
        request.snapshot(),
    )
    .unwrap();
    let record = captured.binding().unwrap();
    let scope = ScopeInput {
        installation: foreground.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let runtime = foreground.auth.runtime().check(request.snapshot()).unwrap();
    let caller = foreground
        .auth
        .issuer()
        .authenticate_runtime(
            std::str::from_utf8(runtime.secret(request.snapshot()).unwrap()).unwrap(),
            &request,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(foreground.auth.issuer());
    let read = authority
        .admit_authenticated_read(&caller, &request, scope, ProjectReadModule::Memory)
        .unwrap();
    let mut service = foreground
        .auth
        .issuer()
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = service.local().unwrap();
    let client = ModelServiceClient::new(ChildOptions::standard().unwrap().rpc).unwrap();
    let model = MemoryEmbeddingModel::new(
        request.snapshot(),
        crate::foreground_memory_jobs::MemoryJobResources::memory_identity(
            &crate::embedding_service::EmbeddingMetadata::local(&expected).unwrap(),
        )
        .unwrap(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    let document = prepare_memory_embedding_document(memory, model.identity()).unwrap();
    let address = foreground
        .memory_jobs
        .as_ref()
        .unwrap()
        .child
        .as_ref()
        .unwrap()
        .address();
    let output = client
        .embed(
            &lease,
            foreground.auth.runtime(),
            &expected,
            graphit_daemon::model_service::EmbedOperation {
                input: EmbedInput::Document,
                texts: vec![document.into_text()],
                deadline: request.snapshot().required_deadline().unwrap(),
            },
            TcpStream::connect(address),
        )
        .await
        .unwrap();
    assert_eq!(output.dimensions, 128);
    assert_eq!(output.config_id, expected.config_id());
    let vector = &output.vectors[0];
    assert!(vector.iter().all(|n| n.is_finite()));
    let found = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let result = NativeMemoryStore::search_semantic(
                request.snapshot(),
                &read,
                model.identity().clone(),
                vector,
                MemorySemanticSearchOptions {
                    filter: MemoryListFilter::default(),
                    include_deleted_history: false,
                    top_k: NonZeroUsize::new(8).unwrap(),
                },
                read_limits(foreground),
            )
            .await
            .unwrap();
            if let MemorySemanticSearchRead::Published {
                generation, hits, ..
            } = result
                && let Some(hit) = hits.iter().find(|hit| hit.current.id == memory.id)
            {
                assert_eq!(generation.project_id, *id);
                assert_eq!(generation.project_version, version.version());
                assert_eq!(hit.current.revision, memory.revision);
                assert!(hit.distance.abs() < 1e-4);
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;
    found.expect("foreground initial rescan did not publish actual Memory vectors");
}

async fn create_memory(foreground: &ForegroundSource, index: usize, key: &str) -> Memory {
    let cancellation = Cancellation::default();
    let request = foreground
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let (paths, id, version) = &foreground.projects.as_ref().unwrap()[index];
    let captured =
        CapturedPublicationScope::project(paths, *id, foreground.scope, request.snapshot())
            .unwrap();
    let record = captured.binding().unwrap();
    let scope = ScopeInput {
        installation: foreground.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let runtime = foreground.auth.runtime().check(request.snapshot()).unwrap();
    let caller = foreground
        .auth
        .issuer()
        .authenticate_runtime(
            std::str::from_utf8(runtime.secret(request.snapshot()).unwrap()).unwrap(),
            &request,
        )
        .unwrap();
    let authority = ProjectRecordAuthority::new(foreground.auth.issuer());
    let admission = authority
        .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Memory)
        .unwrap();
    let content = foreground.policy.content();
    NativeMemoryStore::create(
        request.snapshot(),
        &admission,
        CreateMemory {
            idempotency_key: key.into(),
            title: format!("new memory on project {index}"),
            body: "Tokyo is the capital of Japan.".into(),
            attributes: MemoryAttributes {
                classification: MemoryClass::Decision,
                mandatory: false,
                important: false,
            },
            tags: vec![],
            references: None,
        },
        MemoryStoreWriteLimits {
            read: read_limits(foreground),
            artifacts: foreground.policy.artifacts(),
            max_input_bytes: NonZeroUsize::new(content.max_input_bytes).unwrap(),
            max_row_bytes: NonZeroUsize::new(content.max_input_bytes).unwrap(),
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn foreground_memory_jobs_two_projects_share_child_and_retire_independently() {
    use graphit_daemon::supervisor::{DesiredProject, ProjectContext};
    let fixture = Fixture::new();
    fixture.install_model();
    let memories = fixture.seed_memories().await;
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let mut foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &request,
    )
    .await
    .unwrap();
    let address = foreground
        .memory_jobs
        .as_ref()
        .unwrap()
        .child
        .as_ref()
        .unwrap()
        .address();
    assert_eq!(foreground.supervisors.supervised().count(), 2);
    assert_indexed(&foreground, 0, &memories[0]).await;
    assert_indexed(&foreground, 1, &memories[1]).await;
    let desired = foreground
        .projects
        .as_ref()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, (_, id, version))| DesiredProject {
            context: ProjectContext {
                id: *id,
                version: version.clone(),
            },
            active: i != 0,
        })
        .collect();
    foreground
        .supervisors
        .reconcile(desired, |_| async {
            panic!("parking one project must preserve the other actual supervisor")
        })
        .await
        .unwrap();
    assert_eq!(foreground.supervisors.supervised().count(), 1);
    assert_eq!(foreground.supervisors.parked().count(), 1);
    let memory = create_memory(&foreground, 1, "after-parking-first").await;
    let (_, id, version) = &foreground.projects.as_ref().unwrap()[1];
    let key = ProjectContext {
        id: *id,
        version: version.clone(),
    }
    .key();
    let rescan = foreground.supervisors.rescan_handle(&key).unwrap();
    assert!(rescan.request_rescan());
    assert_indexed(&foreground, 1, &memory).await;
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
    foreground.supervisors.shutdown().await.unwrap();
    {
        let mut closing = std::pin::pin!(foreground.memory_jobs.as_mut().unwrap().shutdown());
        assert!(
            std::future::Future::poll(
                closing.as_mut(),
                &mut std::task::Context::from_waker(std::task::Waker::noop())
            )
            .is_pending()
        );
    }
    assert!(foreground.memory_jobs.as_ref().unwrap().child.is_none());
    foreground.shutdown().await.unwrap();
    assert!(!rescan.request_rescan());
    assert!(TcpStream::connect(address).await.is_err());
}

#[test]
fn foreground_memory_jobs_watch_configs_follow_each_project_and_captured_overrides() {
    use graphit_core::{
        config::{ConfigDocument, ConfigSource},
        ignore_rules::IgnoreScope,
    };
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    for (project, key, value) in [
        (
            &fixture.projects[0],
            "knowledge.docs_dir",
            json!("first docs"),
        ),
        (&fixture.projects[1], "ast.index_docs", json!(true)),
    ] {
        ConfigStore::project(project)
            .unwrap()
            .set(
                ConfigKey::new(key).unwrap(),
                value,
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let configs = fixture
        .projects
        .iter()
        .map(|p| project_watch_config(p, &fixture.config, &request).unwrap())
        .collect::<Vec<_>>();
    let first = &configs[0];
    let defaults = first
        .ast
        .defaults
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let scope =
        IgnoreScope::load(&first.root, "", first.ast.filename.as_deref(), &defaults).unwrap();
    assert!(scope.is_ignored("first docs/page.md", false).unwrap());
    let second = &configs[1];
    let defaults = second
        .ast
        .defaults
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let scope =
        IgnoreScope::load(&second.root, "", second.ast.filename.as_deref(), &defaults).unwrap();
    assert!(!scope.is_ignored("docs/page.md", false).unwrap());
    let override_config = ConfigSnapshot::new(
        fixture.config.brand(),
        [ConfigDocument::new(
            fixture.config.brand(),
            ConfigSource::Command,
            [(ConfigKey::new("ast.index_docs").unwrap(), json!(true))],
        )
        .unwrap()],
    )
    .unwrap();
    let config = project_watch_config(&fixture.projects[0], &override_config, &request).unwrap();
    assert!(!config.ast.defaults.iter().any(|p| p.contains("first docs")));
}

#[path = "../../../graphit-daemon/tests/support/model_fixture.rs"]
#[allow(
    dead_code,
    reason = "shared model fixture supplies the actual pinned GGUF CPU bundle"
)]
pub(crate) mod model_fixture;

pub(crate) struct Fixture {
    _directory: tempfile::TempDir,
    installation: InstallationPaths,
    projects: Vec<ProjectPaths>,
    config: ConfigSnapshot,
    policy: HubResourcePolicy,
}

/// Reuses the actual foreground CPU child and its shared resources for HTTP search.
pub(crate) async fn browser_search_fixture() -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    String,
    Vec<Memory>,
) {
    browser_search_fixture_configured(None).await
}
async fn browser_search_fixture_configured(
    browser: Option<&str>,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    String,
    Vec<Memory>,
) {
    browser_search_fixture_with_imports(browser, false, false).await
}

async fn browser_search_fixture_with_imports(
    browser: Option<&str>,
    imported_knowledge: bool,
    knowledge_conversations: bool,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    String,
    Vec<Memory>,
) {
    browser_search_fixture_with_services(
        browser,
        imported_knowledge,
        knowledge_conversations,
        false,
        true,
    )
    .await
}

/// Authenticated Source/Task fixture without model files, processes or inference.
pub(crate) async fn browser_syntax_fixture() -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    String,
    Vec<Memory>,
) {
    browser_search_fixture_with_services(None, false, false, false, false).await
}

async fn browser_search_fixture_with_services(
    browser: Option<&str>,
    imported_knowledge: bool,
    knowledge_conversations: bool,
    rerank: bool,
    embedding: bool,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    String,
    Vec<Memory>,
) {
    let mut fixture = Fixture::with_services(embedding, rerank);
    if let Some(endpoint) = browser {
        fixture.configure_browser(endpoint, knowledge_conversations);
    }
    if embedding {
        fixture.install_model();
    }
    if rerank {
        fixture.install_rerank_model().await;
    }
    let memories = if embedding {
        fixture.seed_memories().await
    } else {
        Vec::new()
    };
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let provider = active_provider(&fixture.installation, &fixture.config, &request)
        .unwrap()
        .unwrap();
    let mut foreground = ForegroundSource::start_with_inventory(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        &request,
        Some(&provider),
    )
    .unwrap();
    if rerank {
        foreground.project_control.as_mut().unwrap().environment = fixture.environment();
    }
    let resources = if embedding {
        let modules = foreground
            .server
            .as_ref()
            .unwrap()
            .project_modules(&request)
            .unwrap();
        let expected = crate::foreground_memory_jobs::expected_model(
            &fixture.installation,
            Some(&provider),
            request.snapshot(),
            &modules,
        )
        .unwrap()
        .unwrap();
        let mut memory = crate::foreground_memory_jobs::ForegroundMemoryJobs::new(
            Arc::clone(&foreground.auth),
            (foreground.policy.clone()).clone(),
            Some(expected),
            true,
            None,
            crate::foreground_memory_jobs::test_broker_cache(&(foreground.policy.clone())),
        )
        .unwrap();
        let resources = memory
            .start(
                &fixture.installation,
                fixture.environment(),
                foreground.scope,
                foreground.native,
                NativeBinding::KiroV3,
                &request,
            )
            .await
            .unwrap()
            .expect("configured embedding resources");
        foreground.memory_jobs = Some(memory);
        foreground.memory_job_resources = Arc::downgrade(&resources);
        Some(resources)
    } else {
        None
    };
    let mut policy = crate::foreground_tasks::task_policy(
        &foreground.policy,
        foreground.scope,
        foreground.native,
        NativeBinding::KiroV3,
    )
    .unwrap();
    if imported_knowledge {
        let identity = InstallationIdentityStore::new(&fixture.installation)
            .unwrap()
            .load()
            .unwrap();
        policy.imported_knowledge = Some(crate::task_server::ImportedKnowledgePolicy {
            registry: graphit_hub::registry::HubRegistry::global(
                &fixture.installation,
                identity.unit_id().unwrap(),
                foreground.policy.hub(),
            )
            .unwrap(),
            directory: foreground.native,
        });
    }
    let projects = Arc::clone(foreground.projects.as_ref().unwrap());
    let tasks = crate::task_server::TaskServer::new(
        Arc::clone(&foreground.auth),
        Arc::clone(&projects),
        0,
        policy,
        tokio_util::sync::CancellationToken::new(),
    )
    .unwrap();
    if rerank {
        let source = foreground.server.as_ref().unwrap();
        tasks
            .attach_local_model_control(Arc::clone(
                source.ast_search_context().unwrap().local_models,
            ))
            .unwrap();
    }
    let tasks = if browser.is_some() {
        tasks
            .with_native_storage(
                reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .no_proxy()
                    .build()
                    .unwrap(),
                true,
                None,
            )
            .unwrap()
    } else {
        tasks
    };
    tasks.attach_memory_model(resources.clone()).unwrap();
    tasks
        .attach_project_configuration(
            foreground
                .server
                .as_ref()
                .unwrap()
                .project_configuration()
                .unwrap(),
        )
        .unwrap();
    let raw = {
        let key = foreground.auth.runtime().check(request.snapshot()).unwrap();
        std::str::from_utf8(key.secret(request.snapshot()).unwrap())
            .unwrap()
            .to_owned()
    };
    if let Some(resources) = resources {
        for (paths, id, version) in projects.iter() {
            let selected = graphit_daemon::supervisor::ProjectContext {
                id: *id,
                version: version.clone(),
            };
            resources
                .index(paths, &selected, &cancellation)
                .await
                .unwrap();
        }
    }
    (fixture, foreground, tasks, raw, memories)
}

/// Publishes AST Raw and NativeNN through the foreground pipeline on the same CPU child.
pub(crate) async fn browser_ast_search_fixture() -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    (ProjectPaths, ProjectId, VersionContext),
) {
    browser_ast_search_fixture_configured(None, false).await
}
pub(crate) async fn browser_ast_import_search_fixture(
    endpoint: &str,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    (ProjectPaths, ProjectId, VersionContext),
) {
    browser_ast_search_fixture_configured(Some(endpoint), false).await
}
pub(crate) async fn live_ast_search_fixture(
    endpoint: &str,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    (ProjectPaths, ProjectId, VersionContext),
) {
    browser_ast_search_fixture_configured(Some(endpoint), true).await
}
async fn browser_ast_search_fixture_configured(
    browser: Option<&str>,
    knowledge_conversations: bool,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    (ProjectPaths, ProjectId, VersionContext),
) {
    let (fixture, mut foreground, tasks, raw, _) = browser_search_fixture_with_imports(
        browser,
        knowledge_conversations,
        knowledge_conversations,
    )
    .await;
    let resources = foreground
        .memory_job_resources
        .upgrade()
        .expect("actual shared CPU child resources");
    foreground
        .server
        .as_mut()
        .unwrap()
        .attach_semantic_model(Some(Arc::clone(&resources)))
        .unwrap();
    let selected = foreground.projects.as_ref().unwrap()[0].clone();
    let cancellation = Cancellation::default();
    ConfigStore::project(&selected.0)
        .unwrap()
        .set(
            Module::Ast.key(),
            json!(true),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    fs::write(
        selected.0.root().join("browser_search.rs"),
        "pub fn browser_search_sum(left: i32, right: i32) -> i32 { left + right }\n",
    )
    .unwrap();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let ast = Arc::new(
        ForegroundAstSources::new(
            Arc::clone(&foreground.auth),
            ForegroundAstSourceConfig {
                live: foreground
                    .server
                    .as_ref()
                    .unwrap()
                    .graph_server()
                    .live_ast(),
                installation: fixture.installation.clone(),
                base: fixture.config.clone(),
                policy: foreground.policy.clone(),
                scope: foreground.scope,
                native: foreground.native,
                binding: NativeBinding::KiroV3,
                environment: Vec::new(),
            },
            request.snapshot(),
        )
        .unwrap(),
    );
    let context = graphit_daemon::supervisor::ProjectContext {
        id: selected.1,
        version: selected.2.clone(),
    };
    assert!(
        ast.index(
            &selected.0,
            &context,
            graphit_daemon::watcher::ChangeBatch {
                rescan: true,
                ..Default::default()
            },
            &cancellation,
        )
        .await
        .unwrap()
        .is_some(),
        "genuine AST Raw publication"
    );
    assert!(
        ast.index_semantic(
            selected.0.clone(),
            context,
            Some(resources),
            None,
            std::collections::BTreeSet::new(),
            false,
            cancellation.clone(),
            cancellation.clone(),
        )
        .await
        .unwrap()
        .version
        .is_some(),
        "genuine CodeDocument inference and NativeNN publication"
    );
    let source = foreground.server.as_ref().unwrap().clone();
    (fixture, foreground, tasks, source, raw, selected)
}

/// Consume the pinned BGE producer in a private fixture inode, preserving any
/// existing embedding slot. The rerank slot must not already be selected.
pub(crate) fn install_rerank_fixture_model(installation: &InstallationPaths) {
    use crate::model_provision::{
        RERANK_MODEL_BYTES, RERANK_MODEL_FILE, RERANK_MODEL_ID, RERANK_MODEL_REVISION,
    };
    let asset = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_BGE_RERANKER_V2_M3_MODEL").expect("pinned BGE F16 artifact"),
    );
    assert_eq!(fs::metadata(&asset).unwrap().len(), RERANK_MODEL_BYTES);
    // Reuse the qualified model producer; this consumer test exercises real
    // capture, model loading and inference, rather than another installation.
    let bundle = installation.root().join("models").join("bge cpu");
    fs::create_dir_all(&bundle).unwrap();
    fs::copy(&asset, bundle.join(RERANK_MODEL_FILE)).unwrap();
    fs::write(
        bundle.join("model.json"),
        crate::models::pinned_rerank_manifest().unwrap(),
    )
    .unwrap();
    let global = ConfigStore::global(installation).unwrap();
    let snapshot = ConfigSnapshot::new(installation.brand(), [global.load().unwrap()]).unwrap();
    let mut settings = model_api::ModelSettings::read(&snapshot)
        .unwrap()
        .unwrap_or(model_api::ModelSettings { slots: Vec::new() });
    assert!(
        !settings
            .slots
            .iter()
            .any(|slot| slot.name == crate::models::DEFAULT_RERANK_SLOT)
    );
    settings.slots.push(model_api::SlotSettings {
        name: crate::models::DEFAULT_RERANK_SLOT.into(),
        selection: model_api::SelectionSettings {
            id: RERANK_MODEL_ID.into(),
            revision: RERANK_MODEL_REVISION.into(),
            task: model_api::TaskKind::Rerank,
        },
        bundle,
        manifest_relative: "model.json".into(),
        runtime: crate::models::default_rerank_runtime().unwrap(),
    });
    global
        .set(
            ConfigKey::new("daemon.models").unwrap(),
            serde_json::to_value(settings).unwrap(),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
}

impl Fixture {
    pub(crate) fn config(&self) -> &ConfigSnapshot {
        &self.config
    }

    /// Exercise the CLI wire and Source MCP dispatcher over this foreground's actual authority.
    pub(crate) async fn run_ast_graph_commands(
        &self,
        foreground: &mut ForegroundSource,
        source: &crate::source_server::SourceServer,
        commands: &[crate::graph_entry::Command],
        exports: &[crate::ast_export_entry::Arguments],
    ) -> Vec<serde_json::Value> {
        let cancellation = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let listener = foreground
            .listener
            .take()
            .expect("the original published Source listener");
        let server = source.clone();
        let serving = tokio::spawn(async move { server.serve(listener).await });
        let mut results = Vec::new();
        for command in commands {
            let mut output = Vec::new();
            crate::graph_entry::run(
                &self.installation,
                &reqwest::Client::new(),
                command.prepare(source.policy().content).unwrap(),
                (source.policy().index, source.policy().content),
                &original,
                &mut output,
            )
            .await
            .unwrap();
            results.push(serde_json::from_slice(&output).unwrap());
        }
        for arguments in exports {
            let mut output = Vec::new();
            crate::ast_export_entry::call(
                &self.installation,
                crate::ast_export_entry::prepare(arguments, None, &original).unwrap(),
                source.policy().index,
                &original,
                &mut output,
            )
            .await
            .unwrap();
            results.push(serde_json::from_slice(&output).unwrap());
        }
        source.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
        results
    }
    pub(crate) fn publish_ast_search_package(
        &self,
        foreground: &ForegroundSource,
        raw: &str,
        selected: &(ProjectPaths, ProjectId, VersionContext),
    ) -> std::path::PathBuf {
        use graphit_hub::{ArtifactId, ConcreteVersion};
        use graphit_hub::{
            ArtifactType, InstallationTarget, SourceRef,
            native_package::{self, PackageKind},
            registry::{Installation, InstalledArtifact, InstalledBackend, InstalledPayload},
        };
        let cancellation = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let source = foreground.server.as_ref().unwrap();
        let archive = self._directory.path().join("ast-search.zip");
        crate::project_graph::with_exported_graph(
            &source.graph_server(),
            crate::project_graph::ReadInput {
                auth: &foreground.auth,
                policy: source.policy(),
                base: &self.config,
                live: &source.live_projects(),
                selected: (&selected.0, selected.1, &selected.2),
                credential: raw,
                local: false,
                request: &original,
            },
            |graph, _| {
                let reader = graph
                    .source_reader(original.snapshot())
                    .map_err(io::Error::other)?;
                native_package::export_ast_graph(reader, &archive, original.snapshot())
                    .map_err(io::Error::other)
            },
        )
        .unwrap()
        .unwrap();
        let input = CapturedSourceReader::capture(
            &archive,
            graphit_fs::NodeKind::File,
            &source.policy().file,
            source.policy().read,
            Arc::clone(original.snapshot()),
        )
        .unwrap();
        let installed = self._directory.path().join("ast-search-installed");
        native_package::import(
            &input,
            Some(PackageKind::Ast),
            &installed,
            self.policy.copy(),
            original.snapshot(),
        )
        .unwrap();
        let projection = self.policy.projection();
        let native = SourceRef::local_link(
            graphit_core::native_path::NativePath::new(installed.clone()).unwrap(),
            selected.1,
            ArtifactType::Ast,
            ArtifactId::new("fixture/ast-search", projection).unwrap(),
            ConcreteVersion::new("packaged-v1", projection).unwrap(),
            projection,
        )
        .unwrap();
        source
            .policy()
            .registry
            .writer(Duration::from_secs(5), &cancellation)
            .unwrap()
            .put_metadata(InstalledArtifact {
                consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                artifact_type: ArtifactType::Ast,
                id: native.id().clone(),
                installations: vec![Installation {
                    target: InstallationTarget::GlobalNative,
                    original_source: native,
                    alias: None,
                    entry_name: None,
                    display: None,
                    backend: InstalledBackend::Native,
                    payload: InstalledPayload::Original {
                        selection_binding: None,
                    },
                }],
            })
            .unwrap();
        installed
    }
    fn configure_browser(&mut self, endpoint: &str, knowledge_conversations: bool) {
        use graphit_auth::profile::BrokerService;
        let cancellation = Cancellation::default();
        let request = self
            .policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let identity = InstallationIdentityStore::new(&self.installation)
            .unwrap()
            .ensure(Duration::from_secs(5), &cancellation)
            .unwrap();
        let scope = CapturedPublicationScope::installation(
            &self.installation,
            identity.unit_id().clone(),
            [self.installation.root().to_owned()],
            ScopeLimits::new(16, 65536, 256).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(1048576, 65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .unwrap(),
            ProfileLimits {
                bytes: 1048576,
                nodes: 65536,
                scalar_bytes: 65536,
                collection_entries: 1024,
                providers: 16,
                profiles: 16,
            },
            &request,
        )
        .unwrap();
        let publication = store
            .mutate(store.load(&request).unwrap(), &request, |doc, original| {
                doc.add_provider(
                    "broker".into(),
                    ProviderConfig {
                        kind: ProviderKind::Broker(BrokerService {
                            endpoint: endpoint.into(),
                            mcp_resource: knowledge_conversations.then(|| {
                                crate::task_server::browser::user_memory_fixture::RESOURCE
                                    .to_owned()
                            }),
                            allow_anonymous: false,
                        }),
                        embedding: AiService::Disabled,
                        rerank: AiService::Disabled,
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: S3Source::BrokerScope,
                        },
                    },
                    doc.revision(),
                    1,
                    original,
                )
            })
            .unwrap();
        assert!(publication.failure().is_none());
        assert!(publication.cleanup_failure().is_none());
        self.config = ConfigSnapshot::new(self.installation.brand(), [ConfigDocument::new(
            self.installation.brand(), ConfigSource::Global,
            [(ConfigKey::new("broker.allow_loopback_http").unwrap(), json!(true)),
             (ConfigKey::new("task.browser").unwrap(), json!({
                "providers":["broker"], "project_endpoint":format!("{endpoint}/v1/project/access/authorize"),
                "allow_loopback_http":true,
                "knowledge_conversations":knowledge_conversations,
            }))],
        ).unwrap()]).unwrap();
    }
    fn new() -> Self {
        Self::with_services(true, false)
    }
    fn with_services(embedding: bool, rerank: bool) -> Self {
        let local = || {
            AiService::Local(LocalExecution {
                device: LocalDevice::Cpu,
                device_id: 0,
            })
        };
        Self::with_ai_services(
            if embedding {
                local()
            } else {
                AiService::Disabled
            },
            if rerank { local() } else { AiService::Disabled },
        )
    }
    fn with_ai_services(embedding: AiService, rerank: AiService) -> Self {
        let direct_embedding = matches!(&embedding, AiService::Direct { .. });
        let direct_rerank = matches!(&rerank, AiService::Direct { .. });
        let directory = tempfile::tempdir().unwrap();
        let brand = Brand::default();
        let installation = InstallationPaths::resolve(
            brand.clone(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let config = ConfigSnapshot::new(&brand, []).unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let cancellation = Cancellation::default();
        let request = policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let private = installation_bootstrap::prepare(
            &installation,
            BootstrapLimits::new(65536, 256).unwrap(),
            request.snapshot(),
        )
        .into_installation()
        .unwrap();
        let identity = InstallationIdentityStore::new(&installation)
            .unwrap()
            .ensure(Duration::from_secs(5), &cancellation)
            .unwrap();
        private.revalidate(request.snapshot()).unwrap();
        let scope = CapturedPublicationScope::installation(
            &installation,
            identity.unit_id().clone(),
            [installation.root().to_owned()],
            ScopeLimits::new(16, 65536, 256).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(1048576, 65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .unwrap(),
            ProfileLimits {
                bytes: 1048576,
                nodes: 65536,
                scalar_bytes: 65536,
                collection_entries: 1024,
                providers: 16,
                profiles: 16,
            },
            &request,
        )
        .unwrap();
        let before = store.load(&request).unwrap();
        let outcome = store
            .mutate(before, &request, |doc, request| {
                doc.add_provider(
                    "cpu".into(),
                    ProviderConfig {
                        kind: ProviderKind::Local {
                            allow_aws_credential_chain: false,
                            broker: None,
                        },
                        embedding,
                        rerank,
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: S3Source::LocalCredentials,
                        },
                    },
                    doc.revision(),
                    1,
                    request,
                )?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("cpu").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(
                    ProfileData {
                        name: "local".into(),
                        provider: "cpu".into(),
                        username: "native fixture".into(),
                        organization: None,
                        teams: vec![],
                        credentials: ProfileCredentials::Local(LocalCredentials {
                            anonymous: false,
                            mcp_key: Some(Secret::new(SecretString::from("fixture-local-only"))),
                            broker_key: None,
                            embedding_api_key: direct_embedding.then(|| {
                                Secret::new(SecretString::from("fixture-direct-embedding"))
                            }),
                            rerank_api_key: direct_rerank
                                .then(|| Secret::new(SecretString::from("fixture-direct-rerank"))),
                            s3: None,
                        }),
                    },
                    expected,
                    1,
                    request,
                )?;
                doc.activate("local", doc.revision(), request)?;
                Ok(())
            })
            .unwrap();
        assert!(outcome.failure().is_none());
        assert!(outcome.cleanup_failure().is_none());
        let projects = ["first", "second"]
            .map(|name| {
                let root = directory.path().join(name);
                fs::create_dir(&root).unwrap();
                let project = ProjectPaths::at(&root, &installation).unwrap();
                ProjectIdentityStore::at(&project)
                    .writer(Duration::from_secs(5), &cancellation)
                    .unwrap()
                    .ensure(&ProjectMetadata::new(name).unwrap())
                    .unwrap();
                project
            })
            .to_vec();
        Self {
            _directory: directory,
            installation,
            projects,
            config,
            policy,
        }
    }
    fn inputs(&self) -> Vec<ProjectBootstrapInput<'_>> {
        self.projects
            .iter()
            .enumerate()
            .map(|(i, project)| ProjectBootstrapInput {
                project,
                explicit_version: if i == 0 { "main" } else { "feature" },
                pin: None,
                detached_base: "",
            })
            .collect()
    }
    fn install_model(&self) {
        let mut fixture = model_fixture::Fixture::actual(model_api::TaskKind::Embedding);
        fixture.body["recipe"]["dimensions"] = json!(128);
        // The real child resolves and verifies this installed bundle. Do not
        // re-hash the entire model in a discarded preflight configuration.
        fs::write(
            fixture.directory.path().join("model.json"),
            serde_json::to_vec(&fixture.body).unwrap(),
        )
        .unwrap();
        let mut settings = fixture.settings(&[crate::models::DEFAULT_SLOT]);
        let bundle = self.installation.root().join("models").join("gemma cpu");
        fs::create_dir_all(&bundle).unwrap();
        fs::hard_link(
            fixture.directory.path().join("model.gguf"),
            bundle.join("model.gguf"),
        )
        .unwrap();
        fs::copy(
            fixture.directory.path().join("model.json"),
            bundle.join("model.json"),
        )
        .unwrap();
        settings.slots[0].bundle = bundle;
        ConfigStore::global(&self.installation)
            .unwrap()
            .set(
                ConfigKey::new("daemon.models").unwrap(),
                serde_json::to_value(settings).unwrap(),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
    }
    async fn install_rerank_model(&self) {
        install_rerank_fixture_model(&self.installation);
    }
    fn environment(&self) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
        let executable = std::env::var_os("GRAPHIT_TEST_MODEL_SERVER")
            .expect("actual compiled graphit-model-server fixture path");
        let parent = Path::new(&executable).parent().unwrap();
        assert!(Path::new(&executable).is_absolute());
        assert!(Path::new(&executable).is_file());
        let mut environment = vec![("PATH".into(), parent.as_os_str().to_owned())];
        // ProcessHost clears inherited variables. Keep the scenario's captured
        // staging directory available to the real native model child as well.
        for name in ["TMPDIR", "TMP", "TEMP"] {
            if let Some(value) = std::env::var_os(name) {
                assert!(
                    Path::new(&value).is_absolute() && Path::new(&value).is_dir(),
                    "{name} must identify an existing absolute staging directory"
                );
                environment.push((name.into(), value));
            }
        }
        environment
    }

    fn remove_provider(&self, request: &RequestContext<'_>) {
        let identity = InstallationIdentityStore::new(&self.installation)
            .unwrap()
            .ensure(Duration::from_secs(5), request.cancellation())
            .unwrap();
        let scope = CapturedPublicationScope::installation(
            &self.installation,
            identity.unit_id().clone(),
            [self.installation.root().to_owned()],
            ScopeLimits::new(16, 65536, 256).unwrap(),
            request.snapshot(),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(1048576, 65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .unwrap(),
            ProfileLimits {
                bytes: 1048576,
                nodes: 65536,
                scalar_bytes: 65536,
                collection_entries: 1024,
                providers: 16,
                profiles: 16,
            },
            request,
        )
        .unwrap();
        let before = store.load(request).unwrap();
        let outcome = store
            .mutate(before, request, |document, request| {
                document.remove_provider("cpu", true, document.revision(), request)
            })
            .unwrap();
        assert!(outcome.failure().is_none());
        assert!(outcome.cleanup_failure().is_none());
    }

    async fn seed_memories(&self) -> Vec<graphit_records::memory_store::Memory> {
        let cancellation = Cancellation::default();
        let request = self
            .policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let provider = active_provider(&self.installation, &self.config, &request)
            .unwrap()
            .unwrap();
        let foreground = ForegroundSource::start_with_inventory(
            &self.installation,
            &self.inputs(),
            0,
            &self.config,
            NativeBinding::KiroV3,
            &request,
            Some(&provider),
        )
        .unwrap();
        let native = foreground.native;
        let content = foreground.policy.content();
        let read = MemoryStoreReadLimits {
            native,
            batch_rows: NonZeroU32::new(16).unwrap(),
            max_query_bytes: NonZeroUsize::new(content.max_output_bytes).unwrap(),
        };
        let limits = MemoryStoreWriteLimits {
            read,
            artifacts: foreground.policy.artifacts(),
            max_input_bytes: NonZeroUsize::new(content.max_input_bytes).unwrap(),
            max_row_bytes: NonZeroUsize::new(content.max_input_bytes).unwrap(),
        };
        let mut memories = Vec::new();
        for (i, (project, id, version)) in foreground.projects.as_ref().unwrap().iter().enumerate()
        {
            let scope_limits = ScopeLimits::new(16, 65536, 256).unwrap();
            let captured =
                CapturedPublicationScope::project(project, *id, scope_limits, request.snapshot())
                    .unwrap();
            let record = captured.binding().unwrap();
            let scope = ScopeInput {
                installation: foreground.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &captured,
                        record: &record,
                    },
                    version,
                }),
                providers: &[],
                binding: NativeBinding::KiroV3,
                global_version: "",
            };
            let runtime = foreground.auth.runtime().check(request.snapshot()).unwrap();
            let caller = foreground
                .auth
                .issuer()
                .authenticate_runtime(
                    std::str::from_utf8(runtime.secret(request.snapshot()).unwrap()).unwrap(),
                    &request,
                )
                .unwrap();
            let authority = ProjectRecordAuthority::new(foreground.auth.issuer());
            let admission = authority
                .admit_authenticated_write(&caller, &request, scope, ProjectWriteOperation::Memory)
                .unwrap();
            let memory = NativeMemoryStore::create(
                request.snapshot(),
                &admission,
                CreateMemory {
                    idempotency_key: format!("initial-{i}"),
                    title: format!("captured project {i}"),
                    body: "Paris is the capital of France.".into(),
                    attributes: MemoryAttributes {
                        classification: MemoryClass::Decision,
                        mandatory: false,
                        important: false,
                    },
                    tags: vec!["fixture".into()],
                    references: None,
                },
                limits,
            )
            .await
            .unwrap();
            assert_eq!(memory.revision, 1);
            memories.push(memory);
        }
        foreground.shutdown().await.unwrap();
        memories
    }
}

#[tokio::test]
async fn now_local_reads_active_memory_for_each_captured_project_version() {
    use futures_util::FutureExt;
    use rmcp::model::CallToolRequestParams;

    let fixture = Fixture::new();
    let memories = fixture.seed_memories().await;
    let cancellation = Cancellation::default();
    let original = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        vec![],
        &original,
    )
    .await
    .unwrap();
    assert!(foreground.memory_jobs.is_none());
    let native = foreground.native;
    let projects = foreground.projects.clone().unwrap();
    assert_eq!(projects.len(), memories.len());
    let service = Cancellation::default();
    let running_signal = service.clone();
    let running = tokio::spawn(async move { foreground.run(&running_signal).await });
    let exercise = std::panic::AssertUnwindSafe(async {
        let client = reqwest::Client::new();
        let tool = fixture
            .installation
            .brand()
            .mcp_tool_name(&["now"])
            .unwrap();
        for (index, (_, id, version)) in projects.iter().enumerate() {
            assert_eq!(
                version.version(),
                if index == 0 { "main" } else { "feature" }
            );
            let original = fixture
                .policy
                .request_context(Instant::now(), &cancellation)
                .unwrap();
            let address = crate::source_entry::address(&fixture.installation, native, &original)
                .await
                .unwrap();
            let connection = crate::ast_source::connect_source(
                &fixture.installation,
                address,
                &client,
                native,
                &original,
                (),
            )
            .await
            .unwrap();
            let call = CallToolRequestParams::new(tool.clone()).with_arguments(
                json!({"mode":"local","project_id":id,"version":version.version()})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
            let deadline =
                tokio::time::Instant::from_std(original.snapshot().required_deadline().unwrap());
            let response = tokio::select! {
                response = connection.peer().call_tool(call) => response.map_err(io::Error::other),
                () = original.cancellation().cancelled() => Err(io::ErrorKind::Interrupted.into()),
                () = tokio::time::sleep_until(deadline) => Err(io::ErrorKind::TimedOut.into()),
            };
            connection.cancel().await.unwrap();
            original.check().unwrap();
            let response = response.unwrap();
            assert_ne!(response.is_error, Some(true), "{:?}", response.content);
            assert!(response.structured_content.is_none());
            assert_eq!(response.content.len(), 1);
            let snapshot: serde_json::Value =
                serde_json::from_str(&response.content[0].as_text().unwrap().text).unwrap();
            assert_eq!(snapshot["project_id"], json!(id));
            assert!(
                snapshot["generated_at"]
                    .as_str()
                    .is_some_and(|v| !v.is_empty())
            );
            let memory = &snapshot["sections"]["memories"];
            assert_eq!(memory["availability"], "available");
            assert_eq!(memory["total"], 1);
            assert_eq!(memory["has_more"], false);
            assert!(memory["error"].is_null());
            let heads = memory["items"].as_array().unwrap();
            assert_eq!(heads.len(), 1);
            assert_eq!(heads[0]["id"], json!(memories[index].id));
            assert_eq!(heads[0]["title"], memories[index].title);
            assert!(
                heads[0]["updated_at"]
                    .as_str()
                    .is_some_and(|v| !v.is_empty())
            );
            assert!(memories.iter().enumerate().all(|(other, expected)| {
                other == index || heads.iter().all(|head| head["id"] != json!(expected.id))
            }));
            eprintln!(
                "Now {}: populated Memory head read through Source MCP",
                version.version()
            );
        }
    })
    .catch_unwind()
    .await;
    service.cancel();
    running.await.unwrap().unwrap();
    if let Err(panic) = exercise {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn foreground_memory_jobs_missing_model_keeps_source_without_compute() {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        vec![],
        &request,
    )
    .await
    .unwrap();
    assert!(foreground.memory_jobs.is_none());
    assert!(foreground.server.is_some());
    foreground.shutdown().await.unwrap();
}

#[tokio::test]
async fn foreground_memory_jobs_invalid_version_never_starts_model() {
    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &fixture.projects[0],
        explicit_version: "bad\nversion",
        pin: None,
        detached_base: "",
    }];
    let failure = ForegroundSource::start_and_publish(
        &fixture.installation,
        &inputs,
        0,
        &fixture.config,
        NativeBinding::KiroV3,
        vec![],
        &request,
    )
    .await
    .err()
    .unwrap();
    let selection = failure
        .error
        .get_ref()
        .unwrap()
        .downcast_ref::<graphit_daemon::project_bootstrap::ProjectBootstrapError>()
        .unwrap();
    let graphit_daemon::project_bootstrap::ProjectBootstrapError::Selection(cause) = selection
    else {
        panic!("invalid version must fail in the actual selection producer")
    };
    assert_eq!(cause.kind(), io::ErrorKind::InvalidInput);
    if let Some(owner) = failure.owner {
        assert!(owner.memory_jobs.is_none());
        owner.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn foreground_memory_jobs_disabled_model_rejects_malformed_winning_configuration() {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let global = ConfigStore::global(&fixture.installation).unwrap();
    // Task is now a real embedding consumer. Keep all other corpus consumers
    // disabled so malformed winning configuration cannot pass by an obsolete gate.
    for module in [Module::Memory, Module::Ast, Module::Knowledge] {
        global
            .set(
                module.key(),
                json!(false),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    global
        .set(
            Module::Embedding.key(),
            json!(false),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let provider = active_provider(&fixture.installation, &fixture.config, &request).unwrap();
    assert!(
        graphit_core::module_settings::ModuleSettings::resolve(
            &ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap(),
        )
        .and_then(|modules| crate::foreground_memory_jobs::expected_model(
            &fixture.installation,
            provider.as_ref(),
            request.snapshot(),
            &[modules],
        ))
        .unwrap()
        .is_none()
    );
    global
        .set(
            Module::Embedding.key(),
            json!("yes"),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    assert_eq!(
        graphit_core::module_settings::ModuleSettings::resolve(
            &ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap(),
        )
        .and_then(|modules| crate::foreground_memory_jobs::expected_model(
            &fixture.installation,
            provider.as_ref(),
            request.snapshot(),
            &[modules],
        ))
        .err()
        .unwrap()
        .kind(),
        io::ErrorKind::InvalidData
    );
    global
        .set(
            Module::Embedding.key(),
            json!(true),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    global
        .set(
            ConfigKey::new("daemon.models").unwrap(),
            json!({"slots":"malformed"}),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
    assert!(
        graphit_core::module_settings::ModuleSettings::resolve(
            &ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap(),
        )
        .and_then(|modules| crate::foreground_memory_jobs::expected_model(
            &fixture.installation,
            provider.as_ref(),
            request.snapshot(),
            &[modules],
        ))
        .is_err()
    );
}

#[tokio::test]
async fn foreground_memory_jobs_service_context_rejected_before_native_startup() {
    let fixture = Fixture::new();
    let cancellation = Cancellation::default();
    let request = RequestContext::service(fixture.policy.content(), &cancellation).unwrap();
    assert_eq!(
        ForegroundSource::start_and_publish(
            &fixture.installation,
            &fixture.inputs(),
            0,
            &fixture.config,
            NativeBinding::KiroV3,
            vec![],
            &request
        )
        .await
        .err()
        .unwrap()
        .error
        .kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(!fixture.installation.root().join("daemon").exists());
}

#[path = "role_rescan_tests.rs"]
mod role_rescan_tests;

#[path = "model_ready_wake_tests.rs"]
mod model_ready_wake_tests;

#[tokio::test]
async fn foreground_memory_jobs_ast_consumer_shares_model_without_memory_callbacks() {
    let fixture = Fixture::new();
    fixture.install_model();
    let cancellation = Cancellation::default();
    let global = ConfigStore::global(&fixture.installation).unwrap();
    for (module, enabled) in [
        (Module::Memory, false),
        (Module::Ast, true),
        (Module::Knowledge, false),
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
    let config = ConfigSnapshot::new(fixture.config.brand(), [global.load().unwrap()]).unwrap();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &config,
        NativeBinding::KiroV3,
        fixture.environment(),
        &request,
    )
    .await
    .unwrap();
    let resources = foreground
        .memory_job_resources
        .upgrade()
        .expect("actual shared child resources");
    assert!(foreground.memory_jobs.as_ref().unwrap().child.is_some());
    assert!(
        foreground
            .server
            .as_ref()
            .unwrap()
            .shares_semantic_model(&resources)
    );
    assert_eq!(foreground.supervisors.supervised().count(), 0);
    assert_eq!(
        foreground
            .supervisors
            .supervised()
            .filter_map(|context| foreground.supervisors.rescan_handle(&context.key()))
            .count(),
        0
    );
    drop(resources);
    foreground.shutdown().await.unwrap();
    for project in &fixture.projects {
        for module in [Module::Ast, Module::Memory, Module::Knowledge] {
            ConfigStore::project(project)
                .unwrap()
                .set(
                    module.key(),
                    json!(false),
                    Duration::from_secs(5),
                    &cancellation,
                )
                .unwrap();
        }
    }
    let foreground = ForegroundSource::start_and_publish(
        &fixture.installation,
        &fixture.inputs(),
        0,
        &config,
        NativeBinding::KiroV3,
        vec![],
        &request,
    )
    .await
    .unwrap();
    assert!(foreground.memory_jobs.is_none());
    assert!(foreground.memory_job_resources.upgrade().is_none());
    assert_eq!(foreground.supervisors.supervised().count(), 0);
    foreground.shutdown().await.unwrap();
}

/// Knowledge uses the existing foreground CPU process and native authored/NN producers.
pub(crate) async fn browser_knowledge_search_fixture() -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    (ProjectPaths, ProjectId, VersionContext),
) {
    knowledge_cpu_search_fixture(false, None, false).await
}

pub(crate) async fn knowledge_cpu_search_fixture(
    imported: bool,
    browser: Option<&str>,
    knowledge_conversations: bool,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    (ProjectPaths, ProjectId, VersionContext),
) {
    knowledge_cpu_search_fixture_configured(imported, browser, knowledge_conversations, false).await
}

/// Drive the returned ForegroundSource::run during reranking: that owner holds
/// the real LocalModel command receiver.
pub(crate) async fn knowledge_cpu_search_fixture_with_rerank(
    imported: bool,
    browser: Option<&str>,
    knowledge_conversations: bool,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    (ProjectPaths, ProjectId, VersionContext),
) {
    knowledge_cpu_search_fixture_configured(imported, browser, knowledge_conversations, true).await
}

async fn knowledge_cpu_search_fixture_configured(
    imported: bool,
    browser: Option<&str>,
    knowledge_conversations: bool,
    rerank: bool,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    (ProjectPaths, ProjectId, VersionContext),
) {
    use crate::knowledge_semantic::{
        KnowledgeSemanticIndexLimits, KnowledgeSemanticIndexResult, KnowledgeSemanticModel,
        index_knowledge_semantics,
    };
    use graphit_data::knowledge_document_projection::{
        AuthoredPageFields, AuthoredParameters, CompilationDigest32, PhysicalSourceDigest32,
        RecipeFields,
    };
    use graphit_data::{
        knowledge_embeddings::KnowledgeEmbeddingWriteMode,
        knowledge_index::KnowledgeIndexWriteLimits,
    };
    use sha2::{Digest, Sha256};
    let (fixture, mut foreground, tasks, raw, _) = browser_search_fixture_with_services(
        browser,
        imported,
        knowledge_conversations,
        rerank,
        true,
    )
    .await;
    let resources = foreground
        .memory_job_resources
        .upgrade()
        .expect("existing CPU resources");
    foreground
        .server
        .as_mut()
        .unwrap()
        .attach_semantic_model(Some(Arc::clone(&resources)))
        .unwrap();
    let selected = foreground.projects.as_ref().unwrap()[0].clone();
    let page = |slug: &'static str,
                title: &'static str,
                body: &'static str,
                source_path: &'static str| AuthoredPageFields {
        slug: Some(slug),
        title: Some(title),
        body: Some(body),
        kind: Some("guide"),
        summary: Some(title),
        description: None,
        summary_truncated: false,
        source_path,
        physical_digest: Some(PhysicalSourceDigest32::from_bytes(
            Sha256::digest(body.as_bytes()).into(),
        )),
        compilation_digest: CompilationDigest32::from_bytes(Sha256::digest(body.as_bytes()).into()),
        recipe: RecipeFields {
            id: graphit_data::knowledge_document_projection::AUTHORED_RECIPE_ID,
            parameters: Some(AuthoredParameters {
                fallback_title: title,
                kind_on_absent: "guide",
                max_summary_bytes: NonZeroUsize::new(512).unwrap(),
            }),
        },
        updated_at: None,
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: &[],
    };
    let pages = if imported {
        vec![
            page(
                "B",
                "Knowledge CPU search B first",
                "Knowledge CPU search original namespace B literal first body.",
                "docs/B.md",
            ),
            page(
                "B-second",
                "Knowledge CPU search B second",
                "Knowledge CPU search original namespace B literal second body.",
                "docs/B-second.md",
            ),
        ]
    } else {
        vec![page(
            "knowledge-cpu",
            "Knowledge CPU search",
            "Knowledge CPU search explains a real local embedding query and same snapshot lexical results.",
            "docs/knowledge-cpu.md",
        )]
    };
    fs::create_dir_all(selected.0.root().join("docs")).unwrap();
    for page in &pages {
        fs::write(selected.0.root().join(page.source_path), page.body.unwrap()).unwrap();
    }
    crate::task_server::browser::knowledge_fixture::seed(&tasks, &raw, 0, &pages).await;
    let cancellation = Cancellation::default();
    let request = fixture
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let issuer = foreground.auth.issuer();
    let caller = issuer.authenticate_runtime(&raw, &request).unwrap();
    let live = CapturedPublicationScope::project(
        &selected.0,
        selected.1,
        foreground.scope,
        request.snapshot(),
    )
    .unwrap();
    let record = live.binding().unwrap();
    let input = || ScopeInput {
        installation: foreground.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &live,
                record: &record,
            },
            version: &selected.2,
        }),
        providers: &[],
        binding: NativeBinding::KiroV3,
        global_version: "",
    };
    let authority = ProjectRecordAuthority::new(issuer);
    let read = authority
        .admit_authenticated_read(&caller, &request, input(), ProjectReadModule::Knowledge)
        .unwrap();
    let write = authority
        .admit_authenticated_write(&caller, &request, input(), ProjectWriteOperation::Knowledge)
        .unwrap();
    let policy = crate::foreground_tasks::task_policy(
        &foreground.policy,
        foreground.scope,
        foreground.native,
        NativeBinding::KiroV3,
    )
    .unwrap();
    let read_limits = crate::foreground_knowledge::limits(&policy).unwrap();
    let metadata = resources.metadata(&request).await.unwrap();
    let model = KnowledgeSemanticModel {
        metadata: &metadata,
        batch_limits: resources.rpc_limits(),
        rpc_timeout: std::time::Duration::from_millis(
            foreground
                .policy
                .limit(graphit_hub::policy::ResourceSetting::RequestDeadlineMs)
                .value,
        ),
    };
    let indexed = index_knowledge_semantics(
        request.snapshot(),
        &read,
        &write,
        &model,
        KnowledgeEmbeddingWriteMode::ReplaceComplete,
        KnowledgeSemanticIndexLimits {
            read: read_limits,
            write: KnowledgeIndexWriteLimits {
                content: read_limits.content,
                native: foreground.native,
                artifacts: foreground.policy.artifacts(),
            },
        },
        |operation| resources.embed_batch(request.snapshot(), &metadata, operation),
    )
    .await
    .unwrap();
    assert!(
        matches!(indexed, KnowledgeSemanticIndexResult::Published(_)),
        "genuine document inference and Knowledge NN publication"
    );
    let source = foreground.server.as_ref().unwrap().clone();
    (fixture, foreground, tasks, source, raw, selected)
}

/// Freeze the real two-document CPU publication and mount its native package as A.
/// The document IDs and text remain the original B literals, independent of the mount.
pub(crate) async fn imported_knowledge_cpu_search_fixture(
    browser: Option<&str>,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    u64,
) {
    imported_knowledge_cpu_search_fixture_with_live(browser, false).await
}

/// Opt in through the same browser policy as Live before constructing Auth/model resources.
pub(crate) async fn live_imported_knowledge_cpu_search_fixture(
    browser: &str,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    u64,
) {
    imported_knowledge_cpu_search_fixture_with_live(Some(browser), true).await
}

async fn imported_knowledge_cpu_search_fixture_with_live(
    browser: Option<&str>,
    knowledge_conversations: bool,
) -> (
    Fixture,
    ForegroundSource,
    crate::task_server::TaskServer,
    SourceServer,
    String,
    u64,
) {
    use crate::knowledge_package_export::{
        KnowledgePackageExportOutcome, export_native_knowledge_package,
    };
    use graphit_core::{
        artifact_store::{
            CapturedSourceReader, CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy,
            SourceReadLimits,
        },
        installation_identity::InstallationIdentityStore,
        native_path::NativePath,
    };
    use graphit_hub::{
        ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
        native_package::{PackageKind, stage_import},
        registry::{
            HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
        },
    };
    let (fixture, foreground, tasks, source, raw, selected) =
        knowledge_cpu_search_fixture(true, browser, knowledge_conversations).await;
    let pair = {
        let cancel = Cancellation::default();
        let request = fixture
            .policy
            .request_context(Instant::now(), &cancel)
            .unwrap();
        let issuer = foreground.auth.issuer();
        let caller = issuer.authenticate_runtime(&raw, &request).unwrap();
        let live = CapturedPublicationScope::project(
            &selected.0,
            selected.1,
            foreground.scope,
            request.snapshot(),
        )
        .unwrap();
        let record = live.binding().unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let admission = authority
            .admit_authenticated_read(
                &caller,
                &request,
                ScopeInput {
                    installation: foreground.auth.installation(),
                    project: Some(ProjectCapture {
                        scope: ScopeCapture {
                            live: &live,
                            record: &record,
                        },
                        version: &selected.2,
                    }),
                    providers: &[],
                    binding: NativeBinding::KiroV3,
                    global_version: "",
                },
                ProjectReadModule::Knowledge,
            )
            .unwrap();
        let policy = crate::foreground_tasks::task_policy(
            &foreground.policy,
            foreground.scope,
            foreground.native,
            NativeBinding::KiroV3,
        )
        .unwrap();
        let copy = fixture.policy.copy();
        let archive = selected
            .0
            .root()
            .parent()
            .unwrap()
            .join("i1-cpu-source.zip");
        let KnowledgePackageExportOutcome::Exported { generation } =
            export_native_knowledge_package(
                &request,
                &admission,
                &caller,
                &archive,
                crate::foreground_knowledge::limits(&policy).unwrap(),
            )
            .await
            .unwrap()
        else {
            panic!("actual CPU Source/NN package required")
        };
        assert!(generation.version > 0);
        let resources = foreground.memory_job_resources.upgrade().unwrap();
        let retention = RetainedBytesLimits::new(
            usize::try_from(copy.total_file_bytes_limit()).unwrap(),
            copy.node_limit(),
            copy.total_relative_path_bytes_limit(),
        )
        .unwrap();
        let files = SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            copy,
            RetainedFileSet::new([], retention).unwrap(),
            retention,
        )
        .unwrap();
        let reader = CapturedSourceReader::capture(
            &archive,
            graphit_fs::NodeKind::File,
            &files,
            SourceReadLimits::new(65536, 65536, copy.total_file_bytes_limit()).unwrap(),
            Arc::clone(request.snapshot()),
        )
        .unwrap();
        let imported = selected.0.root().parent().unwrap().join("i1-mounted-a");
        stage_import(
            &reader,
            Some(PackageKind::Knowledge),
            &imported,
            copy,
            request.snapshot(),
        )
        .unwrap()
        .publish()
        .unwrap();
        assert!(imported.join("index.lance").is_dir());
        let uri = url::Url::from_file_path(imported.join("index.lance"))
            .unwrap()
            .to_string();
        let pin =
            graphit_data::lance_branch_store::BranchSnapshot::capture_observed(&uri, None, None)
                .await
                .unwrap();
        let dimensions =
            graphit_data::knowledge_embeddings::native_knowledge_embedding_dimensions(pin.schema())
                .unwrap()
                .unwrap();
        assert_eq!(
            u64::from(dimensions.get()),
            resources
                .local
                .as_ref()
                .unwrap()
                .expected
                .dimensions()
                .unwrap()
        );
        let version = pin.version();
        let identity = InstallationIdentityStore::new(&fixture.installation)
            .unwrap()
            .load()
            .unwrap();
        let registry = HubRegistry::global(
            &fixture.installation,
            identity.unit_id().unwrap(),
            foreground.policy.hub(),
        )
        .unwrap();
        let projection = registry.limits().projection();
        let mounted = SourceRef::local_link(
            NativePath::new(imported).unwrap(),
            selected.1,
            ArtifactType::Knowledge,
            ArtifactId::new("vendor/mount-a", projection).unwrap(),
            ConcreteVersion::new("exact-release", projection).unwrap(),
            projection,
        )
        .unwrap();
        registry
            .writer(
                request.remaining().unwrap().unwrap(),
                request.cancellation(),
            )
            .unwrap()
            .put_metadata(InstalledArtifact {
                consumer: graphit_core::artifact_store::PublicationSlot::Scope,
                artifact_type: ArtifactType::Knowledge,
                id: mounted.id().clone(),
                installations: vec![Installation {
                    target: InstallationTarget::GlobalNative,
                    original_source: mounted,
                    alias: None,
                    entry_name: None,
                    display: None,
                    backend: InstalledBackend::Native,
                    payload: InstalledPayload::Original {
                        selection_binding: None,
                    },
                }],
            })
            .unwrap();
        version
    };
    (fixture, foreground, tasks, source, raw, pair)
}

pub(crate) async fn stop_browser_model(foreground: &mut ForegroundSource) {
    let model = foreground
        .memory_jobs
        .as_mut()
        .expect("existing foreground model owner");
    let address = model.child.as_ref().expect("actual CPU child").address();
    model.shutdown().await.unwrap();
    assert!(
        TcpStream::connect(address).await.is_err(),
        "real model endpoint closed"
    );
}

/// Reuse real Source/Auth/model metadata without launching the compute process.
pub(crate) fn memory_shutdown_fixture() -> (
    Fixture,
    ForegroundSource,
    crate::foreground_memory_jobs::ForegroundMemoryJobs,
) {
    let fixture = Fixture::new();
    fixture.install_model();
    let (source, jobs) = {
        let cancellation = Cancellation::default();
        let request = fixture
            .policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        let source = ForegroundSource::start(
            &fixture.installation,
            &fixture.inputs(),
            0,
            &fixture.config,
            NativeBinding::KiroV3,
            &request,
        )
        .unwrap();
        let modules = source
            .server
            .as_ref()
            .expect("configured Source server")
            .project_modules(&request)
            .unwrap();
        let provider = active_provider(&fixture.installation, &fixture.config, &request).unwrap();
        let expected = crate::foreground_memory_jobs::expected_model(
            &fixture.installation,
            provider.as_ref(),
            request.snapshot(),
            &modules,
        )
        .unwrap()
        .expect("configured Memory model");
        let jobs = crate::foreground_memory_jobs::ForegroundMemoryJobs::new(
            Arc::clone(&source.auth),
            (source.policy.clone()).clone(),
            Some(expected),
            true,
            None,
            crate::foreground_memory_jobs::test_broker_cache(&(source.policy.clone())),
        )
        .expect("configured Memory owner");
        (source, jobs)
    };
    assert!(
        jobs.child.is_none(),
        "shutdown regressions must not launch compute"
    );
    (fixture, source, jobs)
}
