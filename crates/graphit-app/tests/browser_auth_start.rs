use aws_lc_rs::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use axum::{
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_app::browser_auth::{BrowserAuth, BrowserPolicy};
use graphit_auth::broker::{
    BrokerPolicy, BrokerVerifier, JsonLimits, VerifierLimits, oauth::OAuthLimits,
};
use graphit_http::client::{ClientPolicy, Limits};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Notify, task::JoinHandle};
use tokio_util::sync::CancellationToken;

const PRIVATE: &str = "browser-start-private-url-header-body-token-canary";

#[derive(Clone, Copy)]
enum Mode {
    InvalidDiscovery,
    RejectedRegistration,
    InvalidAuthorization,
    HeldDiscovery,
    Success,
}

struct Fixture {
    service: BrowserAuth,
    origin: String,
    client: reqwest::Client,
    stop: CancellationToken,
    server_stop: CancellationToken,
    entered: Arc<Notify>,
    servers: Vec<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
        self.server_stop.cancel();
        for server in &self.servers {
            server.abort();
        }
    }
}
impl Fixture {
    async fn new(mode: Mode, login_timeout: Duration) -> Self {
        let stop = CancellationToken::new();
        let server_stop = CancellationToken::new();
        let entered = Arc::new(Notify::new());
        let broker = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", broker.local_addr().unwrap());
        let key = Ed25519KeyPair::from_pkcs8(
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .unwrap()
                .as_ref(),
        )
        .unwrap();
        let keys = json!({"keys":[{"kty":"OKP","crv":"Ed25519","kid":"one","alg":"EdDSA","use":"sig","key_ops":["verify"],"x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref())}]});
        let metadata = if matches!(mode, Mode::InvalidDiscovery) {
            json!({"version":"1","issuer":PRIVATE})
        } else {
            json!({"version":"1","issuer":issuer,"authentication":{"type":"oidc","issuer":issuer,"client_id":"native-client","scopes":["openid","profile"],"redirect_uri_path":"/callback","audiences":["native-api"],"access_token_audience":"native-api","mcp_resources":[]}})
        };
        let authorize = if matches!(mode, Mode::InvalidAuthorization) {
            format!("{issuer}/authorize?state={PRIVATE}")
        } else {
            format!("{issuer}/authorize")
        };
        let oidc = json!({"issuer":issuer,"authorization_endpoint":authorize,"token_endpoint":format!("{issuer}/token"),"registration_endpoint":format!("{issuer}/register"),"jwks_uri":format!("{issuer}/keys"),"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"id_token_signing_alg_values_supported":["EdDSA"]});
        let arrived = Arc::clone(&entered);
        let cancelled = server_stop.clone();
        let router = Router::new()
            .route("/.well-known/graphit-broker", get(move || {
                let metadata = metadata.clone(); let arrived = Arc::clone(&arrived); let cancelled = cancelled.clone();
                async move {
                    arrived.notify_one();
                    if matches!(mode, Mode::HeldDiscovery) { cancelled.cancelled().await; }
                    Json(metadata)
                }
            }))
            .route("/.well-known/openid-configuration", get(move || { let oidc=oidc.clone(); async move { Json(oidc) } }))
            .route("/keys", get(move || { let keys=keys.clone(); async move { Json(keys) } }))
            .route("/register", post(move |Json(input): Json<Value>| async move {
                if matches!(mode, Mode::RejectedRegistration) {
                    return (StatusCode::BAD_REQUEST, Json(json!({"error":PRIVATE,"access_token":PRIVATE}))).into_response();
                }
                (StatusCode::CREATED, Json(json!({"client_id":"browser-client","application_type":input["application_type"],"redirect_uris":input["redirect_uris"],"token_endpoint_auth_method":"none"}))).into_response()
            }));
        let stopping = server_stop.clone();
        let first = tokio::spawn(async move {
            axum::serve(broker, router)
                .with_graceful_shutdown(stopping.cancelled_owned())
                .await
                .unwrap();
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let verifier = BrokerVerifier::new(
            BrokerPolicy {
                endpoint: &issuer,
                mcp_resource: None,
                provider_revision: 1,
                network: ClientPolicy {
                    origin: &issuer,
                    allow_loopback_http: true,
                    proxy: None,
                    additional_roots: &[],
                },
            },
            Limits {
                url_bytes: 8192,
                request_body_bytes: 8192,
                response_body_bytes: 32768,
                max_in_flight: 4,
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
        .unwrap();
        let service = BrowserAuth::new(
            vec![("broker".into(), Arc::new(verifier))],
            &origin,
            BrowserPolicy {
                oauth: OAuthLimits {
                    authorization_url_bytes: 8192,
                    callback_bytes: 8192,
                    callback_query_entries: 16,
                    form_body_bytes: 8192,
                    token_bytes: 4096,
                    response_nodes: 32,
                    response_scalar_bytes: 8192,
                },
                request_timeout: Duration::from_secs(2),
                login_timeout,
                session_lifetime: Duration::from_secs(60),
                refresh_margin: Duration::ZERO,
                allow_loopback_http: true,
            },
            stop.clone(),
        )
        .unwrap();
        let router = service.router();
        let stopping = server_stop.clone();
        let second = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stopping.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            service,
            origin,
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
            stop,
            server_stop,
            entered,
            servers: vec![first, second],
        }
    }
    fn request(&self) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}/auth/login", self.origin))
            .header("origin", &self.origin)
            .json(&json!({"provider":"broker"}))
    }
    async fn finish(mut self) {
        self.service.shutdown().await.unwrap();
        self.server_stop.cancel();
        for server in std::mem::take(&mut self.servers) {
            server.await.unwrap();
        }
    }
}

async fn diagnostic(response: reqwest::Response) -> (StatusCode, Value) {
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers().get("set-cookie").is_none());
    let body = response.text().await.unwrap();
    assert!(!body.contains(PRIVATE));
    assert!(!body.contains("http://"));
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    (status, value)
}

#[tokio::test]
async fn discovery_failure_has_finite_register_cause_without_provider_data() {
    let f = Fixture::new(Mode::InvalidDiscovery, Duration::from_secs(2)).await;
    let (status, body) = diagnostic(f.request().send().await.unwrap()).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(
        body,
        json!({"error":"provider verification failed","phase":"register","cause":"broker_invalid_metadata"})
    );
    f.finish().await;
}

#[tokio::test]
async fn registration_rejection_preserves_http_error_and_redacts_provider_body() {
    let f = Fixture::new(Mode::RejectedRegistration, Duration::from_secs(2)).await;
    let (status, body) = diagnostic(f.request().send().await.unwrap()).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(
        body,
        json!({"error":"OAuth provider operation failed","phase":"register","cause":"http_rejected"})
    );
    f.finish().await;
}

#[tokio::test]
async fn authorization_failure_identifies_actual_callsite_without_url_query() {
    let f = Fixture::new(Mode::InvalidAuthorization, Duration::from_secs(2)).await;
    let (status, body) = diagnostic(f.request().send().await.unwrap()).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(
        body,
        json!({"error":"OAuth provider operation failed","phase":"authorize","cause":"invalid_policy"})
    );
    f.finish().await;
}

#[tokio::test]
async fn login_start_deadline_preserves_original_and_cleans_pending_work() {
    let f = Fixture::new(Mode::HeldDiscovery, Duration::from_millis(100)).await;
    let (status, body) = diagnostic(f.request().send().await.unwrap()).await;
    // The existing outer deadline and SDK deadline can both observe the same cutoff.
    assert!(matches!(
        status,
        StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
    ));
    assert_eq!(body["phase"], "register");
    assert_eq!(body["cause"], "original_deadline");
    assert_eq!(
        body["error"],
        if status == StatusCode::SERVICE_UNAVAILABLE {
            "authentication interrupted"
        } else {
            "provider deadline exceeded"
        }
    );
    f.finish().await;
}

#[tokio::test]
async fn login_start_cancellation_retains_cause_and_joins_owned_work() {
    let f = Fixture::new(Mode::HeldDiscovery, Duration::from_secs(2)).await;
    let request = f.request();
    let mut job = tokio::spawn(async move { request.send().await.unwrap() });
    let response = tokio::time::timeout(Duration::from_secs(5), async {
        f.entered.notified().await;
        f.stop.cancel();
        (&mut job).await
    })
    .await;
    let response = match response {
        Ok(joined) => joined.unwrap(),
        Err(_) => {
            job.abort();
            let _ = job.await;
            f.finish().await;
            panic!("discovery admission and the cancelled request must finish");
        }
    };
    let (status, body) = diagnostic(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body,
        json!({"error":"authentication interrupted","phase":"register","cause":"original_cancelled"})
    );
    f.finish().await;
}

#[tokio::test]
async fn successful_start_keeps_authorization_url_cookie_and_pkce_without_diagnostic() {
    let f = Fixture::new(Mode::Success, Duration::from_secs(2)).await;
    let response = f.request().send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    let body: Value = response.json().await.unwrap();
    assert_eq!(body.as_object().unwrap().len(), 1);
    let url = url::Url::parse(body["authorization_url"].as_str().unwrap()).unwrap();
    let query: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(query.get("client_id").unwrap(), "browser-client");
    assert_eq!(query.get("code_challenge_method").unwrap(), "S256");
    for name in ["state", "nonce", "code_challenge"] {
        assert!(!query.get(name).unwrap().is_empty());
    }
    f.finish().await;
}
