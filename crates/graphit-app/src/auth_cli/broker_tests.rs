#[path = "device_tests.rs"]
mod device;
use super::super::{
    Command,
    tests::{Fixture, request},
};
use super::*;
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use axum::{
    Json,
    extract::State as AxumState,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_auth::{
    AuthenticatedCaller,
    broker::{BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits, oauth::OAuthLimits},
    profile_manager::{InteractiveLoginLimits, InteractiveTransportError},
};
use graphit_core::{auth_document::publication::AuthPublicationEffect, file_lock::Cancellation};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use graphit_hub::policy::RequestContext;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Notify,
};

struct State {
    origin: String,
    key: Ed25519KeyPair,
    flow: Mutex<Option<BTreeMap<String, String>>>,
    invalid: Mutex<Option<&'static str>>,
    rotation: AtomicUsize,
    pause: AtomicBool,
    started: Notify,
    release: Notify,
}
struct Broker {
    state: Arc<State>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.server.abort();
    }
}
fn signed(key: &Ed25519KeyPair, claims: Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA","kid":"one","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
    let input = format!("{header}.{payload}");
    format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(input.as_bytes()).as_ref())
    )
}
async fn metadata(AxumState(s): AxumState<Arc<State>>) -> Json<Value> {
    if *s.invalid.lock().unwrap() == Some("metadata") {
        return Json(json!({"version":"unsupported"}));
    }
    Json(
        json!({"version":"1","issuer":s.origin,"authentication":{"type":"oidc","issuer":s.origin,"client_id":"native-client","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["native-api"],"access_token_audience":"native-api","mcp_resources":[]}}),
    )
}
async fn discovery(AxumState(s): AxumState<Arc<State>>) -> Json<Value> {
    Json(
        json!({"issuer":s.origin,"authorization_endpoint":format!("{}/authorize",s.origin),"token_endpoint":format!("{}/token",s.origin),"jwks_uri":format!("{}/keys",s.origin),"device_authorization_endpoint":format!("{}/device/authorize",s.origin),"grant_types_supported":["authorization_code","refresh_token","urn:ietf:params:oauth:grant-type:device_code"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}),
    )
}
async fn keys(AxumState(s): AxumState<Arc<State>>) -> Json<Value> {
    Json(
        json!({"keys":[{"kty":"OKP","crv":"Ed25519","kid":"one","alg":"EdDSA","use":"sig","key_ops":["verify"],"x":URL_SAFE_NO_PAD.encode(s.key.public_key().as_ref())}]}),
    )
}
async fn token(AxumState(s): AxumState<Arc<State>>, body: String) -> Response {
    let form: BTreeMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    if form.get("grant_type").map(String::as_str)
        == Some("urn:ietf:params:oauth:grant-type:device_code")
    {
        if form.len() != 3
            || form.get("client_id").map(String::as_str) != Some("native-client")
            || form.get("device_code").map(String::as_str) != Some("device-secret-canary")
        {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid_request"})),
            )
                .into_response();
        }
        s.started.notify_one();
        if s.pause.load(Ordering::SeqCst) {
            s.release.notified().await;
        }
        s.rotation.fetch_add(1, Ordering::SeqCst);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        return Json(json!({"access_token":signed(&s.key,json!({"iss":s.origin,"sub":"device-subject","aud":"native-api","client_id":"native-client","token_use":"access","jti":"device-jti","iat":now,"exp":now+3600})),"token_type":"Bearer","expires_in":300,"scope":""})).into_response();
    }
    let flow = s.flow.lock().unwrap().clone().unwrap();
    let grant = form.get("grant_type").map(String::as_str);
    let valid = form.get("client_id").map(String::as_str) == Some("native-client")
        && match grant {
            Some("authorization_code") => {
                form.get("code").map(String::as_str) == Some("code-canary")
                    && form.get("redirect_uri") == flow.get("redirect_uri")
                    && form.get("code_verifier").is_some_and(|v| {
                        URL_SAFE_NO_PAD.encode(Sha256::digest(v.as_bytes()))
                            == flow["code_challenge"]
                    })
            }
            Some("refresh_token") => {
                form.get("refresh_token").map(String::as_str) == Some("refresh-token-1")
            }
            _ => false,
        };
    if !valid {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_grant"})),
        )
            .into_response();
    }
    s.started.notify_one();
    if s.pause.load(Ordering::SeqCst) {
        s.release.notified().await;
    }
    let serial = s.rotation.fetch_add(1, Ordering::SeqCst) + 1;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let invalid = *s.invalid.lock().unwrap();
    let access = json!({"iss":s.origin,"sub":"native-subject","aud":[if invalid==Some("aud") {"foreign-api"} else {"native-api"}],"client_id":"native-client","token_use":"access","jti":format!("token-{serial}"),"iat":now,"exp":now+3600});
    let mut id = json!({"iss":s.origin,"sub":"native-subject","aud":"native-client","iat":now,"exp":now+3600,"preferred_username":"signed-native-user","organization":"signed-org","groups":["signed-team"]});
    if grant == Some("authorization_code") {
        id["nonce"] = if invalid == Some("nonce") {
            "wrong-nonce".into()
        } else {
            flow["nonce"].clone().into()
        };
    }
    Json(json!({"access_token":signed(&s.key,access),"id_token":signed(&s.key,id),"refresh_token":format!("refresh-token-{serial}"),"token_type":"Bearer","expires_in":300})).into_response()
}
impl Broker {
    async fn new() -> Self {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let state = Arc::new(State {
            origin: format!("http://{}", listener.local_addr().unwrap()),
            key,
            flow: Mutex::new(None),
            invalid: Mutex::new(None),
            rotation: AtomicUsize::new(0),
            pause: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
        });
        let router = axum::Router::new()
            .route("/.well-known/graphit-broker", get(metadata))
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/keys", get(keys))
            .route("/device/authorize", post(|AxumState(s): AxumState<Arc<State>>,body:String| async move {
                assert_eq!(url::form_urlencoded::parse(body.as_bytes()).into_owned().collect::<BTreeMap<String,String>>(),BTreeMap::from([("client_id".into(),"native-client".into())]));
                Json(json!({"device_code":"device-secret-canary","user_code":"ABCD-EFGH","verification_uri":format!("{}/device",s.origin),"expires_in":600,"interval":1}))
            }))
            .route("/token", post(token))
            .with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { state, server }
    }
    fn origin(&self) -> &str {
        &self.state.origin
    }
    fn callback(&self, authorization: &str) -> url::Url {
        let url = url::Url::parse(authorization).unwrap();
        let flow: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
        let mut callback = url::Url::parse(&flow["redirect_uri"]).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("code", "code-canary")
            .append_pair("state", &flow["state"]);
        *self.state.flow.lock().unwrap() = Some(flow);
        callback
    }
    fn verifier(&self, f: &Fixture) -> BrokerVerifier {
        let revision = f.read(&["provider", "show", "broker"])["provider_revision"]
            .as_u64()
            .unwrap();
        BrokerVerifier::new(
            BrokerPolicy {
                endpoint: self.origin(),
                mcp_resource: None,
                provider_revision: revision,
                network: ClientPolicy {
                    origin: self.origin(),
                    allow_loopback_http: true,
                    proxy: None,
                    additional_roots: &[],
                },
            },
            NetworkLimits {
                request_body_bytes: 8192,
                response_body_bytes: 8192,
                url_bytes: 4096,
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
                    nodes: 512,
                    depth: 16,
                    scalar_bytes: 2048,
                    collection_entries: 64,
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
fn oauth_limits() -> OAuthLimits {
    OAuthLimits {
        authorization_url_bytes: 4096,
        callback_bytes: 4096,
        callback_query_entries: 16,
        form_body_bytes: 8192,
        token_bytes: 8192,
        response_nodes: 512,
        response_scalar_bytes: 2048,
    }
}
fn config() -> InteractiveLoginConfig<'static> {
    InteractiveLoginConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        callback_path: "/callback",
        limits: InteractiveLoginLimits {
            head_bytes: 4096,
            header_entries: 16,
            target_bytes: 2048,
            callback_bytes: 4096,
        },
    }
}
fn now() -> io::Result<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)
        .and_then(|v| i64::try_from(v.as_secs()).map_err(io::Error::other))
}
fn applied(p: AuthDocumentPublication) {
    assert_eq!(p.effect(), AuthPublicationEffect::Applied);
    assert!(p.failure().is_none());
    assert!(p.cleanup_failure().is_none());
    assert!(p.into_pending_cleanup().is_none());
}
fn login_args() -> LoginArguments {
    match super::super::tests::command(&[
        "login",
        "--provider",
        "broker",
        "--profile",
        "remote",
        "--s3-disabled",
    ]) {
        Command::Login(input) => *input,
        _ => panic!("login args"),
    }
}
fn seed(f: &Fixture, b: &Broker) {
    f.publish(
        &[
            "provider",
            "add",
            "broker",
            "--type",
            "broker",
            "--broker-endpoint",
            b.origin(),
            "--embedding-mode",
            "broker",
            "--rerank-mode",
            "broker",
            "--s3-credential-source",
            "broker",
        ],
        1,
    );
}
fn caller<'a>(f: &'a Fixture, ctx: &'a RequestContext<'a>) -> AuthenticatedCaller<'a> {
    let key = f.auth.runtime().check(ctx.snapshot()).unwrap();
    f.auth
        .issuer()
        .authenticate_runtime(
            std::str::from_utf8(key.secret(ctx.snapshot()).unwrap()).unwrap(),
            ctx,
        )
        .unwrap()
}
async fn callback(callback: url::Url, malformed: bool) {
    let address = callback_request(callback, malformed).await;
    assert!(TcpStream::connect(address).await.is_err());
}
async fn callback_request(callback: url::Url, malformed: bool) -> std::net::SocketAddr {
    let address = callback.socket_addrs(|| None).unwrap()[0];
    if malformed {
        let mut peer = TcpStream::connect(address).await.unwrap();
        peer.write_all(b"POST /callback HTTP/1.1\r\nHost: invalid\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
        let mut response = vec![];
        peer.read_to_end(&mut response).await.unwrap();
        assert!(response.starts_with(b"HTTP/1.1 400"));
    }
    let mut peer = TcpStream::connect(address).await.unwrap();
    let head = format!(
        "GET {}?{} HTTP/1.1\r\nHost: {}\r\n\r\n",
        callback.path(),
        callback.query().unwrap(),
        address
    );
    peer.write_all(head.as_bytes()).await.unwrap();
    let mut response = vec![];
    peer.read_to_end(&mut response).await.unwrap();
    assert!(response.starts_with(b"HTTP/1.1 204"));
    address
}

#[tokio::test]
async fn native_login_callback_and_refresh_publish_actual_signed_profile() {
    let f = Fixture::new(true);
    let b = Broker::new().await;
    seed(&f, &b);
    let verifier = b.verifier(&f);
    let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let caller = caller(&f, &ctx);
    let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
    let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(f.auth.issuer());
    let input = login_args();
    let mut peer = None;
    applied(
        login(
            &input,
            &manager,
            ProfileManagerAuthority::Admin(&parent),
            &oauth,
            config(),
            |url| {
                peer = Some(tokio::spawn(callback(b.callback(url), true)));
                Ok(())
            },
            now,
        )
        .await
        .unwrap(),
    );
    peer.unwrap().await.unwrap();
    let metadata = f.read(&["account", "show"]);
    assert_eq!(metadata["profile"]["username"], "signed-native-user");
    let token_revision = metadata["profile"]["token_revision"].as_u64().unwrap();
    assert!(token_revision > 0);
    assert!(!metadata.to_string().contains("refresh-token"));
    applied(
        refresh(
            &RefreshArguments {
                expected_revision: None,
            },
            &manager,
            ProfileManagerAuthority::Admin(&parent),
            &oauth,
            now().unwrap(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        f.read(&["account", "show"])["profile"]["token_revision"],
        token_revision + 1
    );
    assert_eq!(b.state.rotation.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn installation_only_native_login_uses_selected_broker_and_cleans_runtime() {
    use graphit_core::{brand::Brand, config::ConfigSnapshot, project_paths::InstallationPaths};
    use graphit_hub::policy::HubResourcePolicy;

    let b = Broker::new().await;
    let runtime = tokio::runtime::Handle::current();
    let (b, peer) = tokio::task::spawn_blocking(move || {
        let directory = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            directory.path(),
            None,
            Some(&directory.path().join("installation")),
        )
        .unwrap();
        let config = ConfigSnapshot::new(
            paths.brand(),
            [HubResourcePolicy::distribution_defaults(paths.brand()).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let run = |args: &[&str], announce: &mut dyn FnMut(&str) -> io::Result<()>| {
            assert!(!paths.root().join("daemon/mcp.json").exists());
            let cancel = Cancellation::default();
            let original = policy.request_context(Instant::now(), &cancel).unwrap();
            let mut output = Vec::new();
            let mut diagnostic = Vec::new();
            let code = crate::auth_entry::run(
                super::super::tests::command(args),
                &paths,
                &policy,
                &original,
                &mut output,
                &mut diagnostic,
                announce,
            )
            .unwrap();
            let diagnostic = String::from_utf8(diagnostic).unwrap();
            assert_eq!(code, std::process::ExitCode::SUCCESS, "{diagnostic}");
            assert!(diagnostic.is_empty(), "{diagnostic}");
            assert!(!paths.root().join("daemon/mcp.key").exists());
            assert!(!paths.root().join("daemon/mcp.json").exists());
            String::from_utf8(output).unwrap()
        };
        let mut unexpected =
            |_: &str| panic!("unexpected authorization for a Local/read operation");
        for args in [
            vec!["provider", "add", "local", "--type", "local"],
            vec![
                "login",
                "--provider",
                "local",
                "--profile",
                "local",
                "--username",
                "local-operator",
                "--mcp-key",
                "standalone-local-fixture-key",
            ],
            vec![
                "provider",
                "add",
                "broker",
                "--type",
                "broker",
                "--broker-endpoint",
                b.origin(),
                "--embedding-mode",
                "broker",
                "--rerank-mode",
                "broker",
                "--s3-credential-source",
                "broker",
            ],
        ] {
            assert_eq!(run(&args, &mut unexpected), "Applied\n");
        }
        let local: Value =
            serde_json::from_str(&run(&["account", "show"], &mut unexpected)).unwrap();
        assert_eq!(local["profile"]["name"], "local");
        let provider: Value =
            serde_json::from_str(&run(&["provider", "show", "broker"], &mut unexpected)).unwrap();
        let mut peer = None;
        assert_eq!(
            run(
                &[
                    "login",
                    "--provider",
                    "broker",
                    "--profile",
                    "remote",
                    "--s3-disabled"
                ],
                &mut |url| {
                    assert!(peer.is_none());
                    peer = Some(runtime.spawn(callback_request(b.callback(url), false)));
                    Ok(())
                },
            ),
            "Applied\n"
        );
        let account: Value =
            serde_json::from_str(&run(&["account", "show"], &mut unexpected)).unwrap();
        let profile = &account["profile"];
        assert_eq!(profile["name"], "remote");
        assert_eq!(profile["provider"], "broker");
        assert_eq!(profile["active"], true);
        assert_eq!(profile["username"], "signed-native-user");
        assert_eq!(profile["provider_revision"], provider["provider_revision"]);
        assert!(profile["revision"].as_u64().unwrap() > 0);
        assert!(profile["token_revision"].as_u64().unwrap() > 0);
        assert_eq!(
            profile["broker_identity"]["issuer"],
            url::Url::parse(b.origin()).unwrap().as_str()
        );
        assert_eq!(profile["broker_identity"]["subject"], "native-subject");
        assert_eq!(profile["s3_disabled"], true);
        let accounts: Value =
            serde_json::from_str(&run(&["account", "list"], &mut unexpected)).unwrap();
        assert_eq!(accounts["profiles"].as_array().unwrap().len(), 2);
        assert!(!account.to_string().contains("access_token"));
        assert!(!account.to_string().contains("refresh-token"));
        (b, peer.unwrap())
    })
    .await
    .unwrap();
    // The standalone entry and its cleanup have returned before checking the listener.
    // A callback EOF alone precedes the SDK dropping the listener on the other runtime.
    let address = peer.await.unwrap();
    assert!(matches!(
        TcpStream::connect(address).await,
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused
    ));
    assert_eq!(b.state.rotation.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn native_login_reauthenticates_same_profile_after_provider_revision() {
    let f = Fixture::new(true);
    let b = Broker::new().await;
    seed(&f, &b);
    f.publish(&["provider", "add", "local", "--type", "local"], 2);
    f.publish(
        &[
            "login",
            "--provider",
            "local",
            "--profile",
            "local",
            "--username",
            "local-operator",
            "--mcp-key",
            "local-reauth-fixture-key",
        ],
        3,
    );
    let input = login_args();
    let mut prior_token_revision = None;
    for round in 0..2 {
        if round == 1 {
            let prior_provider_revision =
                f.read(&["provider", "show", "broker"])["provider_revision"]
                    .as_u64()
                    .unwrap();
            f.publish(
                &["provider", "update", "broker", "--rerank-mode", "disabled"],
                now().unwrap(),
            );
            let stale = f.read(&["account", "show", "remote"]);
            assert_eq!(
                stale["profile"]["provider_revision"],
                prior_provider_revision
            );
            assert_ne!(
                stale["profile"]["provider_revision"],
                f.read(&["provider", "show", "broker"])["provider_revision"]
            );
            // Select the existing Local account without replacing the stale Broker profile.
            f.publish(&["account", "use", "local"], now().unwrap());
        }
        let provider_revision = f.read(&["provider", "show", "broker"])["provider_revision"]
            .as_u64()
            .unwrap();
        let verifier = b.verifier(&f);
        let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let caller = caller(&f, &ctx);
        let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
        let manager = NativeProfileManager::new(f.auth.issuer());
        let mut peer = None;
        applied(
            login(
                &input,
                &manager,
                ProfileManagerAuthority::Admin(&parent),
                &oauth,
                config(),
                |url| {
                    peer = Some(tokio::spawn(callback(b.callback(url), false)));
                    Ok(())
                },
                now,
            )
            .await
            .unwrap(),
        );
        peer.unwrap().await.unwrap();
        let account = f.read(&["account", "show", "remote"]);
        assert_eq!(account["profile"]["name"], "remote");
        assert_eq!(account["profile"]["active"], true);
        assert_eq!(account["profile"]["username"], "signed-native-user");
        assert_eq!(account["profile"]["provider_revision"], provider_revision);
        let current_token_revision = account["profile"]["token_revision"].as_u64().unwrap();
        if let Some(prior) = prior_token_revision {
            assert_eq!(current_token_revision, prior + 1);
        }
        prior_token_revision = Some(current_token_revision);
        assert_eq!(
            f.read(&["account", "list"])["profiles"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    assert_eq!(b.state.rotation.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn native_login_before_authorization_reports_finite_oauth_category() {
    let f = Fixture::new(true);
    let b = Broker::new().await;
    seed(&f, &b);
    *b.state.invalid.lock().unwrap() = Some("metadata");
    let before = f.bytes();
    let verifier = b.verifier(&f);
    let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let caller = caller(&f, &ctx);
    let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
    let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(f.auth.issuer());
    let input = login_args();
    let mut announced = false;
    let error = login(
        &input,
        &manager,
        ProfileManagerAuthority::Admin(&parent),
        &oauth,
        config(),
        |_| {
            announced = true;
            Ok(())
        },
        now,
    )
    .await
    .err()
    .unwrap();
    assert!(!announced);
    assert_eq!(
        error.to_string(),
        "native Broker login failed: Broker(InvalidMetadata)"
    );
    assert_eq!(error.remote_effect(), Some(RemoteEffect::NotSent));
    assert_eq!(f.bytes(), before);
}

#[tokio::test]
async fn invalid_native_proofs_do_not_publish_credentials() {
    for invalid in ["state", "nonce", "aud"] {
        let f = Fixture::new(true);
        let b = Broker::new().await;
        seed(&f, &b);
        let prior = f.bytes();
        *b.state.invalid.lock().unwrap() = Some(invalid);
        let verifier = b.verifier(&f);
        let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let caller = caller(&f, &ctx);
        let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
        let manager = NativeProfileManager::new(f.auth.issuer());
        let input = login_args();
        let mut peer = None;
        let outcome = login(
            &input,
            &manager,
            ProfileManagerAuthority::Admin(&parent),
            &oauth,
            config(),
            |url| {
                let mut cb = b.callback(url);
                if invalid == "state" {
                    cb.set_query(Some("code=code-canary&state=wrong"));
                }
                peer = Some(tokio::spawn(callback(cb, false)));
                Ok(())
            },
            now,
        )
        .await;
        peer.unwrap().await.unwrap();
        let error = outcome.err().unwrap();
        assert!(matches!(error, Error::Login(_)));
        assert_eq!(f.bytes(), prior);
        if invalid == "state" {
            assert_eq!(error.remote_effect(), Some(RemoteEffect::NotSent));
        }
    }
}

#[tokio::test]
async fn announce_failure_and_original_cancel_close_owned_callback() {
    for cancelled in [false, true] {
        let f = Fixture::new(true);
        let b = Broker::new().await;
        seed(&f, &b);
        let prior = f.bytes();
        let verifier = b.verifier(&f);
        let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let caller = caller(&f, &ctx);
        let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
        let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
        let manager = NativeProfileManager::new(f.auth.issuer());
        let input = login_args();
        let mut address = None;
        let outcome = login(
            &input,
            &manager,
            ProfileManagerAuthority::Admin(&parent),
            &oauth,
            config(),
            |url| {
                address = Some(b.callback(url).socket_addrs(|| None).unwrap()[0]);
                if cancelled {
                    cancel.cancel();
                    Ok(())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "opener unavailable",
                    ))
                }
            },
            now,
        )
        .await;
        if cancelled {
            assert!(matches!(
                outcome,
                Err(Error::Interactive(InteractiveLoginError::Transport(
                    InteractiveTransportError::Cancelled
                )))
            ));
        } else {
            assert!(matches!(outcome, Err(Error::Announce(_))));
        }
        assert!(TcpStream::connect(address.unwrap()).await.is_err());
        assert_eq!(f.bytes(), prior);
    }
}

#[tokio::test]
async fn dropping_login_future_closes_owned_listener() {
    let f = Fixture::new(true);
    let b = Broker::new().await;
    seed(&f, &b);
    let prior = f.bytes();
    let verifier = b.verifier(&f);
    let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let caller = caller(&f, &ctx);
    let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
    let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(f.auth.issuer());
    let input = login_args();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut pending = Box::pin(login(
        &input,
        &manager,
        ProfileManagerAuthority::Admin(&parent),
        &oauth,
        config(),
        |url| {
            tx.send(b.callback(url).socket_addrs(|| None).unwrap()[0])
                .unwrap();
            Ok(())
        },
        now,
    ));
    let address = tokio::select! {
        result = &mut pending => panic!("login completed before callback: {}", result.is_ok()),
        address = rx => address.unwrap(),
    };
    drop(pending);
    assert!(TcpStream::connect(address).await.is_err());
    assert_eq!(f.bytes(), prior);
}

#[tokio::test]
async fn provider_change_during_exchange_preserves_newer_document() {
    let f = Fixture::new(true);
    let b = Broker::new().await;
    seed(&f, &b);
    b.state.pause.store(true, Ordering::SeqCst);
    let verifier = b.verifier(&f);
    let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let caller = caller(&f, &ctx);
    let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
    let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(f.auth.issuer());
    let input = login_args();
    let mut peer = None;
    let pending = login(
        &input,
        &manager,
        ProfileManagerAuthority::Admin(&parent),
        &oauth,
        config(),
        |url| {
            peer = Some(tokio::spawn(callback(b.callback(url), false)));
            Ok(())
        },
        now,
    );
    let update = async {
        b.state.started.notified().await;
        f.publish(
            &[
                "provider",
                "update",
                "broker",
                "--embedding-mode",
                "disabled",
            ],
            now().unwrap(),
        );
        let document = f.bytes();
        b.state.release.notify_one();
        document
    };
    let (outcome, updated) = tokio::join!(pending, update);
    peer.unwrap().await.unwrap();
    let error = outcome.err().unwrap();
    assert_eq!(error.remote_effect(), Some(RemoteEffect::MayHaveApplied));
    assert_eq!(f.bytes(), updated);
    assert_eq!(
        f.read(&["provider", "show", "broker"])["config"]["embedding"]["mode"],
        "disabled"
    );
    assert!(
        f.read(&["account", "list"])["profiles"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn logout_during_refresh_cannot_restore_deleted_credentials() {
    let f = Fixture::new(true);
    let b = Broker::new().await;
    seed(&f, &b);
    let verifier = b.verifier(&f);
    let oauth = NativeOAuth::new(&verifier, oauth_limits()).unwrap();
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let caller = caller(&f, &ctx);
    let session = f.auth.issuer().bootstrap_admin(&caller).unwrap();
    let parent = f.auth.issuer().admin_parent(&caller, &session).unwrap();
    let manager = NativeProfileManager::new(f.auth.issuer());
    let input = login_args();
    let mut peer = None;
    applied(
        login(
            &input,
            &manager,
            ProfileManagerAuthority::Admin(&parent),
            &oauth,
            config(),
            |url| {
                peer = Some(tokio::spawn(callback(b.callback(url), false)));
                Ok(())
            },
            now,
        )
        .await
        .unwrap(),
    );
    peer.unwrap().await.unwrap();
    b.state.started.notified().await;
    b.state.pause.store(true, Ordering::SeqCst);
    let refresh_input = RefreshArguments {
        expected_revision: None,
    };
    let pending = refresh(
        &refresh_input,
        &manager,
        ProfileManagerAuthority::Admin(&parent),
        &oauth,
        now().unwrap(),
    );
    let logout = async {
        b.state.started.notified().await;
        f.publish(&["logout"], now().unwrap());
        let document = f.bytes();
        b.state.release.notify_one();
        document
    };
    let (outcome, logged_out) = tokio::join!(pending, logout);
    assert_eq!(
        outcome.err().unwrap().remote_effect(),
        Some(RemoteEffect::MayHaveApplied)
    );
    assert_eq!(f.bytes(), logged_out);
    assert!(f.read(&["account", "show"])["profile"].is_null());
}
