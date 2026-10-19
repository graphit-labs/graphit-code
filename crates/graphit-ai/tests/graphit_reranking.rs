use graphit_ai::{
    cohere_rerank::CohereReranker,
    context::InferenceContext,
    graphit_rerank::GraphitReranker,
    jina_rerank::JinaReranker,
    native_rerank::{GuardedRerankError, NativeRerankError, NativeRerankLimits},
    voyage_rerank::VoyageReranker,
};
use graphit_auth::broker::{BrokerModelIdentity, BrokerRerankService};
use graphit_core::file_lock::Cancellation;
use graphit_http::client::{BoundedClient, ClientPolicy, Limits, NetworkEffect};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde_json::json;
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
#[path = "support/rerank_http.rs"]
mod wire;
use wire::{Reply, fixture, fixture_with_barriers, reply};

fn limits(batch: usize) -> NativeRerankLimits {
    NativeRerankLimits::new(100, batch, 10000, 10000, 10000).unwrap()
}
fn client(origin: &str) -> BoundedClient {
    BoundedClient::new(
        ClientPolicy {
            origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
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
        },
    )
    .unwrap()
}
fn context(cancel: &Cancellation) -> InferenceContext<'_> {
    InferenceContext::new(cancel, Instant::now() + Duration::from_secs(5))
}

#[tokio::test]
async fn existing_adapters_borrow_custom_headers_and_allow_no_key() {
    for family in ["cohere", "voyage", "jina"] {
        let rows = json!([{"index":1,"relevance_score":-0.0},{"index":0,"relevance_score":0.7}]);
        let response = if family == "voyage" {
            json!({"data":rows})
        } else {
            json!({"results":rows})
        };
        let tcp = fixture(vec![reply(response.clone()), reply(response)]).await;
        let client = client(&tcp.origin);
        let cancel = Cancellation::default();
        let mut custom = HeaderMap::new();
        custom.insert(
            "x-fixture-api-key",
            HeaderValue::from_static("synthetic-custom-canary"),
        );
        for headers in [&custom, &HeaderMap::new()] {
            let mut gate = || Ok::<(), Infallible>(());
            let scores = match family {
                "cohere" => {
                    CohereReranker::new(&client, &tcp.origin, "model", limits(2))
                        .unwrap()
                        .score_guarded("query", &["a", "b"], headers, context(&cancel), &mut gate)
                        .await
                }
                "voyage" => {
                    VoyageReranker::new(&client, &tcp.origin, "model", limits(2))
                        .unwrap()
                        .score_guarded("query", &["a", "b"], headers, context(&cancel), &mut gate)
                        .await
                }
                _ => {
                    JinaReranker::new(&client, &tcp.origin, "model", limits(2))
                        .unwrap()
                        .score_guarded("query", &["a", "b"], headers, context(&cancel), &mut gate)
                        .await
                }
            }
            .unwrap();
            assert_eq!(scores, [0.7, -0.0]);
        }
        let requests = tcp.server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0]
                .head
                .contains("x-fixture-api-key: synthetic-custom-canary")
        );
        assert!(!requests[1].head.contains("x-fixture-api-key"));
        for request in requests {
            assert!(!request.head.to_ascii_lowercase().contains("authorization:"));
            assert!(
                request
                    .head
                    .to_ascii_lowercase()
                    .contains("content-type: application/json")
            );
            assert_eq!(request.body["model"], "model");
        }
    }
}

fn descriptor(endpoint: &str) -> BrokerRerankService {
    BrokerRerankService {
        endpoint: endpoint.to_owned(),
        identity: BrokerModelIdentity {
            service_revision: "fixture-service-v3".into(),
            model: "fixture-model".into(),
            model_revision: "fixture-model-v7".into(),
            recipe: "fixture-recipe-v1".into(),
        },
        max_documents: 2,
    }
}
fn identity() -> serde_json::Value {
    json!({"revision":"fixture-service-v3","model":"fixture-model",
        "model_revision":"fixture-model-v7","recipe":"fixture-recipe-v1"})
}
fn graphit_reply(mut value: serde_json::Value) -> Reply {
    value["graphit"] = identity();
    reply(value)
}
fn with_identity(body: &str) -> String {
    assert!(body.starts_with('{'));
    format!("{{\"graphit\":{},{}", identity(), &body[1..])
}

#[tokio::test]
async fn graphit_wire_has_no_model_and_remaps_complete_batches() {
    let tcp = fixture(vec![graphit_reply(json!({"results":[{"index":1,"relevance_score":0.3},{"index":0,"relevance_score":0.9}],"graphit":{"revision":"fixture"}})),
        graphit_reply(json!({"results":[{"index":0,"relevance_score":-0.2}]}))]).await;
    let client = client(&tcp.origin);
    let selected = descriptor(&tcp.origin);
    let adapter = GraphitReranker::new(&client, &selected, limits(100)).unwrap();
    let cancel = Cancellation::default();
    let scores = adapter
        .score_guarded(
            "query",
            &["a", "b", "c"],
            &HeaderMap::new(),
            context(&cancel),
            &mut || Ok::<(), Infallible>(()),
        )
        .await
        .unwrap();
    assert_eq!(scores, [0.9, 0.3, -0.2]);
    let requests = tcp.server.await.unwrap();
    assert_eq!(
        requests[0].body,
        json!({"query":"query","documents":["a","b"],"top_n":2})
    );
    assert_eq!(
        requests[1].body,
        json!({"query":"query","documents":["c"],"top_n":1})
    );
}

#[tokio::test]
async fn graphit_rejects_malformed_full_sets_and_keeps_late_failure_effect() {
    for body in [
        r#"{"results":[{"index":0,"relevance_score":1},{"index":0,"relevance_score":2}]}"#,
        r#"{"results":[{"index":0,"relevance_score":1}]}"#,
        r#"{"results":[{"index":0,"relevance_score":1},{"index":2,"relevance_score":2}]}"#,
        r#"{"results":[{"index":0,"index":1,"relevance_score":1},{"index":1,"relevance_score":2}]}"#,
        r#"{"data":[{"index":0,"relevance_score":1},{"index":1,"relevance_score":2}]}"#,
        r#"{"results":[{"index":0,"relevance_score":1},{"index":1,"relevance_score":2}]} {}"#,
    ] {
        let tcp = fixture(vec![Reply {
            status: 200,
            body: with_identity(body),
            delay: Duration::ZERO,
        }])
        .await;
        let client = client(&tcp.origin);
        let cancel = Cancellation::default();
        let selected = descriptor(&tcp.origin);
        let error = GraphitReranker::new(&client, &selected, limits(2))
            .unwrap()
            .score_guarded(
                "query",
                &["a", "b"],
                &HeaderMap::new(),
                context(&cancel),
                &mut || Ok::<(), Infallible>(()),
            )
            .await
            .unwrap_err();
        assert_eq!(error.effect(), NetworkEffect::MayHaveApplied);
        assert_eq!(tcp.server.await.unwrap().len(), 1);
    }
    let tcp = fixture(vec![
        graphit_reply(json!({"results":[{"index":0,"relevance_score":1}]})),
        Reply {
            status: 429,
            body: "private-provider-canary".into(),
            delay: Duration::ZERO,
        },
    ])
    .await;
    let client = client(&tcp.origin);
    let cancel = Cancellation::default();
    let selected = descriptor(&tcp.origin);
    let error = GraphitReranker::new(&client, &selected, limits(1))
        .unwrap()
        .score_guarded(
            "query",
            &["a", "b", "c"],
            &HeaderMap::new(),
            context(&cancel),
            &mut || Ok::<(), Infallible>(()),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GuardedRerankError::Rerank {
            source: NativeRerankError::Status(StatusCode::TOO_MANY_REQUESTS),
            effect: NetworkEffect::MayHaveApplied
        }
    ));
    assert!(!format!("{error:?} {error}").contains("private-provider-canary"));
    assert_eq!(tcp.server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn graphit_guard_denial_after_observed_send_is_not_sent_again() {
    let release = Arc::new(Semaphore::new(0));
    let tcp = fixture_with_barriers(
        vec![graphit_reply(
            json!({"results":[{"index":0,"relevance_score":1}]}),
        )],
        vec![release.clone()],
    )
    .await;
    let client = client(&tcp.origin);
    let cancel = Cancellation::default();
    let allowed = AtomicBool::new(true);
    let mut gate = || {
        if allowed.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err("private-guard-canary")
        }
    };
    let selected = descriptor(&tcp.origin);
    let adapter = GraphitReranker::new(&client, &selected, limits(1)).unwrap();
    let headers = HeaderMap::new();
    let operation =
        adapter.score_guarded("query", &["a", "b"], &headers, context(&cancel), &mut gate);
    tokio::pin!(operation);
    tokio::select! { biased;
        _ = tcp.observed.notified() => {},
        result = &mut operation => panic!("operation ended before observed send: {result:?}"),
    }
    allowed.store(false, Ordering::Release);
    release.add_permits(1);
    let error = operation.await.unwrap_err();
    assert!(matches!(
        error,
        GuardedRerankError::Admission {
            source: "private-guard-canary",
            effect: NetworkEffect::MayHaveApplied
        }
    ));
    assert!(!format!("{error:?} {error}").contains("private-guard-canary"));
    assert_eq!(tcp.server.await.unwrap().len(), 1);
}

#[tokio::test]
async fn graphit_preflight_and_controlled_content_type_fail_before_network() {
    let tcp = fixture(vec![]).await;
    let client = client(&tcp.origin);
    let cancel = Cancellation::default();
    let selected = descriptor(&tcp.origin);
    let adapter = GraphitReranker::new(&client, &selected, limits(2)).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    let error = adapter
        .score_guarded("query", &["a"], &headers, context(&cancel), &mut || {
            Ok::<(), Infallible>(())
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GuardedRerankError::Rerank {
            source: NativeRerankError::Policy,
            effect: NetworkEffect::NotSent
        }
    ));
    let large = "x".repeat(10001);
    let error = adapter
        .score_guarded("query", &[&large], &headers, context(&cancel), &mut || {
            Ok::<(), Infallible>(())
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GuardedRerankError::Rerank {
            source: NativeRerankError::Limit,
            effect: NetworkEffect::NotSent
        }
    ));
    assert!(tcp.server.await.unwrap().is_empty());
}

#[tokio::test]
async fn graphit_requires_the_captured_service_and_model_identity_in_every_batch() {
    let valid = json!({"results":[{"index":0,"relevance_score":0.9}],"graphit":identity()});
    let mut invalid = Vec::new();
    for field in ["revision", "model", "model_revision", "recipe"] {
        let mut different = valid.clone();
        different["graphit"][field] = json!("another-observation");
        invalid.push(different.to_string());
        let mut missing = valid.clone();
        missing["graphit"].as_object_mut().unwrap().remove(field);
        invalid.push(missing.to_string());
    }
    invalid.push(json!({"results":valid["results"]}).to_string());
    invalid.push(valid.to_string().replace(
        "\"revision\":\"fixture-service-v3\"",
        "\"revision\":\"fixture-service-v3\",\"revision\":\"fixture-service-v3\"",
    ));
    invalid.push(format!(
        "{{\"graphit\":{},\"graphit\":{},\"results\":{}}}",
        identity(),
        identity(),
        valid["results"],
    ));
    for body in invalid {
        let tcp = fixture(vec![
            graphit_reply(valid.clone()),
            Reply {
                status: 200,
                body,
                delay: Duration::ZERO,
            },
        ])
        .await;
        let client = client(&tcp.origin);
        let selected = descriptor(&tcp.origin);
        let adapter = GraphitReranker::new(&client, &selected, limits(1)).unwrap();
        let cancel = Cancellation::default();
        let error = adapter
            .score_guarded(
                "query",
                &["first", "second", "unsent"],
                &HeaderMap::new(),
                context(&cancel),
                &mut || Ok::<(), Infallible>(()),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            GuardedRerankError::Rerank {
                source: NativeRerankError::Shape | NativeRerankError::Encoding,
                effect: NetworkEffect::MayHaveApplied,
            }
        ));
        assert_eq!(tcp.server.await.unwrap().len(), 2);
    }
}
