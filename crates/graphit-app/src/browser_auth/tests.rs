use super::*;
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use axum::extract::State as FixtureState;
use graphit_auth::broker::{BrokerPolicy, JsonLimits, VerifierLimits};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicU64, AtomicUsize},
};
use tokio::net::TcpListener;

#[cfg(target_os = "linux")]
mod ast;
#[cfg(target_os = "linux")]
mod daemon;
#[cfg(target_os = "linux")]
mod hub_catalog;
#[cfg(target_os = "linux")]
mod imported_knowledge_heads;
#[cfg(target_os = "linux")]
#[path = "../live_agent_ast/tests.rs"]
mod live_ast;
#[cfg(target_os = "linux")]
mod live_browser;
#[cfg(target_os = "linux")]
mod live_catalog;
#[cfg(target_os = "linux")]
mod live_page;
#[cfg(target_os = "linux")]
mod now;
#[cfg(target_os = "linux")]
mod record_references;
#[cfg(target_os = "linux")]
pub(crate) mod remote_native_web;
mod workspace;

struct Issuer {
    origin: String,
    key: Ed25519KeyPair,
    nonce: StdMutex<String>,
    challenge: StdMutex<String>,
    issued: StdMutex<Vec<String>>,
    refresh: AtomicUsize,
    userinfo: AtomicUsize,
    user_status: StdMutex<StatusCode>,
    revoke_status: StdMutex<StatusCode>,
    token_status: StdMutex<StatusCode>,
    revocations: AtomicUsize,
    hold_refresh: AtomicBool,
    refresh_started: tokio::sync::Notify,
    release_refresh: tokio::sync::Notify,
    access_seconds: AtomicU64,
    profile: AtomicBool,
    issue_refresh: AtomicBool,
    project_grant: AtomicBool,
    project_write_grant: AtomicBool,
    denied_modules: StdMutex<Vec<String>>,
    denied_projects: StdMutex<Vec<Value>>,
    project_module: StdMutex<Option<String>>,
    project_calls: AtomicUsize,
    project_denials: StdMutex<Vec<Value>>,
    keys_calls: AtomicUsize,
    hold_project: AtomicBool,
    project_started: tokio::sync::Notify,
    release_project: tokio::sync::Notify,
    access_override: StdMutex<Option<String>>,
    extra_keys: StdMutex<Vec<Value>>,
}
impl Issuer {
    fn jwt(&self, value: Value) -> String {
        let message = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&json!({"alg":"EdDSA","kid":"one","typ":"JWT"})).unwrap()
            ),
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap())
        );
        format!(
            "{}.{}",
            message,
            URL_SAFE_NO_PAD.encode(self.key.sign(message.as_bytes()).as_ref())
        )
    }
}

async fn wait_until_access_expiration(fixture: &Fixture) {
    let expiration = {
        let issued = fixture.issuer.issued.lock().unwrap();
        let payload = issued.last().unwrap().split('.').nth(1).unwrap();
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
        UNIX_EPOCH + Duration::from_secs(claims["exp"].as_u64().unwrap())
    };
    tokio::time::sleep(
        expiration
            .duration_since(SystemTime::now())
            .unwrap_or_default(),
    )
    .await;
    assert!(SystemTime::now() >= expiration);
}

#[cfg(target_os = "linux")]
fn graphit_executable() -> std::path::PathBuf {
    let executable = std::env::var_os("GRAPHIT_TEST_GRAPHIT")
        .map(std::path::PathBuf::from)
        .expect("run cargo xtask cargo test to prepare the normal Graphit CLI artifact");
    assert!(executable.is_absolute() && executable.is_file());
    executable
}

pub(super) struct Fixture {
    issuer: Arc<Issuer>,
    pub(super) service: BrowserAuth,
    origin: String,
    client: reqwest::Client,
    stop: CancellationToken,
    servers: Vec<tokio::task::JoinHandle<()>>,
    tasks: Option<(crate::task_server::TaskServer, String)>,
    public_project: Option<task::PublicProject>,
    #[cfg(target_os = "linux")]
    graphs: Option<crate::graph_server::GraphServer>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
        for server in &self.servers {
            server.abort();
        }
    }
}
pub(super) fn policy() -> BrowserPolicy {
    BrowserPolicy {
        oauth: OAuthLimits {
            authorization_url_bytes: 8192,
            callback_bytes: 8192,
            callback_query_entries: 8,
            form_body_bytes: 8192,
            token_bytes: 4096,
            response_nodes: 32,
            response_scalar_bytes: 8192,
        },
        request_timeout: Duration::from_secs(5),
        login_timeout: Duration::from_secs(5),
        session_lifetime: Duration::from_secs(60),
        refresh_margin: Duration::ZERO,
        allow_loopback_http: true,
    }
}
impl Fixture {
    pub(super) async fn new(policy: BrowserPolicy) -> Self {
        Self::with_tasks(policy, None).await
    }
    async fn with_tasks(
        policy: BrowserPolicy,
        tasks: Option<(crate::task_server::TaskServer, String)>,
    ) -> Self {
        Self::configured(
            policy,
            tasks,
            None,
            #[cfg(target_os = "linux")]
            None,
        )
        .await
    }
    #[cfg(target_os = "linux")]
    async fn with_graphs(
        policy: BrowserPolicy,
        graphs: (crate::source_server::SourceServer, String),
    ) -> Self {
        Self::configured(policy, None, None, Some(graphs)).await
    }
    async fn configured(
        policy: BrowserPolicy,
        tasks: Option<(crate::task_server::TaskServer, String)>,
        public: Option<&std::ffi::OsStr>,
        #[cfg(target_os = "linux")] graphs: Option<(crate::source_server::SourceServer, String)>,
    ) -> Self {
        let (issuer, first) = Self::start_issuer().await;
        Self::configured_with_issuer(
            policy,
            tasks,
            public,
            #[cfg(target_os = "linux")]
            graphs,
            issuer,
            first,
        )
        .await
    }
    async fn start_issuer() -> (Arc<Issuer>, tokio::task::JoinHandle<()>) {
        let issuer_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer_origin = format!("http://{}", issuer_listener.local_addr().unwrap());
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let issuer = Arc::new(Issuer {
            origin: issuer_origin,
            key: Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap(),
            nonce: StdMutex::new(String::new()),
            challenge: StdMutex::new(String::new()),
            issued: StdMutex::new(Vec::new()),
            refresh: AtomicUsize::new(0),
            userinfo: AtomicUsize::new(0),
            user_status: StdMutex::new(StatusCode::OK),
            revoke_status: StdMutex::new(StatusCode::OK),
            token_status: StdMutex::new(StatusCode::OK),
            revocations: AtomicUsize::new(0),
            hold_refresh: AtomicBool::new(false),
            refresh_started: tokio::sync::Notify::new(),
            release_refresh: tokio::sync::Notify::new(),
            access_seconds: AtomicU64::new(300),
            profile: AtomicBool::new(true),
            issue_refresh: AtomicBool::new(true),
            project_grant: AtomicBool::new(true),
            project_write_grant: AtomicBool::new(true),
            denied_modules: StdMutex::new(Vec::new()),
            denied_projects: StdMutex::new(Vec::new()),
            project_module: StdMutex::new(None),
            project_calls: AtomicUsize::new(0),
            project_denials: StdMutex::new(Vec::new()),
            keys_calls: AtomicUsize::new(0),
            hold_project: AtomicBool::new(false),
            project_started: tokio::sync::Notify::new(),
            release_project: tokio::sync::Notify::new(),
            access_override: StdMutex::new(None),
            extra_keys: StdMutex::new(Vec::new()),
        });
        let router = Router::new()
            .route("/.well-known/graphit-broker", get(broker_metadata))
            .route("/.well-known/openid-configuration", get(oidc_metadata))
            .route("/keys", get(keys))
            .route("/authorize", get(authorize))
            .route("/register", post(register))
            .route("/token", post(tokens))
            .route("/userinfo", get(userinfo))
            .route("/revoke", post(revoke))
            .route("/v1/project/access/authorize", post(project_access))
            .with_state(issuer.clone());
        let first = tokio::spawn(async move {
            axum::serve(issuer_listener, router).await.unwrap();
        });
        (issuer, first)
    }
    async fn configured_with_issuer(
        policy: BrowserPolicy,
        tasks: Option<(crate::task_server::TaskServer, String)>,
        public: Option<&std::ffi::OsStr>,
        #[cfg(target_os = "linux")] graphs: Option<(crate::source_server::SourceServer, String)>,
        issuer: Arc<Issuer>,
        first: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self::configured_host(
            policy,
            tasks,
            public,
            #[cfg(target_os = "linux")]
            graphs,
            issuer,
            first,
            false,
        )
        .await
    }
    #[cfg(target_os = "linux")]
    async fn with_live_tasks(
        policy: BrowserPolicy,
        tasks: (crate::task_server::TaskServer, String),
    ) -> Self {
        let (issuer, first) = Self::start_issuer().await;
        Self::configured_host(policy, Some(tasks), None, None, issuer, first, true).await
    }
    async fn configured_host(
        policy: BrowserPolicy,
        tasks: Option<(crate::task_server::TaskServer, String)>,
        public: Option<&std::ffi::OsStr>,
        #[cfg(target_os = "linux")] graphs: Option<(crate::source_server::SourceServer, String)>,
        issuer: Arc<Issuer>,
        first: tokio::task::JoinHandle<()>,
        live_browser: bool,
    ) -> Self {
        let (public_project, project_endpoint) = if let Some(binary) = public {
            let selection =
                crate::task_server::tests::browser_selection(&tasks.as_ref().unwrap().0);
            let (process, ready) =
                task::PublicProject::start(binary, &issuer.origin, selection["project_id"].clone())
                    .await;
            *issuer.access_override.lock().unwrap() =
                Some(ready["access_bearer"].as_str().unwrap().to_owned());
            *issuer.extra_keys.lock().unwrap() =
                ready["trusted_jwks"]["keys"].as_array().unwrap().clone();
            (
                Some(process),
                ready["authorize_endpoint"].as_str().unwrap().to_owned(),
            )
        } else {
            (
                None,
                format!("{}/v1/project/access/authorize", issuer.origin),
            )
        };
        let provider_revision = {
            #[cfg(target_os = "linux")]
            if let Some((server, raw)) = &graphs {
                crate::graph_server::tests::browser_provider(
                    &server.graph_server(),
                    raw,
                    &issuer.origin,
                )
            } else {
                tasks.as_ref().map_or(7, |(server, raw)| {
                    crate::task_server::tests::browser_provider(server, raw, &issuer.origin, false)
                })
            }
            #[cfg(not(target_os = "linux"))]
            {
                tasks.as_ref().map_or(7, |(server, raw)| {
                    crate::task_server::tests::browser_provider(server, raw, &issuer.origin, false)
                })
            }
        };
        let verifier = Arc::new(
            BrokerVerifier::new(
                BrokerPolicy {
                    endpoint: &issuer.origin,
                    mcp_resource: None,
                    provider_revision,
                    network: ClientPolicy {
                        origin: &issuer.origin,
                        allow_loopback_http: true,
                        proxy: None,
                        additional_roots: &[],
                    },
                },
                NetworkLimits {
                    url_bytes: 8192,
                    request_body_bytes: 8192,
                    response_body_bytes: 32768,
                    max_in_flight: 8,
                    header_bytes: 16384,
                    header_count: 32,
                    max_idle_per_host: 2,
                    idle_timeout: Duration::from_secs(30),
                    max_certificates: 2,
                    max_certificate_bytes: 8192,
                },
                VerifierLimits {
                    token_bytes: 4096,
                    json: JsonLimits {
                        bytes: 32768,
                        nodes: 256,
                        depth: 16,
                        scalar_bytes: 8192,
                        collection_entries: 64,
                    },
                    max_keys: 8,
                    metadata_ttl: Duration::from_secs(60),
                    key_ttl: Duration::from_secs(60),
                    unknown_key_cooldown: Duration::from_secs(1),
                },
            )
            .unwrap(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stop = CancellationToken::new();
        let service = BrowserAuth::new(
            vec![("broker".into(), verifier)],
            &origin,
            policy,
            stop.clone(),
        )
        .unwrap();
        let router = if let Some((server, _)) = &tasks {
            let backend = task::backend(&project_endpoint);
            service.task_router(
                server.clone(),
                Arc::new(backend),
                #[cfg(target_os = "linux")]
                graphs.as_ref().map(|(server, _)| server.clone()),
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                None,
            )
        } else {
            service.router()
        };
        #[cfg(target_os = "linux")]
        let router = if let Some((server, _)) = &graphs {
            router
                .merge(service.daemon_router(server.clone()))
                .merge(service.now_router(server.clone()))
                .merge(
                    service.ast_router(server.clone(), Arc::new(task::backend(&project_endpoint))),
                )
        } else {
            router
        };
        #[cfg(target_os = "linux")]
        let router = if live_browser {
            let server = &tasks.as_ref().expect("Live browser TaskServer").0;
            router
                .merge(
                    service.live_execution_router(
                        server.clone(),
                        Arc::new(task::backend(&project_endpoint)),
                        listener.local_addr().unwrap(),
                        std::env::current_exe()
                            .unwrap()
                            .parent()
                            .unwrap()
                            .parent()
                            .unwrap()
                            .join("graphit"),
                        graphs.as_ref().map(|(server, _)| server.clone()),
                    ),
                )
                .merge(crate::foreground_ui::router())
                .layer(axum::middleware::from_fn_with_state(
                    std::path::PathBuf::from(
                        std::env::var_os("GRAPHIT_UI_BROWSER_PROBE_FILE")
                            .expect("explicit browser probe path"),
                    ),
                    live_browser::diagnostics,
                ))
        } else {
            router
        };
        #[cfg(not(target_os = "linux"))]
        assert!(!live_browser);
        let second = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let server_timeout = tasks
            .as_ref()
            .map_or(policy.request_timeout, |(server, _)| {
                server.browser_policy().1
            })
            .max(policy.request_timeout);
        #[cfg(target_os = "linux")]
        let server_timeout = graphs.as_ref().map_or(server_timeout, |(server, _)| {
            server_timeout.max(server.policy().timeout)
        });
        let client_timeout = server_timeout.checked_add(Duration::from_secs(5)).unwrap();
        Self {
            issuer,
            service,
            origin,
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(client_timeout)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            stop,
            servers: vec![first, second],
            tasks,
            public_project,
            #[cfg(target_os = "linux")]
            graphs: graphs.map(|(server, _)| server.graph_server()),
        }
    }
    async fn start(&self) -> (String, String) {
        let metadata = self
            .client
            .get(format!("{}/auth/providers", self.origin))
            .send()
            .await
            .unwrap();
        assert_eq!(metadata.status(), StatusCode::OK);
        assert_eq!(
            metadata.json::<Value>().await.unwrap(),
            json!({"providers":["broker"]})
        );
        let response = self
            .client
            .post(format!("{}/auth/login", self.origin))
            .header(header::ORIGIN, &self.origin)
            .json(&json!({"provider":"broker"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let cookie = cookie_header(&response);
        assert!(!response.headers().contains_key(header::LOCATION));
        let response = response.json::<Value>().await.unwrap();
        assert_eq!(response.as_object().unwrap().len(), 1);
        let url = Url::parse(response["authorization_url"].as_str().unwrap()).unwrap();
        let fields: HashMap<String, String> = url.query_pairs().into_owned().collect();
        *self.issuer.nonce.lock().unwrap() = fields["nonce"].clone();
        *self.issuer.challenge.lock().unwrap() = fields["code_challenge"].clone();
        assert_eq!(fields["resource"], format!("{}/v1", self.issuer.origin));
        assert_eq!(fields["client_id"], "web-client");
        (
            cookie,
            format!(
                "{}/auth/callback?code=code&state={}",
                self.origin, fields["state"]
            ),
        )
    }
    pub(super) async fn login(&self) -> String {
        let (cookie, callback) = self.start().await;
        let response = self
            .client
            .get(callback)
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/");
        let header = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .find(|h| h.to_str().unwrap().starts_with("graphit_session="))
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            header.contains("HttpOnly")
                && header.contains("SameSite=Strict")
                && header.contains("Path=/")
        );
        assert!(!header.contains("access_token") && !header.contains("refresh"));
        header.split(';').next().unwrap().to_owned()
    }
    async fn identity(&self, cookie: &str) -> reqwest::Response {
        self.client
            .get(format!("{}/auth/session", self.origin))
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap()
    }
}
fn cookie_header(response: &reqwest::Response) -> String {
    let header = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(header.contains("HttpOnly") && header.contains("SameSite=Lax"));
    header.split(';').next().unwrap().to_owned()
}
async fn broker_metadata(FixtureState(s): FixtureState<Arc<Issuer>>) -> Json<Value> {
    Json(
        json!({"version":"1","issuer":s.origin,"authentication":{"type":"oidc","issuer":s.origin,"client_id":"native-client","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":["https://mcp.example/user-memory"]}}),
    )
}
async fn oidc_metadata(FixtureState(s): FixtureState<Arc<Issuer>>) -> Json<Value> {
    let mut metadata = json!({"issuer":s.origin,"authorization_endpoint":format!("{}/authorize",s.origin),"registration_endpoint":format!("{}/register",s.origin),"token_endpoint":format!("{}/token",s.origin),"revocation_endpoint":format!("{}/revoke",s.origin),"jwks_uri":format!("{}/keys",s.origin),"userinfo_endpoint":format!("{}/userinfo",s.origin),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]});
    if !s.profile.load(Ordering::Acquire) {
        metadata
            .as_object_mut()
            .unwrap()
            .remove("userinfo_endpoint");
    }
    Json(metadata)
}
async fn authorize(
    FixtureState(issuer): FixtureState<Arc<Issuer>>,
    axum::extract::RawQuery(raw): axum::extract::RawQuery,
) -> axum::response::Redirect {
    let fields: HashMap<String, String> =
        url::form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    assert_eq!(fields["client_id"], "web-client");
    assert_eq!(fields["response_type"], "code");
    assert_eq!(fields["code_challenge_method"], "S256");
    assert_eq!(fields["resource"], format!("{}/v1", issuer.origin));
    let mut callback = Url::parse(&fields["redirect_uri"]).unwrap();
    assert_eq!(callback.host_str(), Some("127.0.0.1"));
    assert_eq!(callback.path(), "/auth/callback");
    *issuer.nonce.lock().unwrap() = fields["nonce"].clone();
    *issuer.challenge.lock().unwrap() = fields["code_challenge"].clone();
    callback
        .query_pairs_mut()
        .append_pair("code", "code")
        .append_pair("state", &fields["state"]);
    axum::response::Redirect::to(callback.as_str())
}
async fn keys(FixtureState(s): FixtureState<Arc<Issuer>>) -> Json<Value> {
    s.keys_calls.fetch_add(1, Ordering::SeqCst);
    let mut keys = s.extra_keys.lock().unwrap().clone();
    keys.push(json!({"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(s.key.public_key().as_ref()),"kid":"one","alg":"EdDSA","use":"sig","key_ops":["verify"]}));
    Json(json!({"keys":keys}))
}
async fn project_access(
    FixtureState(s): FixtureState<Arc<Issuer>>,
    headers: HeaderMap,
    Json(value): Json<Value>,
) -> (StatusCode, Json<Value>) {
    s.project_calls.fetch_add(1, Ordering::SeqCst);
    let bearer = headers[header::AUTHORIZATION]
        .to_str()
        .unwrap()
        .strip_prefix("Bearer ")
        .unwrap();
    // Refresh rotation does not revoke access tokens already in flight.
    assert!(
        s.issued.lock().unwrap().iter().any(|token| token == bearer),
        "PROJECT must receive an issued access token"
    );
    assert_eq!(value.as_object().unwrap().len(), 3);
    assert!(matches!(
        value["module"].as_str(),
        Some("task" | "memory" | "knowledge" | "ast" | "hub")
    ));
    assert!(matches!(value["action"].as_str(), Some("read" | "write")));
    if s.hold_project.load(Ordering::Acquire) {
        s.project_started.notify_one();
        s.release_project.notified().await;
    }
    if !s.project_grant.load(Ordering::Acquire)
        || (value["action"] == "write" && !s.project_write_grant.load(Ordering::Acquire))
        || s.denied_modules
            .lock()
            .unwrap()
            .iter()
            .any(|module| Some(module.as_str()) == value["module"].as_str())
        || s.denied_projects
            .lock()
            .unwrap()
            .contains(&value["project_id"])
    {
        s.project_denials.lock().unwrap().push(value);
        return (StatusCode::FORBIDDEN, Json(json!({})));
    }
    (
        StatusCode::OK,
        Json(
            json!({"subject":{"issuer":s.origin,"subject":"web-subject"},"authorization_revision":1,"project_id":value["project_id"],"module":s.project_module.lock().unwrap().as_deref().unwrap_or(value["module"].as_str().unwrap()),"action":value["action"]}),
        ),
    )
}
async fn register(Json(value): Json<Value>) -> (StatusCode, Json<Value>) {
    // These fixtures use a local HTTP callback, which OIDC registers as native.
    assert_eq!(value["application_type"], "native");
    assert_eq!(value["token_endpoint_auth_method"], "none");
    (
        StatusCode::CREATED,
        Json(
            json!({"client_id":"web-client","application_type":value["application_type"],"redirect_uris":value["redirect_uris"]}),
        ),
    )
}
async fn tokens(
    FixtureState(s): FixtureState<Arc<Issuer>>,
    body: String,
) -> (StatusCode, Json<Value>) {
    let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    assert_eq!(form["client_id"], "web-client");
    assert_eq!(form["resource"], format!("{}/v1", s.origin));
    let refresh = s.refresh.fetch_add(1, Ordering::SeqCst) + 1;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let seconds = s.access_seconds.load(Ordering::Acquire);
    let mut claims = json!({"iss":s.origin,"sub":"web-subject","client_id":"web-client","token_use":"access","jti":format!("jwt-{refresh}"),"aud":["broker-api",format!("{}/v1",s.origin)],"iat":now,"exp":now+seconds,"preferred_username":"web-user"});
    if !s.profile.load(Ordering::Acquire) {
        claims.as_object_mut().unwrap().remove("preferred_username");
    }
    let access = s
        .access_override
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| s.jwt(claims));
    s.issued.lock().unwrap().push(access.clone());
    let mut response = json!({"access_token":access,"token_type":"Bearer","expires_in":seconds});
    if s.issue_refresh.load(Ordering::Acquire) {
        response["refresh_token"] = format!("refresh-{refresh}").into();
    }
    if form["grant_type"] == "authorization_code" {
        assert_eq!(
            URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes())),
            *s.challenge.lock().unwrap()
        );
        let mut claims = json!({"iss":s.origin,"sub":"web-subject","aud":"web-client","iat":now,"exp":now+3600,"nonce":s.nonce.lock().unwrap().clone(),"preferred_username":"web-user"});
        if !s.profile.load(Ordering::Acquire) {
            claims.as_object_mut().unwrap().remove("preferred_username");
        }
        response["id_token"] = s.jwt(claims).into();
    } else {
        assert_eq!(form["refresh_token"], format!("refresh-{}", refresh - 1));
        if s.hold_refresh.load(Ordering::Acquire) {
            s.refresh_started.notify_one();
            s.release_refresh.notified().await;
        }
    }
    (*s.token_status.lock().unwrap(), Json(response))
}
async fn userinfo(
    FixtureState(s): FixtureState<Arc<Issuer>>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    s.userinfo.fetch_add(1, Ordering::SeqCst);
    assert_eq!(
        headers[header::AUTHORIZATION].to_str().unwrap(),
        format!("Bearer {}", s.issued.lock().unwrap().last().unwrap())
    );
    (
        *s.user_status.lock().unwrap(),
        Json(json!({"sub":"web-subject"})),
    )
}
async fn revoke(FixtureState(s): FixtureState<Arc<Issuer>>, body: String) -> StatusCode {
    assert!(body.contains("refresh-"));
    s.revocations.fetch_add(1, Ordering::SeqCst);
    *s.revoke_status.lock().unwrap()
}

#[tokio::test]
async fn signed_cookie_login_local_jwt_and_logout_are_real() {
    let fixture = Fixture::new(policy()).await;
    let cookie = fixture.login().await;
    for _ in 0..2 {
        let response = fixture.identity(&cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.text().await.unwrap();
        assert!(body.contains("web-subject") && body.contains("web-user"));
        assert!(
            !body.contains("access_token") && !body.contains("refresh-") && !body.contains("eyJ")
        );
    }
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    let response = fixture
        .client
        .post(format!("{}/auth/logout", fixture.origin))
        .header(header::ORIGIN, &fixture.origin)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(fixture.issuer.revocations.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.identity(&cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
    fixture.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn automatic_refresh_rotation_uses_signed_jwt_without_userinfo() {
    let mut p = policy();
    p.refresh_margin = Duration::from_secs(600);
    let fixture = Fixture::new(p).await;
    let cookie = fixture.login().await;
    let (a, b) = tokio::join!(fixture.identity(&cookie), fixture.identity(&cookie));
    assert_eq!(a.status(), StatusCode::OK);
    assert_eq!(b.status(), StatusCode::OK);
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 3);
    *fixture.issuer.user_status.lock().unwrap() = StatusCode::UNAUTHORIZED;
    assert_eq!(fixture.identity(&cookie).await.status(), StatusCode::OK);
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 4);
    fixture.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn wrong_origin_state_cookie_and_replay_are_denied_without_token_effect() {
    let fixture = Fixture::new(policy()).await;
    let denied = fixture
        .client
        .post(format!("{}/auth/login", fixture.origin))
        .header(header::ORIGIN, "https://foreign.example")
        .json(&json!({"provider":"broker"}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let unknown = fixture
        .client
        .post(format!("{}/auth/login", fixture.origin))
        .header(header::ORIGIN, &fixture.origin)
        .json(&json!({"provider":"unknown"}))
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
    let (cookie, callback) = fixture.start().await;
    let failed = fixture
        .client
        .get(format!("{}&state=foreign", callback))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(failed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 0);
    assert_eq!(
        fixture
            .client
            .get(callback)
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        fixture.identity("graphit_session=malformed").await.status(),
        StatusCode::UNAUTHORIZED
    );
    fixture.service.shutdown().await.unwrap();
}
#[tokio::test]
async fn expiry_shutdown_and_remote_logout_failure_leave_no_session() {
    let mut p = policy();
    p.session_lifetime = Duration::from_secs(1);
    let fixture = Fixture::new(p).await;
    let cookie = fixture.login().await;
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut jobs = fixture.service.0.jobs.lock().await;
        while let Some(completed) = jobs.join_next().await {
            completed.unwrap();
        }
    })
    .await
    .expect("the actual session expiry job must finish");
    assert!(fixture.service.0.sessions.lock().await.is_empty());
    assert_eq!(
        fixture.identity(&cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let cookie = fixture.login().await;
    *fixture.issuer.revoke_status.lock().unwrap() = StatusCode::INTERNAL_SERVER_ERROR;
    assert_eq!(
        fixture
            .client
            .post(format!("{}/auth/logout", fixture.origin))
            .header(header::ORIGIN, &fixture.origin)
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        fixture.identity(&cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let (flow, callback) = fixture.start().await;
    fixture.service.shutdown().await.unwrap();
    assert_eq!(
        fixture
            .client
            .get(callback)
            .header(header::COOKIE, flow)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(fixture.service.0.flows.lock().await.is_empty());
    assert!(fixture.service.0.sessions.lock().await.is_empty());
}
#[tokio::test]
async fn flow_timeout_and_configuration_cookie_contract() {
    let mut p = policy();
    p.login_timeout = Duration::from_secs(1);
    let fixture = Fixture::new(p).await;
    let (cookie, callback) = fixture.start().await;
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut jobs = fixture.service.0.jobs.lock().await;
        while let Some(completed) = jobs.join_next().await {
            completed.unwrap();
        }
    })
    .await
    .expect("the actual login expiry job must finish");
    assert!(fixture.service.0.flows.lock().await.is_empty());
    assert_eq!(
        fixture
            .client
            .get(callback)
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let providers = vec![("broker".into(), fixture.service.0.providers[0].1.clone())];
    let secure = BrowserAuth::new(
        providers.clone(),
        "https://workspace.example",
        policy(),
        CancellationToken::new(),
    )
    .unwrap();
    let header = secure.0.cookie(false, "opaque".into()).to_string();
    assert!(
        header.contains("Secure")
            && header.contains("HttpOnly")
            && header.starts_with("__Host-graphit_session=")
    );
    for origin in [
        "http://workspace.example",
        "https://workspace.example/foreign",
        "https://u@workspace.example",
        "https://workspace.example?query=1",
        "https://workspace.example#fragment",
    ] {
        assert!(
            BrowserAuth::new(
                providers.clone(),
                origin,
                policy(),
                CancellationToken::new()
            )
            .is_err()
        );
    }
    fixture.service.shutdown().await.unwrap();
    secure.shutdown().await.unwrap();
}

#[tokio::test]
async fn successful_callback_replay_and_cross_origin_logout_cannot_reuse_or_destroy_session() {
    let fixture = Fixture::new(policy()).await;
    let (flow_cookie, callback) = fixture.start().await;
    let response = fixture
        .client
        .get(&callback)
        .header(header::COOKIE, &flow_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let session_cookie = response
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
        .to_owned();
    assert_eq!(
        fixture
            .client
            .get(callback)
            .header(header::COOKIE, flow_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let denied = fixture
        .client
        .post(format!("{}/auth/logout", fixture.origin))
        .header(header::ORIGIN, "https://foreign.example")
        .header(header::COOKIE, &session_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(fixture.issuer.revocations.load(Ordering::SeqCst), 0);
    assert_eq!(
        fixture.identity(&session_cookie).await.status(),
        StatusCode::OK
    );
    fixture.service.shutdown().await.unwrap();
    assert_eq!(
        fixture.identity(&session_cookie).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn ambiguous_rotating_refresh_failure_does_not_retry_retired_token() {
    let mut p = policy();
    p.refresh_margin = Duration::from_secs(600);
    let fixture = Fixture::new(p).await;
    let cookie = fixture.login().await;
    *fixture.issuer.token_status.lock().unwrap() = StatusCode::INTERNAL_SERVER_ERROR;
    assert_eq!(
        fixture.identity(&cookie).await.status(),
        StatusCode::BAD_GATEWAY
    );
    let count = fixture.issuer.refresh.load(Ordering::SeqCst);
    assert_eq!(count, 2);
    assert_eq!(
        fixture.identity(&cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), count);
    fixture.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn valid_signed_jwt_needs_no_userinfo_even_when_remote_account_is_denied() {
    let fixture = Fixture::new(policy()).await;
    let cookie = fixture.login().await;
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        *fixture.issuer.user_status.lock().unwrap() = status;
        let response = fixture.identity(&cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = response.text().await.unwrap();
        assert!(body.contains("web-subject") && body.contains("web-user"));
        assert!(
            !body.contains("access_token") && !body.contains("refresh-") && !body.contains("eyJ")
        );
    }
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 1);
    fixture.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn dropped_rotating_refresh_future_does_not_reuse_retired_token() {
    let mut p = policy();
    p.refresh_margin = Duration::from_secs(600);
    let fixture = Fixture::new(p).await;
    let cookie = fixture.login().await;
    fixture.issuer.hold_refresh.store(true, Ordering::Release);
    let jar = CookieJar::new().add(Cookie::parse(cookie.clone()).unwrap().into_owned());
    let task = tokio::spawn(session::identity(State(fixture.service.clone()), jar));
    tokio::time::timeout(
        Duration::from_secs(2),
        fixture.issuer.refresh_started.notified(),
    )
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    fixture.issuer.release_refresh.notify_one();
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 2);
    assert_eq!(
        fixture.identity(&cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 2);
    fixture.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn missing_signed_profile_is_null_and_access_expiry_denies_without_refresh() {
    let fixture = Fixture::new(policy()).await;
    fixture.issuer.profile.store(false, Ordering::Release);
    fixture.issuer.issue_refresh.store(false, Ordering::Release);
    fixture.issuer.access_seconds.store(2, Ordering::Release);
    let cookie = fixture.login().await;
    let response = fixture.identity(&cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["subject"], "web-subject");
    assert!(value["username"].is_null());
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    wait_until_access_expiration(&fixture).await;
    assert_eq!(
        fixture.identity(&cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(fixture.issuer.refresh.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    fixture.service.shutdown().await.unwrap();
}

mod references;
mod task;

#[cfg(target_os = "linux")]
mod memory;

#[cfg(target_os = "linux")]
mod knowledge;

#[cfg(target_os = "linux")]
mod user_memory;

#[cfg(target_os = "linux")]
#[path = "../knowledge_live/browser_tests.rs"]
mod live;

#[cfg(target_os = "linux")]
mod live_mcp;

#[cfg(target_os = "linux")]
mod live_execute;
