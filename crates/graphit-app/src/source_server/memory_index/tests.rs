use super::*;
use crate::{mcp_source::SourceProxy, source_server::memory_search::tests as search_fixture};
use graphit_auth::{Permission, profile::*};
use graphit_core::brand::Brand;
use graphit_daemon::broker_verifiers::BrokerVerifierCache;
use rmcp::ServiceExt;

// Reuse the signed Broker used by the Native installation journeys. Its
// UserInfo and refresh endpoints are absent: authentication must use the JWT.
use crate::native_install_http::auth_fixture as native_support;

type SourceFixture = super::super::test_fixture::Fixture;

fn request(cancel: &Cancellation) -> RequestContext<'_> {
    let config = ConfigSnapshot::new(&Brand::default(), []).unwrap();
    RequestContext::new(
        graphit_hub::policy::HubResourcePolicy::resolve(&config)
            .unwrap()
            .content(),
        cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap()
}

struct Connection {
    client: rmcp::service::RunningService<rmcp::RoleClient, ()>,
    proxy: SourceProxy,
    proxy_task: tokio::task::JoinHandle<io::Result<()>>,
    serving: tokio::task::JoinHandle<io::Result<()>>,
}
impl Connection {
    async fn new(source: &SourceFixture, request: &RequestContext<'_>) -> Self {
        let listener = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            request.cancellation(),
        )
        .unwrap();
        let address = listener.address();
        let server = source.server.clone();
        let serving = tokio::spawn(async move { SourceServer::serve(&server, listener).await });
        let (paths, _, version) = &source.projects[0];
        let proxy = SourceProxy::new(
            source.paths.clone(),
            address,
            reqwest::Client::new(),
            (
                source.server.0.policy.index,
                source.server.0.policy.content,
                source.server.0.policy.timeout,
            ),
            Some((paths.root(), Some(version.version()))),
            request,
        )
        .await
        .unwrap();
        let (local, remote) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(remote);
        let owner = proxy.clone();
        let proxy_task = tokio::spawn(async move { owner.serve_stream(read, write).await });
        let client = ().serve(local).await.unwrap();
        Self {
            client,
            proxy,
            proxy_task,
            serving,
        }
    }
    async fn tool(&self, name: &str, input: Value) -> CallToolResult {
        self.client
            .peer()
            .call_tool(
                CallToolRequestParams::new(name.to_owned())
                    .with_arguments(input.as_object().unwrap().clone()),
            )
            .await
            .unwrap()
    }
    async fn close(self) -> tokio::task::JoinHandle<io::Result<()>> {
        self.client.cancel().await.unwrap();
        self.proxy_task.await.unwrap().unwrap();
        self.proxy.shutdown().await.unwrap();
        self.serving
    }
}
fn payload(result: &CallToolResult) -> &Value {
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.as_ref().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_index_ensure_refresh_search_and_disabled_selection_use_real_proxy() {
    let mut source = super::super::test_fixture::fixture(
        &[Permission::Read(Module::Memory), Permission::WriteMemory],
        Brand::default(),
        NativeBinding::Codex,
    );
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let config = ConfigSnapshot::new(source.paths.brand(), []).unwrap();
    source.server.attach_project_hook(&config).unwrap();
    search_fixture::create(
        &source,
        0,
        "project-index",
        "indexneedle authoritative corpus",
    )
    .await;
    let connection = Connection::new(&source, &request).await;
    let ensured = connection.tool(TOOL, json!({})).await;
    let ensured = payload(&ensured);
    assert_eq!(ensured["scope"], "project");
    assert_eq!(ensured["lexical"]["state"], "ready");
    assert_eq!(ensured["semantic"]["state"], "not_configured");
    let pin = ensured["lexical"]["generation"]["sdk_version"]
        .as_u64()
        .unwrap();
    let repeated = connection.tool(TOOL, json!({})).await;
    assert_eq!(
        payload(&repeated)["lexical"]["generation"]["sdk_version"],
        pin
    );
    let refreshed_result = connection.tool(TOOL, json!({"refresh":true})).await;
    let refreshed = payload(&refreshed_result);
    assert_eq!(refreshed["lexical"]["state"], "published");
    assert!(refreshed["lexical"]["recovery_key"].is_object());
    let refreshed_pin = refreshed["lexical"]["generation"]["sdk_version"]
        .as_u64()
        .unwrap();
    assert!(refreshed_pin > pin);
    crate::memory_index_entry::tests::assert_publication_delivery(&refreshed_result);
    // A cancellation observed together with the completed result must not
    // become success or discard this actual SDK publication and receipt.
    let late = finish_response(Ok(Ok(refreshed.clone())), true);
    assert_eq!(late.is_error, Some(true));
    let late = late.structured_content.unwrap();
    assert_eq!(late["code"], "request_cancelled_or_expired");
    assert_eq!(late["result"]["lexical"], refreshed["lexical"]);
    let found = connection
        .tool(
            super::super::memory_search::TOOL,
            json!({"text":"indexneedle","top_k":4,"mode":"fts"}),
        )
        .await;
    assert_eq!(payload(&found)["hits"].as_array().unwrap().len(), 1);
    let project = ConfigStore::project(&source.projects[0].0).unwrap();
    project
        .set(
            Module::Memory.key(),
            json!(false),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    let disabled = connection.tool(TOOL, json!({"refresh":true})).await;
    assert_eq!(disabled.is_error, Some(true));
    assert_eq!(
        disabled.structured_content.as_ref().unwrap()["code"],
        "module_disabled"
    );
    project
        .set(
            Module::Memory.key(),
            json!(true),
            Duration::from_secs(5),
            &cancel,
        )
        .unwrap();
    let unchanged = connection.tool(TOOL, json!({})).await;
    assert_eq!(
        payload(&unchanged)["lexical"]["generation"]["sdk_version"],
        refreshed_pin
    );
    let serving = connection.close().await;
    source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fenced_memory_permission_does_not_authorize_explicit_ordinary_index() {
    let source = super::super::test_fixture::fixture(
        &[
            Permission::Read(Module::Memory),
            Permission::WriteFencedMemory,
        ],
        Brand::default(),
        NativeBinding::Codex,
    );
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let connection = Connection::new(&source, &request).await;
    let denied = connection.tool(TOOL, json!({})).await;
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(
        denied.structured_content.as_ref().unwrap()["code"],
        "access_denied"
    );
    let no_corpus = connection
        .tool(
            super::super::memory_search::TOOL,
            json!({"text":"absent","top_k":1,"mode":"fts"}),
        )
        .await;
    assert_eq!(payload(&no_corpus)["kind"], "no_corpus");
    let serving = connection.close().await;
    source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

fn native_store(source: &SourceFixture, request: &RequestContext<'_>) -> AuthStore {
    let StoreOwner::Installation { installation_id } = source.auth.installation().live.owner()
    else {
        unreachable!()
    };
    let live = CapturedPublicationScope::installation(
        &source.paths,
        installation_id.clone(),
        source
            .auth
            .installation()
            .live
            .allowed_roots()
            .map(ToOwned::to_owned),
        source.server.0.policy.scope,
        request.snapshot(),
    )
    .unwrap();
    AuthStore::new(
        AuthDocumentStore::capture(
            live,
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
    .unwrap()
}
fn native_profile(
    store: &AuthStore,
    broker: &native_support::Broker,
    audience: &str,
    request: &RequestContext<'_>,
) {
    let binding = BrokerService {
        endpoint: broker.origin.clone(),
        mcp_resource: None,
        allow_anonymous: false,
    };
    let expiry = native_support::now() + 600;
    let data = ProfileData {
        name: "native".into(),
        provider: "service".into(),
        username: "display-not-principal".into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Broker {
            issuer: broker.origin.clone(),
            subject: "native-fixture-sub".into(),
            s3_disabled: true,
            tokens: OidcTokens {
                access_token: Secret::new(broker.token(expiry, audience).into()),
                refresh_token: None,
                id_token: Secret::new("historical-fixture-id-token".into()),
                expires_at: expiry.try_into().unwrap(),
            },
        },
    };
    let result = store
        .mutate(store.load(request).unwrap(), request, |doc, request| {
            if doc.provider("service").is_none() {
                doc.add_provider(
                    "service".into(),
                    ProviderConfig {
                        kind: ProviderKind::Broker(binding),
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
                    request,
                )?;
            }
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("service").unwrap().revision(),
                profile_revision: doc.profile("native").map(|profile| profile.revision()),
            };
            let updated = doc
                .profile("native")
                .map_or(1, |profile| profile.updated_at() + 1);
            doc.replace_profile_data(data, expected, updated, request)?;
            doc.activate("native", doc.revision(), request)
        })
        .unwrap();
    assert!(result.failure().is_none() && result.cleanup_failure().is_none());
}
fn cache(source: &SourceFixture) -> Arc<BrokerVerifierCache> {
    Arc::new(BrokerVerifierCache::new(
        graphit_http::client::Limits {
            request_body_bytes: 8192,
            response_body_bytes: 8192,
            url_bytes: 2048,
            header_bytes: 16384,
            header_count: 32,
            max_in_flight: 8,
            max_idle_per_host: 2,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 2,
            max_certificate_bytes: 8192,
        },
        crate::admin_auth::verifier_limits(source.server.0.policy.content),
        true,
        None,
        &[],
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verified_user_index_through_project_selected_proxy_preserves_subject_and_warm_jwks() {
    let broker = native_support::Broker::start().await;
    let mut source = super::super::test_fixture::fixture(
        &[
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
            Permission::ReadUser,
            Permission::MutateUser,
        ],
        Brand::default(),
        NativeBinding::Codex,
    );
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let store = native_store(&source, &request);
    native_profile(&store, &broker, "broker-api", &request);
    let cache = cache(&source);
    source
        .server
        .attach_native_verifiers(Arc::clone(&cache))
        .unwrap();
    let caller = source
        .auth
        .issuer()
        .authenticate_runtime(&source.raw, &request)
        .unwrap();
    let (binding, revision) = caller.native_provider().unwrap();
    let verifier = cache.get_native(&binding, revision).unwrap();
    assert!(Arc::ptr_eq(
        &verifier,
        &cache.get_native(&binding, revision).unwrap()
    ));
    assert_eq!(broker.requests(), [0, 0, 0]);
    let caller = caller.authenticate_native(&verifier).await.unwrap();
    let connection = Connection::new(&source, &request).await;
    let absent = connection.tool(TOOL, json!({"scope":"user"})).await;
    assert_eq!(payload(&absent)["scope"], "user");
    assert_eq!(payload(&absent)["lexical"]["state"], "no_corpus");
    assert!(payload(&absent).get("project").is_none());
    let warm = broker.requests();
    assert_eq!(warm, [1, 1, 1]);
    let issuer = source.auth.issuer();
    let subject = issuer.user_from_authenticated(&caller).unwrap();
    assert_eq!(subject.subject().unwrap(), "native-fixture-sub");
    let StoreOwner::Installation { installation_id } = source.auth.installation().live.owner()
    else {
        unreachable!()
    };
    let live = CapturedPublicationScope::user(
        &source.paths,
        installation_id.clone(),
        UserIdentity::new(
            subject.issuer().unwrap().to_owned(),
            subject.subject().unwrap().to_owned(),
        )
        .unwrap(),
        source.server.0.policy.scope,
        request.snapshot(),
    )
    .unwrap();
    let record = live.binding().unwrap();
    let scope = ScopeCapture {
        live: &live,
        record: &record,
    };
    let write = issuer
        .admit_authenticated_user_memory_write(
            &caller,
            &request,
            scope,
            UserMemoryWriteOperation::Memory,
        )
        .unwrap();
    let limits = write_limits(&source.server.0).unwrap_or_else(|error| panic!("{}", error.0));
    NativeUserMemoryStore::create(
        request.snapshot(),
        &write,
        CreateMemory {
            idempotency_key: "user-index".into(),
            title: "User index".into(),
            body: "userindexneedle belongs to verified subject".into(),
            attributes: graphit_records::memory_attributes::MemoryAttributes {
                classification: graphit_records::memory_attributes::MemoryClass::Fact,
                mandatory: false,
                important: false,
            },
            tags: vec![],
            references: None,
        },
        limits,
    )
    .await
    .unwrap();
    let ensured = connection.tool(TOOL, json!({"scope":"user"})).await;
    assert_eq!(payload(&ensured)["lexical"]["state"], "ready");
    assert_eq!(payload(&ensured)["semantic"]["state"], "not_configured");
    let generation: UserMemoryGeneration =
        serde_json::from_value(payload(&ensured)["lexical"]["generation"].clone()).unwrap();
    assert_eq!(generation.owner.identity.subject(), "native-fixture-sub");
    let refreshed = connection
        .tool(TOOL, json!({"scope":"user","refresh":true}))
        .await;
    assert_eq!(payload(&refreshed)["lexical"]["state"], "published");
    let read = issuer
        .admit_authenticated_user_memory_read(&caller, &request, scope)
        .unwrap();
    let found = NativeUserMemoryStore::search_lexical(
        request.snapshot(),
        &read,
        MemoryLexicalQuery {
            text: "userindexneedle".into(),
            filter: MemoryListFilter::default(),
            include_deleted_history: false,
            top_k: NonZeroUsize::new(4).unwrap(),
        },
        limits.read,
    )
    .await
    .unwrap();
    assert!(matches!(
        found,
        UserMemoryLexicalSearchRead::Published { total_hits: 1, .. }
    ));
    assert_eq!(broker.requests(), warm);
    let mixed = connection
        .tool(TOOL, json!({"scope":"user","project_version":"main"}))
        .await;
    assert_eq!(mixed.is_error, Some(true));
    assert_eq!(
        mixed.structured_content.as_ref().unwrap()["code"],
        "invalid_arguments"
    );
    native_profile(&store, &broker, "other-service", &request);
    let wrong_audience = connection.tool(TOOL, json!({"scope":"user"})).await;
    assert_eq!(wrong_audience.is_error, Some(true));
    assert_eq!(
        wrong_audience.structured_content.as_ref().unwrap()["code"],
        "service_denied"
    );
    assert_eq!(broker.requests(), warm);
    let serving = connection.close().await;
    source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
    broker.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires GRAPHIT_TEST_MODEL_SERVER and the existing real model fixture"]
async fn configured_index_uses_existing_model_and_preserves_lexical_publication_on_model_failure() {
    let mut fixture = search_fixture::Fixture::new(true).await;
    search_fixture::local_profile(&fixture.source);
    search_fixture::create(&fixture.source, 0, "model-index", "model index first text").await;
    let cancel = Cancellation::default();
    let request = request(&cancel);
    let connection = Connection::new(&fixture.source, &request).await;
    let indexed = connection.tool(TOOL, json!({})).await;
    assert_eq!(payload(&indexed)["lexical"]["state"], "ready");
    assert_eq!(payload(&indexed)["semantic"]["state"], "complete");
    assert_eq!(
        payload(&indexed)["semantic"]["last_attachment"]["state"],
        "published"
    );
    let repeated = connection.tool(TOOL, json!({})).await;
    assert_eq!(payload(&repeated)["semantic"]["state"], "complete");
    assert!(payload(&repeated)["semantic"]["last_attachment"].is_null());
    search_fixture::create(
        &fixture.source,
        0,
        "model-index-second",
        "second new document needs inference",
    )
    .await;
    fixture.model.shutdown().await.unwrap();
    let failed = connection.tool(TOOL, json!({"refresh":true})).await;
    assert_eq!(failed.is_error, Some(true));
    let failed = failed.structured_content.as_ref().unwrap();
    assert_eq!(failed["code"], "model_failed");
    assert_eq!(failed["result"]["lexical"]["state"], "published");
    assert_eq!(failed["result"]["semantic"]["state"], "failed");
    assert!(failed["result"]["lexical"]["recovery_key"].is_object());
    let serving = connection.close().await;
    fixture.close().await;
    serving.await.unwrap().unwrap();
}
