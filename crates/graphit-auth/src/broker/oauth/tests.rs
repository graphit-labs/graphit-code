#[path = "device/tests.rs"]
mod device;
use super::*;
use crate::broker::tests::{
    context, key_set, limits as verifier_limits, network_limits, pair, token,
};
use crate::broker::{BrokerPolicy, Clock, Moment};
use aws_lc_rs::signature::Ed25519KeyPair;
use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
use graphit_http::client::ClientPolicy;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
    task::JoinHandle,
};

pub(crate) struct State {
    broker: Mutex<Value>,
    standard: Mutex<Value>,
    pub(crate) keys: Mutex<Value>,
    token: Mutex<String>,
    device_response: Mutex<String>,
    device_steps: Mutex<std::collections::VecDeque<(u16, String)>>,
    device_times: Mutex<Vec<Instant>>,
    disconnect_device: std::sync::atomic::AtomicBool,
    registration: Mutex<Value>,
    userinfo: Mutex<Value>,
    pub(crate) userinfo_status: Mutex<u16>,
    pub(crate) project_response: Mutex<(u16, String)>,
    status: Mutex<u16>,
    delay: Mutex<Duration>,
    token_gate: Mutex<Option<Arc<Notify>>>,
    counts: Mutex<BTreeMap<String, usize>>,
    forms: Mutex<Vec<(String, String)>>,
}
pub(crate) struct Fixture {
    pub(crate) origin: String,
    resource: &'static str,
    pub(crate) state: Arc<State>,
    task: JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    pub(crate) async fn new(key: &Ed25519KeyPair) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let resource = "https://workspace.example/mcp";
        let state = Arc::new(State {
            broker: Mutex::new(json!({"version":"1","issuer":origin,"authentication":{
                "type":"oidc","issuer":origin,"client_id":"native-client","scopes":["openid","profile"],
                "redirect_uri_path":"/callback","audiences":["native-api"],"access_token_audience":"native-api",
                "mcp_resources":[resource]}})),
            standard: Mutex::new(
                json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),
                "registration_endpoint":format!("{origin}/register"),"token_endpoint":format!("{origin}/token"),"revocation_endpoint":format!("{origin}/revoke"),
                "jwks_uri":format!("{origin}/keys"),"userinfo_endpoint":format!("{origin}/userinfo"),
                "grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],
                "token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]}),
            ),
            keys: Mutex::new(key_set(key, "one")),
            token: Mutex::new("{}".into()),
            device_response: Mutex::new(json!({"device_code":"gb_dc_device_secret_canary", "user_code":"ABCD-EFGH", "verification_uri":format!("{origin}/device"), "verification_uri_complete":format!("{origin}/device?user_code=ABCD-EFGH"),"expires_in":600,"interval":1}).to_string()),
            device_steps: Mutex::new(std::collections::VecDeque::new()),
            device_times: Mutex::new(Vec::new()),
            disconnect_device: std::sync::atomic::AtomicBool::new(false),
            registration: Mutex::new(json!({"client_id":"web-client"})),
            userinfo: Mutex::new(json!({"sub":"native-subject"})),
            userinfo_status: Mutex::new(200),
            project_response: Mutex::new((403, "{}".into())),
            status: Mutex::new(200),
            delay: Mutex::new(Duration::ZERO),
            token_gate: Mutex::new(None),
            counts: Mutex::new(BTreeMap::new()),
            forms: Mutex::new(vec![]),
        });
        let serve = state.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let state = serve.clone();
                tokio::spawn(async move {
                    let mut input = vec![];
                    let mut block = [0u8; 1024];
                    let end = loop {
                        if let Some(n) = input.windows(4).position(|w| w == b"\r\n\r\n") {
                            break n + 4;
                        }
                        let Ok(n) = stream.read(&mut block).await else {
                            return;
                        };
                        if n == 0 || input.len() + n > 65536 {
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
                        .to_owned();
                    let post = head.starts_with("POST ");
                    let len: usize = head
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                .map(|(_, v)| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    while input.len() < end + len {
                        let Ok(n) = stream.read(&mut block).await else {
                            return;
                        };
                        if n == 0 || input.len() + n > 65536 {
                            return;
                        }
                        input.extend_from_slice(&block[..n]);
                    }
                    let base = path.split('?').next().unwrap().to_owned();
                    *state
                        .counts
                        .lock()
                        .unwrap()
                        .entry(base.clone())
                        .or_default() += 1;
                    if post {
                        state.forms.lock().unwrap().push((
                            base.clone(),
                            String::from_utf8(input[end..end + len].to_vec()).unwrap(),
                        ));
                    }
                    if base == "/token"
                        && state
                            .disconnect_device
                            .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        return;
                    }
                    let (status, body, delay) = match base.as_str() {
                        "/device/authorize" => (
                            200,
                            state.device_response.lock().unwrap().clone(),
                            Duration::ZERO,
                        ),
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
                        "/keys" => (200, state.keys.lock().unwrap().to_string(), Duration::ZERO),
                        "/v1/project/access/authorize" => {
                            let (status, body) = state.project_response.lock().unwrap().clone();
                            (status, body, Duration::ZERO)
                        }
                        "/register" => (
                            201,
                            state.registration.lock().unwrap().to_string(),
                            Duration::ZERO,
                        ),
                        "/token" => {
                            state.device_times.lock().unwrap().push(Instant::now());
                            if let Some((status, body)) =
                                state.device_steps.lock().unwrap().pop_front()
                            {
                                (status, body, Duration::ZERO)
                            } else {
                                (
                                    *state.status.lock().unwrap(),
                                    state.token.lock().unwrap().clone(),
                                    *state.delay.lock().unwrap(),
                                )
                            }
                        }
                        "/revoke" => (
                            *state.status.lock().unwrap(),
                            "".into(),
                            *state.delay.lock().unwrap(),
                        ),
                        "/userinfo" => (
                            *state.userinfo_status.lock().unwrap(),
                            state.userinfo.lock().unwrap().to_string(),
                            Duration::ZERO,
                        ),
                        _ => (404, "{}".into(), Duration::ZERO),
                    };
                    let gate = if base == "/token" {
                        state.token_gate.lock().unwrap().clone()
                    } else {
                        None
                    };
                    if let Some(gate) = gate {
                        gate.notified().await;
                    }
                    tokio::time::sleep(delay).await;
                    let response = format!(
                        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        Self {
            origin,
            resource,
            state,
            task,
        }
    }
    fn verifier(&self, resource: bool) -> BrokerVerifier {
        BrokerVerifier::new(
            BrokerPolicy {
                endpoint: &self.origin,
                mcp_resource: resource.then_some(self.resource),
                provider_revision: 7,
                network: ClientPolicy {
                    origin: &self.origin,
                    allow_loopback_http: true,
                    proxy: None,
                    additional_roots: &[],
                },
            },
            network_limits(),
            verifier_limits(),
        )
        .unwrap()
    }
    pub(crate) fn count(&self, path: &str) -> usize {
        self.state
            .counts
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or(0)
    }
    async fn wait(&self, path: &str, n: usize) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.count(path) < n {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    fn response(
        &self,
        key: &Ed25519KeyPair,
        nonce: Option<&str>,
        refresh: Option<&str>,
        now: Moment,
    ) {
        let iat = now.utc.duration_since(UNIX_EPOCH).unwrap().as_secs();
        let access = json!({"iss":self.origin,"sub":"native-subject","client_id":"native-client",
            "token_use":"access","jti":"jwt-id","aud":["native-api",self.resource],"iat":iat,"exp":iat+3600});
        let mut body =
            json!({"access_token":token(key,"one",&access),"token_type":"bEaReR","expires_in":300});
        if let Some(nonce) = nonce {
            let id = json!({"iss":self.origin,"sub":"native-subject","aud":"native-client",
                "iat":iat,"exp":iat+3600,"nonce":nonce,"preferred_username":"verified-user"});
            body["id_token"] = token(key, "one", &id).into();
        }
        if let Some(refresh) = refresh {
            body["refresh_token"] = refresh.into();
        }
        *self.state.token.lock().unwrap() = body.to_string();
    }
}
fn flow_limits() -> OAuthLimits {
    OAuthLimits {
        authorization_url_bytes: 8192,
        callback_bytes: 8192,
        callback_query_entries: 8,
        form_body_bytes: 8192,
        token_bytes: 4096,
        response_nodes: 32,
        response_scalar_bytes: 8192,
    }
}
fn pairs(text: &str) -> BTreeMap<String, String> {
    form_urlencoded::parse(text.as_bytes())
        .into_owned()
        .collect()
}
fn callback(pending: &PendingLogin<'_>, code: &str) -> String {
    let mut url = Url::parse(&pending.redirect).unwrap();
    url.query_pairs_mut()
        .append_pair("code", code)
        .append_pair("state", &pending.state);
    url.to_string()
}
struct TestClock(Mutex<Moment>);
impl Clock for TestClock {
    fn now(&self) -> Moment {
        *self.0.lock().unwrap()
    }
}

#[tokio::test]
async fn sdk_pkce_login_api_only_resource_and_raw_default_port_forms() {
    for resource in [false, true] {
        let key = pair();
        let fixture = Fixture::new(&key).await;
        let verifier = fixture.verifier(resource);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        let original = context(&Cancellation::default());
        let ctx = BrokerRequestContext::new(&original);
        let pending = flow
            .begin("http://127.0.0.1:80/callback", ctx)
            .await
            .unwrap();
        let url = Url::parse(pending.authorization_url(ctx).unwrap()).unwrap();
        let fields = pairs(url.query().unwrap());
        assert_eq!(url.query_pairs().count(), if resource { 9 } else { 8 });
        assert_eq!(fields["client_id"], "native-client");
        assert_eq!(fields["response_type"], "code");
        assert_eq!(fields["redirect_uri"], "http://127.0.0.1:80/callback");
        assert_eq!(fields["scope"], "openid profile");
        assert_eq!(fields["code_challenge_method"], "S256");
        let challenge = PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(
            pending.verifier.to_string(),
        ));
        assert_eq!(fields["code_challenge"], challenge.as_str());
        assert_eq!(fields["state"], pending.state.as_str());
        assert_eq!(fields["nonce"], pending.nonce.as_str());
        assert_ne!(pending.state.as_str(), pending.nonce.as_str());
        assert_ne!(pending.verifier.as_str(), pending.nonce.as_str());
        assert_eq!(
            fields.get("resource").map(String::as_str),
            resource.then_some(fixture.resource)
        );
        fixture.response(
            &key,
            Some(&pending.nonce),
            Some("opaque-initial"),
            verifier.clock.now(),
        );
        let cb = callback(&pending, "unicode-ação");
        let expected_verifier = pending.verifier.to_string();
        let output = flow.exchange(pending, &cb, ctx).await.unwrap();
        assert!(output.access_token(ctx).unwrap().contains('.'));
        assert_eq!(output.refresh_token(ctx).unwrap(), Some("opaque-initial"));
        assert_eq!(
            output
                .identity(ctx)
                .unwrap()
                .profile_claims()
                .unwrap()
                .username(),
            "verified-user"
        );
        let form = pairs(&fixture.state.forms.lock().unwrap()[0].1);
        assert_eq!(form["grant_type"], "authorization_code");
        assert_eq!(form["code"], "unicode-ação");
        assert_eq!(form["code_verifier"], expected_verifier);
        assert_eq!(form["redirect_uri"], "http://127.0.0.1:80/callback");
        assert_eq!(
            form.get("resource").map(String::as_str),
            resource.then_some(fixture.resource)
        );
        assert_eq!(fixture.count("/token"), 1);
        assert_eq!(fixture.count("/keys"), 1);
        assert_eq!(fixture.count("/userinfo"), 0);
    }
}

#[tokio::test]
async fn callbacks_cross_origin_duplicates_state_unknown_replay_and_cancel_are_not_sent() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let cancel = Cancellation::default();
    let original = context(&cancel);
    let ctx = BrokerRequestContext::new(&original);
    let cases = [
        "badstate",
        "duplicate",
        "encodedduplicate",
        "unknown",
        "wrongpath",
        "foreign",
        "fragment",
        "both",
        "missingstate",
        "fresh",
        "description",
    ];
    for case in cases {
        let pending = flow
            .begin("http://[::1]:45001/callback", ctx)
            .await
            .unwrap();
        let mut cb = callback(&pending, "code");
        match case {
            "badstate" => cb = cb.replace(pending.state.as_str(), "wrong"),
            "duplicate" => cb.push_str("&code=again"),
            "encodedduplicate" => cb.push_str("&%73tate=again"),
            "unknown" => cb.push_str("&extra=x"),
            "wrongpath" => cb = cb.replace("/callback", "/other"),
            "foreign" => cb = cb.replace("[::1]", "[::2]"),
            "fragment" => cb.push_str("#canary"),
            "both" => cb.push_str("&error=access_denied"),
            "missingstate" => cb = "http://[::1]:45001/callback?code=x".into(),
            "description" => cb.push_str("&error_description=canary"),
            _ => {}
        }
        let fresh = context(&cancel);
        let used = if case == "fresh" {
            BrokerRequestContext::new(&fresh)
        } else {
            ctx
        };
        let error = flow.exchange(pending, &cb, used).await.err().unwrap();
        assert_eq!(error.effect(), RemoteEffect::NotSent, "{case}");
        assert!(!error.to_string().contains("canary"));
    }
    let pending = flow
        .begin("http://127.0.0.1:45001/callback", ctx)
        .await
        .unwrap();
    let error_cb = format!(
        "{}?state={}&error=access_denied&error_description=canary&error_uri=https%3A%2F%2Fcanary",
        pending.redirect,
        pending.state.as_str()
    );
    let error = flow.exchange(pending, &error_cb, ctx).await.err().unwrap();
    assert_eq!(error.kind(), NativeErrorKind::AuthorizationRejected);
    assert_eq!(error.effect(), RemoteEffect::NotSent);
    let pending = flow
        .begin("http://127.0.0.1:45001/callback", ctx)
        .await
        .unwrap();
    let old_cb = callback(&pending, "code");
    drop(pending);
    let new_pending = flow
        .begin("http://127.0.0.1:45001/callback", ctx)
        .await
        .unwrap();
    assert_eq!(
        flow.exchange(new_pending, &old_cb, ctx)
            .await
            .err()
            .unwrap()
            .effect(),
        RemoteEffect::NotSent
    );
    let pending = flow
        .begin("http://127.0.0.1:45001/callback", ctx)
        .await
        .unwrap();
    let cb = callback(&pending, "code");
    cancel.cancel();
    assert_eq!(
        flow.exchange(pending, &cb, ctx)
            .await
            .err()
            .unwrap()
            .effect(),
        RemoteEffect::NotSent
    );
    assert_eq!(fixture.count("/token"), 0);
}

#[tokio::test]
async fn context_partition_and_redirect_vendor_rules() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    for redirect in [
        "http://localhost:50/callback",
        "http://127.1:50/callback",
        "http://127.0.0.1/callback",
        "http://127.0.0.1:0/callback",
        "https://127.0.0.1:50/callback",
        "http://u@127.0.0.1:50/callback",
        "http://127.0.0.1:50/callback?x=1",
        "http://127.0.0.1:50/callback#x",
        "http://127.0.0.1:50/other",
    ] {
        assert!(flow.begin(redirect, ctx).await.is_err(), "{redirect}");
    }
    let pending = flow
        .begin("http://127.0.0.1:50/callback", ctx)
        .await
        .unwrap();
    assert!(
        flow.begin("http://127.0.0.1:50/callback", ctx)
            .await
            .is_ok()
    );
    assert!(
        pending
            .authorization_url(BrokerRequestContext::new(&original))
            .is_ok()
    );
    let other = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    assert!(pending.check(&other, ctx).is_err());
    drop(pending);
    assert!(flow.begin("http://[::1]:50/callback", ctx).await.is_ok());
    assert_eq!(fixture.count("/token"), 0);
}

#[tokio::test]
async fn refresh_rotates_with_optional_id_and_rejects_same_absent_subject_or_bad_jwt_after_send() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(true);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    fixture.response(&key, None, Some("new-refresh"), verifier.clock.now());
    let result = flow
        .refresh("old-refresh", "native-subject", ctx)
        .await
        .unwrap();
    assert_eq!(result.refresh_token(ctx).unwrap(), Some("new-refresh"));
    assert!(result.id_token(ctx).unwrap().is_none());
    assert!(result.identity(ctx).unwrap().profile_claims().is_none());
    fixture.response(
        &key,
        Some("nonce-not-required-on-refresh"),
        Some("newer-refresh"),
        verifier.clock.now(),
    );
    let result = flow
        .refresh("new-refresh", "native-subject", ctx)
        .await
        .unwrap();
    assert!(result.id_token(ctx).unwrap().is_some());
    assert!(result.identity(ctx).unwrap().profile_claims().is_some());
    for (next, subject) in [
        (Some("old-refresh"), "native-subject"),
        (None, "native-subject"),
        (Some("new-refresh"), "wrong-subject"),
    ] {
        fixture.response(&key, None, next, verifier.clock.now());
        let error = flow
            .refresh("old-refresh", subject, ctx)
            .await
            .err()
            .unwrap();
        assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
    }
    let form = pairs(&fixture.state.forms.lock().unwrap()[0].1);
    assert_eq!(form["grant_type"], "refresh_token");
    assert_eq!(form["scope"], "openid profile");
    assert_eq!(form["resource"], fixture.resource);
    assert_eq!(form["client_id"], "native-client");
    assert_eq!(fixture.count("/token"), 5);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn revocation_endpoint_advertised_optional_and_foreign_or_null_are_denied() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    let verifier = fixture.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    flow.revoke("opaque-refresh", ctx).await.unwrap();
    let form = pairs(&fixture.state.forms.lock().unwrap()[0].1);
    assert_eq!(form["token"], "opaque-refresh");
    assert_eq!(form["token_type_hint"], "refresh_token");
    assert_eq!(form["client_id"], "native-client");
    assert_eq!(fixture.count("/revoke"), 1);
    *fixture.state.status.lock().unwrap() = 503;
    assert_eq!(
        flow.revoke("opaque-refresh", ctx)
            .await
            .err()
            .unwrap()
            .effect(),
        RemoteEffect::MayHaveApplied
    );
    fixture
        .state
        .standard
        .lock()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("revocation_endpoint");
    let verifier = fixture.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let error = flow.revoke("opaque-refresh", ctx).await.err().unwrap();
    assert_eq!(error.kind(), NativeErrorKind::Unsupported);
    assert_eq!(error.effect(), RemoteEffect::NotSent);
    for value in [
        Value::Null,
        json!(""),
        json!(2),
        json!("https://foreign.example/revoke"),
    ] {
        fixture.state.standard.lock().unwrap()["revocation_endpoint"] = value;
        let verifier = fixture.verifier(false);
        let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
        assert_eq!(
            flow.revoke("opaque-refresh", ctx)
                .await
                .err()
                .unwrap()
                .effect(),
            RemoteEffect::NotSent
        );
    }
    assert_eq!(fixture.count("/revoke"), 2);
}

#[tokio::test]
async fn reserved_endpoint_parameters_never_create_pending_or_post() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    for endpoint in [
        "authorization_endpoint",
        "token_endpoint",
        "revocation_endpoint",
    ] {
        for name in [
            "client_id",
            "redirect_uri",
            "response_type",
            "scope",
            "state",
            "nonce",
            "code_challenge",
            "code_challenge_method",
            "resource",
            "grant_type",
            "code",
            "code_verifier",
            "refresh_token",
            "token",
            "token_type_hint",
            "%63lient_id",
        ] {
            let saved = fixture.state.standard.lock().unwrap()[endpoint].clone();
            fixture.state.standard.lock().unwrap()[endpoint] =
                format!("{}?{name}=canary", saved.as_str().unwrap()).into();
            let verifier = fixture.verifier(false);
            let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
            let error = flow
                .begin("http://127.0.0.1:45001/callback", ctx)
                .await
                .err()
                .unwrap();
            assert_eq!(error.effect(), RemoteEffect::NotSent);
            assert!(!error.to_string().contains("canary"));
            fixture.state.standard.lock().unwrap()[endpoint] = saved;
        }
    }
    assert_eq!(fixture.count("/token"), 0);
    assert_eq!(fixture.count("/revoke"), 0);
}

#[tokio::test]
async fn closed_response_adversarial_matrix_is_may_have_applied_and_no_credential_canary() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    for body in [
        r#"{"access_token":"canary","token_type":"Bearer","expires_in":1,"refresh_token":"new","unknown":[]}"#,
        r#"{"access_token":"canary","access_\u0074oken":"duplicate","token_type":"Bearer","expires_in":1}"#,
        r#"{"access_token":null,"token_type":"Bearer","expires_in":1}"#,
        r#"{"access_token":"canary","token_type":"Basic","expires_in":1}"#,
        r#"{"access_token":"canary","token_type":"Bearer","expires_in":0}"#,
        r#"{"access_token":"canary","token_type":"Bearer","expires_in":1.5}"#,
        r#"{"access_token":"canary","token_type":"Bearer","expires_in":-1}"#,
        r#"{"access_token":"canary","token_type":"Bearer","expires_in":1} {}"#,
        r#"{"access_token":"canary","token_type":"Bearer","expires_in":1,"id_token":null}"#,
        r#"{"access_token":[],"token_type":"Bearer","expires_in":1}"#,
    ] {
        *fixture.state.token.lock().unwrap() = body.into();
        let error = flow
            .refresh("old", "native-subject", ctx)
            .await
            .err()
            .unwrap();
        assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
        assert!(!error.to_string().contains("canary"));
    }
    assert_eq!(fixture.count("/token"), 10);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn initial_id_required_refresh_optional_and_wrong_nonce_is_remote_failure() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    for nonce_mode in [0, 1, 2] {
        let pending = flow
            .begin("http://127.0.0.1:45001/callback", ctx)
            .await
            .unwrap();
        let nonce = match nonce_mode {
            0 => None,
            1 => Some("wrong"),
            _ => Some(pending.nonce.as_str()),
        };
        fixture.response(&key, nonce, None, verifier.clock.now());
        let cb = callback(&pending, "code");
        let result = flow.exchange(pending, &cb, ctx).await;
        if nonce_mode == 2 {
            assert!(result.unwrap().refresh_token(ctx).unwrap().is_none());
        } else {
            assert_eq!(result.err().unwrap().effect(), RemoteEffect::MayHaveApplied);
        }
    }
    assert_eq!(fixture.count("/token"), 3);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[tokio::test]
async fn timeout_cancel_after_post_and_drop_without_retry() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let cancel = Cancellation::default();
    let original = context(&cancel);
    let ctx = BrokerRequestContext::new(&original);
    let pending = flow
        .begin("http://127.0.0.1:45001/callback", ctx)
        .await
        .unwrap();
    let cb = callback(&pending, "code");
    fixture.response(
        &key,
        Some(&pending.nonce),
        Some("new"),
        verifier.clock.now(),
    );
    *fixture.state.delay.lock().unwrap() = Duration::from_secs(5);
    let (result, ()) = tokio::join!(flow.exchange(pending, &cb, ctx), async {
        fixture.wait("/token", 1).await;
        cancel.cancel();
    });
    let error = result.err().unwrap();
    assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
    assert_eq!(
        error.kind(),
        NativeErrorKind::Broker(BrokerError::Cancelled)
    );
    let fresh = context(&Cancellation::default());
    let fresh_ctx = BrokerRequestContext::new(&fresh);
    let pending = flow
        .begin("http://127.0.0.1:45001/callback", fresh_ctx)
        .await
        .unwrap();
    let cb = callback(&pending, "code");
    let mut operation = Box::pin(flow.exchange(pending, &cb, fresh_ctx));
    tokio::select! { _ = fixture.wait("/token",2) => {}, _ = &mut operation => panic!("unexpected completion") }
    drop(operation);
    assert!(
        flow.begin("http://127.0.0.1:45001/callback", fresh_ctx)
            .await
            .is_ok()
    );
    let short = SnapshotContext::new(
        Instant::now() + Duration::from_millis(50),
        Cancellation::default(),
    );
    let error = flow
        .revoke("opaque", BrokerRequestContext::new(&short))
        .await
        .err()
        .unwrap();
    assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
    assert_eq!(error.kind(), NativeErrorKind::Broker(BrokerError::Deadline));
    assert_eq!(fixture.count("/token"), 2);
    assert_eq!(fixture.count("/revoke"), 1);
}

#[tokio::test]
async fn retained_output_context_service_and_response_lifetime_are_checked_without_renewal() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let mut verifier = fixture.verifier(false);
    let anchor = Moment {
        utc: SystemTime::now(),
        mono: Instant::now(),
    };
    let clock = Arc::new(TestClock(Mutex::new(anchor)));
    verifier.clock = clock.clone();
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    fixture.response(&key, None, Some("new"), anchor);
    let output = flow.refresh("old", "native-subject", ctx).await.unwrap();
    assert!(output.utc <= anchor.utc + Duration::from_secs(300));
    assert_eq!(
        output.expires_at_micros(ctx).unwrap(),
        i64::try_from(output.utc.duration_since(UNIX_EPOCH).unwrap().as_micros()).unwrap()
    );
    assert!(
        output
            .access_token(BrokerRequestContext::new(&original))
            .is_ok()
    );
    let fresh = context(&Cancellation::default());
    assert!(
        output
            .access_token(BrokerRequestContext::new(&fresh))
            .is_err()
    );
    let other = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    assert!(output.check(&other, ctx).is_err());
    *clock.0.lock().unwrap() = Moment {
        utc: anchor.utc - Duration::from_secs(60),
        mono: anchor.mono + Duration::from_secs(1),
    };
    assert!(output.access_token(ctx).is_ok());
    *clock.0.lock().unwrap() = Moment {
        utc: anchor.utc - Duration::from_secs(60),
        mono: anchor.mono + Duration::from_secs(300),
    };
    assert_eq!(
        output.access_token(ctx).err().unwrap().effect(),
        RemoteEffect::MayHaveApplied
    );
}

#[tokio::test]
async fn pre_post_anchor_clamp_and_checked_expiration_overflow() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let mut verifier = fixture.verifier(false);
    let anchor = Moment {
        utc: SystemTime::now(),
        mono: Instant::now(),
    };
    let clock = Arc::new(TestClock(Mutex::new(anchor)));
    verifier.clock = clock.clone();
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    fixture.response(&key, None, Some("new"), anchor);
    let mut value: Value = serde_json::from_str(&fixture.state.token.lock().unwrap()).unwrap();
    value["expires_in"] = u64::MAX.into();
    *fixture.state.token.lock().unwrap() = value.to_string();
    assert_eq!(
        flow.refresh("old", "native-subject", ctx)
            .await
            .err()
            .unwrap()
            .effect(),
        RemoteEffect::MayHaveApplied
    );
    fixture.response(&key, None, Some("new"), anchor);
    *fixture.state.delay.lock().unwrap() = Duration::from_millis(30);
    let (result, ()) = tokio::join!(flow.refresh("old", "native-subject", ctx), async {
        fixture.wait("/token", 2).await;
        *clock.0.lock().unwrap() = Moment {
            utc: anchor.utc,
            mono: anchor.mono + Duration::from_secs(301),
        };
    });
    assert_eq!(result.err().unwrap().effect(), RemoteEffect::MayHaveApplied);
}

#[tokio::test]
async fn short_jwt_pre_post_monotonic_anchor_wins_during_response_utc_rollback() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let mut verifier = fixture.verifier(false);
    let epoch_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let anchor = Moment {
        utc: UNIX_EPOCH + Duration::from_secs(epoch_seconds),
        mono: Instant::now(),
    };
    let clock = Arc::new(TestClock(Mutex::new(anchor)));
    verifier.clock = clock.clone();
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    let claims = json!({"iss":fixture.origin,"sub":"native-subject","client_id":"native-client",
        "token_use":"access","jti":"short-jwt","aud":["native-api",fixture.resource],
        "iat":epoch_seconds-60,"exp":epoch_seconds+120});
    *fixture.state.token.lock().unwrap() = json!({
        "access_token":token(&key,"one",&claims),"token_type":"Bearer",
        "expires_in":600,"refresh_token":"rotated"
    })
    .to_string();
    let gate = Arc::new(Notify::new());
    *fixture.state.token_gate.lock().unwrap() = Some(gate.clone());
    let (result, ()) = tokio::join!(flow.refresh("old", "native-subject", ctx), async {
        fixture.wait("/token", 1).await;
        *clock.0.lock().unwrap() = Moment {
            utc: anchor.utc - Duration::from_secs(60),
            mono: anchor.mono + Duration::from_secs(10),
        };
        gate.notify_one();
    });
    let output = result.unwrap();
    let deadline = anchor.mono + Duration::from_secs(120);
    assert_eq!(output.utc, anchor.utc + Duration::from_secs(120));
    assert_eq!(output.mono, deadline);
    // The other two candidates are later. Removing the pre-POST JWT minimum
    // therefore breaks this exact assertion rather than passing via another cap.
    assert!(output.identity.expires_mono > deadline);
    assert!(anchor.mono + Duration::from_secs(600) > deadline);
    assert_eq!(
        output.expires_at_micros(ctx).unwrap(),
        i64::try_from((epoch_seconds + 120) as u128 * 1_000_000).unwrap()
    );
    *clock.0.lock().unwrap() = Moment {
        utc: anchor.utc - Duration::from_secs(60),
        mono: deadline - Duration::from_nanos(1),
    };
    assert!(output.access_token(ctx).is_ok());
    *clock.0.lock().unwrap() = Moment {
        utc: anchor.utc - Duration::from_secs(60),
        mono: deadline,
    };
    let error = output.access_token(ctx).err().unwrap();
    assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
    assert_eq!(
        error.kind(),
        NativeErrorKind::Broker(BrokerError::InvalidToken)
    );
    assert_eq!(fixture.count("/token"), 1);
    assert_eq!(fixture.count("/userinfo"), 0);
}

#[test]
fn closed_codec_exact_nodes_tokens_scalars_and_zero_overflow_limits() {
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    let body = br#"{"access_token":"abc","token_type":"Bearer","expires_in":1}"#;
    let limits = OAuthLimits {
        response_nodes: 7,
        response_scalar_bytes: 12,
        token_bytes: 3,
        ..flow_limits()
    };
    assert!(codec::parse(body, limits, ctx).is_ok());
    assert!(
        codec::parse(
            body,
            OAuthLimits {
                response_nodes: 6,
                ..limits
            },
            ctx
        )
        .is_err()
    );
    assert!(
        codec::parse(
            body,
            OAuthLimits {
                response_scalar_bytes: 11,
                ..limits
            },
            ctx
        )
        .is_err()
    );
    assert!(
        codec::parse(
            body,
            OAuthLimits {
                token_bytes: 2,
                ..limits
            },
            ctx
        )
        .is_err()
    );
    let mut fields = [flow_limits(); 7];
    fields[0].authorization_url_bytes = 0;
    fields[1].callback_bytes = 0;
    fields[2].callback_query_entries = 0;
    fields[3].form_body_bytes = 0;
    fields[4].token_bytes = 0;
    fields[5].response_nodes = 0;
    fields[6].response_scalar_bytes = 0;
    for limits in fields {
        assert!(limits.validate().is_err());
    }
    assert!(encoded_bound([("x", usize::MAX)].into_iter(), usize::MAX).is_err());
}

#[tokio::test]
async fn callback_and_form_unicode_exact_one_under_query_and_authorization_caps() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    let exact = encoded_bound([("code", "ação".len())].into_iter(), 8192).unwrap();
    let flow = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            form_body_bytes: exact,
            ..flow_limits()
        },
    )
    .unwrap();
    assert_eq!(
        pairs(&flow.client.form(&[("code", "ação")], ctx).unwrap())["code"],
        "ação"
    );
    let under = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            form_body_bytes: exact - 1,
            ..flow_limits()
        },
    )
    .unwrap();
    assert!(under.client.form(&[("code", "ação")], ctx).is_err());
    let flow = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            callback_query_entries: 2,
            ..flow_limits()
        },
    )
    .unwrap();
    let pending = flow
        .begin("http://127.0.0.1:45001/callback", ctx)
        .await
        .unwrap();
    let cb = callback(&pending, "unicode-ação");
    assert!(
        flow.client
            .callback(&pending.redirect, &pending.state, &cb, ctx)
            .is_ok()
    );
    assert!(
        flow.client
            .callback(
                &pending.redirect,
                &pending.state,
                &format!("{cb}&code=x"),
                ctx
            )
            .is_err()
    );
    let under = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            callback_query_entries: 1,
            ..flow_limits()
        },
    )
    .unwrap();
    assert!(
        under
            .client
            .callback(&pending.redirect, &pending.state, &cb, ctx)
            .is_err()
    );
    let exact_callback = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            callback_bytes: cb.len(),
            ..flow_limits()
        },
    )
    .unwrap();
    assert!(
        exact_callback
            .client
            .callback(&pending.redirect, &pending.state, &cb, ctx)
            .is_ok()
    );
    let under = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            callback_bytes: cb.len() - 1,
            ..flow_limits()
        },
    )
    .unwrap();
    assert!(
        under
            .client
            .callback(&pending.redirect, &pending.state, &cb, ctx)
            .is_err()
    );
    let tiny = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            authorization_url_bytes: 1,
            ..flow_limits()
        },
    )
    .unwrap();
    assert!(
        tiny.begin("http://127.0.0.1:45001/callback", ctx)
            .await
            .is_err()
    );
    assert_eq!(fixture.count("/token"), 0);
}

#[tokio::test]
async fn authorization_conservative_exact_boundary_and_nonreserved_query_preservation() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    fixture.state.standard.lock().unwrap()["authorization_endpoint"] =
        format!("{}/authorize?tenant=isolated", fixture.origin).into();
    fixture.state.standard.lock().unwrap()["token_endpoint"] =
        format!("{}/token?tenant=isolated", fixture.origin).into();
    let verifier = fixture.verifier(false);
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    let redirect = "http://127.0.0.1:45001/callback";
    let metadata = verifier.metadata(ctx).await.unwrap();
    let bound = metadata.authorization_endpoint().len()
        + 1
        + encoded_bound(
            [
                ("response_type", 4),
                ("client_id", metadata.client_id().len()),
                ("state", 43),
                ("code_challenge", 43),
                ("code_challenge_method", 4),
                ("redirect_uri", redirect.len()),
                ("scope", "openid profile".len()),
                ("nonce", 43),
            ]
            .into_iter(),
            8192,
        )
        .unwrap();
    let exact = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            authorization_url_bytes: bound,
            ..flow_limits()
        },
    )
    .unwrap();
    let pending = exact.begin(redirect, ctx).await.unwrap();
    let url = Url::parse(pending.authorization_url(ctx).unwrap()).unwrap();
    assert_eq!(pairs(url.query().unwrap())["tenant"], "isolated");
    fixture.response(
        &key,
        Some(&pending.nonce),
        Some("new"),
        verifier.clock.now(),
    );
    let cb = callback(&pending, "code");
    exact.exchange(pending, &cb, ctx).await.unwrap();
    let under = NativeOAuth::new(
        &verifier,
        OAuthLimits {
            authorization_url_bytes: bound - 1,
            ..flow_limits()
        },
    )
    .unwrap();
    assert_eq!(
        under.begin(redirect, ctx).await.err().unwrap().kind(),
        NativeErrorKind::Limit
    );
    assert_eq!(fixture.count("/token"), 1);
}

#[tokio::test]
async fn verifier_expected_subject_exact_and_plus_one_admission_precedes_remote_rotation() {
    let key = pair();
    let fixture = Fixture::new(&key).await;
    let verifier = fixture.verifier(false);
    let flow = NativeOAuth::new(&verifier, flow_limits()).unwrap();
    let original = context(&Cancellation::default());
    let ctx = BrokerRequestContext::new(&original);
    let cap = verifier.limits.json.scalar_bytes;
    let over = "s".repeat(cap + 1);
    let error = flow.refresh("old", &over, ctx).await.err().unwrap();
    assert_eq!(error.effect(), RemoteEffect::NotSent);
    assert_eq!(error.kind(), NativeErrorKind::Broker(BrokerError::Limit));
    assert_eq!(fixture.count("/.well-known/graphit-broker"), 0);
    assert_eq!(fixture.count("/token"), 0);
    let exact = "s".repeat(cap);
    fixture.response(&key, None, Some("new"), verifier.clock.now());
    let error = flow.refresh("old", &exact, ctx).await.err().unwrap();
    assert_eq!(error.effect(), RemoteEffect::MayHaveApplied);
    assert_eq!(
        error.kind(),
        NativeErrorKind::Broker(BrokerError::InvalidToken)
    );
    assert_eq!(fixture.count("/token"), 1);
}

#[path = "web/tests.rs"]
mod web;
