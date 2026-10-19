//! Product consumers over independently owned installations and authoritative S3 tables.
//! Run explicitly with GRAPHIT_TEST_S3_FIXTURE pointing at a private, short-lived
//! local S3/STS fixture. Credentials never appear in test output or repository files.
#[path = "record_storage_tests/embeddings.rs"]
mod embeddings;
#[path = "record_storage_tests/git_records.rs"]
mod git_records;
#[path = "record_storage_tests/native_refresh.rs"]
mod native_refresh;
#[path = "record_storage_tests/transports.rs"]
mod transports;
use super::*;
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::{
    AdminSessionPolicy, CredentialSource, IncomingPrincipalInput, IssuerAuthorityPolicy,
    IssuerLimits, Permission, ProjectWriteOperation, RemoteServiceBindingInput,
    ServiceAdmissionLimits, ServiceAuthorityInput, ServiceOrigin, ServicePurpose, ServiceRuleInput,
    SourceAuthorityPolicy, SubjectSelector, TtlPolicy,
    broker::{JsonLimits, VerifierLimits},
    profile::{
        AiService, BrokerDeviceMetadata, BrokerDeviceProfileData, BrokerDeviceTokens,
        BrokerService, ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location,
        S3Source, Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    installation_bootstrap::BootstrapLimits,
    module_settings::Module,
    project_paths::InstallationPaths,
    runtime_key::RuntimeKeyLimits,
};
use graphit_daemon::{auth_startup::StartupPolicy, broker_verifiers::BrokerVerifierCache};
use rmcp::{
    RoleClient, ServiceExt,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use secrecy::SecretString;
use serde::Deserialize;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Deserialize)]
struct S3Fixture {
    endpoint: String,
    bucket: String,
    region: String,
    access_key_id: String,
    secret_access_key: String,
    session_token: String,
    #[serde(rename = "expires_at")]
    _expires_at: String,
}
impl S3Fixture {
    fn load() -> Self {
        let path = std::env::var_os("GRAPHIT_TEST_S3_FIXTURE")
            .expect("explicit local S3/temporary-STS fixture is required");
        let bytes = zeroize::Zeroizing::new(std::fs::read(path).expect("private S3 fixture"));
        serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!("valid private S3 fixture"))
    }
}

struct BrokerState {
    origin: String,
    key: Ed25519KeyPair,
    s3: S3Fixture,
    prefix: String,
    requests: Mutex<Vec<Value>>,
    calls: AtomicUsize,
    ttl: AtomicU64,
    status: Mutex<StatusCode>,
    s3_endpoint: Mutex<String>,
    invalid_response: AtomicBool,
    hold: AtomicBool,
    entered: tokio::sync::Notify,
    released: tokio::sync::Notify,
    storage_enabled: AtomicBool,
    web: Mutex<transports::WebState>,
    web_sts_calls: AtomicUsize,
    incoming_sts_calls: AtomicUsize,
    incoming_project_calls: AtomicUsize,
    project_grant: AtomicBool,
    native_refresh: native_refresh::RefreshState,
}
struct Broker {
    state: Arc<BrokerState>,
    serving: tokio::task::JoinHandle<()>,
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.serving.abort();
    }
}
impl Broker {
    async fn start(s3: S3Fixture) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let key = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let state = Arc::new(BrokerState {
            origin,
            key,
            s3_endpoint: Mutex::new(s3.endpoint.clone()),
            s3,
            prefix: format!(
                "records-product/ação %literal/{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            requests: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
            ttl: AtomicU64::new(600),
            status: Mutex::new(StatusCode::OK),
            invalid_response: AtomicBool::new(false),
            hold: AtomicBool::new(false),
            entered: tokio::sync::Notify::new(),
            released: tokio::sync::Notify::new(),
            storage_enabled: AtomicBool::new(true),
            web: Mutex::new(transports::WebState::default()),
            web_sts_calls: AtomicUsize::new(0),
            incoming_sts_calls: AtomicUsize::new(0),
            incoming_project_calls: AtomicUsize::new(0),
            project_grant: AtomicBool::new(true),
            native_refresh: native_refresh::RefreshState::default(),
        });
        let router = Router::new()
            .route("/.well-known/graphit-broker", axum::routing::get(discovery))
            .route(
                "/.well-known/openid-configuration",
                axum::routing::get(openid),
            )
            .route("/keys", axum::routing::get(keys))
            .route("/v1/s3/credentials", post(sts))
            .route("/register", post(transports::register))
            .route("/authorize", axum::routing::get(transports::authorize))
            .route("/token", post(native_refresh::tokens))
            .route(
                "/userinfo",
                axum::routing::get(|| async { StatusCode::SERVICE_UNAVAILABLE }),
            )
            .route(
                "/v1/project/access/authorize",
                post(transports::project_access),
            )
            .with_state(Arc::clone(&state));
        let serving = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { state, serving }
    }
    fn token(&self, subject: &str) -> String {
        self.token_for(subject, None)
    }
    fn token_for(&self, subject: &str, resource: Option<&str>) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut audiences = vec!["broker-api"];
        audiences.extend(resource);
        let claims = json!({"iss":self.state.origin,"sub":subject,"aud":audiences,
            "client_id":"graphit-cli","jti":"s3-product-token","token_use":"access",
            "iat":now.saturating_sub(1),"exp":now+3600,"preferred_username":subject});
        transports::signed(&self.state, claims)
    }
}
async fn discovery(AxumState(state): AxumState<Arc<BrokerState>>) -> Json<Value> {
    let mut value = json!({"version":"1","issuer":state.origin,
        "authentication":{"type":"oidc","issuer":state.origin,"client_id":"graphit-cli",
            "scopes":["openid","profile"],"redirect_uri_path":"/callback",
            "audiences":["broker-api"],"access_token_audience":"broker-api",
            "mcp_resources":[format!("{}/v1",state.origin)]},
        "services":{"s3_credentials":{"protocol":"graphit-s3-credentials-v3","path":"/v1/s3/credentials"}}});
    if !state.storage_enabled.load(Ordering::SeqCst) {
        value.as_object_mut().unwrap().remove("services");
    }
    Json(value)
}
async fn openid(AxumState(state): AxumState<Arc<BrokerState>>) -> Json<Value> {
    Json(
        json!({"issuer":state.origin,"authorization_endpoint":format!("{}/authorize",state.origin),
        "registration_endpoint":format!("{}/register",state.origin),
        "token_endpoint":format!("{}/token",state.origin),"jwks_uri":format!("{}/keys",state.origin),
        "userinfo_endpoint":format!("{}/userinfo",state.origin),
        "grant_types_supported":["authorization_code","refresh_token"],
        "code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],
        "id_token_signing_alg_values_supported":["EdDSA"]}),
    )
}
async fn keys(AxumState(state): AxumState<Arc<BrokerState>>) -> Json<Value> {
    Json(json!({"keys":[{"kty":"OKP","crv":"Ed25519",
        "x":URL_SAFE_NO_PAD.encode(state.key.public_key().as_ref()),"kid":"actual","alg":"EdDSA",
        "use":"sig","key_ops":["verify"]}]}))
}
async fn sts(
    AxumState(state): AxumState<Arc<BrokerState>>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> (StatusCode, Json<Value>) {
    state.calls.fetch_add(1, Ordering::SeqCst);
    state.requests.lock().unwrap().push(input.clone());
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("Bearer ")
        .unwrap();
    let (message, signature) = bearer.rsplit_once('.').unwrap();
    aws_lc_rs::signature::UnparsedPublicKey::new(
        &aws_lc_rs::signature::ED25519,
        state.key.public_key().as_ref(),
    )
    .verify(
        message.as_bytes(),
        &URL_SAFE_NO_PAD.decode(signature).unwrap(),
    )
    .unwrap();
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(message.split_once('.').unwrap().1)
            .unwrap(),
    )
    .unwrap();
    let subject = claims["sub"].as_str().unwrap();
    if claims["client_id"] == "web-client" {
        state.web_sts_calls.fetch_add(1, Ordering::SeqCst);
    }
    if subject == "incoming-subject" {
        state.incoming_sts_calls.fetch_add(1, Ordering::SeqCst);
    }
    if state.hold.load(Ordering::SeqCst) {
        state.entered.notify_one();
        state.released.notified().await;
    }
    let status = *state.status.lock().unwrap();
    if status != StatusCode::OK {
        return (
            status,
            Json(json!({"error":"fixture authorization denied"})),
        );
    }
    let scope = input["scope"].as_str().expect("scoped STSv3 request");
    let module = input["module"].as_str().expect("physical STSv3 module");
    assert!(matches!(module, "memory" | "task"));
    let owner = if scope == "project" {
        input["project_id"].as_str().unwrap()
    } else {
        input["installation_id"].as_str().unwrap()
    };
    let expires = SystemTime::now() + Duration::from_secs(state.ttl.load(Ordering::SeqCst));
    let namespace = if scope == "user" {
        format!("{owner}/{subject}")
    } else {
        owner.to_owned()
    };
    let mut output = json!({"access_key_id":state.s3.access_key_id,
        "secret_access_key":state.s3.secret_access_key,"session_token":state.s3.session_token,
        "expires_at":chrono::DateTime::<chrono::Utc>::from(expires).to_rfc3339(),
        "bucket":state.s3.bucket,"region":state.s3.region,"endpoint":*state.s3_endpoint.lock().unwrap(),
        "conditional_delete":true,"copy_if_not_exists":"multipart",
        "prefixes":[format!("{}/{scope}/{namespace}/{module}/",state.prefix)],
        "authorization_revision":"1","scope":scope,"project_id":input["project_id"].as_str().unwrap_or(""),"module":module});
    if scope == "user" {
        output["installation_id"] = input["installation_id"].clone();
    }
    if state.invalid_response.load(Ordering::SeqCst) {
        output["module"] = json!("wrong-module");
    }
    (status, Json(output))
}

fn startup_with_remote(endpoint: &str, allow_remote: bool) -> StartupPolicy {
    let mut permissions = vec![
        Permission::Read(Module::Task),
        Permission::WriteTask,
        Permission::WriteOwnTask,
        Permission::CoordinateTask,
        Permission::Read(Module::Memory),
        Permission::WriteMemory,
        Permission::ReadUser,
        Permission::MutateUser,
        Permission::ManageAuthProfiles,
        Permission::ManageAccessGrants,
        Permission::Delegate,
    ];
    if allow_remote {
        permissions.push(Permission::External(
            graphit_auth::ExternalEffect::RemoteRequest,
        ));
    }
    let source = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &permissions,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    let resource = format!("{endpoint}/v1");
    let rules = [
        ServiceOrigin::Runtime,
        ServiceOrigin::Local,
        ServiceOrigin::Web,
        ServiceOrigin::BrokerIncoming,
    ]
    .map(|origin| ServiceRuleInput::Remote {
        origin,
        binding: RemoteServiceBindingInput::Broker {
            provider: if origin == ServiceOrigin::BrokerIncoming {
                "incoming"
            } else {
                "broker"
            },
            purpose: ServicePurpose::RecordStorage,
            endpoint,
            resource: (origin == ServiceOrigin::BrokerIncoming).then_some(resource.as_str()),
        },
        credential: if matches!(origin, ServiceOrigin::Web | ServiceOrigin::BrokerIncoming) {
            CredentialSource::IncomingBearer
        } else {
            CredentialSource::NativeAccess
        },
        incoming: (origin == ServiceOrigin::BrokerIncoming).then_some(IncomingPrincipalInput {
            issuer: endpoint,
            subject: SubjectSelector::Exact("incoming-subject"),
            incoming_resource: &resource,
        }),
    });
    StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope: ScopeLimits::new(16, 4096, 256).unwrap(),
        document: AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
        profiles: profile_limits(),
        authority: IssuerAuthorityPolicy::new(
            source(),
            source(),
            IssuerLimits {
                max_permissions: 64,
                max_scope_bytes: 65536,
                max_parent_bearer_bytes: 4096,
            },
            ServiceAuthorityInput {
                rules: &rules,
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 8192,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap(),
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    }
}
fn profile_limits() -> ProfileLimits {
    ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 8192,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn verifier_cache() -> Arc<BrokerVerifierCache> {
    Arc::new(BrokerVerifierCache::new(
        graphit_http::client::Limits {
            request_body_bytes: 65536,
            response_body_bytes: 65536,
            url_bytes: 8192,
            header_bytes: 16384,
            header_count: 32,
            max_in_flight: 8,
            max_idle_per_host: 2,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 2,
            max_certificate_bytes: 8192,
        },
        VerifierLimits {
            token_bytes: 8192,
            json: JsonLimits {
                bytes: 65536,
                nodes: 4096,
                depth: 32,
                scalar_bytes: 8192,
                collection_entries: 64,
            },
            max_keys: 8,
            metadata_ttl: Duration::from_secs(300),
            key_ttl: Duration::from_secs(300),
            unknown_key_cooldown: Duration::from_millis(100),
        },
        true,
        None,
        &[],
    ))
}

struct Client {
    _directory: tempfile::TempDir,
    paths: InstallationPaths,
    server: TaskServer,
    raw: String,
    incoming_verifier: Arc<graphit_auth::broker::BrokerVerifier>,
}
impl Client {
    fn new(broker: &Broker, identity: Option<&Client>) -> Self {
        Self::new_with_remote(broker, identity, true)
    }
    fn new_with_remote(broker: &Broker, identity: Option<&Client>, allow_remote: bool) -> Self {
        let (directory, old, _) = tests::fixture_with_root_names(
            &[Permission::Read(Module::Memory), Permission::WriteMemory],
            &["other"],
        );
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let policy = old.0.policy.clone();
        let mut projects = old
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live storage test selection"))
            .projects
            .as_ref()
            .clone();
        // The bare fixture trusts additional roots but selects only its primary
        // project. Register the second identity/version for actual isolation reads.
        let other =
            graphit_core::project_paths::ProjectPaths::at(directory.path().join("other"), &paths)
                .unwrap();
        let cancellation = Cancellation::default();
        let other_id = graphit_core::project_identity::ProjectIdentityStore::at(&other)
            .writer(Duration::from_secs(5), &cancellation)
            .unwrap()
            .ensure(&graphit_core::project_identity::ProjectMetadata::new("other").unwrap())
            .unwrap()
            .id();
        let checkout = graphit_core::git_checkout::GitCheckout::inspect(
            other.root(),
            Duration::from_secs(5),
            &cancellation,
        )
        .unwrap();
        let other_version = graphit_core::version_context::VersionContext::select(
            &other,
            checkout.as_checkout(),
            "",
            None,
            "",
        )
        .unwrap();
        projects.push((other, other_id, other_version));
        // Replace the test fixture's startup policy only after its owner releases
        // the installation singleton; independent installations retain that lock.
        drop(old);
        if let Some(a) = identity {
            let original = a
                .server
                .record_snapshot()
                .unwrap_or_else(|_| panic!("live storage test selection"));
            std::fs::copy(
                original.projects[0].0.identity_file(),
                projects[0].0.identity_file(),
            )
            .unwrap();
            projects[0].1 = original.projects[0].1;
        }
        let cancel = Cancellation::default();
        let request = RequestContext::new(
            policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let auth = Arc::new(
            DaemonAuth::start(
                &paths,
                &projects.iter().map(|p| p.0.clone()).collect::<Vec<_>>(),
                startup_with_remote(&broker.state.origin, allow_remote),
                &request,
            )
            .unwrap(),
        );
        let raw = String::from_utf8(
            auth.runtime()
                .check(request.snapshot())
                .unwrap()
                .secret(request.snapshot())
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        let scope = auth.installation().live.clone();
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                scope,
                AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .unwrap(),
            profile_limits(),
            &request,
        )
        .unwrap();
        let publication = store
            .mutate(store.load(&request).unwrap(), &request, |doc, ctx| {
                for (name, mcp_resource) in [
                    ("broker", None),
                    ("incoming", Some(format!("{}/v1", broker.state.origin))),
                ] {
                    doc.add_provider(
                        name.into(),
                        ProviderConfig {
                            kind: ProviderKind::Broker(BrokerService {
                                endpoint: broker.state.origin.clone(),
                                mcp_resource,
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
                        ctx,
                    )?;
                }
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("broker").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_broker_device_profile(
                    BrokerDeviceProfileData {
                        name: "native".into(),
                        provider: "broker".into(),
                        issuer: broker.state.origin.clone(),
                        subject: "shared-user".into(),
                        tokens: BrokerDeviceTokens {
                            access_token: Secret::new(SecretString::from(
                                broker.token("shared-user"),
                            )),
                            expires_at: (SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs()
                                + 3600) as i64,
                        },
                        s3_disabled: false,
                        metadata: BrokerDeviceMetadata::default(),
                    },
                    expected,
                    1,
                    ctx,
                )?;
                doc.activate("native", doc.revision(), ctx)
            })
            .unwrap();
        assert!(publication.failure().is_none() && publication.cleanup_failure().is_none());
        // A Device profile is NativeAccess with resource=None. Incoming uses
        // its separately configured resource and real provider revision; this
        // verifier is only a selector until it actually verifies the JWT.
        let incoming_verifier = {
            let snapshot = store.load(&request).unwrap();
            let provider = snapshot.document().provider("incoming").unwrap();
            let ProviderKind::Broker(binding) = &provider.config().kind else {
                panic!("configured Incoming Broker provider")
            };
            verifier_cache()
                .get_native(binding, provider.revision())
                .unwrap()
        };
        auth.issuer()
            .set_record_storage_resolver(Arc::new(
                crate::record_storage::RecordStorageResolver::new(
                    reqwest::Client::builder()
                        .no_proxy()
                        .redirect(reqwest::redirect::Policy::none())
                        .build()
                        .unwrap(),
                    Some(verifier_cache()),
                    true,
                    policy.timeout,
                    Arc::downgrade(&auth),
                    policy.content,
                ),
            ))
            .unwrap();
        let server = TaskServer::new(
            auth,
            Arc::new(projects),
            0,
            policy,
            CancellationToken::new(),
        )
        .unwrap();
        Self {
            _directory: directory,
            paths,
            server,
            raw,
            incoming_verifier,
        }
    }
    fn reopen(&self, _broker: &Broker) -> Self {
        // Independent consumers of one installation share its single Auth owner;
        // their Task state and every captured Lance read remain independent.
        let projects = self
            .server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live storage test selection"))
            .projects
            .clone();
        let auth = Arc::clone(&self.server.0.auth);
        let raw = self.raw.clone();
        Self {
            _directory: tempfile::tempdir().unwrap(),
            paths: self.paths.clone(),
            raw,
            incoming_verifier: Arc::clone(&self.incoming_verifier),
            server: TaskServer::new(
                auth,
                projects,
                0,
                self.server.0.policy.clone(),
                CancellationToken::new(),
            )
            .unwrap(),
        }
    }
    fn subject(&self, broker: &Broker, subject: &str) {
        let cancellation = Cancellation::default();
        let request = RequestContext::new(
            self.server.0.policy.content,
            &cancellation,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(
                self.server.0.auth.installation().live.clone(),
                AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
                request.snapshot(),
            )
            .unwrap(),
            profile_limits(),
            &request,
        )
        .unwrap();
        let publication = store
            .mutate(store.load(&request).unwrap(), &request, |doc, ctx| {
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("broker").unwrap().revision(),
                    profile_revision: Some(doc.profile("native").unwrap().revision()),
                };
                doc.replace_broker_device_profile(
                    BrokerDeviceProfileData {
                        name: "native".into(),
                        provider: "broker".into(),
                        issuer: broker.state.origin.clone(),
                        subject: subject.into(),
                        tokens: BrokerDeviceTokens {
                            access_token: Secret::new(SecretString::from(broker.token(subject))),
                            expires_at: (SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs()
                                + 3600) as i64,
                        },
                        s3_disabled: false,
                        metadata: BrokerDeviceMetadata::default(),
                    },
                    expected,
                    1,
                    ctx,
                )
            })
            .unwrap();
        assert!(publication.failure().is_none() && publication.cleanup_failure().is_none());
    }
    fn assert_no_local_tables(&self) {
        for (paths, project_id, _) in self
            .server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live storage test selection"))
            .projects
            .iter()
        {
            for module in ["memory", "task"] {
                assert!(
                    !paths
                        .project_runtime(*project_id, &[module, "index.lance"])
                        .unwrap()
                        .exists(),
                    "remote records must not create an authoritative local table"
                );
            }
        }
    }
    async fn task(&self, operation: &str, input: Value, write: bool) -> Result<Value, Failure> {
        self.task_at(operation, input, write, 0).await
    }
    async fn task_at(
        &self,
        operation: &str,
        input: Value,
        write: bool,
        selected: usize,
    ) -> Result<Value, Failure> {
        self.task_at_cancel(operation, input, write, selected, CancellationToken::new())
            .await
    }
    async fn task_at_cancel(
        &self,
        operation: &str,
        input: Value,
        write: bool,
        selected: usize,
        cancellation: CancellationToken,
    ) -> Result<Value, Failure> {
        let operation = match operation {
            "session_create" => "create_session",
            "session_get" => "get_session",
            "create" => "create_task",
            "get" => "get_task",
            "claim" => "claim_task",
            "revise" => "revise_task",
            other => other,
        };
        let snapshot = self
            .server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live storage test selection"));
        let (paths, id, version) = &snapshot.projects[selected];
        self.server
            .call(
                serde_json::from_value(json!({"selection":{"project_dir":paths.root(),"project_id":id,"project_version":version.version()},"input":input})).unwrap(),
                operation,
                write,
                Credential {
                    role_binding: None,
                    bearer: zeroize::Zeroizing::new(self.raw.clone()),
                    origin: Origin::Runtime,
                },
                cancellation,
            )
            .await
    }
    fn timeout(&mut self, timeout: Duration) {
        let snapshot = self
            .server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live storage test selection"));
        let mut policy = self.server.0.policy.clone();
        policy.timeout = timeout;
        self.server = TaskServer::new(
            Arc::clone(&self.server.0.auth),
            snapshot.projects.clone(),
            0,
            policy,
            CancellationToken::new(),
        )
        .unwrap();
    }
    async fn index_memory(&self, selected: usize, refresh: bool, scope: &str) -> Value {
        // The real Source MCP operation owns index publication; the test does not
        // call Records index APIs or build an alternate index beside the corpus.
        let fixture = crate::source_server::test_fixture::fixture(
            &[],
            Brand::default(),
            graphit_hub::NativeBinding::KiroV3,
        );
        let mut policy = fixture.server.policy().clone();
        policy.installation = self.paths.clone();
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
            self.server.0.auth.installation().live.owner()
        else {
            unreachable!()
        };
        policy.registry = graphit_hub::registry::HubRegistry::global(
            &self.paths,
            installation_id,
            policy.native_install.hub.hub(),
        )
        .unwrap();
        let snapshot = self
            .server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live storage test selection"));
        let (paths, _, version) = &snapshot.projects[selected];
        graphit_core::config::ConfigStore::project(paths)
            .unwrap()
            .set(
                Module::Memory.key(),
                json!(true),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
        let mut source = crate::source_server::SourceServer::new(
            Arc::clone(&self.server.0.auth),
            Arc::clone(&snapshot.projects),
            selected,
            policy,
            CancellationToken::new(),
        )
        .unwrap();
        source.attach_native_verifiers(verifier_cache()).unwrap();
        let socket = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &Cancellation::default(),
        )
        .unwrap();
        let url = format!("http://{}/mcp", socket.address());
        let serving = source.clone();
        let served = tokio::spawn(async move {
            crate::source_server::SourceServer::serve(&serving, socket).await
        });
        let mut headers = std::collections::HashMap::new();
        let mut bearer =
            axum::http::HeaderValue::from_str(&format!("Bearer {}", self.raw)).unwrap();
        bearer.set_sensitive(true);
        headers.insert(axum::http::header::AUTHORIZATION, bearer);
        headers.insert(
            axum::http::HeaderName::from_static("x-graphit-auth-origin"),
            axum::http::HeaderValue::from_static("runtime"),
        );
        let client: rmcp::service::RunningService<RoleClient, ()> = ()
            .serve(StreamableHttpClientTransport::with_client(
                reqwest::Client::new(),
                StreamableHttpClientTransportConfig::with_uri(url).custom_headers(headers),
            ))
            .await
            .unwrap();
        let arguments = if scope == "user" {
            json!({"ai_optimized":false,"scope":scope,"refresh":refresh})
        } else {
            json!({"ai_optimized":false,"scope":scope,"refresh":refresh,"project_dir":paths.root(),"project_version":version.version()})
        };
        let result = client
            .call_tool(
                CallToolRequestParams::new("graphit_memory_index")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        client.cancel().await.unwrap();
        source.shutdown().await.unwrap();
        served.await.unwrap().unwrap();
        assert_ne!(
            result.is_error,
            Some(true),
            "actual Source Memory index consumer failed"
        );
        result
            .structured_content
            .expect("Memory maintenance result")
    }
    async fn memory(
        &self,
        operation: &str,
        input: Value,
        selected: usize,
    ) -> Result<Value, String> {
        let cancellation = Cancellation::default();
        let policy = &self.server.0.policy;
        let request = RequestContext::new(
            policy.content,
            &cancellation,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let snapshot = self
            .server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live storage test selection"));
        let (paths, id, version) = &snapshot.projects[selected];
        let selection = browser::codec::AdmittedSelection {
            project_dir: graphit_core::native_path::NativePath::new(paths.root().to_owned())
                .unwrap(),
            project_id: *id,
            project_version: version.version().to_owned(),
        };
        let bytes = serde_json::to_vec(&json!({"selection":selection,"input":input})).unwrap();
        let arguments = crate::foreground_memory::parse(&bytes, policy.content, operation)
            .map_err(str::to_owned)?;
        let query: crate::foreground_memory::Query =
            serde_json::from_value(arguments.input.unwrap()).unwrap();
        let scope = CapturedPublicationScope::project(paths, *id, policy.scope, request.snapshot())
            .unwrap();
        let binding = scope.binding().unwrap();
        let scope_input = || ScopeInput {
            installation: self.server.0.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &scope,
                    record: &binding,
                },
                version,
            }),
            providers: &[],
            binding: policy.binding,
            global_version: "",
        };
        let issuer = self.server.0.auth.issuer();
        let caller = issuer.authenticate_runtime(&self.raw, &request).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let bytes = match query {
            crate::foreground_memory::Query::Mutation(input) => {
                let admission = authority
                    .admit_authenticated_write(
                        &caller,
                        &request,
                        scope_input(),
                        ProjectWriteOperation::Memory,
                    )
                    .unwrap();
                crate::foreground_memory::mutation::project(
                    request.snapshot(),
                    &admission,
                    policy,
                    selection,
                    *input,
                )
                .await
                .map_err(|e| e.to_string())?
                .encode()
                .map_err(|e| e.to_string())?
            }
            crate::foreground_memory::Query::Search(input) => {
                let admission = authority
                    .admit_authenticated_read(
                        &caller,
                        &request,
                        scope_input(),
                        ProjectReadModule::Memory,
                    )
                    .unwrap();
                crate::foreground_memory::search::project(
                    &request, &admission, policy, &selection, input, None,
                )
                .await
                .map_err(|e| e.to_string())?
            }
            query => {
                let admission = authority
                    .admit_authenticated_read(
                        &caller,
                        &request,
                        scope_input(),
                        ProjectReadModule::Memory,
                    )
                    .unwrap();
                crate::foreground_memory::read(
                    request.snapshot(),
                    &admission,
                    policy,
                    &selection,
                    query,
                )
                .await
                .map_err(|e| e.to_string())?
            }
        };
        memory_value(&bytes)
    }
    fn branch(&mut self, name: &str) {
        self.server = browser::memory_fixture::with_named_version(&self.server, name);
    }
    async fn user(&self, operation: &str, input: Value) -> Result<Value, String> {
        self.user_storage(operation, input, true).await
    }
    async fn user_storage(
        &self,
        operation: &str,
        input: Value,
        remote: bool,
    ) -> Result<Value, String> {
        use graphit_core::artifact_store::{StoreOwner, UserIdentity};
        let cancellation = Cancellation::default();
        let policy = &self.server.0.policy;
        let request = RequestContext::new(
            policy.content,
            &cancellation,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let issuer = self.server.0.auth.issuer();
        let caller = issuer.authenticate_runtime(&self.raw, &request).unwrap();
        issuer
            .prepare_native_record_access(&caller)
            .await
            .map_err(|error| error.to_string())?;
        let (binding, revision) = caller.native_provider().map_err(|e| e.to_string())?;
        let verifier = verifier_cache()
            .get_native(&binding, revision)
            .map_err(|e| e.to_string())?;
        let caller = caller
            .authenticate_native(&verifier)
            .await
            .map_err(|e| e.to_string())?;
        let subject = issuer
            .user_from_authenticated(&caller)
            .map_err(|e| e.to_string())?;
        let StoreOwner::Installation { installation_id } =
            self.server.0.auth.installation().live.owner()
        else {
            unreachable!()
        };
        let live = CapturedPublicationScope::user(
            &self.paths,
            installation_id.clone(),
            UserIdentity::new(
                subject.issuer().unwrap().into(),
                subject.subject().unwrap().into(),
            )
            .unwrap(),
            policy.scope,
            request.snapshot(),
        )
        .unwrap();
        let record = live.binding().unwrap();
        let bytes = serde_json::to_vec(&json!({"input":input})).unwrap();
        let arguments = crate::foreground_user_memory::parse(&bytes, policy.content, operation)
            .map_err(str::to_owned)?;
        let query: crate::foreground_memory::Query =
            serde_json::from_value(arguments.input.unwrap()).unwrap();
        let bytes = match query {
            crate::foreground_memory::Query::Mutation(input) => {
                let write = issuer
                    .admit_authenticated_user_memory_write(
                        &caller,
                        &request,
                        ScopeCapture {
                            live: &live,
                            record: &record,
                        },
                        graphit_auth::UserMemoryWriteOperation::Memory,
                    )
                    .map_err(|e| e.to_string())?;
                crate::foreground_memory::mutation::user(request.snapshot(), &write, policy, *input)
                    .await
                    .map_err(|e| e.to_string())?
                    .encode()
                    .map_err(|e| e.to_string())?
            }
            query => {
                let read = issuer
                    .admit_authenticated_user_memory_read(
                        &caller,
                        &request,
                        ScopeCapture {
                            live: &live,
                            record: &record,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                match query {
                    crate::foreground_memory::Query::Search(input) => {
                        crate::foreground_memory::search::user(&request, &read, policy, input, None)
                            .await
                            .map_err(|e| e.to_string())?
                    }
                    query => crate::foreground_user_memory::read(
                        request.snapshot(),
                        &read,
                        policy,
                        query,
                    )
                    .await
                    .map_err(|e| e.to_string())?,
                }
            }
        };
        assert_eq!(
            live.runtime_root().join("memory/index.lance").exists(),
            !remote,
            "USER Memory must use its admitted local or remote authoritative table"
        );
        memory_value(&bytes)
    }
}
fn memory_input(key: &str) -> Value {
    json!({"idempotency_key":key,"title":"S3 shared memory","body":"Original shared wording",
        "attributes":{"classification":"fact","mandatory":false,"important":false},"tags":["s3"]})
}
fn memory_value(bytes: &[u8]) -> Result<Value, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value["native"]
        .get("completion_error")
        .is_some_and(|error| !error.is_null())
        || value["native"]
            .get("delivery_error")
            .is_some_and(|error| !error.is_null())
    {
        return Err(format!(
            "Memory consumer receipt: {}, completion={}, delivery={}",
            value["native"]["state"],
            value["native"]["completion_error"],
            value["native"]["delivery_error"]
        ));
    }
    Ok(value)
}
fn task_value(result: Result<Value, Failure>) -> Value {
    result.unwrap_or_else(|e| panic!("Task consumer failed: {}", e.code))
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn independent_consumers_share_s3_revisions_sessions_history_and_versions() {
    let broker = Broker::start(S3Fixture::load()).await;
    let mut a = Client::new(&broker, None);
    let mut b = Client::new(&broker, Some(&a));
    let created = a
        .memory("memory_create", memory_input("shared-create"), 0)
        .await
        .unwrap();
    let id = created["native"]["id"].clone();
    assert!(id.is_string());
    let read = b.memory("memory_get", json!({"id":id}), 0).await.unwrap();
    assert_eq!(read["native"]["memory"]["revision"], 1);
    let updated = b.memory("memory_update",json!({"id":id,"expected_revision":1,
        "idempotency_key":"shared-update","reason":"shared revision","body":"New remotely visible text"}),0).await.unwrap();
    assert_eq!(updated["native"]["revision"], 2);
    let read = a.memory("memory_get", json!({"id":id}), 0).await.unwrap();
    assert_eq!(
        read["native"]["memory"]["body"],
        "New remotely visible text"
    );
    let history = a
        .memory("memory_history", json!({"id":id}), 0)
        .await
        .unwrap();
    assert_eq!(history["native"]["revisions"].as_array().unwrap().len(), 1);
    assert!(b.memory("memory_get", json!({"id":id}), 1).await.unwrap()["native"].is_null());
    assert!(matches!(
        a.index_memory(0, false, "project").await["lexical"]["state"].as_str(),
        Some("published" | "ready")
    ));
    let found = b
        .memory(
            "memory_search",
            json!({"text":"remotely","mode":"fts","top_k":8}),
            0,
        )
        .await
        .unwrap();
    assert_eq!(found["native"]["state"], "published");
    assert_eq!(found["native"]["hits"][0]["current"]["id"], id);
    assert_eq!(
        b.index_memory(0, true, "project").await["lexical"]["state"],
        "published"
    );
    assert_eq!(
        a.memory(
            "memory_search",
            json!({"text":"remotely","mode":"fts","top_k":8}),
            0
        )
        .await
        .unwrap()["native"]["hits"][0]["current"]["revision"],
        2
    );

    let session = task_value(
        a.task(
            "session_create",
            json!({"idempotency_key":"remote-session",
        "title":"Shared session","description":"Shared S3 records","strategy":"native consumers"}),
            true,
        )
        .await,
    );
    let session_id = session["result"]["id"].clone();
    let task = task_value(a.task("create",json!({"idempotency_key":"remote-task","session_id":session_id,
        "title":"Shared Task","description":"Authoritative S3 Task","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Persist the authorized task"},{"kind":"test","text":"Read the shared remote task"}]}),true).await);
    let task_id = task["result"]["specification"]["id"].clone();
    let read = task_value(b.task("get", json!({"id":task_id}), false).await);
    assert_eq!(
        read["result"]["task"]["specification"]["title"],
        "Shared Task"
    );
    assert!(
        task_value(b.task_at("get", json!({"id":task_id}), false, 1).await)["result"].is_null()
    );
    let claimed = task_value(
        b.task("claim", json!({"id":task_id,"lease_ms":120000}), true)
            .await,
    );
    let token = claimed["result"]["token"].clone();
    let revision = claimed["result"]["record"]["specification"]["revision"].clone();
    let changed = task_value(
        b.task(
            "revise",
            json!({"target":{"id":task_id,
        "claim":{"token":token,"expected_revision":revision}},"lease_ms":120000,
        "reason":"independent client","title":"Shared revised Task"}),
            true,
        )
        .await,
    );
    let read = task_value(a.task("get", json!({"id":task_id}), false).await);
    assert_eq!(
        read["result"]["task"]["specification"]["title"],
        "Shared revised Task"
    );
    assert!(read["result"].get("events").is_none());
    assert!(read["result"]["history"].is_null());
    assert!(read["result"]["comments"].is_null());
    let mut cursor = read["result"]["history_start_cursor"].clone();
    for revision in 2..=3 {
        let page = task_value(
            a.task(
                "get",
                json!({"id":task_id,"history":{"page_size":1,"cursor":cursor}}),
                false,
            )
            .await,
        );
        assert_eq!(page["result"]["generation"], read["result"]["generation"]);
        let history = &page["result"]["history"];
        assert_eq!(history["before"]["specification"]["revision"], revision - 1);
        assert_eq!(history["events"].as_array().unwrap().len(), 1);
        assert_eq!(
            history["events"][0]["record"]["specification"]["revision"],
            revision
        );
        cursor = history["next_cursor"].clone();
        assert_eq!(cursor.is_null(), revision == 3);
    }
    let session_read = task_value(b.task("session_get", json!({"id":session_id}), false).await);
    assert_eq!(
        session_read["result"]["session"]["input"]["title"],
        "Shared session"
    );
    assert!(session_read["result"].get("events").is_none());
    assert!(session_read["result"]["history"].is_null());
    let empty_history = task_value(
        b.task(
            "session_get",
            json!({"id":session_id,"history":{"page_size":1,
            "cursor":session_read["result"]["history_start_cursor"]}}),
            false,
        )
        .await,
    );
    assert_eq!(
        empty_history["result"]["generation"],
        session_read["result"]["generation"]
    );
    assert!(
        empty_history["result"]["history"]["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(empty_history["result"]["history"]["next_cursor"].is_null());
    let found = task_value(
        a.task(
            "search",
            json!({"text":"revised","filter":{},"page_size":8,"top_k":8}),
            false,
        )
        .await,
    );
    assert!(found["result"].get("Published").is_some());
    assert_eq!(
        found["result"]["Published"]["results"][0]["task"]["id"],
        task_id
    );

    a.branch("feature/s3");
    b.branch("feature/s3");
    let branch_created = a
        .memory("memory_create", memory_input("branch-only"), 2)
        .await
        .unwrap();
    let branch_id = branch_created["native"]["id"].clone();
    assert_eq!(
        b.memory("memory_get", json!({"id":branch_id}), 2)
            .await
            .unwrap()["native"]["memory"]["revision"],
        1
    );
    assert!(
        b.memory("memory_get", json!({"id":branch_id}), 0)
            .await
            .unwrap()["native"]
            .is_null()
    );
    let branch_session = task_value(
        a.task_at(
            "session_create",
            json!({"idempotency_key":"satellite-session",
        "title":"Satellite session","description":"Isolated branch","strategy":"S3 version"}),
            true,
            2,
        )
        .await,
    );
    let branch_task = task_value(a.task_at("create",json!({"idempotency_key":"satellite-task",
        "session_id":branch_session["result"]["id"],"title":"Satellite Task","description":"Only on feature branch",
        "task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Persist the authorized task"},{"kind":"test","text":"Read the shared remote task"}]}),true,2).await);
    let branch_task_id = branch_task["result"]["specification"]["id"].clone();
    assert_eq!(
        task_value(
            b.task_at("get", json!({"id":branch_task_id}), false, 2)
                .await
        )["result"]["task"]["specification"]["title"],
        "Satellite Task"
    );
    assert!(
        task_value(
            b.task_at("get", json!({"id":branch_task_id}), false, 0)
                .await
        )["result"]
            .is_null()
    );
    assert_eq!(
        task_value(
            b.task_at(
                "search",
                json!({"text":"satellite","filter":{},"page_size":8,"top_k":8}),
                false,
                2
            )
            .await
        )["result"]["Published"]["results"][0]["task"]["id"],
        branch_task_id
    );
    assert_eq!(
        task_value(
            b.task_at(
                "search",
                json!({"text":"satellite","filter":{},"page_size":8,"top_k":8}),
                false,
                0
            )
            .await
        )["result"]["Published"]["total_hits"],
        0
    );
    // Main data remain visible after a fresh authority/client, not a cached snapshot.
    let restarted = Client::new(&broker, Some(&a));
    assert_eq!(
        restarted
            .memory("memory_get", json!({"id":id}), 0)
            .await
            .unwrap()["native"]["memory"]["revision"],
        2
    );
    assert_eq!(
        task_value(restarted.task("get", json!({"id":task_id}), false).await)["result"]["task"]["specification"]
            ["revision"],
        changed["result"]["specification"]["revision"]
    );
    let calls = broker.state.calls.load(Ordering::SeqCst);
    a.memory("memory_get", json!({"id":id}), 0).await.unwrap();
    task_value(a.task("get", json!({"id":task_id}), false).await);
    assert_eq!(
        broker.state.calls.load(Ordering::SeqCst),
        calls,
        "valid STS sessions must be reused"
    );
    assert!(broker.state.requests.lock().unwrap().iter().all(
        |v| v["scope"] == "project" && matches!(v["module"].as_str(), Some("memory" | "task"))
    ));
    for client in [&a, &b, &restarted] {
        client.assert_no_local_tables();
        client.server.shutdown().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn s3_consumers_conflict_renew_credentials_and_never_fallback_on_denial() {
    let broker = Broker::start(S3Fixture::load()).await;
    broker.state.ttl.store(2, Ordering::SeqCst);
    let a = Client::new(&broker, None);
    let b = Client::new(&broker, Some(&a));
    let created = a
        .memory("memory_create", memory_input("concurrent-create"), 0)
        .await
        .unwrap();
    let id = created["native"]["id"].clone();
    let update = |key| json!({"id":id,"expected_revision":1,"idempotency_key":key,"reason":"concurrent CAS","body":key});
    let (first, second) = tokio::join!(
        a.memory("memory_update", update("winner-a"), 0),
        b.memory("memory_update", update("winner-b"), 0)
    );
    assert_ne!(
        first.is_ok(),
        second.is_ok(),
        "one CAS update commits and the other conflicts"
    );
    let latest = a.memory("memory_get", json!({"id":id}), 0).await.unwrap();
    assert_eq!(latest["native"]["memory"]["revision"], 2);
    let before = broker.state.calls.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(2100)).await;
    a.memory("memory_get", json!({"id":id}), 0).await.unwrap();
    assert!(
        broker.state.calls.load(Ordering::SeqCst) > before,
        "expired STS session renews"
    );
    *broker.state.status.lock().unwrap() = StatusCode::FORBIDDEN;
    let denied = Client::new(&broker, Some(&a));
    assert!(
        denied
            .memory("memory_create", memory_input("denied-memory"), 0)
            .await
            .is_err()
    );
    assert!(
        denied
            .task(
                "session_create",
                json!({"idempotency_key":"denied-task","title":"denied",
        "description":"must not write locally","strategy":"none"}),
                true
            )
            .await
            .is_err()
    );
    for client in [&a, &b, &denied] {
        client.assert_no_local_tables();
        client.server.shutdown().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn user_memory_s3_uses_verified_identity_and_installation_without_project_leakage() {
    let broker = Broker::start(S3Fixture::load()).await;
    let a = Client::new(&broker, None);
    let b = a.reopen(&broker);
    let created = a
        .user("user_memory_create", memory_input("user-create"))
        .await
        .unwrap();
    let id = created["native"]["id"].clone();
    assert!(id.is_string());
    assert_eq!(
        b.user("user_memory_get", json!({"id":id})).await.unwrap()["native"]["memory"]["revision"],
        1
    );
    b.user(
        "user_memory_update",
        json!({"id":id,"expected_revision":1,
        "idempotency_key":"user-update","reason":"another client","body":"Shared user revision"}),
    )
    .await
    .unwrap();
    assert_eq!(
        a.user("user_memory_get", json!({"id":id})).await.unwrap()["native"]["memory"]["revision"],
        2
    );
    assert_eq!(
        a.user("user_memory_history", json!({"id":id}))
            .await
            .unwrap()["native"]["revisions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(
        a.index_memory(0, false, "user").await["lexical"]["state"].as_str(),
        Some("published" | "ready")
    ));
    assert_eq!(
        b.user(
            "user_memory_search",
            json!({"text":"Shared","mode":"fts","top_k":8})
        )
        .await
        .unwrap()["native"]["hits"][0]["current"]["id"],
        id
    );
    assert!(a.memory("memory_get", json!({"id":id}), 0).await.unwrap()["native"].is_null());
    let other_installation = Client::new(&broker, Some(&a));
    assert!(
        other_installation
            .user("user_memory_get", json!({"id":id}))
            .await
            .unwrap()["native"]
            .is_null()
    );
    a.subject(&broker, "other-user");
    assert!(
        b.user("user_memory_get", json!({"id":id})).await.unwrap()["native"].is_null(),
        "valid STS cache for the former identity must not authorize another subject"
    );
    a.subject(&broker, "shared-user");
    assert_eq!(
        b.user("user_memory_get", json!({"id":id})).await.unwrap()["native"]["memory"]["revision"],
        2
    );
    assert!(
        broker
            .state
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["scope"] == "user"
                && v["module"] == "memory"
                && v["installation_id"].is_string())
    );
    for client in [&a, &b, &other_installation] {
        client.assert_no_local_tables();
        client.server.shutdown().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn s3_access_failure_after_valid_sts_is_explicit_and_keeps_local_tables_absent() {
    let broker = Broker::start(S3Fixture::load()).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let router = Router::new().fallback(move || {
        let observed = Arc::clone(&observed);
        async move {
            observed.fetch_add(1,Ordering::SeqCst);
            (StatusCode::FORBIDDEN,[(axum::http::header::CONTENT_TYPE,"application/xml")],
                "<Error><Code>InvalidAccessKeyId</Code><Message>Fixture temporary credentials rejected</Message></Error>")
        }
    });
    let serving = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    *broker.state.s3_endpoint.lock().unwrap() = endpoint;
    let client = Client::new(&broker, None);
    assert!(
        client
            .memory("memory_create", memory_input("s3-rejected-memory"), 0)
            .await
            .is_err()
    );
    assert!(
        client
            .task(
                "session_create",
                json!({"idempotency_key":"s3-rejected-task","title":"rejected",
        "description":"S3 rejects a valid Broker grant","strategy":"no local fallback"}),
                true
            )
            .await
            .is_err()
    );
    assert!(
        calls.load(Ordering::SeqCst) >= 2,
        "both consumers must attempt authoritative S3 IO"
    );
    assert!(
        broker.state.calls.load(Ordering::SeqCst) >= 2,
        "both physical modules obtained STS"
    );
    client.assert_no_local_tables();
    client.server.shutdown().await.unwrap();
    serving.abort();
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn record_consumers_reject_wrong_sts_scope_and_preserve_cancellation_and_deadlines() {
    let broker = Broker::start(S3Fixture::load()).await;
    broker.state.invalid_response.store(true, Ordering::SeqCst);
    let a = Client::new(&broker, None);
    assert!(
        a.memory("memory_create", memory_input("malformed-sts"), 0)
            .await
            .is_err()
    );
    assert!(
        a.task(
            "session_create",
            json!({"idempotency_key":"malformed-sts-task","title":"invalid",
        "description":"reject mismatched physical module","strategy":"no fallback"}),
            true
        )
        .await
        .is_err()
    );
    a.assert_no_local_tables();
    a.server.shutdown().await.unwrap();
    broker.state.invalid_response.store(false, Ordering::SeqCst);
    broker.state.hold.store(true, Ordering::SeqCst);
    let cancelled = Client::new(&broker, None);
    let cancellation = CancellationToken::new();
    let pending = cancelled.task_at_cancel("session_create",json!({"idempotency_key":"cancel-sts",
        "title":"cancel","description":"cancel held credentials","strategy":"original cancellation"}),true,0,cancellation.clone());
    tokio::pin!(pending);
    tokio::select! {
        _ = broker.state.entered.notified()=>{},
        _ = &mut pending=>panic!("held STS must remain pending before cancellation"),
    }
    cancellation.cancel();
    let failure = pending.await.expect_err("cancelled consumer must fail");
    assert!(matches!(
        failure.code,
        "cancelled" | "request_cancelled_or_expired"
    ));
    broker.state.released.notify_waiters();
    tokio::time::timeout(Duration::from_secs(5), cancelled.server.shutdown())
        .await
        .unwrap()
        .unwrap();
    cancelled.assert_no_local_tables();
    let mut deadline = Client::new(&broker, None);
    deadline.timeout(Duration::from_millis(300));
    let result = deadline.task("session_create",json!({"idempotency_key":"deadline-sts",
        "title":"deadline","description":"deadline while obtaining credentials","strategy":"original request"}),true).await;
    let failure = result.expect_err("expired original consumer must fail");
    assert!(matches!(
        failure.code,
        "deadline" | "request_cancelled_or_expired"
    ));
    broker.state.released.notify_waiters();
    tokio::time::timeout(Duration::from_secs(5), deadline.server.shutdown())
        .await
        .unwrap()
        .unwrap();
    deadline.assert_no_local_tables();
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn unavailable_s3_does_not_fallback_or_keep_task_job_alive_after_deadline() {
    let broker = Broker::start(S3Fixture::load()).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    *broker.state.s3_endpoint.lock().unwrap() =
        format!("http://{}", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let router = Router::new().fallback(move || {let observed=observed.clone();async move {
        observed.fetch_add(1,Ordering::SeqCst);
        (StatusCode::SERVICE_UNAVAILABLE,[(axum::http::header::CONTENT_TYPE,"application/xml")],
            "<Error><Code>ServiceUnavailable</Code><Message>Fixture S3 unavailable</Message></Error>")
    }});
    let serving = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut client = Client::new(&broker, None);
    let request_timeout = Duration::from_secs(2);
    client.timeout(request_timeout);
    let result = client.task("session_create",json!({"idempotency_key":"unavailable-s3",
        "title":"unavailable","description":"unavailable authoritative storage","strategy":"no local fallback"}),true).await;
    assert!(
        result.is_err(),
        "unavailable authoritative S3 must fail explicitly"
    );
    assert!(
        calls.load(Ordering::SeqCst) > 0,
        "the selected S3 endpoint was actually reached"
    );
    client.assert_no_local_tables();
    tokio::time::timeout(
        request_timeout + Duration::from_secs(1),
        client.server.shutdown(),
    )
    .await
    .expect("original cancellation must end the owned S3 operation")
    .unwrap();
    serving.abort();
}
