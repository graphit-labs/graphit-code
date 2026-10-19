use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::{
    broker::{
        BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits,
        oauth::{OAuthLimits, web::WebOAuth},
    },
    profile::{
        AiService, BrokerService, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
    },
    store::AuthStore,
    *,
};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CopyLimits, PublicationSlot, ScopeBindingRecord,
        ScopeLimits,
    },
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
    brand::Brand,
    file_lock::Cancellation,
    installation_bootstrap::{self, BootstrapLimits},
    installation_identity::InstallationId,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::{RuntimeKeyLimits, RuntimeKeyOwner},
    version_context::{Checkout, VersionContext},
};
use graphit_data::{
    knowledge_document_projection as h, knowledge_index::*, knowledge_references::*,
};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_hub::{
    materialization::{LineageLimits, MaterializationLimits},
    native::ContentLimits,
    policy::RequestContext,
    registry::{
        DocumentLimits, HubLimits, HubRegistry, Installation, InstalledArtifact, InstalledBackend,
        InstalledPayload,
    },
    *,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    future::Future,
    num::{NonZeroU32, NonZeroUsize},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};

pub fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
pub fn context(cancel: &Cancellation) -> RequestContext<'_> {
    context_deadline(cancel, Instant::now() + Duration::from_secs(180))
}
pub fn context_deadline(cancel: &Cancellation, deadline: Instant) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 65536,
            max_scalar_bytes: 4096,
            max_collection_entries: 128,
        },
        cancel,
        deadline,
    )
    .unwrap()
}
pub fn native_test(f: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(f)
        })
        .unwrap()
        .join()
        .unwrap();
}
pub fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap()
}
pub fn content() -> KnowledgeIndexContentLimits {
    KnowledgeIndexContentLimits {
        max_identifier_bytes: n(256),
    }
}
pub fn page_limits() -> KnowledgeIndexReadLimits {
    KnowledgeIndexReadLimits {
        content: content(),
        native: native(),
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
pub fn heads_limits() -> KnowledgeHeadReadLimits {
    KnowledgeHeadReadLimits {
        max_slug_bytes: n(256),
        native: native(),
        batch_rows: NonZeroU32::new(1).unwrap(),
    }
}
fn issuer_limits() -> IssuerLimits {
    IssuerLimits {
        max_permissions: 64,
        max_scope_bytes: 65536,
        max_parent_bearer_bytes: 1024,
    }
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

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
        let metadata=json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,"client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":["https://mcp.example/graphit"]}}).to_string();
        let standard=json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),"registration_endpoint":format!("{origin}/register"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}).to_string();
        let keys=json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),"kid":"actual","alg":"EdDSA","use":"sig","key_ops":["verify"]}]}).to_string();
        let counts = Arc::new(Mutex::new(BTreeMap::<String, usize>::new()));
        let seen = Arc::clone(&counts);
        let task = tokio::spawn(async move {
            let mut sockets = JoinSet::new();
            loop {
                tokio::select! {
                    accepted=listener.accept()=>{
                        let Ok((mut socket,_))=accepted else {break};
                        let counts=Arc::clone(&seen);let metadata=metadata.clone();let standard=standard.clone();let keys=keys.clone();
                        sockets.spawn(async move {
                            let mut input=Vec::new();let mut buffer=[0u8;1024];
                            while !input.windows(4).any(|b|b==b"\r\n\r\n") {
                                let Ok(read)=socket.read(&mut buffer).await else {return};
                                if read==0||input.len()+read>16384{return} input.extend_from_slice(&buffer[..read]);
                            }
                            let Ok(request)=std::str::from_utf8(&input) else {return};
                            let Some(path)=request.lines().next().and_then(|l|l.split_whitespace().nth(1)) else {return};
                            *counts.lock().unwrap().entry(path.into()).or_default()+=1;
                            let (status,body)=match path {
                                "/.well-known/graphit-broker"=>(200,metadata),
                                "/.well-known/openid-configuration"=>(200,standard),
                                "/keys"=>(200,keys),
                                "/register"=>(201,json!({"client_id":"web-client"}).to_string()),
                                "/userinfo"|"/token"=>(503,"{}".into()),
                                _=>(404,"{}".into()),
                            };
                            let response=format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len());
                            let _=socket.write_all(response.as_bytes()).await;
                        });
                    },
                    _=sockets.join_next(),if !sockets.is_empty()=>(),
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
    pub fn access(&self, resource: &str) -> String {
        let claims = json!({"iss":self.origin,"sub":"fixture-subject","aud":["broker-api",resource],"client_id":"web-client","jti":"knowledge-import-read","token_use":"access","iat":now().saturating_sub(1),"exp":now()+600});
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
    pub fn verifier(&self, revision: u64) -> BrokerVerifier {
        BrokerVerifier::new(
            BrokerPolicy {
                endpoint: &self.origin,
                mcp_resource: None,
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
pub fn web<'a>(verifier: &'a BrokerVerifier) -> WebOAuth<'a> {
    WebOAuth::new(
        verifier,
        OAuthLimits {
            authorization_url_bytes: 8192,
            callback_bytes: 8192,
            callback_query_entries: 8,
            form_body_bytes: 8192,
            token_bytes: 8192,
            response_nodes: 32,
            response_scalar_bytes: 8192,
        },
        "https://workspace.example/callback",
        false,
    )
    .unwrap()
}
pub fn broker_config(endpoint: &str) -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Broker(BrokerService {
            endpoint: endpoint.into(),
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
    }
}
fn hub_limits() -> HubLimits {
    let copy = CopyLimits::new(16 * 1024 * 1024, 512, 32 * 1024, 16).unwrap();
    HubLimits::new(
        DocumentLimits {
            bytes: 256 * 1024,
            entries: 128,
            key_bytes: 8192,
            json_nodes: 50000,
            json_string_bytes: 32 * 1024,
        },
        ProjectionLimits::new(4096, 32 * 1024, 128, 8192, 128).unwrap(),
        ArtifactLimits::new(256 * 1024, 8192, copy).unwrap(),
        MaterializationLimits {
            output: CopyLimits::new(512 * 1024 * 1024, 100000, 8 * 1024 * 1024, 128).unwrap(),
            snapshots: copy,
            lineage: LineageLimits::new(4096, 8192, 32 * 1024).unwrap(),
        },
    )
    .unwrap()
}
pub struct Fixture {
    pub _dir: tempfile::TempDir,
    pub paths: InstallationPaths,
    pub installation: CapturedPublicationScope,
    record: ScopeBindingRecord,
    runtime: Arc<RuntimeKeyOwner>,
    project: CapturedPublicationScope,
    project_record: ScopeBindingRecord,
    version: VersionContext,
    pub store: Arc<AuthStore>,
    pub registry: HubRegistry,
    pub id: ArtifactId,
    pub artifact_version: ConcreteVersion,
    pub source: SourceRef,
}
impl Fixture {
    pub fn foreign_registry(&self) -> HubRegistry {
        HubRegistry::global(
            &self.paths,
            &InstallationId::new("foreign-installation").unwrap(),
            hub_limits(),
        )
        .unwrap()
    }
    pub fn registry_limits_projection(&self) -> ProjectionLimits {
        hub_limits().projection()
    }
    pub fn new(ctx: &RequestContext<'_>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&home).unwrap();
        let paths = InstallationPaths::resolve(
            Brand::new("knowledge-import-test").unwrap(),
            &root,
            Some(&home),
            None,
        )
        .unwrap();
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
        let installation_id = InstallationId::new("local-installation").unwrap();
        let limits = ScopeLimits::new(16, 4096, 128).unwrap();
        let installation = CapturedPublicationScope::installation(
            &paths,
            installation_id.clone(),
            [root.clone()],
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let record = installation.binding().unwrap();
        let project_paths = ProjectPaths::at(&root, &paths).unwrap();
        let project = CapturedPublicationScope::project(
            &project_paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let project_record = project.binding().unwrap();
        let version =
            VersionContext::select(&project_paths, Checkout::NonGit, "", None, "").unwrap();
        let auth_scope = CapturedPublicationScope::installation(
            &paths,
            installation_id.clone(),
            [paths.root().to_owned()],
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let physical = AuthDocumentStore::capture(
            auth_scope,
            AuthDocumentLimits::new(4096, 65536, 256, 65536).unwrap(),
            ctx.snapshot(),
        )
        .unwrap();
        let store = Arc::new(
            AuthStore::new(
                physical,
                ProfileLimits {
                    bytes: 4096,
                    nodes: 4096,
                    scalar_bytes: 2048,
                    collection_entries: 64,
                    providers: 16,
                    profiles: 16,
                },
                ctx,
            )
            .unwrap(),
        );
        let registry = HubRegistry::global(&paths, &installation_id, hub_limits()).unwrap();
        let projection = hub_limits().projection();
        let id = ArtifactId::new("vendor/knowledge", projection).unwrap();
        let artifact_version = ConcreteVersion::new("exact-version", projection).unwrap();
        let source = SourceRef::hub_cache(
            &paths,
            "01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap(),
            ArtifactType::Knowledge,
            id.clone(),
            None,
            artifact_version.clone(),
            projection,
        )
        .unwrap();
        Self {
            _dir: dir,
            paths,
            installation,
            record,
            runtime,
            project,
            project_record,
            version,
            store,
            registry,
            id,
            artifact_version,
            source,
        }
    }
    pub fn realm(&self) -> ScopeCapture<'_> {
        ScopeCapture {
            live: &self.installation,
            record: &self.record,
        }
    }
    pub fn issuer(&self, ctx: &RequestContext<'_>) -> RoleIssuer {
        let permissions = [
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ];
        let source = SourceAuthorityPolicy {
            ordinary: &permissions,
            administrative: &permissions,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(3600),
                maximum: Duration::from_secs(3600),
            },
        };
        let policy = IssuerAuthorityPolicy::new(
            source,
            source,
            issuer_limits(),
            ServiceAuthorityInput {
                rules: &[],
                limits: ServiceAdmissionLimits {
                    max_rules: 16,
                    max_binding_bytes: 65536,
                },
                native_refresh_margin: Duration::ZERO,
            },
        )
        .unwrap()
        .with_web_imported_knowledge_read("service")
        .unwrap();
        RoleIssuer::new(
            self.realm(),
            Arc::clone(&self.runtime),
            Arc::clone(&self.store),
            policy,
            TtlPolicy {
                ttl: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
                refresh_margin: Duration::from_secs(120),
            },
            ctx,
        )
        .unwrap()
    }
    pub fn provider(&self, server: &Server, ctx: &RequestContext<'_>) -> u64 {
        let result = self
            .store
            .mutate(self.store.load(ctx).unwrap(), ctx, |doc, ctx| {
                doc.add_provider(
                    "service".into(),
                    broker_config(&server.origin),
                    doc.revision(),
                    1,
                    ctx,
                )
            })
            .unwrap();
        assert!(result.failure().is_none());
        self.store
            .load(ctx)
            .unwrap()
            .document()
            .provider("service")
            .unwrap()
            .revision()
    }
    pub fn entry(&self) -> InstalledArtifact {
        InstalledArtifact {
            consumer: PublicationSlot::Scope,
            artifact_type: ArtifactType::Knowledge,
            id: self.id.clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: self.source.clone(),
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::Native,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        }
    }
    pub fn register(&self, ctx: &RequestContext<'_>) {
        let mut writer = self
            .registry
            .writer(
                ctx.remaining().unwrap().expect("finite fixture deadline"),
                ctx.cancellation(),
            )
            .unwrap();
        writer.put_metadata(self.entry()).unwrap();
    }
    pub fn remove_registration(&self, ctx: &RequestContext<'_>) {
        let mut writer = self
            .registry
            .writer(
                ctx.remaining().unwrap().expect("finite fixture deadline"),
                ctx.cancellation(),
            )
            .unwrap();
        assert!(
            writer
                .remove_metadata_version(
                    ArtifactType::Knowledge,
                    self.id.as_str(),
                    &self.artifact_version
                )
                .unwrap()
        );
    }
    pub async fn seed(
        &self,
        issuer: &RoleIssuer,
        ctx: &RequestContext<'_>,
        pages: &[h::AuthoredPageFields<'_>],
        references: Option<&[KnowledgeReferenceFields<'_>]>,
    ) {
        let secret =
            std::fs::read_to_string(self.installation.owner_root().join("daemon/mcp.key")).unwrap();
        let caller = issuer.authenticate_runtime(&secret, ctx).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let scope = ScopeInput {
            installation: self.realm(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &self.project,
                    record: &self.project_record,
                },
                version: &self.version,
            }),
            providers: &[],
            binding: NativeBinding::KiroV3,
            global_version: "",
        };
        let write = authority
            .admit_authenticated_write(&caller, ctx, scope, ProjectWriteOperation::Knowledge)
            .unwrap();
        let h = h::prepare(
            ctx.snapshot(),
            pages,
            &[],
            h::KnowledgeDocumentProjectionLimits {
                max_documents: n(8),
                max_observations: n(8),
                max_rows: n(64),
                max_field_bytes: n(65536),
                max_text_bytes: n(131072),
            },
        )
        .unwrap();
        let write_limits = KnowledgeIndexWriteLimits {
            content: content(),
            native: native(),
            artifacts: ArtifactLimits::new(
                65536,
                4096,
                CopyLimits::new(4 * 1024 * 1024, 128, 65536, 16).unwrap(),
            )
            .unwrap(),
        };
        let published = if let Some(fields) = references {
            let refs = prepare_knowledge_references(ctx.snapshot(), &h, fields, content()).unwrap();
            publish_native_authored_with_references(ctx.snapshot(), &write, &h, &refs, write_limits)
                .await
                .unwrap()
        } else {
            publish_native_authored(ctx.snapshot(), &write, &h, write_limits)
                .await
                .unwrap()
        };
        let source = PathBuf::from(&published.default.corpus_uri);
        let target = self.source.directory().as_path().join("index.lance");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        copy_tree(&source, &target);
        self.register(ctx);
    }
}
fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let dest = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &dest)
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}
pub fn image(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for e in std::fs::read_dir(path).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if e.file_type().unwrap().is_dir() {
                visit(root, &p, out)
            } else {
                out.insert(
                    p.strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(p).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
pub fn page<'a>(slug: &'a str, targets: &'a [&'a str]) -> h::AuthoredPageFields<'a> {
    h::AuthoredPageFields {
        slug: Some(slug),
        title: Some(" título "),
        body: Some("# corpo integral é\n"),
        kind: Some("page"),
        summary: Some("corpo"),
        description: Some("descrição completa"),
        summary_truncated: false,
        source_path: "docs/page.md",
        physical_digest: None,
        compilation_digest: h::CompilationDigest32::from_bytes([3; 32]),
        recipe: h::RecipeFields {
            id: h::AUTHORED_RECIPE_ID,
            parameters: Some(h::AuthoredParameters {
                fallback_title: "title",
                kind_on_absent: "page",
                max_summary_bytes: n(32),
            }),
        },
        updated_at: Some(UNIX_EPOCH - Duration::from_nanos(1)),
        stale_since: None,
        stale_after: None,
        resolved_wiki_targets: targets,
    }
}
pub fn fields<'a>(
    slug: &'a str,
    state: KnowledgeDocumentState,
    authored: &'a [KnowledgeAuthoredReference<'a>],
) -> KnowledgeReferenceFields<'a> {
    KnowledgeReferenceFields {
        slug,
        state,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project("01KSH1CRFFG8Z74B5ZS78WW808".parse().unwrap()),
            context: None,
        },
        authored,
    }
}
