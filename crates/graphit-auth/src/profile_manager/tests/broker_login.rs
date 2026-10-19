#[path = "broker_login/device_login.rs"]
mod device_login;
#[path = "broker_login/interactive_login.rs"]
mod interactive_login;

use super::*;
use crate::broker::oauth::{NativeOAuth, OAuthLimits, RemoteEffect};
use crate::broker::{BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits};
use crate::profile::{
    AuthDocument, BrokerService, OidcTokens, ProfileCredentials, ProfileData, ProfileError,
    ProfileLimits, TokenExpectation, TokenUpdate,
};
use crate::profile_manager::{
    BrokerLoginError, BrokerLoginErrorKind, BrokerLoginSelection, ManagedBrokerLogin,
};
use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_http::client::{ClientPolicy, Limits as NetworkLimits};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Mutex, time::UNIX_EPOCH};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
    task::{JoinHandle, JoinSet},
};

struct HttpState {
    origin: String,
    keys: Value,
    resource: Mutex<Option<String>>,
    response: Mutex<String>,
    status: Mutex<u16>,
    disconnect_token: std::sync::atomic::AtomicBool,
    gates: Mutex<BTreeMap<String, Arc<Notify>>>,
    counts: Mutex<BTreeMap<String, usize>>,
    forms: Mutex<Vec<String>>,
    observed: Notify,
}
struct LoginFixture {
    state: Arc<HttpState>,
    task: Option<JoinHandle<()>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    key: Ed25519KeyPair,
}
impl Drop for LoginFixture {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
impl LoginFixture {
    async fn new() -> Self {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let keys = json!({"keys":[{"kty":"OKP","crv":"Ed25519","kid":"one","alg":"EdDSA","use":"sig","key_ops":["verify"],"x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref())}]});
        let state = Arc::new(HttpState {
            origin,
            keys,
            resource: Mutex::new(None),
            response: Mutex::new("{}".into()),
            status: Mutex::new(200),
            disconnect_token: std::sync::atomic::AtomicBool::new(false),
            gates: Mutex::new(BTreeMap::new()),
            counts: Mutex::new(BTreeMap::new()),
            forms: Mutex::new(Vec::new()),
            observed: Notify::new(),
        });
        let serve = Arc::clone(&state);
        let (stop, mut stopping) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut stopping => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break; };
                        if connections.len() >= 16 { break; }
                        let state = Arc::clone(&serve);
                        connections.spawn(serve_connection(stream, state));
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
            connections.abort_all();
            while connections.join_next().await.is_some() {}
        });
        Self {
            state,
            task: Some(task),
            stop: Some(stop),
            key,
        }
    }
    async fn shutdown(mut self) {
        self.stop.take().unwrap().send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap();
    }
    fn origin(&self) -> &str {
        &self.state.origin
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
    fn gate(&self, path: &str) -> Arc<Notify> {
        let gate = Arc::new(Notify::new());
        self.state
            .gates
            .lock()
            .unwrap()
            .insert(path.into(), gate.clone());
        gate
    }
    async fn wait(&self, path: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notified = self.state.observed.notified();
                if self.count(path) != 0 {
                    break;
                }
                notified.await;
            }
        })
        .await
        .unwrap();
    }
    fn response(
        &self,
        nonce: &str,
        expires_in: u64,
        subject: &str,
        refresh: Option<&str>,
    ) -> Value {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut access = json!({"iss":self.origin(),"sub":subject,"aud":["native-api"],"client_id":"native-client","token_use":"access","jti":"refreshed","iat":now,"exp":now+3600});
        if let Some(resource) = self.state.resource.lock().unwrap().as_ref() {
            access["aud"]
                .as_array_mut()
                .unwrap()
                .push(resource.clone().into());
        }
        let mut body = json!({"access_token":signed(&self.key,&access),"token_type":"Bearer","expires_in":expires_in});
        {
            let claims = json!({"iss":self.origin(),"sub":subject,"aud":"native-client","iat":now,"exp":now+3600,"preferred_username":"verified-user","organization":"verified-org","groups":["two","one"],"nonce":nonce});
            body["id_token"] = signed(&self.key, &claims).into();
        }
        if let Some(refresh) = refresh {
            body["refresh_token"] = refresh.into();
        }
        *self.state.response.lock().unwrap() = body.to_string();
        body
    }
    fn verifier(&self, revision: u64, endpoint: &str, resource: Option<&str>) -> BrokerVerifier {
        BrokerVerifier::new(
            BrokerPolicy {
                endpoint,
                mcp_resource: resource,
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
async fn serve_connection(mut stream: tokio::net::TcpStream, state: Arc<HttpState>) {
    let mut input = Vec::new();
    let mut block = [0; 1024];
    let end = loop {
        if let Some(n) = input.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break n + 4;
        }
        let Ok(n) = stream.read(&mut block).await else {
            return;
        };
        if n == 0 || input.len() + n > 16384 {
            return;
        }
        input.extend_from_slice(&block[..n]);
    };
    let head = std::str::from_utf8(&input[..end]).unwrap();
    let path = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .split('?')
        .next()
        .unwrap()
        .to_owned();
    let post = head.starts_with("POST ");
    let len: usize = head
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse().unwrap())
        })
        .unwrap_or(0);
    if len > 8192 {
        return;
    }
    while input.len() < end + len {
        let Ok(n) = stream.read(&mut block).await else {
            return;
        };
        if n == 0 || input.len() + n > 16384 {
            return;
        }
        input.extend_from_slice(&block[..n]);
    }
    {
        let mut counts = state.counts.lock().unwrap();
        if counts.len() >= 8 && !counts.contains_key(&path) {
            return;
        }
        *counts.entry(path.clone()).or_default() += 1;
    }
    state.observed.notify_waiters();
    if post {
        let mut forms = state.forms.lock().unwrap();
        assert!(forms.len() < 8);
        forms.push(String::from_utf8(input[end..end + len].to_vec()).unwrap());
    }
    let origin = &state.origin;
    let (status, body) = match path.as_str() {
        "/.well-known/graphit-broker" => (200, json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,"client_id":"native-client","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["native-api"],"access_token_audience":"native-api","mcp_resources":state.resource.lock().unwrap().iter().cloned().collect::<Vec<_>>()}}).to_string()),
        "/.well-known/openid-configuration" => (200, json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),"device_authorization_endpoint":format!("{origin}/device/authorize"),"grant_types_supported":["authorization_code","refresh_token","urn:ietf:params:oauth:grant-type:device_code"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}).to_string()),
        "/device/authorize" => (200, json!({"device_code":"device-grant-secret-canary","user_code":"ABCD-EFGH","verification_uri":format!("{origin}/device"),"expires_in":600,"interval":1}).to_string()),
        "/keys" => (200, state.keys.to_string()),
        "/token" => (*state.status.lock().unwrap(), state.response.lock().unwrap().clone()),
        "/userinfo" => (200, json!({"sub":"native-subject"}).to_string()),
        _ => (404, "{}".into()),
    };
    let gate = state.gates.lock().unwrap().get(&path).cloned();
    if let Some(gate) = gate {
        gate.notified().await;
    }
    if path == "/token" && state.disconnect_token.load(Ordering::SeqCst) {
        return;
    }
    let response = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}
fn signed(key: &Ed25519KeyPair, claims: &Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA","kid":"one","typ":"JWT"}"#);
    let body = URL_SAFE_NO_PAD.encode(claims.to_string());
    let input = format!("{header}.{body}");
    format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(input.as_bytes()).as_ref())
    )
}

fn flow_limits() -> OAuthLimits {
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
fn profile_policy() -> ProfileLimits {
    ProfileLimits {
        bytes: 4096,
        nodes: 4096,
        scalar_bytes: 2048,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    }
}
fn issuer_for(
    f: &Fixture,
    ctx: &RequestContext<'_>,
    clock: Arc<TestClock>,
    policy: ProfileLimits,
    admin: &[Permission],
) -> RoleIssuer {
    RoleIssuer::with_sources(
        f.realm(),
        Arc::clone(&f.runtime),
        f.auth_store_policy(ctx, policy),
        authority(&all_permissions(), admin, limits(), 60),
        ttl(),
        ctx,
        clock,
        Arc::new(OsEntropy),
    )
    .unwrap()
}
fn broker_config(origin: &str) -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Broker(BrokerService {
            endpoint: origin.into(),
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
fn secret(value: &str) -> Secret {
    Secret::new(SecretString::from(value))
}
fn stored_profile(name: &str, origin: &str) -> ProfileData {
    ProfileData {
        name: name.into(),
        provider: "broker".into(),
        username: "old-user".into(),
        organization: Some("old-org".into()),
        teams: vec!["old-team".into()],
        credentials: ProfileCredentials::Broker {
            issuer: origin.into(),
            subject: "old-subject".into(),
            tokens: OidcTokens {
                access_token: secret("old-access"),
                refresh_token: Some(secret("old-refresh")),
                id_token: secret("old-id"),
                expires_at: 0,
            },
            s3_disabled: false,
        },
    }
}
fn seed(issuer: &RoleIssuer, ctx: &RequestContext<'_>, origin: &str, profiles: bool) -> u64 {
    let store = issuer.profile_store();
    applied(
        store
            .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| {
                doc.add_provider(
                    "broker".into(),
                    broker_config(origin),
                    doc.revision(),
                    1,
                    ctx,
                )?;
                if profiles {
                    let revision = doc.provider("broker").unwrap().revision();
                    for name in ["a", "b"] {
                        doc.replace_profile_data(
                            stored_profile(name, origin),
                            ProfileExpectation {
                                document_revision: doc.revision(),
                                provider_revision: revision,
                                profile_revision: None,
                            },
                            1,
                            ctx,
                        )?;
                    }
                    doc.activate("a", doc.revision(), ctx)?;
                }
                Ok(())
            })
            .unwrap(),
    );
    store
        .load(ctx)
        .unwrap()
        .document()
        .provider("broker")
        .unwrap()
        .revision()
}
fn saved(f: &Fixture) -> Vec<u8> {
    std::fs::read(f.installation.owner_root().join("auth.json")).unwrap()
}
fn selected(profile: &str) -> BrokerLoginSelection<'_> {
    BrokerLoginSelection {
        profile,
        provider: "broker",
        s3_disabled: true,
    }
}
fn callback(pending: &mut ManagedBrokerLogin<'_>) -> (String, String) {
    let url = url::Url::parse(pending.authorization_url().unwrap()).unwrap();
    let params: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(params["response_type"], "code");
    (
        format!(
            "http://127.0.0.1:43891/callback?code=canary-code&state={}",
            params["state"]
        ),
        params["nonce"].clone(),
    )
}
fn denied(
    error: BrokerLoginError,
    effect: RemoteEffect,
    broker: &LoginFixture,
    posts: usize,
    f: &Fixture,
    prior: &[u8],
) {
    assert_eq!(error.remote_effect(), effect);
    assert_eq!(broker.count("/token"), posts);
    assert_eq!(broker.count("/userinfo"), 0);
    assert_eq!(saved(f), prior);
    assert!(
        !f.installation
            .owner_root()
            .join(".auth.json.stage")
            .exists()
    );
    let text = format!("{error:?} {error}");
    for sensitive in ["canary", "old-refresh", "verified-user", broker.origin()] {
        assert!(!text.contains(sensitive));
    }
}
fn mutate(issuer: &RoleIssuer, ctx: &RequestContext<'_>, origin: &str, variant: usize) {
    let store = issuer.profile_store();
    applied(
        store
            .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| match variant {
                0 => doc.add_provider("other".into(), config(), doc.revision(), 2, ctx),
                1 => doc.update_provider(
                    "broker",
                    broker_config(origin),
                    doc.revision(),
                    doc.provider("broker").unwrap().revision(),
                    2,
                    ctx,
                ),
                2 => doc.replace_profile_data(
                    stored_profile("a", origin),
                    ProfileExpectation {
                        document_revision: doc.revision(),
                        provider_revision: doc.provider("broker").unwrap().revision(),
                        profile_revision: Some(doc.profile("a").unwrap().revision()),
                    },
                    2,
                    ctx,
                ),
                3 => {
                    let p = doc.profile("a").unwrap();
                    let expected = TokenExpectation {
                        document_revision: doc.revision(),
                        provider_revision: doc.provider("broker").unwrap().revision(),
                        profile_revision: p.revision(),
                        token_revision: p.token_revision(),
                    };
                    doc.rotate_tokens(
                        "a",
                        TokenUpdate {
                            access_token: secret("winner-access"),
                            refresh_token: secret("winner-refresh"),
                            identity: crate::profile::ProfileIdentityUpdate::Preserve,
                            expires_at: 1,
                        },
                        expected,
                        2,
                        ctx,
                    )
                }
                4 => doc.activate("b", doc.revision(), ctx),
                5 => {
                    doc.activate("b", doc.revision(), ctx)?;
                    doc.activate("a", doc.revision(), ctx)
                }
                _ => unreachable!(),
            })
            .unwrap(),
    );
}

#[tokio::test]
async fn initial_login_creates_or_replaces_and_activates_once_with_optional_refresh() {
    for existing in [false, true] {
        for refresh in [None, Some("new-refresh")] {
            let cancel = Cancellation::default();
            let ctx = operation(&cancel);
            let f = Fixture::new(&ctx);
            let broker = LoginFixture::new().await;
            let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
            let revision = seed(&issuer, &ctx, broker.origin(), existing);
            let verifier = broker.verifier(revision, broker.origin(), None);
            let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let session = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &session).unwrap();
            let m = manager(&issuer);
            let baseline = issuer.profile_store().load(&ctx).unwrap();
            let old = baseline
                .document()
                .profile("a")
                .map(|p| (p.revision(), p.token_revision(), p.created_at()));
            let document_revision = baseline.document().revision();
            let mut pending = m
                .begin_broker_login(
                    begin(&m, &parent),
                    &flow,
                    selected("a"),
                    "http://127.0.0.1:43891/callback",
                )
                .await
                .unwrap();
            let (callback, nonce) = callback(&mut pending);
            let response = broker.response(&nonce, 300, "new-subject", refresh);
            applied(m.finish_broker_login(pending, &callback, 2).await.unwrap());
            let result = issuer.profile_store().load(&ctx).unwrap();
            assert_eq!(result.document().revision(), document_revision + 2);
            assert_eq!(result.document().active_name(), Some("a"));
            let profile = result.document().profile("a").unwrap();
            assert_eq!(profile.data().unwrap().username, "verified-user");
            assert_eq!(profile.data().unwrap().teams, ["two", "one"]);
            let ProfileCredentials::Broker {
                subject,
                issuer: actual_issuer,
                tokens,
                s3_disabled,
            } = &profile.data().unwrap().credentials
            else {
                panic!()
            };
            assert_eq!(subject, "new-subject");
            assert_eq!(actual_issuer, broker.origin());
            assert!(*s3_disabled);
            assert_eq!(
                tokens.access_token.expose(),
                response["access_token"].as_str().unwrap()
            );
            assert_eq!(tokens.refresh_token.as_ref().map(Secret::expose), refresh);
            assert_eq!(
                tokens.id_token.expose(),
                response["id_token"].as_str().unwrap()
            );
            if let Some((p, t, created)) = old {
                assert_eq!(profile.revision(), p + 1);
                assert_eq!(profile.token_revision(), t + 1);
                assert_eq!(profile.created_at(), created);
            } else {
                assert_eq!(profile.revision(), document_revision + 2);
                assert_eq!(profile.token_revision(), document_revision + 1);
                assert_eq!(profile.created_at(), 2);
            }
            assert_eq!(broker.count("/token"), 1);
            assert_eq!(broker.count("/userinfo"), 0);
            {
                let forms = broker.state.forms.lock().unwrap();
                let params: BTreeMap<String, String> =
                    url::form_urlencoded::parse(forms[0].as_bytes())
                        .into_owned()
                        .collect();
                assert_eq!(params["grant_type"], "authorization_code");
                assert_eq!(params["code"], "canary-code");
                assert!(params.contains_key("code_verifier"));
            }
            broker.shutdown().await;
        }
    }
}

#[tokio::test]
async fn wrong_manager_service_provider_selectors_and_timestamp_deny_before_post() {
    for variant in 0..8 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), true);
        let verifier = broker.verifier(revision + u64::from(variant == 1), broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let other = manager(&issuer);
        let prior = saved(&f);
        let snapshot = begin(&m, &parent);
        let result = match variant {
            0 => {
                other
                    .begin_broker_login(
                        snapshot,
                        &flow,
                        selected("a"),
                        "http://127.0.0.1:43891/callback",
                    )
                    .await
            }
            2 => {
                m.begin_broker_login(
                    snapshot,
                    &flow,
                    BrokerLoginSelection {
                        profile: "a",
                        provider: "missing",
                        s3_disabled: true,
                    },
                    "http://127.0.0.1:43891/callback",
                )
                .await
            }
            3 => {
                m.begin_broker_login(
                    snapshot,
                    &flow,
                    selected(" bad "),
                    "http://127.0.0.1:43891/callback",
                )
                .await
            }
            4 => {
                m.begin_broker_login(
                    snapshot,
                    &flow,
                    selected("a/b"),
                    "http://127.0.0.1:43891/callback",
                )
                .await
            }
            _ => {
                m.begin_broker_login(
                    snapshot,
                    &flow,
                    selected("a"),
                    "http://127.0.0.1:43891/callback",
                )
                .await
            }
        };
        let error = if variant >= 5 {
            let mut p = result.unwrap();
            let (callback, nonce) = callback(&mut p);
            broker.response(&nonce, 300, "new-subject", None);
            match variant {
                5 => other
                    .finish_broker_login(p, &callback, 2)
                    .await
                    .err()
                    .unwrap(),
                6 => m.finish_broker_login(p, &callback, -1).await.err().unwrap(),
                _ => m.finish_broker_login(p, &callback, 0).await.err().unwrap(),
            }
        } else {
            result.err().unwrap()
        };
        denied(error, RemoteEffect::NotSent, &broker, 0, &f, &prior);
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn physical_cas_and_active_aba_deny_url_and_finish_without_post() {
    for variant in 0..6 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), true);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let mut p = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();
        let (callback, nonce) = callback(&mut p);
        broker.response(&nonce, 300, "new-subject", None);
        mutate(&issuer, &ctx, broker.origin(), variant);
        let winner = saved(&f);
        denied(
            p.authorization_url().err().unwrap(),
            RemoteEffect::NotSent,
            &broker,
            0,
            &f,
            &winner,
        );
        denied(
            m.finish_broker_login(p, &callback, 2).await.err().unwrap(),
            RemoteEffect::NotSent,
            &broker,
            0,
            &f,
            &winner,
        );
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn discovery_barrier_changes_original_snapshot_before_pending_exposure() {
    for variant in 0..6 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), true);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let gate = broker.gate("/.well-known/graphit-broker");
        let future = m.begin_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            "http://127.0.0.1:43891/callback",
        );
        tokio::pin!(future);
        let (error, winner) = tokio::select! {
            _=&mut future=>panic!("barrier bypassed"),
            _=broker.wait("/.well-known/graphit-broker")=>{
                mutate(&issuer,&ctx,broker.origin(),variant);
                let winner = saved(&f);
                gate.notify_one();
                (future.await.err().unwrap(), winner)
            }
        };
        denied(error, RemoteEffect::NotSent, &broker, 0, &f, &winner);
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn post_barrier_cas_changes_preserve_remote_uncertainty_and_winner() {
    for variant in 0..6 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), true);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let mut p = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();
        let (callback, nonce) = callback(&mut p);
        broker.response(&nonce, 300, "new-subject", None);
        let gate = broker.gate("/token");
        let future = m.finish_broker_login(p, &callback, 2);
        tokio::pin!(future);
        let (error, winner) = tokio::select! {
            _=&mut future=>panic!("barrier bypassed"),
            _=broker.wait("/token")=>{
                mutate(&issuer,&ctx,broker.origin(),variant);
                let winner = saved(&f);
                gate.notify_one();(future.await.err().unwrap(), winner)
            }
        };
        denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &winner);
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn bad_nonce_or_missing_id_never_publishes() {
    for bad_id in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let mut p = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();
        let (callback, nonce) = callback(&mut p);
        let mut body = broker.response(
            if bad_id { &nonce } else { "wrong-nonce" },
            300,
            "new-subject",
            None,
        );
        if bad_id {
            body.as_object_mut().unwrap().remove("id_token");
            *broker.state.response.lock().unwrap() = body.to_string();
        }
        let prior = saved(&f);
        denied(
            m.finish_broker_login(p, &callback, 2).await.err().unwrap(),
            RemoteEffect::MayHaveApplied,
            &broker,
            1,
            &f,
            &prior,
        );
        let p = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();

        drop(p);
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn original_cancel_and_final_authority_check_block_local_commit() {
    for final_stage in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let test_clock = clock();
        let issuer = issuer_for(
            &f,
            &ctx,
            Arc::clone(&test_clock),
            profile_policy(),
            &all_permissions(),
        );
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let mut p = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();
        let (callback, nonce) = callback(&mut p);
        broker.response(&nonce, 300, "new-subject", None);
        let prior = saved(&f);
        if final_stage {
            after_encoding(move || test_clock.seconds.store(61, Ordering::SeqCst));
            denied(
                m.finish_broker_login(p, &callback, 2).await.err().unwrap(),
                RemoteEffect::MayHaveApplied,
                &broker,
                1,
                &f,
                &prior,
            );
        } else {
            cancel.cancel();
            denied(
                p.authorization_url().err().unwrap(),
                RemoteEffect::NotSent,
                &broker,
                0,
                &f,
                &prior,
            );
            denied(
                m.finish_broker_login(p, &callback, 2).await.err().unwrap(),
                RemoteEffect::NotSent,
                &broker,
                0,
                &f,
                &prior,
            );
        }
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn real_core_cancelled_publication_returns_original_cleanup_proof() {
    use graphit_core::auth_document::{AuthDocumentError, recovery::AuthCleanupEffect};
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), false);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut p = m
        .begin_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            "http://127.0.0.1:43891/callback",
        )
        .await
        .unwrap();
    let (callback, nonce) = callback(&mut p);
    broker.response(&nonce, 300, "new-subject", None);
    let prior = saved(&f);
    let guard = issuer
        .profile_store()
        .cancel_publication_before_replace(&ctx)
        .unwrap();
    let publication = m.finish_broker_login(p, &callback, 2).await.unwrap();
    assert_eq!(publication.effect(), AuthPublicationEffect::NotApplied);
    assert_eq!(publication.failure(), Some(AuthDocumentError::Cancelled));
    assert_eq!(publication.cleanup_effect(), AuthCleanupEffect::NotApplied);
    assert_eq!(
        publication.cleanup_failure(),
        Some(AuthDocumentError::Cancelled)
    );
    let pending = publication
        .into_pending_cleanup()
        .expect("actual retained Core proof");
    assert_eq!(saved(&f), prior);
    assert_eq!(broker.count("/token"), 1);
    let stage = f.installation.owner_root().join(".auth.json.stage");
    let staged = std::fs::read(&stage).unwrap();
    drop(guard);
    let denied = m
        .cleanup_pending(ProfileManagerAuthority::Admin(&parent), pending)
        .err()
        .unwrap();
    let pending = denied.into_pending_cleanup();
    assert_eq!(std::fs::read(&stage).unwrap(), staged);
    let cleanup_cancel = Cancellation::default();
    let cleanup = operation(&cleanup_cancel);
    let caller = issuer.authenticate_runtime(&f.secret(), &cleanup).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let result = m.cleanup_pending(ProfileManagerAuthority::Admin(&parent), pending);
    assert!(result.is_ok());
    assert!(!stage.exists());
    assert_eq!(saved(&f), prior);
    broker.shutdown().await;
}

#[tokio::test]
async fn role_authority_positive_revoked_before_post_and_during_post() {
    for revoked in 0..3 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let definition = f.definition(
            Role::Coordinator,
            &[Permission::ManageAuthProfiles],
            1,
            limits(),
        );
        let reference = definition.reference().clone();
        issuer.register_definition(&parent, definition).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(900))
            .unwrap();
        let bearer = issuer
            .issue(
                &parent,
                &run,
                &reference,
                &[Permission::ManageAuthProfiles],
                &f.scope(),
            )
            .unwrap();
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let m = manager(&issuer);
        let snapshot = m.begin(ProfileManagerAuthority::Role(&grant)).unwrap();
        let mut p = m
            .begin_broker_login(
                snapshot,
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();
        let (callback, nonce) = callback(&mut p);
        broker.response(&nonce, 300, "new-subject", None);
        let prior = saved(&f);
        if revoked == 0 {
            applied(m.finish_broker_login(p, &callback, 2).await.unwrap());
            assert_eq!(broker.count("/token"), 1);
        } else if revoked == 1 {
            issuer.revoke(&parent, &bearer).unwrap();
            denied(
                p.authorization_url().err().unwrap(),
                RemoteEffect::NotSent,
                &broker,
                0,
                &f,
                &prior,
            );
            denied(
                m.finish_broker_login(p, &callback, 2).await.err().unwrap(),
                RemoteEffect::NotSent,
                &broker,
                0,
                &f,
                &prior,
            );
        } else {
            let gate = broker.gate("/token");
            let future = m.finish_broker_login(p, &callback, 2);
            tokio::pin!(future);
            let error = tokio::select! {
                _=&mut future=>panic!("barrier bypassed"),
                _=broker.wait("/token")=>{issuer.revoke(&parent,&bearer).unwrap();gate.notify_one();future.await.err().unwrap()}
            };
            denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &prior);
        }
        broker.shutdown().await;
    }
}
#[tokio::test]
async fn actual_token_scalar_exact_and_one_over_admission_has_no_local_effect_on_denial() {
    for token_cap in [false, true] {
        for one_under in [false, true] {
            let cancel = Cancellation::default();
            let ctx = operation(&cancel);
            let f = Fixture::new(&ctx);
            let broker = LoginFixture::new().await;
            let sample = broker.response(&"n".repeat(43), 300, "new-subject", None);
            let exact = sample["access_token"]
                .as_str()
                .unwrap()
                .len()
                .max(sample["id_token"].as_str().unwrap().len());
            let mut profile_policy = profile_policy();
            let mut native_policy = flow_limits();
            if token_cap {
                native_policy.token_bytes = exact - usize::from(one_under);
            } else {
                profile_policy.scalar_bytes = exact - usize::from(one_under);
            }
            let issuer = issuer_for(&f, &ctx, clock(), profile_policy, &all_permissions());
            let revision = seed(&issuer, &ctx, broker.origin(), false);
            let verifier = broker.verifier(revision, broker.origin(), None);
            let flow = NativeOAuth::new(&verifier, native_policy).unwrap();
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let session = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &session).unwrap();
            let m = manager(&issuer);
            let mut p = m
                .begin_broker_login(
                    begin(&m, &parent),
                    &flow,
                    selected("a"),
                    "http://127.0.0.1:43891/callback",
                )
                .await
                .unwrap();
            let (callback, nonce) = callback(&mut p);
            let body = broker.response(&nonce, 300, "new-subject", None);
            assert_eq!(
                body["access_token"]
                    .as_str()
                    .unwrap()
                    .len()
                    .max(body["id_token"].as_str().unwrap().len()),
                exact
            );
            let prior = saved(&f);
            let result = m.finish_broker_login(p, &callback, 2).await;
            if one_under {
                denied(
                    result.err().unwrap(),
                    RemoteEffect::MayHaveApplied,
                    &broker,
                    1,
                    &f,
                    &prior,
                );
            } else {
                applied(result.unwrap());
            }
            broker.shutdown().await;
        }
    }
}
#[tokio::test]
async fn selector_scalar_exact_and_profile_slot_boundaries_precede_discovery() {
    for over in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let name = "x".repeat(profile_policy().scalar_bytes + usize::from(over));
        let result = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected(&name),
                "http://127.0.0.1:43891/callback",
            )
            .await;
        if over {
            assert_eq!(
                result.err().unwrap().kind(),
                BrokerLoginErrorKind::Manager(ProfileManagerError::Store(AuthStoreError::Profile(
                    ProfileError::Limit
                )))
            );
            assert_eq!(broker.count("/.well-known/graphit-broker"), 0);
        } else {
            drop(result.unwrap());
            assert_eq!(broker.count("/.well-known/graphit-broker"), 1);
        }
        assert_eq!(broker.count("/token"), 0);
        broker.shutdown().await;
    }
    for profiles in [2, 3] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let mut policy = profile_policy();
        policy.profiles = profiles;
        let issuer = issuer_for(&f, &ctx, clock(), policy, &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), true);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let result = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("new-profile"),
                "http://127.0.0.1:43891/callback",
            )
            .await;
        if profiles == 2 {
            assert_eq!(result.err().unwrap().remote_effect(), RemoteEffect::NotSent);
            assert_eq!(broker.count("/.well-known/graphit-broker"), 0);
        } else {
            drop(result.unwrap());
            assert_eq!(broker.count("/.well-known/graphit-broker"), 1);
        }
        broker.shutdown().await;
    }
}

#[tokio::test]
async fn literal_endpoint_resource_local_provider_and_missing_permission_deny_without_discovery() {
    for variant in 0..4 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let admin = if variant == 3 {
            vec![Permission::ManageAccessGrants]
        } else {
            all_permissions()
        };
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &admin);
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        if variant == 2 {
            let store = issuer.profile_store();
            applied(
                store
                    .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                        doc.add_provider("local".into(), config(), doc.revision(), 1, ctx)
                    })
                    .unwrap(),
            );
        }
        let wrong_endpoint = format!("{}/other", broker.origin());
        let endpoint = if variant == 0 {
            &wrong_endpoint
        } else {
            broker.origin()
        };
        let verifier = broker.verifier(
            revision,
            endpoint,
            if variant == 1 {
                Some("https://other.example/mcp")
            } else {
                None
            },
        );
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        if variant == 3 {
            assert!(m.begin(ProfileManagerAuthority::Admin(&parent)).is_err());
        } else {
            let selector = if variant == 2 {
                BrokerLoginSelection {
                    profile: "a",
                    provider: "local",
                    s3_disabled: false,
                }
            } else {
                selected("a")
            };
            let prior = saved(&f);
            denied(
                m.begin_broker_login(
                    begin(&m, &parent),
                    &flow,
                    selector,
                    "http://127.0.0.1:43891/callback",
                )
                .await
                .err()
                .unwrap(),
                RemoteEffect::NotSent,
                &broker,
                0,
                &f,
                &prior,
            );
        }
        assert_eq!(broker.count("/.well-known/graphit-broker"), 0);
        broker.shutdown().await;
    }
}
#[tokio::test]
async fn verified_groups_exact_and_one_over_observe_profile_cap_before_publication() {
    for count in [32, 33] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let mut policy = profile_policy();
        policy.collection_entries = 32;
        let issuer = issuer_for(&f, &ctx, clock(), policy, &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let mut p = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();
        let (callback, nonce) = callback(&mut p);
        let mut body = broker.response(&nonce, 300, "new-subject", None);
        let id = body["id_token"].as_str().unwrap();
        let encoded = id.split('.').nth(1).unwrap();
        let mut claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded).unwrap()).unwrap();
        claims["groups"] = (0..count)
            .map(|n| format!("team-{n:02}"))
            .collect::<Vec<_>>()
            .into();
        body["id_token"] = signed(&broker.key, &claims).into();
        *broker.state.response.lock().unwrap() = body.to_string();
        let prior = saved(&f);
        let result = m.finish_broker_login(p, &callback, 2).await;
        if count == 33 {
            denied(
                result.err().unwrap(),
                RemoteEffect::MayHaveApplied,
                &broker,
                1,
                &f,
                &prior,
            );
        } else {
            applied(result.unwrap());
            assert_eq!(
                issuer
                    .profile_store()
                    .load(&ctx)
                    .unwrap()
                    .document()
                    .profile("a")
                    .unwrap()
                    .data()
                    .unwrap()
                    .teams
                    .len(),
                32
            );
        }
        broker.shutdown().await;
    }
}
#[tokio::test]
async fn replacement_can_switch_provider_subject_and_activate_inactive_target() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    seed(&issuer, &ctx, broker.origin(), true);
    let store = issuer.profile_store();
    applied(
        store
            .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                doc.add_provider(
                    "other-broker".into(),
                    broker_config(broker.origin()),
                    doc.revision(),
                    1,
                    ctx,
                )
            })
            .unwrap(),
    );
    let old = store.load(&ctx).unwrap();
    let b = old.document().profile("b").unwrap();
    let revisions = (
        old.document().revision(),
        b.revision(),
        b.token_revision(),
        b.created_at(),
    );
    let revision = old.document().provider("other-broker").unwrap().revision();
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut p = m
        .begin_broker_login(
            begin(&m, &parent),
            &flow,
            BrokerLoginSelection {
                profile: "b",
                provider: "other-broker",
                s3_disabled: false,
            },
            "http://127.0.0.1:43891/callback",
        )
        .await
        .unwrap();
    let (callback, nonce) = callback(&mut p);
    broker.response(&nonce, 300, "another-subject", None);
    applied(m.finish_broker_login(p, &callback, 2).await.unwrap());
    let result = store.load(&ctx).unwrap();
    let b = result.document().profile("b").unwrap();
    assert_eq!(result.document().active_name(), Some("b"));
    assert_eq!(result.document().revision(), revisions.0 + 2);
    assert_eq!(b.revision(), revisions.1 + 2);
    assert_eq!(b.token_revision(), revisions.2 + 1);
    assert_eq!(b.created_at(), revisions.3);
    assert_eq!(b.data().unwrap().provider, "other-broker");
    let ProfileCredentials::Broker {
        subject,
        tokens,
        s3_disabled,
        ..
    } = &b.data().unwrap().credentials
    else {
        panic!()
    };
    assert_eq!(subject, "another-subject");
    assert!(tokens.refresh_token.is_none());
    assert!(!s3_disabled);
    broker.shutdown().await;
}
#[tokio::test]
async fn revision_exact_and_one_over_are_checked_before_discovery() {
    for over in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let initial = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&initial, &ctx, broker.origin(), true);
        // Owned untrusted input fixture, before any opaque managed observation exists.
        let mut input: Value = serde_json::from_slice(&saved(&f)).unwrap();
        input["document_revision"] = json!(u64::MAX - 2 + u64::from(over));
        for p in input["profiles"].as_array_mut().unwrap() {
            p["revision"] = json!(u64::MAX - 2);
            p["token_revision"] = json!(u64::MAX - 2);
        }
        std::fs::write(
            f.installation.owner_root().join("auth.json"),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let prior = saved(&f);
        let result = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("b"),
                "http://127.0.0.1:43891/callback",
            )
            .await;
        if over {
            denied(
                result.err().unwrap(),
                RemoteEffect::NotSent,
                &broker,
                0,
                &f,
                &prior,
            );
            assert_eq!(broker.count("/.well-known/graphit-broker"), 0);
        } else {
            let mut p = result.unwrap();
            let (callback, nonce) = callback(&mut p);
            broker.response(&nonce, 300, "new-subject", None);
            applied(m.finish_broker_login(p, &callback, 2).await.unwrap());
            let final_doc = issuer.profile_store().load(&ctx).unwrap();
            assert_eq!(final_doc.document().revision(), u64::MAX);
            assert_eq!(
                final_doc.document().profile("b").unwrap().revision(),
                u64::MAX
            );
        }
        broker.shutdown().await;
    }
}
#[tokio::test]
async fn prospective_encoded_aggregate_exact_and_one_under_use_real_prepare() {
    for one_under in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let sample = broker.response(&"n".repeat(43), 300, "new-subject", None);
        let initial = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&initial, &ctx, broker.origin(), false);
        let mut model = AuthDocument::decode(&saved(&f), profile_policy(), &ctx).unwrap();
        let expectation = ProfileExpectation {
            document_revision: model.revision(),
            provider_revision: revision,
            profile_revision: None,
        };
        let expires = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros(),
        )
        .unwrap()
            + 300_000_000;
        model
            .replace_profile_data(
                ProfileData {
                    name: "a".into(),
                    provider: "broker".into(),
                    username: "verified-user".into(),
                    organization: Some("verified-org".into()),
                    teams: vec!["one".into(), "two".into()],
                    credentials: ProfileCredentials::Broker {
                        issuer: broker.origin().into(),
                        subject: "new-subject".into(),
                        tokens: OidcTokens {
                            access_token: secret(sample["access_token"].as_str().unwrap()),
                            refresh_token: None,
                            id_token: secret(sample["id_token"].as_str().unwrap()),
                            expires_at: expires,
                        },
                        s3_disabled: true,
                    },
                },
                expectation,
                2,
                &ctx,
            )
            .unwrap();
        model.activate("a", model.revision(), &ctx).unwrap();
        let exact = model.encode(&ctx).unwrap().len();
        drop(model);
        let mut policy = profile_policy();
        policy.bytes = exact - usize::from(one_under);
        let issuer = issuer_for(&f, &ctx, clock(), policy, &all_permissions());
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let mut p = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();
        let (callback, nonce) = callback(&mut p);
        broker.response(&nonce, 300, "new-subject", None);
        let prior = saved(&f);
        let result = m.finish_broker_login(p, &callback, 2).await;
        if one_under {
            denied(
                result.err().unwrap(),
                RemoteEffect::MayHaveApplied,
                &broker,
                1,
                &f,
                &prior,
            );
        } else {
            applied(result.unwrap());
            assert_eq!(saved(&f).len(), exact);
        }
        broker.shutdown().await;
    }
}

fn groups_response(broker: &LoginFixture, nonce: &str, count: usize) -> Value {
    let mut body = broker.response(nonce, 300, "new-subject", None);
    let mut claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(
                body["id_token"]
                    .as_str()
                    .unwrap()
                    .split('.')
                    .nth(1)
                    .unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    claims["groups"] = (0..count)
        .map(|n| format!("team-{n:02}"))
        .collect::<Vec<_>>()
        .into();
    body["id_token"] = signed(&broker.key, &claims).into();
    *broker.state.response.lock().unwrap() = body.to_string();
    body
}
#[tokio::test]
async fn aggregate_copy_exact_and_one_under_preserves_publication_boundary() {
    //64 teams keep the admitted Vec backing larger than encoded-document overhead.
    //The native verifier/profile collection caps already admit64; no policy is raised.
    let count = profile_policy().collection_entries;
    assert_eq!(count, 64);
    let mut encoded_positive = None;
    for one_under in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = LoginFixture::new().await;
        let sample = groups_response(&broker, &"n".repeat(43), count);
        let exact = count * std::mem::size_of::<String>()
            + 2 * "a".len()
            + "broker".len()
            + broker.origin().len()
            + "new-subject".len()
            + sample["access_token"].as_str().unwrap().len()
            + sample["id_token"].as_str().unwrap().len()
            + "verified-user".len()
            + "verified-org".len()
            + count * "team-00".len();
        let mut policy = profile_policy();
        policy.bytes = exact - usize::from(one_under);
        assert!(sample["access_token"].as_str().unwrap().len() <= policy.scalar_bytes);
        assert!(sample["id_token"].as_str().unwrap().len() <= policy.scalar_bytes);
        if let Some(encoded_bytes) = encoded_positive {
            assert!(
                encoded_bytes < policy.bytes,
                "the independent document cap must fit"
            );
        }
        let issuer = issuer_for(&f, &ctx, clock(), policy, &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), false);
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let mut p = m
            .begin_broker_login(
                begin(&m, &parent),
                &flow,
                selected("a"),
                "http://127.0.0.1:43891/callback",
            )
            .await
            .unwrap();
        let (callback, nonce) = callback(&mut p);
        let body = groups_response(&broker, &nonce, count);
        assert_eq!(
            body["access_token"].as_str().unwrap().len(),
            sample["access_token"].as_str().unwrap().len()
        );
        assert_eq!(
            body["id_token"].as_str().unwrap().len(),
            sample["id_token"].as_str().unwrap().len()
        );
        let prior = saved(&f);
        let result = m.finish_broker_login(p, &callback, 2).await;
        if one_under {
            denied(
                result.err().unwrap(),
                RemoteEffect::MayHaveApplied,
                &broker,
                1,
                &f,
                &prior,
            );
        } else {
            applied(result.unwrap());
            let encoded_bytes = saved(&f).len();
            assert!(
                encoded_bytes < exact - 1,
                "document cap must have slack at copy -1"
            );
            encoded_positive = Some(encoded_bytes);
        }
        broker.shutdown().await;
    }
}
#[tokio::test]
async fn cancellation_during_real_post_preserves_remote_effect() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), false);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut p = m
        .begin_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            "http://127.0.0.1:43891/callback",
        )
        .await
        .unwrap();
    let (callback, nonce) = callback(&mut p);
    broker.response(&nonce, 300, "new-subject", None);
    let prior = saved(&f);
    let gate = broker.gate("/token");
    let future = m.finish_broker_login(p, &callback, 2);
    tokio::pin!(future);
    let error = tokio::select! {
    _=&mut future=>panic!("barrier bypassed"),
    _=broker.wait("/token")=>{cancel.cancel();gate.notify_one();future.await.err().unwrap()}
    };
    denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &prior);
    broker.shutdown().await;
}

#[tokio::test]
async fn dropping_actual_inflight_finish_keeps_remote_observation_and_releases_last_leases() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), false);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut p = m
        .begin_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            "http://127.0.0.1:43891/callback",
        )
        .await
        .unwrap();
    let (callback, nonce) = callback(&mut p);
    broker.response(&nonce, 300, "new-subject", None);
    let prior = saved(&f);
    let gate = broker.gate("/token");
    let mut future = Box::pin(m.finish_broker_login(p, &callback, 2));
    tokio::select! { _=&mut future=>panic!("barrier bypassed"),_=broker.wait("/token")=>{} }

    drop(future);

    assert_eq!(saved(&f), prior);
    assert_eq!(broker.count("/token"), 1);
    assert!(ctx.remaining().is_ok());
    gate.notify_one();
    // Dropping one operation leaves the original context usable.
    let p = m
        .begin_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            "http://127.0.0.1:43891/callback",
        )
        .await
        .unwrap();
    drop(p);
    broker.shutdown().await;
}
#[tokio::test]
async fn original_deadline_expires_during_real_post_without_local_publication() {
    let cancel = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(2);
    let ctx = RequestContext::new(
        ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 256,
            max_collection_entries: 64,
        },
        &cancel,
        deadline,
    )
    .unwrap();
    let f = Fixture::new(&ctx);
    let broker = LoginFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), false);
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let mut p = m
        .begin_broker_login(
            begin(&m, &parent),
            &flow,
            selected("a"),
            "http://127.0.0.1:43891/callback",
        )
        .await
        .unwrap();
    let (callback, nonce) = callback(&mut p);
    broker.response(&nonce, 300, "new-subject", None);
    let prior = saved(&f);
    let gate = broker.gate("/token");
    let future = m.finish_broker_login(p, &callback, 2);
    tokio::pin!(future);
    let error = tokio::select! {
        _=&mut future=>panic!("barrier bypassed"),
        _=broker.wait("/token")=>{
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline+Duration::from_millis(5))).await;
            gate.notify_one();future.await.err().unwrap()
        }
    };
    denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &prior);
    assert!(ctx.remaining().is_err());
    broker.shutdown().await;
}
