use graphit_ai::{
    context::{ContextError, InferenceContext},
    embedding::{EmbeddingError, EmbeddingLimits, GuardedEmbeddingError},
    openai::OpenAiEmbeddings,
};
use graphit_core::file_lock::Cancellation;
use graphit_http::client::{BoundedClient, ClientPolicy, Limits, NetworkEffect, NetworkError};
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
    sync::{Notify, oneshot},
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
    json!({"data":(0..count).rev().map(|i|json!({"index":i,"embedding":[i as f32,1.0]})).collect::<Vec<_>>(),"usage":{"tokens":1}})
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
    fixture_with_first_body_gate(replies, None).await
}
async fn fixture_with_first_body_gate(
    replies: Vec<Reply>,
    mut release: Option<oneshot::Receiver<()>>,
) -> Fixture {
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
            if let Some(gate) = release.take() {
                // Hold the first body until the client reports its terminal error.
                // Dropping the sender on a test failure also releases the peer.
                let _ = gate.await;
            }
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
) -> OpenAiEmbeddings<'a> {
    OpenAiEmbeddings::new(client, endpoint, "fixture-model", 2, None, policy).unwrap()
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
async fn exact_requests_and_scrambled_indices() {
    for (model, truncation) in [
        ("fixture-model", None),
        ("text-embedding-ada-002", None),
        ("text-embedding-3-small", Some(2)),
        ("text-embedding-3-large", Some(2)),
    ] {
        let fixture = fixture(vec![reply(rows(2))]).await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/embeddings?fixture-query-canary", fixture.origin);
        let adapter =
            OpenAiEmbeddings::new(&client, &endpoint, model, 2, truncation, limits()).unwrap();
        let cancel = Cancellation::default();
        let result = adapter
            .embed(
                &["fixture-input-canary", "second"],
                &key(),
                context(&cancel),
            )
            .await
            .unwrap();
        assert_eq!(result.model(), model);
        assert_eq!(result.dimensions(), 2);
        assert_eq!(result.vectors(), &[vec![0.0, 1.0], vec![1.0, 1.0]]);
        let requests = fixture.server.await.unwrap();
        assert_eq!(requests.len(), 1);
        let r = &requests[0];
        assert!(
            r.head
                .starts_with("POST /embeddings?fixture-query-canary HTTP/1.1\r\n")
        );
        assert!(
            r.head
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-key-canary\r\n")
        );
        let mut expected = json!({"model":model,"input":["fixture-input-canary","second"],"encoding_format":"float"});
        if let Some(width) = truncation {
            expected["dimensions"] = json!(width);
        }
        assert_eq!(r.body, expected);
    }
}
#[tokio::test]
async fn native_width_omits_extension_and_invalid_truncation_fails() {
    let fixture = fixture(vec![reply(
        json!({"data":[{"index":0,"embedding":vec![0.25;1536]}]}),
    )])
    .await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/embeddings", fixture.origin);
    let adapter = OpenAiEmbeddings::new(
        &client,
        &endpoint,
        "text-embedding-3-small",
        1536,
        None,
        limits(),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let vectors = adapter
        .embed(&["native"], &key(), context(&cancel))
        .await
        .unwrap();
    assert_eq!(vectors.vectors()[0].len(), 1536);
    let requests = fixture.server.await.unwrap();
    assert!(requests[0].body.get("dimensions").is_none());
    for (model, width) in [("fixture-model", 0), ("fixture-model", 3)] {
        assert_eq!(
            error(OpenAiEmbeddings::new(
                &client,
                &endpoint,
                model,
                2,
                Some(width),
                limits()
            )),
            EmbeddingError::InvalidRequest
        );
    }
}
#[tokio::test]
async fn batch_cap_is_2048_and_policy_can_lower_it() {
    for (count, batch) in [(2049, 3000), (5, 2)] {
        let sizes: Vec<_> = (0..count)
            .step_by(batch.min(2048))
            .map(|i| (count - i).min(batch.min(2048)))
            .collect();
        let fixture = fixture(sizes.iter().map(|n| reply(rows(*n))).collect()).await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/embeddings", fixture.origin);
        let policy = EmbeddingLimits::new(3000, 10000, 2, 6000, batch, 200000, 400000).unwrap();
        let adapter = adapter(&client, &endpoint, policy);
        let cancel = Cancellation::default();
        let texts = vec!["x"; count];
        let result = adapter
            .embed(&texts, &key(), context(&cancel))
            .await
            .unwrap();
        assert_eq!(result.vectors().len(), count);
        let requests = fixture.server.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .map(|r| r.body["input"].as_array().unwrap().len())
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
    let endpoint = format!("{origin}/embeddings");
    let cancel = Cancellation::default();
    let adapter = adapter(&client, &endpoint, limits());
    assert!(
        adapter
            .embed(&[], &key(), context(&cancel))
            .await
            .unwrap()
            .vectors()
            .is_empty()
    );
    let low = EmbeddingLimits::new(2, 2, 2, 4, 1, 90, 90).unwrap();
    let small = adapter_fn(&client, &endpoint, low);
    assert_eq!(
        error(small.embed(&["abc"], &key(), context(&cancel)).await),
        EmbeddingError::Limit
    );
    assert_eq!(
        error(
            small
                .embed(&["x", "y", "z"], &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Limit
    );
    // Input bytes fit, but escaping causes only the second encoded batch to exceed cap.
    let escaped = EmbeddingLimits::new(3, 100, 2, 6, 1, 90, 90).unwrap();
    let narrow = adapter_fn(&client, &endpoint, escaped);
    assert_eq!(
        error(
            narrow
                .embed(
                    &["x", "\u{1}".repeat(20).as_str()],
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
                .embed(&["x"], &SecretString::from(""), context(&cancel))
                .await
        ),
        EmbeddingError::Credential
    );
    assert_eq!(
        error(
            adapter
                .embed(&["x"], &SecretString::from("bad\r\nkey"), context(&cancel))
                .await
        ),
        EmbeddingError::Credential
    );
    let tensor_policy = EmbeddingLimits::new(3, 100, 2, 3, 2, 1000, 1000).unwrap();
    let tensor_bounded = adapter_fn(&client, &endpoint, tensor_policy);
    assert_eq!(
        error(
            tensor_bounded
                .embed(&["a", "b"], &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Limit
    );
    assert_eq!(
        error(OpenAiEmbeddings::new(
            &client,
            &endpoint,
            "fixture-model",
            3,
            None,
            tensor_policy
        )),
        EmbeddingError::Limit
    );
    let expired = InferenceContext::new(&cancel, Instant::now());
    assert_eq!(
        error(adapter.embed(&[], &key(), expired).await),
        EmbeddingError::Context(ContextError::Deadline)
    );
    cancel.cancel();
    assert_eq!(
        error(adapter.embed(&["x"], &key(), context(&cancel)).await),
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
) -> OpenAiEmbeddings<'a> {
    adapter(client, endpoint, limits)
}
#[tokio::test]
async fn malformed_or_incomplete_responses_return_only_errors() {
    let bodies = [
        "{",
        "{\"data\":[]}",
        "{\"data\":[{\"embedding\":[1,2]}]}",
        "{\"data\":[{\"index\":0}]}",
        "{\"data\":[{\"index\":-1,\"embedding\":[1,2]}]}",
        "{\"data\":[{\"index\":1,\"embedding\":[1,2]}]}",
        "{\"data\":[{\"index\":0,\"embedding\":[1]}]}",
        "{\"data\":[{\"index\":0,\"embedding\":[1,2,3]}]}",
        "{\"data\":[{\"index\":0,\"embedding\":[1e100,2]}]}",
        "{\"data\":[{\"index\":0,\"embedding\":[NaN,2]}]}",
        "{\"data\":[{\"index\":0,\"embedding\":[1,2]},{\"index\":0,\"embedding\":[1,2]}]}",
        "{\"data\":[],\"data\":[]}",
        "{\"data\":[{\"index\":0,\"index\":0,\"embedding\":[1,2]}]}",
        "{\"data\":[{\"index\":0,\"embedding\":[1,2],\"embedding\":[1,2]}]}",
        "{\"data\":[{\"index\":0,\"embedding\":[1,2]}]} trailing",
    ];
    for body in bodies {
        let fixture = fixture(vec![Reply {
            status: 200,
            body: body.to_owned(),
            delay: Duration::ZERO,
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/embeddings", fixture.origin);
        let cancel = Cancellation::default();
        let e = error(
            adapter(&client, &endpoint, limits())
                .embed(&["fixture-input-canary"], &key(), context(&cancel))
                .await,
        );
        assert!(matches!(
            e,
            EmbeddingError::Codec | EmbeddingError::Shape | EmbeddingError::NonFinite
        ));
        redacted(e);
        assert_eq!(fixture.server.await.unwrap().len(), 1);
    }
    let duplicate = fixture(vec![reply(
        json!({"data":[{"index":0,"embedding":[1,2]},{"index":0,"embedding":[3,4]}]}),
    )])
    .await;
    let client = client(&duplicate.origin);
    let endpoint = format!("{}/embeddings", duplicate.origin);
    let cancel = Cancellation::default();
    assert_eq!(
        error(
            adapter(&client, &endpoint, limits())
                .embed(&["a", "b"], &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Shape
    );
    duplicate.server.await.unwrap();
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
        let endpoint = format!("{}/embeddings", fixture.origin);
        let cancel = Cancellation::default();
        let e = error(
            adapter(&client, &endpoint, limits())
                .embed(&["fixture-input-canary"], &key(), context(&cancel))
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
    let endpoint = format!("{origin}/embeddings");
    let cancel = Cancellation::default();
    let e = error(
        adapter(&client, &endpoint, limits())
            .embed(&["x"], &key(), context(&cancel))
            .await,
    );
    assert_eq!(e, EmbeddingError::Network(NetworkError::Connection));
    redacted(e);
    let wrong = endpoint.replace("127.0.0.1", "localhost");
    assert_eq!(
        error(
            adapter(&client, &wrong, limits())
                .embed(&["x"], &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Network(NetworkError::Origin)
    );
    let fixture = fixture(vec![reply(rows(1))]).await;
    let client = client_fn(&fixture.origin);
    let endpoint = format!("{}/embeddings", fixture.origin);
    let policy = EmbeddingLimits::new(2, 100, 2, 4, 2, 1000, 10).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoint, policy)
                .embed(&["x"], &key(), context(&cancel))
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
    let endpoint = format!("{}/embeddings", fixture.origin);
    let cancel = Cancellation::default();
    let policy = EmbeddingLimits::new(3, 100, 2, 6, 1, 1000, 1000).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoint, policy)
                .embed(&["first", "second", "third"], &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Status(StatusCode::SERVICE_UNAVAILABLE)
    );
    assert_eq!(fixture.server.await.unwrap().len(), 2);
}
#[tokio::test]
async fn stalled_body_observes_shared_cancellation_and_original_deadline() {
    for cancelled in [true, false] {
        let (release, gate) = oneshot::channel();
        let fixture = fixture_with_first_body_gate(vec![reply(rows(1))], Some(gate)).await;
        let client = client(&fixture.origin);
        let endpoint = format!("{}/embeddings", fixture.origin);
        let cancel = Cancellation::default();
        let signal = cancel.clone();
        let observed = fixture.observed.clone();
        let trigger = tokio::spawn(async move {
            observed.notified().await;
            if cancelled {
                signal.cancel();
            }
        });
        let budget = if cancelled {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(250)
        };
        let context = InferenceContext::new(&cancel, Instant::now() + budget);
        let e = error(
            adapter(&client, &endpoint, limits())
                .embed(&["x"], &key(), context)
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
        redacted(e);
        release.send(()).unwrap();
        assert_eq!(fixture.server.await.unwrap().len(), 1);
        trigger.await.unwrap();
    }
    // The second request inherits the original deadline; no per-batch extension.
    let mut first = reply(rows(1));
    first.delay = Duration::from_millis(60);
    let mut second = reply(rows(1));
    // Body arrives around 120ms: after original100, before renewedHTTP160.
    // A renewed HTTP deadline would deliver the body and yield Context(Deadline),
    // rather than the required Network(Deadline).
    second.delay = Duration::from_millis(60);
    let fixture = fixture(vec![first, second]).await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/embeddings", fixture.origin);
    let cancel = Cancellation::default();
    let policy = EmbeddingLimits::new(3, 100, 2, 6, 1, 1000, 1000).unwrap();
    let context = InferenceContext::new(&cancel, Instant::now() + Duration::from_millis(100));
    let started = Instant::now();
    assert_eq!(
        error(
            adapter(&client, &endpoint, policy)
                .embed(&["a", "b", "c"], &key(), context)
                .await
        ),
        EmbeddingError::Network(NetworkError::Deadline)
    );
    assert!(started.elapsed() < Duration::from_millis(170));
    assert_eq!(fixture.server.await.unwrap().len(), 2);
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
#[ignore = "optional loopback throughput measurement"]
async fn measure_direct_loopback_fixture() {
    let fixture = fixture((0..128).map(|_| reply(rows(8))).collect()).await;
    let client = client(&fixture.origin);
    let endpoint = format!("{}/embeddings", fixture.origin);
    let cancel = Cancellation::default();
    let adapter = adapter(&client, &endpoint, limits());
    let mut samples = Vec::new();
    for _ in 0..128 {
        let start = Instant::now();
        let output = adapter
            .embed(&["x"; 8], &key(), context(&cancel))
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

#[tokio::test]
async fn guarded_admission_denies_before_send_and_after_response_without_vectors() {
    let f = fixture(vec![]).await;
    let transport = client(&f.origin);
    let policy = limits();
    let endpoint = format!("{}/literal", f.origin);
    let sdk = adapter(&transport, &endpoint, policy);
    let cancel = Cancellation::default();
    let auth = http::HeaderMap::from_iter([(
        http::HeaderName::from_static("authorization"),
        http::HeaderValue::from_static("Bearer fixture-key-canary"),
    )]);
    let mut denied = || Err::<(), _>("synthetic-admission-denial");
    let failure = sdk
        .embed_guarded(&["text"], &auth, context(&cancel), &mut denied)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure,
        GuardedEmbeddingError::Admission {
            effect: NetworkEffect::NotSent,
            ..
        }
    ));
    assert!(f.server.await.unwrap().is_empty());

    let f = fixture(vec![Reply {
        status: 200,
        body: rows(1).to_string(),
        delay: Duration::from_millis(30),
    }])
    .await;
    let transport = client(&f.origin);
    let policy = limits();
    let endpoint = format!("{}/literal", f.origin);
    let sdk = adapter(&transport, &endpoint, policy);
    let admitted = std::sync::atomic::AtomicBool::new(true);
    let mut gate = || {
        if admitted.load(std::sync::atomic::Ordering::SeqCst) {
            Ok(())
        } else {
            Err("synthetic-private-denial")
        }
    };
    let call = sdk.embed_guarded(&["text"], &auth, context(&cancel), &mut gate);
    let revoke = async {
        f.observed.notified().await;
        admitted.store(false, std::sync::atomic::Ordering::SeqCst);
    };
    let (result, ()) = tokio::join!(call, revoke);
    let failure = result.err().unwrap();
    assert!(matches!(
        failure,
        GuardedEmbeddingError::Admission {
            effect: NetworkEffect::MayHaveApplied,
            ..
        }
    ));
    let safe = format!("{failure:?} {failure}");
    for canary in [
        "fixture-key-canary",
        "synthetic-private-denial",
        "127.0.0.1",
    ] {
        assert!(!safe.contains(canary));
    }
    assert_eq!(f.server.await.unwrap().len(), 1);
}
