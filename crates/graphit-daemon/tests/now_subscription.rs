#![cfg(target_os = "linux")]

#[expect(
    dead_code,
    reason = "Shared Broker PostgreSQL test support also exposes control-specific helpers unused by this native subscriber journey"
)]
#[path = "../../../../graphit-broker/crates/broker-control/tests/support/mod.rs"]
mod pg;

use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use broker_control::{
    BrokerAccessVerifier, ConnectionLimits, InstanceInventory, InstanceProjectVersions,
    PostgresConfig, PostgresControlStore, PostgresCredentials, PostgresEndpoint,
};
use broker_now_wire::NowSnapshotData;
use broker_presence::{
    BrokerOwnReadHttpConfig, BrokerPublishHttpConfig, LocalRealtimeHttpConfig, RealtimeHttpConfig,
    RealtimeServerRuntime, broker_instance_inventory_router, broker_realtime_router,
    local_realtime_router,
};
use broker_security::TokenKind;
use graphit_auth::{
    AdminSessionPolicy, CredentialSource, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits,
    Permission, RemoteServiceBindingInput, ServiceAdmissionLimits, ServiceAuthorityInput,
    ServiceOrigin, ServicePurpose, ServiceRuleInput, SourceAuthorityPolicy, TtlPolicy,
    broker::{JsonLimits, VerifierLimits},
    profile::{
        AiService, BrokerService, LocalCredentials, OidcTokens, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::{
    artifact_store::{CapturedPublicationScope, ScopeLimits},
    auth_document::{AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::BootstrapLimits,
    installation_identity::InstallationIdentityStore,
    project_paths::InstallationPaths,
    runtime_key::RuntimeKeyLimits,
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    broker_verifiers::BrokerVerifierCache,
    now_subscription::{
        NowRecoveryCause, NowRemoteCode, NowSubscriptionConfig, NowSubscriptionError,
        start_now_subscription,
    },
};
use graphit_http::{
    client::{BoundedClient, ClientPolicy, Limits, NetworkRequest, NetworkRequestContext},
    websocket::{
        BoundedWebSocket, CloseCode, Message, WebSocketClient, WebSocketLimits, WebSocketRead,
    },
};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use http::{HeaderMap, HeaderValue, Method, header};
use pg::n;
use secrecy::SecretString;
use serde_json::json;
use sqlx_core::{connection::Connection, executor::Executor};
use sqlx_postgres::PgConnection;
use std::{
    num::NonZeroU16,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;
use zeroize::Zeroizing;

const PROJECT: &str = "01D39ZY06FGSCTVN4T2V9PKHFZ";
const OTHER: &str = "01D39ZY06FGSCTVN4T2V9PKHG0";
const LOCAL_RAW: &str = "gb_at_runtime-now-fixture";

fn seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn request(signal: &Cancellation, duration: Duration) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        signal,
        Instant::now() + duration,
    )
    .unwrap()
}
fn network() -> Limits {
    Limits {
        request_body_bytes: 65536,
        response_body_bytes: 65536,
        url_bytes: 2048,
        header_bytes: 16384,
        header_count: 32,
        max_in_flight: 4,
        max_idle_per_host: 2,
        idle_timeout: Duration::from_secs(5),
        max_certificates: 2,
        max_certificate_bytes: 8192,
    }
}
fn websocket(origin: &str) -> WebSocketClient {
    WebSocketClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
        WebSocketLimits {
            frame_bytes: n(65536),
            message_bytes: n(65536),
            send_bytes: n(65536),
        },
    )
    .unwrap()
}
fn cache() -> BrokerVerifierCache {
    BrokerVerifierCache::new(
        network(),
        VerifierLimits {
            token_bytes: 8192,
            json: JsonLimits {
                bytes: 8192,
                nodes: 256,
                depth: 16,
                scalar_bytes: 2048,
                collection_entries: 32,
            },
            max_keys: 8,
            metadata_ttl: Duration::from_secs(300),
            key_ttl: Duration::from_secs(300),
            unknown_key_cooldown: Duration::from_millis(100),
        },
        true,
        None,
        &[],
    )
}
fn config(project: &str) -> NowSubscriptionConfig<'_> {
    NowSubscriptionConfig {
        project_id: project.parse().unwrap(),
        instance_id: None,
        version: None,
        command_bytes: n(65536),
        snapshot_bytes: n(65536),
    }
}

enum Credential {
    Native(String),
    Local,
}
struct AuthFixture {
    auth: DaemonAuth,
    store: AuthStore,
    _directory: tempfile::TempDir,
}
impl AuthFixture {
    fn new(endpoint: &str, credential: Credential, original: &RequestContext<'_>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let native = matches!(&credential, Credential::Native(_));
        let scope = ScopeLimits::new(16, 4096, 128).unwrap();
        let document = AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap();
        let profiles = ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 4,
            profiles: 4,
        };
        let rules = [ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::Broker {
                provider: "now",
                purpose: ServicePurpose::PresenceRead,
                endpoint,
                resource: None,
            },
            credential: if native {
                CredentialSource::NativeAccess
            } else {
                CredentialSource::LocalBrokerKey
            },
            incoming: None,
        }];
        let permissions = [Permission::External(ExternalEffect::RemoteRequest)];
        let source = || SourceAuthorityPolicy {
            ordinary: &permissions,
            administrative: &[],
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = StartupPolicy {
            bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
            runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
            scope,
            document,
            profiles,
            authority: IssuerAuthorityPolicy::new(
                source(),
                source(),
                IssuerLimits {
                    max_permissions: 64,
                    max_scope_bytes: 65536,
                    max_parent_bearer_bytes: 1024,
                },
                ServiceAuthorityInput {
                    rules: &rules,
                    limits: ServiceAdmissionLimits {
                        max_rules: 4,
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
        };
        let auth = DaemonAuth::start(&paths, &[], policy, original).unwrap();
        let id = InstallationIdentityStore::new(&paths)
            .unwrap()
            .load()
            .unwrap()
            .unit_id()
            .unwrap()
            .clone();
        let captured = CapturedPublicationScope::installation(
            &paths,
            id,
            auth.installation()
                .live
                .allowed_roots()
                .map(ToOwned::to_owned),
            scope,
            original.snapshot(),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(captured, document, original.snapshot()).unwrap(),
            profiles,
            original,
        )
        .unwrap();
        let before = store.load(original).unwrap();
        let result = store
            .mutate(before, original, |doc, ctx| {
                let service = BrokerService {
                    endpoint: endpoint.into(),
                    mcp_resource: None,
                    allow_anonymous: false,
                };
                doc.add_provider(
                    "now".into(),
                    ProviderConfig {
                        kind: if native {
                            ProviderKind::Broker(service)
                        } else {
                            ProviderKind::Local {
                                allow_aws_credential_chain: false,
                                broker: Some(service),
                            }
                        },
                        embedding: AiService::Broker,
                        rerank: AiService::Broker,
                        s3: S3Location {
                            bucket: None,
                            region: None,
                            endpoint: None,
                            prefix: None,
                            source: if native {
                                S3Source::BrokerScope
                            } else {
                                S3Source::LocalCredentials
                            },
                        },
                    },
                    doc.revision(),
                    1,
                    ctx,
                )?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("now").unwrap().revision(),
                    profile_revision: None,
                };
                let credentials = match &credential {
                    Credential::Native(raw) => ProfileCredentials::Broker {
                        issuer: endpoint.into(),
                        subject: "fixture-sub".into(),
                        s3_disabled: true,
                        tokens: OidcTokens {
                            access_token: Secret::new(SecretString::from(raw.clone())),
                            refresh_token: None,
                            id_token: Secret::new(SecretString::from("historical-fixture")),
                            expires_at: (seconds() + 3600).try_into().unwrap(),
                        },
                    },
                    Credential::Local => ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new(SecretString::from("runtime-fixture"))),
                        broker_key: Some(Secret::new(SecretString::from(LOCAL_RAW))),
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: None,
                    }),
                };
                doc.replace_profile_data(
                    ProfileData {
                        name: "now".into(),
                        provider: "now".into(),
                        username: "fixture".into(),
                        organization: None,
                        teams: vec![],
                        credentials,
                    },
                    expected,
                    1,
                    ctx,
                )?;
                doc.activate("now", doc.revision(), ctx)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
        assert!(result.failure().is_none());
        Self {
            auth,
            store,
            _directory: directory,
        }
    }
    fn runtime_key(&self, original: &RequestContext<'_>) -> Zeroizing<String> {
        let key = self.auth.runtime().check(original.snapshot()).unwrap();
        Zeroizing::new(
            String::from_utf8(key.secret(original.snapshot()).unwrap().to_vec()).unwrap(),
        )
    }
}

struct Server {
    origin: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl Server {
    fn start(listener: TcpListener, app: axum::Router) -> Self {
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (stop, rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        Self {
            origin,
            stop: Some(stop),
            task: Some(task),
        }
    }
    async fn shutdown(mut self) {
        let _ = self.stop.take().unwrap().send(());
        tokio::time::timeout(Duration::from_secs(3), self.task.as_mut().unwrap())
            .await
            .unwrap()
            .unwrap();
        self.task.take();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn pg_config(f: &pg::Fixture) -> PostgresConfig {
    PostgresConfig::new(
        PostgresEndpoint {
            host: "127.0.0.1".into(),
            port: NonZeroU16::new(f.port).unwrap(),
            database: "postgres".into(),
        },
        PostgresCredentials::new(
            "runtime_now_owner".into(),
            Zeroizing::new("ephemeral-owner".into()),
        ),
        f.ca.clone(),
        ConnectionLimits {
            text_bytes: n(256),
            ca_bytes: n(8192),
        },
        pg::pool(2),
        pg::limits(),
    )
    .unwrap()
}
fn realtime() -> RealtimeHttpConfig {
    RealtimeHttpConfig {
        required_scopes: vec!["presence-sdk".into()],
        grants_json_bytes: n(65536),
        inventory_json_bytes: n(65536),
        command_bytes: n(65536),
        outbound_bytes: n(65536),
        operation_timeout: Duration::from_secs(2),
    }
}
async fn acl(connection: &mut PgConnection, caps: &[&str]) {
    connection.execute("BEGIN; DELETE FROM public.resource_grants; UPDATE public.resource_acl_state SET revision=revision+1,updated_at=0 WHERE id=1").await.unwrap();
    sqlx_core::query::query("INSERT INTO public.resource_grants(id,name,access_kind,principal,s3_route,created_at,updated_at) VALUES('runtime-now','fixture','authenticated','','',0,0)").execute(&mut *connection).await.unwrap();
    for cap in caps {
        sqlx_core::query::query(
            "INSERT INTO public.grant_capabilities(grant_id,value) VALUES('runtime-now',$1)",
        )
        .bind(*cap)
        .execute(&mut *connection)
        .await
        .unwrap();
    }
    for project in [PROJECT, OTHER] {
        sqlx_core::query::query(
            "INSERT INTO public.grant_projects(grant_id,value) VALUES('runtime-now',$1)",
        )
        .bind(project)
        .execute(&mut *connection)
        .await
        .unwrap();
    }
    connection.execute("COMMIT").await.unwrap();
}
fn data(project: &str) -> NowSnapshotData {
    let section =
        json!({"availability":"available","total":0,"items":[],"has_more":false,"error":null});
    let wire = json!({"project_id":project,"generated_at":"9007199254740993001","sections":{"tasks":section,"sessions":section,"memories":section,"knowledge":section}});
    let bytes = serde_json::to_vec(&wire).unwrap();
    NowSnapshotData::decode(&bytes, n(bytes.len())).unwrap()
}
fn auth_headers(raw: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let raw = Zeroizing::new(format!("Bearer {raw}"));
    let mut value = HeaderValue::from_str(&raw).unwrap();
    value.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, value);
    headers
}
async fn publisher<'a>(
    client: &WebSocketClient,
    origin: &str,
    raw: &str,
    id: &str,
    version: &str,
    original: &'a RequestContext<'a>,
) -> BoundedWebSocket<'a> {
    let mut socket = client
        .connect(
            &format!("{origin}/v1/realtime"),
            auth_headers(raw),
            NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
        )
        .await
        .unwrap();
    socket.send(Message::Text(json!({"op":"publish","instance_id":id,"project_id":PROJECT,"version":version,"snapshot":data(PROJECT)}).to_string())).await.unwrap();
    let WebSocketRead::Message(Message::Text(text)) = socket.receive().await.unwrap() else {
        panic!("publisher did not receive a Text welcome");
    };
    let welcome: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        welcome["type"], "welcome",
        "publisher's actual protocol envelope"
    );
    socket
}

async fn renew(socket: &mut BoundedWebSocket<'_>, id: &str, version: &str) {
    socket.send(Message::Text(json!({"op":"publish","instance_id":id,"project_id":PROJECT,"version":version,"snapshot":data(PROJECT)}).to_string())).await.unwrap();
}

#[tokio::test]
#[ignore = "real Runtime AuthStore/Native JWT/Local keys, Broker TCP WS and PostgreSQL TLS NOSUPERUSER"]
async fn now_subscription_broker_postgres() {
    let fixture = pg::Fixture::new(true).await;
    let mut connection = fixture.connection().await;
    connection.execute("CREATE ROLE runtime_now_owner LOGIN NOSUPERUSER PASSWORD 'ephemeral-owner'; DROP SCHEMA public CASCADE; CREATE SCHEMA public AUTHORIZATION runtime_now_owner; GRANT CONNECT,TEMP ON DATABASE postgres TO runtime_now_owner").await.unwrap();
    let store = Arc::new(
        PostgresControlStore::open(pg_config(&fixture), pg::digester(), pg::deadline())
            .await
            .unwrap(),
    );
    assert!(
        !sqlx_core::query_scalar::query_scalar::<_, bool>(
            "SELECT rolsuper FROM pg_roles WHERE rolname='runtime_now_owner'"
        )
        .fetch_one(&mut connection)
        .await
        .unwrap()
    );
    acl(
        &mut connection,
        &[
            "presence:read",
            "presence:publish",
            "task:read",
            "memory:read",
            "knowledge:read",
        ],
    )
    .await;
    let key = Arc::new(
        Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let keys = json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),"kid":"actual","alg":"EdDSA","use":"sig","key_ops":["verify"]}]});
    let jwks = serde_json::from_value(keys.clone()).unwrap();
    let verifier =
        Arc::new(BrokerAccessVerifier::new(&origin, "broker-api", None, &jwks, n(8192)).unwrap());
    let claims = json!({"iss":origin,"sub":"fixture-sub","aud":["broker-api"],"client_id":"graphit-cli","jti":"runtime-now","token_use":"access","scope":"presence-sdk","iat":seconds()-1,"exp":seconds()+3600});
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let raw = Zeroizing::new(format!(
        "{message}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(message.as_bytes()).as_ref())
    ));
    let metadata = json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,"client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":[]}});
    let discovery = json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]});
    let runtime = RealtimeServerRuntime::new(Duration::from_secs(60), n(8)).unwrap();
    let routes = broker_realtime_router(store.clone(), verifier.clone(), realtime(), &runtime)
        .unwrap()
        .merge(
            broker_instance_inventory_router(
                store.clone(),
                verifier,
                BrokerPublishHttpConfig {
                    required_scopes: vec!["presence-sdk".into()],
                    grants_json_bytes: n(65536),
                    body_bytes: n(65536),
                    request_timeout: Duration::from_secs(2),
                },
                BrokerOwnReadHttpConfig {
                    required_scopes: vec!["presence-sdk".into()],
                    grants_json_bytes: n(65536),
                    inventory_json_bytes: n(65536),
                    request_timeout: Duration::from_secs(2),
                },
            )
            .unwrap(),
        )
        .route(
            "/.well-known/graphit-broker",
            axum::routing::get(move || async move { axum::Json(metadata) }),
        )
        .route(
            "/.well-known/openid-configuration",
            axum::routing::get(move || async move { axum::Json(discovery) }),
        )
        .route(
            "/keys",
            axum::routing::get(move || async move { axum::Json(keys) }),
        );
    let native = Server::start(listener, routes);
    let signal = Cancellation::default();
    let original = request(&signal, Duration::from_secs(40));
    let transport = websocket(&origin);
    let http = BoundedClient::new(
        ClientPolicy {
            origin: &origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap();
    for id in ["native-a", "native-b"] {
        let body = json!({"projects":[{"project_id":PROJECT,"versions":["main","branch/ação"]}]})
            .to_string();
        let mut headers = auth_headers(&raw);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let response = http
            .execute(
                NetworkRequest {
                    method: Method::PUT,
                    url: &format!("{origin}/v1/instances/{id}/inventory"),
                    headers: &headers,
                    body: body.as_bytes(),
                },
                NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::CREATED);
    }
    let mut p1 = publisher(&transport, &origin, &raw, "native-a", "main", &original).await;
    let mut p2 = publisher(
        &transport,
        &origin,
        &raw,
        "native-b",
        "branch/ação",
        &original,
    )
    .await;
    let auth = AuthFixture::new(&origin, Credential::Native(raw.to_string()), &original);
    let runtime_key = auth.runtime_key(&original);
    let caller = auth
        .auth
        .issuer()
        .authenticate_runtime(&runtime_key, &original)
        .unwrap();
    let mut selection = auth
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let cache = cache();
    let verifier = cache.get(&mut selection).unwrap();
    let lease = selection.native_credential(&verifier).await.unwrap();
    let foreign_original = request(&signal, Duration::from_secs(40));
    assert!(matches!(
        start_now_subscription(&transport, &lease, &foreign_original, config(PROJECT)).await,
        Err(NowSubscriptionError::DifferentOriginal)
    ));
    let mut invalid = config(PROJECT);
    invalid.instance_id = Some("");
    assert!(matches!(
        start_now_subscription(&transport, &lease, &original, invalid).await,
        Err(NowSubscriptionError::InvalidInput)
    ));
    let mut capped_config = config(PROJECT);
    capped_config.snapshot_bytes = n(128);
    let mut capped = start_now_subscription(&transport, &lease, &original, capped_config)
        .await
        .unwrap();
    assert!(matches!(
        capped.next().await,
        Err(NowSubscriptionError::Limit)
    ));
    assert!(matches!(
        capped.next().await,
        Err(NowSubscriptionError::Closed(1000))
    ));
    capped.shutdown().await.unwrap();
    let mut subscription = start_now_subscription(&transport, &lease, &original, config(PROJECT))
        .await
        .unwrap();
    let full = subscription.next().await.unwrap();
    let current = serde_json::to_value(full.instances()[0].now()).unwrap();
    assert_eq!(
        current["generated_at"].as_str(),
        Some("9007199254740993001")
    );
    assert_eq!(current["sections"]["tasks"]["total"].as_u64(), Some(0));
    assert_eq!(
        full.instances()
            .iter()
            .map(|i| (i.instance_id(), i.version()))
            .collect::<Vec<_>>(),
        vec![("native-a", "main"), ("native-b", "branch/ação")]
    );
    subscription.shutdown().await.unwrap();
    let mut registered = start_now_subscription(&transport, &lease, &original, config(PROJECT))
        .await
        .unwrap();
    registered.next().await.unwrap();
    for versions in [vec!["main"], vec!["main", "branch/ação"]] {
        let body = json!({"projects":[{"project_id":PROJECT,"versions":versions}]}).to_string();
        let mut headers = auth_headers(&raw);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let response = http
            .execute(
                NetworkRequest {
                    method: Method::PUT,
                    url: &format!("{origin}/v1/instances/native-b/inventory"),
                    headers: &headers,
                    body: body.as_bytes(),
                },
                NetworkRequestContext::with_deadline(original.deadline(), original.cancellation()),
            )
            .await
            .unwrap();
        assert!(response.status().is_success());
        if versions.len() == 1 {
            renew(&mut p1, "native-a", "main").await;
            loop {
                match registered.next().await {
                    Ok(_) => continue,
                    Err(NowSubscriptionError::Remote(NowRemoteCode::NotRegistered)) => break,
                    Err(error) => {
                        panic!("expected actual version-registration denial, received {error:?}")
                    }
                }
            }
            assert!(matches!(
                registered.next().await,
                Err(NowSubscriptionError::Closed(1000))
            ));
        }
    }
    registered.shutdown().await.unwrap();
    let mut filtered = config(PROJECT);
    filtered.version = Some("branch/ação");
    let mut subscription = start_now_subscription(&transport, &lease, &original, filtered)
        .await
        .unwrap();
    assert_eq!(subscription.next().await.unwrap().instances().len(), 1);
    p2.close(CloseCode::Normal).await.unwrap();
    loop {
        if subscription.next().await.unwrap().instances().is_empty() {
            break;
        }
    }
    subscription.shutdown().await.unwrap();
    p2 = publisher(
        &transport,
        &origin,
        &raw,
        "native-b",
        "branch/ação",
        &original,
    )
    .await;
    let mut restricted = config(PROJECT);
    restricted.instance_id = Some("native-a");
    let mut subscription = start_now_subscription(&transport, &lease, &original, restricted)
        .await
        .unwrap();
    let replacement = subscription.next().await.unwrap();
    assert_eq!(replacement.instances().len(), 1);
    assert_eq!(replacement.instances()[0].instance_id(), "native-a");
    subscription.shutdown().await.unwrap();
    let mut subscription = start_now_subscription(&transport, &lease, &original, config(PROJECT))
        .await
        .unwrap();
    subscription.next().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), subscription.next())
            .await
            .is_err()
    );
    assert!(matches!(
        subscription.next().await,
        Err(NowSubscriptionError::Closed(1000))
    ));
    subscription.shutdown().await.unwrap();
    let mut replay_selection = auth
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "now")
        .unwrap();
    let replay_verifier = cache.get(&mut replay_selection).unwrap();
    let replay_lease = replay_selection
        .native_credential(&replay_verifier)
        .await
        .unwrap();
    let mut replay = start_now_subscription(&transport, &replay_lease, &original, config(PROJECT))
        .await
        .unwrap();
    assert_eq!(replay.next().await.unwrap().instances().len(), 2);
    replay.shutdown().await.unwrap();
    drop(replay_lease);
    drop(replay_selection);
    let mut subscription = start_now_subscription(&transport, &lease, &original, config(OTHER))
        .await
        .unwrap();
    assert!(subscription.next().await.unwrap().instances().is_empty());
    subscription.shutdown().await.unwrap();
    let local_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    connection.execute("INSERT INTO public.local_users(username,subject,identity_kind,password_hash,name,email,organization,teams_json,enabled,password_change_required,revision,created_at,updated_at) VALUES('fixture','fixture-local','human','','Fixture','','','[]',1,0,1,0,0)").await.unwrap();
    let digest = pg::digester()
        .token(TokenKind::LocalAccess, LOCAL_RAW.as_bytes())
        .unwrap();
    sqlx_core::query::query("INSERT INTO public.local_tokens(token_hash,token_id,token_kind,subject,oidc_subject,principal_json,client_id,audience,resource,scopes_json,local_user_revision,family_id,auth_time,expires_at,created_at) VALUES($1,'runtime-local','access','fixture-local','local',$2,'graphit-cli','explicit-audience','','[\"presence-sdk\"]',1,'',0,$3,0)")
        .bind(digest.to_sql_hex()).bind(json!({"issuer":"local","subject":"fixture-local","username":"fixture","auth_method":"fixture","local_user_revision":1}).to_string()).bind(i64::try_from(seconds()*1_000_000).unwrap()+60_000_000).execute(&mut connection).await.unwrap();
    let local = Server::start(
        local_listener,
        local_realtime_router(
            store.clone(),
            LocalRealtimeHttpConfig {
                audience: "explicit-audience".into(),
                identity_json_bytes: n(16384),
                realtime: realtime(),
            },
            &runtime,
        )
        .unwrap(),
    );
    let local_auth = AuthFixture::new(&local.origin, Credential::Local, &original);
    let local_key = local_auth.runtime_key(&original);
    let local_caller = local_auth
        .auth
        .issuer()
        .authenticate_runtime(&local_key, &original)
        .unwrap();
    let mut local_selection = local_auth
        .auth
        .issuer()
        .capture_presence_read_service(&local_caller, "now")
        .unwrap();
    let local_lease = local_selection.credential().unwrap();
    let local_transport = websocket(&local.origin);
    store
        .local_instance_registrar("explicit-audience", n(16384), n(65536))
        .unwrap()
        .register(
            LOCAL_RAW,
            &["presence-sdk".into()],
            &InstanceInventory::new(
                "local-c".into(),
                vec![InstanceProjectVersions::new(PROJECT.into(), vec!["main".into()]).unwrap()],
            )
            .unwrap(),
            pg::deadline(),
        )
        .await
        .unwrap();
    let mut p3 = publisher(
        &local_transport,
        &local.origin,
        LOCAL_RAW,
        "local-c",
        "main",
        &original,
    )
    .await;
    renew(&mut p1, "native-a", "main").await;
    renew(&mut p2, "native-b", "branch/ação").await;
    let mut local_subscription =
        start_now_subscription(&local_transport, &local_lease, &original, config(PROJECT))
            .await
            .unwrap();
    loop {
        let replacement = local_subscription.next().await.unwrap();
        if replacement.instances().len() == 3 {
            break;
        }
        eprintln!(
            "fixture Local current pairs: {:?}",
            replacement
                .instances()
                .iter()
                .map(|item| (item.instance_id(), item.version()))
                .collect::<Vec<_>>()
        );
    }
    local_subscription.shutdown().await.unwrap();
    let expiry: i64 = sqlx_core::query_scalar::query_scalar(
        "SELECT expires_at FROM public.local_tokens WHERE token_id='runtime-local'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    connection
        .execute("UPDATE public.local_tokens SET expires_at=0 WHERE token_id='runtime-local'")
        .await
        .unwrap();
    assert!(matches!(
        start_now_subscription(&local_transport, &local_lease, &original, config(PROJECT)).await,
        Err(NowSubscriptionError::Http { status: 401, .. })
    ));
    sqlx_core::query::query(
        "UPDATE public.local_tokens SET expires_at=$1 WHERE token_id='runtime-local'",
    )
    .bind(expiry)
    .execute(&mut connection)
    .await
    .unwrap();
    let mut insufficient_claims = claims.clone();
    insufficient_claims["scope"] = json!("");
    let insufficient_message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
        URL_SAFE_NO_PAD.encode(insufficient_claims.to_string())
    );
    let insufficient_raw = Zeroizing::new(format!(
        "{insufficient_message}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(insufficient_message.as_bytes()).as_ref())
    ));
    let insufficient_auth = AuthFixture::new(
        &origin,
        Credential::Native(insufficient_raw.to_string()),
        &original,
    );
    let insufficient_key = insufficient_auth.runtime_key(&original);
    let insufficient_caller = insufficient_auth
        .auth
        .issuer()
        .authenticate_runtime(&insufficient_key, &original)
        .unwrap();
    let mut insufficient_selection = insufficient_auth
        .auth
        .issuer()
        .capture_presence_read_service(&insufficient_caller, "now")
        .unwrap();
    let insufficient_verifier = cache.get(&mut insufficient_selection).unwrap();
    let insufficient_lease = insufficient_selection
        .native_credential(&insufficient_verifier)
        .await
        .unwrap();
    assert!(matches!(
        start_now_subscription(&transport, &insufficient_lease, &original, config(PROJECT)).await,
        Err(NowSubscriptionError::Http { status: 403, .. })
    ));
    drop(insufficient_lease);
    drop(insufficient_selection);
    drop(insufficient_caller);
    drop(insufficient_auth);
    // Real broadcast lag: only the Local subscriber's SQL authentication is held.
    let lag_runtime = RealtimeServerRuntime::new(Duration::from_secs(2), n(1)).unwrap();
    let lag_native_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let lag_native = Server::start(
        lag_native_listener,
        broker_realtime_router(
            store.clone(),
            Arc::new(
                BrokerAccessVerifier::new(&origin, "broker-api", None, &jwks, n(8192)).unwrap(),
            ),
            realtime(),
            &lag_runtime,
        )
        .unwrap(),
    );
    let lag_local_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let lag_local = Server::start(
        lag_local_listener,
        local_realtime_router(
            store.clone(),
            LocalRealtimeHttpConfig {
                audience: "explicit-audience".into(),
                identity_json_bytes: n(16384),
                realtime: realtime(),
            },
            &lag_runtime,
        )
        .unwrap(),
    );
    let lag_auth = AuthFixture::new(&lag_local.origin, Credential::Local, &original);
    let lag_key = lag_auth.runtime_key(&original);
    let lag_caller = lag_auth
        .auth
        .issuer()
        .authenticate_runtime(&lag_key, &original)
        .unwrap();
    let mut lag_selection = lag_auth
        .auth
        .issuer()
        .capture_presence_read_service(&lag_caller, "now")
        .unwrap();
    let lag_lease = lag_selection.credential().unwrap();
    let lag_transport = websocket(&lag_local.origin);
    let lag_publish_transport = websocket(&lag_native.origin);
    let mut timeout_connection = fixture.connection().await;
    timeout_connection
        .execute("BEGIN; LOCK TABLE public.local_tokens IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    let timed_upgrade =
        start_now_subscription(&lag_transport, &lag_lease, &original, config(PROJECT)).await;
    timeout_connection.execute("COMMIT").await.unwrap();
    timeout_connection.close().await.unwrap();
    assert!(matches!(
        timed_upgrade,
        Err(NowSubscriptionError::Http { status: 504, .. })
    ));
    let mut lag_publisher = publisher(
        &lag_publish_transport,
        &lag_native.origin,
        &raw,
        "native-a",
        "main",
        &original,
    )
    .await;
    let mut lagged = start_now_subscription(&lag_transport, &lag_lease, &original, config(PROJECT))
        .await
        .unwrap();
    assert_eq!(lagged.next().await.unwrap().instances().len(), 1);
    let mut lock_connection = fixture.connection().await;
    lock_connection
        .execute("BEGIN; LOCK TABLE public.local_tokens IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    renew(&mut lag_publisher, "native-a", "main").await;
    loop {
        original.snapshot().check().unwrap();
        if sqlx_core::query_scalar::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE usename='runtime_now_owner' AND wait_event_type='Lock')")
            .fetch_one(&mut lock_connection).await.unwrap() { break; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for _ in 0..4 {
        renew(&mut lag_publisher, "native-a", "main").await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    lock_connection.execute("COMMIT").await.unwrap();
    loop {
        match lagged.next().await {
            Ok(_) => continue,
            Err(NowSubscriptionError::RecoveryRequired(
                NowRecoveryCause::Notice(NowRemoteCode::Backpressure)
                | NowRecoveryCause::Close(1013),
            )) => break,
            Err(error) => {
                panic!("expected actual backpressure after queued snapshots, received {error:?}")
            }
        }
    }
    assert!(matches!(
        lagged.next().await,
        Err(NowSubscriptionError::Closed(1000))
    ));
    lagged.shutdown().await.unwrap();
    drop(lag_lease);
    drop(lag_selection);
    let mut fresh_selection = lag_auth
        .auth
        .issuer()
        .capture_presence_read_service(&lag_caller, "now")
        .unwrap();
    let fresh_lease = fresh_selection.credential().unwrap();
    let mut fresh =
        start_now_subscription(&lag_transport, &fresh_lease, &original, config(PROJECT))
            .await
            .unwrap();
    assert_eq!(fresh.next().await.unwrap().instances().len(), 1);
    fresh.shutdown().await.unwrap();
    drop(fresh_lease);
    drop(fresh_selection);
    lag_publisher.close(CloseCode::Normal).await.unwrap();
    lag_runtime.shutdown().await.unwrap();
    let mut closed_selection = lag_auth
        .auth
        .issuer()
        .capture_presence_read_service(&lag_caller, "now")
        .unwrap();
    let closed_lease = closed_selection.credential().unwrap();
    assert!(matches!(
        start_now_subscription(&lag_transport, &closed_lease, &original, config(PROJECT)).await,
        Err(NowSubscriptionError::Http { status: 503, .. })
    ));
    drop(closed_lease);
    drop(closed_selection);
    lag_native.shutdown().await;
    lag_local.shutdown().await;
    lock_connection.close().await.unwrap();
    drop(lag_caller);
    drop(lag_auth);
    renew(&mut p1, "native-a", "main").await;
    renew(&mut p2, "native-b", "branch/ação").await;
    renew(&mut p3, "local-c", "main").await;
    let mut subscription = start_now_subscription(&transport, &lease, &original, config(PROJECT))
        .await
        .unwrap();
    subscription.next().await.unwrap();
    acl(&mut connection, &["presence:read", "presence:publish"]).await;
    renew(&mut p1, "native-a", "main").await;
    renew(&mut p2, "native-b", "branch/ação").await;
    renew(&mut p3, "local-c", "main").await;
    loop {
        let replacement = subscription.next().await.unwrap();
        if replacement.instances().len() == 3
            && replacement.instances().iter().all(|item| {
                let now = serde_json::to_value(item.now()).unwrap();
                ["tasks", "sessions", "memories", "knowledge"]
                    .into_iter()
                    .all(|name| {
                        let section = &now["sections"][name];
                        section["availability"] == "failed"
                            && section["error"] == "forbidden"
                            && section["items"] == json!([])
                            && section["total"].is_null()
                            && section["has_more"].is_null()
                    })
            })
        {
            break;
        }
    }
    acl(&mut connection, &["presence:publish"]).await;
    loop {
        match subscription.next().await {
            Ok(_) => continue,
            Err(NowSubscriptionError::Remote(NowRemoteCode::Forbidden)) => break,
            Err(error) => panic!("expected actual forbidden notice, received {error:?}"),
        }
    }
    subscription.shutdown().await.unwrap();
    acl(&mut connection, &["presence:read", "presence:publish"]).await;
    let mut subscription = start_now_subscription(&transport, &lease, &original, config(PROJECT))
        .await
        .unwrap();
    subscription.next().await.unwrap();
    let before = auth.store.load(&original).unwrap();
    let changed = auth
        .store
        .mutate(before, &original, |document, ctx| {
            let provider = document.provider("now").unwrap();
            let revision = provider.revision();
            let config = provider.config().clone();
            document.update_provider("now", config, document.revision(), revision, 2, ctx)
        })
        .unwrap();
    assert_eq!(changed.effect(), AuthPublicationEffect::Applied);
    assert!(changed.failure().is_none());
    assert!(matches!(
        subscription.next().await,
        Err(NowSubscriptionError::Admission(_))
    ));
    subscription.shutdown().await.unwrap();
    for cancel in [true, false] {
        let bounded_signal = Cancellation::default();
        let bounded_original = request(
            &bounded_signal,
            if cancel {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(500)
            },
        );
        let bounded_caller = local_auth
            .auth
            .issuer()
            .authenticate_runtime(&local_key, &bounded_original)
            .unwrap();
        let mut bounded_selection = local_auth
            .auth
            .issuer()
            .capture_presence_read_service(&bounded_caller, "now")
            .unwrap();
        let bounded_lease = bounded_selection.credential().unwrap();
        let mut bounded = start_now_subscription(
            &local_transport,
            &bounded_lease,
            &bounded_original,
            config(PROJECT),
        )
        .await
        .unwrap();
        bounded.next().await.unwrap();
        let outcome = if cancel {
            let (outcome, ()) = tokio::join!(bounded.next(), async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                bounded_signal.cancel();
            });
            outcome
        } else {
            bounded.next().await
        };
        assert!(matches!(outcome, Err(NowSubscriptionError::Request(_))));
        assert!(matches!(
            bounded.next().await,
            Err(NowSubscriptionError::Closed(1000))
        ));
        bounded.shutdown().await.unwrap();
    }
    drop(p1);
    drop(p2);
    drop(p3);
    runtime.shutdown().await.unwrap();
    native.shutdown().await;
    local.shutdown().await;
    drop(lease);
    drop(selection);
    drop(local_lease);
    drop(local_selection);
    drop(caller);
    drop(local_caller);
    drop(auth);
    drop(local_auth);
    connection.close().await.unwrap();
    store.shutdown(pg::deadline()).await.unwrap();
    drop(store);
    fixture.shutdown().unwrap();
}
