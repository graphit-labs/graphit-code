use graphit_ai::{
    cohere_rerank::CohereReranker,
    context::{ContextError, InferenceContext},
    native_rerank::{NativeRerankError, NativeRerankLimits},
};
use graphit_core::file_lock::Cancellation;
use graphit_http::client::{BoundedClient, ClientPolicy, Limits, NetworkError};
use http::StatusCode;
use secrecy::SecretString;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
#[path = "support/rerank_http.rs"]
mod rerank_http;
use rerank_http::{Reply, fixture, reply};

fn limits() -> NativeRerankLimits {
    NativeRerankLimits::new(10000, 200, 200000, 200000, 400000).unwrap()
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
            request_body_bytes: 200000,
            response_body_bytes: 400000,
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
fn key() -> SecretString {
    SecretString::from("fixture-key-canary")
}
fn context(cancel: &Cancellation) -> InferenceContext<'_> {
    InferenceContext::new(cancel, Instant::now() + Duration::from_secs(5))
}
fn rows(count: usize) -> Value {
    json!({"results":(0..count).rev().map(|i|json!({"index":i,"relevance_score":i as f64})).collect::<Vec<_>>(),"meta":{"tokens":1}})
}
fn adapter<'a>(
    client: &'a BoundedClient,
    endpoint: &'a str,
    policy: NativeRerankLimits,
) -> CohereReranker<'a> {
    CohereReranker::new(client, endpoint, "fixture-model", policy).unwrap()
}
fn error<T>(result: Result<T, NativeRerankError>) -> NativeRerankError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("expected rerank error"),
    }
}
fn redacted(e: NativeRerankError) {
    let mut diagnostic = format!("{e:?} {e}");
    if let Some(source) = std::error::Error::source(&e) {
        diagnostic.push_str(&source.to_string());
    }
    for canary in [
        "fixture-key-canary",
        "fixture-input-canary",
        "fixture-model",
        "fixture-query-canary",
        "127.0.0.1",
        "provider-secret-canary",
    ] {
        assert!(!diagnostic.contains(canary));
    }
}

#[tokio::test]
async fn exact_wire_full_set_remap_and_per_call_credentials() {
    let fixture = fixture(vec![
        reply(json!({"results":[
        {"index":1,"relevance_score":0.5000000000001},
        {"index":0,"relevance_score":0.5}]})),
        reply(rows(1)),
    ])
    .await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/v2/rerank?fixture-query-canary", fixture.origin);
    let adapter = adapter(&client, &endpoint, limits());
    let cancel = Cancellation::default();
    let scores = adapter
        .score(
            "fixture-query-canary",
            &["fixture-input-canary", "second"],
            &key(),
            context(&cancel),
        )
        .await
        .unwrap();
    assert_eq!(scores, [0.5, 0.5000000000001]);
    assert!(scores[1] > scores[0]);
    assert_eq!(
        adapter
            .score(
                "another",
                &["query"],
                &SecretString::from("second-synthetic-key"),
                context(&cancel)
            )
            .await
            .unwrap(),
        [0.0]
    );
    let requests = fixture.server.await.unwrap();
    assert_eq!(requests.len(), 2);
    for (r, q, docs, bearer) in [
        (
            &requests[0],
            "fixture-query-canary",
            json!(["fixture-input-canary", "second"]),
            "fixture-key-canary",
        ),
        (
            &requests[1],
            "another",
            json!(["query"]),
            "second-synthetic-key",
        ),
    ] {
        assert!(
            r.head
                .starts_with("POST /v2/rerank?fixture-query-canary HTTP/1.1\r\n")
        );
        assert!(
            r.head
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {bearer}\r\n"))
        );
        assert_eq!(
            r.body,
            json!({"model":"fixture-model","query":q,"documents":docs,"top_n":docs.as_array().unwrap().len()})
        );
    }
}
#[tokio::test]
async fn explicit_batch_policy_offsets_without_hidden_200_or_1000_cap() {
    for (count, batch) in [(201, 200), (5, 2), (1001, 1001)] {
        let sizes: Vec<_> = (0..count)
            .step_by(batch)
            .map(|i| (count - i).min(batch))
            .collect();
        let fixture = fixture(sizes.iter().map(|n| reply(rows(*n))).collect()).await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/rerank", fixture.origin);
        let cancel = Cancellation::default();
        let policy = NativeRerankLimits::new(2000, batch, 10000, 200000, 400000).unwrap();
        let result = adapter(&client, &endpoint, policy)
            .score("same query", &vec!["x"; count], &key(), context(&cancel))
            .await
            .unwrap();
        assert_eq!(result.len(), count);
        let requests = fixture.server.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .map(|r| r.body["top_n"].as_u64().unwrap() as usize)
                .collect::<Vec<_>>(),
            sizes
        );
        let mut offset = 0;
        for (request, size) in requests.iter().zip(sizes) {
            assert_eq!(request.body["query"], "same query");
            assert_eq!(request.body["documents"], json!(vec!["x"; size]));
            for i in 0..size {
                assert_eq!(result[offset + i], i as f64);
            }
            offset += size;
        }
    }
}
#[tokio::test]
async fn empty_preflight_and_original_context_have_zero_socket_effects() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let client = client(&origin);
    let endpoint = format!("{origin}/v2/rerank");
    let cancel = Cancellation::default();
    let bounded = adapter(&client, &endpoint, limits());
    assert!(
        bounded
            .score("unused", &[], &SecretString::from(""), context(&cancel))
            .await
            .unwrap()
            .is_empty()
    );
    let policy = NativeRerankLimits::new(2, 1, 3, 1000, 1000).unwrap();
    let small = adapter(&client, &endpoint, policy);
    for (query, documents) in [("é", vec!["ab"]), ("q", vec!["a", "b", "c"])] {
        assert_eq!(
            error(
                small
                    .score(query, &documents, &key(), context(&cancel))
                    .await
            ),
            NativeRerankError::Limit
        );
    }
    let cap = json!({"model":"fixture-model","query":"q","documents":["x"],"top_n":1})
        .to_string()
        .len();
    let narrow = adapter(
        &client,
        &endpoint,
        NativeRerankLimits::new(3, 1, 100, cap, 1000).unwrap(),
    );
    let escaped = "\u{1}".repeat(20);
    assert_eq!(
        error(
            narrow
                .score("q", &["x", &escaped], &key(), context(&cancel))
                .await
        ),
        NativeRerankError::Limit
    );
    // All JSON batches are checked before even validating header credentials.
    assert_eq!(
        error(
            narrow
                .score(
                    "q",
                    &["x", &escaped],
                    &SecretString::from(""),
                    context(&cancel)
                )
                .await
        ),
        NativeRerankError::Limit
    );
    for key in [SecretString::from(""), SecretString::from("bad\r\nkey")] {
        assert_eq!(
            error(bounded.score("q", &["x"], &key, context(&cancel)).await),
            NativeRerankError::Policy
        );
    }
    assert_eq!(
        error(
            bounded
                .score(
                    "unused",
                    &[],
                    &key(),
                    InferenceContext::new(&cancel, Instant::now())
                )
                .await
        ),
        NativeRerankError::Context(ContextError::Deadline)
    );
    cancel.cancel();
    assert_eq!(
        error(bounded.score("unused", &[], &key(), context(&cancel)).await),
        NativeRerankError::Context(ContextError::Cancelled)
    );
    assert_eq!(
        error(bounded.score("q", &["x"], &key(), context(&cancel)).await),
        NativeRerankError::Context(ContextError::Cancelled)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(25), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn invalid_wire_never_fills_zero_or_returns_partial() {
    for body in [
        "{",
        "{}",
        "{\"results\":null}",
        "{\"results\":{}}",
        "{\"results\":[]}",
        "{\"results\":[{}]}",
        "{\"results\":[{\"index\":0}]}",
        "{\"results\":[{\"relevance_score\":1}]}",
        "{\"results\":[{\"index\":null,\"relevance_score\":1}]}",
        "{\"results\":[{\"index\":0,\"relevance_score\":null}]}",
        "{\"results\":[{\"index\":-1,\"relevance_score\":1}]}",
        "{\"results\":[{\"index\":1,\"relevance_score\":1}]}",
        "{\"results\":[{\"index\":18446744073709551616,\"relevance_score\":1}]}",
        "{\"results\":[{\"index\":0,\"relevance_score\":1e400}]}",
        "{\"results\":[{\"index\":0,\"relevance_score\":NaN}]}",
        "{\"results\":[{\"index\":0,\"index\":0,\"relevance_score\":1}]}",
        "{\"results\":[{\"index\":0,\"relevance_score\":1,\"relevance_score\":2}]}",
        "{\"results\":[{\"index\":0,\"relevance_score\":1}],\"results\":[{\"index\":0,\"relevance_score\":2}]}",
        "{\"results\":[{\"index\":0,\"relevance_score\":1},{\"index\":0,\"relevance_score\":1}]}",
        "{\"results\":[{\"index\":0,\"relevance_score\":1}]} trailing",
    ] {
        let fixture = fixture(vec![Reply {
            status: 200,
            body: body.to_owned(),
            delay: Duration::ZERO,
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/rerank", fixture.origin);
        let cancel = Cancellation::default();
        let e = error(
            adapter(&client, &endpoint, limits())
                .score(
                    "fixture-query-canary",
                    &["fixture-input-canary"],
                    &key(),
                    context(&cancel),
                )
                .await,
        );
        assert!(matches!(
            e,
            NativeRerankError::Shape | NativeRerankError::Encoding
        ));
        redacted(e);
        assert_eq!(fixture.server.await.unwrap().len(), 1);
    }
    for value in [
        rows(1),
        json!({"results":[{"index":0,"relevance_score":1},{"index":0,"relevance_score":2}]}),
    ] {
        let fixture = fixture(vec![reply(value)]).await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/rerank", fixture.origin);
        let cancel = Cancellation::default();
        assert_eq!(
            error(
                adapter(&client, &endpoint, limits())
                    .score("q", &["a", "b"], &key(), context(&cancel))
                    .await
            ),
            NativeRerankError::Shape
        );
        fixture.server.await.unwrap();
    }
}
#[tokio::test]
async fn real_status_refused_origin_and_body_caps_remain_typed_redacted() {
    for status in [401, 429, 503] {
        let fixture = fixture(vec![Reply {
            status,
            body: "provider-secret-canary".to_owned(),
            delay: Duration::ZERO,
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/rerank", fixture.origin);
        let cancel = Cancellation::default();
        let e = error(
            adapter(&client, &endpoint, limits())
                .score(
                    "fixture-query-canary",
                    &["fixture-input-canary"],
                    &key(),
                    context(&cancel),
                )
                .await,
        );
        assert_eq!(
            e,
            NativeRerankError::Status(StatusCode::from_u16(status).unwrap())
        );
        redacted(e);
        fixture.server.await.unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = client(&origin);
    let endpoint = format!("{origin}/v2/rerank");
    let cancel = Cancellation::default();
    let e = error(
        adapter(&client, &endpoint, limits())
            .score("q", &["x"], &key(), context(&cancel))
            .await,
    );
    assert_eq!(e, NativeRerankError::Network(NetworkError::Connection));
    redacted(e);
    let wrong = endpoint.replace("127.0.0.1", "localhost");
    let e = error(
        adapter(&client, &wrong, limits())
            .score("q", &["x"], &key(), context(&cancel))
            .await,
    );
    assert_eq!(e, NativeRerankError::Network(NetworkError::Origin));
    redacted(e);
}
#[tokio::test]
async fn later_batch_failure_discards_scores_and_stops_before_third_request() {
    let fixture = fixture(vec![
        reply(rows(1)),
        Reply {
            status: 503,
            body: "provider-secret-canary".to_owned(),
            delay: Duration::ZERO,
        },
    ])
    .await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/v2/rerank", fixture.origin);
    let cancel = Cancellation::default();
    let policy = NativeRerankLimits::new(3, 1, 100, 1000, 1000).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoint, policy)
                .score("q", &["first", "second", "third"], &key(), context(&cancel))
                .await
        ),
        NativeRerankError::Status(StatusCode::SERVICE_UNAVAILABLE)
    );
    assert_eq!(fixture.server.await.unwrap().len(), 2);
}
#[tokio::test]
async fn stalled_body_preserves_signal_and_discriminates_absolute_batch_deadline() {
    for cancelled in [true, false] {
        let fixture = fixture(vec![Reply {
            status: 200,
            body: rows(1).to_string(),
            delay: Duration::from_millis(120),
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/rerank", fixture.origin);
        let cancel = Cancellation::default();
        let signal = cancel.clone();
        let observed = fixture.observed.clone();
        let trigger = tokio::spawn(async move {
            observed.notified().await;
            if cancelled {
                signal.cancel();
            }
        });
        let ctx = InferenceContext::new(&cancel, Instant::now() + Duration::from_millis(70));
        let started = Instant::now();
        let e = error(
            adapter(&client, &endpoint, limits())
                .score("q", &["x"], &key(), ctx)
                .await,
        );
        assert_eq!(
            e,
            NativeRerankError::Network(if cancelled {
                NetworkError::Cancelled
            } else {
                NetworkError::Deadline
            })
        );
        assert!(started.elapsed() < Duration::from_millis(120));
        redacted(e);
        trigger.await.unwrap();
        fixture.server.await.unwrap();
    }
    let mut first = reply(rows(1));
    first.delay = Duration::from_millis(60);
    let mut second = reply(rows(1));
    second.delay = Duration::from_millis(60);
    let fixture = fixture(vec![first, second]).await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/v2/rerank", fixture.origin);
    let cancel = Cancellation::default();
    let policy = NativeRerankLimits::new(3, 1, 100, 1000, 1000).unwrap();
    let ctx = InferenceContext::new(&cancel, Instant::now() + Duration::from_millis(100));
    assert_eq!(
        error(
            adapter(&client, &endpoint, policy)
                .score("q", &["a", "b", "c"], &key(), ctx)
                .await
        ),
        NativeRerankError::Network(NetworkError::Deadline)
    );
    assert_eq!(fixture.server.await.unwrap().len(), 2);
}
#[tokio::test]
async fn exact_utf8_request_and_response_caps_then_plus_one() {
    let request_bytes = json!({"model":"fixture-model","query":"é","documents":["x"],"top_n":1})
        .to_string()
        .len();
    let response_bytes = rows(1).to_string().len();
    for response_cap in [response_bytes, response_bytes - 1] {
        let fixture = fixture(vec![reply(rows(1))]).await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/rerank", fixture.origin);
        let cancel = Cancellation::default();
        let policy = NativeRerankLimits::new(1, 1, 3, request_bytes, response_cap).unwrap();
        let result = adapter(&client, &endpoint, policy)
            .score("é", &["x"], &key(), context(&cancel))
            .await;
        if response_cap == response_bytes {
            assert_eq!(result.unwrap(), [0.0]);
        } else {
            assert_eq!(error(result), NativeRerankError::Limit);
        }
        fixture.server.await.unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let client = client(&origin);
    let endpoint = format!("{origin}/v2/rerank");
    let cancel = Cancellation::default();
    let policy = NativeRerankLimits::new(1, 1, 3, request_bytes, response_bytes).unwrap();
    for (q, docs) in [("éx", vec!["x"]), ("é", vec!["x", "y"])] {
        assert_eq!(
            error(
                adapter(&client, &endpoint, policy)
                    .score(q, &docs, &key(), context(&cancel))
                    .await
            ),
            NativeRerankError::Limit
        );
    }
    let low = NativeRerankLimits::new(1, 1, 3, request_bytes - 1, response_bytes).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoint, low)
                .score("é", &["x"], &key(), context(&cancel))
                .await
        ),
        NativeRerankError::Limit
    );
    for (e, m) in [("", "fixture-model"), (&endpoint, "")] {
        assert_eq!(
            error(CohereReranker::new(&client, e, m, policy)),
            NativeRerankError::Policy
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(25), listener.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
#[ignore = "loopback measurement; run precompiled in coordinated CPU window"]
async fn measure_cohere_rerank_loopback_fixture() {
    let fixture = fixture((0..128).map(|_| reply(rows(8))).collect()).await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/v2/rerank", fixture.origin);
    let cancel = Cancellation::default();
    let adapter = adapter(&client, &endpoint, limits());
    let mut samples = Vec::new();
    for _ in 0..128 {
        let start = Instant::now();
        let scores = adapter
            .score("query", &["x"; 8], &key(), context(&cancel))
            .await
            .unwrap();
        assert_eq!(scores.len(), 8);
        samples.push(start.elapsed().as_nanos());
    }
    let total: u128 = samples.iter().sum();
    samples.sort_unstable();
    println!(
        "loopback_samples=128 documents=8 queries=1 scalar_scores=8 total_ns={total} p95_ns={}",
        samples[121]
    );
    assert_eq!(fixture.server.await.unwrap().len(), 128);
}
