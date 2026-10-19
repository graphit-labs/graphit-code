use graphit_ai::{
    cohere_embedding::{CohereEmbeddings, CohereInput},
    context::{ContextError, InferenceContext},
    embedding::{EmbeddingError, EmbeddingLimits},
};
use graphit_core::file_lock::Cancellation;
use graphit_http::client::{BoundedClient, ClientPolicy, Limits, NetworkError};
use http::StatusCode;
use secrecy::SecretString;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::JoinHandle,
};

fn limits() -> EmbeddingLimits {
    EmbeddingLimits::new(10000, 200000, 4096, 100000, 2048, 200000, 400000).unwrap()
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
struct Request {
    head: String,
    body: Value,
}
struct Reply {
    status: u16,
    body: String,
    delay: Duration,
}
fn reply(value: Value) -> Reply {
    Reply {
        status: 200,
        body: value.to_string(),
        delay: Duration::ZERO,
    }
}
fn rows(count: usize) -> Value {
    json!({"embeddings":{"float":(0..count).map(|i|vec![i as f32,1.0]).collect::<Vec<_>>()},"meta":{"tokens":1}})
}
async fn read_request(socket: &mut TcpStream) -> Request {
    let mut bytes = Vec::new();
    let end = loop {
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let mut buf = [0; 1024];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() < 210000);
    };
    let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let length: usize = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .map(str::to_owned)
        })
        .unwrap()
        .parse()
        .unwrap();
    while bytes.len() < end + length {
        let mut buf = [0; 4096];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() <= 210000);
    }
    Request {
        head,
        body: serde_json::from_slice(&bytes[end..end + length]).unwrap(),
    }
}
struct Fixture {
    origin: String,
    observed: Arc<Notify>,
    server: JoinHandle<Vec<Request>>,
}
async fn fixture(replies: Vec<Reply>) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let observed = Arc::new(Notify::new());
    let signal = observed.clone();
    let server = tokio::spawn(async move {
        let mut requests = Vec::new();
        for reply in replies {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
            requests.push(read_request(&mut socket).await);
            let header = format!(
                "HTTP/1.1 {} fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.status,
                reply.body.len()
            );
            socket.write_all(header.as_bytes()).await.unwrap();
            signal.notify_one();
            if !reply.delay.is_zero() {
                tokio::time::sleep(reply.delay).await;
            }
            let _ = socket.write_all(reply.body.as_bytes()).await;
        }
        // A failure on a later batch must not send the following batch.
        assert!(
            tokio::time::timeout(Duration::from_millis(25), listener.accept())
                .await
                .is_err()
        );
        requests
    });
    Fixture {
        origin,
        observed,
        server,
    }
}
fn adapter<'a>(
    client: &'a BoundedClient,
    endpoint: &'a str,
    policy: EmbeddingLimits,
) -> CohereEmbeddings<'a> {
    CohereEmbeddings::new(client, endpoint, "fixture-model", 2, policy).unwrap()
}
fn error<T>(result: Result<T, EmbeddingError>) -> EmbeddingError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("expected embedding error"),
    }
}
fn redacted(e: EmbeddingError) {
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
async fn exact_wire_and_positional_document_query_use_per_call_credentials() {
    let fixture = fixture(vec![reply(rows(2)), reply(rows(1))]).await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/v2/embed?fixture-query-canary", fixture.origin);
    let adapter = adapter(&client, &endpoint, limits());
    let cancel = Cancellation::default();
    let output = adapter
        .embed(
            &["fixture-input-canary", "second"],
            CohereInput::Document,
            &key(),
            context(&cancel),
        )
        .await
        .unwrap();
    assert_eq!(output.model(), "fixture-model");
    assert_eq!(output.dimensions(), 2);
    assert_eq!(output.vectors(), &[vec![0.0, 1.0], vec![1.0, 1.0]]);
    let output = adapter
        .embed(
            &["query"],
            CohereInput::Query,
            &SecretString::from("second-synthetic-key"),
            context(&cancel),
        )
        .await
        .unwrap();
    assert_eq!(output.vectors(), &[vec![0.0, 1.0]]);
    let requests = fixture.server.await.unwrap();
    assert_eq!(requests.len(), 2);
    for (r, input, texts, bearer) in [
        (
            &requests[0],
            "search_document",
            json!(["fixture-input-canary", "second"]),
            "fixture-key-canary",
        ),
        (
            &requests[1],
            "search_query",
            json!(["query"]),
            "second-synthetic-key",
        ),
    ] {
        assert!(
            r.head
                .starts_with("POST /v2/embed?fixture-query-canary HTTP/1.1\r\n")
        );
        assert!(
            r.head
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {bearer}\r\n"))
        );
        assert_eq!(
            r.body,
            json!({"model":"fixture-model","texts":texts,"input_type":input,"embedding_types":["float"]})
        );
        assert!(r.body.get("output_dimension").is_none());
    }
}
#[tokio::test]
async fn batch_cap_is_96_and_policy_can_lower_it() {
    for (count, batch) in [(97, 3000), (5, 2)] {
        let sizes: Vec<_> = (0..count)
            .step_by(batch.min(96))
            .map(|i| (count - i).min(batch.min(96)))
            .collect();
        let fixture = fixture(sizes.iter().map(|n| reply(rows(*n))).collect()).await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/embed", fixture.origin);
        let policy = EmbeddingLimits::new(3000, 10000, 2, 6000, batch, 200000, 400000).unwrap();
        let adapter = adapter(&client, &endpoint, policy);
        let cancel = Cancellation::default();
        let texts = vec!["x"; count];
        let result = adapter
            .embed(&texts, CohereInput::Document, &key(), context(&cancel))
            .await
            .unwrap();
        assert_eq!(result.vectors().len(), count);
        let requests = fixture.server.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .map(|r| r.body["texts"].as_array().unwrap().len())
                .collect::<Vec<_>>(),
            sizes
        );
        let mut offset = 0;
        for size in sizes {
            for i in 0..size {
                assert_eq!(result.vectors()[offset + i], vec![i as f32, 1.0]);
            }
            offset += size;
        }
    }
}
#[tokio::test]
async fn empty_and_preflight_failures_have_zero_socket_effects() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let client = client(&origin);
    let endpoint = format!("{origin}/v2/embed");
    let cancel = Cancellation::default();
    let adapter = adapter(&client, &endpoint, limits());
    assert!(
        adapter
            .embed(&[], CohereInput::Document, &key(), context(&cancel))
            .await
            .unwrap()
            .vectors()
            .is_empty()
    );
    let low = EmbeddingLimits::new(2, 2, 2, 4, 1, 1000, 1000).unwrap();
    let small = adapter_fn(&client, &endpoint, low);
    assert_eq!(
        error(
            small
                .embed(&["abc"], CohereInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Limit
    );
    assert_eq!(
        error(
            small
                .embed(
                    &["x", "y", "z"],
                    CohereInput::Document,
                    &key(),
                    context(&cancel)
                )
                .await
        ),
        EmbeddingError::Limit
    );
    // Input bytes fit, but escaping causes only the second encoded batch to exceed cap.
    let cap = json!({"model":"fixture-model","texts":["x"],"input_type":"search_document","embedding_types":["float"]}).to_string().len();
    let escaped = EmbeddingLimits::new(3, 100, 2, 6, 1, cap, 1000).unwrap();
    let narrow = adapter_fn(&client, &endpoint, escaped);
    assert_eq!(
        error(
            narrow
                .embed(
                    &["x", "\u{1}".repeat(20).as_str()],
                    CohereInput::Document,
                    &key(),
                    context(&cancel)
                )
                .await
        ),
        EmbeddingError::Limit
    );
    assert_eq!(
        error(
            adapter
                .embed(
                    &["x"],
                    CohereInput::Document,
                    &SecretString::from(""),
                    context(&cancel)
                )
                .await
        ),
        EmbeddingError::Credential
    );
    assert_eq!(
        error(
            adapter
                .embed(
                    &["x"],
                    CohereInput::Document,
                    &SecretString::from("bad\r\nkey"),
                    context(&cancel)
                )
                .await
        ),
        EmbeddingError::Credential
    );
    let tensor_policy = EmbeddingLimits::new(3, 100, 2, 3, 2, 1000, 1000).unwrap();
    let tensor_bounded = adapter_fn(&client, &endpoint, tensor_policy);
    assert_eq!(
        error(
            tensor_bounded
                .embed(&["a", "b"], CohereInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Limit
    );
    assert_eq!(
        error(CohereEmbeddings::new(
            &client,
            &endpoint,
            "fixture-model",
            3,
            tensor_policy
        )),
        EmbeddingError::Limit
    );
    let expired = InferenceContext::new(&cancel, Instant::now());
    assert_eq!(
        error(
            adapter
                .embed(&[], CohereInput::Document, &key(), expired)
                .await
        ),
        EmbeddingError::Context(ContextError::Deadline)
    );
    cancel.cancel();
    assert_eq!(
        error(
            adapter
                .embed(&["x"], CohereInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Context(ContextError::Cancelled)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(25), listener.accept())
            .await
            .is_err()
    );
}
fn adapter_fn<'a>(
    client: &'a BoundedClient,
    endpoint: &'a str,
    limits: EmbeddingLimits,
) -> CohereEmbeddings<'a> {
    adapter(client, endpoint, limits)
}
#[tokio::test]
async fn malformed_missing_duplicate_and_excess_shapes_return_only_errors() {
    let bodies = [
        "{",
        "{}",
        "{\"embeddings\":{}}",
        "{\"embeddings\":{\"float\":[]}}",
        "{\"embeddings\":[]}",
        "{\"embeddings\":{\"float\":{}}}",
        "{\"embeddings\":{\"float\":[[1]]}}",
        "{\"embeddings\":{\"float\":[[1,2,3]]}}",
        "{\"embeddings\":{\"float\":[[1e100,2]]}}",
        "{\"embeddings\":{\"float\":[[NaN,2]]}}",
        "{\"embeddings\":{\"float\":[[1,2],[3,4]]}}",
        "{\"embeddings\":{\"float\":[[1,2]],\"float\":[[3,4]]}}",
        "{\"embeddings\":{\"float\":[[1,2]]},\"embeddings\":{\"float\":[[3,4]]}}",
        "{\"embeddings\":{\"float\":[[1,2]]}} trailing",
    ];
    for body in bodies {
        let fixture = fixture(vec![Reply {
            status: 200,
            body: body.to_owned(),
            delay: Duration::ZERO,
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/embed", fixture.origin);
        let cancel = Cancellation::default();
        let e = error(
            adapter(&client, &endpoint, limits())
                .embed(
                    &["fixture-input-canary"],
                    CohereInput::Document,
                    &key(),
                    context(&cancel),
                )
                .await,
        );
        assert!(matches!(
            e,
            EmbeddingError::Codec | EmbeddingError::Shape | EmbeddingError::NonFinite
        ));
        redacted(e);
        assert_eq!(fixture.server.await.unwrap().len(), 1);
    }
    let fixture = fixture(vec![reply(rows(1))]).await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/v2/embed", fixture.origin);
    let cancel = Cancellation::default();
    assert_eq!(
        error(
            adapter(&client, &endpoint, limits())
                .embed(&["a", "b"], CohereInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Shape
    );
    fixture.server.await.unwrap();
}
#[tokio::test]
async fn real_status_transport_origin_and_response_limits_remain_typed() {
    for status in [401, 429, 503] {
        let fixture = fixture(vec![Reply {
            status,
            body: "provider-secret-canary".to_owned(),
            delay: Duration::ZERO,
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/embed", fixture.origin);
        let cancel = Cancellation::default();
        let e = error(
            adapter(&client, &endpoint, limits())
                .embed(
                    &["fixture-input-canary"],
                    CohereInput::Document,
                    &key(),
                    context(&cancel),
                )
                .await,
        );
        assert_eq!(
            e,
            EmbeddingError::Status(StatusCode::from_u16(status).unwrap())
        );
        redacted(e);
        fixture.server.await.unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = client(&origin);
    let endpoint = format!("{origin}/v2/embed");
    let cancel = Cancellation::default();
    let e = error(
        adapter(&client, &endpoint, limits())
            .embed(&["x"], CohereInput::Document, &key(), context(&cancel))
            .await,
    );
    assert_eq!(e, EmbeddingError::Network(NetworkError::Connection));
    redacted(e);
    let wrong = endpoint.replace("127.0.0.1", "localhost");
    assert_eq!(
        error(
            adapter(&client, &wrong, limits())
                .embed(&["x"], CohereInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Network(NetworkError::Origin)
    );
    let fixture = fixture(vec![reply(rows(1))]).await;
    let client = client_fn(&fixture.origin);
    let endpoint = format!("{}/v2/embed", fixture.origin);
    let policy = EmbeddingLimits::new(2, 100, 2, 4, 2, 1000, 10).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoint, policy)
                .embed(&["x"], CohereInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Limit
    );
    fixture.server.await.unwrap();
}
fn client_fn(origin: &str) -> BoundedClient {
    client(origin)
}
#[tokio::test]
async fn later_batch_failure_exposes_no_partial_result_and_stops_requests() {
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
    let endpoint = format!("{}/v2/embed", fixture.origin);
    let cancel = Cancellation::default();
    let policy = EmbeddingLimits::new(3, 100, 2, 6, 1, 1000, 1000).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoint, policy)
                .embed(
                    &["first", "second", "third"],
                    CohereInput::Document,
                    &key(),
                    context(&cancel)
                )
                .await
        ),
        EmbeddingError::Status(StatusCode::SERVICE_UNAVAILABLE)
    );
    assert_eq!(fixture.server.await.unwrap().len(), 2);
}
#[tokio::test]
async fn stalled_body_observes_shared_cancellation_and_original_deadline() {
    for cancelled in [true, false] {
        let fixture = fixture(vec![Reply {
            status: 200,
            body: rows(1).to_string(),
            delay: Duration::from_millis(120),
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/embed", fixture.origin);
        let cancel = Cancellation::default();
        let signal = cancel.clone();
        let observed = fixture.observed.clone();
        let trigger = tokio::spawn(async move {
            observed.notified().await;
            if cancelled {
                signal.cancel();
            }
        });
        let context = InferenceContext::new(&cancel, Instant::now() + Duration::from_millis(70));
        let started = Instant::now();
        let e = error(
            adapter(&client, &endpoint, limits())
                .embed(&["x"], CohereInput::Document, &key(), context)
                .await,
        );
        assert_eq!(
            e,
            EmbeddingError::Network(if cancelled {
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
    // The second request inherits the original deadline; no per-batch extension.
    let mut first = reply(rows(1));
    first.delay = Duration::from_millis(60);
    let mut second = reply(rows(1));
    // Body arrives around 120ms: after original100, before renewedHTTP160.
    // Renewal delivers the body and yields Context(Deadline), not Network(Deadline).
    second.delay = Duration::from_millis(60);
    let fixture = fixture(vec![first, second]).await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/v2/embed", fixture.origin);
    let cancel = Cancellation::default();
    let policy = EmbeddingLimits::new(3, 100, 2, 6, 1, 1000, 1000).unwrap();
    let context = InferenceContext::new(&cancel, Instant::now() + Duration::from_millis(100));
    let started = Instant::now();
    assert_eq!(
        error(
            adapter(&client, &endpoint, policy)
                .embed(&["a", "b", "c"], CohereInput::Document, &key(), context)
                .await
        ),
        EmbeddingError::Network(NetworkError::Deadline)
    );
    assert!(started.elapsed() < Duration::from_millis(170));
    assert_eq!(fixture.server.await.unwrap().len(), 2);
}
#[tokio::test]
async fn exact_byte_and_tensor_bounds_accept_then_reject_next_value() {
    let request_bytes = json!({"model":"fixture-model","texts":["é"],"input_type":"search_document","embedding_types":["float"]}).to_string().len();
    let response_bytes = rows(1).to_string().len();
    for response_cap in [response_bytes, response_bytes - 1] {
        let fixture = fixture(vec![reply(rows(1))]).await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/v2/embed", fixture.origin);
        let cancel = Cancellation::default();
        let policy = EmbeddingLimits::new(1, 2, 2, 2, 1, request_bytes, response_cap).unwrap();
        let result = adapter(&client, &endpoint, policy)
            .embed(&["é"], CohereInput::Document, &key(), context(&cancel))
            .await;
        if response_cap == response_bytes {
            assert_eq!(result.unwrap().vectors(), &[vec![0.0, 1.0]]);
        } else {
            assert_eq!(error(result), EmbeddingError::Limit);
        }
        assert_eq!(fixture.server.await.unwrap().len(), 1);
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let client = client(&origin);
    let endpoint = format!("{origin}/v2/embed");
    let cancel = Cancellation::default();
    let exact = EmbeddingLimits::new(1, 2, 2, 2, 1, request_bytes, response_bytes).unwrap();
    let bounded = adapter(&client, &endpoint, exact);
    for texts in [vec!["éx"], vec!["a", "b"]] {
        assert_eq!(
            error(
                bounded
                    .embed(&texts, CohereInput::Document, &key(), context(&cancel))
                    .await
            ),
            EmbeddingError::Limit
        );
    }
    let smaller = EmbeddingLimits::new(1, 2, 2, 2, 1, request_bytes - 1, response_bytes).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoint, smaller)
                .embed(&["é"], CohereInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Limit
    );
    for (endpoint_value, model, width, expected) in [
        (
            endpoint.as_str(),
            "fixture-model",
            0,
            EmbeddingError::InvalidRequest,
        ),
        (endpoint.as_str(), "fixture-model", 3, EmbeddingError::Limit),
        ("", "fixture-model", 2, EmbeddingError::InvalidRequest),
        (endpoint.as_str(), "", 2, EmbeddingError::InvalidRequest),
    ] {
        assert_eq!(
            error(CohereEmbeddings::new(
                &client,
                endpoint_value,
                model,
                width,
                exact
            )),
            expected
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(25), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn invalid_policies_reject_zero_and_unrepresentable_caps() {
    for index in 0..7 {
        let mut fields = [10, 100, 2, 20, 2, 1000, 1000];
        fields[index] = 0;
        assert_eq!(
            EmbeddingLimits::new(
                fields[0], fields[1], fields[2], fields[3], fields[4], fields[5], fields[6]
            )
            .unwrap_err(),
            EmbeddingError::InvalidPolicy
        );
    }
    assert_eq!(
        EmbeddingLimits::new(usize::MAX, 10, 2, 20, 2, 1000, 1000).unwrap_err(),
        EmbeddingError::InvalidPolicy
    );
}
#[tokio::test]
#[ignore = "loopback measurement; run precompiled in the coordinated CPU window"]
async fn measure_cohere_loopback_fixture() {
    let fixture = fixture((0..128).map(|_| reply(rows(8))).collect()).await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/v2/embed", fixture.origin);
    let cancel = Cancellation::default();
    let adapter = adapter(&client, &endpoint, limits());
    let mut samples = Vec::new();
    for _ in 0..128 {
        let start = Instant::now();
        let output = adapter
            .embed(&["x"; 8], CohereInput::Document, &key(), context(&cancel))
            .await
            .unwrap();
        assert_eq!(output.vectors().len(), 8);
        samples.push(start.elapsed().as_nanos());
    }
    let total: u128 = samples.iter().sum();
    samples.sort_unstable();
    println!(
        "loopback_samples=128 texts=8 dimensions=2 total_ns={total} p95_ns={}",
        samples[121]
    );
    assert_eq!(fixture.server.await.unwrap().len(), 128);
}
