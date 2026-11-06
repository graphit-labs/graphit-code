//! Real Direct Auth leases and bounded HTTP exercise the shared dispatcher.
use graphit_ai::embedding::{EmbeddingError, EmbeddingLimits, GuardedEmbeddingError};
use graphit_app::embedding_service::*;
use graphit_auth::ServicePurpose;
use graphit_auth::{
    broker::{JsonLimits, VerifierLimits},
    profile::DirectProtocol,
};
use graphit_core::file_lock::Cancellation;
use graphit_daemon::{
    broker_verifiers::BrokerVerifierCache,
    model_service::{EmbedInput, EmbedOperation, ModelErrorKind},
};
use graphit_http::client::BoundedClient;
use graphit_http::client::{ClientPolicy, Limits};
use serde_json::json;
use std::time::Duration;
use std::time::Instant;

#[allow(
    dead_code,
    reason = "shared fixture also covers rerank and Broker admission"
)]
#[path = "../../graphit-ai/tests/support/direct_rerank_auth.rs"]
mod auth;
use crate::rerank_http as wire;

fn network() -> Limits {
    Limits {
        request_body_bytes: 10000,
        response_body_bytes: 10000,
        url_bytes: 2048,
        header_bytes: 4096,
        header_count: 32,
        max_in_flight: 1,
        max_idle_per_host: 1,
        idle_timeout: Duration::from_secs(1),
        max_certificates: 1,
        max_certificate_bytes: 4096,
    }
}
fn client(origin: &str) -> BoundedClient {
    BoundedClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        network(),
    )
    .unwrap()
}
fn verifiers() -> BrokerVerifierCache {
    BrokerVerifierCache::new(
        network(),
        VerifierLimits {
            token_bytes: 4096,
            json: JsonLimits {
                bytes: 10000,
                nodes: 1000,
                depth: 16,
                scalar_bytes: 4096,
                collection_entries: 100,
            },
            max_keys: 8,
            metadata_ttl: Duration::from_secs(30),
            key_ttl: Duration::from_secs(30),
            unknown_key_cooldown: Duration::from_secs(1),
        },
        true,
        None,
        &[],
    )
}
fn resources<'a>(
    http: &'a BoundedClient,
    brokers: &'a BrokerVerifierCache,
) -> EmbeddingResources<'a> {
    EmbeddingResources {
        local: None,
        http: Some(http),
        brokers,
        limits: EmbeddingLimits::new(8, 4096, 3, 24, 8, 10000, 10000).unwrap(),
    }
}

#[tokio::test]
async fn direct_code_and_record_roles_preserve_literal_text_without_local_resources() {
    let response = wire::reply(json!({"object":"list", "data":[
        {"object":"embedding","index":1,"embedding":[0.25,0.5,0.75]},
        {"object":"embedding","index":0,"embedding":[1.,0.,-1.]}
    ]}));
    let tcp = wire::fixture(
        (0..4)
            .map(|_| wire::Reply {
                status: response.status,
                body: response.body.clone(),
                delay: response.delay,
            })
            .collect(),
    )
    .await;
    let endpoint = format!("{}/selected/embedding", tcp.origin);
    let cancel = Cancellation::default();
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::embedding(&request, DirectProtocol::OpenAi, &endpoint);
    let issuer = auth.issuer(&request, ServicePurpose::Embedding);
    let caller = issuer.authenticate_local(auth::MCP_KEY, &request).unwrap();
    let http = client(&tcp.origin);
    let brokers = verifiers();
    let execution = resources(&http, &brokers);
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let expected = metadata(&mut selection, &execution).await.unwrap();
    assert_eq!(expected.dimensions, 3);
    assert_eq!(expected.local_generation, None);
    for input in [
        EmbedInput::Query,
        EmbedInput::Document,
        EmbedInput::CodeQuery,
        EmbedInput::CodeDocument,
    ] {
        let output = embed(
            &mut selection,
            &execution,
            &expected,
            EmbedOperation {
                input,
                texts: vec!["função literal\n🙂".into(), "documento".into()],
                deadline: request.snapshot().required_deadline().unwrap(),
            },
        )
        .await
        .unwrap();
        assert_eq!(output.vectors, [vec![1., 0., -1.], vec![0.25, 0.5, 0.75]]);
        assert_eq!(output.metadata.vector_id, expected.vector_id);
        assert_eq!(output.metadata.local_generation, None);
    }
    let observed = tcp.server.await.unwrap();
    assert_eq!(observed.len(), 4);
    for request in observed {
        assert!(
            request
                .head
                .starts_with("POST /selected/embedding HTTP/1.1")
        );
        assert!(
            request
                .head
                .contains(&format!("Bearer {}", auth::EMBEDDING_KEY))
        );
        assert!(!request.head.contains(auth::MCP_KEY));
        assert!(!request.head.contains(auth::RERANK_KEY));
        assert_eq!(
            request.body["input"],
            json!(["função literal\n🙂", "documento"])
        );
    }
}

#[tokio::test]
async fn changed_vector_space_is_rejected_before_a_connection() {
    let tcp = wire::fixture(Vec::new()).await;
    let cancel = Cancellation::default();
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::embedding(&request, DirectProtocol::OpenAi, &tcp.origin);
    let issuer = auth.issuer(&request, ServicePurpose::Embedding);
    let caller = issuer.authenticate_local(auth::MCP_KEY, &request).unwrap();
    let http = client(&tcp.origin);
    let brokers = verifiers();
    let execution = resources(&http, &brokers);
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let mut expected = metadata(&mut selection, &execution).await.unwrap();
    expected.vector_id[0] ^= 1;
    let error = embed(
        &mut selection,
        &execution,
        &expected,
        EmbedOperation {
            input: EmbedInput::CodeDocument,
            texts: vec!["literal".into()],
            deadline: request.snapshot().required_deadline().unwrap(),
        },
    )
    .await
    .err()
    .expect("identity mismatch must fail");
    assert!(matches!(error, EmbeddingServiceError::Metadata));
    assert!(tcp.server.await.unwrap().is_empty());
}

#[test]
fn remote_error_classification_preserves_deadline_cancellation_and_admission() {
    use graphit_ai::context::ContextError;
    use graphit_http::client::{NetworkEffect, NetworkError};
    for (source, expected) in [
        (
            EmbeddingError::Context(ContextError::Cancelled),
            ModelErrorKind::Cancelled,
        ),
        (
            EmbeddingError::Network(NetworkError::Deadline),
            ModelErrorKind::Deadline,
        ),
        (
            EmbeddingError::Network(NetworkError::Connection),
            ModelErrorKind::Transport,
        ),
        (EmbeddingError::Credential, ModelErrorKind::Authentication),
        (EmbeddingError::NonFinite, ModelErrorKind::Output),
    ] {
        let error = EmbeddingServiceError::Graphit(GuardedEmbeddingError::Embedding {
            source,
            effect: NetworkEffect::MayHaveApplied,
            completed: Vec::new(),
            usage: Vec::new(),
        });
        assert_eq!(error.kind(), expected);
        assert!(std::error::Error::source(&error).is_some());
    }
}

#[tokio::test]
async fn broker_dispatcher_discovers_once_and_embeds_documents_and_query_without_a_local_model() {
    let tcp = wire::fixture_with_origin(|origin| {
        use aws_lc_rs::{rand::SystemRandom, signature::{Ed25519KeyPair, KeyPair}};
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let jwks = json!({"keys":[{"kty":"OKP","crv":"Ed25519","kid":"fixture-key",
            "alg":"EdDSA","use":"sig","key_ops":["verify"],"x":URL_SAFE_NO_PAD.encode(key.public_key().as_ref())}]});
        let broker = json!({"version":"1","issuer":origin,
            "authentication":{"type":"oidc","issuer":origin,"client_id":"fixture-client",
                "scopes":["openid"],"audiences":["fixture-api"],"access_token_audience":"fixture-api",
                "redirect_uri_path":"/callback"},
            "services":{
                "embeddings":{"protocol":"graphit-embeddings-v1","path":"/discovered/embeddings",
                    "revision":"service-r7","model":"fixture-remote-embedding","model_revision":"weights-r2",
                    "recipe":"retrieval-r3","dimensions":3,"max_batch":1,"max_input_bytes":4096},
                "rerank":{"protocol":"intentionally-invalid-unused-service"}
            }});
        let oidc = json!({"issuer":origin,
            "authorization_endpoint":format!("{origin}/authorize"),"token_endpoint":format!("{origin}/token"),
            "jwks_uri":format!("{origin}/jwks"),"grant_types_supported":["authorization_code","refresh_token"],
            "code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],
            "id_token_signing_alg_values_supported":["EdDSA"]});
        let response = |vector| wire::reply(json!({"object":"list",
            "data":[{"object":"embedding","index":0,"embedding":vector}],
            "graphit":{"revision":"service-r7","model":"fixture-remote-embedding",
                "model_revision":"weights-r2","recipe":"retrieval-r3","dimensions":3},
            "usage":{"input_tokens":1}}));
        let headers = vec![
            ("x-graphit-embedding-revision".into(),"service-r7".into()),
            ("x-graphit-embedding-dimensions".into(),"3".into()),
        ];
        (vec![wire::reply(broker), wire::reply(oidc), wire::reply(jwks), response([1.,0.,0.]),
            response([0.,1.,0.]), response([0.,0.,1.])],
         vec![Vec::new(),Vec::new(),Vec::new(),headers.clone(),headers.clone(),headers])
    }).await;
    let cancel = Cancellation::default();
    let request = auth::request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = auth::AuthFixture::broker(&request, &tcp.origin);
    let issuer = auth.issuer(&request, ServicePurpose::Embedding);
    let caller = issuer.authenticate_local(auth::MCP_KEY, &request).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    assert_eq!(
        selection.mode().unwrap(),
        graphit_auth::ServiceMode::BrokerKey
    );
    let http = client(&tcp.origin);
    let brokers = verifiers();
    let execution = resources(&http, &brokers);
    let expected = metadata(&mut selection, &execution).await.unwrap();
    assert_eq!(expected.model, "fixture-remote-embedding");
    assert_eq!(expected.revision, "weights-r2");
    assert_eq!(expected.dimensions, 3);
    let deadline = request.snapshot().required_deadline().unwrap();
    let documents = embed(
        &mut selection,
        &execution,
        &expected,
        EmbedOperation {
            input: EmbedInput::CodeDocument,
            texts: vec!["função um".into(), "literal dois".into()],
            deadline,
        },
    )
    .await
    .unwrap();
    assert_eq!(documents.vectors, [vec![1., 0., 0.], vec![0., 1., 0.]]);
    let query = embed(
        &mut selection,
        &execution,
        &expected,
        EmbedOperation {
            input: EmbedInput::CodeQuery,
            texts: vec!["consulta literal".into()],
            deadline,
        },
    )
    .await
    .unwrap();
    assert_eq!(query.vectors, [vec![0., 0., 1.]]);
    assert_eq!(query.metadata.vector_id, documents.metadata.vector_id);
    assert_eq!(query.metadata.local_generation, None);
    assert_eq!(documents.metadata.local_generation, None);
    let requests = tcp.server.await.unwrap();
    assert_eq!(
        requests.len(),
        6,
        "metadata and JWKS must be reused without another discovery or rerank request"
    );
    assert!(
        requests[0]
            .head
            .starts_with("GET /.well-known/graphit-broker HTTP/1.1")
    );
    assert!(
        requests[1]
            .head
            .starts_with("GET /.well-known/openid-configuration HTTP/1.1")
    );
    assert!(requests[2].head.starts_with("GET /jwks HTTP/1.1"));
    for (index, request) in requests[3..].iter().enumerate() {
        assert!(
            request
                .head
                .starts_with("POST /discovered/embeddings HTTP/1.1")
        );
        assert!(request.head.contains("Bearer private-fixture-broker-key"));
        assert!(!request.head.contains(auth::MCP_KEY));
        assert!(!request.head.contains(auth::EMBEDDING_KEY));
        assert!(!request.head.contains(auth::RERANK_KEY));
        assert!(request.body.get("model").is_none());
        assert_eq!(
            request.body["input_type"],
            if index < 2 {
                json!("document")
            } else {
                json!("query")
            }
        );
    }
    assert_eq!(requests[3].body["input"], json!(["função um"]));
    assert_eq!(requests[4].body["input"], json!(["literal dois"]));
    assert_eq!(requests[5].body["input"], json!(["consulta literal"]));
}
