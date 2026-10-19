use super::*;
#[path = "broker_refresh_shared.rs"]
mod shared;
use crate::broker::oauth::{NativeOAuth, OAuthLimits, RemoteEffect};
use crate::broker::{BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits};
use crate::profile::{
    AuthDocument, BrokerService, OidcTokens, ProfileCredentials, ProfileData,
    ProfileIdentityUpdate, ProfileLimits, TokenExpectation, TokenUpdate,
};
use crate::profile_manager::{BrokerRefreshError, BrokerRefreshErrorKind};
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
}
struct BrokerFixture {
    state: Arc<HttpState>,
    task: JoinHandle<()>,
    key: Ed25519KeyPair,
}
impl Drop for BrokerFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl BrokerFixture {
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
        });
        let serve = Arc::clone(&state);
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break; };
                        if connections.len() >= 16 { break; }
                        let state = Arc::clone(&serve);
                        connections.spawn(serve_connection(stream, state));
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Self { state, task, key }
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
            while self.count(path) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    fn response(&self, id: bool, expires_in: u64, subject: &str, refresh: Option<&str>) -> Value {
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
        if id {
            let claims = json!({"iss":self.origin(),"sub":subject,"aud":"native-client","iat":now,"exp":now+3600,"preferred_username":"verified-user","organization":"verified-org","groups":["two","one"]});
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
    if post {
        let mut forms = state.forms.lock().unwrap();
        assert!(forms.len() < 8);
        forms.push(String::from_utf8(input[end..end + len].to_vec()).unwrap());
    }
    let origin = &state.origin;
    let (status, body) = match path.as_str() {
        "/.well-known/graphit-broker" => (200, json!({"version":"1","issuer":origin,"authentication":{"type":"oidc","issuer":origin,"client_id":"native-client","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["native-api"],"access_token_audience":"native-api","mcp_resources":state.resource.lock().unwrap().iter().cloned().collect::<Vec<_>>()}}).to_string()),
        "/.well-known/openid-configuration" => (200, json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}).to_string()),
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
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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

#[tokio::test]
async fn no_permission_wrong_issuer_local_and_absent_selection_have_no_remote_effect() {
    for variant in 0..4 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let admin = if variant == 0 {
            vec![Permission::ManageAccessGrants]
        } else {
            all_permissions()
        };
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &admin);
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let before = saved(&f);
        match variant {
            0 => {
                assert!(m.begin(ProfileManagerAuthority::Admin(&parent)).is_err());
            }
            1 => {
                let other = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
                let caller = other.authenticate_runtime(&f.secret(), &ctx).unwrap();
                let session = other.bootstrap_admin(&caller).unwrap();
                let parent = other.admin_parent(&caller, &session).unwrap();
                assert!(m.begin(ProfileManagerAuthority::Admin(&parent)).is_err());
            }
            2 => {
                let store = issuer.profile_store();
                applied(
                    store
                        .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                            doc.ensure_default_local(doc.revision(), 2, ctx)?;
                            let provider = doc.provider("local").unwrap().revision();
                            doc.replace_profile_data(
                                ProfileData {
                                    name: "local".into(),
                                    provider: "local".into(),
                                    username: "local-user".into(),
                                    organization: None,
                                    teams: vec![],
                                    credentials: ProfileCredentials::Local(
                                        input("local", Some("local-key")).credentials,
                                    ),
                                },
                                ProfileExpectation {
                                    document_revision: doc.revision(),
                                    provider_revision: provider,
                                    profile_revision: None,
                                },
                                2,
                                ctx,
                            )?;
                            doc.activate("local", doc.revision(), ctx)
                        })
                        .unwrap(),
                );
                let local_bytes = saved(&f);
                assert_denied(
                    m.refresh_broker(begin(&m, &parent), &flow, 3)
                        .await
                        .err()
                        .unwrap(),
                    RemoteEffect::NotSent,
                    &broker,
                    0,
                    &f,
                    &local_bytes,
                );
                let local = issuer.authenticate_local("local-key", &ctx).unwrap();
                let session = issuer.bootstrap_admin(&local).unwrap();
                let local_parent = issuer.admin_parent(&local, &session).unwrap();
                let snapshot = begin(&m, &local_parent);
                applied(
                    m.activate(begin(&m, &parent), "a", snapshot.revision())
                        .unwrap(),
                );
                let winner = saved(&f);
                assert_denied(
                    m.refresh_broker(snapshot, &flow, 3).await.err().unwrap(),
                    RemoteEffect::NotSent,
                    &broker,
                    0,
                    &f,
                    &winner,
                );
            }
            3 => {
                let snapshot = begin(&m, &parent);
                applied(
                    m.logout(
                        snapshot,
                        None,
                        issuer
                            .profile_store()
                            .load(&ctx)
                            .unwrap()
                            .document()
                            .revision(),
                    )
                    .unwrap(),
                );
                let winner = saved(&f);
                assert_denied(
                    m.refresh_broker(begin(&m, &parent), &flow, 2)
                        .await
                        .err()
                        .unwrap(),
                    RemoteEffect::NotSent,
                    &broker,
                    0,
                    &f,
                    &winner,
                );
            }
            _ => unreachable!(),
        }
        if variant < 2 {
            assert_eq!(saved(&f), before);
        }
        assert_eq!(broker.count("/token"), 0);
        assert_eq!(broker.count("/.well-known/graphit-broker"), 0);
    }
}

#[tokio::test]
async fn stored_issuer_mismatch_denies_after_discovery_before_post() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let store = issuer.profile_store();
    applied(
        store
            .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                let mut data = profile_data("a", broker.origin(), Some("old-refresh"));
                if let ProfileCredentials::Broker { issuer, .. } = &mut data.credentials {
                    *issuer = format!("{}/different", broker.origin());
                }
                let own = doc.profile("a").unwrap().revision();
                doc.replace_profile_data(
                    data,
                    ProfileExpectation {
                        document_revision: doc.revision(),
                        provider_revision: revision,
                        profile_revision: Some(own),
                    },
                    2,
                    ctx,
                )
            })
            .unwrap(),
    );
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let before = saved(&f);
    let error = m
        .refresh_broker(begin(&m, &parent), &flow, 3)
        .await
        .err()
        .unwrap();
    assert_denied(error, RemoteEffect::NotSent, &broker, 0, &f, &before);
    assert_eq!(error.kind(), BrokerRefreshErrorKind::IdentityMismatch);
    assert_eq!(broker.count("/.well-known/graphit-broker"), 1);
}

#[tokio::test]
async fn revoked_role_or_closed_run_during_http_is_checked_at_both_boundaries() {
    for after_post in [false, true] {
        for close in [false, true] {
            let cancel = Cancellation::default();
            let ctx = operation(&cancel);
            let f = Fixture::new(&ctx);
            let broker = BrokerFixture::new().await;
            let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
            let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
            let verifier = broker.verifier(revision, broker.origin(), None);
            let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
            broker.response(false, 300, "native-subject", Some("new-refresh"));
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
            let before = saved(&f);

            let path = if after_post {
                "/token"
            } else {
                "/.well-known/graphit-broker"
            };
            let gate = broker.gate(path);
            let (result, ()) = tokio::join!(m.refresh_broker(snapshot, &flow, 2), async {
                broker.wait(path).await;
                if close {
                    issuer.close_run(&parent, &run).unwrap();
                } else {
                    issuer.revoke(&parent, &bearer).unwrap();
                }
                gate.notify_one();
            });
            assert_denied(
                result.err().unwrap(),
                if after_post {
                    RemoteEffect::MayHaveApplied
                } else {
                    RemoteEffect::NotSent
                },
                &broker,
                usize::from(after_post),
                &f,
                &before,
            );
        }
    }
}

#[tokio::test]
async fn old_refresh_limit_denies_before_metadata_and_token_send() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(revision, broker.origin(), None);
    let mut limits = flow_limits();
    limits.token_bytes = "old-refresh".len() - 1;
    let flow = NativeOAuth::new(&verifier, limits).unwrap();
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let before = saved(&f);
    let error = m
        .refresh_broker(begin(&m, &parent), &flow, 2)
        .await
        .err()
        .unwrap();
    assert_denied(error, RemoteEffect::NotSent, &broker, 0, &f, &before);
    assert_eq!(broker.count("/.well-known/graphit-broker"), 0);
}

#[tokio::test]
async fn configured_literal_resource_is_preserved_through_native_refresh_and_form() {
    let resource = "https://workspace.example/mcp";
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    *broker.state.resource.lock().unwrap() = Some(resource.into());
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed_resource(
        &issuer,
        &ctx,
        broker.origin(),
        Some("old-refresh"),
        Some(resource),
    );
    let verifier = broker.verifier(revision, broker.origin(), Some(resource));
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    broker.response(true, 300, "native-subject", Some("new-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    applied(
        m.refresh_broker(begin(&m, &parent), &flow, 2)
            .await
            .unwrap(),
    );
    assert_eq!(broker.count("/token"), 1);
    assert_eq!(broker.count("/userinfo"), 0);
    let forms = broker.state.forms.lock().unwrap();
    let pairs: Vec<_> = url::form_urlencoded::parse(forms[0].as_bytes())
        .into_owned()
        .collect();
    assert_eq!(
        pairs.iter().filter(|(name, _)| name == "resource").count(),
        1
    );
    assert_eq!(
        pairs.iter().find(|(name, _)| name == "resource").unwrap().1,
        resource
    );
}

#[tokio::test]
async fn verified_groups_exact_and_one_over_use_real_profile_collection_cap() {
    for count in [16, 17] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let mut policy = profile_policy();
        policy.collection_entries = 16;
        let issuer = issuer_for(&f, &ctx, clock(), policy, &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let mut body = broker.response(true, 300, "native-subject", Some("new-refresh"));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = json!({"iss":broker.origin(),"sub":"native-subject","aud":"native-client","iat":now,"exp":now+3600,"preferred_username":"verified-user","groups":(0..count).map(|n|format!("group-{n}")).collect::<Vec<_>>()});
        body["id_token"] = signed(&broker.key, &claims).into();
        *broker.state.response.lock().unwrap() = body.to_string();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let before = saved(&f);
        let result = m.refresh_broker(begin(&m, &parent), &flow, 2).await;
        if count == 16 {
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
                16
            );
        } else {
            assert_denied(
                result.err().unwrap(),
                RemoteEffect::MayHaveApplied,
                &broker,
                1,
                &f,
                &before,
            );
        }
    }
}

#[tokio::test]
async fn wrong_jwt_issuer_bad_optional_id_and_http_body_cap_cannot_publish() {
    for variant in 0..3 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let mut body = broker.response(true, 300, "native-subject", Some("new-refresh"));
        if variant == 0 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let claims = json!({"iss":"https://wrong-issuer.example","sub":"native-subject","aud":["native-api"],"client_id":"native-client","token_use":"access","jti":"refreshed","iat":now,"exp":now+3600});
            body["access_token"] = signed(&broker.key, &claims).into();
        } else if variant == 1 {
            body["id_token"] = "canary".into();
        }
        *broker.state.response.lock().unwrap() = if variant == 2 {
            "x".repeat(8193)
        } else {
            body.to_string()
        };
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let before = saved(&f);
        let error = m
            .refresh_broker(begin(&m, &parent), &flow, 2)
            .await
            .err()
            .unwrap();
        assert_denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &before);
        assert!(!format!("{error:?}").contains("canary"));
    }
}

#[tokio::test]
async fn final_token_expiry_after_encoding_denies_before_local_commit() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    broker.response(false, 3, "native-subject", Some("new-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let before = saved(&f);

    let encoded = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let encoded_hook = Arc::clone(&encoded);
    after_encoding(move || {
        encoded_hook.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_secs(3));
    });
    let error = m
        .refresh_broker(begin(&m, &parent), &flow, 2)
        .await
        .err()
        .unwrap();
    assert!(
        encoded.load(Ordering::SeqCst),
        "verified tokens reached actual post-encoding failpoint"
    );
    assert_denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &before);
}

#[tokio::test]
async fn prospective_document_exact_and_one_under_preserve_physical_effects() {
    for one_under in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let body = broker.response(false, 300, "native-subject", Some("new-refresh"));
        let initial = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&initial, &ctx, broker.origin(), Some("old-refresh"));
        // Pure schema fixture model, not a restored observation or authentication proof.
        let mut model = AuthDocument::decode(&saved(&f), profile_policy(), &ctx).unwrap();
        let p = model.profile("a").unwrap();
        let expected = TokenExpectation {
            document_revision: model.revision(),
            provider_revision: revision,
            profile_revision: p.revision(),
            token_revision: p.token_revision(),
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
            .rotate_tokens(
                "a",
                TokenUpdate {
                    access_token: secret(body["access_token"].as_str().unwrap()),
                    refresh_token: secret("new-refresh"),
                    identity: ProfileIdentityUpdate::Preserve,
                    expires_at: expires,
                },
                expected,
                2,
                &ctx,
            )
            .unwrap();
        let exact = model.encode(&ctx).unwrap().len();
        let mut policy = profile_policy();
        policy.bytes = exact - usize::from(one_under);
        drop(model);
        let issuer = issuer_for(&f, &ctx, clock(), policy, &all_permissions());
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let before = saved(&f);

        let result = m.refresh_broker(begin(&m, &parent), &flow, 2).await;
        if one_under {
            assert_denied(
                result.err().unwrap(),
                RemoteEffect::MayHaveApplied,
                &broker,
                1,
                &f,
                &before,
            );
        } else {
            applied(result.unwrap());
            assert_eq!(saved(&f).len(), exact);
        }
    }
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
fn profile_data(name: &str, origin: &str, refresh: Option<&str>) -> ProfileData {
    ProfileData {
        name: name.into(),
        provider: "broker".into(),
        username: "old-user".into(),
        organization: Some("old-org".into()),
        teams: vec!["old-one".into(), "old-two".into()],
        credentials: ProfileCredentials::Broker {
            issuer: origin.into(),
            subject: "native-subject".into(),
            tokens: OidcTokens {
                access_token: secret("expired-access"),
                refresh_token: refresh.map(secret),
                id_token: secret("old-id"),
                expires_at: 0,
            },
            s3_disabled: true,
        },
    }
}
fn secret(value: &str) -> Secret {
    Secret::new(SecretString::from(value))
}
fn seed(issuer: &RoleIssuer, ctx: &RequestContext<'_>, origin: &str, refresh: Option<&str>) -> u64 {
    seed_resource(issuer, ctx, origin, refresh, None)
}
fn seed_resource(
    issuer: &RoleIssuer,
    ctx: &RequestContext<'_>,
    origin: &str,
    refresh: Option<&str>,
    resource: Option<&str>,
) -> u64 {
    let store = issuer.profile_store();
    applied(
        store
            .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| {
                let mut config = broker_config(origin);
                let ProviderKind::Broker(service) = &mut config.kind else {
                    unreachable!()
                };
                service.mcp_resource = resource.map(str::to_owned);
                doc.add_provider("broker".into(), config, doc.revision(), 1, ctx)?;
                let revision = doc.provider("broker").unwrap().revision();
                for name in ["a", "b"] {
                    doc.replace_profile_data(
                        profile_data(name, origin, refresh),
                        ProfileExpectation {
                            document_revision: doc.revision(),
                            provider_revision: revision,
                            profile_revision: None,
                        },
                        1,
                        ctx,
                    )?;
                }
                doc.activate("a", doc.revision(), ctx)
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
fn assert_denied(
    error: BrokerRefreshError,
    expected: RemoteEffect,
    broker: &BrokerFixture,
    posts: usize,
    f: &Fixture,
    winner: &[u8],
) {
    assert_eq!(error.remote_effect(), expected);
    assert_eq!(broker.count("/token"), posts);
    assert_eq!(broker.count("/userinfo"), 0);
    assert_eq!(saved(f), winner);
    assert!(
        !f.installation
            .owner_root()
            .join(".auth.json.stage")
            .exists()
    );
    assert!(!error.to_string().contains("old-refresh"));
}

#[tokio::test]
async fn refresh_admin_preserves_or_replaces_verified_identity_in_one_publication() {
    for with_id in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let body = broker.response(with_id, 300, "native-subject", Some("new-refresh"));
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let snapshot = begin(&m, &parent);
        let old_doc = snapshot.revision();
        let old = issuer.profile_store().load(&ctx).unwrap();
        let p = old.document().profile("a").unwrap();
        let old_profile = p.revision();
        let old_token = p.token_revision();
        let created = p.created_at();
        drop(old);
        applied(m.refresh_broker(snapshot, &flow, 2).await.unwrap());
        let current = issuer.profile_store().load(&ctx).unwrap();
        let doc = current.document();
        let p = doc.profile("a").unwrap();
        assert_eq!(
            (doc.revision(), p.revision(), p.token_revision()),
            (old_doc + 1, old_profile + 1, old_token + 1)
        );
        assert_eq!(doc.provider("broker").unwrap().revision(), revision);
        assert_eq!(p.created_at(), created);
        assert_eq!(doc.active_name(), Some("a"));
        let data = p.data().unwrap();
        let ProfileCredentials::Broker {
            issuer: stored_issuer,
            subject,
            tokens,
            s3_disabled,
        } = &data.credentials
        else {
            panic!()
        };
        assert_eq!(stored_issuer, broker.origin());
        assert_eq!(subject, "native-subject");
        assert!(*s3_disabled);
        assert_eq!(
            tokens.access_token.expose(),
            body["access_token"].as_str().unwrap()
        );
        assert_eq!(
            tokens.refresh_token.as_ref().unwrap().expose(),
            "new-refresh"
        );
        assert!(tokens.expires_at > 0);
        if with_id {
            assert_eq!(data.username, "verified-user");
            assert_eq!(data.organization.as_deref(), Some("verified-org"));
            assert_eq!(data.teams, ["two", "one"]);
            assert_eq!(tokens.id_token.expose(), body["id_token"].as_str().unwrap());
        } else {
            assert_eq!(data.username, "old-user");
            assert_eq!(data.organization.as_deref(), Some("old-org"));
            assert_eq!(data.teams, ["old-one", "old-two"]);
            assert_eq!(tokens.id_token.expose(), "old-id");
        }
        assert_eq!(broker.count("/token"), 1);
        assert_eq!(broker.count("/userinfo"), 0);
        let form = broker.state.forms.lock().unwrap();
        let pairs: BTreeMap<_, _> = url::form_urlencoded::parse(form[0].as_bytes())
            .into_owned()
            .collect();
        assert_eq!(pairs.get("refresh_token").unwrap(), "old-refresh");
        assert_eq!(pairs.get("grant_type").unwrap(), "refresh_token");
        assert_eq!(pairs.get("client_id").unwrap(), "native-client");
        assert_eq!(pairs.get("scope").unwrap(), "openid profile");
        assert!(!pairs.contains_key("resource"));
    }
}

#[derive(Clone, Copy, Debug)]
enum Change {
    Active,
    Provider,
    Profile,
    Token,
    Document,
    Aba,
}
fn competing(issuer: &RoleIssuer, ctx: &RequestContext<'_>, origin: &str, change: Change) {
    let store = issuer.profile_store();
    applied(
        store
            .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| {
                let p = doc.profile("a").unwrap();
                let provider = doc.provider("broker").unwrap().revision();
                let own = p.revision();
                let token = p.token_revision();
                match change {
                    Change::Active => doc.activate("b", doc.revision(), ctx),
                    Change::Aba => {
                        doc.activate("b", doc.revision(), ctx)?;
                        doc.activate("a", doc.revision(), ctx)
                    }
                    Change::Provider => doc.update_provider(
                        "broker",
                        broker_config(origin),
                        doc.revision(),
                        provider,
                        2,
                        ctx,
                    ),
                    Change::Profile => doc.replace_profile_data(
                        profile_data("a", origin, Some("competitor")),
                        ProfileExpectation {
                            document_revision: doc.revision(),
                            provider_revision: provider,
                            profile_revision: Some(own),
                        },
                        2,
                        ctx,
                    ),
                    Change::Token => doc.rotate_tokens(
                        "a",
                        TokenUpdate {
                            access_token: secret("competitor-access"),
                            refresh_token: secret("competitor-refresh"),
                            identity: ProfileIdentityUpdate::Preserve,
                            expires_at: 1,
                        },
                        TokenExpectation {
                            document_revision: doc.revision(),
                            provider_revision: provider,
                            profile_revision: own,
                            token_revision: token,
                        },
                        2,
                        ctx,
                    ),
                    Change::Document => {
                        doc.add_provider("unrelated".into(), config(), doc.revision(), 2, ctx)
                    }
                }
            })
            .unwrap(),
    );
}
#[tokio::test]
async fn six_changes_before_post_keep_original_four_cas_and_send_nothing() {
    competing_matrix(false).await;
}
#[tokio::test]
async fn six_changes_after_post_preserve_winner_and_remote_uncertainty() {
    competing_matrix(true).await;
}
async fn competing_matrix(after_post: bool) {
    for change in [
        Change::Active,
        Change::Provider,
        Change::Profile,
        Change::Token,
        Change::Document,
        Change::Aba,
    ] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        broker.response(false, 300, "native-subject", Some("new-refresh"));
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let snapshot = begin(&m, &parent);
        let path = if after_post {
            "/token"
        } else {
            "/.well-known/graphit-broker"
        };
        let gate = broker.gate(path);
        let mut winner = Vec::new();

        let (result, ()) = tokio::join!(m.refresh_broker(snapshot, &flow, 2), async {
            broker.wait(path).await;
            competing(&issuer, &ctx, broker.origin(), change);
            winner = saved(&f);
            gate.notify_one();
        });
        assert_denied(
            result.err().unwrap(),
            if after_post {
                RemoteEffect::MayHaveApplied
            } else {
                RemoteEffect::NotSent
            },
            &broker,
            usize::from(after_post),
            &f,
            &winner,
        );
    }
}

#[tokio::test]
async fn captured_runtime_role_authorizes_refresh_and_revoked_role_denies() {
    for revoked in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        broker.response(true, 300, "native-subject", Some("new-refresh"));
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
        let before = saved(&f);
        if revoked {
            issuer.revoke(&parent, &bearer).unwrap();
            assert_denied(
                m.refresh_broker(snapshot, &flow, 2).await.err().unwrap(),
                RemoteEffect::NotSent,
                &broker,
                0,
                &f,
                &before,
            );
        } else {
            applied(m.refresh_broker(snapshot, &flow, 2).await.unwrap());
            assert_eq!(broker.count("/token"), 1);
            assert_eq!(broker.count("/userinfo"), 0);
        }
    }
}

#[tokio::test]
async fn authority_expiry_or_cancellation_during_discovery_and_post_never_rebases() {
    for after_post in [false, true] {
        for cancelled in [false, true] {
            let cancel = Cancellation::default();
            let ctx = operation(&cancel);
            let f = Fixture::new(&ctx);
            let broker = BrokerFixture::new().await;
            let clock = clock();
            let issuer = issuer_for(
                &f,
                &ctx,
                clock.clone(),
                profile_policy(),
                &all_permissions(),
            );
            let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
            let verifier = broker.verifier(revision, broker.origin(), None);
            let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
            broker.response(false, 300, "native-subject", Some("new-refresh"));
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let session = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &session).unwrap();
            let m = manager(&issuer);
            let snapshot = begin(&m, &parent);
            let before = saved(&f);

            let path = if after_post {
                "/token"
            } else {
                "/.well-known/graphit-broker"
            };
            let gate = broker.gate(path);
            let (result, ()) = tokio::join!(m.refresh_broker(snapshot, &flow, 2), async {
                broker.wait(path).await;
                if cancelled {
                    cancel.cancel();
                } else {
                    clock.seconds.store(60, Ordering::SeqCst);
                }
                gate.notify_one();
            });
            assert_denied(
                result.err().unwrap(),
                if after_post {
                    RemoteEffect::MayHaveApplied
                } else {
                    RemoteEffect::NotSent
                },
                &broker,
                usize::from(after_post),
                &f,
                &before,
            );
        }
    }
}

#[tokio::test]
async fn original_deadline_expires_during_discovery_without_post_or_publication() {
    let cancel = Cancellation::default();
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
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    broker.response(false, 300, "native-subject", Some("new-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let snapshot = begin(&m, &parent);
    let before = saved(&f);

    let gate = broker.gate("/.well-known/graphit-broker");
    let (result, ()) = tokio::join!(m.refresh_broker(snapshot, &flow, 2), async {
        broker.wait("/.well-known/graphit-broker").await;
        tokio::time::sleep_until(tokio::time::Instant::from_std(ctx.deadline().unwrap())).await;
    });
    assert_denied(
        result.err().unwrap(),
        RemoteEffect::NotSent,
        &broker,
        0,
        &f,
        &before,
    );

    assert!(ctx.snapshot().check().is_err());
    gate.notify_one();
}

#[tokio::test]
async fn service_literal_binding_and_timestamp_fail_before_discovery() {
    for variant in 0..4 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let endpoint = if variant == 0 {
            format!("{}/", broker.origin())
        } else {
            broker.origin().into()
        };
        let resource = (variant == 1).then_some("https://workspace.example/mcp");
        let verifier = broker.verifier(
            if variant == 2 { revision + 1 } else { revision },
            &endpoint,
            resource,
        );
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let before = saved(&f);
        let error = m
            .refresh_broker(begin(&m, &parent), &flow, if variant == 3 { 0 } else { 2 })
            .await
            .err()
            .unwrap();
        assert_denied(error, RemoteEffect::NotSent, &broker, 0, &f, &before);
        assert_eq!(broker.count("/.well-known/graphit-broker"), 0);
        if variant != 3 {
            assert_eq!(error.kind(), BrokerRefreshErrorKind::ServiceMismatch);
        }
    }
}

#[tokio::test]
async fn missing_refresh_and_wrong_manager_never_send_or_publish() {
    for wrong_manager in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(
            &issuer,
            &ctx,
            broker.origin(),
            if wrong_manager {
                Some("old-refresh")
            } else {
                None
            },
        );
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let other = manager(&issuer);
        let snapshot = begin(&m, &parent);
        let before = saved(&f);
        let result = if wrong_manager {
            other.refresh_broker(snapshot, &flow, 2).await
        } else {
            m.refresh_broker(snapshot, &flow, 2).await
        };
        assert_denied(
            result.err().unwrap(),
            RemoteEffect::NotSent,
            &broker,
            0,
            &f,
            &before,
        );
    }
}

#[tokio::test]
async fn post_response_failures_keep_remote_effect_and_never_publish() {
    for variant in 0..6 {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        broker.response(
            false,
            300,
            if variant == 0 {
                "other-subject"
            } else {
                "native-subject"
            },
            if variant == 1 {
                None
            } else if variant == 2 {
                Some("old-refresh")
            } else {
                Some("new-refresh")
            },
        );
        if variant == 3 {
            *broker.state.response.lock().unwrap() = "{\"access_token\":\"canary\"}".into();
        }
        if variant == 4 {
            *broker.state.status.lock().unwrap() = 500;
        }
        if variant == 5 {
            broker.state.disconnect_token.store(true, Ordering::SeqCst);
        }
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let before = saved(&f);
        let error = m
            .refresh_broker(begin(&m, &parent), &flow, 2)
            .await
            .err()
            .unwrap();
        assert_denied(error, RemoteEffect::MayHaveApplied, &broker, 1, &f, &before);
        assert!(!format!("{error:?}").contains("canary"));
    }
}

#[tokio::test]
async fn final_authority_after_encoding_and_real_core_pending_cleanup_are_preserved() {
    for publication_started in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let clock = clock();
        let issuer = issuer_for(
            &f,
            &ctx,
            clock.clone(),
            profile_policy(),
            &all_permissions(),
        );
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        broker.response(true, 300, "native-subject", Some("new-refresh"));
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let snapshot = begin(&m, &parent);
        let before = saved(&f);
        if !publication_started {
            let observed = clock.clone();
            after_encoding(move || observed.seconds.store(60, Ordering::SeqCst));

            assert_denied(
                m.refresh_broker(snapshot, &flow, 2).await.err().unwrap(),
                RemoteEffect::MayHaveApplied,
                &broker,
                1,
                &f,
                &before,
            );
        } else {
            use graphit_core::auth_document::{AuthDocumentError, recovery::AuthCleanupEffect};
            let guard = issuer
                .profile_store()
                .cancel_publication_before_replace(&ctx)
                .unwrap();
            let publication = m.refresh_broker(snapshot, &flow, 2).await.unwrap();
            assert_eq!(publication.effect(), AuthPublicationEffect::NotApplied);
            assert_eq!(publication.failure(), Some(AuthDocumentError::Cancelled));
            assert_eq!(publication.cleanup_effect(), AuthCleanupEffect::NotApplied);
            assert_eq!(
                publication.cleanup_failure(),
                Some(AuthDocumentError::Cancelled)
            );
            let pending = publication
                .into_pending_cleanup()
                .expect("actual Core creation proof");
            assert_eq!(saved(&f), before);
            assert_eq!(broker.count("/token"), 1);
            assert!(
                f.installation
                    .owner_root()
                    .join(".auth.json.stage")
                    .exists()
            );
            drop(guard);
            assert!(ctx.snapshot().check().is_err());
            let cleanup_cancel = Cancellation::default();
            let cleanup = operation(&cleanup_cancel);
            let caller = issuer.authenticate_runtime(&f.secret(), &cleanup).unwrap();
            let session = issuer.bootstrap_admin(&caller).unwrap();
            let parent = issuer.admin_parent(&caller, &session).unwrap();
            let result = m
                .cleanup_pending(ProfileManagerAuthority::Admin(&parent), pending)
                .unwrap_or_else(|error| panic!("cleanup denied: {:?}", error.error()));
            assert_eq!(result.effect(), AuthCleanupEffect::Applied);
            assert_eq!(saved(&f), before);
            assert!(
                !f.installation
                    .owner_root()
                    .join(".auth.json.stage")
                    .exists()
            );
        }
    }
}

#[tokio::test]
async fn new_token_scalar_exact_and_one_over_are_admitted_before_own_copy() {
    for one_under in [false, true] {
        let cancel = Cancellation::default();
        let ctx = operation(&cancel);
        let f = Fixture::new(&ctx);
        let broker = BrokerFixture::new().await;
        let body = broker.response(false, 300, "native-subject", Some("new-refresh"));
        let bytes = body["access_token"].as_str().unwrap().len();
        let mut policy = profile_policy();
        policy.scalar_bytes = bytes - usize::from(one_under);
        let issuer = issuer_for(&f, &ctx, clock(), policy, &all_permissions());
        let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
        let verifier = broker.verifier(revision, broker.origin(), None);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let m = manager(&issuer);
        let before = saved(&f);
        let result = m.refresh_broker(begin(&m, &parent), &flow, 2).await;
        if one_under {
            assert_denied(
                result.err().unwrap(),
                RemoteEffect::MayHaveApplied,
                &broker,
                1,
                &f,
                &before,
            );
        } else {
            applied(result.unwrap());
            assert_eq!(broker.count("/token"), 1);
        }
    }
}

#[tokio::test]
async fn dropping_awaiting_refresh_preserves_original_document() {
    let cancel = Cancellation::default();
    let ctx = operation(&cancel);
    let f = Fixture::new(&ctx);
    let broker = BrokerFixture::new().await;
    let issuer = issuer_for(&f, &ctx, clock(), profile_policy(), &all_permissions());
    let revision = seed(&issuer, &ctx, broker.origin(), Some("old-refresh"));
    let verifier = broker.verifier(revision, broker.origin(), None);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    broker.response(false, 300, "native-subject", Some("new-refresh"));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    let m = manager(&issuer);
    let snapshot = begin(&m, &parent);
    let before = saved(&f);
    let gate = broker.gate("/token");

    let mut future = Box::pin(m.refresh_broker(snapshot, &flow, 2));
    tokio::select! { result=&mut future => { assert!(result.is_ok(),"request returned before fixture barrier");panic!("fixture barrier not reached"); }, ()=broker.wait("/token") => {} }

    // Drop the actual pinned future, not just a reference to it.
    drop(future);

    gate.notify_one();
    assert_eq!(saved(&f), before);
}
