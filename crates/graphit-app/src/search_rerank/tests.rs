use super::*;
use graphit_auth::{
    broker::{BrokerModelIdentity, JsonLimits, VerifierLimits},
    profile::DirectProtocol,
};
use graphit_core::file_lock::Cancellation;
use graphit_http::client::{ClientPolicy, Limits};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

#[test]
fn admission_diagnostics_keep_transport_distinct_from_denial() {
    use graphit_auth::broker::BrokerError;
    for (cause, expected) in [
        (
            ServiceUseError::Broker(BrokerError::Connection),
            "rerank_service_failure",
        ),
        (
            ServiceUseError::Broker(BrokerError::Deadline),
            "request_cancelled_or_expired",
        ),
        (ServiceUseError::Broker(BrokerError::Limit), "rerank_limit"),
        (
            ServiceUseError::Auth(graphit_auth::AuthError::Denied),
            "authentication_or_permission_denied",
        ),
    ] {
        let error = SearchRerankError {
            cause: Failure::Service(cause),
            effect: NetworkEffect::NotSent,
        };
        assert_eq!(error.code(), expected);
        assert!(error.source().is_some());
    }
}

// Reuse the real public Auth and bounded TCP fixtures, rather than inventing
// another grant, network harness or model implementation for this consumer.
#[allow(
    dead_code,
    reason = "shared Auth fixture also serves embedding consumers"
)]
#[path = "../../../graphit-ai/tests/support/direct_rerank_auth.rs"]
mod auth;
#[allow(
    dead_code,
    reason = "shared wire fixture includes optional response barriers"
)]
use crate::rerank_test_http as wire;
use auth::{AuthFixture, MCP_KEY, RERANK_KEY, request};

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
    client: &'a BoundedClient,
    verifiers: &'a BrokerVerifierCache,
    broker: Option<&'a BrokerRerankService>,
    batch: usize,
) -> SearchRerankResources<'a> {
    SearchRerankResources {
        local: None,
        http: Some(client),
        verifiers: Some(verifiers),
        broker,
        limits: NativeRerankLimits::new(100, batch, 10000, 10000, 10000).unwrap(),
        ranking: RankingLimits::new(100, 1, 1).unwrap(),
    }
}

fn descriptor(endpoint: &str) -> BrokerRerankService {
    BrokerRerankService {
        endpoint: endpoint.to_owned(),
        identity: BrokerModelIdentity {
            service_revision: "service-v3".into(),
            model: "fixture-cross-encoder".into(),
            model_revision: "model-v7".into(),
            recipe: "fixture-query-document-v1".into(),
        },
        max_documents: 2,
    }
}
fn broker_response(results: serde_json::Value) -> serde_json::Value {
    json!({"results":results,"graphit":{
        "revision":"service-v3","model":"fixture-cross-encoder",
        "model_revision":"model-v7","recipe":"fixture-query-document-v1"
    }})
}

#[tokio::test]
async fn direct_scores_keep_every_candidate_and_its_original_retrieval_values() {
    let tcp = wire::fixture(vec![wire::reply(json!({"results":[
        {"index":2,"relevance_score":0.9},
        {"index":1,"relevance_score":-0.0},
        {"index":0,"relevance_score":0.0}
    ]}))])
    .await;
    let endpoint = format!("{}/captured/rerank", tcp.origin);
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&ctx, DirectProtocol::Cohere, &endpoint);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let http = client(&tcp.origin);
    let cache = verifiers();
    let selected = resources(&http, &cache, None, 3);
    let ranked = score(
        ctx.snapshot(),
        &mut selection,
        &selected,
        "query",
        &["a", "b", "c"],
    )
    .await
    .unwrap();
    assert_eq!(ranked.scores, [0.0, -0.0, 0.9]);
    assert!(ranked.scores[1].is_sign_negative());
    assert_eq!(ranked.order, [2, 0, 1]);
    let candidates = [("alpha", 4.0, 0.2), ("beta", 3.0, 0.1), ("gamma", 2.0, 0.3)];
    let reordered: Vec<_> = ranked
        .order
        .iter()
        .map(|&position| candidates[position])
        .collect();
    assert_eq!(reordered, [candidates[2], candidates[0], candidates[1]]);
    let requests = tcp.server.await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .head
            .starts_with("POST /captured/rerank HTTP/1.1")
    );
    assert!(requests[0].head.contains(&format!("Bearer {RERANK_KEY}")));
    assert!(!requests[0].head.contains(MCP_KEY));
}

#[tokio::test]
async fn broker_uses_resolved_endpoint_its_real_key_and_complete_graphit_batches() {
    let tcp = wire::fixture(vec![
        wire::reply(broker_response(
            json!([{"index":1,"relevance_score":0.8},{"index":0,"relevance_score":0.1}]),
        )),
        wire::reply(broker_response(json!([{"index":0,"relevance_score":0.4}]))),
    ])
    .await;
    let endpoint = format!("{}/discovered/rerank", tcp.origin);
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::broker(&ctx, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let http = client(&tcp.origin);
    let cache = verifiers();
    let descriptor = descriptor(&endpoint);
    // The descriptor's max_documents=2 limits the request below local batch=100.
    let selected = resources(&http, &cache, Some(&descriptor), 100);
    let ranked = score(
        ctx.snapshot(),
        &mut selection,
        &selected,
        "query",
        &["a", "b", "c"],
    )
    .await
    .unwrap();
    assert_eq!(ranked.scores, [0.1, 0.8, 0.4]);
    assert_eq!(ranked.order, [1, 2, 0]);
    let requests = tcp.server.await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert!(request.head.starts_with("POST /discovered/rerank HTTP/1.1"));
        assert!(request.head.contains("Bearer private-fixture-broker-key"));
        assert!(!request.head.contains(RERANK_KEY));
        assert!(!request.head.contains(MCP_KEY));
        assert!(request.body.get("model").is_none());
        assert_eq!(
            request.body["top_n"],
            request.body["documents"].as_array().unwrap().len()
        );
    }
}

#[tokio::test]
async fn foreign_request_wrong_purpose_and_empty_page_have_no_network_effect() {
    let tcp = wire::fixture(vec![]).await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let foreign = request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin);
    let http = client(&tcp.origin);
    let cache = verifiers();
    let selected = resources(&http, &cache, None, 1);
    for purpose in [ServicePurpose::Embedding, ServicePurpose::Rerank] {
        let issuer = auth.issuer(&ctx, purpose);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer.capture_service(&caller, purpose).unwrap();
        let error = score(foreign.snapshot(), &mut selection, &selected, "q", &["doc"])
            .await
            .unwrap_err();
        assert!(matches!(error.cause, Failure::ForeignRequest));
        assert_eq!(error.effect(), NetworkEffect::NotSent);
        if purpose == ServicePurpose::Embedding {
            let error = score(ctx.snapshot(), &mut selection, &selected, "q", &["doc"])
                .await
                .unwrap_err();
            assert!(matches!(error.cause, Failure::WrongPurpose));
        } else {
            let empty = score(ctx.snapshot(), &mut selection, &selected, "q", &[])
                .await
                .unwrap();
            assert!(empty.scores.is_empty() && empty.order.is_empty());
        }
    }
    assert!(tcp.server.await.unwrap().is_empty());
}

#[tokio::test]
async fn real_input_limit_and_missing_broker_descriptor_fail_before_send() {
    let tcp = wire::fixture(vec![]).await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let http = client(&tcp.origin);
    let cache = verifiers();
    let mut selected = resources(&http, &cache, None, 1);
    selected.limits = NativeRerankLimits::new(1, 1, 100, 10000, 10000).unwrap();
    for broker in [false, true] {
        let auth = if broker {
            AuthFixture::broker(&ctx, &tcp.origin)
        } else {
            AuthFixture::new(&ctx, DirectProtocol::Jina, &tcp.origin)
        };
        let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Rerank)
            .unwrap();
        if broker {
            selected.broker = None;
        }
        let error = score(ctx.snapshot(), &mut selection, &selected, "q", &["a", "b"])
            .await
            .unwrap_err();
        assert_eq!(error.effect(), NetworkEffect::NotSent);
        if broker {
            assert!(matches!(error.cause, Failure::MissingBrokerDescriptor));
        } else {
            assert!(
                error
                    .source()
                    .unwrap()
                    .downcast_ref::<DirectNativeRerankError>()
                    .is_some()
            );
        }
    }
    assert!(tcp.server.await.unwrap().is_empty());
}

#[tokio::test]
async fn malformed_later_batch_is_terminal_and_never_returns_partial_scores() {
    let tcp = wire::fixture(vec![
        wire::reply(broker_response(json!([{"index":0,"relevance_score":0.8}]))),
        wire::reply(broker_response(
            json!([{"index":0,"relevance_score":0.7},{"index":0,"relevance_score":0.6}]),
        )),
    ])
    .await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::broker(&ctx, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let http = client(&tcp.origin);
    let cache = verifiers();
    let descriptor = descriptor(&tcp.origin);
    let selected = resources(&http, &cache, Some(&descriptor), 1);
    let error = score(
        ctx.snapshot(),
        &mut selection,
        &selected,
        "q",
        &["a", "b", "c"],
    )
    .await
    .unwrap_err();
    assert_eq!(error.effect(), NetworkEffect::MayHaveApplied);
    assert!(matches!(error.cause, Failure::Broker(_)));
    assert_eq!(tcp.server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn cancellation_and_physical_auth_revocation_discard_observed_response() {
    for revoke in [false, true] {
        let release = Arc::new(Semaphore::new(0));
        let tcp = wire::fixture_with_barriers(
            vec![wire::reply(
                json!({"results":[{"index":0,"relevance_score":0.9}]}),
            )],
            vec![Arc::clone(&release)],
        )
        .await;
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin);
        let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Rerank)
            .unwrap();
        let http = client(&tcp.origin);
        let cache = verifiers();
        let selected = resources(&http, &cache, None, 1);
        let call = score(
            ctx.snapshot(),
            &mut selection,
            &selected,
            "private-query",
            &["document"],
        );
        let interrupt = async {
            tcp.observed.notified().await;
            if revoke {
                auth.rotate_key(&ctx);
            } else {
                cancel.cancel();
            }
            release.add_permits(1);
        };
        let (result, ()) = tokio::join!(call, interrupt);
        let error = result.unwrap_err();
        assert_eq!(error.effect(), NetworkEffect::MayHaveApplied);
        assert_eq!(
            error.code(),
            if revoke {
                "authentication_or_permission_denied"
            } else {
                "request_cancelled_or_expired"
            }
        );
        assert!(error.source().is_some());
        for secret in ["private-query", MCP_KEY, RERANK_KEY, "127.0.0.1"] {
            assert!(!format!("{error:?} {error}").contains(secret));
        }
        assert_eq!(tcp.server.await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn original_cancel_and_deadline_fail_before_network_without_replacement_context() {
    for cancel_request in [true, false] {
        let tcp = wire::fixture(vec![]).await;
        let cancel = Cancellation::default();
        let setup = request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = AuthFixture::new(&setup, DirectProtocol::Cohere, &tcp.origin);
        let deadline = Instant::now() + Duration::from_secs(1);
        let ctx = request(&cancel, deadline);
        let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Rerank)
            .unwrap();
        let http = client(&tcp.origin);
        let cache = verifiers();
        let selected = resources(&http, &cache, None, 1);
        if cancel_request {
            cancel.cancel();
        } else {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        }
        let error = score(ctx.snapshot(), &mut selection, &selected, "q", &["doc"])
            .await
            .unwrap_err();
        let Failure::Request(cause) = &error.cause else {
            panic!("expected original IO cause");
        };
        assert_eq!(
            cause.kind(),
            if cancel_request {
                io::ErrorKind::Interrupted
            } else {
                io::ErrorKind::TimedOut
            }
        );
        assert_eq!(error.effect(), NetworkEffect::NotSent);
        assert!(tcp.server.await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn broker_rejects_missing_duplicate_or_different_service_and_model_identity() {
    let rows = json!([{"index":0,"relevance_score":0.9}]);
    let valid = broker_response(rows.clone());
    let mut bodies = Vec::new();
    for field in ["revision", "model", "model_revision", "recipe"] {
        let mut changed = valid.clone();
        changed["graphit"][field] = json!("different-captured-identity");
        bodies.push(changed.to_string());
        let mut missing = valid.clone();
        missing["graphit"].as_object_mut().unwrap().remove(field);
        bodies.push(missing.to_string());
    }
    bodies.push(json!({"results":rows}).to_string());
    bodies.push(valid.to_string().replace(
        "\"revision\":\"service-v3\"",
        "\"revision\":\"service-v3\",\"revision\":\"service-v3\"",
    ));
    let identity = valid["graphit"].to_string();
    bodies.push(format!(
        "{{\"graphit\":{identity},\"graphit\":{identity},\"results\":{}}}",
        valid["results"],
    ));
    for body in bodies {
        let tcp = wire::fixture(vec![wire::Reply {
            status: 200,
            body,
            delay: Duration::ZERO,
        }])
        .await;
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = AuthFixture::broker(&ctx, &tcp.origin);
        let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Rerank)
            .unwrap();
        let http = client(&tcp.origin);
        let cache = verifiers();
        let descriptor = descriptor(&tcp.origin);
        let selected = resources(&http, &cache, Some(&descriptor), 1);
        let error = score(ctx.snapshot(), &mut selection, &selected, "q", &["a"])
            .await
            .unwrap_err();
        assert!(matches!(error.cause, Failure::Broker(_)));
        assert_eq!(error.effect(), NetworkEffect::MayHaveApplied);
        assert_eq!(tcp.server.await.unwrap().len(), 1);
    }
}
