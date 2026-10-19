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
use serde_json::Value;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;
pub(super) struct State {
    pub(super) origin: String,
    pub(super) key: Ed25519KeyPair,
    pub(super) flow: Mutex<Option<BTreeMap<String, String>>>,
    pub(super) invalid: Mutex<Option<&'static str>>,
    pub(super) rotation: AtomicUsize,
    pub(super) pause: AtomicBool,
    pub(super) device: AtomicBool,
    pub(super) started: Notify,
    pub(super) release: Notify,
}
pub(super) struct Broker {
    pub(super) state: Arc<State>,
    pub(super) server: tokio::task::JoinHandle<()>,
    pub(super) stop: graphit_core::file_lock::Cancellation,
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.stop.cancel();
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
    Json(
        json!({"version":"1","issuer":s.origin,"authentication":{"type":"oidc","issuer":s.origin,"client_id":"native-client","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["native-api"],"access_token_audience":"native-api","mcp_resources":[]}}),
    )
}
async fn discovery(AxumState(s): AxumState<Arc<State>>) -> Json<Value> {
    let mut document = json!({"issuer":s.origin,"authorization_endpoint":format!("{}/authorize",s.origin),"token_endpoint":format!("{}/token",s.origin),"jwks_uri":format!("{}/keys",s.origin),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]});
    if s.device.load(Ordering::SeqCst) {
        document["device_authorization_endpoint"] = format!("{}/device/authorize", s.origin).into();
        document["grant_types_supported"]
            .as_array_mut()
            .unwrap()
            .push("urn:ietf:params:oauth:grant-type:device_code".into());
    }
    Json(document)
}
async fn keys(AxumState(s): AxumState<Arc<State>>) -> Json<Value> {
    Json(
        json!({"keys":[{"kty":"OKP","crv":"Ed25519","kid":"one","alg":"EdDSA","use":"sig","key_ops":["verify"],"x":URL_SAFE_NO_PAD.encode(s.key.public_key().as_ref())}]}),
    )
}
async fn device_authorize(AxumState(s): AxumState<Arc<State>>, body: String) -> Response {
    let form: Vec<(String, String)> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    if !s.device.load(Ordering::SeqCst)
        || form != vec![("client_id".into(), "native-client".into())]
    {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_request"})),
        )
            .into_response();
    }
    Json(json!({"device_code":"device-private-canary","user_code":"ABCD-EFGH","verification_uri":format!("{}/device",s.origin),"verification_uri_complete":format!("{}/device?user_code=ABCD-EFGH",s.origin),"expires_in":60,"interval":1})).into_response()
}
async fn token(AxumState(s): AxumState<Arc<State>>, body: String) -> Response {
    let form: BTreeMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let grant = form.get("grant_type").map(String::as_str);
    if grant == Some("urn:ietf:params:oauth:grant-type:device_code") {
        let expected = BTreeMap::from([
            (
                "grant_type".into(),
                "urn:ietf:params:oauth:grant-type:device_code".into(),
            ),
            ("client_id".into(), "native-client".into()),
            ("device_code".into(), "device-private-canary".into()),
        ]);
        if !s.device.load(Ordering::SeqCst) || form != expected {
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
        let access = json!({"iss":s.origin,"sub":"native-subject","aud":["native-api"],"client_id":"native-client","token_use":"access","jti":format!("device-token-{serial}"),"iat":now,"exp":now+3600});
        return Json(json!({"access_token":signed(&s.key,access),"token_type":"Bearer","expires_in":300,"scope":""})).into_response();
    }
    let flow = s.flow.lock().unwrap().clone().unwrap();
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
    pub(super) async fn new() -> Self {
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
            device: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
        });
        let router = axum::Router::new()
            .route("/.well-known/graphit-broker", get(metadata))
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/keys", get(keys))
            .route("/token", post(token))
            .route("/device/authorize", post(device_authorize))
            .with_state(state.clone());
        let stop = graphit_core::file_lock::Cancellation::default();
        let signal = stop.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move { signal.cancelled().await })
                .await
                .unwrap();
        });
        Self {
            state,
            server,
            stop,
        }
    }
    pub(super) fn origin(&self) -> &str {
        &self.state.origin
    }
    pub(super) fn callback(&self, authorization: &str) -> url::Url {
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
}
