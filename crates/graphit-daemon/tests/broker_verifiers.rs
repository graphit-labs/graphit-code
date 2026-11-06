use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::{
    AdminSessionPolicy, CredentialSource, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits,
    Permission, RemoteServiceBindingInput, ServiceAdmissionLimits, ServiceAuthorityInput,
    ServiceOrigin, ServicePurpose, ServiceRuleInput, ServiceUseError, SourceAuthorityPolicy,
    TtlPolicy,
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
    broker_verifiers::{BrokerVerifierCache, BrokerVerifierCacheError},
};
use graphit_http::client::Limits as NetworkLimits;
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use secrecy::SecretString;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};

const RESOURCE: &str = "https://mcp.example/graphit";
fn request(cancel: &Cancellation, duration: Duration) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        },
        cancel,
        Instant::now() + duration,
    )
    .unwrap()
}
fn scope_limits() -> ScopeLimits {
    ScopeLimits::new(16, 4096, 128).unwrap()
}
fn documents() -> AuthDocumentLimits {
    AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap()
}
fn profiles() -> ProfileLimits {
    ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn network() -> NetworkLimits {
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
    }
}
fn cache(http: bool) -> BrokerVerifierCache {
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
        http,
        None,
        &[],
    )
}

struct Fixture {
    auth: DaemonAuth,
    store: AuthStore,
    native: bool,
    _directory: tempfile::TempDir,
}
impl Fixture {
    fn new(endpoints: &[&str], allowed: bool, ctx: &RequestContext<'_>) -> Self {
        Self::new_mode(endpoints, allowed, true, ctx)
    }
    fn new_mode(endpoints: &[&str], allowed: bool, native: bool, ctx: &RequestContext<'_>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let mut rules = Vec::new();
        for provider in ["a", "b"] {
            for endpoint in endpoints {
                rules.push(ServiceRuleInput::Remote {
                    origin: ServiceOrigin::Runtime,
                    binding: RemoteServiceBindingInput::BrokerInventory {
                        provider,
                        purpose: ServicePurpose::InventoryRead,
                        endpoint,
                    },
                    credential: if native {
                        CredentialSource::NativeAccess
                    } else {
                        CredentialSource::LocalBrokerKey
                    },
                    incoming: None,
                });
                rules.push(ServiceRuleInput::Remote {
                    origin: ServiceOrigin::Runtime,
                    binding: RemoteServiceBindingInput::Broker {
                        provider,
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
                });
            }
        }
        let permissions = if allowed {
            vec![Permission::External(ExternalEffect::RemoteRequest)]
        } else {
            Vec::new()
        };
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
            scope: scope_limits(),
            document: documents(),
            profiles: profiles(),
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
        };
        let auth = DaemonAuth::start(&paths, &[], policy, ctx).unwrap();
        let id = InstallationIdentityStore::new(&paths)
            .unwrap()
            .load()
            .unwrap()
            .unit_id()
            .unwrap()
            .clone();
        let scope = CapturedPublicationScope::installation(
            &paths,
            id,
            auth.installation()
                .live
                .allowed_roots()
                .map(ToOwned::to_owned),
            scope_limits(),
            ctx.snapshot(),
        )
        .unwrap();
        let store = AuthStore::new(
            AuthDocumentStore::capture(scope, documents(), ctx.snapshot()).unwrap(),
            profiles(),
            ctx,
        )
        .unwrap();
        Self {
            auth,
            store,
            native,
            _directory: directory,
        }
    }
    fn key(&self, ctx: &RequestContext<'_>) -> String {
        let checked = self.auth.runtime().check(ctx.snapshot()).unwrap();
        String::from_utf8(checked.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
    }
    fn publish(
        &self,
        provider: &str,
        endpoint: &str,
        resource: Option<&str>,
        token: &str,
        ctx: &RequestContext<'_>,
    ) {
        let before = self.store.load(ctx).unwrap();
        let result = self
            .store
            .mutate(before, ctx, |doc, ctx| {
                let service = BrokerService {
                    endpoint: endpoint.into(),
                    mcp_resource: resource.map(str::to_owned),
                    allow_anonymous: false,
                };
                let config = ProviderConfig {
                    kind: if self.native {
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
                        source: if self.native {
                            S3Source::BrokerScope
                        } else {
                            S3Source::LocalCredentials
                        },
                    },
                };
                if let Some(old) = doc.provider(provider) {
                    let (revision, now) = (old.revision(), old.updated_at() + 1);
                    doc.update_provider(provider, config, doc.revision(), revision, now, ctx)?;
                } else {
                    doc.add_provider(provider.into(), config, doc.revision(), 1, ctx)?;
                }
                let old = doc.profile(provider);
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider(provider).unwrap().revision(),
                    profile_revision: old.map(|p| p.revision()),
                };
                let now = old.map_or(1, |p| p.updated_at() + 1);
                let data = ProfileData {
                    name: provider.into(),
                    provider: provider.into(),
                    username: "fixture-display".into(),
                    organization: None,
                    teams: vec![],
                    credentials: if self.native {
                        ProfileCredentials::Broker {
                            issuer: endpoint.into(),
                            subject: "fixture-sub".into(),
                            s3_disabled: true,
                            tokens: OidcTokens {
                                access_token: Secret::new(SecretString::from(token)),
                                refresh_token: None,
                                id_token: Secret::new(SecretString::from("historical")),
                                expires_at: Duration::from_secs(now_seconds() + 3600)
                                    .as_micros()
                                    .try_into()
                                    .unwrap(),
                            },
                        }
                    } else {
                        ProfileCredentials::Local(LocalCredentials {
                            anonymous: false,
                            mcp_key: Some(Secret::new(SecretString::from("local-fixture"))),
                            broker_key: Some(Secret::new(SecretString::from(token))),
                            embedding_api_key: None,
                            rerank_api_key: None,
                            s3: None,
                        })
                    },
                };
                doc.replace_profile_data(data, expected, now, ctx)?;
                doc.activate(provider, doc.revision(), ctx)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(result.effect(), AuthPublicationEffect::Applied);
        assert!(result.failure().is_none());
    }
}
fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

struct Server {
    origin: String,
    key: Ed25519KeyPair,
    counts: Arc<Mutex<BTreeMap<String, usize>>>,
    task: JoinHandle<()>,
}
impl Server {
    async fn new() -> Self {
        let key = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let metadata = json!({"version":"1","issuer":origin,"authentication":{
            "type":"oidc","issuer":origin,"client_id":"graphit-cli", "scopes":["openid","profile"],
            "redirect_uri_path":"/callback", "audiences":["broker-api"],
            "access_token_audience":"broker-api","mcp_resources":[RESOURCE]}})
        .to_string();
        let discovery = json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),
            "token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),
            "userinfo_endpoint":format!("{origin}/userinfo"),
            "grant_types_supported":["authorization_code","refresh_token"],
            "code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],
            "id_token_signing_alg_values_supported":["EdDSA"]}).to_string();
        let keys = json!({"keys":[{"kty":"OKP","crv":"Ed25519",
            "x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),"kid":"actual","alg":"EdDSA",
            "use":"sig","key_ops":["verify"]}]})
        .to_string();
        let counts = Arc::new(Mutex::new(BTreeMap::new()));
        let counted = Arc::clone(&counts);
        let task = tokio::spawn(async move {
            let mut sockets = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((mut socket, _)) = accepted else { break };
                        let counts = Arc::clone(&counted);
                        let metadata = metadata.clone(); let discovery = discovery.clone();
                        let keys = keys.clone();
                        sockets.spawn(async move {
                            let mut input = Vec::new(); let mut buffer = [0; 1024];
                            while !input.windows(4).any(|b| b == b"\r\n\r\n") {
                                let Ok(n) = socket.read(&mut buffer).await else { return };
                                if n == 0 || input.len() + n > 16384 { return }
                                input.extend_from_slice(&buffer[..n]);
                            }
                            let request = std::str::from_utf8(&input).unwrap();
                            let path = request.split_whitespace().nth(1).unwrap();
                            *counts.lock().unwrap().entry(path.to_owned()).or_default() += 1;
                            let (status, body) = match path {
                                "/.well-known/graphit-broker" => (200, metadata),
                                "/.well-known/openid-configuration" => (200, discovery),
                                "/keys" => (200, keys),
                                _ => (503, "{}".into()),
                            };
                            let response = format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len());
                            let _ = socket.write_all(response.as_bytes()).await;
                        });
                    }
                    _ = sockets.join_next(), if !sockets.is_empty() => {}
                }
            }
        });
        Self {
            origin,
            key,
            counts,
            task,
        }
    }
    fn token(&self, resource: bool) -> String {
        let header =
            URL_SAFE_NO_PAD.encode(json!({"alg":"EdDSA","kid":"actual","typ":"JWT"}).to_string());
        let claims = json!({"iss":self.origin,"sub":"fixture-sub", "aud":if resource {
            json!(["broker-api",RESOURCE]) } else { json!(["broker-api"]) },
            "client_id":"graphit-cli","jti":"native-cache-test","token_use":"access",
            "iat":now_seconds().saturating_sub(1),"exp":now_seconds()+3600});
        let message = format!("{header}.{}", URL_SAFE_NO_PAD.encode(claims.to_string()));
        format!(
            "{message}.{}",
            URL_SAFE_NO_PAD.encode(self.key.sign(message.as_bytes()).as_ref())
        )
    }
    fn count(&self, path: &str) -> usize {
        self.counts.lock().unwrap().get(path).copied().unwrap_or(0)
    }
    async fn shutdown(self) {
        self.task.abort();
        assert!(self.task.await.unwrap_err().is_cancelled());
    }
}

#[tokio::test]
async fn presence_and_inventory_share_real_native_verifier_and_warm_keys() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Duration::from_secs(30));
    let f = Fixture::new(&[&server.origin], true, &ctx);
    let raw = server.token(false);
    f.publish("a", &server.origin, None, &raw, &ctx);
    let key = f.key(&ctx);
    let caller = f.auth.issuer().authenticate_runtime(&key, &ctx).unwrap();
    let mut presence = f
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "a")
        .unwrap();
    let mut inventory = f
        .auth
        .issuer()
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    let cache = cache(true);
    let a = cache.get(&mut presence).unwrap();
    let b = cache.get(&mut inventory).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    let (a_lease, b_lease) = tokio::join!(
        presence.native_credential(&a),
        inventory.native_credential(&b)
    );
    assert_eq!(a_lease.unwrap().credential().unwrap(), Some(raw.as_str()));
    assert_eq!(b_lease.unwrap().credential().unwrap(), Some(raw.as_str()));
    drop(presence);
    drop(inventory);
    let mut next = f
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "a")
        .unwrap();
    let warm = cache.get(&mut next).unwrap();
    assert!(Arc::ptr_eq(&a, &warm));
    assert!(next.native_credential(&warm).await.is_ok());
    assert_eq!(server.count("/.well-known/graphit-broker"), 1);
    assert_eq!(server.count("/.well-known/openid-configuration"), 1);
    assert_eq!(server.count("/keys"), 1);
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn provider_revision_and_resource_replace_and_foreign_endpoint_is_denied() {
    let server = Server::new().await;
    let other = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Duration::from_secs(30));
    let f = Fixture::new(&[&server.origin], true, &ctx);
    let cache = cache(true);
    f.publish("a", &server.origin, None, &server.token(false), &ctx);
    let key = f.key(&ctx);
    let caller = f.auth.issuer().authenticate_runtime(&key, &ctx).unwrap();
    let mut stale = f
        .auth
        .issuer()
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    let mut old = cache.get(&mut stale).unwrap();
    for (endpoint, resource, raw) in [
        (&server.origin, None, server.token(false)),
        (&server.origin, Some(RESOURCE), server.token(true)),
    ] {
        f.publish("a", endpoint, resource, &raw, &ctx);
        assert!(cache.get(&mut stale).is_err());
        let mut current = f
            .auth
            .issuer()
            .capture_service(&caller, ServicePurpose::InventoryRead)
            .unwrap();
        assert!(current.native_credential(&old).await.is_err());
        let new = cache.get(&mut current).unwrap();
        assert!(!Arc::ptr_eq(&old, &new));
        assert!(current.native_credential(&new).await.is_ok());
        let weak = Arc::downgrade(&old);
        drop(old);
        assert!(weak.upgrade().is_none());
        old = new;
    }
    f.publish("a", &other.origin, Some(RESOURCE), &other.token(true), &ctx);
    assert!(cache.get(&mut stale).is_err());
    assert!(
        f.auth
            .issuer()
            .capture_service(&caller, ServicePurpose::InventoryRead)
            .is_err()
    );
    f.publish(
        "b",
        &server.origin,
        Some(RESOURCE),
        &server.token(true),
        &ctx,
    );
    let mut b = f
        .auth
        .issuer()
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    let other_provider = cache.get(&mut b).unwrap();
    assert!(!Arc::ptr_eq(&old, &other_provider));
    assert!(b.native_credential(&other_provider).await.is_ok());
    server.shutdown().await;
    other.shutdown().await;
}

#[test]
fn current_authority_original_and_explicit_network_policy_are_checked_before_use() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Duration::from_secs(30));
    let endpoint = "http://127.0.0.1:9";
    let f = Fixture::new(&[endpoint], true, &ctx);
    f.publish("a", endpoint, None, "invalid", &ctx);
    let key = f.key(&ctx);
    let caller = f.auth.issuer().authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = f
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "a")
        .unwrap();
    assert!(matches!(
        cache(false).get(&mut selection),
        Err(BrokerVerifierCacheError::Broker(_))
    ));
    let cache = cache(true);
    let verifier = cache
        .get(&mut selection)
        .expect("live Original admits the verifier");
    cancel.cancel();
    assert!(matches!(
        cache.get(&mut selection),
        Err(BrokerVerifierCacheError::Service(_))
    ));

    let cancel = Cancellation::default();
    let ctx = request(&cancel, Duration::from_secs(30));
    let denied = Fixture::new(&[endpoint], false, &ctx);
    denied.publish("a", endpoint, None, "invalid", &ctx);
    let key = denied.key(&ctx);
    let caller = denied
        .auth
        .issuer()
        .authenticate_runtime(&key, &ctx)
        .unwrap();
    assert!(
        denied
            .auth
            .issuer()
            .capture_presence_read_service(&caller, "a")
            .is_err()
    );

    let key = f.key(&ctx);
    let caller = f.auth.issuer().authenticate_runtime(&key, &ctx).unwrap();
    let mut live = f
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "a")
        .unwrap();
    assert!(Arc::ptr_eq(
        &verifier,
        &cache
            .get(&mut live)
            .expect("a fresh live Original reuses the verifier")
    ));

    // The positive cache proof is independent of the expiry budget. This
    // Original is used only to prove revalidation after its real deadline.
    let short = request(&cancel, Duration::from_millis(200));
    let short_caller = f.auth.issuer().authenticate_runtime(&key, &short).unwrap();
    let mut selection = f
        .auth
        .issuer()
        .capture_presence_read_service(&short_caller, "a")
        .unwrap();
    std::thread::sleep(
        short
            .deadline()
            .unwrap()
            .saturating_duration_since(Instant::now()),
    );
    assert_eq!(
        short.check().unwrap_err().kind(),
        std::io::ErrorKind::TimedOut
    );
    assert!(matches!(
        cache.get(&mut selection),
        Err(BrokerVerifierCacheError::Service(_))
    ));
    assert!(Arc::ptr_eq(
        &verifier,
        &cache
            .get(&mut live)
            .expect("expiry rejects only the expired Original")
    ));
}

#[test]
fn opaque_broker_selection_never_becomes_native_through_the_cache() {
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Duration::from_secs(30));
    let endpoint = "http://127.0.0.1:9";
    let f = Fixture::new_mode(&[endpoint], true, false, &ctx);
    f.publish("a", endpoint, None, "opaque-key", &ctx);
    let key = f.key(&ctx);
    let caller = f.auth.issuer().authenticate_runtime(&key, &ctx).unwrap();
    let mut selection = f
        .auth
        .issuer()
        .capture_presence_read_service(&caller, "a")
        .unwrap();
    let discovery = cache(true);
    let metadata_owner = discovery.get_for_service(&mut selection).unwrap();
    assert!(Arc::ptr_eq(
        &metadata_owner,
        &discovery.get_for_service(&mut selection).unwrap()
    ));
    assert!(matches!(
        discovery.get(&mut selection),
        Err(BrokerVerifierCacheError::Service(
            ServiceUseError::WrongMode
        ))
    ));
    assert_eq!(
        selection.credential().unwrap().credential().unwrap(),
        Some("opaque-key")
    );
}
