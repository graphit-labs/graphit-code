use super::*;
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use graphit_core::file_lock::Cancellation;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};
mod native;

struct State {
    broker: Mutex<Value>,
    standard: Mutex<Value>,
    keys: Mutex<Value>,
    userinfo: Mutex<Value>,
    jwks_status: Mutex<u16>,
    userinfo_status: Mutex<u16>,
    jwks_delay: Mutex<Duration>,
    userinfo_delay: Mutex<Duration>,
    counts: Mutex<BTreeMap<String, usize>>,
    last_bearer: Mutex<String>,
    sockets: AtomicUsize,
}
struct Fixture {
    origin: String,
    resource: String,
    state: Arc<State>,
    task: JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new(pair: &Ed25519KeyPair, kid: &str) -> Self {
        Self::with_resource(pair, kid, "https://mcp.example/mcp").await
    }
    async fn with_resource(pair: &Ed25519KeyPair, kid: &str, resource: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(State {
            broker: Mutex::new(
                json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,"client_id":"graphit-cli","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["broker-api"],"access_token_audience":"broker-api","mcp_resources":[resource]}}),
            ),
            standard: Mutex::new(
                json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}),
            ),
            keys: Mutex::new(key_set(pair, kid)),
            userinfo: Mutex::new(json!({"sub":"stable-subject"})),
            jwks_status: Mutex::new(200),
            userinfo_status: Mutex::new(200),
            jwks_delay: Mutex::new(Duration::ZERO),
            userinfo_delay: Mutex::new(Duration::ZERO),
            counts: Mutex::new(BTreeMap::new()),
            last_bearer: Mutex::new(String::new()),
            sockets: AtomicUsize::new(0),
        });
        let server = state.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                server.sockets.fetch_add(1, Ordering::SeqCst);
                let state = server.clone();
                tokio::spawn(async move {
                    let mut input = Vec::new();
                    let mut block = [0; 1024];
                    while !input.windows(4).any(|v| v == b"\r\n\r\n") {
                        let Ok(read) = stream.read(&mut block).await else {
                            return;
                        };
                        if read == 0 || input.len() + read > 16384 {
                            return;
                        }
                        input.extend_from_slice(&block[..read]);
                    }
                    let request = std::str::from_utf8(&input).unwrap();
                    let path = request
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap();
                    *state.counts.lock().unwrap().entry(path.into()).or_default() += 1;
                    if path == "/userinfo" {
                        *state.last_bearer.lock().unwrap() = request
                            .lines()
                            .find_map(|line| {
                                line.split_once(':')
                                    .filter(|(key, _)| key.eq_ignore_ascii_case("authorization"))
                                    .map(|(_, value)| value.trim().into())
                            })
                            .unwrap_or_default();
                    }
                    let (status, body, delay) = match path {
                        "/.well-known/graphit-broker" => (
                            200,
                            state.broker.lock().unwrap().to_string(),
                            Duration::ZERO,
                        ),
                        "/.well-known/openid-configuration" => (
                            200,
                            state.standard.lock().unwrap().to_string(),
                            Duration::ZERO,
                        ),
                        "/keys" => (
                            *state.jwks_status.lock().unwrap(),
                            state.keys.lock().unwrap().to_string(),
                            *state.jwks_delay.lock().unwrap(),
                        ),
                        "/userinfo" => (
                            *state.userinfo_status.lock().unwrap(),
                            state.userinfo.lock().unwrap().to_string(),
                            *state.userinfo_delay.lock().unwrap(),
                        ),
                        _ => (404, "{}".into(), Duration::ZERO),
                    };
                    tokio::time::sleep(delay).await;
                    let response = format!(
                        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        Self {
            origin,
            resource: resource.into(),
            state,
            task,
        }
    }
    fn verifier(&self, limits: VerifierLimits) -> BrokerVerifier {
        self.verifier_resource(Some(&self.resource), limits)
    }
    fn verifier_resource(&self, resource: Option<&str>, limits: VerifierLimits) -> BrokerVerifier {
        BrokerVerifier::new(
            BrokerPolicy {
                endpoint: &self.origin,
                mcp_resource: resource,
                provider_revision: 7,
                network: ClientPolicy {
                    origin: &self.origin,
                    allow_loopback_http: true,
                    proxy: None,
                    additional_roots: &[],
                },
            },
            network_limits(),
            limits,
        )
        .unwrap()
    }
    fn count(&self, path: &str) -> usize {
        self.state
            .counts
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or(0)
    }
    async fn wait_for(&self, path: &str, count: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.count(path) < count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    fn claims(&self) -> Value {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        json!({"iss":self.origin,"aud":["broker-api",self.resource],"sub":"stable-subject","client_id":"registered-other-client","jti":"token-id","token_use":"access","iat":now,"exp":now+3600,"preferred_username":"fixture-user","organization":"fixture-org","groups":["fixture-team"]})
    }
}
pub(crate) fn pair() -> Ed25519KeyPair {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
}
pub(super) fn key_set(pair: &Ed25519KeyPair, kid: &str) -> Value {
    json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(pair.public_key().as_ref()),"kid":kid,"alg":"EdDSA","use":"sig","key_ops":["verify"]}]})
}
pub(crate) fn token(pair: &Ed25519KeyPair, kid: &str, claims: &Value) -> String {
    token_header(
        pair,
        &json!({"alg":"EdDSA","kid":kid,"typ":"JWT"}),
        &claims.to_string(),
    )
}
fn token_header(pair: &Ed25519KeyPair, header: &Value, claims: &str) -> String {
    let message = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims)
    );
    format!(
        "{message}.{}",
        URL_SAFE_NO_PAD.encode(pair.sign(message.as_bytes()).as_ref())
    )
}
pub(crate) fn limits() -> VerifierLimits {
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
    }
}
pub(crate) fn network_limits() -> NetworkLimits {
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
pub(super) fn context(cancellation: &Cancellation) -> SnapshotContext {
    SnapshotContext::new(
        Instant::now() + Duration::from_secs(3),
        cancellation.clone(),
    )
}

#[tokio::test]
async fn real_signature_admits_each_original_locally_without_userinfo() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let verifier = fixture.verifier(limits());
    let cancel = Cancellation::default();
    let raw = token(&key, "one", &fixture.claims());
    for status in [200, 401, 403, 429, 500] {
        *fixture.state.userinfo_status.lock().unwrap() = status;
        let original = context(&cancel);
        let caller = verifier
            .verify(&raw, BrokerRequestContext::new(&original))
            .await
            .unwrap();
        caller
            .check(&verifier, BrokerRequestContext::new(&original))
            .unwrap();
        assert_eq!(caller.subject(), "stable-subject");
        assert_eq!(caller.username(), Some("fixture-user"));
        assert_eq!(caller.organization(), Some("fixture-org"));
        assert_eq!(caller.teams(), ["fixture-team"]);
        assert_eq!(caller.issuer(), fixture.origin);
        assert_eq!(caller.provider_revision(), 7);
        assert_eq!(caller.bearer_for_broker(), raw);
    }
    assert_eq!(fixture.count("/keys"), 1);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn claims_and_header_matrix_never_reach_userinfo() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let verifier = fixture.verifier(limits());
    let cancel = Cancellation::default();
    verifier
        .metadata(BrokerRequestContext::new(&context(&cancel)))
        .await
        .unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for (field, value) in [
        ("iss", json!("https://foreign.example")),
        ("aud", json!("broker-api")),
        ("aud", json!([fixture.resource, fixture.resource])),
        ("token_use", json!("id")),
        ("sub", json!("")),
        ("client_id", json!("")),
        ("jti", json!("")),
        ("iat", json!(now + 7200)),
        ("exp", json!(now)),
        ("exp", json!(-1)),
        ("nbf", json!(now + 7200)),
        ("nbf", json!(null)),
        ("preferred_username", json!(1)),
        ("groups", json!([1])),
    ] {
        let mut claims = fixture.claims();
        claims[field] = value;
        let raw = token(&key, "one", &claims);
        assert!(
            verifier
                .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_err(),
            "field {field}"
        );
    }
    for field in [
        "iss",
        "aud",
        "sub",
        "client_id",
        "jti",
        "iat",
        "exp",
        "token_use",
    ] {
        let mut claims = fixture.claims();
        claims.as_object_mut().unwrap().remove(field);
        assert!(
            verifier
                .verify(
                    &token(&key, "one", &claims),
                    BrokerRequestContext::new(&context(&cancel))
                )
                .await
                .is_err(),
            "missing {field}"
        );
    }
    for header in [
        json!({"alg":"none","kid":"one"}),
        json!({"alg":"HS256","kid":"one"}),
        json!({"alg":"EdDSA"}),
        json!({"alg":"EdDSA","kid":"one","jku":"https://sink.example/key"}),
        json!({"alg":"EdDSA","kid":"one","crit":[]}),
        json!({"alg":"EdDSA","kid":"one","b64":true}),
    ] {
        let raw = token_header(&key, &header, &fixture.claims().to_string());
        assert!(
            verifier
                .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_err()
        );
    }
    let foreign = pair();
    assert!(
        verifier
            .verify(
                &token(&foreign, "one", &fixture.claims()),
                BrokerRequestContext::new(&context(&cancel))
            )
            .await
            .is_err()
    );
    let valid = token(&key, "one", &fixture.claims());
    let mut bad = valid.into_bytes();
    let last = bad.len() - 2;
    bad[last] = if bad[last] == b'A' { b'B' } else { b'A' };
    assert!(
        verifier
            .verify(
                std::str::from_utf8(&bad).unwrap(),
                BrokerRequestContext::new(&context(&cancel))
            )
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn metadata_and_public_key_failures_cannot_birth_identity() {
    let key = pair();
    let cancel = Cancellation::default();
    for field in ["authorization_endpoint", "token_endpoint", "jwks_uri"] {
        let fixture = Fixture::new(&key, "one").await;
        fixture.state.standard.lock().unwrap()[field] = json!("https://foreign.example/endpoint");
        assert_eq!(
            fixture
                .verifier(limits())
                .metadata(BrokerRequestContext::new(&context(&cancel)))
                .await
                .err(),
            Some(BrokerError::InvalidMetadata)
        );
        assert_eq!(fixture.count("/keys"), 0);
        assert_eq!(fixture.count("/userinfo"), 0);
    }
    for (field, value) in [
        ("issuer", json!("https://foreign.example")),
        ("grant_types_supported", json!(["authorization_code"])),
        ("code_challenge_methods_supported", json!(["plain"])),
        (
            "token_endpoint_auth_methods_supported",
            json!(["client_secret_basic"]),
        ),
        ("id_token_signing_alg_values_supported", json!(["RS256"])),
    ] {
        let fixture = Fixture::new(&key, "one").await;
        fixture.state.standard.lock().unwrap()[field] = value;
        assert!(
            fixture
                .verifier(limits())
                .metadata(BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_err()
        );
        assert_eq!(fixture.count("/keys"), 0);
    }
    let fixture = Fixture::new(&key, "one").await;
    fixture.state.broker.lock().unwrap()["authentication"]["mcp_resources"] =
        json!(["https://other.example/mcp"]);
    assert!(
        fixture
            .verifier(limits())
            .metadata(BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/.well-known/openid-configuration"), 0);
    for bad in [
        json!({"kid":"one","d":"private"}),
        json!({"kid":"one","kty":"OKP","crv":"Ed25519","x":"short"}),
        json!({"kid":"one","kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref()),"use":"enc"}),
    ] {
        let fixture = Fixture::new(&key, "one").await;
        *fixture.state.keys.lock().unwrap() = json!({"keys":[bad]});
        assert!(
            fixture
                .verifier(limits())
                .metadata(BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_err()
        );
        assert_eq!(fixture.count("/userinfo"), 0);
    }
    let fixture = Fixture::new(&key, "one").await;
    let first = fixture.state.keys.lock().unwrap()["keys"][0].clone();
    *fixture.state.keys.lock().unwrap() = json!({"keys":[first,first]});
    assert!(
        fixture
            .verifier(limits())
            .metadata(BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn captured_resource_literal_and_token_size_boundaries() {
    let key = pair();
    let fixture = Fixture::with_resource(&key, "one", "https://Graphit.Example/mcp?x=%2f").await;
    let cancel = Cancellation::default();
    let raw = token(&key, "one", &fixture.claims());
    let mut policy = limits();
    policy.token_bytes = raw.len();
    assert!(
        fixture
            .verifier(policy)
            .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_ok()
    );
    policy.token_bytes -= 1;
    let before = fixture.state.sockets.load(Ordering::SeqCst);
    assert_eq!(
        fixture
            .verifier(policy)
            .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
            .await
            .err(),
        Some(BrokerError::Limit)
    );
    assert_eq!(fixture.state.sockets.load(Ordering::SeqCst), before);
    let mut claims = fixture.claims();
    claims["aud"] = json!(Url::parse(&fixture.resource).unwrap().as_str());
    assert!(
        fixture
            .verifier(limits())
            .verify(
                &token(&key, "one", &claims),
                BrokerRequestContext::new(&context(&cancel))
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn bounded_serde_rejects_duplicates_and_exact_one_under_limits() {
    let cancel = Cancellation::default();
    let input = br#"{"a":["xx",1]}"#;
    let exact = JsonLimits {
        bytes: input.len(),
        nodes: 5,
        depth: 3,
        scalar_bytes: 2,
        collection_entries: 2,
    };
    assert!(json::parse(input, exact, BrokerRequestContext::new(&context(&cancel))).is_ok());
    let mut cases = [exact; 5];
    cases[0].bytes -= 1;
    cases[1].nodes -= 1;
    cases[2].depth -= 1;
    cases[3].scalar_bytes -= 1;
    cases[4].collection_entries -= 1;
    for policy in cases {
        assert_eq!(
            json::parse(input, policy, BrokerRequestContext::new(&context(&cancel))).err(),
            Some(BrokerError::Limit)
        );
    }
    for input in [
        br#"{"a":1,"a":2}"#.as_slice(),
        br#"{"a":{"x":1,"\u0078":2}}"#.as_slice(),
        b"{}{}",
        b"[1,]",
    ] {
        assert_eq!(
            json::parse(
                input,
                limits().json,
                BrokerRequestContext::new(&context(&cancel))
            )
            .err(),
            Some(BrokerError::InvalidDocument)
        );
    }
    assert!(
        json::parse(
            br#"{"escaped":"\u00e9"}"#,
            limits().json,
            BrokerRequestContext::new(&context(&cancel))
        )
        .is_ok()
    );
}

#[tokio::test]
async fn cache_ttls_do_not_cache_authorized_users() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let cancel = Cancellation::default();
    let mut policy = limits();
    policy.metadata_ttl = Duration::from_secs(1);
    policy.key_ttl = Duration::from_millis(20);
    let verifier = fixture.verifier(policy);
    let raw = token(&key, "one", &fixture.claims());
    assert!(
        verifier
            .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_ok()
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(
        verifier
            .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_ok()
    );
    assert_eq!(fixture.count("/keys"), 2);
    assert_eq!(fixture.count("/.well-known/graphit-broker"), 1);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        verifier
            .metadata(BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_ok()
    );
    assert_eq!(fixture.count("/.well-known/graphit-broker"), 2);
    assert_eq!(fixture.count("/keys"), 3);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn rotation_reloads_once_for_new_and_reused_kid() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let verifier = fixture.verifier(limits());
    let cancel = Cancellation::default();
    assert!(
        verifier
            .verify(
                &token(&key, "one", &fixture.claims()),
                BrokerRequestContext::new(&context(&cancel))
            )
            .await
            .is_ok()
    );
    let new = pair();
    *fixture.state.keys.lock().unwrap() = key_set(&new, "two");
    let raw = token(&new, "two", &fixture.claims());
    assert!(
        verifier
            .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_ok()
    );
    assert_eq!(fixture.count("/keys"), 2);
    for _ in 0..20 {
        assert!(
            verifier
                .verify(
                    &token(&new, "unknown", &fixture.claims()),
                    BrokerRequestContext::new(&context(&cancel))
                )
                .await
                .is_err()
        );
    }
    assert_eq!(fixture.count("/keys"), 2);
    tokio::time::sleep(Duration::from_millis(110)).await;
    let replacement = pair();
    *fixture.state.keys.lock().unwrap() = key_set(&replacement, "two");
    assert!(
        verifier
            .verify(
                &token(&replacement, "two", &fixture.claims()),
                BrokerRequestContext::new(&context(&cancel))
            )
            .await
            .is_ok()
    );
    assert_eq!(fixture.count("/keys"), 3);
    assert_eq!(fixture.count("/.well-known/graphit-broker"), 1);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn failed_or_cancelled_forced_refresh_consumes_shared_cooldown() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let cancel = Cancellation::default();
    let verifier = Arc::new(fixture.verifier(limits()));
    let claims = fixture.claims();
    assert!(
        verifier
            .verify(
                &token(&key, "one", &claims),
                BrokerRequestContext::new(&context(&cancel))
            )
            .await
            .is_ok()
    );
    *fixture.state.jwks_status.lock().unwrap() = 500;
    let unknown = token(&key, "unknown", &claims);
    assert!(
        verifier
            .verify(&unknown, BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_err()
    );
    for _ in 0..20 {
        assert!(
            verifier
                .verify(&unknown, BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_err()
        );
    }
    assert_eq!(fixture.count("/keys"), 2);
    assert_eq!(fixture.count("/userinfo"), 0);
    tokio::time::sleep(Duration::from_millis(110)).await;
    *fixture.state.jwks_status.lock().unwrap() = 200;
    *fixture.state.jwks_delay.lock().unwrap() = Duration::from_secs(2);
    let cloned = verifier.clone();
    let request_cancel = Cancellation::default();
    let child = request_cancel.clone();
    let raw = unknown.clone();
    let pending = tokio::spawn(async move {
        cloned
            .verify(&raw, BrokerRequestContext::new(&context(&child)))
            .await
            .map(|_| ())
    });
    fixture.wait_for("/keys", 3).await;
    request_cancel.cancel();
    assert_eq!(pending.await.unwrap().err(), Some(BrokerError::Cancelled));
    for _ in 0..10 {
        assert!(
            verifier
                .verify(&unknown, BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_err()
        );
    }
    assert_eq!(fixture.count("/keys"), 3);
    tokio::time::sleep(Duration::from_millis(110)).await;
    let cloned = verifier.clone();
    let raw = unknown.clone();
    let pending = tokio::spawn(async move {
        let cancel = Cancellation::default();
        cloned
            .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
            .await
            .map(|_| ())
    });
    fixture.wait_for("/keys", 4).await;
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert!(
        verifier
            .verify(&unknown, BrokerRequestContext::new(&context(&cancel)))
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/keys"), 4);
}

#[tokio::test]
async fn cache_waiting_preserves_original_cancellation_and_deadline() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let verifier = fixture.verifier(limits());
    let held = verifier.fetch_gate.lock().await;
    let cancel = Cancellation::default();
    let original = SnapshotContext::service(cancel.clone());
    let mut pending = Box::pin(verifier.metadata(BrokerRequestContext::new(&original)));
    let waker = std::task::Waker::noop();
    let mut poll_context = std::task::Context::from_waker(waker);
    assert!(std::future::Future::poll(pending.as_mut(), &mut poll_context).is_pending());
    cancel.cancel();
    assert_eq!(pending.await.err(), Some(BrokerError::Cancelled));
    let deadline = SnapshotContext::new(
        Instant::now() + Duration::from_millis(20),
        Cancellation::default(),
    );
    assert_eq!(
        verifier
            .metadata(BrokerRequestContext::new(&deadline))
            .await
            .err(),
        Some(BrokerError::Deadline)
    );
    assert_eq!(fixture.state.sockets.load(Ordering::SeqCst), 0);
    drop(held);
    verifier
        .metadata(BrokerRequestContext::new(
            &context(&Cancellation::default()),
        ))
        .await
        .unwrap();
    assert_eq!(fixture.count("/keys"), 1);
}

struct TestClock(Mutex<Moment>);

#[tokio::test]
async fn expired_cache_failures_and_cancellation_never_fall_back_to_old_keys() {
    let key = pair();
    for expire_metadata in [false, true] {
        let fixture = Fixture::new(&key, "one").await;
        let mut policy = limits();
        if expire_metadata {
            policy.metadata_ttl = Duration::from_millis(20);
        } else {
            policy.key_ttl = Duration::from_millis(20);
        }
        let verifier = Arc::new(fixture.verifier(policy));
        let cancel = Cancellation::default();
        let raw = token(&key, "one", &fixture.claims());
        assert!(
            verifier
                .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_ok()
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
        *fixture.state.jwks_status.lock().unwrap() = 500;
        assert!(
            verifier
                .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_err()
        );
        assert_eq!(fixture.count("/userinfo"), 0);
        assert_eq!(fixture.count("/keys"), 2);
        assert!(verifier.cached_now().unwrap().is_none());
        *fixture.state.jwks_status.lock().unwrap() = 200;
        *fixture.state.jwks_delay.lock().unwrap() = Duration::from_secs(2);
        let cloned = verifier.clone();
        let request_cancel = Cancellation::default();
        let child = request_cancel.clone();
        let token = raw.clone();
        let pending = tokio::spawn(async move {
            cloned
                .verify(&token, BrokerRequestContext::new(&context(&child)))
                .await
                .map(|_| ())
        });
        fixture.wait_for("/keys", 3).await;
        request_cancel.cancel();
        assert_eq!(pending.await.unwrap().err(), Some(BrokerError::Cancelled));
        assert!(verifier.cached_now().unwrap().is_none());
        assert_eq!(fixture.count("/userinfo"), 0);
        *fixture.state.jwks_delay.lock().unwrap() = Duration::ZERO;
        assert!(
            verifier
                .verify(&raw, BrokerRequestContext::new(&context(&cancel)))
                .await
                .is_ok()
        );
        assert_eq!(fixture.count("/userinfo"), 0);
    }
}

#[tokio::test]
async fn public_key_count_has_its_own_exact_boundary() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let first = key_set(&key, "one")["keys"][0].clone();
    let second = key_set(&key, "two")["keys"][0].clone();
    *fixture.state.keys.lock().unwrap() = json!({"keys":[first, second]});
    let cancel = Cancellation::default();
    let mut policy = limits();
    policy.max_keys = 2;
    assert!(policy.json.collection_entries > policy.max_keys);
    assert!(
        fixture
            .verifier(policy)
            .verify(
                &token(&key, "one", &fixture.claims()),
                BrokerRequestContext::new(&context(&cancel))
            )
            .await
            .is_ok()
    );
    policy.max_keys = 1;
    let before = fixture.count("/userinfo");
    assert_eq!(
        fixture
            .verifier(policy)
            .metadata(BrokerRequestContext::new(&context(&cancel)))
            .await
            .err(),
        Some(BrokerError::Limit)
    );
    assert_eq!(fixture.count("/userinfo"), before);
}

impl Clock for TestClock {
    fn now(&self) -> Moment {
        *self.0.lock().unwrap()
    }
}
#[tokio::test]
async fn local_proof_expiry_and_utc_rollback_cannot_extend_identity() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    let mut verifier = fixture.verifier(limits());
    let start = Moment {
        utc: SystemTime::now(),
        mono: Instant::now(),
    };
    let clock = Arc::new(TestClock(Mutex::new(start)));
    verifier.clock = clock.clone();
    for (rollback, service) in [(true, false), (false, false), (true, true), (false, true)] {
        *clock.0.lock().unwrap() = start;
        let mut claims = fixture.claims();
        claims["iat"] = json!(start.utc.duration_since(UNIX_EPOCH).unwrap().as_secs());
        claims["exp"] = json!(start.utc.duration_since(UNIX_EPOCH).unwrap().as_secs() + 10);
        let raw = token(&key, "one", &claims);
        let original = if service {
            SnapshotContext::service(Cancellation::default())
        } else {
            context(&Cancellation::default())
        };
        let caller = verifier
            .verify(&raw, BrokerRequestContext::new(&original))
            .await
            .unwrap();
        *clock.0.lock().unwrap() = Moment {
            utc: if rollback {
                start.utc - Duration::from_secs(100)
            } else {
                start.utc + Duration::from_secs(20)
            },
            mono: start.mono + Duration::from_secs(20),
        };
        assert_eq!(
            caller.check(&verifier, BrokerRequestContext::new(&original)),
            Err(BrokerError::InvalidToken)
        );
    }
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[test]
fn invalid_policy_and_errors_are_static() {
    for bad in [
        VerifierLimits {
            token_bytes: 0,
            ..limits()
        },
        VerifierLimits {
            max_keys: 0,
            ..limits()
        },
        VerifierLimits {
            metadata_ttl: Duration::ZERO,
            ..limits()
        },
        VerifierLimits {
            key_ttl: Duration::MAX,
            ..limits()
        },
    ] {
        assert_eq!(bad.validate().err(), Some(BrokerError::InvalidPolicy));
    }
    for error in [
        BrokerError::InvalidPolicy,
        BrokerError::InvalidMetadata,
        BrokerError::InvalidDocument,
        BrokerError::InvalidKeys,
        BrokerError::InvalidToken,
        BrokerError::Limit,
        BrokerError::Busy,
        BrokerError::Cancelled,
        BrokerError::Deadline,
        BrokerError::Connection,
        BrokerError::Transport,
    ] {
        assert!(!format!("{error:?}: {error}").contains("fixture-secret-canary"));
    }
}

#[tokio::test]
async fn access_profile_is_optional_and_userinfo_is_not_required_discovery() {
    let key = pair();
    let fixture = Fixture::new(&key, "one").await;
    fixture
        .state
        .standard
        .lock()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("userinfo_endpoint");
    let verifier = fixture.verifier(limits());
    for username in [None, Some(json!("")), Some(json!(null))] {
        let mut claims = fixture.claims();
        match username {
            Some(value) => {
                claims["preferred_username"] = value;
            }
            None => {
                claims.as_object_mut().unwrap().remove("preferred_username");
            }
        }
        let raw = token(&key, "one", &claims);
        let original = context(&Cancellation::default());
        let caller = verifier
            .verify(&raw, BrokerRequestContext::new(&original))
            .await
            .unwrap();
        assert_eq!(caller.username(), None);
        assert_eq!(caller.subject(), "stable-subject");
    }
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn concurrent_key_rotation_reuses_fetched_keys_for_both_callers() {
    let old = pair();
    let new = pair();
    let fixture = Fixture::new(&old, "old").await;
    let verifier = fixture.verifier(limits());
    let cancel_a = Cancellation::default();
    let cancel_b = Cancellation::default();
    let original_a = context(&cancel_a);
    let original_b = context(&cancel_b);
    verifier
        .metadata(BrokerRequestContext::new(&original_a))
        .await
        .unwrap();
    *fixture.state.keys.lock().unwrap() = key_set(&new, "new");
    let raw = token(&new, "new", &fixture.claims());
    let (first, second) = tokio::join!(
        verifier.verify(&raw, BrokerRequestContext::new(&original_a)),
        verifier.verify(&raw, BrokerRequestContext::new(&original_b))
    );
    first
        .unwrap()
        .check(&verifier, BrokerRequestContext::new(&original_a))
        .unwrap();
    second
        .unwrap()
        .check(&verifier, BrokerRequestContext::new(&original_b))
        .unwrap();
    assert_eq!(fixture.count("/keys"), 2);
    assert_eq!(fixture.count("/userinfo"), 0);
}
