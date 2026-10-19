mod native_inventory;
mod standalone_s3;

use super::*;
use crate::{
    CredentialSource, IncomingPrincipalInput, RemoteServiceBindingInput, ServiceAdmissionLimits,
    ServiceAuthorityInput, ServiceBindingView, ServiceMode, ServiceOrigin, ServicePurpose,
    ServiceRuleInput, ServiceUseError, SubjectSelector,
    broker::{BrokerPolicy, BrokerRequestContext, BrokerVerifier, JsonLimits, VerifierLimits},
    profile::{
        AiService, BrokerService, DirectProtocol, LocalCredentials, LocalDevice, LocalExecution,
        OidcTokens, ProfileCredentials, ProfileData, ProfileError, ProfileExpectation,
        ProfileIdentityUpdate, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
        TokenExpectation, TokenUpdate,
    },
};
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Mutex, atomic::AtomicUsize},
    time::UNIX_EPOCH,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};

const MCP_A: &str = "mcp-authority-a";
const MCP_B: &str = "mcp-authority-b";
const EMBEDDING_KEY: &str = "embedding-distinct-key";
const RERANK_KEY: &str = "rerank-distinct-key";
pub(super) const USER_SUB: &str = "stable-real-subject";
pub(super) const INCOMING_RESOURCE: &str = "https://mcp.example/graphit";

pub(super) fn context(cancel: &Cancellation) -> RequestContext<'_> {
    request_for_documents(cancel)
}
fn service_limits() -> ServiceAdmissionLimits {
    ServiceAdmissionLimits {
        max_rules: 16,
        max_binding_bytes: 8192,
    }
}
pub(super) fn policy(
    rules: &[ServiceRuleInput<'_>],
    ordinary: &[Permission],
    cap: IssuerLimits,
    admission: ServiceAdmissionLimits,
    margin: Duration,
) -> Result<crate::IssuerAuthorityPolicy, AuthError> {
    let all = all_permissions();
    let sessions = crate::AdminSessionPolicy {
        lifetime: Duration::from_secs(7200),
        maximum: Duration::from_secs(7200),
    };
    crate::IssuerAuthorityPolicy::new(
        crate::SourceAuthorityPolicy {
            ordinary,
            administrative: &all,
            sessions,
        },
        crate::SourceAuthorityPolicy {
            ordinary,
            administrative: &all,
            sessions,
        },
        cap,
        ServiceAuthorityInput {
            rules,
            limits: admission,
            native_refresh_margin: margin,
        },
    )
}
pub(super) fn issuer(
    fixture: &Fixture,
    store: Arc<AuthStore>,
    ctx: &RequestContext<'_>,
    rules: &[ServiceRuleInput<'_>],
) -> RoleIssuer {
    issuer_config(
        fixture,
        store,
        ctx,
        rules,
        &all_permissions(),
        service_limits(),
        Duration::ZERO,
    )
}
fn issuer_config(
    fixture: &Fixture,
    store: Arc<AuthStore>,
    ctx: &RequestContext<'_>,
    rules: &[ServiceRuleInput<'_>],
    ordinary: &[Permission],
    admission: ServiceAdmissionLimits,
    margin: Duration,
) -> RoleIssuer {
    RoleIssuer::new(
        fixture.realm(),
        Arc::clone(&fixture.runtime),
        store,
        policy(rules, ordinary, limits(), admission, margin).unwrap(),
        ttl(),
        ctx,
    )
    .unwrap()
}
fn s3(source: S3Source) -> S3Location {
    S3Location {
        bucket: None,
        region: None,
        endpoint: None,
        prefix: None,
        source,
    }
}
fn direct_config(endpoint: &str) -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: None,
        },
        embedding: AiService::Direct {
            protocol: DirectProtocol::OpenAi,
            endpoint: endpoint.into(),
            model: "embedding-model".into(),
            model_revision: "fixture-deployment-2026-10".into(),
            recipe: "fixture-vector-v1".into(),
            dimensions: 3,
            request_dimensions: None,
            batch_endpoint: None,
        },
        rerank: AiService::Direct {
            protocol: DirectProtocol::Cohere,
            endpoint: endpoint.into(),
            model: "rerank-model".into(),
            model_revision: "fixture-deployment-2026-10".into(),
            recipe: "fixture-vector-v1".into(),
            dimensions: 0,
            request_dimensions: None,
            batch_endpoint: None,
        },
        s3: s3(S3Source::LocalCredentials),
    }
}
pub(super) fn local_config() -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: None,
        },
        embedding: AiService::Local(cpu()),
        rerank: AiService::Local(cpu()),
        s3: s3(S3Source::LocalCredentials),
    }
}
pub(super) fn cpu() -> LocalExecution {
    LocalExecution {
        device: LocalDevice::Cpu,
        device_id: 0,
    }
}
pub(super) fn broker_config(endpoint: &str, native: bool, anonymous: bool) -> ProviderConfig {
    let broker = BrokerService {
        endpoint: endpoint.into(),
        mcp_resource: None,
        allow_anonymous: anonymous,
    };
    ProviderConfig {
        kind: if native {
            ProviderKind::Broker(broker)
        } else {
            ProviderKind::Local {
                allow_aws_credential_chain: false,
                broker: Some(broker),
            }
        },
        embedding: AiService::Broker,
        rerank: AiService::Broker,
        s3: s3(if native {
            S3Source::BrokerScope
        } else {
            S3Source::LocalCredentials
        }),
    }
}
pub(super) fn local_profile(
    name: &str,
    mcp: &str,
    broker: Option<&str>,
    anonymous: bool,
) -> ProfileData {
    ProfileData {
        name: name.into(),
        provider: "service".into(),
        username: if anonymous {
            "anonymous".into()
        } else {
            name.into()
        },
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous,
            mcp_key: Some(secret(mcp)),
            broker_key: broker.map(secret),
            embedding_api_key: Some(secret(EMBEDDING_KEY)),
            rerank_api_key: Some(secret(RERANK_KEY)),
            s3: None,
        }),
    }
}
fn secret(raw: &str) -> Secret {
    Secret::new(SecretString::from(raw))
}
pub(super) fn native_profile(origin: &str, raw: &str, expiration: u64) -> ProfileData {
    ProfileData {
        name: "native".into(),
        provider: "service".into(),
        username: "display-is-not-sub".into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Broker {
            issuer: origin.into(),
            subject: USER_SUB.into(),
            s3_disabled: true,
            tokens: OidcTokens {
                access_token: secret(raw),
                refresh_token: Some(secret("refresh-private")),
                // A refresh with no ID token preserves this historical token. S1 must not verify it.
                id_token: secret("historical-expired-id-token"),
                expires_at: i64::try_from(Duration::from_secs(expiration).as_micros()).unwrap(),
            },
        },
    }
}
pub(super) fn put(
    store: &AuthStore,
    config: ProviderConfig,
    data: ProfileData,
    activate: bool,
    ctx: &RequestContext<'_>,
) {
    let before = store.load(ctx).unwrap();
    let result = store
        .mutate(before, ctx, |doc, ctx| {
            if doc.provider("service").is_none() {
                doc.add_provider("service".into(), config, doc.revision(), 1, ctx)?;
            }
            let profile = doc.profile(&data.name);
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("service").unwrap().revision(),
                profile_revision: profile.map(|p| p.revision()),
            };
            let now = profile.map_or(1, |p| p.updated_at() + 1);
            let name = data.name.clone();
            doc.replace_profile_data(data, expected, now, ctx)?;
            if activate {
                doc.activate(&name, doc.revision(), ctx)?;
            }
            Ok(())
        })
        .unwrap();
    applied(result);
}
fn applied(result: graphit_core::auth_document::publication::AuthDocumentPublication) {
    use graphit_core::auth_document::publication::AuthPublicationEffect;
    assert_eq!(result.effect(), AuthPublicationEffect::Applied);
    assert_eq!(result.failure(), None);
    assert_eq!(result.cleanup_failure(), None);
}
fn direct_rule(
    endpoint: &str,
    origin: ServiceOrigin,
    purpose: ServicePurpose,
) -> ServiceRuleInput<'_> {
    let (protocol, model, dimensions) = match purpose {
        ServicePurpose::Embedding => (DirectProtocol::OpenAi, "embedding-model", 3),
        ServicePurpose::Rerank => (DirectProtocol::Cohere, "rerank-model", 0),
        ServicePurpose::InventoryRead | ServicePurpose::InventoryPublish => {
            panic!("Inventory purpose cannot use a Direct fixture")
        }
        ServicePurpose::HubRead => panic!("HubRead purpose cannot use a Direct fixture"),
        ServicePurpose::HubWrite => panic!("HubWrite purpose cannot use a Direct fixture"),
        ServicePurpose::RecordStorage => {
            panic!("RecordStorage requires Broker STSv3 or standalone S3, not Direct")
        }
        ServicePurpose::PresenceRead | ServicePurpose::PresencePublish => {
            panic!("Presence purpose cannot use a Direct fixture")
        }
    };
    ServiceRuleInput::Remote {
        origin,
        binding: RemoteServiceBindingInput::Direct {
            provider: "service",
            purpose,
            protocol,
            endpoint,
            model,
            model_revision: "fixture-deployment-2026-10",
            recipe: "fixture-vector-v1",
            dimensions,
            request_dimensions: None,
            batch_endpoint: None,
        },
        credential: CredentialSource::DirectKey,
        incoming: None,
    }
}
pub(super) fn broker_rule(
    endpoint: &str,
    origin: ServiceOrigin,
    credential: CredentialSource,
) -> ServiceRuleInput<'_> {
    ServiceRuleInput::Remote {
        origin,
        binding: RemoteServiceBindingInput::Broker {
            provider: "service",
            purpose: ServicePurpose::Embedding,
            endpoint,
            resource: None,
        },
        credential,
        incoming: (origin == ServiceOrigin::BrokerIncoming).then_some(IncomingPrincipalInput {
            issuer: endpoint,
            subject: SubjectSelector::Exact(USER_SUB),
            incoming_resource: INCOMING_RESOURCE,
        }),
    }
}
fn local_rule(origin: ServiceOrigin) -> ServiceRuleInput<'static> {
    ServiceRuleInput::Local {
        origin,
        provider: "service",
        purpose: ServicePurpose::Embedding,
        execution: cpu(),
    }
}

#[test]
fn web_local_ai_requires_explicit_rule_and_web_direct_remains_invalid() {
    let local = local_rule(ServiceOrigin::Web);
    assert!(
        policy(
            &[local],
            &all_permissions(),
            limits(),
            service_limits(),
            Duration::ZERO
        )
        .is_ok()
    );
    let rerank = ServiceRuleInput::Local {
        origin: ServiceOrigin::Web,
        provider: "service",
        purpose: ServicePurpose::Rerank,
        execution: cpu(),
    };
    let remote = direct_rule(
        "https://model.example",
        ServiceOrigin::Web,
        ServicePurpose::Embedding,
    );
    assert!(
        policy(
            &[rerank],
            &all_permissions(),
            limits(),
            service_limits(),
            Duration::ZERO
        )
        .is_ok()
    );
    {
        let rule = remote;
        assert!(matches!(
            policy(
                &[rule],
                &all_permissions(),
                limits(),
                service_limits(),
                Duration::ZERO
            ),
            Err(AuthError::Invalid)
        ));
    }
}

// Real TCP producer/consumer. Every request and POST is counted; child sockets are
// tracked by JoinSet and aborted when the fixture's listener task is dropped.
pub(super) struct Server {
    pub(super) origin: String,
    key: Ed25519KeyPair,
    counts: Arc<Mutex<BTreeMap<String, usize>>>,
    posted: Arc<Mutex<Vec<String>>>,
    gate: Arc<tokio::sync::Notify>,
    hold_keys: Arc<AtomicBool>,
    entered_keys: Arc<tokio::sync::Notify>,
    pub(super) userinfo_status: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    pub(super) async fn new() -> Self {
        let key = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let metadata = json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,
            "client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback",
            "audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":[INCOMING_RESOURCE]}}).to_string();
        let standard = json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),
            "registration_endpoint":format!("{origin}/register"),
            "token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),
            "userinfo_endpoint":format!("{origin}/userinfo"),"grant_types_supported":["authorization_code","refresh_token"],
            "code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],
            "id_token_signing_alg_values_supported":["EdDSA"]}).to_string();
        let keys = json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),
            "kid":"actual","alg":"EdDSA","use":"sig","key_ops":["verify"]}]}).to_string();
        let counts = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
        let posted = Arc::new(Mutex::new(Vec::<String>::new()));
        let gate = Arc::new(tokio::sync::Notify::new());
        let entered_keys = Arc::new(tokio::sync::Notify::new());
        let hold_keys = Arc::new(AtomicBool::new(false));
        let userinfo_status = Arc::new(AtomicUsize::new(200));
        let c = Arc::clone(&counts);
        let p = Arc::clone(&posted);
        let g = Arc::clone(&gate);
        let e = Arc::clone(&entered_keys);
        let h = Arc::clone(&hold_keys);
        let user_status = Arc::clone(&userinfo_status);
        let task = tokio::spawn(async move {
            let mut sockets = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((mut socket, _)) = accepted else { break; };
                        let counts = Arc::clone(&c); let posted = Arc::clone(&p);
                        let gate = Arc::clone(&g); let entered = Arc::clone(&e); let hold = Arc::clone(&h);
                        let userinfo_status = Arc::clone(&user_status);
                        let metadata = metadata.clone(); let standard = standard.clone(); let keys = keys.clone();
                        sockets.spawn(async move {
                            let mut input = Vec::new(); let mut buffer = [0u8; 1024];
                            while !input.windows(4).any(|b| b == b"\r\n\r\n") {
                                let Ok(read) = socket.read(&mut buffer).await else { return; };
                                if read == 0 || input.len() + read > 16 * 1024 { return; }
                                input.extend_from_slice(&buffer[..read]);
                            }
                            let Ok(request) = std::str::from_utf8(&input) else { return; };
                            let mut line = request.lines().next().unwrap().split_whitespace();
                            let method = line.next().unwrap(); let path = line.next().unwrap();
                            *counts.lock().unwrap().entry(path.into()).or_default() += 1;
                            if method == "POST" {
                                posted.lock().unwrap().push(request.lines().find_map(|line| line.split_once(':')
                                    .filter(|(key, _)| key.eq_ignore_ascii_case("authorization"))
                                    .map(|(_, value)| value.trim().to_owned())).unwrap_or_default());
                            }
                            let body = match path {
                                "/.well-known/graphit-broker" => metadata,
                                "/.well-known/openid-configuration" => standard,
                                "/keys" => {
                                    if hold.load(Ordering::SeqCst) { entered.notify_one(); gate.notified().await; }
                                    keys
                                },
                                "/userinfo" => json!({"sub":USER_SUB}).to_string(),
                                "/register" => json!({"client_id":"web-client"}).to_string(),
                                "/service" => "{}".into(),
                                _ => return,
                            };
                            let status = if path == "/userinfo" { userinfo_status.load(Ordering::SeqCst) } else if path == "/register" { 201 } else { 200 };
                            let response = format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len());
                            let _ = socket.write_all(response.as_bytes()).await;
                        });
                    },
                    _ = sockets.join_next(), if !sockets.is_empty() => (),
                }
            }
        });
        Self {
            origin,
            key,
            counts,
            posted,
            gate,
            hold_keys,
            entered_keys,
            userinfo_status,
            task,
        }
    }
    pub(super) fn count(&self, path: &str) -> usize {
        self.counts.lock().unwrap().get(path).copied().unwrap_or(0)
    }
    pub(super) fn token(&self, native: bool, expiry: u64) -> String {
        let now = now_seconds();
        let claims = json!({"iss":self.origin,"sub":USER_SUB,"aud":if native { json!(["broker-api"]) } else { json!([INCOMING_RESOURCE]) },
            "client_id":if native { "graphit-cli" } else { "another-registered-client" },
            "jti":"actual-token","token_use":"access","iat":now.saturating_sub(1),"exp":expiry,
            "preferred_username":"display-not-sub"});
        self.sign(&claims)
    }
    pub(super) fn sign(&self, claims: &Value) -> String {
        let message = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        format!(
            "{message}.{}",
            URL_SAFE_NO_PAD.encode(self.key.sign(message.as_bytes()).as_ref())
        )
    }
    pub(super) fn verifier(&self, incoming: bool, revision: u64) -> BrokerVerifier {
        BrokerVerifier::new(
            BrokerPolicy {
                endpoint: &self.origin,
                mcp_resource: incoming.then_some(INCOMING_RESOURCE),
                provider_revision: revision,
                network: ClientPolicy {
                    origin: &self.origin,
                    allow_loopback_http: true,
                    proxy: None,
                    additional_roots: &[],
                },
            },
            NetworkLimits {
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
        )
        .unwrap()
    }
    async fn keys_entered(&self) {
        tokio::time::timeout(Duration::from_secs(10), self.entered_keys.notified())
            .await
            .unwrap();
    }
    fn pause_keys(&self) {
        self.hold_keys.store(true, Ordering::SeqCst);
    }
    fn release_keys(&self) {
        self.gate.notify_one();
    }
    pub(super) async fn post(&self, key: Option<&str>) {
        use tokio::net::TcpStream;
        let authority = self.origin.strip_prefix("http://").unwrap();
        let mut socket = TcpStream::connect(authority).await.unwrap();
        let authorization = key.map_or(String::new(), |key| {
            format!("Authorization: Bearer {key}\r\n")
        });
        socket.write_all(format!("POST /service HTTP/1.1\r\nHost: {authority}\r\n{authorization}Content-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        let mut response = Vec::new();
        socket.read_to_end(&mut response).await.unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200"));
    }
    fn post_keys(&self) -> Vec<String> {
        self.posted.lock().unwrap().clone()
    }
}
pub(super) fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
async fn runtime_local_and_role_use_two_distinct_real_service_keys() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        direct_config(&server.origin),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    let rules = [
        direct_rule(
            &server.origin,
            ServiceOrigin::Runtime,
            ServicePurpose::Embedding,
        ),
        direct_rule(
            &server.origin,
            ServiceOrigin::Runtime,
            ServicePurpose::Rerank,
        ),
        direct_rule(
            &server.origin,
            ServiceOrigin::Local,
            ServicePurpose::Embedding,
        ),
        direct_rule(&server.origin, ServiceOrigin::Local, ServicePurpose::Rerank),
    ];
    let issuer = issuer(&f, store, &ctx, &rules);
    let runtime = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let local = issuer.authenticate_local(MCP_A, &ctx).unwrap();
    for caller in [&runtime, &local] {
        for (purpose, expected) in [
            (ServicePurpose::Embedding, EMBEDDING_KEY),
            (ServicePurpose::Rerank, RERANK_KEY),
        ] {
            let mut selected = issuer.capture_service(caller, purpose).unwrap();
            let lease = selected.credential().unwrap();
            let key = lease.credential().unwrap().unwrap();
            assert_eq!(key, expected);
            assert!(lease.user_subject().unwrap().is_none());
            lease.check().unwrap();
            server.post(Some(key)).await;
            lease.check().unwrap();
        }
    }
    let session = issuer.bootstrap_admin(&runtime).unwrap();
    let parent = issuer.admin_parent(&runtime, &session).unwrap();
    let remote = Permission::External(ExternalEffect::RemoteRequest);
    let def = f.definition(Role::Coordinator, &[remote], 1, limits());
    let reference = def.reference().clone();
    issuer.register_definition(&parent, def).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &[remote], &f.scope())
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    for purpose in [ServicePurpose::Embedding, ServicePurpose::Rerank] {
        let mut selected = issuer.capture_role_service(&grant, purpose).unwrap();
        let lease = selected.credential().unwrap();
        let key = lease.credential().unwrap().unwrap();
        lease.check().unwrap();
        server.post(Some(key)).await;
        lease.check().unwrap();
    }
    assert_eq!(server.count("/service"), 6);
    assert_eq!(
        server.post_keys(),
        (0..3)
            .flat_map(|_| [EMBEDDING_KEY, RERANK_KEY])
            .map(|key| format!("Bearer {key}"))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn permission_and_purpose_denials_make_zero_service_posts() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        direct_config(&server.origin),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    let rules = [
        direct_rule(
            &server.origin,
            ServiceOrigin::Runtime,
            ServicePurpose::Embedding,
        ),
        direct_rule(
            &server.origin,
            ServiceOrigin::Local,
            ServicePurpose::Embedding,
        ),
    ];
    let denied = issuer_config(
        &f,
        Arc::clone(&store),
        &ctx,
        &rules,
        &[Permission::InspectGit],
        service_limits(),
        Duration::ZERO,
    );
    let runtime = denied.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let local = denied.authenticate_local(MCP_A, &ctx).unwrap();
    for caller in [&runtime, &local] {
        assert!(matches!(
            denied.capture_service(caller, ServicePurpose::Embedding),
            Err(ServiceUseError::Auth(AuthError::Denied))
        ));
    }
    let allowed = issuer(&f, store, &ctx, &rules);
    let caller = allowed.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        allowed.capture_service(&caller, ServicePurpose::Rerank),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    assert_eq!(server.count("/service"), 0);
}

#[test]
fn service_observations_are_independent_and_preserve_original_rechecks() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        local_config(),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    let issuer = issuer_config(
        &f,
        store,
        &ctx,
        &[local_rule(ServiceOrigin::Runtime)],
        &all_permissions(),
        service_limits(),
        Duration::ZERO,
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let mut first = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let mut second = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    assert_eq!(first.local().unwrap().execution().unwrap(), cpu());
    assert_eq!(second.local().unwrap().execution().unwrap(), cpu());
    drop(first);
    assert_eq!(second.local().unwrap().execution().unwrap(), cpu());
    let mut third = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    assert_eq!(third.local().unwrap().execution().unwrap(), cpu());
    cancel.cancel();
    assert!(second.local().is_err());
    assert!(third.local().is_err());
}

#[test]
fn constructor_prechecks_actual_rule_and_binding_inputs() {
    let endpoint = "https://fixture.example";
    let rules = [direct_rule(
        endpoint,
        ServiceOrigin::Runtime,
        ServicePurpose::Embedding,
    )];
    let all = all_permissions();
    assert!(policy(&rules, &all, limits(), service_limits(), Duration::ZERO).is_ok());
    let mut scope = limits();
    scope.max_scope_bytes = endpoint.len() - 1;
    assert!(matches!(
        policy(&rules, &all, scope, service_limits(), Duration::ZERO),
        Err(AuthError::Limit)
    ));
    let mut bytes = service_limits();
    let ServiceRuleInput::Remote {
        binding:
            RemoteServiceBindingInput::Direct {
                provider,
                endpoint,
                model,
                model_revision,
                recipe,
                ..
            },
        ..
    } = rules[0]
    else {
        unreachable!("Direct fixture")
    };
    bytes.max_binding_bytes = [provider, endpoint, model, model_revision, recipe]
        .iter()
        .map(|value| value.len())
        .sum();
    assert!(policy(&rules, &all, limits(), bytes, Duration::ZERO).is_ok());
    bytes.max_binding_bytes -= 1;
    assert!(matches!(
        policy(&rules, &all, limits(), bytes, Duration::ZERO),
        Err(AuthError::Limit)
    ));
    assert!(matches!(
        policy(
            &rules,
            &[Permission::InspectGit, Permission::InspectGit],
            limits(),
            service_limits(),
            Duration::ZERO
        ),
        Err(AuthError::Invalid)
    ));
}

#[test]
fn zero_overflow_duplicate_and_invalid_service_configs_fail_closed() {
    let rules = [local_rule(ServiceOrigin::Runtime)];
    for selector in 0..2 {
        let mut l = service_limits();
        match selector {
            0 => l.max_rules = 0,
            _ => l.max_binding_bytes = 0,
        };
        assert!(policy(&rules, &all_permissions(), limits(), l, Duration::ZERO).is_err());
    }
    let mut l = service_limits();
    l.max_rules = usize::MAX;
    assert!(policy(&rules, &all_permissions(), limits(), l, Duration::ZERO).is_err());
    assert!(
        policy(
            &rules,
            &all_permissions(),
            limits(),
            service_limits(),
            Duration::MAX
        )
        .is_err()
    );
    assert!(
        policy(
            &[rules[0], rules[0]],
            &all_permissions(),
            limits(),
            service_limits(),
            Duration::ZERO
        )
        .is_err()
    );
    let bad = ServiceRuleInput::Local {
        origin: ServiceOrigin::BrokerIncoming,
        provider: "service",
        purpose: ServicePurpose::Embedding,
        execution: cpu(),
    };
    assert!(
        policy(
            &[bad],
            &all_permissions(),
            limits(),
            service_limits(),
            Duration::ZERO
        )
        .is_err()
    );
    let mut rule = broker_rule(
        "https://broker.example",
        ServiceOrigin::BrokerIncoming,
        CredentialSource::IncomingBearer,
    );
    if let ServiceRuleInput::Remote {
        incoming: Some(ref mut incoming),
        ..
    } = rule
    {
        incoming.incoming_resource = "";
    }
    assert!(
        policy(
            &[rule],
            &all_permissions(),
            limits(),
            service_limits(),
            Duration::ZERO
        )
        .is_err()
    );
    // These incoherent startup tuples are constructor failures, not admission
    // denials. Do not pass them through the fixture's valid-policy unwrap.
    for change_issuer in [false, true] {
        let mut rule = broker_rule(
            "https://broker.example",
            ServiceOrigin::BrokerIncoming,
            CredentialSource::IncomingBearer,
        );
        if let ServiceRuleInput::Remote {
            binding:
                RemoteServiceBindingInput::Broker {
                    ref mut endpoint, ..
                },
            incoming: Some(ref mut principal),
            ..
        } = rule
        {
            if change_issuer {
                principal.issuer = "https://other-issuer.example";
            } else {
                *endpoint = "https://other-outbound.example";
            }
        }
        assert!(matches!(
            policy(
                &[rule],
                &all_permissions(),
                limits(),
                service_limits(),
                Duration::ZERO
            ),
            Err(AuthError::Invalid)
        ));
    }
}

#[test]
fn explicit_local_disabled_missing_and_no_active_are_distinct_without_writes() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    let local_rules = [local_rule(ServiceOrigin::Runtime)];
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &local_rules);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();

    assert!(matches!(
        issuer.capture_service(&caller, ServicePurpose::Embedding),
        Err(ServiceUseError::Profile(ProfileError::NoActive))
    ));

    assert!(!f.installation.owner_root().join("auth.json").exists());
    put(
        &store,
        local_config(),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    {
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        assert_eq!(selection.mode().unwrap(), ServiceMode::LocalAuthorized);
        assert!(matches!(
            selection.credential(),
            Err(ServiceUseError::WrongMode)
        ));
        let lease = selection.local().unwrap();
        assert_eq!(lease.execution().unwrap(), cpu());
    }
    let mut disabled_config = local_config();
    disabled_config.embedding = AiService::Disabled;
    let mutation = store.load(&ctx).unwrap();
    applied(
        store
            .mutate(mutation, &ctx, |doc, ctx| {
                let provider = doc.provider("service").unwrap();
                let revision = provider.revision();
                let now = provider.updated_at() + 1;
                doc.update_provider(
                    "service",
                    disabled_config.clone(),
                    doc.revision(),
                    revision,
                    now,
                    ctx,
                )
            })
            .unwrap(),
    );
    assert!(matches!(
        issuer.capture_service(&caller, ServicePurpose::Embedding),
        Err(ServiceUseError::Profile(ProfileError::Stale))
    ));
    put(
        &store,
        disabled_config,
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    assert!(matches!(
        issuer.capture_service(&caller, ServicePurpose::Embedding),
        Err(ServiceUseError::Disabled)
    ));
}

#[test]
fn broker_profile_rejects_local_and_direct_inference_before_publication() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = broker_embedding_store(&f, &ctx);
    let direct = direct_config("https://inference.example");
    for rerank in [false, true] {
        for service in [
            AiService::Local(cpu()),
            direct.embedding.clone(),
            direct.rerank.clone(),
        ] {
            let before = store.load(&ctx).unwrap();
            let original = before.document().encode(&ctx).unwrap();
            let mut config = broker_config("https://broker.example", true, false);
            if rerank {
                config.rerank = service;
            } else {
                config.embedding = service;
            }
            let result = store.mutate(before, &ctx, |doc, ctx| {
                doc.update_provider(
                    "service",
                    config,
                    doc.revision(),
                    doc.provider("service").unwrap().revision(),
                    2,
                    ctx,
                )
            });
            assert!(matches!(
                result,
                Err(crate::store::AuthStoreError::Profile(
                    ProfileError::InvalidDocument
                ))
            ));
            assert_eq!(
                &*store.load(&ctx).unwrap().document().encode(&ctx).unwrap(),
                &*original
            );
        }
    }
    let denied = issuer(&f, store, &ctx, &[local_rule(ServiceOrigin::Runtime)]);
    let caller = denied.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        denied.capture_service(&caller, ServicePurpose::Embedding),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
}

fn broker_embedding_store(f: &Fixture, ctx: &RequestContext<'_>) -> Arc<AuthStore> {
    let store = native_store(f, ctx);
    put(
        &store,
        broker_config("https://broker.example", true, false),
        native_profile(
            "https://broker.example",
            "unused-native-token",
            now_seconds() + 600,
        ),
        true,
        ctx,
    );
    store
}

fn local_embedding_store(f: &Fixture, ctx: &RequestContext<'_>) -> Arc<AuthStore> {
    let store = f.auth_store(ctx);
    put(
        &store,
        local_config(),
        local_profile("native", MCP_A, None, false),
        true,
        ctx,
    );
    store
}

#[test]
fn broker_runtime_selection_survives_unrelated_provider_and_profile_publications() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = broker_embedding_store(&f, &ctx);
    let issuer = issuer(
        &f,
        Arc::clone(&store),
        &ctx,
        &[broker_rule(
            "https://broker.example",
            ServiceOrigin::Runtime,
            CredentialSource::NativeAccess,
        )],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let mut selected = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let mutation = context(&cancel);
    let before = store.load(&mutation).unwrap();
    applied(
        store
            .mutate(before, &mutation, |doc, ctx| {
                doc.add_provider("unrelated".into(), local_config(), doc.revision(), 1, ctx)
            })
            .unwrap(),
    );
    assert_eq!(selected.mode().unwrap(), ServiceMode::BrokerNative);
    for token in ["another-profile-token", "rotated-other-profile-token"] {
        let mut data = native_profile("https://broker.example", token, now_seconds() + 600);
        data.name = "other".into();
        put(
            &store,
            broker_config("https://broker.example", true, false),
            data,
            false,
            &mutation,
        );
        assert_eq!(selected.mode().unwrap(), ServiceMode::BrokerNative);
    }
    assert!(std::ptr::eq(selected.original().unwrap(), &ctx));
    // The selected credential still has to be the captured one.
    applied(
        store
            .commit(prepare_native_token_rotation(&store, &mutation))
            .unwrap(),
    );
    assert!(selected.mode().is_err());
}

#[test]
fn broker_profile_token_rotation_invalidates_captured_service() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = broker_embedding_store(&f, &ctx);
    let issuer = issuer(
        &f,
        Arc::clone(&store),
        &ctx,
        &[broker_rule(
            "https://broker.example",
            ServiceOrigin::Runtime,
            CredentialSource::NativeAccess,
        )],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let mut selected = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    assert_eq!(selected.mode().unwrap(), ServiceMode::BrokerNative);
    assert!(matches!(selected.local(), Err(ServiceUseError::WrongMode)));
    applied(
        store
            .commit(prepare_native_token_rotation(&store, &ctx))
            .unwrap(),
    );
    assert!(selected.mode().is_err());
    let mut current = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    assert_eq!(current.mode().unwrap(), ServiceMode::BrokerNative);
    assert!(std::ptr::eq(current.original().unwrap(), &ctx));
}

fn prepare_native_token_rotation<'a>(
    store: &'a AuthStore,
    mutation: &'a RequestContext<'a>,
) -> crate::store::PreparedAuthPublication<'a> {
    let before = store.load(mutation).unwrap();
    let document = before.document();
    let (provider, profile) = document.active(mutation).unwrap();
    let name = profile.name().to_owned();
    let expected = TokenExpectation {
        document_revision: document.revision(),
        provider_revision: provider.revision(),
        profile_revision: profile.revision(),
        token_revision: profile.token_revision(),
    };
    let now = profile.updated_at() + 1;
    store
        .prepare(before, mutation, |doc, ctx| {
            doc.rotate_tokens(
                &name,
                TokenUpdate {
                    access_token: secret("rotated-unused-access"),
                    refresh_token: secret("rotated-unused-refresh"),
                    identity: ProfileIdentityUpdate::Preserve,
                    expires_at: i64::try_from(
                        Duration::from_secs(now_seconds() + 1200).as_micros(),
                    )
                    .unwrap(),
                },
                expected,
                now,
                ctx,
            )
        })
        .unwrap()
}

fn prepare_local_profile_publication<'a>(
    store: &'a AuthStore,
    mutation: &'a RequestContext<'a>,
) -> crate::store::PreparedAuthPublication<'a> {
    let before = store.load(mutation).unwrap();
    let document = before.document();
    let (provider, profile) = document.active(mutation).unwrap();
    let expected = ProfileExpectation {
        document_revision: document.revision(),
        provider_revision: provider.revision(),
        profile_revision: Some(profile.revision()),
    };
    let now = profile.updated_at() + 1;
    store
        .prepare(before, mutation, |document, context| {
            document.replace_profile_data(
                local_profile("native", MCP_B, None, false),
                expected,
                now,
                context,
            )
        })
        .unwrap()
}

#[test]
fn uncoordinated_read_keeps_conflict_during_local_profile_publication() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = local_embedding_store(&f, &ctx);
    let issuer = issuer(
        &f,
        Arc::clone(&store),
        &ctx,
        &[local_rule(ServiceOrigin::Runtime)],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let mut selected = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selected.local().unwrap();
    let before = store.load(&ctx).unwrap();
    let (provider, profile) = before.document().active(&ctx).unwrap();
    let provider_revision = provider.revision();
    let profile_revision = profile.revision();
    let token_revision = profile.token_revision();
    let writer = Arc::clone(&store);
    crate::store::AFTER_AUTH_READ.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Box::new(move || {
            let cancel = Cancellation::default();
            let mutation = context(&cancel);
            applied(
                writer
                    .commit(prepare_local_profile_publication(&writer, &mutation))
                    .unwrap(),
            );
        }));
    });
    // The publisher replaces the document after this uncoordinated physical read.
    assert!(matches!(
        store.load(&ctx),
        Err(crate::store::AuthStoreError::Core(
            graphit_core::auth_document::AuthDocumentError::Conflict
        ))
    ));
    crate::store::AFTER_AUTH_READ.with(|slot| assert!(slot.borrow().is_none()));
    let after = store.load(&ctx).unwrap();
    let (provider, profile) = after.document().active(&ctx).unwrap();
    assert_eq!(provider.revision(), provider_revision);
    assert_eq!(profile.name(), "native");
    assert_eq!(profile.revision(), profile_revision + 1);
    assert_eq!(profile.token_revision(), token_revision + 1);
    // Runtime local execution revalidates the same provider/profile selection.
    lease.check().unwrap();
    assert_eq!(lease.execution().unwrap(), cpu());
    assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
}

#[test]
fn locked_absence_does_not_rebase_to_a_newly_published_profile() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    let (guard, absent) = store.load_locked(&ctx).unwrap();
    assert!(guard.is_none());
    assert!(!absent.was_present());
    assert!(!f.installation.owner_root().join(".auth.json.lock").exists());
    let writer = Arc::clone(&store);
    crate::store::AFTER_AUTH_READ.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Box::new(move || {
            let cancel = Cancellation::default();
            let mutation = context(&cancel);
            put(
                &writer,
                local_config(),
                local_profile("new", MCP_A, None, false),
                true,
                &mutation,
            );
        }));
    });
    assert!(matches!(
        store.load_locked(&ctx),
        Err(crate::store::AuthStoreError::Core(
            graphit_core::auth_document::AuthDocumentError::Conflict
        ))
    ));
    let (guard, present) = store.load_locked(&ctx).unwrap();
    assert!(guard.is_some());
    assert!(present.was_present());
    assert_eq!(present.document().active(&ctx).unwrap().1.name(), "new");
}

#[test]
fn local_runtime_capture_and_barrier_coordinate_with_profile_publication() {
    for during_capture in [true, false] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = local_embedding_store(&f, &ctx);
        let issuer = issuer(
            &f,
            Arc::clone(&store),
            &ctx,
            &[local_rule(ServiceOrigin::Runtime)],
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = if during_capture {
            None
        } else {
            Some(
                issuer
                    .capture_service(&caller, ServicePurpose::Embedding)
                    .unwrap(),
            )
        };
        let completed = Arc::new(Mutex::new(None));
        let completed_in_hook = Arc::clone(&completed);
        let writer = Arc::clone(&store);
        let path = f.installation.owner_root().join(".auth.json.lock");
        crate::store::AFTER_AUTH_READ.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Box::new(move || {
                assert!(graphit_core::file_lock::FileLock::is_locked(&path).unwrap());
                let (ready, prepared) = std::sync::mpsc::channel();
                let worker = std::thread::spawn(move || {
                    let cancel = Cancellation::default();
                    let mutation = context(&cancel);
                    let publication = prepare_local_profile_publication(&writer, &mutation);
                    ready.send(()).unwrap();
                    applied(writer.commit(publication).unwrap());
                });
                prepared.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(!worker.is_finished());
                *completed_in_hook.lock().unwrap() = Some(worker);
            }));
        });
        let lease = if during_capture {
            selected = Some(
                issuer
                    .capture_service(&caller, ServicePurpose::Embedding)
                    .unwrap(),
            );
            selected.as_mut().unwrap().local().unwrap()
        } else {
            let lease = selected.as_mut().unwrap().local().unwrap();
            lease.check().unwrap();
            lease
        };
        let worker = completed.lock().unwrap().take().unwrap();
        worker.join().unwrap();
        let after = store.load(&ctx).unwrap();
        let (provider, profile) = after.document().active(&ctx).unwrap();
        assert_eq!(profile.name(), "native");
        assert_eq!(provider.config().embedding, AiService::Local(cpu()));
        lease.check().unwrap();
        assert_eq!(lease.execution().unwrap(), cpu());
        assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                f.installation.owner_root().join(".auth.json.lock")
            )
            .unwrap()
        );
    }
}

#[test]
fn local_runtime_selection_changes_deny_existing_lease() {
    for change in 0..6 {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = local_embedding_store(&f, &ctx);
        let issuer = issuer(
            &f,
            Arc::clone(&store),
            &ctx,
            &[local_rule(ServiceOrigin::Runtime)],
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selected.local().unwrap();
        let mutation = context(&cancel);
        match change {
            0 => {
                let mut profile = local_profile("other-active", MCP_A, None, false);
                profile.name = "other-active".into();
                put(&store, local_config(), profile, true, &mutation);
            }
            1 => {
                let before = store.load(&mutation).unwrap();
                applied(
                    store
                        .mutate(before, &mutation, |doc, ctx| {
                            doc.logout(None, doc.revision(), ctx)
                        })
                        .unwrap(),
                );
            }
            2..=4 => {
                let before = store.load(&mutation).unwrap();
                applied(
                    store
                        .mutate(before, &mutation, |doc, ctx| {
                            let p = doc.provider("service").unwrap();
                            let mut config = local_config();
                            config.embedding = match change {
                                2 => AiService::Local(cpu()),
                                3 => AiService::Local(LocalExecution {
                                    device: LocalDevice::Cpu,
                                    device_id: 1,
                                }),
                                _ => AiService::Disabled,
                            };
                            doc.update_provider(
                                "service",
                                config,
                                doc.revision(),
                                p.revision(),
                                p.updated_at() + 1,
                                ctx,
                            )
                        })
                        .unwrap(),
                );
                // Rebind the active profile so the old lease must reject the
                // provider revision even when the current active data is valid.
                put(
                    &store,
                    local_config(),
                    local_profile("native", MCP_A, None, false),
                    true,
                    &mutation,
                );
            }
            _ => std::fs::remove_file(f.installation.owner_root().join("auth.json")).unwrap(),
        }
        assert!(lease.check().is_err(), "change {change}");
        assert!(lease.binding().is_err());
        assert!(lease.execution().is_err());
        assert!(lease.original().is_err());
        assert!(selected.mode().is_err());
    }
}

#[test]
fn local_runtime_lease_keeps_runtime_and_policy_revocation() {
    for revoke_runtime in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = local_embedding_store(&f, &ctx);
        let issuer = issuer(&f, store, &ctx, &[local_rule(ServiceOrigin::Runtime)]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selected.local().unwrap();
        if revoke_runtime {
            assert_eq!(f.runtime.remove(ctx.snapshot()).failure(), None);
        } else {
            let session = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &session).unwrap();
            issuer.revoke_service_policy(&parent).unwrap();
        }
        assert!(lease.check().is_err());
        assert!(lease.binding().is_err());
        assert!(lease.execution().is_err());
        assert!(lease.original().is_err());
    }
}

#[cfg(unix)]
#[test]
fn local_runtime_lease_keeps_document_privacy_and_installation_binding() {
    use std::os::unix::fs::PermissionsExt;
    for move_installation in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = local_embedding_store(&f, &ctx);
        let issuer = issuer(&f, store, &ctx, &[local_rule(ServiceOrigin::Runtime)]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selected.local().unwrap();
        let root = f.installation.owner_root();
        if move_installation {
            std::fs::rename(root, root.with_extension("moved")).unwrap();
        } else {
            std::fs::set_permissions(
                root.join("auth.json"),
                std::fs::Permissions::from_mode(0o644),
            )
            .unwrap();
        }
        assert!(lease.check().is_err());
        assert!(lease.binding().is_err());
        assert!(lease.execution().is_err());
        assert!(lease.original().is_err());
    }
}

#[test]
fn local_caller_keeps_original_document_fence_for_local_execution() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        local_config(),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    let issuer = issuer(
        &f,
        Arc::clone(&store),
        &ctx,
        &[local_rule(ServiceOrigin::Local)],
    );
    let caller = issuer.authenticate_local(MCP_A, &ctx).unwrap();
    let mut selected = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selected.local().unwrap();
    put(
        &store,
        local_config(),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    ctx.check().unwrap();
    for observed in [
        lease.check(),
        lease.binding().map(|_| ()),
        lease.execution().map(|_| ()),
        lease.original().map(|_| ()),
    ] {
        assert!(matches!(
            observed,
            Err(ServiceUseError::Auth(AuthError::Denied))
        ));
    }
}

#[tokio::test]
async fn static_and_anonymous_broker_modes_are_explicit_and_never_subjects() {
    let server = Server::new().await;
    for anonymous in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = f.auth_store(&ctx);
        put(
            &store,
            broker_config(&server.origin, false, anonymous),
            local_profile(
                "a",
                MCP_A,
                (!anonymous).then_some("static-broker-key"),
                anonymous,
            ),
            true,
            &ctx,
        );
        let rule = broker_rule(
            &server.origin,
            ServiceOrigin::Runtime,
            if anonymous {
                CredentialSource::Anonymous
            } else {
                CredentialSource::LocalBrokerKey
            },
        );
        let issuer = issuer(&f, store, &ctx, &[rule]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selected.credential().unwrap();
        assert!(lease.user_subject().unwrap().is_none());
        let key = lease.credential().unwrap();
        assert_eq!(
            key,
            if anonymous {
                None
            } else {
                Some("static-broker-key")
            }
        );
        lease.check().unwrap();
        server.post(key).await;
        lease.check().unwrap();
    }
    assert_eq!(
        server.post_keys(),
        vec!["Bearer static-broker-key".to_owned(), String::new()]
    );
    assert_eq!(server.count("/userinfo"), 0);
}

#[test]
fn missing_key_and_present_key_under_anonymous_rule_do_not_fallback() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    let mut invalid_direct = local_profile("a", MCP_A, None, false);
    if let ProfileCredentials::Local(keys) = &mut invalid_direct.credentials {
        keys.embedding_api_key = None;
    }

    let publication = store.mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
        doc.add_provider(
            "service".into(),
            direct_config("https://direct.example"),
            doc.revision(),
            1,
            ctx,
        )?;
        let expected = ProfileExpectation {
            document_revision: doc.revision(),
            provider_revision: doc.provider("service").unwrap().revision(),
            profile_revision: None,
        };
        doc.replace_profile_data(invalid_direct, expected, 1, ctx)
    });
    assert!(matches!(
        publication,
        Err(crate::store::AuthStoreError::Profile(
            ProfileError::InvalidDocument
        ))
    ));

    assert!(!f.installation.owner_root().join("auth.json").exists());
    for anonymous in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = f.auth_store(&ctx);
        let profile = local_profile(
            "a",
            MCP_A,
            anonymous.then_some("must-not-be-ignored"),
            false,
        );
        let config = broker_config("https://broker.example", false, true);
        put(&store, config, profile, true, &ctx);
        let rule = if anonymous {
            broker_rule(
                "https://broker.example",
                ServiceOrigin::Runtime,
                CredentialSource::Anonymous,
            )
        } else {
            broker_rule(
                "https://broker.example",
                ServiceOrigin::Runtime,
                CredentialSource::LocalBrokerKey,
            )
        };
        let issuer = issuer(&f, store, &ctx, &[rule]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        if anonymous {
            assert!(matches!(
                selected.credential(),
                Err(ServiceUseError::Auth(AuthError::Denied))
            ));
        } else {
            assert!(matches!(
                selected.credential(),
                Err(ServiceUseError::MissingCredential)
            ));
        }
        assert!(matches!(selected.local(), Err(ServiceUseError::WrongMode)));
    }
}

#[test]
fn physical_account_key_provider_and_logout_changes_invalidate_all_old_outputs() {
    for change in 0..4 {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = f.auth_store(&ctx);
        put(
            &store,
            direct_config("https://direct.example"),
            local_profile("a", MCP_A, None, false),
            true,
            &ctx,
        );
        let rules = [direct_rule(
            "https://direct.example",
            ServiceOrigin::Runtime,
            ServicePurpose::Embedding,
        )];
        let issuer = issuer(&f, Arc::clone(&store), &ctx, &rules);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selected.credential().unwrap();
        let key = lease.credential().unwrap().unwrap();
        assert_eq!(key, EMBEDDING_KEY);
        let mutation = context(&cancel);
        match change {
            0 => put(
                &store,
                direct_config("https://direct.example"),
                local_profile("b", MCP_B, None, false),
                true,
                &mutation,
            ),
            1 => {
                let mut profile = local_profile("a", MCP_A, None, false);
                if let ProfileCredentials::Local(keys) = &mut profile.credentials {
                    keys.embedding_api_key = Some(secret("replacement-key"));
                }
                put(
                    &store,
                    direct_config("https://direct.example"),
                    profile,
                    true,
                    &mutation,
                );
            }
            2 => {
                let before = store.load(&mutation).unwrap();
                applied(
                    store
                        .mutate(before, &mutation, |doc, ctx| {
                            let p = doc.provider("service").unwrap();
                            let revision = p.revision();
                            let now = p.updated_at() + 1;
                            doc.update_provider(
                                "service",
                                direct_config("https://changed.example"),
                                doc.revision(),
                                revision,
                                now,
                                ctx,
                            )
                        })
                        .unwrap(),
                );
            }
            _ => {
                let before = store.load(&mutation).unwrap();
                applied(
                    store
                        .mutate(before, &mutation, |doc, ctx| {
                            doc.logout(None, doc.revision(), ctx)
                        })
                        .unwrap(),
                );
            }
        }
        // The raw immutable borrow stays finite and unchanged; every permission-bearing
        // getter must recheck the original observation before exposing any new output.
        assert_eq!(key, EMBEDDING_KEY);
        assert!(lease.check().is_err());
        assert!(lease.credential().is_err());
        assert!(lease.binding().is_err());
        assert!(lease.original().is_err());
        assert!(lease.user_subject().is_err());
        drop(lease);
        assert!(selected.mode().is_err());
        if change < 2 {
            let fresh = context(&cancel);
            let caller = issuer.authenticate_runtime(&f.secret(), &fresh).unwrap();
            let mut fresh_selection = issuer
                .capture_service(&caller, ServicePurpose::Embedding)
                .unwrap();
            assert_eq!(
                fresh_selection.credential().unwrap().credential().unwrap(),
                Some(if change == 1 {
                    "replacement-key"
                } else {
                    EMBEDDING_KEY
                })
            );
            assert!(selected.mode().is_err());
        }
    }
}

#[test]
fn live_policy_revocation_denies_old_and_new_and_foreign_revoker_changes_nothing() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        direct_config("https://direct.example"),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    let rules = [direct_rule(
        "https://direct.example",
        ServiceOrigin::Runtime,
        ServicePurpose::Embedding,
    )];
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &rules);
    let foreign = super::service_credentials::issuer(&f, store, &ctx, &rules);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let other = foreign.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let other_session = foreign.bootstrap_admin(&other).unwrap();
    let other_parent = foreign.admin_parent(&other, &other_session).unwrap();
    assert_eq!(
        issuer.revoke_service_policy(&other_parent),
        Err(AuthError::Denied)
    );
    let mut selected = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selected.credential().unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let before = issuer.lock().unwrap().service_generation;
    issuer.revoke_service_policy(&parent).unwrap();
    assert_eq!(issuer.lock().unwrap().service_generation, before + 1);
    assert!(matches!(
        lease.check(),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    assert!(lease.binding().is_err());
    assert!(lease.credential().is_err());
    assert!(lease.original().is_err());
    assert!(
        issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .is_err()
    );
}

#[test]
fn original_cancellation_denies_all_lease_getters_without_replacing_context() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        direct_config("https://direct.example"),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    let issuer = issuer(
        &f,
        store,
        &ctx,
        &[direct_rule(
            "https://direct.example",
            ServiceOrigin::Runtime,
            ServicePurpose::Embedding,
        )],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.credential().unwrap();
    assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
    cancel.cancel();
    assert!(matches!(
        lease.check(),
        Err(ServiceUseError::Profile(ProfileError::Cancelled))
    ));
    assert!(lease.binding().is_err());
    assert!(lease.credential().is_err());
    assert!(lease.original().is_err());
}

pub(super) fn native_store(f: &Fixture, ctx: &RequestContext<'_>) -> Arc<AuthStore> {
    f.auth_store_policy(
        ctx,
        crate::profile::ProfileLimits {
            bytes: 4096,
            nodes: 4096,
            scalar_bytes: 2048,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
    )
}

#[tokio::test]
async fn native_access_uses_real_signature_userinfo_zero_and_ignores_preserved_id() {
    let server = Server::new().await;
    let raw = server.token(true, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    put(
        &store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    let revision = store
        .load(&ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision();
    let verifier = server.verifier(false, revision);
    let issuer = issuer(
        &f,
        store,
        &ctx,
        &[broker_rule(
            &server.origin,
            ServiceOrigin::Runtime,
            CredentialSource::NativeAccess,
        )],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.native_credential(&verifier).await.unwrap();
    assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
    let user = lease.user_subject().unwrap().unwrap();
    assert_eq!(user.issuer().unwrap(), server.origin);
    assert_eq!(user.subject().unwrap(), USER_SUB);
    assert_eq!(user.origin().unwrap(), crate::VerifiedUserOrigin::Native);
    lease.check().unwrap();
    server.post(lease.credential().unwrap()).await;
    lease.check().unwrap();
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    assert_eq!(server.count("/service"), 1);
}

#[tokio::test]
async fn native_hub_s3_disabled_rejects_storage_purposes_before_network() {
    let server = Server::new().await;
    let raw = server.token(true, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    // The ordinary Native profile helper deliberately records s3_disabled=true.
    put(
        &store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    let revision = store
        .load(&ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision();
    let verifier = server.verifier(false, revision);
    let rules = [ServicePurpose::HubRead, ServicePurpose::HubWrite].map(|purpose| {
        ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::Broker {
                provider: "service",
                purpose,
                endpoint: &server.origin,
                resource: None,
            },
            credential: CredentialSource::NativeAccess,
            incoming: None,
        }
    });
    let issuer = issuer(&f, store, &ctx, &rules);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    for purpose in [ServicePurpose::HubRead, ServicePurpose::HubWrite] {
        let mut selection = match purpose {
            ServicePurpose::HubRead => issuer.capture_hub_service(&caller, "service"),
            ServicePurpose::HubWrite => issuer.capture_hub_write_service(&caller, "service"),
            _ => unreachable!(),
        }
        .unwrap();
        assert!(matches!(
            selection.native_credential(&verifier).await,
            Err(ServiceUseError::Disabled)
        ));
    }
    assert_eq!(server.counts.lock().unwrap().values().sum::<usize>(), 0);
}

#[tokio::test]
async fn native_needs_refresh_is_typed_and_wrong_purpose_is_not_refresh() {
    for case in 0..3 {
        let incoming = case == 2;
        let near = case != 2;
        let server = Server::new().await;
        let raw = server.token(
            !incoming,
            now_seconds() + if case == 1 { 300 } else { 3600 },
        );
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        put(
            &store,
            broker_config(&server.origin, true, false),
            native_profile(&server.origin, &raw, now_seconds() + 3600),
            true,
            &ctx,
        );
        let revision = store
            .load(&ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision();
        let verifier = server.verifier(false, revision);
        let issuer = issuer_config(
            &f,
            store,
            &ctx,
            &[broker_rule(
                &server.origin,
                ServiceOrigin::Runtime,
                CredentialSource::NativeAccess,
            )],
            &all_permissions(),
            service_limits(),
            if near {
                Duration::from_secs(if case == 0 { 7200 } else { 600 })
            } else {
                Duration::ZERO
            },
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        if near {
            assert!(matches!(
                selection.native_credential(&verifier).await,
                Err(ServiceUseError::NeedsRefresh)
            ));
        } else {
            assert!(matches!(
                selection.native_credential(&verifier).await,
                Err(ServiceUseError::Broker(
                    crate::broker::BrokerError::InvalidToken
                ))
            ));
        }
        assert_eq!(server.count("/service"), 0);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}

#[tokio::test]
async fn native_profile_microsecond_expiry_requests_refresh_before_jwt_io() {
    for remaining in [None, Some(300)] {
        let server = Server::new().await;
        let now = now_seconds();
        let raw = server.token(true, now + 3600);
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        let expiration = remaining.map_or(now.saturating_sub(1), |seconds| now + seconds);
        put(
            &store,
            broker_config(&server.origin, true, false),
            native_profile(&server.origin, &raw, expiration),
            true,
            &ctx,
        );
        let revision = store
            .load(&ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision();
        let verifier = server.verifier(false, revision);
        let issuer = issuer_config(
            &f,
            Arc::clone(&store),
            &ctx,
            &[broker_rule(
                &server.origin,
                ServiceOrigin::Runtime,
                CredentialSource::NativeAccess,
            )],
            &all_permissions(),
            service_limits(),
            Duration::from_secs(600),
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let observed = selection.native_profile_revision().unwrap();
        assert_eq!(observed.provider, "service");
        assert_eq!(observed.profile, "native");
        assert_eq!(observed.issuer, server.origin);
        assert_eq!(observed.subject, USER_SUB);
        assert_eq!(observed.revisions.provider_revision, revision);
        let current = store.load(&ctx).unwrap();
        let profile = current.document().profile("native").unwrap();
        assert_eq!(
            observed.revisions.document_revision,
            current.document().revision()
        );
        assert_eq!(observed.revisions.profile_revision, profile.revision());
        assert_eq!(observed.revisions.token_revision, profile.token_revision());
        assert!(matches!(
            selection.native_credential(&verifier).await,
            Err(ServiceUseError::NeedsRefresh)
        ));
        assert_eq!(server.counts.lock().unwrap().values().sum::<usize>(), 0);
        cancel.cancel();
        assert!(selection.native_profile_revision().is_err());
    }
}

#[tokio::test]
async fn native_await_rechecks_policy_physical_observation_and_cancellation() {
    for change in 0..3 {
        let server = Server::new().await;
        server.pause_keys();
        let raw = server.token(true, now_seconds() + 3600);
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        put(
            &store,
            broker_config(&server.origin, true, false),
            native_profile(&server.origin, &raw, now_seconds() + 3600),
            true,
            &ctx,
        );
        let revision = store
            .load(&ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision();
        let verifier = server.verifier(false, revision);
        let issuer = issuer(
            &f,
            Arc::clone(&store),
            &ctx,
            &[broker_rule(
                &server.origin,
                ServiceOrigin::Runtime,
                CredentialSource::NativeAccess,
            )],
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let admit = selection.native_credential(&verifier);
        let alter = async {
            server.keys_entered().await;
            match change {
                0 => issuer.revoke_service_policy(&parent).unwrap(),
                1 => {
                    let mutation = context(&cancel);
                    put(
                        &store,
                        broker_config(&server.origin, true, false),
                        native_profile(&server.origin, &raw, now_seconds() + 7200),
                        true,
                        &mutation,
                    );
                }
                _ => cancel.cancel(),
            }
            server.release_keys();
        };
        let (result, ()) = tokio::join!(admit, alter);
        assert!(result.is_err());
        assert_eq!(server.count("/service"), 0);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}

#[tokio::test]
async fn incoming_uses_server_policy_and_same_bearer_not_daemon_profile_identity() {
    let server = Server::new().await;
    let raw = server.token(false, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    let mut daemon = native_profile(
        &server.origin,
        "different-daemon-access",
        now_seconds() + 3600,
    );
    if let ProfileCredentials::Broker { subject, .. } = &mut daemon.credentials {
        *subject = "daemon-b".into();
    }
    put(
        &store,
        broker_config(&server.origin, true, false),
        daemon,
        false,
        &ctx,
    );
    let revision = store
        .load(&ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision();
    let verifier = server.verifier(true, revision);
    let caller = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let issuer = issuer(
        &f,
        store,
        &ctx,
        &[broker_rule(
            &server.origin,
            ServiceOrigin::BrokerIncoming,
            CredentialSource::IncomingBearer,
        )],
    );
    let mut selected = issuer
        .capture_incoming_service(&caller, &verifier, &ctx, ServicePurpose::Embedding)
        .unwrap();
    let lease = selected.credential().unwrap();
    assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
    assert!(matches!(
        lease.binding().unwrap(),
        ServiceBindingView::Broker { resource: None, .. }
    ));
    let user = lease.user_subject().unwrap().unwrap();
    assert_eq!(user.subject().unwrap(), USER_SUB);
    assert_eq!(user.origin().unwrap(), crate::VerifiedUserOrigin::Incoming);
    lease.check().unwrap();
    server.post(lease.credential().unwrap()).await;
    lease.check().unwrap();
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    assert_eq!(server.count("/service"), 1);
    // A new incoming request performs fresh UserInfo even with cached metadata.
    let fresh = context(&cancel);
    verifier
        .verify(&raw, BrokerRequestContext::new(fresh.snapshot()))
        .await
        .unwrap();
    assert_eq!(server.count("/userinfo"), 0);
}

#[tokio::test]
async fn incoming_identity_without_exact_server_remote_rule_never_admits_service() {
    let server = Server::new().await;
    let other_issuer = Server::new().await;
    let raw = server.token(false, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    put(
        &store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, "daemon-access", now_seconds() + 3600),
        false,
        &ctx,
    );
    let revision = store
        .load(&ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision();
    let verifier = server.verifier(true, revision);
    let caller = verifier
        .verify(&raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    for mismatch in 0..5 {
        let mut rule = broker_rule(
            &server.origin,
            ServiceOrigin::BrokerIncoming,
            CredentialSource::IncomingBearer,
        );
        if let ServiceRuleInput::Remote {
            binding:
                RemoteServiceBindingInput::Broker {
                    ref mut endpoint,
                    ref mut resource,
                    ..
                },
            incoming: Some(ref mut principal),
            ..
        } = rule
        {
            match mismatch {
                1 => principal.subject = SubjectSelector::Exact("other-subject"),
                2 => principal.incoming_resource = "https://wrong.example/mcp",
                3 => {
                    principal.issuer = &other_issuer.origin;
                    *endpoint = &other_issuer.origin;
                }
                4 => *resource = Some("https://outbound-resource.example/broker"),
                _ => (),
            }
        }
        let rules = [rule];
        let chosen = if mismatch == 0 { &[][..] } else { &rules[..] };
        let issuer = issuer(&f, Arc::clone(&store), &ctx, chosen);
        assert!(
            issuer
                .capture_incoming_service(&caller, &verifier, &ctx, ServicePurpose::Embedding)
                .is_err()
        );
    }
    let allowed = issuer(
        &f,
        store,
        &ctx,
        &[broker_rule(
            &server.origin,
            ServiceOrigin::BrokerIncoming,
            CredentialSource::IncomingBearer,
        )],
    );
    let fresh = context(&cancel);
    assert!(
        allowed
            .capture_incoming_service(&caller, &verifier, &fresh, ServicePurpose::Embedding)
            .is_err()
    );
    assert_eq!(server.count("/service"), 0);
    assert_eq!(server.count("/token"), 0);
    assert_eq!(other_issuer.count("/service"), 0);
    assert_eq!(other_issuer.count("/token"), 0);
}

#[test]
fn role_revoke_run_close_expiry_and_policy_revocation_invalidate_the_same_lease() {
    for change in 0..4 {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = f.auth_store(&ctx);
        put(
            &store,
            direct_config("https://direct.example"),
            local_profile("a", MCP_A, None, false),
            true,
            &ctx,
        );
        let rules = [direct_rule(
            "https://direct.example",
            ServiceOrigin::Runtime,
            ServicePurpose::Embedding,
        )];
        let clock = clock();
        let issuer = RoleIssuer::with_sources(
            f.realm(),
            Arc::clone(&f.runtime),
            store,
            policy(
                &rules,
                &all_permissions(),
                limits(),
                service_limits(),
                Duration::ZERO,
            )
            .unwrap(),
            ttl(),
            &ctx,
            clock.clone(),
            Arc::new(OsEntropy),
        )
        .unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let permissions = [
            Permission::External(ExternalEffect::RemoteRequest),
            Permission::ManageAccessGrants,
        ];
        let definition = f.definition(Role::Coordinator, &permissions, 1, limits());
        let reference = definition.reference().clone();
        issuer.register_definition(&parent, definition).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(&parent, &run, &reference, &permissions, &f.scope())
            .unwrap();
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let mut selected = issuer
            .capture_role_service(&grant, ServicePurpose::Embedding)
            .unwrap();
        let lease = selected.credential().unwrap();
        assert_eq!(lease.credential().unwrap(), Some(EMBEDDING_KEY));
        match change {
            0 => issuer.revoke(&parent, &bearer).unwrap(),
            1 => issuer.close_run(&parent, &run).unwrap(),
            2 => clock.seconds.store(900, Ordering::SeqCst),
            _ => issuer.revoke_service_policy_with_role(&grant).unwrap(),
        }
        assert!(lease.check().is_err());
        assert!(lease.binding().is_err());
        assert!(lease.credential().is_err());
        assert!(lease.original().is_err());
        assert!(lease.user_subject().is_err());
        assert!(
            issuer
                .capture_role_service(&grant, ServicePurpose::Embedding)
                .is_err()
        );
    }
}

#[test]
fn role_without_remote_or_management_permission_cannot_admit_or_revoke_policy() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        direct_config("https://direct.example"),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    let issuer = issuer(
        &f,
        store,
        &ctx,
        &[direct_rule(
            "https://direct.example",
            ServiceOrigin::Runtime,
            ServicePurpose::Embedding,
        )],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let permission = Permission::Read(graphit_core::module_settings::Module::Memory);
    let definition = f.definition(Role::Scout, &[permission], 1, limits());
    let reference = definition.reference().clone();
    issuer.register_definition(&parent, definition).unwrap();
    let run = issuer
        .start_run(&parent, &f.scope(), Duration::from_secs(3600))
        .unwrap();
    let bearer = issuer
        .issue(&parent, &run, &reference, &[permission], &f.scope())
        .unwrap();
    let grant = issuer
        .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
        .unwrap();
    assert!(matches!(
        issuer.capture_role_service(&grant, ServicePurpose::Embedding),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    let before = issuer.lock().unwrap().service_generation;
    assert_eq!(
        issuer.revoke_service_policy_with_role(&grant),
        Err(AuthError::Denied)
    );
    assert_eq!(issuer.lock().unwrap().service_generation, before);
    let mut allowed = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    assert_eq!(
        allowed.credential().unwrap().credential().unwrap(),
        Some(EMBEDDING_KEY)
    );
}

#[tokio::test]
async fn incoming_new_request_retains_valid_jwt_identity_without_userinfo() {
    let server = Server::new().await;
    let raw = server.token(false, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let original = context(&cancel);
    let verifier = server.verifier(true, 1);
    let caller = verifier
        .verify(&raw, BrokerRequestContext::new(original.snapshot()))
        .await
        .unwrap();
    server.userinfo_status.store(401, Ordering::SeqCst);
    let fresh = context(&cancel);
    let fresh_caller = verifier
        .verify(&raw, BrokerRequestContext::new(fresh.snapshot()))
        .await
        .unwrap();
    fresh_caller
        .check(&verifier, BrokerRequestContext::new(fresh.snapshot()))
        .unwrap();
    assert_eq!(fresh_caller.issuer(), caller.issuer());
    assert_eq!(fresh_caller.subject(), caller.subject());
    caller
        .check(&verifier, BrokerRequestContext::new(original.snapshot()))
        .unwrap();
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/service"), 0);
}

#[test]
fn rule_count_exact_next_and_startup_without_local_profile_preserve_data_limits() {
    let one = [local_rule(ServiceOrigin::Runtime)];
    let two = [one[0], local_rule(ServiceOrigin::Local)];
    let mut admission = service_limits();
    admission.max_rules = 1;
    assert!(
        policy(
            &one,
            &all_permissions(),
            limits(),
            admission,
            Duration::ZERO
        )
        .is_ok()
    );
    assert!(matches!(
        policy(
            &two,
            &all_permissions(),
            limits(),
            admission,
            Duration::ZERO
        ),
        Err(AuthError::Limit)
    ));
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);

    let exact_policy = policy(
        &one,
        &all_permissions(),
        limits(),
        admission,
        Duration::ZERO,
    )
    .unwrap();
    assert!(
        RoleIssuer::new(
            f.realm(),
            Arc::clone(&f.runtime),
            store,
            exact_policy,
            ttl(),
            &ctx
        )
        .is_ok()
    );
    assert!(!f.installation.owner_root().join("auth.json").exists());
}

#[cfg(unix)]
#[test]
fn physical_private_permission_change_denies_all_old_outputs() {
    use std::os::unix::fs::PermissionsExt;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        direct_config("https://direct.example"),
        local_profile("a", MCP_A, None, false),
        true,
        &ctx,
    );
    let issuer = issuer(
        &f,
        store,
        &ctx,
        &[direct_rule(
            "https://direct.example",
            ServiceOrigin::Runtime,
            ServicePurpose::Embedding,
        )],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let mut selected = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selected.credential().unwrap();
    assert_eq!(lease.credential().unwrap(), Some(EMBEDDING_KEY));
    std::fs::set_permissions(
        f.installation.owner_root().join("auth.json"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(lease.check().is_err());
    assert!(lease.binding().is_err());
    assert!(lease.credential().is_err());
    assert!(lease.original().is_err());
}

#[test]
fn local_service_caller_cancellation_during_physical_recheck_stays_cancelled() {
    assert_local_service_caller_interruption(ProfileError::Cancelled);
}

#[test]
fn local_service_caller_expiry_during_physical_recheck_stays_deadline() {
    assert_local_service_caller_interruption(ProfileError::Deadline);
}

fn assert_local_service_caller_interruption(expected: ProfileError) {
    let setup_cancel = Cancellation::default();
    let setup = context(&setup_cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(
        &store,
        local_config(),
        local_profile("a", MCP_A, None, false),
        true,
        &setup,
    );
    let issuer = issuer(&f, store, &setup, &[local_rule(ServiceOrigin::Local)]);
    let path = f.installation.owner_root().join("auth.json");
    let before = Zeroizing::new(std::fs::read(&path).unwrap());
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(1);
    let original = request_before(&cancel, deadline);
    let caller = issuer.authenticate_local(MCP_A, &original).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let captured = Arc::clone(lease.original().unwrap().snapshot());
    assert!(captured.same_request(original.snapshot()));
    assert!(captured.deadline() == Some(deadline));
    let signal = cancel.clone();
    source::BEFORE_LOCAL_OBSERVATION_LOCK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            if expected == ProfileError::Deadline {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            } else {
                signal.cancel();
            }
        }));
    });
    // The existing hook interrupts the actual physical caller check after the
    // service's first Original check, rather than fabricating a service error.
    let checked = lease.check();
    assert!(
        matches!(checked, Err(ServiceUseError::Profile(error)) if error == expected),
        "physical caller interruption must preserve {expected:?}, got {checked:?}"
    );
    for observed in [
        lease.binding().map(|_| ()),
        lease.execution().map(|_| ()),
        lease.original().map(|_| ()),
    ] {
        assert!(matches!(observed, Err(ServiceUseError::Profile(error)) if error == expected));
    }
    assert!(captured.same_request(original.snapshot()));
    assert!(captured.deadline() == Some(deadline));
    assert!(std::fs::read(&path).unwrap() == before.as_slice());
    let live_caller = issuer.authenticate_local(MCP_A, &setup).unwrap();
    let mut live_selection = issuer
        .capture_service(&live_caller, ServicePurpose::Embedding)
        .unwrap();
    live_selection.local().unwrap().check().unwrap();
}

#[test]
fn concurrent_service_calls_wait_and_preserve_original_and_profile() {
    let setup_cancel = Cancellation::default();
    let setup = context(&setup_cancel);
    let f = Fixture::new(&setup);
    let store = f.auth_store(&setup);
    put(
        &store,
        local_config(),
        local_profile("a", MCP_A, None, false),
        true,
        &setup,
    );
    let issuer = issuer(&f, store, &setup, &[local_rule(ServiceOrigin::Runtime)]);
    let caller = issuer.authenticate_runtime(&f.secret(), &setup).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.local().unwrap();
    let barrier = std::sync::Barrier::new(16);
    std::thread::scope(|threads| {
        let workers: Vec<_> = (0..16)
            .map(|_| {
                let (lease, barrier) = (&lease, &barrier);
                threads.spawn(move || {
                    barrier.wait();
                    lease.check()
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
    });
    let path = f.installation.owner_root().join("auth.json");
    let before = Zeroizing::new(std::fs::read(&path).unwrap());
    for expired in [false, true] {
        let cancel = Cancellation::default();
        let deadline = Instant::now()
            + if expired {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(30)
            };
        let original = request_before(&cancel, deadline);
        let caller = issuer.authenticate_runtime(&f.secret(), &original).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selection.local().unwrap();
        let guard = lease.test_observation_lock();
        std::thread::scope(|threads| {
            let (entered, waiting) = std::sync::mpsc::channel();
            let lease = &lease;
            let worker = threads.spawn(move || {
                crate::issuer::service_credentials::BEFORE_SERVICE_OBSERVATION_LOCK.with(|slot| {
                    *slot.borrow_mut() = Some(Box::new(move || entered.send(()).unwrap()));
                });
                lease.check()
            });
            waiting.recv().unwrap();
            if expired {
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            } else {
                cancel.cancel();
            }
            drop(guard);
            let expected = if expired {
                ProfileError::Deadline
            } else {
                ProfileError::Cancelled
            };
            assert!(
                matches!(worker.join().unwrap(), Err(ServiceUseError::Profile(error)) if error == expected)
            );
        });
        assert_eq!(std::fs::read(&path).unwrap(), before.as_slice());
    }
}

#[test]
fn direct_ai_options_are_captured_in_real_lease_and_provider_changes_invalidate_it() {
    for protocol in [
        DirectProtocol::OpenAi,
        DirectProtocol::GoogleGemini1,
        DirectProtocol::GoogleGemini2,
    ] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = f.auth_store(&ctx);
        let google = matches!(
            protocol,
            DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
        );
        let endpoint = if google {
            "https://ai.example/v1/models/retrieval:embedContent"
        } else {
            "https://ai.example/v1/embeddings"
        };
        let model = if google {
            "models/retrieval"
        } else {
            "text-embedding-3-small"
        };
        let width = (!google).then_some(512);
        let batch = google.then_some("https://ai.example/v1/models/retrieval:batchEmbedContents");
        let mut config = direct_config(endpoint);
        config.embedding = AiService::Direct {
            protocol,
            endpoint: endpoint.into(),
            model: model.into(),
            model_revision: "fixture-deployment-2026-10".into(),
            recipe: "fixture-vector-v1".into(),
            dimensions: 512,
            request_dimensions: width,
            batch_endpoint: batch.map(Into::into),
        };
        put(
            &store,
            config.clone(),
            local_profile("a", MCP_A, None, false),
            true,
            &ctx,
        );
        let rules = [ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::Direct {
                provider: "service",
                purpose: ServicePurpose::Embedding,
                protocol,
                endpoint,
                model,
                model_revision: "fixture-deployment-2026-10",
                recipe: "fixture-vector-v1",
                dimensions: 512,
                request_dimensions: width,
                batch_endpoint: batch,
            },
            credential: CredentialSource::DirectKey,
            incoming: None,
        }];
        let issuer = issuer(&f, Arc::clone(&store), &ctx, &rules);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selection.credential().unwrap();
        match lease.binding().unwrap() {
            ServiceBindingView::Direct {
                protocol: captured,
                endpoint: e,
                model: m,
                model_revision,
                recipe,
                dimensions,
                request_dimensions,
                batch_endpoint,
                ..
            } => {
                assert_eq!(captured, protocol);
                assert_eq!(e, endpoint);
                assert_eq!(m, model);
                assert_eq!(model_revision, "fixture-deployment-2026-10");
                assert_eq!(recipe, "fixture-vector-v1");
                assert_eq!(dimensions, 512);
                assert_eq!(request_dimensions, width);
                assert_eq!(batch_endpoint, batch);
            }
            _ => panic!("expected Direct binding"),
        }
        assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
        assert_eq!(lease.credential().unwrap(), Some(EMBEDDING_KEY));
        for change_revision in [true, false] {
            let mut identity_mismatch = rules;
            if let ServiceRuleInput::Remote {
                binding:
                    RemoteServiceBindingInput::Direct {
                        model_revision,
                        recipe,
                        ..
                    },
                ..
            } = &mut identity_mismatch[0]
            {
                if change_revision {
                    *model_revision = "another-deployment-revision";
                } else {
                    *recipe = "another-vector-recipe";
                }
            }
            let other = self::issuer(&f, Arc::clone(&store), &ctx, &identity_mismatch);
            let other_caller = other.authenticate_runtime(&f.secret(), &ctx).unwrap();
            assert!(
                other
                    .capture_service(&other_caller, ServicePurpose::Embedding)
                    .and_then(|mut selected| selected.credential().map(|_| ()))
                    .is_err()
            );
        }
        let mut mismatched = rules;
        if let ServiceRuleInput::Remote {
            binding:
                RemoteServiceBindingInput::Direct {
                    request_dimensions,
                    batch_endpoint,
                    ..
                },
            ..
        } = &mut mismatched[0]
        {
            if google {
                *batch_endpoint =
                    Some("https://ai.example/other/models/retrieval:batchEmbedContents");
            } else {
                *request_dimensions = None;
            }
        }
        let other = self::issuer(&f, Arc::clone(&store), &ctx, &mismatched);
        let other_caller = other.authenticate_runtime(&f.secret(), &ctx).unwrap();
        assert!(
            other
                .capture_service(&other_caller, ServicePurpose::Embedding)
                .and_then(|mut selected| selected.credential().map(|_| ()))
                .is_err()
        );
        if let AiService::Direct {
            request_dimensions,
            batch_endpoint,
            ..
        } = &mut config.embedding
        {
            if google {
                *batch_endpoint =
                    Some("https://ai.example/other/models/retrieval:batchEmbedContents".into());
            } else {
                *request_dimensions = None;
            }
        }
        let before = store.load(&ctx).unwrap();
        applied(
            store
                .mutate(before, &ctx, |doc, ctx| {
                    let provider = doc.provider("service").unwrap().revision();
                    doc.update_provider("service", config, doc.revision(), provider, 2, ctx)
                })
                .unwrap(),
        );
        assert!(lease.binding().is_err());
        assert!(lease.credential().is_err());
    }
}

#[test]
fn web_remote_policy_only_allows_explicit_hub_or_imported_ai_broker_bearer() {
    let read_rule = |purpose, credential| ServiceRuleInput::Remote {
        origin: ServiceOrigin::Web,
        binding: RemoteServiceBindingInput::Broker {
            provider: "service",
            purpose,
            endpoint: "https://broker.example",
            resource: Some("https://broker.example/mcp"),
        },
        credential,
        incoming: None,
    };
    let accepted = read_rule(ServicePurpose::HubRead, CredentialSource::IncomingBearer);
    assert!(policy(&[accepted], &[], limits(), service_limits(), Duration::ZERO).is_ok());
    let write = read_rule(ServicePurpose::HubWrite, CredentialSource::IncomingBearer);
    assert!(policy(&[write], &[], limits(), service_limits(), Duration::ZERO).is_ok());
    assert!(matches!(
        policy(
            &[write, write],
            &[],
            limits(),
            service_limits(),
            Duration::ZERO
        ),
        Err(AuthError::Invalid)
    ));
    for purpose in [ServicePurpose::Embedding, ServicePurpose::Rerank] {
        assert!(
            policy(
                &[read_rule(purpose, CredentialSource::IncomingBearer)],
                &[],
                limits(),
                service_limits(),
                Duration::ZERO
            )
            .is_ok()
        );
    }
    for purpose in [
        ServicePurpose::PresenceRead,
        ServicePurpose::PresencePublish,
        ServicePurpose::InventoryRead,
        ServicePurpose::InventoryPublish,
    ] {
        assert!(matches!(
            policy(
                &[read_rule(purpose, CredentialSource::IncomingBearer)],
                &all_permissions(),
                limits(),
                service_limits(),
                Duration::ZERO
            ),
            Err(AuthError::Invalid)
        ));
    }
    for purpose in [
        ServicePurpose::HubRead,
        ServicePurpose::HubWrite,
        ServicePurpose::Embedding,
        ServicePurpose::Rerank,
    ] {
        for credential in [
            CredentialSource::NativeAccess,
            CredentialSource::LocalBrokerKey,
            CredentialSource::Anonymous,
            CredentialSource::DirectKey,
        ] {
            assert!(matches!(
                policy(
                    &[read_rule(purpose, credential)],
                    &all_permissions(),
                    limits(),
                    service_limits(),
                    Duration::ZERO
                ),
                Err(AuthError::Invalid)
            ));
        }
    }
    assert!(matches!(
        policy(
            &[accepted, accepted],
            &[],
            limits(),
            service_limits(),
            Duration::ZERO
        ),
        Err(AuthError::Invalid)
    ));
}

#[tokio::test]
async fn device_service_uses_access_until_expiry_and_then_requires_login() {
    use crate::profile::{BrokerDeviceMetadata, BrokerDeviceProfileData, BrokerDeviceTokens};
    for variant in 0..3 {
        let expired = variant == 1;
        let server = Server::new().await;
        let raw = server.token(true, now_seconds() + if variant == 2 { 2 } else { 300 });
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        let publication = store
            .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                doc.add_provider(
                    "service".into(),
                    broker_config(&server.origin, true, false),
                    doc.revision(),
                    1,
                    ctx,
                )?;
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("service").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_broker_device_profile(
                    BrokerDeviceProfileData {
                        name: "native".into(),
                        provider: "service".into(),
                        issuer: server.origin.clone(),
                        subject: USER_SUB.into(),
                        tokens: BrokerDeviceTokens {
                            access_token: secret(&raw),
                            expires_at: (now_seconds() + if expired { 0 } else { 300 })
                                .try_into()
                                .unwrap(),
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
        applied(publication);
        let revision = store
            .load(&ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision();
        let verifier = server.verifier(false, revision);
        let issuer = issuer_config(
            &f,
            store,
            &ctx,
            &[broker_rule(
                &server.origin,
                ServiceOrigin::Runtime,
                CredentialSource::NativeAccess,
            )],
            &all_permissions(),
            service_limits(),
            Duration::from_secs(600),
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        if expired {
            assert!(matches!(
                selected.native_credential(&verifier).await,
                Err(ServiceUseError::LoginRequired)
            ));
        } else {
            let lease = selected.native_credential(&verifier).await.unwrap();
            lease.check().unwrap();
            server.post(lease.credential().unwrap()).await;
            assert_eq!(
                lease.user_subject().unwrap().unwrap().subject().unwrap(),
                USER_SUB
            );
            if variant == 2 {
                tokio::time::sleep(Duration::from_millis(2100)).await;
                assert!(matches!(lease.check(), Err(ServiceUseError::LoginRequired)));
                assert!(matches!(
                    lease.credential(),
                    Err(ServiceUseError::LoginRequired)
                ));
            }
        }
        assert_eq!(server.count("/token"), 0);
        assert_eq!(server.count("/userinfo"), 0);
    }
}
