use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::{
    broker::{BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits},
    profile::{AiService, BrokerService, ProviderConfig, ProviderKind, S3Location, S3Source},
    *,
};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeBindingRecord, ScopeLimits,
        UserIdentity,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_paths::InstallationPaths,
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_hub::{native::ContentLimits, policy::RequestContext};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::*,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    num::{NonZeroU32, NonZeroUsize},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};
// These persistence/authorization tests explicitly select local records. The
// signed Broker fixture supplies identity only; S3 transport has separate tests.
struct LocalRecordStorage;
impl RecordStorageResolver for LocalRecordStorage {
    fn resolve<'a>(
        &'a self,
        admission: RecordStorageAdmission<'a>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::io::Result<Option<RemoteRecordStorage>>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            admission.recheck().map_err(std::io::Error::other)?;
            Ok(None)
        })
    }
}

pub const SUBJECT: &str = "literal USER subject β";
pub const RESOURCE: &str = "https://mcp.example/records";
pub const ALL: [Permission; 4] = [
    Permission::ReadUser,
    Permission::Read(Module::Memory),
    Permission::MutateUser,
    Permission::WriteMemory,
];
pub const FENCED: [Permission; 5] = [
    Permission::ReadUser,
    Permission::Read(Module::Memory),
    Permission::MutateUser,
    Permission::WriteMemory,
    Permission::WriteFencedMemory,
];
pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
pub fn request(cancel: &Cancellation) -> RequestContext<'_> {
    request_until(cancel, Instant::now() + Duration::from_secs(120))
}
pub fn request_until(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 1048576,
            max_output_bytes: 1048576,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 65536,
            max_collection_entries: 65536,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
pub fn read_limits() -> MemoryStoreReadLimits {
    MemoryStoreReadLimits {
        max_query_bytes: NonZeroUsize::new(4 * 1024 * 1024).unwrap(),
        native: NativeDocumentFileLimits::new(1048576, 65536, 256, 8 * 1024 * 1024).unwrap(),
        batch_rows: NonZeroU32::new(2).unwrap(),
    }
}
pub fn head_limits() -> MemoryHeadReadLimits {
    let r = read_limits();
    MemoryHeadReadLimits {
        native: r.native,
        batch_rows: r.batch_rows,
    }
}
pub fn write_limits() -> MemoryStoreWriteLimits {
    MemoryStoreWriteLimits {
        max_input_bytes: NonZeroUsize::new(1048576).unwrap(),
        max_row_bytes: NonZeroUsize::new(2 * 1024 * 1024).unwrap(),
        read: read_limits(),
        artifacts: ArtifactLimits::new(
            1048576,
            65536,
            CopyLimits::new(1048576, 128, 65536, 16).unwrap(),
        )
        .unwrap(),
    }
}
pub fn input(key: &str) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: " Literal title β \n".into(),
        body: "quartz initial body".into(),
        attributes: MemoryAttributes {
            classification: MemoryClass::Decision,
            mandatory: true,
            important: false,
        },
        tags: vec![" tag ".into(), " tag ".into()],
        references: None,
    }
}
pub fn update(key: &str, revision: u64) -> UpdateMemory {
    UpdateMemory {
        expected_revision: revision,
        idempotency_key: key.into(),
        reason: "literal edit reason".into(),
        title: None,
        body: Some("topaz revised body".into()),
        classification: None,
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    }
}

// A minimal real broker: discovery, dynamic registration and EdDSA JWKS only.
// Unsupported routes are failures; no token refresh or UserInfo substitute exists.
pub struct Server {
    pub origin: String,
    key: Ed25519KeyPair,
    counts: Arc<Mutex<BTreeMap<String, usize>>>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    pub async fn new() -> Self {
        let key = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let metadata=json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,"client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":[RESOURCE]}}).to_string();
        let standard=json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),"registration_endpoint":format!("{origin}/register"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}).to_string();
        let keys=json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),"kid":"records-user","alg":"EdDSA","use":"sig","key_ops":["verify"]}]}).to_string();
        let counts = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
        let counter = Arc::clone(&counts);
        let task = tokio::spawn(async move {
            let mut sockets = JoinSet::new();
            loop {
                tokio::select! {
                 accepted=listener.accept()=>{let Ok((mut socket,_))=accepted else{break;};let counts=Arc::clone(&counter);let metadata=metadata.clone();let standard=standard.clone();let keys=keys.clone();sockets.spawn(async move{
                  let mut input=Vec::new();let mut buffer=[0u8;1024];while !input.windows(4).any(|v|v==b"\r\n\r\n"){let Ok(n)=socket.read(&mut buffer).await else{return;};if n==0||input.len()+n>16384{return;}input.extend_from_slice(&buffer[..n]);}
                  let Ok(request)=std::str::from_utf8(&input) else{return;};let path=request.lines().next().unwrap().split_whitespace().nth(1).unwrap();*counts.lock().unwrap().entry(path.into()).or_default()+=1;
                  let (status,body)=match path{"/.well-known/graphit-broker"=>(200,metadata),"/.well-known/openid-configuration"=>(200,standard),"/keys"=>(200,keys),"/register"=>(201,json!({"client_id":"web-client"}).to_string()),_=>(503,"{}".into())};
                  let response=format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len());let _=socket.write_all(response.as_bytes()).await;
                 });},_=sockets.join_next(),if !sockets.is_empty()=>(),
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
    pub fn count(&self, path: &str) -> usize {
        self.counts.lock().unwrap().get(path).copied().unwrap_or(0)
    }
    pub fn token(&self, subject: &str, audience: &str, expiry: u64) -> String {
        self.sign(&json!({"iss":self.origin,"sub":subject,"aud":[audience],"client_id":"graphit-cli","jti":"records-user-attempt","token_use":"access","iat":now_seconds().saturating_sub(1),"exp":expiry}))
    }
    pub fn sign(&self, claims: &serde_json::Value) -> String {
        let message = format!(
            "{}.{}",
            URL_SAFE_NO_PAD
                .encode(json!({"alg":"EdDSA","kid":"records-user","typ":"JWT"}).to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        format!(
            "{message}.{}",
            URL_SAFE_NO_PAD.encode(self.key.sign(message.as_bytes()).as_ref())
        )
    }
    pub fn verifier(&self, incoming: bool, revision: u64) -> BrokerVerifier {
        BrokerVerifier::new(
            BrokerPolicy {
                endpoint: &self.origin,
                mcp_resource: incoming.then_some(RESOURCE),
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
}
pub struct Fixture {
    pub server: Server,
    pub user: CapturedPublicationScope,
    record: ScopeBindingRecord,
    pub issuer: RoleIssuer,
    pub store: Arc<store::AuthStore>,
    pub installation: CapturedPublicationScope,
    _runtime: Arc<RuntimeKeyOwner>,
    _temp: tempfile::TempDir,
}
impl Fixture {
    pub async fn new(ctx: &RequestContext<'_>) -> Self {
        Self::with_permissions(ctx, &ALL).await
    }
    pub async fn with_permissions(ctx: &RequestContext<'_>, permissions: &[Permission]) -> Self {
        Self::configured(ctx, permissions, true).await
    }
    pub async fn native(ctx: &RequestContext<'_>) -> Self {
        Self::configured(ctx, &ALL, false).await
    }
    pub async fn fenced(ctx: &RequestContext<'_>) -> Self {
        Self::configured(ctx, &FENCED, true).await
    }
    pub async fn native_fenced(ctx: &RequestContext<'_>) -> Self {
        Self::configured(ctx, &FENCED, false).await
    }
    async fn configured(
        ctx: &RequestContext<'_>,
        permissions: &[Permission],
        incoming: bool,
    ) -> Self {
        let server = Server::new().await;
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let root = temp.path().join("project");
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&root).unwrap();
        let paths = InstallationPaths::resolve(Brand::default(), &root, Some(&home), None).unwrap();
        let private = installation_bootstrap::prepare(
            &paths,
            BootstrapLimits::new(65536, 256).unwrap(),
            ctx.snapshot(),
        )
        .into_installation()
        .unwrap();
        let runtime = Arc::new(
            RuntimeKeyOwner::start(
                private,
                RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
                ctx.snapshot(),
            )
            .into_owner()
            .ok()
            .unwrap(),
        );
        let installation = CapturedPublicationScope::installation(
            &paths,
            InstallationId::new("user-records-fixture").unwrap(),
            [paths.root().to_owned()],
            ScopeLimits::new(16, 4096, 128).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let binding = installation.binding().unwrap();
        let physical = AuthDocumentStore::capture(
            installation.clone(),
            AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let store = Arc::new(
            store::AuthStore::new(
                physical,
                profile::ProfileLimits {
                    bytes: 65536,
                    nodes: 4096,
                    scalar_bytes: 8192,
                    collection_entries: 64,
                    providers: 16,
                    profiles: 16,
                },
                ctx,
            )
            .unwrap(),
        );
        let config = ProviderConfig {
            kind: ProviderKind::Broker(BrokerService {
                endpoint: server.origin.clone(),
                mcp_resource: incoming.then(|| RESOURCE.into()),
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
        };
        let published = store
            .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| {
                doc.add_provider("service".into(), config, doc.revision(), 1, ctx)
            })
            .unwrap();
        assert!(published.failure().is_none());
        let sessions = AdminSessionPolicy {
            lifetime: Duration::from_secs(3600),
            maximum: Duration::from_secs(3600),
        };
        let policy = IssuerAuthorityPolicy::new(
            SourceAuthorityPolicy {
                ordinary: permissions,
                administrative: permissions,
                sessions,
            },
            SourceAuthorityPolicy {
                ordinary: permissions,
                administrative: permissions,
                sessions,
            },
            IssuerLimits {
                max_permissions: 16,
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
        .unwrap()
        .with_incoming_user_permissions("service", permissions)
        .unwrap();
        let issuer = RoleIssuer::new(
            ScopeCapture {
                live: &installation,
                record: &binding,
            },
            Arc::clone(&runtime),
            Arc::clone(&store),
            policy,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap();
        issuer
            .set_record_storage_resolver(Arc::new(LocalRecordStorage))
            .unwrap();
        let user = CapturedPublicationScope::user(
            &paths,
            InstallationId::new("user-records-fixture").unwrap(),
            UserIdentity::new(server.origin.clone(), SUBJECT.into()).unwrap(),
            ScopeLimits::new(16, 4096, 128).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let record = user.binding().unwrap();
        Self {
            server,
            user,
            record,
            issuer,
            store,
            installation,
            _runtime: runtime,
            _temp: temp,
        }
    }
    pub fn scope(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.user,
            record: &self.record,
        }
    }
    pub fn provider_revision(&self, ctx: &RequestContext<'_>) -> u64 {
        self.store
            .load(ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision()
    }
    pub fn secret(&self) -> String {
        std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap()
    }
    pub fn install_native_profile(&self, ctx: &RequestContext<'_>) {
        use profile::{OidcTokens, ProfileCredentials, ProfileData, ProfileExpectation, Secret};
        let expiry = now_seconds() + 600;
        let raw = self.server.token(SUBJECT, "broker-api", expiry);
        let data = ProfileData {
            name: "native".into(),
            provider: "service".into(),
            username: "display-not-principal".into(),
            organization: None,
            teams: vec![],
            credentials: ProfileCredentials::Broker {
                issuer: self.server.origin.clone(),
                subject: SUBJECT.into(),
                s3_disabled: false,
                tokens: OidcTokens {
                    access_token: Secret::new(raw.into()),
                    refresh_token: None,
                    id_token: Secret::new("unused-historical-id-token".into()),
                    expires_at: expiry.try_into().unwrap(),
                },
            },
        };
        let result = self
            .store
            .mutate(self.store.load(ctx).unwrap(), ctx, |doc, ctx| {
                let expected = ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("service").unwrap().revision(),
                    profile_revision: None,
                };
                doc.replace_profile_data(data, expected, 2, ctx)?;
                doc.activate("native", doc.revision(), ctx)
            })
            .unwrap();
        assert!(result.failure().is_none());
    }
}
