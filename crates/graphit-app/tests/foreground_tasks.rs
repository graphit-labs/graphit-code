#![cfg(target_os = "linux")]
//! Actual Foreground/compiled CLI composition, signed Broker TCP and persisted native Task.
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
use graphit_app::{foreground_source::ForegroundSource, task_server::TaskPolicy};
use graphit_auth::{
    AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission, ProjectCapture,
    ProjectRecordAuthority, ScopeCapture, ScopeInput, ServiceAdmissionLimits,
    ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
    profile::{
        AiService, BrokerService, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
    },
    profile_manager::{NativeProfileManager, ProfileManagerAuthority},
};
use graphit_core::{
    artifact_store::{ArtifactLimits, CapturedPublicationScope, CopyLimits, ScopeLimits},
    auth_document::AuthDocumentLimits,
    brand::Brand,
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource, ConfigStore},
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    installation_bootstrap::BootstrapLimits,
    module_settings::Module,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
    runtime_key::RuntimeKeyLimits,
    version_context::VersionContext,
};
use graphit_daemon::{
    auth_startup::{DaemonAuth, StartupPolicy},
    project_bootstrap::ProjectBootstrapInput,
};
use graphit_hub::{
    NativeBinding, ProjectionLimits,
    native::ContentLimits,
    policy::{HubResourcePolicy, RequestContext},
};
use graphit_records::{
    task_protocol::{WriteRequest, execute_write},
    task_store::{TaskReferencesInputLimits, TaskStoreReadLimits, TaskStoreWriteLimits},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    num::{NonZeroU32, NonZeroUsize},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

#[path = "foreground_tasks/hosting.rs"]
mod hosting;
#[path = "foreground_tasks/open_ui.rs"]
mod open_ui;
#[path = "foreground_tasks/tray.rs"]
mod tray;
#[path = "foreground_tasks/user_memory.rs"]
mod user_memory;

struct Seed {
    temp: tempfile::TempDir,
    paths: InstallationPaths,
    project: ProjectPaths,
    reference_project: Option<ProjectPaths>,
    reference_mirror: Option<ProjectPaths>,
    browser_projects: Vec<ProjectPaths>,
    auth: Arc<DaemonAuth>,
    raw: String,
    version: VersionContext,
    policy: TaskPolicy,
}
impl Seed {
    async fn new() -> Self {
        Self::with_record_sources(false).await
    }
    async fn with_record_sources(records: bool) -> Self {
        Self::with_browser_records(records, false).await
    }
    async fn with_browser_records(records: bool, browser_writes: bool) -> Self {
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
        let mut permissions: Vec<_> = [
            Permission::Read(Module::Task),
            Permission::WriteTask,
            Permission::WriteOwnTask,
            Permission::CoordinateTask,
            Permission::ManageAccessGrants,
            Permission::Delegate,
        ]
        .iter()
        .chain([Permission::ManageAuthProfiles].iter())
        .copied()
        .collect();
        if records {
            permissions.extend([
                Permission::Read(Module::Memory),
                Permission::WriteMemory,
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
            ]);
        }
        let source = || SourceAuthorityPolicy {
            ordinary: &permissions,
            administrative: &permissions,
            sessions: AdminSessionPolicy {
                lifetime: Duration::from_secs(900),
                maximum: Duration::from_secs(3600),
            },
        };
        let mut authority = IssuerAuthorityPolicy::new(
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
        if records {
            authority = authority
                .with_incoming_user_permissions(
                    "broker",
                    if browser_writes {
                        &[
                            Permission::ReadUser,
                            Permission::Read(Module::Memory),
                            Permission::MutateUser,
                            Permission::WriteMemory,
                        ]
                    } else {
                        &[Permission::ReadUser, Permission::Read(Module::Memory)]
                    },
                )
                .unwrap()
                .with_incoming_user_permissions(
                    "seed",
                    &[
                        Permission::ReadUser,
                        Permission::Read(Module::Memory),
                        Permission::MutateUser,
                        Permission::WriteMemory,
                    ],
                )
                .unwrap()
                .with_web_imported_knowledge_read("broker")
                .unwrap();
        }
        let policy = StartupPolicy {
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
        };
        let root = temp.path().join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("raw"), "actual native payload").unwrap();
        let project = ProjectPaths::at(&root, &paths).unwrap();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancel)
            .unwrap()
            .ensure(&ProjectMetadata::new("project").unwrap())
            .unwrap();
        let reference_project = if records {
            let root = temp.path().join("other-reference-project");
            fs::create_dir(&root).unwrap();
            let other = ProjectPaths::at(&root, &paths).unwrap();
            ProjectIdentityStore::at(&other)
                .writer(Duration::from_secs(5), &cancel)
                .unwrap()
                .ensure(&ProjectMetadata::new("other-reference-project").unwrap())
                .unwrap();
            Some(other)
        } else {
            None
        };
        let reference_mirror = if records {
            let root = temp.path().join("reference-mirror");
            fs::create_dir(&root).unwrap();
            let mirror = ProjectPaths::at(&root, &paths).unwrap();
            fs::create_dir_all(mirror.brand_directory()).unwrap();
            fs::copy(project.identity_file(), mirror.identity_file()).unwrap();
            ProjectIdentityStore::at(&mirror)
                .writer(Duration::from_secs(5), &cancel)
                .unwrap()
                .ensure(&ProjectMetadata::new("reference-mirror").unwrap())
                .unwrap();
            Some(mirror)
        } else {
            None
        };
        let browser_projects = if browser_writes {
            let mut projects = Vec::new();
            for name in ["empty-knowledge", "no-knowledge"] {
                let root = temp.path().join(name);
                fs::create_dir(&root).unwrap();
                let project = ProjectPaths::at(&root, &paths).unwrap();
                ProjectIdentityStore::at(&project)
                    .writer(Duration::from_secs(5), &cancel)
                    .unwrap()
                    .ensure(&ProjectMetadata::new(name).unwrap())
                    .unwrap();
                projects.push(project);
            }
            projects
        } else {
            vec![]
        };
        let mut trusted_projects = vec![project.clone()];
        trusted_projects.extend(reference_project.iter().cloned());
        trusted_projects.extend(reference_mirror.iter().cloned());
        trusted_projects.extend(browser_projects.iter().cloned());
        let config = ConfigSnapshot::new(paths.brand(), []).unwrap();
        let hub = HubResourcePolicy::resolve(&config).unwrap();
        let bootstrap_original = hub.request_context(Instant::now(), &cancel).unwrap();
        ForegroundSource::start(
            &paths,
            &trusted_projects
                .iter()
                .map(|project| ProjectBootstrapInput {
                    project,
                    explicit_version: "",
                    pin: None,
                    detached_base: "",
                })
                .collect::<Vec<_>>(),
            0,
            &config,
            NativeBinding::Codex,
            &bootstrap_original,
        )
        .unwrap()
        .shutdown()
        .await
        .unwrap();
        let auth = Arc::new(DaemonAuth::start(&paths, &trusted_projects, policy, &ctx).unwrap());
        let raw = {
            let key = auth.runtime().check(ctx.snapshot()).unwrap();
            String::from_utf8(key.secret(ctx.snapshot()).unwrap().to_vec()).unwrap()
        };
        let checkout = GitCheckout::inspect(&root, Duration::from_secs(5), &cancel).unwrap();
        let version =
            VersionContext::select(&project, checkout.as_checkout(), "", None, "").unwrap();
        let copy = CopyLimits::new(1048576, 128, 65536, 16).unwrap();
        let read = TaskStoreReadLimits {
            native: NativeDocumentFileLimits::new(1048576, 65536, 256, 8388608).unwrap(),
            batch_rows: NonZeroU32::new(16).unwrap(),
        };
        let task_policy = TaskPolicy {
            imported_knowledge: None,
            content,
            timeout: Duration::from_secs(30),
            scope,
            read,
            write: TaskStoreWriteLimits {
                read,
                artifacts: ArtifactLimits::new(1048576, 65536, copy).unwrap(),
                references: TaskReferencesInputLimits {
                    max_entries: NonZeroUsize::new(content.max_collection_entries).unwrap(),
                    max_text_bytes: NonZeroUsize::new(content.max_input_bytes).unwrap(),
                },
            },
            role_reference: ProjectionLimits::new(128, 4096, 32, 128, 32).unwrap(),
            binding: NativeBinding::Codex,
        };
        Self {
            temp,
            paths,
            project,
            reference_project,
            reference_mirror,
            browser_projects,
            auth,
            raw,
            version,
            policy: task_policy,
        }
    }
    fn provider(&self, name: &str, kind: ProviderKind) {
        let cancellation = Cancellation::default();
        let original = RequestContext::new(
            self.policy.content,
            &cancellation,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let issuer = self.auth.issuer();
        let caller = issuer.authenticate_runtime(&self.raw, &original).unwrap();
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
                name.into(),
                ProviderConfig {
                    kind: kind.clone(),
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: if matches!(&kind, ProviderKind::Local { .. }) {
                            S3Source::LocalCredentials
                        } else {
                            S3Source::BrokerScope
                        },
                    },
                },
                revision,
                1,
            )
            .unwrap();
        assert!(publication.failure().is_none() && publication.cleanup_failure().is_none());
        assert!(publication.into_pending_cleanup().is_none());
    }
    async fn write(&self, operation: &str, input: Value) -> Value {
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            self.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let id = ProjectIdentityStore::at(&self.project)
            .load()
            .unwrap()
            .unwrap()
            .id();
        let captured = CapturedPublicationScope::project(
            &self.project,
            id,
            self.policy.scope,
            original.snapshot(),
        )
        .unwrap();
        let binding = captured.binding().unwrap();
        let scope = ScopeInput {
            installation: self.auth.installation(),
            project: Some(ProjectCapture {
                scope: ScopeCapture {
                    live: &captured,
                    record: &binding,
                },
                version: &self.version,
            }),
            providers: &[],
            binding: NativeBinding::Codex,
            global_version: "",
        };
        let request: WriteRequest =
            serde_json::from_value(json!({"operation":operation,"input":input})).unwrap();
        let issuer = self.auth.issuer();
        let caller = issuer.authenticate_runtime(&self.raw, &original).unwrap();
        let authority = ProjectRecordAuthority::new(issuer);
        let admission = authority
            .admit_authenticated_write(&caller, &original, scope, request.required_operation())
            .unwrap();
        serde_json::to_value(
            execute_write(request, original.snapshot(), &admission, self.policy.write)
                .await
                .unwrap(),
        )
        .unwrap()["result"]
            .clone()
    }
    async fn prepare(self) -> Prepared {
        let session=self.write("create_session",json!({"idempotency_key":"foreground-session","title":"Foreground session","description":"Persisted native corpus","strategy":"native"})).await;
        let task=self.write("create_task",json!({"idempotency_key":"foreground-task","session_id":session["id"],"title":"Foreground calendar","description":"Persisted scheduled task","task_type":"TASK","priority":1,"schedule":{"start_date":"2028-02-28","end_date":"2028-03-01","due_date":"2028-03-02"},"checks":[{"kind":"acceptance","text":"Native browser read"},{"kind":"test","text":"Validate native browser read"}]})).await;
        let dependent=self.write("create_task",json!({"idempotency_key":"foreground-dependent","session_id":session["id"],"depends_on":[task["specification"]["id"]],"title":"Foreground dependent","description":"Persisted dependency","task_type":"TASK","priority":2,"checks":[{"kind":"acceptance","text":"Native dependency"},{"kind":"test","text":"Validate native dependency"}]})).await;
        let claim = self
            .write(
                "claim_task",
                json!({"id":task["specification"]["id"],"lease_ms":60000}),
            )
            .await;
        let fence = claim["token"].as_str().unwrap().to_owned();
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            self.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let outcome = self.auth.remove_runtime(original.snapshot());
        assert!(outcome.failure().is_none() && outcome.cleanup_failure().is_none());
        drop(self.auth);
        Prepared {
            temp: self.temp,
            paths: self.paths,
            project: self.project,
            session: session["id"].clone(),
            task: task["specification"]["id"].clone(),
            dependent: dependent["specification"]["id"].clone(),
            fence,
        }
    }
}
struct Prepared {
    temp: tempfile::TempDir,
    paths: InstallationPaths,
    project: ProjectPaths,
    session: Value,
    task: Value,
    dependent: Value,
    fence: String,
}
fn broker_kind(endpoint: &str) -> ProviderKind {
    ProviderKind::Broker(BrokerService {
        endpoint: endpoint.into(),
        mcp_resource: None,
        allow_anonymous: false,
    })
}
fn config(issuer: &Issuer) -> Value {
    json!({"providers":["broker"],"project_endpoint":format!("{}/v1/project/access/authorize",issuer.origin),"allow_loopback_http":true})
}

struct Issuer {
    origin: String,
    key: Ed25519KeyPair,
    nonce: Mutex<String>,
    challenge: Mutex<String>,
    issued: Mutex<String>,
    allowed: AtomicBool,
    ast_allowed: AtomicBool,
    memory_allowed: AtomicBool,
    knowledge_allowed: AtomicBool,
    record_sources: bool,
    memory_writes: AtomicBool,
    hold: AtomicBool,
    calls: AtomicUsize,
    active: AtomicUsize,
    userinfo: AtomicUsize,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
impl Issuer {
    fn jwt(&self, value: Value) -> String {
        let payload = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&json!({"alg":"EdDSA","kid":"one","typ":"JWT"})).unwrap()
            ),
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap())
        );
        format!(
            "{}.{}",
            payload,
            URL_SAFE_NO_PAD.encode(self.key.sign(payload.as_bytes()).as_ref())
        )
    }
}
struct Broker {
    issuer: Arc<Issuer>,
    stop: CancellationToken,
    server: tokio::task::JoinHandle<()>,
}
impl Broker {
    async fn new() -> Self {
        Self::with_record_sources(false).await
    }
    async fn with_record_sources(records: bool) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let issuer = Arc::new(Issuer {
            origin,
            key: Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap(),
            nonce: Mutex::new(String::new()),
            challenge: Mutex::new(String::new()),
            issued: Mutex::new(String::new()),
            allowed: AtomicBool::new(true),
            ast_allowed: AtomicBool::new(false),
            memory_allowed: AtomicBool::new(records),
            knowledge_allowed: AtomicBool::new(records),
            record_sources: records,
            memory_writes: AtomicBool::new(false),
            hold: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            userinfo: AtomicUsize::new(0),
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let router = Router::new()
            .route("/authorize", get(hosting::authorize))
            .route("/.well-known/graphit-broker", get(broker_metadata))
            .route("/.well-known/openid-configuration", get(oidc))
            .route("/keys", get(keys))
            .route("/register", post(register))
            .route("/token", post(token))
            .route("/userinfo", get(userinfo))
            .route("/v1/project/access/authorize", post(access))
            .with_state(issuer.clone());
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            issuer,
            stop,
            server,
        }
    }
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.stop.cancel();
        self.server.abort();
    }
}
async fn broker_metadata(State(s): State<Arc<Issuer>>) -> Json<Value> {
    let resources: Vec<&str> = if s.record_sources {
        vec![hosting::USER_RESOURCE]
    } else {
        vec![]
    };
    Json(
        json!({"version":"1","issuer":s.origin,"authentication":{"type":"oidc","issuer":s.origin,"client_id":"native-client","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":resources}}),
    )
}
async fn oidc(State(s): State<Arc<Issuer>>) -> Json<Value> {
    Json(
        json!({"issuer":s.origin,"authorization_endpoint":format!("{}/authorize",s.origin),"registration_endpoint":format!("{}/register",s.origin),"token_endpoint":format!("{}/token",s.origin),"jwks_uri":format!("{}/keys",s.origin),"userinfo_endpoint":format!("{}/userinfo",s.origin),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}),
    )
}
async fn keys(State(s): State<Arc<Issuer>>) -> Json<Value> {
    Json(
        json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(s.key.public_key().as_ref()),"kid":"one","alg":"EdDSA","use":"sig","key_ops":["verify"]}]}),
    )
}
async fn register(Json(v): Json<Value>) -> (StatusCode, Json<Value>) {
    let redirects = v["redirect_uris"].as_array().unwrap();
    assert_eq!(redirects.len(), 1);
    let redirect = url::Url::parse(redirects[0].as_str().unwrap()).unwrap();
    assert!(redirect.username().is_empty() && redirect.password().is_none());
    assert!(redirect.query().is_none() && redirect.fragment().is_none());
    assert_eq!(redirect.path(), "/auth/callback");
    let application_type = match redirect.scheme() {
        "http" => {
            assert!(
                matches!(redirect.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback())
                    || matches!(redirect.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback())
            );
            assert!(redirect.port().is_some_and(|port| port != 0));
            "native"
        }
        "https" => "web",
        _ => panic!("unsupported Browser callback scheme"),
    };
    assert_eq!(v["application_type"], application_type);
    assert_eq!(v["token_endpoint_auth_method"], "none");
    assert_eq!(
        v["grant_types"],
        json!(["authorization_code", "refresh_token"])
    );
    assert_eq!(v["response_types"], json!(["code"]));
    (
        StatusCode::CREATED,
        Json(json!({
            "client_id":"web-client", "redirect_uris":v["redirect_uris"],
            "application_type":application_type, "token_endpoint_auth_method":"none"
        })),
    )
}
async fn token(State(s): State<Arc<Issuer>>, body: String) -> Json<Value> {
    let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    assert_eq!(form["client_id"], "web-client");
    assert_eq!(form["resource"], format!("{}/v1", s.origin));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let access=s.jwt(json!({"iss":s.origin,"sub":"web-subject","client_id":"web-client","token_use":"access","jti":"foreground-token","aud":["broker-api",format!("{}/v1",s.origin)],"iat":now,"exp":now+3600}));
    *s.issued.lock().unwrap() = access.clone();
    let mut response = json!({"access_token":access,"token_type":"Bearer","expires_in":3600,"refresh_token":"refresh-private"});
    if form["grant_type"] == "authorization_code" {
        assert_eq!(
            URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes())),
            *s.challenge.lock().unwrap()
        );
        response["id_token"]=s.jwt(json!({"iss":s.origin,"sub":"web-subject","aud":"web-client","iat":now,"exp":now+3600,"nonce":s.nonce.lock().unwrap().clone()})).into();
    }
    Json(response)
}
async fn userinfo(State(s): State<Arc<Issuer>>) -> StatusCode {
    s.userinfo.fetch_add(1, Ordering::SeqCst);
    StatusCode::INTERNAL_SERVER_ERROR
}
struct IssuerFlight<'a>(&'a AtomicUsize);
impl Drop for IssuerFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
async fn access(
    State(s): State<Arc<Issuer>>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> (StatusCode, Json<Value>) {
    assert!(
        headers[header::AUTHORIZATION].to_str().unwrap()
            == format!("Bearer {}", s.issued.lock().unwrap()),
        "signed issuer bearer binding"
    );
    let module = v["module"].as_str().unwrap();
    assert!(matches!(module, "task" | "ast" | "memory" | "knowledge"));
    let action = v["action"].as_str().unwrap();
    assert!(
        action == "read"
            || (action == "write" && module == "memory" && s.memory_writes.load(Ordering::SeqCst))
    );
    s.calls.fetch_add(1, Ordering::SeqCst);
    s.active.fetch_add(1, Ordering::SeqCst);
    let _flight = IssuerFlight(&s.active);
    let released = s.release.notified();
    tokio::pin!(released);
    released.as_mut().enable();
    if s.hold.load(Ordering::SeqCst) {
        s.entered.notify_one();
        released.await;
    }
    let allowed = match module {
        "task" => s.allowed.load(Ordering::SeqCst),
        "ast" => s.ast_allowed.load(Ordering::SeqCst),
        "memory" => s.memory_allowed.load(Ordering::SeqCst),
        "knowledge" => s.knowledge_allowed.load(Ordering::SeqCst),
        _ => unreachable!(),
    };
    if !allowed {
        return (StatusCode::FORBIDDEN, Json(json!({})));
    }
    (
        StatusCode::OK,
        Json(
            json!({"subject":{"issuer":s.origin,"subject":"web-subject"},"authorization_revision":1,"project_id":v["project_id"],"module":module,"action":action}),
        ),
    )
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}
async fn login(client: &reqwest::Client, origin: &str, issuer: &Issuer) -> String {
    let response = client
        .post(format!("{origin}/auth/login"))
        .header(header::ORIGIN, origin)
        .json(&json!({"provider":"broker"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert!(!response.headers().contains_key(header::LOCATION));
    let response = response.json::<Value>().await.unwrap();
    let url = url::Url::parse(response["authorization_url"].as_str().unwrap()).unwrap();
    let fields: HashMap<String, String> = url.query_pairs().into_owned().collect();
    *issuer.nonce.lock().unwrap() = fields["nonce"].clone();
    *issuer.challenge.lock().unwrap() = fields["code_challenge"].clone();
    let response = client
        .get(format!(
            "{origin}/auth/callback?code=code&state={}",
            fields["state"]
        ))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find(|h| h.to_str().unwrap().starts_with("graphit_session="))
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
async fn read(
    client: &reqwest::Client,
    origin: &str,
    cookie: &str,
    route: &str,
    body: Value,
) -> reqwest::Response {
    client
        .post(format!("{origin}/api/tasks/{route}"))
        .header(header::ORIGIN, origin)
        .header(header::COOKIE, cookie)
        .json(&body)
        .send()
        .await
        .unwrap()
}

impl Prepared {
    fn snapshot(&self, value: Option<Value>) -> ConfigSnapshot {
        let documents = value.map(|v| {
            ConfigDocument::new(
                self.paths.brand(),
                ConfigSource::Global,
                [(ConfigKey::new("task.browser").unwrap(), v)],
            )
            .unwrap()
        });
        ConfigSnapshot::new(self.paths.brand(), documents).unwrap()
    }
    fn start(
        &self,
        value: Option<Value>,
    ) -> Result<ForegroundSource, graphit_app::foreground_source::ForegroundFailure> {
        self.start_projects(value, &[&self.project])
    }
    fn start_projects(
        &self,
        value: Option<Value>,
        projects: &[&ProjectPaths],
    ) -> Result<ForegroundSource, graphit_app::foreground_source::ForegroundFailure> {
        let snapshot = self.snapshot(value);
        let policy = HubResourcePolicy::resolve(&snapshot).unwrap();
        let cancellation = Cancellation::default();
        let original = policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        ForegroundSource::start(
            &self.paths,
            &projects
                .iter()
                .map(|project| ProjectBootstrapInput {
                    project,
                    explicit_version: "",
                    pin: None,
                    detached_base: "",
                })
                .collect::<Vec<_>>(),
            0,
            &snapshot,
            NativeBinding::Codex,
            &original,
        )
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(self.project.root())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.temp.path())
            .env(
                self.paths
                    .brand()
                    .environment_variable("GLOBAL_DIR")
                    .unwrap(),
                self.paths.root(),
            );
        command
    }
    async fn start_browser_records(
        &self,
        value: Value,
        projects: &[&ProjectPaths],
        environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    ) -> Result<ForegroundSource, graphit_app::foreground_source::ForegroundFailure> {
        let snapshot = self.snapshot(Some(value));
        let policy = HubResourcePolicy::resolve(&snapshot).unwrap();
        let cancellation = Cancellation::default();
        let original = policy
            .request_context(Instant::now(), &cancellation)
            .unwrap();
        ForegroundSource::start_and_publish(
            &self.paths,
            &projects
                .iter()
                .map(|project| ProjectBootstrapInput {
                    project,
                    explicit_version: "",
                    pin: None,
                    detached_base: "",
                })
                .collect::<Vec<_>>(),
            0,
            &snapshot,
            NativeBinding::Codex,
            environment,
            &original,
        )
        .await
    }
    async fn daemon(&self, value: Value, environment: bool) -> Daemon {
        let cancellation = Cancellation::default();
        ConfigStore::global(&self.paths)
            .unwrap()
            .set(
                ConfigKey::new("agent").unwrap(),
                json!("codex"),
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
        let mut command = self.command();
        if environment {
            command.env(
                ConfigKey::new("task.browser")
                    .unwrap()
                    .environment_variable(self.paths.brand()),
                value.to_string(),
            );
        } else {
            ConfigStore::global(&self.paths)
                .unwrap()
                .set(
                    ConfigKey::new("task.browser").unwrap(),
                    value,
                    Duration::from_secs(5),
                    &cancellation,
                )
                .unwrap();
        }
        let log = self.temp.path().join("daemon.log");
        let child = command
            .args(["daemon", "foreground"])
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        let mut daemon = Daemon { child, log };
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = daemon.child.try_wait().unwrap() {
                panic!(
                    "foreground failed {status}: {}",
                    fs::read_to_string(&daemon.log).unwrap()
                );
            }
            if self.paths.root().join("daemon/mcp.json").exists() {
                break;
            }
            assert!(Instant::now() < until, "discovery startup timeout");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        daemon
    }
    fn origin(&self) -> String {
        let record: graphit_daemon::source_discovery::DiscoveryRecord =
            serde_json::from_slice(&fs::read(self.paths.root().join("daemon/mcp.json")).unwrap())
                .unwrap();
        format!("http://{}", record.address())
    }
    fn assert_clean(&self) {
        assert!(!self.paths.root().join("daemon/mcp.json").exists());
        assert!(!self.paths.root().join("daemon/mcp.key").exists());
    }
}
struct Daemon {
    child: Child,
    log: std::path::PathBuf,
}
impl Daemon {
    async fn stop(mut self, signal: &str) {
        assert!(
            Command::new("kill")
                .args([signal, &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(
                    status.code(),
                    Some(130),
                    "{}",
                    fs::read_to_string(&self.log).unwrap()
                );
                break;
            }
            assert!(Instant::now() < until, "foreground stop timeout");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
async fn prepared(broker: &Broker) -> Prepared {
    let seed = Seed::new().await;
    seed.provider("broker", broker_kind(&broker.issuer.origin));
    seed.prepare().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_foreground_absence_invalid_trust_and_failed_startup_cleanup() {
    let broker = Broker::new().await;
    let seed = Seed::new().await;
    seed.provider("broker", broker_kind(&broker.issuer.origin));
    seed.provider("other", broker_kind("https://other.example"));
    seed.provider(
        "local",
        ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: None,
        },
    );
    let fixture = seed.prepare().await;
    let auth_before = fs::read(fixture.paths.auth_file()).unwrap();
    let owner = fixture.start(None).unwrap();
    let origin = format!("http://{}", owner.address().unwrap());
    let cancellation = Cancellation::default();
    let stop = cancellation.clone();
    let running = tokio::spawn(async move { owner.run(&stop).await });
    assert_eq!(
        client()
            .get(format!("{origin}/auth/providers"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    for path in [
        "/task/gantt",
        "/task/explorer/task-id",
        "/logo.svg",
        "/favicon.svg",
    ] {
        assert_eq!(
            client()
                .get(format!("{origin}{path}"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    cancellation.cancel();
    running.await.unwrap().unwrap();
    fixture.assert_clean();
    let base = config(&broker.issuer);
    let mut mixed = base.clone();
    mixed["providers"] = json!(["broker", "other"]);
    let mut local = base.clone();
    local["providers"] = json!(["local"]);
    let mut missing = base.clone();
    missing["providers"] = json!(["missing"]);
    let mut foreign = base.clone();
    foreign["project_endpoint"] = json!("https://foreign.example/v1/project/access/authorize");
    let mut path = base.clone();
    path["project_endpoint"] = format!("{}/wrong", broker.issuer.origin).into();
    let mut forbidden = base.clone();
    forbidden["allow_loopback_http"] = json!(false);
    for value in [json!({}), local, mixed, missing, foreign, path, forbidden] {
        let failure = match fixture.start(Some(value)) {
            Err(e) => e,
            Ok(_) => panic!("invalid startup accepted"),
        };
        assert!(!fixture.paths.root().join("daemon/mcp.json").exists());
        if let Some(owner) = failure.owner {
            owner.shutdown().await.unwrap();
        }
        fixture.assert_clean();
        assert_eq!(fs::read(fixture.paths.auth_file()).unwrap(), auth_before);
    }
    let owner = fixture.start(Some(base)).unwrap();
    owner.shutdown().await.unwrap();
    fixture.assert_clean();
    assert_eq!(fs::read(fixture.paths.auth_file()).unwrap(), auth_before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_foreground_signed_cookie_reads_schedule_dependencies_and_source() {
    let broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    let auth_before = fs::read(fixture.paths.auth_file()).unwrap();
    fs::write(fixture.project.root().join("raw"), "Source sibling text").unwrap();
    ConfigStore::global(&fixture.paths)
        .unwrap()
        .set(
            ConfigKey::new("hub.limits.content.input_bytes").unwrap(),
            json!(65536),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    ConfigStore::global(&fixture.paths)
        .unwrap()
        .set(
            ConfigKey::new("hub.limits.content.scalar_bytes").unwrap(),
            json!(65536),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    let daemon = fixture.daemon(config(&broker.issuer), true).await;
    let origin = fixture.origin();
    let asset = client()
        .get(format!("{origin}/task/gantt"))
        .send()
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    assert_eq!(
        asset.bytes().await.unwrap().as_ref(),
        include_bytes!("../assets/ui/index.html")
    );
    let discovery = fs::read(fixture.paths.root().join("daemon/mcp.json")).unwrap();
    let key = fs::read(fixture.paths.root().join("daemon/mcp.key")).unwrap();
    let client = client();
    let response = client
        .get(format!("{origin}/auth/providers"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"providers":["broker"]})
    );
    let cookie = login(&client, &origin, &broker.issuer).await;
    let context = read(&client, &origin, &cookie, "context", json!({})).await;
    assert_eq!(context.status(), StatusCode::OK);
    let selection: Value = context.json().await.unwrap();
    assert_eq!(
        selection["project_id"],
        serde_json::to_value(
            ProjectIdentityStore::at(&fixture.project)
                .load()
                .unwrap()
                .unwrap()
                .id()
        )
        .unwrap()
    );
    let task = read(
        &client,
        &origin,
        &cookie,
        "get",
        json!({"selection":selection,"input":{"id":fixture.task}}),
    )
    .await;
    assert_eq!(task.status(), StatusCode::OK);
    let task_text = task.text().await.unwrap();
    assert!(
        task_text.contains("2028-02-28")
            && task_text.contains("2028-03-01")
            && task_text.contains("2028-03-02")
    );
    assert!(!task_text.contains(&fixture.fence));
    assert!(
        !task_text.contains("access_token")
            && !task_text.contains("refresh-private")
            && !task_text.contains("eyJ")
            && !task_text.contains("claim_token")
    );
    let dependent = read(
        &client,
        &origin,
        &cookie,
        "get",
        json!({"selection":selection,"input":{"id":fixture.dependent}}),
    )
    .await
    .json::<Value>()
    .await
    .unwrap();
    assert_eq!(dependent["task"]["depends_on"][0], fixture.task);
    for (route, input) in [
        ("session", json!({"id":fixture.session})),
        ("session-heads", json!(null)),
        ("view", json!(null)),
        (
            "catalogue",
            json!({"filter":{},"page_size":1,"cursor":null}),
        ),
    ] {
        let response = read(
            &client,
            &origin,
            &cookie,
            route,
            json!({"selection":selection,"input":input}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "route {route}");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    let before = broker.issuer.calls.load(Ordering::SeqCst);
    let oversized = client
        .post(format!("{origin}/api/tasks/context"))
        .header(header::ORIGIN, &origin)
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(format!("{{\"padding\":\"{}\"}}", "x".repeat(65537)))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let invalid = read(
        &client,
        &origin,
        &cookie,
        "catalogue",
        json!({"selection":selection,"input":{"filter":{},"page_size":0,"cursor":null}}),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert_eq!(broker.issuer.calls.load(Ordering::SeqCst), before);

    let missing = read(
        &client,
        &origin,
        "",
        "get",
        json!({"selection":selection,"input":{"id":fixture.task}}),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    let wrong = client
        .post(format!("{origin}/api/tasks/context"))
        .header(header::ORIGIN, "http://foreign.example")
        .header("forwarded", format!("host={origin}"))
        .header(header::COOKIE, &cookie)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::FORBIDDEN);
    assert_eq!(broker.issuer.calls.load(Ordering::SeqCst), before);
    broker.issuer.allowed.store(false, Ordering::SeqCst);
    let denied = read(
        &client,
        &origin,
        &cookie,
        "get",
        json!({"selection":selection,"input":{"id":fixture.task}}),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(broker.issuer.calls.load(Ordering::SeqCst), before + 1);
    broker.issuer.allowed.store(true, Ordering::SeqCst);
    let source = fixture
        .command()
        .args(["ast", "source", "raw"])
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    assert_eq!(source.stdout, b"Source sibling text");
    assert_eq!(
        fs::read(fixture.paths.root().join("daemon/mcp.json")).unwrap(),
        discovery
    );
    assert_eq!(fs::read(fixture.paths.auth_file()).unwrap(), auth_before);
    assert_eq!(
        fs::read(fixture.paths.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    assert_eq!(broker.issuer.userinfo.load(Ordering::SeqCst), 0);
    daemon.stop("-INT").await;
    fixture.assert_clean();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_stop_cancels_actual_project_request_before_host_cleanup() {
    let broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    let daemon = fixture.daemon(config(&broker.issuer), false).await;
    let origin = fixture.origin();
    let client = client();
    let cookie = login(&client, &origin, &broker.issuer).await;
    broker.issuer.hold.store(true, Ordering::SeqCst);
    let request =
        tokio::spawn(async move { read(&client, &origin, &cookie, "context", json!({})).await });
    tokio::time::timeout(Duration::from_secs(5), broker.issuer.entered.notified())
        .await
        .unwrap();
    assert!(fixture.paths.root().join("daemon/mcp.json").exists());
    assert!(daemon.child.id() > 0);
    // The remote handler remains held: local shutdown must cancel its Original and join local owners.
    daemon.stop("-TERM").await;
    fixture.assert_clean();
    let response = request.await;
    assert!(response.is_err() || !response.unwrap().status().is_success());
    broker.issuer.release.notify_one();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_deadline_cancels_project_read_and_leaves_native_data_intact() {
    let broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    let value = config(&broker.issuer);
    let document = ConfigDocument::new(
        fixture.paths.brand(),
        ConfigSource::Global,
        [
            (ConfigKey::new("task.browser").unwrap(), value),
            (
                ConfigKey::new("hub.limits.request.deadline_ms").unwrap(),
                json!(1000),
            ),
        ],
    )
    .unwrap();
    let snapshot = ConfigSnapshot::new(fixture.paths.brand(), [document]).unwrap();
    let hub = HubResourcePolicy::resolve(&snapshot).unwrap();
    let cancellation = Cancellation::default();
    let original = hub.request_context(Instant::now(), &cancellation).unwrap();
    let owner = ForegroundSource::start(
        &fixture.paths,
        &[ProjectBootstrapInput {
            project: &fixture.project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &snapshot,
        NativeBinding::Codex,
        &original,
    )
    .unwrap();
    let origin = format!("http://{}", owner.address().unwrap());
    let service_stop = Cancellation::default();
    let service = service_stop.clone();
    let running = tokio::spawn(async move { owner.run(&service).await });
    let client = client();
    let cookie = login(&client, &origin, &broker.issuer).await;
    broker.issuer.hold.store(true, Ordering::SeqCst);
    let pending = {
        let client = client.clone();
        let cookie = cookie.clone();
        let origin = origin.clone();
        tokio::spawn(async move { read(&client, &origin, &cookie, "context", json!({})).await })
    };
    tokio::time::timeout(Duration::from_secs(5), broker.issuer.entered.notified())
        .await
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(3), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(!response.status().is_success());
    broker.issuer.hold.store(false, Ordering::SeqCst);
    broker.issuer.release.notify_one();
    let response = read(&client, &origin, &cookie, "context", json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let selection = response.json::<Value>().await.unwrap();
    let task = read(
        &client,
        &origin,
        &cookie,
        "get",
        json!({"selection":selection,"input":{"id":fixture.task}}),
    )
    .await;
    assert_eq!(task.status(), StatusCode::OK);
    assert!(task.text().await.unwrap().contains("2028-02-28"));
    // A pooled client abort need not close TCP. Exercise an actual EOF on the bound listener.
    use tokio::io::AsyncWriteExt;
    broker.issuer.hold.store(true, Ordering::SeqCst);
    let url = url::Url::parse(&origin).unwrap();
    let authority = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap());
    let mut socket = tokio::net::TcpStream::connect(&authority).await.unwrap();
    let wire = format!(
        "POST /api/tasks/context HTTP/1.1\r\nHost: {authority}\r\nOrigin: {origin}\r\nCookie: {cookie}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
    );
    socket.write_all(wire.as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), broker.issuer.entered.notified())
        .await
        .unwrap();
    drop(socket);
    // Hyper may retain the accepted handler until its finite Original expires.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    broker.issuer.hold.store(false, Ordering::SeqCst);
    let next = tokio::time::timeout(
        Duration::from_secs(3),
        read(&client, &origin, &cookie, "context", json!({})),
    )
    .await
    .unwrap();
    assert_eq!(next.status(), StatusCode::OK);
    // The old remote handler is still held; successful local read proves the credential lock drained.
    broker.issuer.release.notify_one();
    service_stop.cancel();
    running.await.unwrap().unwrap();
    fixture.assert_clean();
}
