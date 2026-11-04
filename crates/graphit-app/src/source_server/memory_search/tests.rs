use super::*;
use crate::foreground_memory_jobs::MemoryJobResources;
use crate::foreground_source::foreground_memory_jobs_tests::{
    model_fixture, model_peer::ModelPeer,
};
use graphit_auth::{
    Permission, ProjectWriteOperation, ServiceOrigin, ServiceRuleInput,
    profile::{LocalDevice, LocalExecution},
};
use graphit_core::{
    artifact_store::{ArtifactLimits, CopyLimits},
    brand::Brand,
    config::{ConfigSnapshot, ConfigStore},
};
use graphit_daemon::model_service as model_api;
use graphit_daemon::model_service::{EmbedInput, ExpectedModel};
use graphit_records::memory_attributes::MemoryAttributes;
use rmcp::{
    RoleClient,
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::{collections::HashMap, num::NonZeroU64};
use tokio::net::TcpStream;

#[test]
fn nested_store_interruption_preserves_memory_mcp_error_codes() {
    use graphit_auth::{ServiceUseError, store::AuthStoreError};
    use graphit_core::auth_document::AuthDocumentError;
    for (cause, interrupted) in [
        (AuthDocumentError::Cancelled, true),
        (AuthDocumentError::Deadline, true),
        (AuthDocumentError::Denied, false),
        (AuthDocumentError::Conflict, false),
    ] {
        for model in [false, true] {
            let service = ServiceUseError::Store(AuthStoreError::Core(cause));
            let error = if model {
                SearchError::Model(EmbeddingServiceError::Service(service))
            } else {
                SearchError::Service(service)
            };
            let response = error.response();
            assert_eq!(response.is_error, Some(true));
            let expected = if interrupted {
                "request_cancelled_or_expired"
            } else if model {
                "model_failed"
            } else {
                "service_denied"
            };
            assert_eq!(response.structured_content.unwrap()["code"], expected);
        }
    }
}

type SourceFixture = super::super::test_fixture::Fixture;

fn configure(source: &mut SourceFixture, embedding: bool) -> ConfigSnapshot {
    let global = ConfigStore::global(&source.paths).unwrap();
    let cancel = Cancellation::default();
    for (module, enabled) in [
        (Module::Memory, true),
        (Module::Embedding, embedding),
        (Module::Ast, false),
        (Module::Knowledge, false),
    ] {
        global
            .set(
                module.key(),
                json!(enabled),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
    }
    let config = ConfigSnapshot::new(source.paths.brand(), [global.load().unwrap()]).unwrap();
    source.server.attach_project_hook(&config).unwrap();
    config
}

pub(in crate::source_server) struct Fixture {
    pub(in crate::source_server) source: SourceFixture,
    pub(in crate::source_server) model: ModelPeer,
    resources: Arc<MemoryJobResources>,
    _bundle: model_fixture::Fixture,
}
impl Fixture {
    pub(in crate::source_server) async fn new(local_embedding: bool) -> Self {
        let execution = LocalExecution {
            device: LocalDevice::Cpu,
            device_id: 0,
        };
        let rules: Vec<_> = [ServiceOrigin::Runtime, ServiceOrigin::Local]
            .into_iter()
            .filter(|origin| local_embedding || *origin == ServiceOrigin::Runtime)
            .map(|origin| ServiceRuleInput::Local {
                origin,
                provider: "cpu",
                purpose: ServicePurpose::Embedding,
                execution,
            })
            .collect();
        let mut source = super::super::test_fixture::fixture_with_projects_and_rules(
            &[Permission::Read(Module::Memory), Permission::WriteMemory],
            Brand::default(),
            NativeBinding::KiroV3,
            &["first-memory", "second-memory"],
            &rules,
        );
        local_profile(&source);
        let mut bundle = model_fixture::Fixture::metadata(model_api::TaskKind::Embedding);
        bundle.body["recipe"]["dimensions"] = serde_json::json!(128);
        std::fs::write(
            bundle.directory.path().join("model.json"),
            serde_json::to_vec(&bundle.body).unwrap(),
        )
        .unwrap();
        let settings = bundle.settings(&[crate::models::DEFAULT_SLOT]);
        let resolution = Arc::new(graphit_core::artifact_store::SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        // Follow production's metadata admission. The private artifact was
        // checked above; the real model child validates the weights on use.
        let resolved = model_api::ResolvedConfig::resolve_declared(
            bundle.settings(&[crate::models::DEFAULT_SLOT]),
            model_fixture::cpu_selection(),
            &resolution,
            &model_fixture::capture_policy(),
            graphit_core::artifact_store::SourceReadLimits::new(65536, 512, 65536).unwrap(),
            &model_fixture::manifest_limits(),
        )
        .unwrap();
        let cancel = Cancellation::default();
        ConfigStore::global(&source.paths)
            .unwrap()
            .set(
                graphit_core::config::ConfigKey::new("daemon.models").unwrap(),
                serde_json::to_value(settings).unwrap(),
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
        let config = configure(&mut source, true);
        let policy = graphit_hub::policy::HubResourcePolicy::resolve(&config).unwrap();
        let expected = ExpectedModel::new(&resolved, crate::models::DEFAULT_SLOT, "cpu").unwrap();
        let mut resources = MemoryJobResources::remote(
            Arc::clone(&source.auth),
            None,
            crate::foreground_memory_jobs::test_broker_cache(&policy),
            policy,
            source.server.0.policy.scope,
            source.server.0.policy.index,
            NativeBinding::KiroV3,
        )
        .unwrap();
        let model = ModelPeer::bind(&mut resources, expected).await;
        let resources = Arc::new(resources);
        source
            .server
            .attach_semantic_model(Some(Arc::clone(&resources)))
            .unwrap();
        assert!(Arc::ptr_eq(
            &source.server.0.semantic_model().unwrap().unwrap(),
            &resources
        ));
        Self {
            source,
            model,
            resources,
            _bundle: bundle,
        }
    }
    pub(in crate::source_server) async fn close(mut self) {
        self.source.server.shutdown().await.unwrap();
        self.model.shutdown().await.unwrap();
    }
}

fn local_profile(source: &SourceFixture) {
    use graphit_auth::{
        profile::{
            AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileExpectation,
            ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
        },
        store::AuthStore,
    };
    use graphit_core::auth_document::{AuthDocumentLimits, AuthDocumentStore};
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        source.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let installation = source.auth.installation();
    let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
        installation.live.owner()
    else {
        panic!("actual installation")
    };
    let scope = CapturedPublicationScope::installation(
        &source.paths,
        installation_id.clone(),
        installation.live.allowed_roots().map(ToOwned::to_owned),
        source.server.0.policy.scope,
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
    let observed = store.load(&request).unwrap();
    let outcome = store
        .mutate(observed, &request, |doc, request| {
            doc.add_provider(
                "cpu".into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: false,
                        broker: None,
                    },
                    embedding: AiService::Local(LocalExecution {
                        device: LocalDevice::Cpu,
                        device_id: 0,
                    }),
                    rerank: AiService::Disabled,
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
                    username: "Memory fixture".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new(secrecy::SecretString::from(
                            "memory-local-fixture",
                        ))),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
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
}

macro_rules! admissions {
    ($source:ident,$index:expr,$cancel:ident,$request:ident,$issuer:ident,$caller:ident,$authority:ident,$read:ident,$write:ident) => {
        let $cancel = Cancellation::default();
        let $request = RequestContext::new(
            $source.server.0.policy.content,
            &$cancel,
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap();
        let $issuer = $source.auth.issuer();
        let $caller = $issuer
            .authenticate_runtime(&$source.raw, &$request)
            .unwrap();
        let (paths, id, version) = &$source.projects[$index];
        let live = CapturedPublicationScope::project(
            paths,
            *id,
            $source.server.0.policy.scope,
            $request.snapshot(),
        )
        .unwrap();
        let record = live.binding().unwrap();
        let scope = ScopeInput {
            installation: $source.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &live,
                    record: &record,
                },
                version,
            }),
            providers: &[],
            binding: $source.server.0.policy.binding,
            global_version: "",
        };
        let $authority = ProjectRecordAuthority::new($issuer);
        let $read = $authority
            .admit_authenticated_read(&$caller, &$request, scope, ProjectReadModule::Memory)
            .unwrap();
        let $write = $authority
            .admit_authenticated_write(&$caller, &$request, scope, ProjectWriteOperation::Memory)
            .unwrap();
    };
}

fn write_limits(source: &SourceFixture) -> MemoryStoreWriteLimits {
    let copies = CopyLimits::new(1048576, 128, 65536, 16).unwrap();
    MemoryStoreWriteLimits {
        read: read_limits(&source.server.0).unwrap(),
        artifacts: ArtifactLimits::new(1048576, 65536, copies).unwrap(),
        max_input_bytes: NonZeroUsize::new(1048576).unwrap(),
        max_row_bytes: NonZeroUsize::new(2097152).unwrap(),
    }
}
fn input(key: &str, body: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: "Memory record".into(),
        body: body.into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory: false,
            important: false,
        },
        tags: vec!["retained".into()],
        references: None,
    }
}
pub(in crate::source_server) async fn create(
    source: &SourceFixture,
    index: usize,
    key: &str,
    body: &str,
) -> Memory {
    admissions!(
        source, index, cancel, request, _issuer, _caller, _authority, _read, write
    );
    NativeMemoryStore::create(
        request.snapshot(),
        &write,
        input(key, body),
        write_limits(source),
    )
    .await
    .unwrap()
}
async fn update(
    source: &SourceFixture,
    index: usize,
    memory: &Memory,
    change: UpdateMemory,
) -> Memory {
    admissions!(
        source, index, cancel, request, _issuer, _caller, _authority, _read, write
    );
    NativeMemoryStore::update(
        request.snapshot(),
        &write,
        memory.id,
        change,
        write_limits(source),
    )
    .await
    .unwrap()
}
fn change(memory: &Memory, key: &str, body: &str) -> UpdateMemory {
    UpdateMemory {
        expected_revision: memory.revision,
        idempotency_key: key.into(),
        reason: "fixture revision".into(),
        title: None,
        body: Some(body.into()),
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    }
}
async fn embed(f: &Fixture, index: usize) {
    let source = &f.source;
    admissions!(
        source, index, cancel, request, issuer, caller, _authority, read, write
    );
    let mut service = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = service.local().unwrap();
    let model = MemoryEmbeddingModel::new(
        request.snapshot(),
        crate::foreground_memory_jobs::MemoryJobResources::memory_identity(
            &crate::embedding_service::EmbeddingMetadata::local(
                &f.resources.local.as_ref().unwrap().expected,
            )
            .unwrap(),
        )
        .unwrap(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    model
        .index(
            &read,
            &write,
            read_limits(&source.server.0).unwrap(),
            write_limits(source),
            f.resources.rpc_limits(),
            |operation| async {
                f.resources
                    .local
                    .as_ref()
                    .unwrap()
                    .client
                    .embed(
                        &lease,
                        source.auth.runtime(),
                        &f.resources.local.as_ref().unwrap().expected,
                        operation,
                        TcpStream::connect(f.resources.local.as_ref().unwrap().address),
                    )
                    .await
                    .map(|result| result.vectors)
            },
        )
        .await
        .unwrap();
}
async fn incompatible_header_with_real_rpc_vectors(f: &Fixture, index: usize) {
    let source = &f.source;
    admissions!(
        source, index, cancel, request, issuer, caller, _authority, read, write
    );
    // Only the SDK identity header is deliberately incompatible. Vectors come from the actual model RPC.
    let identity = MemoryEmbeddingIdentity::new(
        [0xa5; 32],
        NonZeroU32::new(
            u32::try_from(
                f.resources
                    .local
                    .as_ref()
                    .unwrap()
                    .expected
                    .dimensions()
                    .expect("actual model dimensions"),
            )
            .unwrap(),
        )
        .unwrap(),
        MEMORY_DOCUMENT_RECIPE.into(),
    )
    .unwrap();
    let MemoryEmbeddingCorpusRead::Published(mut corpus) = NativeMemoryStore::capture_embeddings(
        request.snapshot(),
        &read,
        identity,
        read_limits(&source.server.0).unwrap(),
    )
    .await
    .unwrap() else {
        panic!("actual corpus")
    };
    let mut service = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = service.local().unwrap();
    let mut attachments = Vec::new();
    while let Some(documents) = corpus
        .next_documents(request.snapshot(), &read)
        .await
        .unwrap()
    {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for document in documents {
            keys.push((
                document.memory_id(),
                NonZeroU64::new(document.revision()).unwrap(),
                *document.fingerprint(),
            ));
            texts.push(document.into_text());
        }
        let output = f
            .resources
            .local
            .as_ref()
            .unwrap()
            .client
            .embed(
                &lease,
                source.auth.runtime(),
                &f.resources.local.as_ref().unwrap().expected,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Document,
                    texts,
                    deadline: request.snapshot().required_deadline().unwrap(),
                },
                TcpStream::connect(f.resources.local.as_ref().unwrap().address),
            )
            .await
            .unwrap();
        assert_eq!(output.vectors.len(), keys.len());
        for ((memory_id, revision, input_fingerprint), vector) in
            keys.into_iter().zip(output.vectors)
        {
            attachments.push(MemoryEmbeddingAttachment {
                memory_id,
                revision,
                input_fingerprint,
                vector,
            });
        }
    }
    NativeMemoryStore::attach_embeddings(
        request.snapshot(),
        &write,
        &corpus,
        attachments,
        write_limits(source),
    )
    .await
    .unwrap();
}
async fn physical_empty(source: &SourceFixture, index: usize) {
    admissions!(
        source, index, cancel, request, _issuer, _caller, _authority, _read, write
    );
    let mut limits = write_limits(source);
    limits.max_row_bytes = NonZeroUsize::MIN;
    assert!(matches!(
        NativeMemoryStore::create(
            request.snapshot(),
            &write,
            input("refused-row", "empty"),
            limits
        )
        .await,
        Err(MemoryStoreError::Limit(_))
    ));
}

async fn client(
    server: &SourceServer,
    credential: &str,
    local: bool,
) -> (
    rmcp::service::RunningService<RoleClient, ()>,
    tokio::task::JoinHandle<io::Result<()>>,
) {
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let uri = format!("http://{}/mcp", listener.address());
    let owner = server.clone();
    let serving = tokio::spawn(async move { owner.serve(listener).await });
    let mut bearer = axum::http::HeaderValue::from_str(&format!("Bearer {credential}")).unwrap();
    bearer.set_sensitive(true);
    let headers = HashMap::from([
        (axum::http::header::AUTHORIZATION, bearer),
        (
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static(if local { "local" } else { "runtime" }),
        ),
    ]);
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::new(),
                StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap();
    (client, serving)
}
fn params(value: Value) -> CallToolRequestParams {
    CallToolRequestParams::new(TOOL).with_arguments(value.as_object().unwrap().clone())
}
async fn query(
    client: &rmcp::service::RunningService<RoleClient, ()>,
    mut value: Value,
    error: bool,
) -> Value {
    value["ai_optimized"] = json!(false);
    let result = client.call_tool(params(value)).await.unwrap();
    assert_eq!(result.is_error.unwrap_or(false), error, "{result:?}");
    result
        .structured_content
        .expect("closed structured Memory result")
}

#[tokio::test]
async fn actual_memory_modes_http_rpc_preserve_archive_channels_filters_and_unavailable() {
    let f = Fixture::new(true).await;
    let first = create(&f.source, 0, "archive-first", "Nebula").await;
    let other = create(&f.source, 0, "semantic-only", "unrelated").await;
    let mut changed = change(&first, "archive-second", "current unrelated body");
    changed.classification = Some(MemoryClass::Convention);
    changed.mandatory = Some(true);
    changed.important = Some(true);
    let second = update(&f.source, 0, &first, changed).await;
    embed(&f, 0).await;
    let (runtime, serving) = client(&f.source.server, &f.source.raw, false).await;
    assert!(
        runtime
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name == TOOL)
    );
    let lexical = query(&runtime, json!({"text":"Nebula","top_k":8}), false).await;
    assert_eq!(lexical["mode"], "fts");
    assert_eq!(lexical["total_hits"], 1);
    assert_eq!(lexical["hits"][0]["current"]["id"], json!(first.id));
    assert_eq!(lexical["hits"][0]["current"]["revision"], 2);
    assert_eq!(lexical["hits"][0]["matched"]["revision"], 1);
    assert_eq!(
        lexical["hits"][0]["location"],
        json!({"kind":"archived","revision":1})
    );
    let semantic = query(
        &runtime,
        json!({"text":"Nebula","top_k":8,"mode":"semantic"}),
        false,
    )
    .await;
    assert_eq!(semantic["kind"], "published");
    assert_eq!(semantic["total_hits"], 2);
    let (rank, semantic_hit) = semantic["hits"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .find(|(_, hit)| hit["current"]["id"] == json!(first.id))
        .unwrap();
    let rank = rank + 1;
    assert_eq!(semantic_hit["current"], lexical["hits"][0]["current"]);
    let semantic_revision = semantic_hit["matched_revision"].as_u64().unwrap();
    assert!([first.revision, second.revision].contains(&semantic_revision));
    let hybrid = query(
        &runtime,
        json!({"text":"Nebula","top_k":1,"mode":"hybrid"}),
        false,
    )
    .await;
    assert_eq!(hybrid["total_hits"], 2);
    assert_eq!(hybrid["hits"].as_array().unwrap().len(), 1);
    let hit = &hybrid["hits"][0];
    assert_eq!(hit["current"]["id"], json!(first.id));
    assert_eq!(hit["current"]["revision"], 2);
    assert_eq!(hit["lexical"]["matched_revision"], 1);
    // Fusion must preserve the semantic reader's chosen revision, independently
    // of the lexical match.
    assert_eq!(
        hit["semantic"]["matched_revision"],
        semantic_hit["matched_revision"]
    );
    assert_eq!(
        hit["lexical"]["location"],
        json!({"kind":"archived","revision":1})
    );
    assert_eq!(hit["semantic"]["location"], semantic_hit["location"]);
    assert!(
        (hit["semantic"]["distance"].as_f64().unwrap()
            - semantic_hit["distance"].as_f64().unwrap())
        .abs()
            < 1e-6
    );
    assert!(
        (hit["score"].as_f64().unwrap() - (1.0 / 61.0 + 1.0 / (60 + rank) as f64)).abs() < 1e-12
    );
    assert!(hit["lexical"]["score"].as_f64().unwrap().is_finite());
    assert!(hit["semantic"]["distance"].as_f64().unwrap().is_finite());
    assert_eq!(hybrid["generation"], semantic["generation"]);
    assert_eq!(hybrid["generation"], lexical["generation"]);
    let filtered = query(&runtime,json!({"text":"Nebula","top_k":8,"mode":"hybrid", "filter":{
        "classification":"fact","tag":"retained","mandatory":false,"important":false,"exclude_mandatory":true,
    }}),false).await;
    assert_eq!(filtered["total_hits"], 1);
    assert_eq!(filtered["hits"][0]["current"]["id"], json!(other.id));
    assert!(filtered["hits"][0]["lexical"].is_null());
    let empty = query(
        &runtime,
        json!({"text":"Nebula","top_k":8,"filter":{"mandatory":true,"exclude_mandatory":true}}),
        false,
    )
    .await;
    assert_eq!(empty["kind"], "published");
    assert_eq!(empty["total_hits"], 0);
    let second_project =
        serde_json::to_value(NativePath::new(f.source.projects[1].0.root().to_owned()).unwrap())
            .unwrap();
    let absent = query(&runtime,json!({"text":"Nebula","top_k":8,"mode":"hybrid","project_dir":second_project,"project_version":"main"}),false).await;
    assert_eq!(absent["kind"], "no_corpus");
    let (local, local_serving) = client(&f.source.server, "memory-local-fixture", true).await;
    let local_result = query(
        &local,
        json!({"text":"Nebula","top_k":1,"mode":"hybrid"}),
        false,
    )
    .await;
    assert_eq!(local_result["hits"][0]["current"], hit["current"]);
    let third = update(
        &f.source,
        0,
        &second,
        change(&second, "missing-coverage", "new unindexed body"),
    )
    .await;
    let unavailable = query(
        &runtime,
        json!({"text":"Nebula","top_k":8,"mode":"hybrid"}),
        false,
    )
    .await;
    assert_eq!(unavailable["kind"], "published");
    assert!(unavailable["total_hits"].as_u64().unwrap() > 0);
    let pending = unavailable["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| hit["current"]["id"] == json!(third.id))
        .unwrap();
    assert_eq!(pending["current"]["revision"], json!(third.revision));
    assert_eq!(
        pending["semantic"]["matched_revision"],
        semantic_hit["matched_revision"]
    );
    assert_eq!(
        pending["semantic"]["location"],
        json!({"kind":"archived","revision":semantic_revision})
    );
    assert!(
        (pending["semantic"]["distance"].as_f64().unwrap()
            - semantic_hit["distance"].as_f64().unwrap())
        .abs()
            < 1e-6
    );
    embed(&f, 0).await;
    {
        let source = &f.source;
        admissions!(
            source, 0, cancel, request, _issuer, _caller, _authority, _read, write
        );
        NativeMemoryStore::delete(
            request.snapshot(),
            &write,
            third.id,
            DeleteMemory {
                expected_revision: third.revision,
                idempotency_key: "delete-history".into(),
                reason: "fixture deletion".into(),
            },
            write_limits(source),
        )
        .await
        .unwrap();
    }
    let deleted = query(
        &runtime,
        json!({"text":"Nebula","top_k":8,"mode":"hybrid"}),
        false,
    )
    .await;
    assert_eq!(deleted["total_hits"], 1);
    let history = query(
        &runtime,
        json!({"text":"Nebula","top_k":8,"mode":"hybrid","include_deleted_history":true}),
        false,
    )
    .await;
    assert_eq!(history["total_hits"], 2);
    let history_hit = history["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| hit["current"]["id"] == json!(first.id))
        .unwrap();
    assert_eq!(history_hit["lexical"]["matched_revision"], 1);
    assert_eq!(history_hit["current"]["revision"], 4);
    incompatible_header_with_real_rpc_vectors(&f, 0).await;
    let incompatible = query(
        &runtime,
        json!({"text":"Nebula","top_k":8,"mode":"hybrid"}),
        false,
    )
    .await;
    assert_eq!(incompatible["reason"]["kind"], "incompatible_identity");
    runtime.cancel().await.unwrap();
    local.cancel().await.unwrap();
    f.source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    local_serving.await.unwrap().unwrap();
    f.close().await;
}

#[tokio::test]
async fn actual_memory_no_model_fts_and_closed_model_absence_empty_are_distinct() {
    for embedding in [true, false] {
        let mut source = super::super::test_fixture::fixture(
            &[Permission::Read(Module::Memory), Permission::WriteMemory],
            Brand::default(),
            NativeBinding::KiroV3,
        );
        configure(&mut source, embedding);
        let memory = create(&source, 0, "fts-without-model", "Nebula").await;
        assert!(source.server.0.semantic_model().unwrap().is_none());
        let (runtime, serving) = client(&source.server, &source.raw, false).await;
        let found = query(&runtime, json!({"text":"Nebula","top_k":1}), false).await;
        assert_eq!(found["hits"][0]["current"]["id"], json!(memory.id));
        for mode in ["semantic", "hybrid"] {
            let unavailable = query(
                &runtime,
                json!({"text":"Nebula","top_k":1,"mode":mode}),
                false,
            )
            .await;
            assert_eq!(unavailable["kind"], "unavailable");
            assert_eq!(
                unavailable["reason"]["kind"],
                if embedding {
                    "model_unavailable"
                } else {
                    "model_disabled"
                }
            );
            assert!(unavailable.get("generation").is_none());
        }
        runtime.cancel().await.unwrap();
        source.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
    let mut denied = super::super::test_fixture::fixture(
        &[Permission::Read(Module::Ast)],
        Brand::default(),
        NativeBinding::KiroV3,
    );
    configure(&mut denied, true);
    let (runtime, serving) = client(&denied.server, &denied.raw, false).await;
    assert_eq!(
        query(
            &runtime,
            json!({"text":"Nebula","top_k":1,"mode":"hybrid"}),
            true
        )
        .await["code"],
        "access_denied"
    );
    runtime.cancel().await.unwrap();
    denied.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    let mut f = Fixture::new(true).await;
    physical_empty(&f.source, 1).await;
    f.model.shutdown().await.unwrap();
    assert!(
        TcpStream::connect(f.resources.local.as_ref().unwrap().address)
            .await
            .is_err()
    );
    let (runtime, serving) = client(&f.source.server, &f.source.raw, false).await;
    let second =
        serde_json::to_value(NativePath::new(f.source.projects[1].0.root().to_owned()).unwrap())
            .unwrap();
    for mode in ["semantic", "hybrid"] {
        let absent = query(
            &runtime,
            json!({"text":"Nebula","top_k":1,"mode":mode}),
            false,
        )
        .await;
        assert_eq!(absent["kind"], "no_corpus");
        let empty = query(
            &runtime,
            json!({"text":"Nebula","top_k":1,"mode":mode,"project_dir":second}),
            false,
        )
        .await;
        assert_eq!(empty["kind"], "published_empty");
        assert!(empty["generation"]["sdk_version"].as_u64().is_some());
    }
    runtime.cancel().await.unwrap();
    f.source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    f.close().await;
}

#[tokio::test]
async fn actual_memory_fts_resolves_retained_version_without_crossing_project_identity() {
    use graphit_core::{
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::ProjectPaths,
        version_context::{Checkout, VersionContext},
    };

    let mut source = super::super::test_fixture::fixture_with_projects(
        &[
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
            Permission::Configure,
        ],
        Brand::default(),
        NativeBinding::KiroV3,
        &["main-checkout", "unrelated-project"],
    );
    configure(&mut source, false);
    let root = source.temp.path().join("feature-checkout");
    std::fs::create_dir(&root).unwrap();
    let paths = ProjectPaths::at(&root, &source.paths).unwrap();
    let cancel = Cancellation::default();
    let id = source.projects[0].1;
    ProjectIdentityStore::at(&paths)
        .writer(Duration::from_secs(5), &cancel)
        .unwrap()
        .ensure_with_id(&ProjectMetadata::new("feature-checkout").unwrap(), id)
        .unwrap();
    let version =
        VersionContext::select(&paths, Checkout::NonGit, "feature/worktree", None, "").unwrap();
    let mut projects = source.projects.as_ref().clone();
    projects.push((paths, id, version));
    source.projects = Arc::new(projects);
    source
        .server
        .live_projects()
        .replace(Arc::clone(&source.projects), 0)
        .unwrap();

    let main = create(&source, 0, "main-only", "MainOnlyMarker").await;
    let (runtime, serving) = client(&source.server, &source.raw, false).await;
    let current_root = NativePath::new(source.projects[0].0.root().to_owned()).unwrap();
    let selected_but_untrusted = query(
        &runtime,
        json!({"text":"FeatureOnlyMarker","top_k":4,
            "project_dir":current_root,"project_version":"feature/worktree"}),
        true,
    )
    .await;
    assert_eq!(selected_but_untrusted["code"], "access_denied");
    let registration = RequestContext::new(
        source.server.0.policy.content,
        &cancel,
        Instant::now() + source.server.0.policy.timeout,
    )
    .unwrap();
    let caller = source
        .auth
        .issuer()
        .authenticate_runtime(&source.raw, &registration)
        .unwrap();
    let scope = CapturedPublicationScope::project(
        &source.projects[2].0,
        id,
        source.server.0.policy.scope,
        registration.snapshot(),
    )
    .unwrap();
    let binding = scope.binding().unwrap();
    source
        .auth
        .trust_project_root(
            &caller,
            &registration,
            ScopeCapture {
                live: &scope,
                record: &binding,
            },
        )
        .unwrap();
    let feature = create(&source, 2, "feature-only", "FeatureOnlyMarker").await;
    assert_ne!(main.id, feature.id);
    let found = query(
        &runtime,
        json!({"text":"FeatureOnlyMarker","top_k":4,
            "project_dir":current_root,"project_version":"feature/worktree"}),
        false,
    )
    .await;
    assert_eq!(found["kind"], "published");
    assert_eq!(found["generation"]["project_version"], "feature/worktree");
    assert_eq!(found["hits"][0]["current"]["id"], json!(feature.id));
    let main_view = query(
        &runtime,
        json!({"text":"FeatureOnlyMarker","top_k":4}),
        false,
    )
    .await;
    assert_eq!(main_view["total_hits"], 0);

    for project_dir in [
        NativePath::new(source.projects[1].0.root().to_owned()).unwrap(),
        NativePath::new(source.temp.path().join("not-retained")).unwrap(),
    ] {
        let refused = query(
            &runtime,
            json!({"text":"FeatureOnlyMarker","top_k":4,
                "project_dir":project_dir,"project_version":"feature/worktree"}),
            true,
        )
        .await;
        assert_eq!(refused["code"], "invalid_query");
    }
    let mut ambiguous = source.projects.as_ref().clone();
    ambiguous.push(ambiguous[2].clone());
    source
        .server
        .live_projects()
        .replace(Arc::new(ambiguous), 0)
        .unwrap();
    let refused = query(
        &runtime,
        json!({"text":"FeatureOnlyMarker","top_k":4,
            "project_dir":current_root,"project_version":"feature/worktree"}),
        true,
    )
    .await;
    assert_eq!(refused["code"], "invalid_query");
    runtime.cancel().await.unwrap();
    source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn actual_memory_closed_wire_separate_service_permission_deadline_and_cancel_join() {
    let mut f = Fixture::new(false).await;
    Arc::get_mut(&mut f.source.server.0).unwrap().policy.timeout = Duration::from_millis(500);
    let (runtime, serving) = client(&f.source.server, &f.source.raw, false).await;
    for value in [
        json!({"text":"","top_k":1}),
        json!({"text":" \t","top_k":1}),
        json!({"text":"x\u{0000}","top_k":1}),
        json!({"text":"x","top_k":0}),
        json!({"text":null,"top_k":1}),
        json!({"text":"x","top_k":1,"mode":"unknown"}),
        json!({"text":"x","top_k":1,"unknown":true}),
        json!({"text":"x","top_k":1,"project_dir":null}),
        json!({"text":"x","top_k":1,"filter":{"mandatory":null}}),
        json!({"text":"x","top_k":1,"filter":{"unknown":true}}),
        json!({"text":"x","top_k":65}),
        json!({"text":"x".repeat(4097),"top_k":1}),
        json!({"text":"x","top_k":1,"project_version":"missing-version"}),
    ] {
        let invalid = query(&runtime, value, true).await;
        assert!(matches!(
            invalid["code"].as_str(),
            Some("invalid_arguments" | "invalid_query")
        ));
    }
    let (local, local_serving) = client(&f.source.server, "memory-local-fixture", true).await;
    assert_eq!(
        query(&local, json!({"text":"x","top_k":1}), false).await["kind"],
        "no_corpus"
    );
    assert_eq!(
        query(
            &local,
            json!({"text":"x","top_k":1,"mode":"semantic"}),
            true
        )
        .await["code"],
        "service_denied"
    );
    let (bad, bad_serving) = client(&f.source.server, "invalid-runtime-bearer", false).await;
    assert_eq!(
        query(&bad, json!({"text":"x","top_k":1}), true).await["code"],
        "access_denied"
    );
    bad.cancel().await.unwrap();
    // The registry protects retained handles, not semantic computation.
    let held = f.source.server.0.jobs.lock().await;
    assert_eq!(
        query(
            &runtime,
            json!({"text":"x","top_k":1,"mode":"hybrid"}),
            true
        )
        .await["code"],
        "request_cancelled_or_expired"
    );
    drop(held);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let mut jobs = f.source.server.0.jobs.lock().await;
            jobs.reap();
            if jobs.tasks.is_empty() {
                break;
            }
            drop(jobs);
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let held = f.source.server.0.jobs.lock().await;
    let pending = runtime
        .send_cancellable_request(
            rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(params(
                json!({"text":"pending","top_k":1,"mode":"hybrid"}),
            ))),
            Default::default(),
        )
        .await
        .unwrap();
    pending.cancel(None).await.unwrap();
    drop(held);
    tokio::time::timeout(Duration::from_secs(5), f.source.server.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(f.source.server.0.jobs.lock().await.tasks.is_empty());
    runtime.cancel().await.unwrap();
    local.cancel().await.unwrap();
    serving.await.unwrap().unwrap();
    local_serving.await.unwrap().unwrap();
    bad_serving.await.unwrap().unwrap();
    f.close().await;
}
