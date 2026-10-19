//! Temporary installation with actual Auth SDK persistence and signed Broker HTTP discovery.
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::{
    AdminSessionPolicy, ExternalEffect, IssuerAuthorityPolicy, IssuerLimits, Permission,
    ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
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
    config::ConfigSnapshot,
    installation_bootstrap::BootstrapLimits,
    project_paths::InstallationPaths,
    runtime_key::RuntimeKeyLimits,
};
use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy};
use graphit_hub::{
    policy::{HubResourcePolicy, RequestContext},
    registry::HubRegistry,
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;

pub struct Broker {
    pub origin: String,
    key: Ed25519KeyPair,
    pub counts: Arc<[AtomicUsize; 3]>,
    stop: tokio::sync::oneshot::Sender<()>,
    task: JoinHandle<()>,
}
impl Broker {
    pub async fn start() -> Self {
        Self::start_with_discovery_gate(None).await
    }
    pub async fn start_with_discovery_gate(
        discovery_gate: Option<(
            tokio::sync::oneshot::Sender<()>,
            tokio::sync::oneshot::Receiver<()>,
        )>,
    ) -> Self {
        let key = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let metadata = json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,
            "client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback",
            "audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":["https://fixture.invalid/mcp"]}});
        let oidc = json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),
            "registration_endpoint":format!("{origin}/register"),"token_endpoint":format!("{origin}/token"),
            "jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),
            "grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],
            "token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]});
        let keys = json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),
            "kid":"actual","alg":"EdDSA","use":"sig","key_ops":["verify"]}]});
        let counts = Arc::new([
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        ]);
        let discovery_gate = Arc::new(tokio::sync::Mutex::new(discovery_gate));
        let router = axum::Router::new();
        let router = [
            ("/.well-known/graphit-broker", metadata),
            ("/.well-known/openid-configuration", oidc),
            ("/keys", keys),
        ]
        .into_iter()
        .enumerate()
        .fold(router, |router, (i, (path, body))| {
            let count = Arc::clone(&counts);
            let discovery_gate = Arc::clone(&discovery_gate);
            router.route(
                path,
                axum::routing::get(move || {
                    let body = body.clone();
                    let count = Arc::clone(&count);
                    let discovery_gate = Arc::clone(&discovery_gate);
                    async move {
                        count[i].fetch_add(1, Ordering::SeqCst);
                        if i == 0 {
                            let gate = discovery_gate.lock().await.take();
                            if let Some((started, release)) = gate {
                                let _ = started.send(());
                                let _ = release.await;
                            }
                        }
                        axum::Json(body)
                    }
                }),
            )
        });
        // The fixture receives the actual guarded inventory publication used by startup.
        // Native verification remains in the production Auth/BoundedClient path.
        let inventories = Arc::new(tokio::sync::Mutex::new(std::collections::BTreeMap::<
            String,
            (serde_json::Value, i64, i64),
        >::new()));
        let router = router.route(
            "/v1/instances/{instance_id}/inventory",
            axum::routing::put(
                move |axum::extract::Path(instance_id): axum::extract::Path<String>,
                      headers: axum::http::HeaderMap,
                      axum::Json(body): axum::Json<serde_json::Value>| {
                    let inventories = Arc::clone(&inventories);
                    async move {
                        assert!(
                            headers
                                .get(axum::http::header::AUTHORIZATION)
                                .and_then(|header| header.to_str().ok())
                                .is_some_and(
                                    |header| header.starts_with("Bearer ") && header.len() > 7
                                )
                        );
                        assert_eq!(body.as_object().unwrap().len(), 1);
                        assert!(body["projects"].is_array());
                        let observed = i64::try_from(
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_micros(),
                        )
                        .unwrap();
                        let mut inventories = inventories.lock().await;
                        let (status, change, created_at, updated_at) =
                            match inventories.get_mut(&instance_id) {
                                Some((projects, created_at, updated_at)) => {
                                    let change = if *projects == body["projects"] {
                                        "unchanged"
                                    } else {
                                        *projects = body["projects"].clone();
                                        *updated_at = observed;
                                        "updated"
                                    };
                                    (axum::http::StatusCode::OK, change, *created_at, *updated_at)
                                }
                                None => {
                                    inventories.insert(
                                        instance_id.clone(),
                                        (body["projects"].clone(), observed, observed),
                                    );
                                    (
                                        axum::http::StatusCode::CREATED,
                                        "created",
                                        observed,
                                        observed,
                                    )
                                }
                            };
                        (
                            status,
                            axum::Json(json!({
                                "instance_id": instance_id,
                                "change": change,
                                "created_at": created_at,
                                "updated_at": updated_at,
                            })),
                        )
                    }
                },
            ),
        );
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        Self {
            origin,
            key,
            counts,
            stop,
            task,
        }
    }
    pub fn token(&self, expiry: u64, audience: &str) -> String {
        let claims = json!({"iss":self.origin,"sub":"native-fixture-sub","aud":[audience],"client_id":"graphit-cli",
            "jti":"actual-token","token_use":"access","iat":now().saturating_sub(1),"exp":expiry});
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
    pub async fn shutdown(self) {
        let _ = self.stop.send(());
        self.task.await.unwrap();
    }
    pub fn requests(&self) -> [usize; 3] {
        std::array::from_fn(|i| self.counts[i].load(Ordering::SeqCst))
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
pub fn scopes() -> ScopeLimits {
    ScopeLimits::new(16, 65536, 256).unwrap()
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
fn startup(native: bool) -> StartupPolicy {
    let origin = || SourceAuthorityPolicy {
        ordinary: &[Permission::Read(graphit_core::module_settings::Module::Ast)],
        administrative: &[],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    let policy = IssuerAuthorityPolicy::new(
        origin(),
        origin(),
        IssuerLimits {
            max_permissions: 64,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 4096,
        },
        ServiceAuthorityInput {
            rules: &[],
            limits: ServiceAdmissionLimits {
                max_rules: 16,
                max_binding_bytes: 8192,
            },
            native_refresh_margin: Duration::ZERO,
        },
    )
    .unwrap();
    let permissions = if native {
        vec![
            Permission::Configure,
            Permission::ManageCache,
            Permission::External(ExternalEffect::PublishedFilesystem),
        ]
    } else {
        vec![]
    };
    StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope: scopes(),
        document: documents(),
        profiles: profiles(),
        authority: policy
            .with_native_installation_permissions(&permissions)
            .unwrap(),
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    }
}
pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub paths: InstallationPaths,
    pub auth: Arc<DaemonAuth>,
    pub store: Arc<AuthStore>,
    pub policy: HubResourcePolicy,
    pub config: ConfigSnapshot,
    pub registry: HubRegistry,
}
impl Fixture {
    pub fn new(original: &RequestContext<'_>, native: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let brand = Brand::default();
        let paths = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let auth = Arc::new(DaemonAuth::start(&paths, &[], startup(native), original).unwrap());
        let scope = auth.installation();
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
            scope.live.owner()
        else {
            panic!("installation owner");
        };
        let registry = HubRegistry::global(&paths, installation_id, policy.hub()).unwrap();
        let live = CapturedPublicationScope::installation(
            &paths,
            installation_id.clone(),
            [paths.root().to_owned()],
            scopes(),
            original.snapshot(),
        )
        .unwrap();
        let store = Arc::new(
            AuthStore::new(
                AuthDocumentStore::capture(live, documents(), original.snapshot()).unwrap(),
                profiles(),
                original,
            )
            .unwrap(),
        );
        Self {
            temp,
            paths,
            auth,
            store,
            policy,
            config,
            registry,
        }
    }
    pub fn key(&self, original: &RequestContext<'_>) -> zeroize::Zeroizing<String> {
        let checked = self.auth.runtime().check(original.snapshot()).unwrap();
        zeroize::Zeroizing::new(
            String::from_utf8(checked.secret(original.snapshot()).unwrap().to_vec()).unwrap(),
        )
    }
    pub fn profile(
        &self,
        broker: &Broker,
        expiry: u64,
        audience: &str,
        local: bool,
        original: &RequestContext<'_>,
    ) {
        let binding = BrokerService {
            endpoint: broker.origin.clone(),
            mcp_resource: None,
            allow_anonymous: local,
        };
        let config = ProviderConfig {
            kind: if local {
                ProviderKind::Local {
                    allow_aws_credential_chain: false,
                    broker: Some(binding),
                }
            } else {
                ProviderKind::Broker(binding)
            },
            embedding: AiService::Disabled,
            rerank: AiService::Disabled,
            s3: S3Location {
                bucket: None,
                region: None,
                endpoint: None,
                prefix: None,
                source: if local {
                    S3Source::LocalCredentials
                } else {
                    S3Source::BrokerScope
                },
            },
        };
        let data = ProfileData {
            name: "native".into(),
            provider: "service".into(),
            username: "display-only".into(),
            organization: None,
            teams: vec![],
            credentials: if local {
                ProfileCredentials::Local(LocalCredentials {
                    anonymous: false,
                    mcp_key: Some(Secret::new("fixture-local-key".into())),
                    broker_key: None,
                    embedding_api_key: None,
                    rerank_api_key: None,
                    s3: None,
                })
            } else {
                ProfileCredentials::Broker {
                    issuer: broker.origin.clone(),
                    subject: "native-fixture-sub".into(),
                    s3_disabled: true,
                    tokens: OidcTokens {
                        access_token: Secret::new(broker.token(expiry, audience).into()),
                        refresh_token: None,
                        id_token: Secret::new("fixture-historical-id-token".into()),
                        expires_at: expiry.try_into().unwrap(),
                    },
                }
            },
        };
        let before = self.store.load(original).unwrap();
        let publication = self
            .store
            .mutate(before, original, |doc, ctx| {
                if doc.provider("service").is_none() {
                    doc.add_provider("service".into(), config, doc.revision(), 1, ctx)?;
                }
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("service").unwrap().revision(),
                    profile_revision: doc.profile("native").map(|p| p.revision()),
                };
                let updated = doc.profile("native").map_or(1, |p| p.updated_at() + 1);
                doc.replace_profile_data(data, expected, updated, ctx)?;
                doc.activate("native", doc.revision(), ctx)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(publication.effect(), AuthPublicationEffect::Applied);
        assert!(publication.failure().is_none());
        assert!(publication.cleanup_failure().is_none());
    }
    pub fn remove(&self, original: &RequestContext<'_>) {
        let result = self.auth.remove_runtime(original.snapshot());
        assert!(result.failure().is_none());
        assert!(result.cleanup_failure().is_none());
    }
}
