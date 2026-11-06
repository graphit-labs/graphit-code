#![cfg(target_os = "linux")]
//! Real trusted Foreground factory, signed Incoming MCP, FULL16 and Process fixtures.
use crate::agent_fixture;
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_ai::knowledge_conversation::ConversationLimits;
use graphit_app::{
    foreground_source::ForegroundSource,
    knowledge_conversation::ConversationAccess,
    native_install_http::NativeInstallPolicy,
    source_server::{SourcePolicy, SourceServer},
};
use graphit_auth::{
    broker::{
        BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits,
        project_access::{BrokerProjectAccess, ProjectAccessPolicy},
    },
    profile::{
        AiService, BrokerService, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
    },
    profile_manager::{NativeProfileManager, ProfileManagerAuthority},
    *,
};
use graphit_core::{
    artifact_store::{
        ArtifactLimits, CapturedPublicationScope, CapturedSourceReader, CopyLimits, CopyPolicy,
        RetainedBytesLimits, RetainedFileSet, ScopeLimits, SnapshotPolicy, SourceReadLimits,
    },
    auth_document::AuthDocumentLimits,
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::BootstrapLimits,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyLimits,
    tcp_listener::{self, Options, PortSelection},
    version_context::VersionContext,
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    project_bootstrap::ProjectBootstrapInput,
};
use graphit_data::{
    knowledge_document_projection as h, knowledge_document_update::KnowledgeDocumentUpdateResult,
    knowledge_index::*,
};
use graphit_fs::NodeKind;
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_hub::{
    NativeBinding,
    native::ContentLimits,
    policy::{HubResourcePolicy, RequestContext},
    registry::HubRegistry,
};
use graphit_knowledge::{
    compilation_fingerprint::CompilationFingerprintLimits,
    document::DocumentParseLimits,
    document_republication::{NativeDocumentRepublicationLimits, republish_native_documents},
    native_document_mapping::{AuthoredDocumentInput, NativeAuthoredMappingLimits},
    native_index_publication::{NativeDocumentPublicationLimits, publish_native_documents},
    publication_metadata::{
        CompiledDocument, PublicationMetadataLimits, PublicationMetadataPolicy,
        compile_captured_document,
    },
    references::ReferenceLimits,
};
use rmcp::{
    model::{CallToolRequestParams, ProtocolVersion},
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    num::{NonZeroU32, NonZeroUsize},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;
const RESOURCE: &str = "https://fixture.invalid/mcp";
fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).unwrap()
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
fn document_parse_limits() -> DocumentParseLimits {
    DocumentParseLimits {
        max_document_bytes: n(65536),
        max_input_bytes: n(65536),
        max_frontmatter_bytes: n(65536),
        max_yaml_events: n(1024),
        max_markdown_events: n(1024),
        max_yaml_depth: n(16),
        max_scalar_bytes: n(4096),
        max_links: n(100),
        max_projection_bytes: n(65536),
        max_summary_bytes: n(16),
    }
}
fn publication_limits() -> NativeDocumentRepublicationLimits {
    let content = KnowledgeIndexContentLimits {
        max_identifier_bytes: n(256),
    };
    let copy = CopyLimits::new(1048576, 1024, 65536, 16).unwrap();
    NativeDocumentRepublicationLimits {
        read: KnowledgeIndexReadLimits {
            content,
            native: native(),
            batch_rows: NonZeroU32::new(1).unwrap(),
        },
        publication: NativeDocumentPublicationLimits {
            parse: document_parse_limits(),
            mapping: NativeAuthoredMappingLimits {
                max_field_bytes: n(65536),
                max_uri_bytes: n(4096),
                max_target_bytes: n(4096),
            },
            fingerprint: CompilationFingerprintLimits {
                max_preimage_bytes: n(131072),
            },
            references: ReferenceLimits {
                max_identifier_bytes: n(256),
            },
            projection: h::KnowledgeDocumentProjectionLimits {
                max_documents: n(8),
                max_observations: n(8),
                max_rows: n(64),
                max_field_bytes: n(65536),
                max_text_bytes: n(131072),
            },
            index: KnowledgeIndexWriteLimits {
                content,
                native: native(),
                artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
            },
            authored_references:
                graphit_knowledge::document_authored_references::DocumentAuthoredReferenceLimits {
                    max_results: n(32),
                    max_identifier_bytes: n(256),
                },
        },
    }
}
fn capture_document(
    project: &CapturedPublicationScope,
    ctx: &RequestContext<'_>,
    path: &str,
    source: &str,
) -> CapturedSourceReader {
    let destination = project.owner_root().join(path);
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::fs::write(destination, source).unwrap();
    let bytes = source.len().max(1);
    let retention = RetainedBytesLimits::new(bytes, 1, 1).unwrap();
    let copy = CopyLimits::new(bytes as u64, 1, 1, 1).unwrap();
    let policy = SnapshotPolicy::new(
        CopyPolicy::RejectFileLinks,
        copy,
        RetainedFileSet::new([PathBuf::new()], retention).unwrap(),
        retention,
    )
    .unwrap();
    CapturedSourceReader::capture_project(
        project,
        Path::new(path),
        NodeKind::File,
        &policy,
        SourceReadLimits::new(bytes, 4096, bytes as u64).unwrap(),
        ctx.snapshot(),
    )
    .unwrap()
}
fn compile_document<'a>(
    reader: &'a CapturedSourceReader,
    ctx: &RequestContext<'_>,
) -> CompiledDocument<'a> {
    compile_document_with_fallback(reader, ctx, "fallback")
}
fn compile_document_with_fallback<'a>(
    reader: &'a CapturedSourceReader,
    ctx: &RequestContext<'_>,
    fallback: &str,
) -> CompiledDocument<'a> {
    compile_captured_document(
        reader,
        Path::new(""),
        ctx.snapshot(),
        PublicationMetadataPolicy {
            fallback_title: fallback.into(),
            kind_on_absent: "page".into(),
        },
        document_parse_limits(),
        PublicationMetadataLimits {
            max_input_bytes: n(65536),
            max_title_bytes: n(1024),
            max_kind_bytes: n(1024),
            max_description_bytes: n(1024),
            max_effective_summary_bytes: n(1024),
            max_effective_metadata_bytes: n(65536),
            max_owned_metadata_bytes: n(65536),
            max_root_entries: n(100),
            max_typed_nodes: n(1024),
            max_typed_depth: n(16),
            max_typed_scalar_bytes: n(4096),
            max_typed_events: n(2048),
            max_typed_lookahead_characters: n(1024),
            max_typed_flow_depth: n(16),
        },
    )
    .unwrap()
}
fn document_input<'d, 's>(
    compiled: &'d CompiledDocument<'s>,
    slug: &'d str,
    source_path: &'d str,
) -> AuthoredDocumentInput<'d, 's> {
    AuthoredDocumentInput {
        compiled,
        slug,
        source_path,
        updated_at: None,
        stale_since: None,
        stale_after: None,
    }
}
fn context(cancel: &Cancellation) -> RequestContext<'_> {
    RequestContext::new(
        ContentLimits {
            max_input_bytes: 1048576,
            max_output_bytes: 8388608,
            max_depth: 64,
            max_nodes: 65536,
            max_scalar_bytes: 1048576,
            max_collection_entries: 4096,
        },
        cancel,
        Instant::now() + Duration::from_secs(90),
    )
    .unwrap()
}
struct BrokerState {
    origin: String,
    key: Ed25519KeyPair,
    allowed: AtomicBool,
    calls: AtomicUsize,
    calls_changed: tokio::sync::Notify,
}
impl BrokerState {
    async fn wait_for_calls(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let changed = self.calls_changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.calls.load(Ordering::SeqCst) >= count {
                    return;
                }
                changed.await;
            }
        })
        .await
        .unwrap();
    }
    fn token(&self, subject: &str, audience: &str, expiry: u64) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = json!({"iss":self.origin,"sub":subject,"aud":[audience],"client_id":"fixture-client","jti":"fixture-only","token_use":"access","iat":now.saturating_sub(1),"exp":expiry});
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
}
struct Broker {
    state: Arc<BrokerState>,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Broker {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(BrokerState {
            origin,
            key: Ed25519KeyPair::from_pkcs8(
                Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                    .unwrap()
                    .as_ref(),
            )
            .unwrap(),
            allowed: AtomicBool::new(true),
            calls: AtomicUsize::new(0),
            calls_changed: tokio::sync::Notify::new(),
        });
        let router = Router::new()
            .route("/.well-known/graphit-broker", get(metadata))
            .route("/.well-known/openid-configuration", get(oidc))
            .route("/keys", get(keys))
            .route("/v1/project/access/authorize", post(access))
            .with_state(Arc::clone(&state));
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap()
        });
        Self { state, stop, task }
    }
    async fn shutdown(self) {
        self.stop.cancel();
        self.task.await.unwrap()
    }
}
async fn metadata(State(s): State<Arc<BrokerState>>) -> Json<Value> {
    Json(
        json!({"version":"1","issuer":s.origin,"authentication":{"type":"oidc","issuer":s.origin,"client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":[RESOURCE]}}),
    )
}
async fn oidc(State(s): State<Arc<BrokerState>>) -> Json<Value> {
    Json(
        json!({"issuer":s.origin,"authorization_endpoint":format!("{}/authorize",s.origin),"registration_endpoint":format!("{}/register",s.origin),"token_endpoint":format!("{}/token",s.origin),"jwks_uri":format!("{}/keys",s.origin),"userinfo_endpoint":format!("{}/userinfo",s.origin),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}),
    )
}
async fn keys(State(s): State<Arc<BrokerState>>) -> Json<Value> {
    Json(
        json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(s.key.public_key().as_ref()),"kid":"actual","alg":"EdDSA","use":"sig","key_ops":["verify"]}]}),
    )
}
async fn access(
    State(s): State<Arc<BrokerState>>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> (StatusCode, Json<Value>) {
    assert_eq!(v["module"], "knowledge");
    assert_eq!(v["action"], "read");
    let raw = headers[header::AUTHORIZATION]
        .to_str()
        .unwrap()
        .strip_prefix("Bearer ")
        .unwrap();
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(raw.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    s.calls.fetch_add(1, Ordering::SeqCst);
    s.calls_changed.notify_waiters();
    if !s.allowed.load(Ordering::SeqCst) {
        return (StatusCode::FORBIDDEN, Json(json!({})));
    }
    (
        StatusCode::OK,
        Json(
            json!({"subject":{"issuer":s.origin,"subject":claims["sub"]},"authorization_revision":1,"project_id":v["project_id"],"module":"knowledge","action":"read"}),
        ),
    )
}
fn seed_policy(scope: ScopeLimits) -> StartupPolicy {
    let permissions: Vec<_> = [
        Permission::Read(Module::Knowledge),
        Permission::MutateModule(Module::Knowledge),
        Permission::ManageAccessGrants,
        Permission::Delegate,
    ]
    .iter()
    .chain([Permission::ManageAuthProfiles].iter())
    .copied()
    .collect();
    let source = || SourceAuthorityPolicy {
        ordinary: &permissions,
        administrative: &permissions,
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    let authority = IssuerAuthorityPolicy::new(
        source(),
        source(),
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
    StartupPolicy {
        bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
        runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
        scope,
        document: AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
        profiles: ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        authority,
        ttl: TtlPolicy {
            ttl: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
            refresh_margin: Duration::from_secs(120),
        },
    }
}
#[derive(Clone, Copy)]
enum DocumentChange {
    Unrelated,
    Selected,
    Recipe,
}
struct Fixture {
    temp: tempfile::TempDir,
    paths: InstallationPaths,
    project: ProjectPaths,
    launches: PathBuf,
    installation_id: graphit_core::installation_identity::InstallationId,
}
impl Fixture {
    async fn new(broker: &Broker) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let cancel = Cancellation::default();
        let content = ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: 64,
        };
        let ctx = RequestContext::new(content, &cancel, Instant::now() + Duration::from_secs(30))
            .unwrap();
        let scope = ScopeLimits::new(16, 4096, 256).unwrap();
        let policy = seed_policy(scope);
        let root = temp.path().join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("raw"), "actual native payload").unwrap();
        let project = ProjectPaths::at(&root, &paths).unwrap();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancel)
            .unwrap()
            .ensure(&ProjectMetadata::new("project").unwrap())
            .unwrap();
        let config = ConfigSnapshot::new(paths.brand(), []).unwrap();
        let hub = HubResourcePolicy::resolve(&config).unwrap();
        let bootstrap_original = hub.request_context(Instant::now(), &cancel).unwrap();
        ForegroundSource::start(
            &paths,
            &[ProjectBootstrapInput {
                project: &project,
                explicit_version: "",
                pin: None,
                detached_base: "",
            }],
            0,
            &config,
            NativeBinding::Codex,
            &bootstrap_original,
        )
        .unwrap()
        .shutdown()
        .await
        .unwrap();
        let auth = Arc::new(
            DaemonAuth::start(&paths, std::slice::from_ref(&project), policy, &ctx).unwrap(),
        );
        let raw = {
            let key = auth.runtime().check(ctx.snapshot()).unwrap();
            String::from_utf8(key.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
        };
        let checkout = GitCheckout::inspect(&root, Duration::from_secs(5), &cancel).unwrap();
        let version =
            VersionContext::select(&project, checkout.as_checkout(), "", None, "").unwrap();

        let issuer = auth.issuer();
        let caller = issuer.authenticate_runtime(&raw, &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let manager = NativeProfileManager::new(issuer);
        let snapshot = manager
            .begin(ProfileManagerAuthority::Admin(&parent))
            .unwrap();
        let revision = snapshot.revision();
        let publication = manager
            .add_provider(
                snapshot,
                "broker".into(),
                ProviderConfig {
                    kind: ProviderKind::Broker(BrokerService {
                        endpoint: broker.state.origin.clone(),
                        mcp_resource: Some(RESOURCE.into()),
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
                revision,
                1,
            )
            .unwrap();
        assert!(publication.failure().is_none() && publication.cleanup_failure().is_none());
        assert!(publication.into_pending_cleanup().is_none());
        let id = ProjectIdentityStore::at(&project)
            .load()
            .unwrap()
            .unwrap()
            .id();
        let live = CapturedPublicationScope::project(&project, id, scope, ctx.snapshot()).unwrap();
        let record = live.binding().unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let write = authority
            .admit_authenticated_write(
                &caller,
                &ctx,
                ScopeInput {
                    installation: auth.installation(),
                    project: Some(ProjectCapture {
                        scope: ScopeCapture {
                            live: &live,
                            record: &record,
                        },
                        version: &version,
                    }),
                    providers: &[],
                    binding: NativeBinding::Codex,
                    global_version: "",
                },
                ProjectWriteOperation::Knowledge,
            )
            .unwrap();
        let paris_reader = capture_document(
            &live,
            &ctx,
            "docs/paris.md",
            "---\ntitle: Capital β\n---\nParis é a capital.\n",
        );
        let unrelated_reader = capture_document(
            &live,
            &ctx,
            "docs/unrelated.md",
            "---\ntitle: Unrelated\n---\nUnrelated original source.\n",
        );
        let paris = compile_document(&paris_reader, &ctx);
        let unrelated = compile_document(&unrelated_reader, &ctx);
        publish_native_documents(
            ctx.snapshot(),
            &write,
            &[
                document_input(&paris, "Paris", "docs/paris.md"),
                document_input(&unrelated, "Unrelated", "docs/unrelated.md"),
            ],
            publication_limits().publication,
        )
        .await
        .unwrap();
        drop(write);
        drop(caller);
        let installation_id = match auth.installation().live.owner() {
            graphit_core::artifact_store::StoreOwner::Installation { installation_id } => {
                installation_id.clone()
            }
            _ => unreachable!("actual daemon installation owner"),
        };
        let removal = auth.remove_runtime(ctx.snapshot());
        assert!(removal.failure().is_none() && removal.cleanup_failure().is_none());
        drop(auth);
        let program = temp.path().join("codex");
        let launches = temp.path().join("launches.jsonl");
        agent_fixture::install(&program, json!({"kind":"conversation","protocol":"codex"}));
        Self {
            temp,
            paths,
            project,
            launches,
            installation_id,
        }
    }
    async fn republish(&self, change: DocumentChange) {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let scope = ScopeLimits::new(16, 4096, 256).unwrap();
        let auth = DaemonAuth::start(
            &self.paths,
            std::slice::from_ref(&self.project),
            seed_policy(scope),
            &ctx,
        )
        .unwrap();
        let raw = {
            let key = auth.runtime().check(ctx.snapshot()).unwrap();
            String::from_utf8(key.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
        };
        let issuer = auth.issuer();
        let caller = issuer.authenticate_runtime(&raw, &ctx).unwrap();
        let checkout =
            GitCheckout::inspect(self.project.root(), Duration::from_secs(5), &cancel).unwrap();
        let version =
            VersionContext::select(&self.project, checkout.as_checkout(), "", None, "").unwrap();
        let id = ProjectIdentityStore::at(&self.project)
            .load()
            .unwrap()
            .unwrap()
            .id();
        let live =
            CapturedPublicationScope::project(&self.project, id, scope, ctx.snapshot()).unwrap();
        let record = live.binding().unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let write = authority
            .admit_authenticated_write(
                &caller,
                &ctx,
                ScopeInput {
                    installation: auth.installation(),
                    project: Some(ProjectCapture {
                        scope: ScopeCapture {
                            live: &live,
                            record: &record,
                        },
                        version: &version,
                    }),
                    providers: &[],
                    binding: NativeBinding::Codex,
                    global_version: "",
                },
                ProjectWriteOperation::Knowledge,
            )
            .unwrap();
        let read = authority
            .admit_authenticated_read(
                &caller,
                &ctx,
                ScopeInput {
                    installation: auth.installation(),
                    project: Some(ProjectCapture {
                        scope: ScopeCapture {
                            live: &live,
                            record: &record,
                        },
                        version: &version,
                    }),
                    providers: &[],
                    binding: NativeBinding::Codex,
                    global_version: "",
                },
                ProjectReadModule::Knowledge,
            )
            .unwrap();
        let KnowledgeIndexRead::Published {
            generation: expected,
            documents: before,
        } = read_native_knowledge_index(ctx.snapshot(), &read, publication_limits().read)
            .await
            .unwrap()
        else {
            panic!("actual published generation required before update");
        };
        let (slug, source_path, unchanged) = match change {
            DocumentChange::Unrelated => ("Unrelated", "docs/unrelated.md", "Paris"),
            DocumentChange::Selected | DocumentChange::Recipe => {
                ("Paris", "docs/paris.md", "Unrelated")
            }
        };
        let source = match change {
            DocumentChange::Selected => {
                "---\ntitle: Capital β\n---\nParis a changé réellement.\n".into()
            }
            DocumentChange::Unrelated => {
                "---\ntitle: Unrelated\n---\nUnrelated revised source.\n".into()
            }
            DocumentChange::Recipe => {
                std::fs::read_to_string(live.owner_root().join(source_path)).unwrap()
            }
        };
        let reader = capture_document(&live, &ctx, source_path, &source);
        let fallback = if matches!(change, DocumentChange::Recipe) {
            "different recipe fallback"
        } else {
            "fallback"
        };
        let compiled = compile_document_with_fallback(&reader, &ctx, fallback);
        let update = republish_native_documents(
            ctx.snapshot(),
            &read,
            &write,
            &expected,
            &[document_input(&compiled, slug, source_path)],
            publication_limits(),
        )
        .await
        .unwrap();
        let KnowledgeDocumentUpdateResult::Updated { generation, .. } = update else {
            panic!("changed captured file must produce an actual update");
        };
        assert!(generation.version > expected.version);
        let KnowledgeIndexRead::Published {
            generation: observed,
            documents: after,
        } = read_native_knowledge_index(ctx.snapshot(), &read, publication_limits().read)
            .await
            .unwrap()
        else {
            panic!("published generation required after update");
        };
        assert_eq!(observed, generation);
        assert_eq!(after.len(), before.len());
        for original in before.iter().filter(|document| document.slug != slug) {
            assert_eq!(
                Some(original),
                after.iter().find(|document| document.slug == original.slug),
            );
        }
        assert!(before.iter().any(|document| document.slug == unchanged));
        let updated = after.iter().find(|d| d.slug == slug).unwrap();
        let prior = before.iter().find(|d| d.slug == slug).unwrap();
        assert_eq!(updated.body, compiled.whole_source());
        assert_eq!(updated.source_path, prior.source_path);
        if matches!(change, DocumentChange::Recipe) {
            assert_eq!(updated.body, prior.body);
            assert_eq!(updated.physical_digest, prior.physical_digest);
            assert_ne!(updated.recipe_parameters, prior.recipe_parameters);
        } else {
            assert_eq!(updated.recipe_parameters, prior.recipe_parameters);
            assert_ne!(updated.physical_digest, prior.physical_digest);
        }
        assert_ne!(updated.compilation_digest, prior.compilation_digest);
        drop(read);
        drop(write);
        drop(caller);
        let removal = auth.remove_runtime(ctx.snapshot());
        assert!(removal.failure().is_none() && removal.cleanup_failure().is_none());
    }
    async fn start(&self, broker: &Broker, optin: Option<Value>) -> ForegroundSource {
        self.start_with_options(broker, optin, self.temp.path(), None)
            .await
    }
    async fn start_with_options(
        &self,
        broker: &Broker,
        optin: Option<Value>,
        program_directory: &Path,
        deadline_ms: Option<u64>,
    ) -> ForegroundSource {
        self.start_native(
            broker,
            optin,
            deadline_ms,
            NativeBinding::Codex,
            None,
            vec![("PATH".into(), program_directory.as_os_str().to_owned())],
        )
        .await
    }
    async fn start_native(
        &self,
        broker: &Broker,
        optin: Option<Value>,
        deadline_ms: Option<u64>,
        binding: NativeBinding,
        completion: Option<graphit_ai::cli_provider::CliCompletionProvider>,
        environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    ) -> ForegroundSource {
        let mut browser = json!({"providers":["broker"],"project_endpoint":format!("{}/v1/project/access/authorize",broker.state.origin),"allow_loopback_http":true});
        if let Some(v) = optin {
            browser["knowledge_conversations"] = v;
        }
        let mut values = vec![(ConfigKey::new("task.browser").unwrap(), browser)];
        if let Some(provider) = completion {
            values.push((
                ConfigKey::new("ai.cli").unwrap(),
                json!(provider.executable()),
            ));
        }
        if let Some(milliseconds) = deadline_ms {
            values.push((
                ConfigKey::new("hub.limits.request.deadline_ms").unwrap(),
                json!(milliseconds),
            ));
        }
        let layers =
            vec![ConfigDocument::new(self.paths.brand(), ConfigSource::Global, values).unwrap()];
        let config = ConfigSnapshot::new(self.paths.brand(), layers).unwrap();
        let cancel = Cancellation::default();
        let original = context(&cancel);
        ForegroundSource::start_and_publish(
            &self.paths,
            &[ProjectBootstrapInput {
                project: &self.project,
                explicit_version: "",
                pin: None,
                detached_base: "",
            }],
            0,
            &config,
            binding,
            environment,
            &original,
        )
        .await
        .unwrap()
    }
    async fn start_kiro(
        &self,
        broker: &Broker,
        binding: NativeBinding,
        mode: &str,
        deadline_ms: Option<u64>,
    ) -> ForegroundSource {
        let program = self.temp.path().join("kiro-cli");
        if !program.exists() {
            std::fs::copy(env!("CARGO_BIN_EXE_kiro-knowledge-fixture"), &program).unwrap();
        }
        self.start_native(
            broker,
            Some(json!(true)),
            deadline_ms,
            binding,
            None,
            vec![
                ("PATH".into(), self.temp.path().as_os_str().to_owned()),
                ("KIRO_CAPTURED".into(), "literal-captured".into()),
                ("KIRO_FIXTURE_MODEL".into(), "fixture-model".into()),
                ("KIRO_FIXTURE_MODE".into(), mode.into()),
                (
                    "KIRO_FIXTURE_JOURNAL".into(),
                    self.temp.path().join("kiro-journal").into_os_string(),
                ),
            ],
        )
        .await
    }
    fn kiro_events(&self) -> Vec<String> {
        match std::fs::read_to_string(self.temp.path().join("kiro-journal")) {
            Ok(s) => s.lines().map(str::to_owned).collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(e) => panic!("fixture journal {:?}", e.kind()),
        }
    }
    fn launches(&self) -> Vec<Value> {
        read_launches(&self.launches)
    }
    fn turn_gate(&self, query: &str, suffix: &str) -> PathBuf {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(query.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        self.temp.path().join(format!("{digest}.{suffix}"))
    }
    async fn wait_for_blocked_turn(&self, query: &str) {
        let ready = self.turn_gate(query, "ready");
        tokio::time::timeout(Duration::from_secs(10), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    fn release_turn(&self, query: &str) {
        std::fs::write(self.turn_gate(query, "release"), b"release native turn").unwrap();
    }
}
fn read_launches(path: &Path) -> Vec<Value> {
    match std::fs::read(path) {
        Ok(bytes) => {
            // The peer publishes a JSONL record by completing its newline.
            // An unfinished tail is not a record; malformed complete records fail.
            let Some(end) = bytes.iter().rposition(|byte| *byte == b'\n') else {
                return Vec::new();
            };
            bytes[..=end]
                .split_inclusive(|byte| *byte == b'\n')
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect()
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => panic!("fixture launch read {:?}", e.kind()),
    }
}

#[test]
fn launch_journal_snapshot_waits_for_complete_jsonl_records() {
    use std::io::Write;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("launches.jsonl");
    assert!(read_launches(&path).is_empty());
    let mut writer = std::fs::OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(&path)
        .unwrap();
    let first = json!({"args":["exec"],"query":"first"});
    let mut committed = serde_json::to_vec(&first).unwrap();
    committed.push(b'\n');
    writer.write_all(&committed).unwrap();
    let second = json!({"args":["exec"],"query":"açāo"});
    let bytes = serde_json::to_vec(&second).unwrap();
    let utf8 = bytes.iter().position(|byte| *byte == 0xc3).unwrap();
    writer.write_all(&bytes[..5]).unwrap();
    assert_eq!(read_launches(&path), vec![first.clone()]);
    writer.write_all(&bytes[5..=utf8]).unwrap();
    assert_eq!(read_launches(&path), vec![first.clone()]);
    writer.write_all(&bytes[utf8 + 1..]).unwrap();
    // Even a complete JSON value is unpublished until its JSONL delimiter.
    assert_eq!(read_launches(&path), vec![first.clone()]);
    writer.write_all(b"\n").unwrap();
    assert_eq!(read_launches(&path), vec![first, second]);
}

#[test]
fn launch_journal_snapshot_rejects_invalid_complete_records() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("launches.jsonl");
    for bytes in [
        b"{invalid}\n".as_slice(),
        b"{\"query\":\"\xff\"}\n".as_slice(),
        b"\n".as_slice(),
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(std::panic::catch_unwind(|| read_launches(&path)).is_err());
    }
}

fn native_test(test: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
                .block_on(test)
        })
        .unwrap()
        .join()
        .unwrap()
}
fn args(operation: &str, id: &str, query: &str) -> Value {
    json!({"ai_optimized":false,"operation":operation,"provider":"broker","conversation_id":id,"query":query,"selected_slugs":["Paris"],"model":"fixture-model"})
}
fn expiry() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600
}
async fn rpc_client(uri: &str, raw: &str) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    rpc_client_with_handler(uri, raw, ()).await
}
async fn rpc_client_with_handler<H: rmcp::ClientHandler>(
    uri: &str,
    raw: &str,
    handler: H,
) -> rmcp::service::RunningService<rmcp::RoleClient, H> {
    let mut headers = HashMap::new();
    let mut bearer = header::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
    bearer.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, bearer);
    handler
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::builder().no_proxy().build().unwrap(),
                StreamableHttpClientTransportConfig::with_uri(uri.to_owned())
                    .custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::LATEST],
            },
        )
        .await
        .unwrap()
}
// Admission, selected sources and the native handshake share this real request
// deadline. Give them room on CI before testing expiration of a blocked turn.
const BLOCKED_TURN_DEADLINE_MS: u64 = 10_000;

#[derive(Clone)]
struct ProgressWitness {
    markers: &'static [&'static str],
    observed: Arc<tokio::sync::Notify>,
}
impl ProgressWitness {
    fn new(markers: &'static [&'static str]) -> Self {
        Self {
            markers,
            observed: Arc::new(tokio::sync::Notify::new()),
        }
    }
    async fn wait(&self, scenario: &str) {
        tokio::time::timeout(Duration::from_secs(15), self.observed.notified())
            .await
            .unwrap_or_else(|_| panic!("{scenario}: expected native progress before completion"));
    }
}
impl rmcp::ClientHandler for ProgressWitness {
    async fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        _: rmcp::service::NotificationContext<rmcp::RoleClient>,
    ) {
        if params
            .message
            .as_deref()
            .is_some_and(|message| self.markers.iter().all(|marker| message.contains(marker)))
        {
            self.observed.notify_one();
        }
    }
}
fn assert_blocked_turn_expired(result: &rmcp::model::CallToolResult) {
    assert_eq!(result.is_error, Some(true));
    if let Some(payload) = &result.structured_content {
        // The native worker can return at the same instant as the wire timer.
        assert_eq!(payload["delivery_error"], "CancelledOrExpired");
        assert!(payload["receipt"]["completion"].get("answer").is_none());
        assert!(payload["receipt"].get("native_result").is_none());
        assert!(payload["receipt"].get("native_result_json").is_none());
        assert!(payload["persistence"].get("conversation").is_none());
    } else {
        assert_eq!(
            result
                .content
                .first()
                .and_then(|block| block.as_text())
                .map(|text| text.text.as_str()),
            Some("Knowledge conversation deadline")
        );
    }
}
#[test]
fn trusted_factory_optin_defaults_off_and_incoming_create_resume_reopen_is_private() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f.start(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        let client = rpc_client(&uri, &raw).await;
        assert!(
            !client
                .list_tools(None)
                .await
                .unwrap()
                .tools
                .iter()
                .any(|t| t.name == "graphit_knowledge_conversation")
        );
        assert!(!f.paths.root().join("users").exists());
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        let owner = f.start(&broker, Some(json!(true))).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &raw).await;
        assert!(
            client
                .list_tools(None)
                .await
                .unwrap()
                .tools
                .iter()
                .any(|t| t.name == "graphit_knowledge_conversation")
        );
        let cold = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(cold.is_error, Some(false));
        assert_eq!(cold.structured_content.unwrap()["conversations"], json!([]));
        assert!(!f.paths.root().join("users").exists());
        assert_eq!(broker.state.calls.load(Ordering::SeqCst), 0);
        for operation in ["create", "continue", "continue"] {
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args(operation, "graphit-A", "literal question β")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(
                result.is_error,
                Some(false),
                "finite response {:?}",
                result.structured_content
            );
            let payload = result.structured_content.unwrap();
            assert_eq!(payload["persistence"]["status"], "persisted");
            assert_eq!(
                payload["persistence"]["conversation"]["conversation_id"],
                "graphit-A"
            );
            assert!(!payload.to_string().contains("private-fixture-native"));
            assert!(!payload.to_string().contains("thread_id"));
        }
        assert_eq!(broker.state.calls.load(Ordering::SeqCst), 3);
        let launches = f.launches();
        assert_eq!(launches.len(), 3);
        assert!(!launches[0]["args"].to_string().contains("resume"));
        assert!(launches[1]["args"].to_string().contains("resume"));
        assert!(
            launches[1]["args"]
                .to_string()
                .contains("private-fixture-native")
        );
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        let owner = f.start(&broker, Some(json!(true))).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &raw).await;
        let result = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("continue", "graphit-A", "after restart")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        assert_eq!(broker.state.calls.load(Ordering::SeqCst), 4);
        let other = rpc_client(&uri, &broker.state.token("caller-B", RESOURCE, expiry())).await;
        let listed = other
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            listed.structured_content.unwrap()["conversations"],
            json!([])
        );
        let missing = other
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("continue", "graphit-A", "foreign")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(missing.is_error, Some(true));
        assert_eq!(f.launches().len(), 4);
        broker.state.allowed.store(false, Ordering::SeqCst);
        let denied = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("continue", "graphit-A", "denied now")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(denied.is_error, Some(true));
        assert_eq!(f.launches().len(), 4);
        let before = broker.state.calls.load(Ordering::SeqCst);
        let deleted = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    json!({"ai_optimized":false,"operation":"delete","provider":"broker","conversation_id":"graphit-A"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(deleted.is_error, Some(false));
        assert_eq!(broker.state.calls.load(Ordering::SeqCst), before);
        other.cancel().await.unwrap();
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    })
}

#[test]
fn incoming_denials_native_failures_and_binding_change_keep_real_receipts() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f.start(&broker, Some(json!(true))).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        let client = rpc_client(&uri, &raw).await;
        for raw in [
            "invalid-fixture-bearer".to_owned(),
            broker.state.token("caller-A", "wrong-purpose", expiry()),
            broker.state.token("caller-A", RESOURCE, 1),
        ] {
            let invalid = rpc_client(&uri, &raw).await;
            let result = invalid
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("create", "denied", "denied")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            assert!(f.launches().is_empty());
            assert!(!f.paths.root().join("users").exists());
            invalid.cancel().await.unwrap();
        }
        for mut input in [
            json!({"operation":"list","provider":"not-configured"}),
            json!({"operation":"list","provider":"broker","native_id":"private"}),
            json!({"operation":"delete","provider":"broker"}),
            json!({"operation":"continue","provider":"broker","conversation_id":"x","query":"x","selected_slugs":[],"model":"fixture-model","project_version":null}),
        ] {
            // Tool payloads cannot choose a native ID or authenticated subject.
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation")
                        .with_arguments(input.as_object_mut().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            assert!(f.launches().is_empty());
        }
        let created = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("create", "current", "initial")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(created.is_error, Some(false));
        let before = f.launches().len();
        let duplicate = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("create", "current", "duplicate")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(duplicate.is_error, Some(true));
        assert_eq!(f.launches().len(), before);
        let mut changed = args("continue", "current", "changed model");
        changed["model"] = json!("different-model");
        let denied = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation")
                    .with_arguments(changed.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(denied.is_error, Some(true));
        assert!(
            serde_json::to_string(&denied)
                .unwrap()
                .contains("BindingChanged")
        );
        assert_eq!(f.launches().len(), before);
        let identity = graphit_core::artifact_store::UserIdentity::new(
            broker.state.origin.clone(),
            "caller-A".into(),
        )
        .unwrap();
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(b"current")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let record = f
            .paths
            .root()
            .join("users")
            .join(identity.namespace_key())
            .join(format!("graphit-ai-conversation-{digest}.json"));
        let bytes = std::fs::read(&record).unwrap();
        std::fs::write(&record, b"invalid existing record").unwrap();
        let corrupt = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("continue", "current", "corrupt record")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(corrupt.is_error, Some(true));
        assert!(
            serde_json::to_string(&corrupt)
                .unwrap()
                .contains("Knowledge conversation: InvalidRecord")
        );
        assert_eq!(f.launches().len(), before);
        std::fs::write(&record, bytes).unwrap();
        for (operation, id, query) in [
            ("continue", "current", "mismatch"),
            ("create", "missing", "missing"),
            ("create", "malformed", "malformed"),
            ("create", "nonzero", "nonzero"),
        ] {
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation")
                        .with_arguments(args(operation, id, query).as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            let payload = result.structured_content.unwrap();
            assert!(payload.get("receipt").is_some());
            assert!(payload["receipt"].get("process").is_some());
            assert!(payload["receipt"].get("usage").is_some());
            assert!(!payload.to_string().contains("different-private-native"));
            assert!(!payload.to_string().contains("private-fixture-native"));
            assert!(payload["receipt"]["completion"].get("answer").is_none());
            if query == "mismatch" || query == "missing" {
                assert_eq!(payload["persistence"]["status"], "failed");
            }
        }
        let listed = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        let records = listed.structured_content.unwrap();
        let ids: Vec<_> = records["conversations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["conversation_id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"current"));
        assert!(!ids.contains(&"missing"));
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn actual_selected_digest_changes_deny_resume_but_unrelated_publication_keeps_it() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        for phase in 0..3 {
            if phase > 0 {
                f.republish(if phase == 2 {
                    DocumentChange::Selected
                } else {
                    DocumentChange::Unrelated
                })
                .await;
            }
            let owner = f.start(&broker, Some(json!(true))).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let operation = if phase == 0 { "create" } else { "continue" };
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args(operation, "digest-bound", "selected source")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(phase == 2));
            if phase == 2 {
                assert!(
                    serde_json::to_string(&result)
                        .unwrap()
                        .contains("BindingChanged")
                );
            } else {
                assert_eq!(
                    result.structured_content.as_ref().unwrap()["persistence"]["status"],
                    "persisted",
                );
            }
            if phase > 0 {
                assert_eq!(f.launches().len(), 2);
            }
            client.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
        }
        assert_eq!(broker.state.calls.load(Ordering::SeqCst), 3);
        broker.shutdown().await;
    });
}

#[test]
fn actual_incoming_partial_permissions_and_runtime_credential_deny_before_user_birth() {
    incoming_partial_permissions(NativeBinding::Codex);
}
#[test]
fn claude_incoming_partial_permissions_and_runtime_credential_deny_before_user_birth() {
    incoming_partial_permissions(NativeBinding::Claude);
}
fn incoming_partial_permissions(binding: NativeBinding) {
    native_test(async move {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        if binding == NativeBinding::Claude {
            // A controlled sentinel only: every call below must deny before Process.
            std::fs::copy(f.temp.path().join("codex"), f.temp.path().join("claude")).unwrap();
        }
        let mut clients = None;
        for (permissions, can_list) in [
            (
                vec![Permission::ReadUser, Permission::Read(Module::Knowledge)],
                true,
            ),
            (
                vec![
                    Permission::MutateUser,
                    Permission::MutateModule(Module::Knowledge),
                ],
                false,
            ),
            (
                vec![
                    Permission::ReadUser,
                    Permission::Read(Module::Memory),
                    Permission::WriteMemory,
                ],
                false,
            ),
        ] {
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let scope = ScopeLimits::new(16, 4096, 256).unwrap();
            let mut startup = seed_policy(scope);
            startup.authority = startup
                .authority
                .with_incoming_user_permissions("broker", &permissions)
                .unwrap();
            let auth = Arc::new(
                DaemonAuth::start(&f.paths, std::slice::from_ref(&f.project), startup, &ctx)
                    .unwrap(),
            );
            let raw = {
                let key = auth.runtime().check(ctx.snapshot()).unwrap();
                zeroize::Zeroizing::new(
                    String::from_utf8(key.secret(ctx.snapshot()).unwrap().to_vec()).unwrap(),
                )
            };
            if clients.is_none() {
                let issuer = auth.issuer();
                let caller = issuer.authenticate_runtime(&raw, &ctx).unwrap();
                let admin = issuer.bootstrap_admin(&caller).unwrap();
                let parent = issuer.admin_parent(&caller, &admin).unwrap();
                let manager = NativeProfileManager::new(issuer);
                let snapshot = manager
                    .begin(ProfileManagerAuthority::Admin(&parent))
                    .unwrap();
                let record = snapshot.providers().find(|r| r.name() == "broker").unwrap();
                let ProviderKind::Broker(service) = &record.config().kind else {
                    unreachable!()
                };
                let json = JsonLimits {
                    bytes: 65536,
                    nodes: 4096,
                    depth: 32,
                    scalar_bytes: 4096,
                    collection_entries: 64,
                };
                let network = NetworkLimits {
                    request_body_bytes: 65536,
                    response_body_bytes: 65536,
                    url_bytes: 4096,
                    header_bytes: 65536,
                    header_count: 64,
                    max_in_flight: 4,
                    max_idle_per_host: 1,
                    idle_timeout: Duration::from_secs(30),
                    max_certificates: 1,
                    max_certificate_bytes: 65536,
                };
                let verifier = Arc::new(
                    BrokerVerifier::new(
                        BrokerPolicy {
                            endpoint: &service.endpoint,
                            mcp_resource: service.mcp_resource.as_deref(),
                            provider_revision: record.revision(),
                            network: ClientPolicy {
                                origin: &service.endpoint,
                                allow_loopback_http: true,
                                proxy: None,
                                additional_roots: &[],
                            },
                        },
                        network,
                        VerifierLimits {
                            token_bytes: 4096,
                            json,
                            max_keys: 64,
                            metadata_ttl: Duration::from_secs(300),
                            key_ttl: Duration::from_secs(300),
                            unknown_key_cooldown: Duration::from_secs(1),
                        },
                    )
                    .unwrap(),
                );
                let endpoint = format!("{}/v1/project/access/authorize", service.endpoint);
                let backend = Arc::new(
                    BrokerProjectAccess::new(
                        ProjectAccessPolicy {
                            endpoint: &endpoint,
                            network: ClientPolicy {
                                origin: &service.endpoint,
                                allow_loopback_http: true,
                                proxy: None,
                                additional_roots: &[],
                            },
                        },
                        network,
                        json,
                    )
                    .unwrap(),
                );
                clients = Some((vec![(record.name().to_owned(), verifier)], backend));
            }
            let retained = RetainedBytesLimits::new(65536, 32, 4096).unwrap();
            let copy = CopyLimits::new(1048576, 128, 65536, 8).unwrap();
            let config = ConfigSnapshot::new(f.paths.brand(), []).unwrap();
            let policy = SourcePolicy {
                graph_query: graphit_ast::icebug_query::QueryOptions {
                    buffer_pool_bytes: 128 * 1024 * 1024,
                    query_threads: 2,
                    max_rows: 4096,
                    max_bytes: 8388608,
                    max_depth: 64,
                },
                installation: f.paths.clone(),
                native_install: NativeInstallPolicy::capture(&config, ctx.snapshot()).unwrap(),
                content: ContentLimits {
                    max_input_bytes: 1048576,
                    max_output_bytes: 8388608,
                    max_depth: 64,
                    max_nodes: 65536,
                    max_scalar_bytes: 1048576,
                    max_collection_entries: 4096,
                },
                timeout: Duration::from_secs(30),
                scope,
                file: SnapshotPolicy::new(
                    CopyPolicy::RejectFileLinks,
                    copy,
                    RetainedFileSet::new([], retained).unwrap(),
                    retained,
                )
                .unwrap(),
                read: SourceReadLimits::new(65536, 4096, 1048576).unwrap(),
                path: retained,
                index: native(),
                binding,
                registry: HubRegistry::global(
                    &f.paths,
                    &f.installation_id,
                    HubResourcePolicy::resolve(&config).unwrap().hub(),
                )
                .unwrap(),
            };
            let checkout =
                GitCheckout::inspect(f.project.root(), Duration::from_secs(5), &cancel).unwrap();
            let version =
                VersionContext::select(&f.project, checkout.as_checkout(), "", None, "").unwrap();
            let project_id = ProjectIdentityStore::at(&f.project)
                .load()
                .unwrap()
                .unwrap()
                .id();
            let projects = Arc::new(vec![(f.project.clone(), project_id, version)]);
            let mut server = SourceServer::new(
                Arc::clone(&auth),
                projects,
                0,
                policy,
                CancellationToken::new(),
            )
            .unwrap();
            let (providers, backend) = clients.as_ref().unwrap();
            server
                .attach_knowledge_conversations(ConversationAccess {
                    providers: providers.clone(),
                    backend: Arc::clone(backend),
                    limits: ConversationLimits {
                        artifacts: ArtifactLimits::new(65536, 4096, copy).unwrap(),
                        native: native(),
                        record_bytes: n(65536),
                        field_bytes: n(4096),
                        sources: n(64),
                        list_records: n(64),
                    },
                })
                .unwrap();
            server
                .attach_ask(
                    &config,
                    vec![("PATH".into(), f.temp.path().as_os_str().to_owned())],
                    &ctx,
                )
                .await
                .unwrap();
            let listener = tcp_listener::reserve(
                Options {
                    address: std::net::Ipv4Addr::LOCALHOST.into(),
                    ports: PortSelection::Ephemeral,
                    timeout: Duration::from_secs(5),
                },
                &cancel,
            )
            .unwrap();
            let uri = format!("http://{}/mcp", listener.address());
            let serving = server.clone();
            let run = tokio::spawn(async move { serving.serve(listener).await });
            let client =
                rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
            let listed = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(listed.is_error, Some(!can_list));
            if can_list {
                assert_eq!(
                    listed.structured_content.unwrap()["conversations"],
                    json!([])
                );
            }
            let denied = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("create", "partial", "denied permission")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(denied.is_error, Some(true));
            client.cancel().await.unwrap();
            let runtime = rpc_client(&uri, &raw).await;
            let native = runtime
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("create", "runtime", "native USER denied")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(native.is_error, Some(true));
            runtime.cancel().await.unwrap();
            assert!(f.launches().is_empty());
            assert!(!f.paths.root().join("users").exists());
            assert_eq!(broker.state.calls.load(Ordering::SeqCst), 0);
            server.shutdown().await.unwrap();
            run.await.unwrap().unwrap();
            let removal = auth.remove_runtime(ctx.snapshot());
            assert!(removal.failure().is_none() && removal.cleanup_failure().is_none());
        }
        broker.shutdown().await;
    });
}

#[test]
fn actual_recipe_or_captured_program_changes_deny_continuation_before_child() {
    native_test(async {
        for recipe in [true, false] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            let raw = broker.state.token("caller-A", RESOURCE, expiry());
            let owner = f.start(&broker, Some(json!(true))).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let created = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("create", "recipe-program", "initial")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(created.is_error, Some(false));
            client.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
            let directory = if recipe {
                f.republish(DocumentChange::Recipe).await;
                f.temp.path().to_path_buf()
            } else {
                let other = f.temp.path().join("other-program");
                std::fs::create_dir(&other).unwrap();
                std::fs::copy(f.temp.path().join("codex"), other.join("codex")).unwrap();
                other
            };
            let owner = f
                .start_with_options(&broker, Some(json!(true)), &directory, None)
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let denied = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("continue", "recipe-program", "changed binding")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(denied.is_error, Some(true));
            assert!(
                serde_json::to_string(&denied)
                    .unwrap()
                    .contains("BindingChanged")
            );
            assert_eq!(f.launches().len(), 1);
            assert_eq!(broker.state.calls.load(Ordering::SeqCst), 2);
            client.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
            broker.shutdown().await;
        }
    });
}

#[path = "knowledge_conversation/antigravity.rs"]
mod antigravity;
#[path = "knowledge_conversation/copilot.rs"]
mod copilot;
#[path = "knowledge_conversation/cursor.rs"]
mod cursor;
#[path = "knowledge_conversation/grok.rs"]
mod grok;
#[path = "knowledge_conversation/kimi.rs"]
mod kimi;
#[path = "knowledge_conversation/opencode.rs"]
mod opencode;
#[path = "knowledge_conversation/qwen.rs"]
mod qwen;

#[test]
fn actual_post_effect_authorization_changes_withhold_delivery_and_keep_receipt() {
    native_test(async {
        for physical in [false, true] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            let owner = f.start(&broker, Some(json!(true))).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let expires = if physical {
                expiry()
            } else {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    + 4
            };
            let raw = broker.state.token("caller-A", RESOURCE, expires);
            let client = rpc_client(&uri, &raw).await;
            let peer = client.peer().clone();
            let call = tokio::spawn(async move {
                peer.call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("create", "delivery-denied", "wait delivery denial")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap()
            });
            // The ready gate follows the peer's completed journal append.
            f.wait_for_blocked_turn("wait delivery denial").await;
            wait_for_launches(&f, 1).await;
            let moved = f.temp.path().join("moved-project");
            if physical {
                std::fs::rename(f.project.root(), &moved).unwrap();
            } else {
                let expiration = UNIX_EPOCH + Duration::from_secs(expires);
                tokio::time::sleep(
                    expiration
                        .duration_since(SystemTime::now())
                        .unwrap_or_default(),
                )
                .await;
            }
            f.release_turn("wait delivery denial");
            let result = call.await.unwrap();
            if physical {
                std::fs::rename(&moved, f.project.root()).unwrap();
            }
            assert_eq!(result.is_error, Some(true));
            let payload = result.structured_content.unwrap();
            assert!(payload.get("receipt").is_some());
            assert!(payload["receipt"].get("process").is_some());
            assert!(payload["receipt"]["completion"].get("answer").is_none());
            assert!(payload["persistence"].get("conversation").is_none());
            assert_eq!(payload["persistence"]["status"], "denied");
            assert_eq!(f.launches().len(), 1);
            client.cancel().await.unwrap();
            let fresh = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
            let list = fresh
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(list.is_error, Some(false));
            assert_eq!(list.structured_content.unwrap()["conversations"], json!([]));
            assert_eq!(broker.state.calls.load(Ordering::SeqCst), 1);
            fresh.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
            broker.shutdown().await;
        }
    });
}

#[test]
fn actual_original_deadline_during_execution_denies_and_joins_before_unlock() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f
            .start_with_options(
                &broker,
                Some(json!(true)),
                f.temp.path(),
                Some(BLOCKED_TURN_DEADLINE_MS),
            )
            .await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let progress = ProgressWitness::new(&["turn.started"]);
        let client = rpc_client_with_handler(
            &uri,
            &broker.state.token("caller-A", RESOURCE, expiry()),
            progress.clone(),
        )
        .await;
        let mut params = CallToolRequestParams::new("graphit_knowledge_conversation")
            .with_arguments(
                args("create", "deadline", "wait original deadline")
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        params.meta =
            Some(serde_json::from_value(json!({"progressToken":"codex-deadline"})).unwrap());
        let handle = client
            .peer()
            .send_cancellable_request(
                rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                    params,
                )),
                rmcp::service::PeerRequestOptions::no_options(),
            )
            .await
            .unwrap();
        progress.wait("Codex deadline").await;
        f.wait_for_blocked_turn("wait original deadline").await;
        let rmcp::model::ServerResult::CallToolResult(response) =
            handle.await_response().await.unwrap()
        else {
            panic!("deadline tool result")
        };
        assert_blocked_turn_expired(&response);
        assert_eq!(f.launches().len(), 1);
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        let identity = graphit_core::artifact_store::UserIdentity::new(
            broker.state.origin.clone(),
            "caller-A".into(),
        )
        .unwrap();
        let observed = Arc::new(graphit_core::file_io::SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let user = CapturedPublicationScope::user(
            &f.paths,
            f.installation_id.clone(),
            identity,
            ScopeLimits::new(16, 4096, 256).unwrap(),
            &observed,
        )
        .unwrap();
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(
                user.runtime_root().join("artifact-store.lock"),
            )
            .unwrap()
        );
        broker.shutdown().await;
    });
}

#[derive(Clone)]
struct ProgressCollector(Arc<std::sync::Mutex<Vec<String>>>);
impl rmcp::ClientHandler for ProgressCollector {
    async fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        _: rmcp::service::NotificationContext<rmcp::RoleClient>,
    ) {
        if let Some(message) = params.message {
            self.0.lock().unwrap().push(message);
        }
    }
}
async fn wait_for_launches(f: &Fixture, count: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while f.launches().len() < count {
            tokio::time::sleep(Duration::from_millis(10)).await
        }
    })
    .await
    .unwrap();
}
#[test]
fn actual_progress_is_private_and_post_effect_store_failure_keeps_receipt_and_writer_serialization()
{
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f.start(&broker, Some(json!(true))).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        let mut headers = HashMap::new();
        let mut bearer = header::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
        bearer.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, bearer);
        let progress = ProgressCollector(Arc::new(std::sync::Mutex::new(Vec::new())));
        let client = progress
            .clone()
            .serve_with_lifecycle(
                StreamableHttpClientTransport::with_client(
                    reqwest::Client::builder().no_proxy().build().unwrap(),
                    StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
                ),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![ProtocolVersion::LATEST],
                },
            )
            .await
            .unwrap();
        let mut params = CallToolRequestParams::new("graphit_knowledge_conversation")
            .with_arguments(
                args("create", "fault", "wait for store fault")
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        params.meta = Some(
            serde_json::from_value(json!({"progressToken":"private-progress-fixture"})).unwrap(),
        );
        let peer = client.peer().clone();
        let fault = tokio::spawn(async move { peer.call_tool(params).await.unwrap() });
        wait_for_launches(&f, 1).await;
        let identity = graphit_core::artifact_store::UserIdentity::new(
            broker.state.origin.clone(),
            "caller-A".into(),
        )
        .unwrap();
        let user_root = f.paths.root().join("users").join(identity.namespace_key());
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(b"fault")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let target = user_root.join(format!("graphit-ai-conversation-{digest}.json"));
        std::fs::write(&target, b"foreign-owned-destination").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        f.release_turn("wait for store fault");
        let result = fault.await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["persistence"]["status"], "failed");
        assert!(payload["receipt"].get("process").is_some());
        assert_eq!(payload["receipt"]["usage"]["provenance"], "observed");
        assert!(payload["receipt"]["completion"].get("answer").is_none());
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"foreign-owned-destination"
        );
        let messages = progress.0.lock().unwrap().clone();
        assert!(!messages.is_empty());
        for message in &messages {
            assert!(!message.contains("thread.started"));
            assert!(!message.contains("private-fixture-native"));
            assert!(!message.contains("thread_id"));
        }
        assert!(messages.iter().any(|m| m.contains("turn.completed")));
        std::fs::remove_file(target).unwrap();
        let base = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("create", "serial", "initial")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(base.is_error, Some(false));
        let before = f.launches().len();
        let peer = client.peer().clone();
        let first = tokio::spawn(async move {
            peer.call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("continue", "serial", "wait first")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap()
        });
        wait_for_launches(&f, before + 1).await;
        f.wait_for_blocked_turn("wait first").await;
        let authorized = broker.state.calls.load(Ordering::SeqCst);
        let peer = client.peer().clone();
        let second = tokio::spawn(async move {
            peer.call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("continue", "serial", "second")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap()
        });
        broker.state.wait_for_calls(authorized + 1).await;
        assert!(!first.is_finished());
        assert_eq!(
            f.launches().len(),
            before + 1,
            "same USER writer forbids simultaneous child"
        );
        f.release_turn("wait first");
        assert_eq!(first.await.unwrap().is_error, Some(false));
        assert_eq!(second.await.unwrap().is_error, Some(false));
        assert_eq!(f.launches().len(), before + 2);
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn expired_grant_or_original_deadline_after_user_writer_wait_denies_before_child() {
    native_test(async {
        for deadline in [false, true] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            let owner = f
                .start_with_options(
                    &broker,
                    Some(json!(true)),
                    f.temp.path(),
                    deadline.then_some(1500),
                )
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let token_expiry = if deadline { expiry() } else { now + 3 };
            let raw = broker.state.token("caller-A", RESOURCE, token_expiry);
            let client = rpc_client(&uri, &raw).await;
            // Birth and hold the exact caller's actual Core USER writer, without a conversation.
            use graphit_core::artifact_store::{ArtifactStore, UserIdentity};
            let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
                Instant::now() + Duration::from_secs(30),
                Cancellation::default(),
            ));
            // The installation identity is durable SDK data, never an access grant.
            let installation_id = f.installation_id.clone();
            let user = CapturedPublicationScope::user(
                &f.paths,
                installation_id,
                UserIdentity::new(broker.state.origin.clone(), "caller-A".into()).unwrap(),
                ScopeLimits::new(16, 4096, 256).unwrap(),
                &original,
            )
            .unwrap();
            let copy = CopyLimits::new(1048576, 128, 65536, 16).unwrap();
            let store = ArtifactStore::new(
                user.clone(),
                ArtifactLimits::new(65536, 4096, copy).unwrap(),
            );
            let held = store.writer_with_context(&original).unwrap();
            let peer = client.peer().clone();
            let call = tokio::spawn(async move {
                peer.call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("create", "expired", "wait authorization")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap()
            });
            tokio::time::timeout(Duration::from_secs(5), async {
                while broker.state.calls.load(Ordering::SeqCst) == 0 {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
            })
            .await
            .unwrap();
            let writer_path = user.runtime_root().join("artifact-store.lock");
            let result = if deadline {
                // Keep the writer held until the request's own deadline wins.
                // The wire timer and expired worker can become ready together.
                let result = tokio::time::timeout(Duration::from_secs(10), call)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(graphit_core::file_lock::FileLock::is_locked(&writer_path).unwrap());
                drop(held);
                result
            } else {
                let expires = UNIX_EPOCH + Duration::from_secs(token_expiry);
                tokio::time::sleep(
                    expires
                        .duration_since(SystemTime::now())
                        .unwrap_or_default(),
                )
                .await;
                assert!(
                    !call.is_finished(),
                    "the live request must still be waiting for the writer"
                );
                drop(held);
                tokio::time::timeout(Duration::from_secs(10), call)
                    .await
                    .unwrap()
                    .unwrap()
            };
            assert!(f.launches().is_empty());
            assert_eq!(result.is_error, Some(true));
            let error_text = result
                .content
                .first()
                .and_then(|block| block.as_text())
                .map(|text| text.text.as_str());
            if deadline {
                if let Some(payload) = &result.structured_content {
                    assert_eq!(payload["status"], "failed");
                    assert_eq!(payload["delivery_error"], "CancelledOrExpired");
                    assert_eq!(
                        payload["error_content"][0]["text"],
                        "Knowledge conversation: Store"
                    );
                    assert!(payload.get("receipt").is_none());
                } else {
                    assert_eq!(error_text, Some("Knowledge conversation deadline"));
                }
            } else {
                assert_eq!(
                    error_text,
                    Some(
                        "Knowledge conversation authorization changed while waiting for USER writer"
                    )
                );
            }
            client.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
            assert!(f.launches().is_empty());
            assert!(!graphit_core::file_lock::FileLock::is_locked(writer_path).unwrap());
            broker.shutdown().await;
        }
    });
}

#[test]
fn actual_http_request_cancel_keeps_worker_and_user_writer_until_process_finishes() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f.start(&broker, Some(json!(true))).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        let client = rpc_client(&uri, &raw).await;
        let created = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("create", "http-cancel", "initial")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(created.is_error, Some(false));
        let before = f.launches().len();
        let handle = client
            .peer()
            .send_cancellable_request(
                rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("continue", "http-cancel", "cancel-http")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )),
                rmcp::service::PeerRequestOptions::no_options(),
            )
            .await
            .unwrap();
        wait_for_launches(&f, before + 1).await;
        let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let user = CapturedPublicationScope::user(
            &f.paths,
            f.installation_id.clone(),
            graphit_core::artifact_store::UserIdentity::new(
                broker.state.origin.clone(),
                "caller-A".into(),
            )
            .unwrap(),
            ScopeLimits::new(16, 4096, 256).unwrap(),
            &original,
        )
        .unwrap();
        let writer_path = user.runtime_root().join("artifact-store.lock");
        assert!(graphit_core::file_lock::FileLock::is_locked(&writer_path).unwrap());
        // This sends the actual MCP cancellation notification; dropping a local future does not.
        handle
            .cancel(Some("cancel actual fixture request".into()))
            .await
            .unwrap();
        let term = f.temp.path().join("http-cancel-term");
        tokio::time::timeout(Duration::from_secs(10), async {
            while !term.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(graphit_core::file_lock::FileLock::is_locked(&writer_path).unwrap());
        let calls = broker.state.calls.load(Ordering::SeqCst);
        let peer = client.peer().clone();
        let second = tokio::spawn(async move {
            peer.call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("continue", "http-cancel", "after cancellation")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap()
        });
        broker.state.wait_for_calls(calls + 1).await;
        assert!(graphit_core::file_lock::FileLock::is_locked(&writer_path).unwrap());
        assert_eq!(
            f.launches().len(),
            before + 1,
            "no second child while the canceled request worker still owns the USER writer"
        );
        std::fs::write(
            f.temp.path().join("http-cancel-release"),
            b"release actual process",
        )
        .unwrap();
        let resumed = second.await.unwrap();
        assert_eq!(resumed.is_error, Some(false));
        let payload = resumed.structured_content.unwrap();
        assert_eq!(payload["persistence"]["status"], "persisted");
        assert!(payload["receipt"].get("process").is_some());
        assert!(!payload.to_string().contains("private-fixture-native"));
        assert_eq!(f.launches().len(), before + 2);
        assert!(!graphit_core::file_lock::FileLock::is_locked(&writer_path).unwrap());
        client.cancel().await.unwrap();
        cancel.cancel();
        // Public shutdown waits for the retained jobs and the actual Host; cancel's return was not cleanup proof.
        running.await.unwrap().unwrap();
        assert!(!graphit_core::file_lock::FileLock::is_locked(&writer_path).unwrap());
        broker.shutdown().await;
    });
}

#[test]
fn kiro_v3_actual_create_continue_reopen_is_private_and_selected_binding_is_fresh() {
    native_test(async {
        for binding in [NativeBinding::KiroV3] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            let raw = broker.state.token("caller-A", RESOURCE, expiry());
            let owner = f.start_kiro(&broker, binding, "answer", None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            for operation in ["create", "continue"] {
                let result = client
                    .call_tool(
                        CallToolRequestParams::new("graphit_knowledge_conversation")
                            .with_arguments(
                                args(operation, "kiro-A", "literal question β")
                                    .as_object()
                                    .unwrap()
                                    .clone(),
                            ),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    result.is_error,
                    Some(false),
                    "finite fixture response {:?}",
                    result.structured_content
                );
                let payload = result.structured_content.unwrap();
                assert_eq!(
                    payload["receipt"]["completion"]["answer"],
                    "fresh fixture answer"
                );
                assert_eq!(payload["receipt"]["process"]["joined"], true);
                assert_eq!(payload["persistence"]["status"], "persisted");
                assert_eq!(
                    payload["persistence"]["conversation"]["conversation_id"],
                    "kiro-A"
                );
                assert!(!payload.to_string().contains("private-kiro-fixture-session"));
                assert!(!payload.to_string().contains("old history"));
            }
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            f.republish(DocumentChange::Unrelated).await;
            let owner = f.start_kiro(&broker, binding, "answer", None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("continue", "kiro-A", "after restart")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(false));
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("continue", "kiro-A", "unrelated changed")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(false));
            let events = f.kiro_events();
            assert_eq!(events.iter().filter(|e| e.as_str() == "new").count(), 1);
            assert_eq!(events.iter().filter(|e| e.as_str() == "load").count(), 3);
            assert_eq!(events.iter().filter(|e| e.as_str() == "prompt").count(), 4);
            assert_eq!(events.iter().filter(|e| e.as_str() == "select").count(), 4);
            let count = events.len();
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            f.republish(DocumentChange::Selected).await;
            let owner = f.start_kiro(&broker, binding, "answer", None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("continue", "kiro-A", "selected changed")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            assert_eq!(f.kiro_events().len(), count);
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            broker.shutdown().await;
        }
    });
}

#[test]
fn kiro_partial_retains_real_session_and_load_failure_never_falls_back_to_new() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        for mode in ["partial", "load_error", "answer"] {
            let owner = f
                .start_kiro(&broker, NativeBinding::KiroV3, mode, None)
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let operation = if mode == "partial" {
                "create"
            } else {
                "continue"
            };
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args(operation, "partial", "literal question")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            let payload = result.structured_content.unwrap();
            if mode == "answer" {
                assert_eq!(result.is_error, Some(false));
                assert_eq!(
                    payload["receipt"]["completion"]["answer"],
                    "fresh fixture answer"
                );
            } else {
                assert_eq!(result.is_error, Some(true));
                assert!(payload["receipt"]["completion"].get("answer").is_none());
                if mode == "partial" {
                    assert_eq!(payload["persistence"]["status"], "persisted");
                } else {
                    assert_eq!(payload["receipt"]["load_response_observed"], false);
                }
            }
            assert!(!payload.to_string().contains("private-kiro-fixture-session"));
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
        }
        let events = f.kiro_events();
        assert_eq!(events.iter().filter(|e| e.as_str() == "new").count(), 1);
        assert_eq!(events.iter().filter(|e| e.as_str() == "load").count(), 2);
        assert_eq!(events.iter().filter(|e| e.as_str() == "prompt").count(), 2);
        broker.shutdown().await;
    });
}

#[test]
fn kiro_native_frame_stderr_and_deadline_errors_do_not_deliver_partial_answers() {
    native_test(async {
        for mode in ["large_frame", "stderr_limit", "cancel"] {
            let deadline = mode == "cancel";
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            let owner = f
                .start_kiro(
                    &broker,
                    NativeBinding::KiroV3,
                    mode,
                    deadline.then_some(BLOCKED_TURN_DEADLINE_MS),
                )
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let raw = broker.state.token("caller-A", RESOURCE, expiry());
            let progress = ProgressWitness::new(&["fresh fixture answer"]);
            let client = rpc_client_with_handler(&uri, &raw, progress.clone()).await;
            let mut params = CallToolRequestParams::new("graphit_knowledge_conversation")
                .with_arguments(
                    args("create", mode, "bounded question")
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            let result = if deadline {
                params.meta = Some(
                    serde_json::from_value(json!({"progressToken":"kiro-deadline-progress"}))
                        .unwrap(),
                );
                let handle = client
                    .peer()
                    .send_cancellable_request(
                        rmcp::model::ClientRequest::CallToolRequest(
                            rmcp::model::CallToolRequest::new(params),
                        ),
                        rmcp::service::PeerRequestOptions::no_options(),
                    )
                    .await
                    .unwrap();
                // The fixture leaves the prompt pending after sending real
                // progress; do not accept a timeout during setup.
                progress.wait("Kiro deadline").await;
                let rmcp::model::ServerResult::CallToolResult(result) =
                    handle.await_response().await.unwrap()
                else {
                    panic!("Kiro deadline tool result")
                };
                assert_blocked_turn_expired(&result);
                result
            } else {
                client.call_tool(params).await.unwrap()
            };
            assert_eq!(result.is_error, Some(true), "{mode}: {result:?}");
            assert!(
                !serde_json::to_string(&result)
                    .unwrap()
                    .contains("private-kiro-fixture-session")
            );
            if let Some(payload) = &result.structured_content {
                assert!(payload["receipt"]["completion"].get("answer").is_none());
            }
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            let events = f.kiro_events();
            assert!(events.iter().any(|e| e == "prompt"), "{mode}: {events:?}");
            if deadline {
                assert!(
                    !events.iter().any(|e| e == "terminal"),
                    "{mode}: {events:?}"
                );
                use sha2::{Digest, Sha256};
                let digest = Sha256::digest(mode.as_bytes())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                let user = kiro_clear_user(&f, &broker);
                assert!(
                    !user
                        .user_root()
                        .unwrap()
                        .join(format!("graphit-ai-conversation-{digest}.json"))
                        .exists()
                );
                assert!(
                    !graphit_core::file_lock::FileLock::is_locked(
                        user.runtime_root().join("artifact-store.lock")
                    )
                    .unwrap()
                );
            }
            broker.shutdown().await;
        }
    });
}

#[test]
fn kiro_actual_user_isolation_and_model_mismatch_deny_before_native_load() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f
            .start_kiro(&broker, NativeBinding::KiroV3, "answer", None)
            .await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let stop = Cancellation::default();
        let stopped = stop.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let created = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("create", "bound", "initial")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(created.is_error, Some(false));
        let before = f.kiro_events().len();
        let other = rpc_client(&uri, &broker.state.token("caller-B", RESOURCE, expiry())).await;
        let denied = other
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    args("continue", "bound", "different caller")
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(denied.is_error, Some(true));
        let listed = other
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            listed.structured_content.unwrap()["conversations"],
            json!([])
        );
        let mut changed = args("continue", "bound", "changed model");
        changed["model"] = json!("different literal model");
        let denied = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation")
                    .with_arguments(changed.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(denied.is_error, Some(true));
        assert!(
            serde_json::to_string(&denied)
                .unwrap()
                .contains("BindingChanged")
        );
        assert_eq!(f.kiro_events().len(), before);
        other.cancel().await.unwrap();
        client.cancel().await.unwrap();
        stop.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn kiro_actual_progress_precedes_terminal_and_post_effect_store_failure_denies_delivery() {
    native_test(async {
        for physical in [false, true] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            let owner = f
                .start_kiro(&broker, NativeBinding::KiroV3, "stream_gate", None)
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let mut headers = HashMap::new();
            let raw = broker.state.token("caller-A", RESOURCE, expiry());
            let mut bearer = header::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
            bearer.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, bearer);
            let progress = ProgressCollector(Arc::new(std::sync::Mutex::new(Vec::new())));
            let client = progress
                .clone()
                .serve_with_lifecycle(
                    StreamableHttpClientTransport::with_client(
                        reqwest::Client::builder().no_proxy().build().unwrap(),
                        StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
                    ),
                    ClientLifecycleMode::Discover {
                        preferred_versions: vec![ProtocolVersion::LATEST],
                    },
                )
                .await
                .unwrap();
            let mut params = CallToolRequestParams::new("graphit_knowledge_conversation")
                .with_arguments(
                    args("create", "kiro-fault", "actual progress")
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            params.meta = Some(
                serde_json::from_value(json!({"progressToken":"fixture-kiro-progress"})).unwrap(),
            );
            let peer = client.peer().clone();
            let execution = tokio::spawn(async move { peer.call_tool(params).await.unwrap() });
            tokio::time::timeout(Duration::from_secs(10), async {
                while !progress
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|m| m.contains("literal-tool-id"))
                {
                    assert!(!execution.is_finished(), "final before gated tool progress");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert!(!execution.is_finished());
            assert!(!f.kiro_events().iter().any(|e| e == "terminal"));
            let identity = graphit_core::artifact_store::UserIdentity::new(
                broker.state.origin.clone(),
                "caller-A".into(),
            )
            .unwrap();
            let user_root = f.paths.root().join("users").join(identity.namespace_key());
            let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
                Instant::now() + Duration::from_secs(30),
                Cancellation::default(),
            ));
            let user = CapturedPublicationScope::user(
                &f.paths,
                f.installation_id.clone(),
                identity,
                ScopeLimits::new(16, 4096, 256).unwrap(),
                &original,
            )
            .unwrap();
            let writer_path = user.runtime_root().join("artifact-store.lock");
            assert!(graphit_core::file_lock::FileLock::is_locked(&writer_path).unwrap());
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(b"kiro-fault")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let target = user_root.join(format!("graphit-ai-conversation-{digest}.json"));
            let moved = f.temp.path().join("moved-project");
            if physical {
                std::fs::rename(f.project.root(), &moved).unwrap();
            } else {
                std::fs::write(&target, b"foreign-owned-destination").unwrap();
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            std::fs::write(
                f.temp.path().join("kiro-journal").with_extension("finish"),
                b"finish",
            )
            .unwrap();
            let result = execution.await.unwrap();
            if physical {
                std::fs::rename(&moved, f.project.root()).unwrap();
            }
            assert_eq!(result.is_error, Some(true));
            let payload = result.structured_content.unwrap();
            assert_eq!(
                payload["persistence"]["status"],
                if physical { "denied" } else { "failed" }
            );
            assert_eq!(payload["receipt"]["new_response_observed"], true);
            if !physical {
                assert_eq!(payload["receipt"]["stop_reason"], "EndTurn");
            }
            assert_eq!(payload["receipt"]["process"]["joined"], true);
            assert!(payload["receipt"]["completion"].get("answer").is_none());
            if physical {
                assert!(!target.exists());
                let listed = client
                    .call_tool(
                        CallToolRequestParams::new("graphit_knowledge_conversation")
                            .with_arguments(
                            json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    listed.structured_content.unwrap()["conversations"],
                    json!([])
                );
            } else {
                assert_eq!(
                    std::fs::read(&target).unwrap(),
                    b"foreign-owned-destination"
                );
            }
            for message in progress.0.lock().unwrap().iter() {
                assert!(!message.contains("private-kiro-fixture-session"));
                assert!(!message.contains("session_id"));
            }
            assert!(!graphit_core::file_lock::FileLock::is_locked(&writer_path).unwrap());
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            broker.shutdown().await;
        }
    });
}

#[test]
fn kiro_actual_http_cancel_keeps_user_writer_until_native_child_wait_and_shutdown() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f
            .start_kiro(&broker, NativeBinding::KiroV3, "cancel_delay_exit", None)
            .await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let stop = Cancellation::default();
        let stopped = stop.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let handle = client
            .peer()
            .send_cancellable_request(
                rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("create", "kiro-http-cancel", "actual HTTP cancellation")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )),
                rmcp::service::PeerRequestOptions::no_options(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !f.kiro_events().iter().any(|e| e == "prompt") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
            Instant::now() + Duration::from_secs(30),
            Cancellation::default(),
        ));
        let user = CapturedPublicationScope::user(
            &f.paths,
            f.installation_id.clone(),
            graphit_core::artifact_store::UserIdentity::new(
                broker.state.origin.clone(),
                "caller-A".into(),
            )
            .unwrap(),
            ScopeLimits::new(16, 4096, 256).unwrap(),
            &original,
        )
        .unwrap();
        let writer = user.runtime_root().join("artifact-store.lock");
        assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        handle
            .cancel(Some("cancel actual Kiro fixture request".into()))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !f.kiro_events().iter().any(|e| e == "driver_closed") {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        // The real SDK driver closed, but this child ignores TERM and delays exit.
        assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        assert!(!f.kiro_events().iter().any(|e| e == "terminal"));
        client.cancel().await.unwrap();
        stop.cancel();
        running.await.unwrap().unwrap();
        assert!(!graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        assert_eq!(
            f.kiro_events()
                .iter()
                .filter(|e| e.as_str() == "engine_v3")
                .count(),
            1
        );
        broker.shutdown().await;
    });
}

fn clear_args(id: &str) -> Value {
    json!({"ai_optimized":false,"operation":"clear","provider":"broker","conversation_id":id,"model":"fixture-model"})
}
async fn kiro_clear_seed(f: &Fixture, broker: &Broker, binding: NativeBinding) {
    let owner = f.start_kiro(broker, binding, "answer", None).await;
    let uri = format!("http://{}/mcp", owner.address().unwrap());
    let stop = Cancellation::default();
    let stopped = stop.clone();
    let running = tokio::spawn(async move { owner.run(&stopped).await });
    let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
    let created = client
        .call_tool(
            CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                args("create", "clear-selected", "seed native session")
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(created.is_error, Some(false));
    client.cancel().await.unwrap();
    stop.cancel();
    running.await.unwrap().unwrap();
}
async fn kiro_clear_event(f: &Fixture, name: &str, count: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while f
            .kiro_events()
            .iter()
            .filter(|event| event.as_str() == name)
            .count()
            < count
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
fn kiro_clear_user(f: &Fixture, broker: &Broker) -> CapturedPublicationScope {
    let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(30),
        Cancellation::default(),
    ));
    CapturedPublicationScope::user(
        &f.paths,
        f.installation_id.clone(),
        graphit_core::artifact_store::UserIdentity::new(
            broker.state.origin.clone(),
            "caller-A".into(),
        )
        .unwrap(),
        ScopeLimits::new(16, 4096, 256).unwrap(),
        &original,
    )
    .unwrap()
}
fn kiro_clear_record(f: &Fixture, broker: &Broker) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(b"clear-selected")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    kiro_clear_user(f, broker)
        .user_root()
        .unwrap()
        .join(format!("graphit-ai-conversation-{digest}.json"))
}
#[test]
fn kiro_clear_v3_uses_private_user_locator_after_corpus_change_and_metadata_delete_is_distinct() {
    native_test(async {
        for binding in [NativeBinding::KiroV3] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            kiro_clear_seed(&f, &broker, binding).await;
            f.republish(DocumentChange::Selected).await;
            let owner = f.start_kiro(&broker, binding, "answer", None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client =
                rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation")
                        .with_arguments(clear_args("clear-selected").as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(
                result.is_error,
                Some(false),
                "native clear result {:?}",
                result.structured_content
            );
            let payload = result.structured_content.unwrap();
            assert_eq!(payload["status"], "cleared");
            assert_eq!(payload["receipt"]["delete_capability"], true);
            assert_eq!(payload["receipt"]["delete_response_observed"], true);
            assert_eq!(payload["receipt"]["driver_success"], true);
            assert_eq!(payload["receipt"]["process"]["joined"], true);
            assert_eq!(payload["local_deletion"]["status"], "deleted");
            assert!(payload["receipt"].get("completion").is_none());
            assert!(!payload.to_string().contains("private-kiro-fixture-session"));
            assert!(!kiro_clear_record(&f, &broker).exists());
            let events = f.kiro_events();
            assert_eq!(events.iter().filter(|e| e.as_str() == "delete").count(), 1);
            assert_eq!(events.iter().filter(|e| e.as_str() == "new").count(), 1);
            assert_eq!(events.iter().filter(|e| e.as_str() == "load").count(), 0);
            assert_eq!(events.iter().filter(|e| e.as_str() == "prompt").count(), 1);
            let created = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args("create", "metadata-only", "new session")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(created.is_error, Some(false));
            let deleted=client.call_tool(CallToolRequestParams::new("graphit_knowledge_conversation")
                .with_arguments(json!({"ai_optimized":false,"operation":"delete","provider":"broker","conversation_id":"metadata-only"}).as_object().unwrap().clone())).await.unwrap();
            assert_eq!(deleted.is_error, Some(false));
            assert_eq!(
                f.kiro_events()
                    .iter()
                    .filter(|e| e.as_str() == "delete")
                    .count(),
                1
            );
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            broker.shutdown().await;
        }
    });
}
#[test]
fn kiro_clear_missing_capability_sdk_error_and_other_user_preserve_binding_without_retry() {
    native_test(async {
        for mode in ["clear_unsupported", "clear_error", "clear_pending"] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            kiro_clear_seed(&f, &broker, NativeBinding::KiroV3).await;
            // Only the pending-response case tests a short deadline. SDK refusal
            // and SDK errors use the actual default request policy.
            let deadline = (mode == "clear_pending").then_some(700);
            let owner = f
                .start_kiro(&broker, NativeBinding::KiroV3, mode, deadline)
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let other = rpc_client(&uri, &broker.state.token("caller-B", RESOURCE, expiry())).await;
            let before = f
                .kiro_events()
                .iter()
                .filter(|e| e.as_str() == "initialize")
                .count();
            let denied = other
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation")
                        .with_arguments(clear_args("clear-selected").as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(denied.is_error, Some(true));
            assert_eq!(
                f.kiro_events()
                    .iter()
                    .filter(|e| e.as_str() == "initialize")
                    .count(),
                before
            );
            other.cancel().await.unwrap();
            let client =
                rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
            let mut wrong = clear_args("clear-selected");
            wrong["model"] = json!("changed-model");
            let denied = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation")
                        .with_arguments(wrong.as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(denied.is_error, Some(true));
            assert_eq!(
                f.kiro_events()
                    .iter()
                    .filter(|e| e.as_str() == "initialize")
                    .count(),
                before
            );
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation")
                        .with_arguments(clear_args("clear-selected").as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            if mode == "clear_pending" {
                assert!(result.structured_content.is_none());
                assert!(
                    result.content[0]
                        .as_text()
                        .unwrap()
                        .text
                        .contains("deadline")
                );
            } else {
                let public_text = result
                    .content
                    .first()
                    .and_then(|content| content.as_text())
                    .map(|content| content.text.to_ascii_lowercase())
                    .unwrap_or_default();
                let markers: Vec<_> = [
                    "deadline",
                    "cancelled",
                    "unsupported",
                    "invalid",
                    "denied",
                    "unavailable",
                    "expired",
                    "timed out",
                ]
                .into_iter()
                .filter(|marker| public_text.contains(marker))
                .collect();
                assert!(
                    result.structured_content.is_some(),
                    "clear mode {mode} lacks receipt; public failure markers={markers:?}"
                );
                let payload = result.structured_content.unwrap();
                assert_eq!(payload["receipt"]["delete_response_observed"], false);
                assert_eq!(payload["local_deletion"]["status"], "retained");
                if mode == "clear_error" {
                    assert_eq!(payload["sdk_error_code"], -32603);
                }
                assert!(!payload.to_string().contains("private-kiro-fixture-session"));
            }
            assert_eq!(
                f.kiro_events()
                    .iter()
                    .filter(|e| e.as_str() == "delete")
                    .count(),
                usize::from(mode != "clear_unsupported")
            );
            assert!(kiro_clear_record(&f, &broker).is_file());
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            assert!(kiro_clear_record(&f, &broker).is_file());
            broker.shutdown().await;
        }
    });
}
#[test]
fn kiro_clear_project_recheck_after_initialize_prevents_actual_sdk_delete() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        kiro_clear_seed(&f, &broker, NativeBinding::KiroV3).await;
        let owner = f
            .start_kiro(&broker, NativeBinding::KiroV3, "clear_init_gate", None)
            .await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let stop = Cancellation::default();
        let stopped = stop.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let peer = client.peer().clone();
        let call = tokio::spawn(async move {
            peer.call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation")
                    .with_arguments(clear_args("clear-selected").as_object().unwrap().clone()),
            )
            .await
            .unwrap()
        });
        kiro_clear_event(&f, "initialize_gate", 1).await;
        let moved = f.temp.path().join("clear-moved-project");
        std::fs::rename(f.project.root(), &moved).unwrap();
        std::fs::write(
            f.temp.path().join("kiro-journal").with_extension("init"),
            b"release",
        )
        .unwrap();
        let result = call.await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let payload = result.structured_content.unwrap();
        assert_eq!(payload["receipt"]["initialize_response_observed"], true);
        assert_eq!(payload["receipt"]["delete_capability"], true);
        assert_eq!(payload["receipt"]["delete_response_observed"], false);
        assert_eq!(
            f.kiro_events()
                .iter()
                .filter(|e| e.as_str() == "delete")
                .count(),
            0
        );
        assert!(kiro_clear_record(&f, &broker).is_file());
        std::fs::rename(moved, f.project.root()).unwrap();
        client.cancel().await.unwrap();
        stop.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}
#[test]
fn kiro_clear_native_response_survives_post_effect_local_failure_or_project_denial() {
    native_test(async {
        for physical in [false, true] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            kiro_clear_seed(&f, &broker, NativeBinding::KiroV3).await;
            let owner = f
                .start_kiro(&broker, NativeBinding::KiroV3, "clear_gate", None)
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client =
                rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
            let peer = client.peer().clone();
            let call = tokio::spawn(async move {
                peer.call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation")
                        .with_arguments(clear_args("clear-selected").as_object().unwrap().clone()),
                )
                .await
                .unwrap()
            });
            kiro_clear_event(&f, "delete", 1).await;
            let writer = kiro_clear_user(&f, &broker)
                .runtime_root()
                .join("artifact-store.lock");
            assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
            assert!(!call.is_finished());
            let record = kiro_clear_record(&f, &broker);
            let saved = record.with_extension("saved");
            let moved = f.temp.path().join("clear-moved-project");
            if physical {
                std::fs::rename(f.project.root(), &moved).unwrap();
            } else {
                std::fs::rename(&record, &saved).unwrap();
                std::fs::create_dir(&record).unwrap();
            }
            std::fs::write(
                f.temp.path().join("kiro-journal").with_extension("clear"),
                b"release",
            )
            .unwrap();
            let result = call.await.unwrap();
            assert_eq!(result.is_error, Some(true));
            let payload = result.structured_content.unwrap();
            assert_eq!(payload["receipt"]["delete_response_observed"], true);
            assert_eq!(payload["receipt"]["process"]["joined"], true);
            assert_ne!(payload["status"], "cleared");
            if physical {
                assert_eq!(payload["local_deletion"]["status"], "retained");
                std::fs::rename(moved, f.project.root()).unwrap();
            } else {
                assert_eq!(payload["local_deletion"]["status"], "failed");
                assert!(payload["local_deletion"]["error"].is_string());
                std::fs::remove_dir(&record).unwrap();
                std::fs::rename(saved, &record).unwrap();
            }
            assert!(record.is_file());
            assert!(!payload.to_string().contains("private-kiro-fixture-session"));
            assert!(!graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            broker.shutdown().await;
        }
    });
}
#[test]
fn kiro_clear_http_cancel_retains_user_writer_until_delayed_native_child_wait() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        std::fs::copy(
            env!("CARGO_BIN_EXE_kiro-knowledge-fixture"),
            f.temp.path().join("kiro-cli"),
        )
        .unwrap();
        kiro_clear_seed(&f, &broker, NativeBinding::KiroV3).await;
        let closed = f
            .kiro_events()
            .iter()
            .filter(|e| e.as_str() == "driver_closed")
            .count();
        let owner = f
            .start_kiro(&broker, NativeBinding::KiroV3, "clear_delay_exit", None)
            .await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let stop = Cancellation::default();
        let stopped = stop.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        let handle = client
            .peer()
            .send_cancellable_request(
                rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                    CallToolRequestParams::new("graphit_knowledge_conversation")
                        .with_arguments(clear_args("clear-selected").as_object().unwrap().clone()),
                )),
                rmcp::service::PeerRequestOptions::no_options(),
            )
            .await
            .unwrap();
        kiro_clear_event(&f, "delete", 1).await;
        let writer = kiro_clear_user(&f, &broker)
            .runtime_root()
            .join("artifact-store.lock");
        assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        handle
            .cancel(Some("cancel native session clear fixture".into()))
            .await
            .unwrap();
        kiro_clear_event(&f, "driver_closed", closed + 1).await;
        assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        assert_eq!(
            f.kiro_events()
                .iter()
                .filter(|e| e.as_str() == "delete_response")
                .count(),
            0
        );
        client.cancel().await.unwrap();
        stop.cancel();
        running.await.unwrap().unwrap();
        assert!(!graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
        assert!(kiro_clear_record(&f, &broker).is_file());
        broker.shutdown().await;
    });
}

#[test]
fn missing_completion_program_rejects_incoming_conversation_before_locator_or_child() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        assert!(!f.temp.path().join("agy").exists());
        let owner = f
            .start_native(
                &broker,
                Some(json!(true)),
                None,
                NativeBinding::Codex,
                Some(graphit_ai::cli_provider::CliCompletionProvider::Antigravity),
                vec![("PATH".into(), f.temp.path().as_os_str().to_owned())],
            )
            .await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        let client = rpc_client(&uri, &raw).await;
        for operation in ["create", "continue"] {
            let result = client
                .call_tool(
                    CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                        args(operation, "missing-cli", "question")
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap();
            assert_eq!(result.is_error, Some(true));
            assert!(
                result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text())
                    .any(|t| t.text.contains("configured captured completion provider"))
            );
        }
        let listed = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(listed.is_error, Some(false));
        assert_eq!(
            listed.structured_content.unwrap()["conversations"],
            json!([])
        );
        let deleted = client.call_tool(CallToolRequestParams::new("graphit_knowledge_conversation")
            .with_arguments(json!({"ai_optimized":false,"operation":"delete","provider":"broker","conversation_id":"missing-cli"}).as_object().unwrap().clone()))
            .await.unwrap();
        assert_eq!(deleted.is_error, Some(true));
        assert!(
            deleted
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .any(|t| t.text == "Knowledge conversation: Missing")
        );
        assert!(f.launches().is_empty());
        assert_eq!(broker.state.calls.load(Ordering::SeqCst), 0);
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

impl Fixture {
    async fn start_claude(&self, broker: &Broker, deadline_ms: Option<u64>) -> ForegroundSource {
        let program = self.temp.path().join("claude");
        agent_fixture::install(&program, json!({"kind":"conversation","protocol":"claude"}));
        self.start_native(
            broker,
            Some(json!(true)),
            deadline_ms,
            NativeBinding::Claude,
            None,
            vec![
                ("PATH".into(), self.temp.path().as_os_str().to_owned()),
                ("CLAUDE_CAPTURED".into(), "literal-captured".into()),
                (
                    "CLAUDE_JOURNAL".into(),
                    self.launches.as_os_str().to_owned(),
                ),
                (
                    "CLAUDE_RELEASE".into(),
                    self.temp.path().join("claude-release").into_os_string(),
                ),
            ],
        )
        .await
    }
}

async fn claude_call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, impl rmcp::ClientHandler>,
    operation: &str,
    id: &str,
    query: &str,
) -> rmcp::model::CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new("graphit_knowledge_conversation")
                .with_arguments(args(operation, id, query).as_object().unwrap().clone()),
        )
        .await
        .unwrap()
}

#[test]
fn claude_incoming_continuity_cold_reopen_selected_binding_and_user_isolation() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let raw = broker.state.token("caller-A", RESOURCE, expiry());
        for restart in 0..4 {
            let owner = f.start_claude(&broker, None).await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let cancel = Cancellation::default();
            let stopped = cancel.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let client = rpc_client(&uri, &raw).await;
            let operations = if restart == 0 {
                vec!["create", "continue"]
            } else {
                vec!["continue"]
            };
            for operation in operations {
                let launches = f.launches().len();
                let query = match restart {
                    2 => "unrelated generation",
                    3 => "selected change",
                    _ => "literal question β",
                };
                let result = claude_call(&client, operation, "claude-A", query).await;
                if restart == 3 {
                    assert_eq!(result.is_error, Some(true));
                    assert_eq!(f.launches().len(), launches);
                    continue;
                }
                assert_eq!(
                    result.is_error,
                    Some(false),
                    "{:?}",
                    result.structured_content
                );
                let payload = result.structured_content.unwrap();
                assert_eq!(payload["persistence"]["status"], "persisted");
                assert_eq!(payload["persistence"]["conversation"]["engine"], "claude");
                assert!(
                    payload["persistence"]["conversation"]
                        .get("native_id")
                        .is_none()
                );
                assert_eq!(payload["receipt"]["completion"]["answer"], "Paris answer");
                assert_eq!(
                    payload["receipt"]["observed_init"]["model"],
                    "observed-native"
                );
            }
            if restart == 1 {
                let other =
                    rpc_client(&uri, &broker.state.token("caller-B", RESOURCE, expiry())).await;
                assert_eq!(
                    claude_call(&other, "continue", "claude-A", "foreign")
                        .await
                        .is_error,
                    Some(true)
                );
                other.cancel().await.unwrap();
                let launches = f.launches().len();
                broker.state.allowed.store(false, Ordering::SeqCst);
                assert_eq!(
                    claude_call(&client, "continue", "claude-A", "denied")
                        .await
                        .is_error,
                    Some(true)
                );
                assert_eq!(f.launches().len(), launches);
                broker.state.allowed.store(true, Ordering::SeqCst);
            }
            client.cancel().await.unwrap();
            cancel.cancel();
            running.await.unwrap().unwrap();
            // The publication fixture owns its own real DaemonAuth singleton.
            // Publish only after the Foreground owner has actually joined.
            match restart {
                1 => f.republish(DocumentChange::Unrelated).await,
                2 => f.republish(DocumentChange::Selected).await,
                _ => {}
            }
        }
        let launches = f.launches();
        assert_eq!(launches.len(), 4);
        let base = json!([
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--model",
            "fixture-model"
        ]);
        assert_eq!(launches[0]["args"], base);
        for launch in &launches[1..] {
            assert_eq!(launch["args"][7], "--resume");
            assert_eq!(launch["args"][8], "00000000-0000-4000-8000-000000000001");
            assert_eq!(launch["captured"], "literal-captured");
            assert_eq!(launch["cwd"], f.project.root().to_str().unwrap());
        }
        broker.shutdown().await;
    });
}

#[test]
fn claude_incoming_native_failures_never_publish_answer_or_create_binding() {
    native_test(async {
        let broker = Broker::new().await;
        let f = Fixture::new(&broker).await;
        let owner = f.start_claude(&broker, None).await;
        let uri = format!("http://{}/mcp", owner.address().unwrap());
        let cancel = Cancellation::default();
        let stopped = cancel.clone();
        let running = tokio::spawn(async move { owner.run(&stopped).await });
        let client = rpc_client(&uri, &broker.state.token("caller-A", RESOURCE, expiry())).await;
        assert_eq!(
            claude_call(&client, "create", "claude-good", "initial")
                .await
                .is_error,
            Some(false)
        );
        for query in [
            "mismatch",
            "native error",
            "malformed",
            "missing result",
            "nonzero",
        ] {
            let (operation, id) = if query == "mismatch" {
                ("continue", "claude-good")
            } else {
                ("create", query)
            };
            let result = claude_call(&client, operation, id, query).await;
            assert_eq!(
                result.is_error,
                Some(true),
                "{query}: {:?}",
                result.structured_content
            );
            let payload = result.structured_content.unwrap();
            let cause = match query {
                "mismatch" => "Sink(ConsumerFailed)",
                "native error" => "Provider",
                "malformed" => "Wire",
                "missing result" => "MissingResult",
                "nonzero" => "NonzeroExit",
                _ => unreachable!(),
            };
            assert_eq!(payload["execution_error"], cause, "{query}: {payload}");
            assert_eq!(payload["receipt"]["classification"], "Structured");
            let known = payload["receipt"]["known_events"].as_u64().unwrap();
            if query == "mismatch" {
                assert_eq!(known, 0, "refused Init is not an accepted known event");
            } else {
                assert!(known >= 1, "{query}: native Init prefix lost");
            }
            assert!(payload["receipt"]["frames"].as_u64().unwrap() >= 1);
            assert!(payload["receipt"]["frames"].as_u64().unwrap() >= known);
            assert!(
                payload["receipt"]["process"]["observed"]["stdout"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
            if query != "mismatch" {
                assert!(payload["receipt"]["accepted"]["stdout"].as_u64().unwrap() > 0);
            }
            if query == "nonzero" {
                assert_eq!(payload["receipt"]["process"]["exit_code"], 7);
            }

            assert!(
                payload["receipt"]["completion"].get("answer").is_none(),
                "{query}: {payload}"
            );
            assert!(
                payload["receipt"].get("native_result_json").is_none()
                    || payload["receipt"]["native_result_json"].is_null()
            );
            assert!(payload["persistence"].get("conversation").is_none());
        }
        let listed = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation").with_arguments(
                    json!({"ai_optimized":false,"operation":"list","provider":"broker"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        let records = listed.structured_content.unwrap();
        assert_eq!(records["conversations"].as_array().unwrap().len(), 1);
        assert_eq!(
            records["conversations"][0]["conversation_id"],
            "claude-good"
        );
        let before = f.launches().len();
        let mut changed = args("continue", "claude-good", "model changed");
        changed["model"] = json!("other-model");
        let rejected = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation")
                    .with_arguments(changed.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(rejected.is_error, Some(true));
        let clear = client
            .call_tool(
                CallToolRequestParams::new("graphit_knowledge_conversation")
                    .with_arguments(clear_args("claude-good").as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(clear.is_error, Some(true));
        assert_eq!(f.launches().len(), before);
        // Failed resume retains the prior binding; there is no fallback create.
        assert_eq!(
            claude_call(&client, "continue", "claude-good", "recovered")
                .await
                .is_error,
            Some(false)
        );
        client.cancel().await.unwrap();
        cancel.cancel();
        running.await.unwrap().unwrap();
        broker.shutdown().await;
    });
}

#[test]
fn claude_incoming_progress_precedes_exit_and_actual_cleanup_releases_writer() {
    native_test(async {
        for mode in [
            "release",
            "store-fault",
            "physical-denied",
            "deadline",
            "cancel",
            "shutdown",
        ] {
            let broker = Broker::new().await;
            let f = Fixture::new(&broker).await;
            let owner = f
                .start_claude(&broker, (mode == "deadline").then_some(2000))
                .await;
            let uri = format!("http://{}/mcp", owner.address().unwrap());
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let running = tokio::spawn(async move { owner.run(&stopped).await });
            let raw = broker.state.token("caller-A", RESOURCE, expiry());
            let mut headers = HashMap::new();
            let mut bearer = header::HeaderValue::from_str(&format!("Bearer {raw}")).unwrap();
            bearer.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, bearer);
            let progress = ProgressCollector(Arc::new(std::sync::Mutex::new(Vec::new())));
            let client = progress
                .clone()
                .serve_with_lifecycle(
                    StreamableHttpClientTransport::with_client(
                        reqwest::Client::builder().no_proxy().build().unwrap(),
                        StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
                    ),
                    ClientLifecycleMode::Discover {
                        preferred_versions: vec![ProtocolVersion::LATEST],
                    },
                )
                .await
                .unwrap();
            let mut params = CallToolRequestParams::new("graphit_knowledge_conversation")
                .with_arguments(
                    args("create", "claude-gated", "wait for release")
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            params.meta = Some(
                serde_json::from_value(json!({"progressToken":"claude-real-progress"})).unwrap(),
            );
            let handle = client
                .peer()
                .send_cancellable_request(
                    rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                        params,
                    )),
                    rmcp::service::PeerRequestOptions::no_options(),
                )
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while !progress
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|message| message.contains("observed-native"))
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(f.launches().len(), 1);
            let user = kiro_clear_user(&f, &broker);
            let writer = user.runtime_root().join("artifact-store.lock");
            assert!(
                graphit_core::file_lock::FileLock::is_locked(&writer).unwrap(),
                "{mode}: writer released before exit"
            );
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(b"claude-gated")
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let target = user
                .user_root()
                .unwrap()
                .join(format!("graphit-ai-conversation-{digest}.json"));
            assert!(!target.exists(), "provisional native identity persisted");
            let moved = f.temp.path().join("claude-moved-project");
            if mode == "physical-denied" {
                std::fs::rename(f.project.root(), &moved).unwrap();
            }
            if mode == "store-fault" {
                // Isolated fixture destination, never the running Graphit store.
                std::fs::write(&target, b"foreign-owned-destination").unwrap();
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            if mode == "cancel" {
                handle
                    .cancel(Some("cancel actual Claude fixture".into()))
                    .await
                    .unwrap();
            } else {
                // Keep the actual request handle alive until the chosen terminal condition.
                if mode == "shutdown" {
                    stop.cancel();
                }
                if mode == "deadline" || mode == "shutdown" {
                    tokio::time::timeout(Duration::from_secs(10), async {
                        while !f.temp.path().join("claude-release.term").exists() {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                    })
                    .await
                    .unwrap();
                    assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
                }
                std::fs::write(f.temp.path().join("claude-release"), b"release").unwrap();
                let response = handle.await_response().await;
                if mode == "release" || mode == "store-fault" || mode == "physical-denied" {
                    let rmcp::model::ServerResult::CallToolResult(result) = response.unwrap()
                    else {
                        panic!("call result")
                    };
                    let payload = result.structured_content.unwrap();
                    assert_eq!(result.is_error, Some(mode != "release"), "{payload}");
                    if mode != "release" {
                        assert_eq!(
                            payload["persistence"]["status"],
                            if mode == "store-fault" {
                                "failed"
                            } else {
                                "denied"
                            }
                        );
                        assert!(payload["receipt"]["completion"].get("answer").is_none());
                        assert!(payload["receipt"]["native_result_json"].is_null());
                    }
                    assert_eq!(payload["receipt"]["process"]["cleanup"], "Complete");
                }
            }
            if mode == "cancel" {
                tokio::time::timeout(Duration::from_secs(10), async {
                    while !f.temp.path().join("claude-release.term").exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                assert!(graphit_core::file_lock::FileLock::is_locked(&writer).unwrap());
                std::fs::write(f.temp.path().join("claude-release"), b"release").unwrap();
            }
            if mode == "physical-denied" {
                std::fs::rename(&moved, f.project.root()).unwrap();
            }
            client.cancel().await.unwrap();
            stop.cancel();
            running.await.unwrap().unwrap();
            assert!(
                !graphit_core::file_lock::FileLock::is_locked(&writer).unwrap(),
                "{mode}: writer retained after Host join"
            );
            if mode == "deadline" || mode == "cancel" || mode == "shutdown" {
                assert!(!target.exists());
            }
            broker.shutdown().await;
        }
    });
}
