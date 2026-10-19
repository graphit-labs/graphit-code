use graphit_ai::{
    context::{ContextError, InferenceContext},
    embedding::{EmbeddingError, EmbeddingLimits, GuardedEmbeddingError},
    google_embedding::{GoogleEmbeddings, GoogleEndpoints, GoogleFamily, GoogleInput},
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
    let vectors: Vec<Value> = (0..count)
        .map(|i| json!({"values":[i as f32,1.0],"shape":[1,2]}))
        .collect();
    if count == 1 {
        json!({"embedding":vectors[0],"usageMetadata":{"tokens":1}})
    } else {
        json!({"embeddings":vectors,"usageMetadata":{"tokens":1}})
    }
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

struct Endpoints {
    single: String,
    batch: String,
}
fn endpoints(origin: &str) -> Endpoints {
    Endpoints {
        single: format!("{origin}/v1beta/models/fixture-model:embedContent"),
        batch: format!("{origin}/v1beta/models/fixture-model:batchEmbedContents"),
    }
}
fn adapter<'a>(
    client: &'a BoundedClient,
    endpoints: &'a Endpoints,
    policy: EmbeddingLimits,
    family: GoogleFamily,
) -> GoogleEmbeddings<'a> {
    GoogleEmbeddings::new(
        client,
        GoogleEndpoints {
            single: &endpoints.single,
            batch: &endpoints.batch,
        },
        "models/fixture-model",
        2,
        Some(2),
        family,
        policy,
    )
    .unwrap()
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
        "127.0.0.1",
        "provider-secret-canary",
    ] {
        assert!(!diagnostic.contains(canary));
    }
}
fn config(family: GoogleFamily, input: GoogleInput) -> Value {
    let mut config = json!({"outputDimensionality":2});
    if family == GoogleFamily::Gemini1 {
        config["taskType"] = json!(if input == GoogleInput::Document {
            "RETRIEVAL_DOCUMENT"
        } else {
            "RETRIEVAL_QUERY"
        });
    }
    config
}
fn text(family: GoogleFamily, input: GoogleInput, raw: &str) -> String {
    let prefix = if family == GoogleFamily::Gemini1 {
        ""
    } else if input == GoogleInput::Document {
        "title: none | text: "
    } else {
        "task: search result | query: "
    };
    format!("{prefix}{raw}")
}
fn request(family: GoogleFamily, input: GoogleInput, texts: &[&str]) -> Value {
    let items: Vec<_> = texts.iter().map(|raw| json!({"model":"models/fixture-model","content":{"parts":[{"text":text(family,input,raw)}]},"embedContentConfig":config(family,input)})).collect();
    if items.len() == 1 {
        json!({"content":items[0]["content"],"embedContentConfig":items[0]["embedContentConfig"]})
    } else {
        json!({"requests":items})
    }
}

#[tokio::test]
async fn exact_single_batch_family_document_query_and_per_call_credentials() {
    for family in [GoogleFamily::Gemini1, GoogleFamily::Gemini2] {
        let fixture = fixture(vec![reply(rows(2)), reply(rows(1))]).await;
        let client = client(&fixture.origin);
        let endpoints = endpoints(&fixture.origin);
        let adapter = adapter(&client, &endpoints, limits(), family);
        let cancel = Cancellation::default();
        let output = adapter
            .embed(
                &["fixture-input-canary", "second"],
                GoogleInput::Document,
                &key(),
                context(&cancel),
            )
            .await
            .unwrap();
        assert_eq!(output.model(), "models/fixture-model");
        assert_eq!(output.dimensions(), 2);
        assert_eq!(output.vectors(), &[vec![0.0, 1.0], vec![1.0, 1.0]]);
        let output = adapter
            .embed(
                &["query"],
                GoogleInput::Query,
                &SecretString::from("second-synthetic-key"),
                context(&cancel),
            )
            .await
            .unwrap();
        assert_eq!(output.vectors(), &[vec![0.0, 1.0]]);
        let requests = fixture.server.await.unwrap();
        for (r, input, texts, key_value, operation) in [
            (
                &requests[0],
                GoogleInput::Document,
                vec!["fixture-input-canary", "second"],
                "fixture-key-canary",
                "batchEmbedContents",
            ),
            (
                &requests[1],
                GoogleInput::Query,
                vec!["query"],
                "second-synthetic-key",
                "embedContent",
            ),
        ] {
            assert!(r.body == request(family, input, &texts));
            let lower = r.head.to_ascii_lowercase();
            assert!(lower.contains(&format!("x-goog-api-key: {key_value}\r\n")));
            assert!(!lower.contains("authorization:"));
            assert!(r.head.starts_with(&format!(
                "POST /v1beta/models/fixture-model:{operation} HTTP/1.1\r\n"
            )));
            assert!(!r.head.contains('?'));
        }
    }
    // Also cover Query batch and Document single for each family.
    for family in [GoogleFamily::Gemini1, GoogleFamily::Gemini2] {
        let fixture = fixture(vec![reply(rows(2)), reply(rows(1))]).await;
        let client = client(&fixture.origin);
        let endpoints = endpoints(&fixture.origin);
        let cancel = Cancellation::default();
        for (texts, input) in [
            (vec!["q1", "q2"], GoogleInput::Query),
            (vec!["doc"], GoogleInput::Document),
        ] {
            adapter(&client, &endpoints, limits(), family)
                .embed(&texts, input, &key(), context(&cancel))
                .await
                .unwrap();
        }
        let requests = fixture.server.await.unwrap();
        assert!(requests[0].body == request(family, GoogleInput::Query, &["q1", "q2"]));
        assert!(requests[1].body == request(family, GoogleInput::Document, &["doc"]));
    }
}
#[tokio::test]
async fn captured_batch_policy_100_101_102_and_lower_caps() {
    for (count, cap) in [(100, 100), (101, 100), (102, 100), (5, 2), (101, 101)] {
        let chunks: Vec<usize> = (0..count)
            .step_by(cap)
            .map(|i| (count - i).min(cap))
            .collect();
        let fixture = fixture(chunks.iter().map(|n| reply(rows(*n))).collect()).await;
        let client = client(&fixture.origin);
        let endpoints = endpoints(&fixture.origin);
        let cancel = Cancellation::default();
        let policy =
            EmbeddingLimits::new(10000, 200000, 4096, 100000, cap, 200000, 400000).unwrap();
        let texts: Vec<_> = (0..count).map(|i| format!("input-{i}")).collect();
        let borrowed: Vec<_> = texts.iter().map(String::as_str).collect();
        let output = adapter(&client, &endpoints, policy, GoogleFamily::Gemini1)
            .embed(&borrowed, GoogleInput::Document, &key(), context(&cancel))
            .await
            .unwrap();
        let expected: Vec<_> = chunks
            .iter()
            .flat_map(|n| (0..*n).map(|i| vec![i as f32, 1.0]))
            .collect();
        assert_eq!(output.vectors(), expected);
        let requests = fixture.server.await.unwrap();
        assert_eq!(requests.len(), chunks.len());
        let mut offset = 0;
        for (r, n) in requests.iter().zip(chunks) {
            assert!(
                r.body
                    == request(
                        GoogleFamily::Gemini1,
                        GoogleInput::Document,
                        &borrowed[offset..offset + n]
                    )
            );
            offset += n;
        }
    }
}
#[tokio::test]
async fn empty_preflight_context_credentials_and_late_encoding_have_zero_sockets() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let client = client(&origin);
    let endpoints = endpoints(&origin);
    let cancel = Cancellation::default();
    let a = adapter(&client, &endpoints, limits(), GoogleFamily::Gemini1);
    assert!(
        a.embed(
            &[],
            GoogleInput::Document,
            &SecretString::from(""),
            context(&cancel)
        )
        .await
        .unwrap()
        .vectors()
        .is_empty()
    );
    for key_value in ["", "bad\r\nheader"] {
        assert_eq!(
            error(
                a.embed(
                    &["x"],
                    GoogleInput::Document,
                    &SecretString::from(key_value),
                    context(&cancel)
                )
                .await
            ),
            EmbeddingError::Credential
        );
    }
    let cap = request(GoogleFamily::Gemini1, GoogleInput::Document, &["x"])
        .to_string()
        .len();
    let policy = EmbeddingLimits::new(3, 100, 2, 6, 1, cap, 1000).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoints, policy, GoogleFamily::Gemini1)
                .embed(
                    &["x", "\n\n\n\n\n"],
                    GoogleInput::Document,
                    &key(),
                    context(&cancel)
                )
                .await
        ),
        EmbeddingError::Limit
    );
    let policy = EmbeddingLimits::new(2, 100, 2, 2, 2, 1000, 1000).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoints, policy, GoogleFamily::Gemini1)
                .embed(&["x", "y"], GoogleInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Limit
    );
    let expired = InferenceContext::new(&cancel, Instant::now());
    assert_eq!(
        error(a.embed(&[], GoogleInput::Document, &key(), expired).await),
        EmbeddingError::Context(ContextError::Deadline)
    );
    cancel.cancel();
    assert_eq!(
        error(
            a.embed(&["x"], GoogleInput::Document, &key(), context(&cancel))
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
#[tokio::test]
async fn required_null_duplicate_count_width_and_nonfinite_responses_fail() {
    let single = [
        "{",
        "{}",
        r#"{"embedding":null}"#,
        r#"{"embedding":[]}"#,
        r#"{"embedding":{}}"#,
        r#"{"embedding":{"values":null}}"#,
        r#"{"embedding":{"values":{}}}"#,
        r#"{"embedding":{"values":[null,2]}}"#,
        r#"{"embedding":{"values":[1]}}"#,
        r#"{"embedding":{"values":[1,2,3]}}"#,
        r#"{"embedding":{"values":[1e100,2]}}"#,
        r#"{"embedding":{"values":[NaN,2]}}"#,
        r#"{"embedding":{"values":[1,2],"values":[3,4]}}"#,
        r#"{"embedding":{"values":[1,2]},"embedding":{"values":[3,4]}}"#,
        r#"{"embedding":{"values":[1,2]}} trailing"#,
    ];
    let batch = [
        "{}",
        r#"{"embeddings":null}"#,
        r#"{"embeddings":{}}"#,
        r#"{"embeddings":[]}"#,
        r#"{"embeddings":[null,{}]}"#,
        r#"{"embeddings":[{},{}]}"#,
        r#"{"embeddings":[{"values":null},{"values":[1,2]}]}"#,
        r#"{"embeddings":[{"values":[1,2]}]}"#,
        r#"{"embeddings":[{"values":[1,2]},{"values":[3,4]},{"values":[5,6]}]}"#,
        r#"{"embeddings":[{"values":[1,2]},{"values":[3]}]}"#,
        r#"{"embeddings":[{"values":[1,2]},{"values":[1e100,2]}]}"#,
        r#"{"embeddings":[],"embeddings":[]}"#,
        r#"{"embeddings":[{"values":[1,2]},{"values":[3,4]}]} trailing"#,
    ];
    for (bodies, texts) in [
        (single.as_slice(), vec!["fixture-input-canary"]),
        (batch.as_slice(), vec!["fixture-input-canary", "second"]),
    ] {
        for body in bodies {
            let fixture = fixture(vec![Reply {
                status: 200,
                body: (*body).to_owned(),
                delay: Duration::ZERO,
            }])
            .await;
            let client = client(&fixture.origin);
            let endpoints = endpoints(&fixture.origin);
            let cancel = Cancellation::default();
            let e = error(
                adapter(&client, &endpoints, limits(), GoogleFamily::Gemini1)
                    .embed(&texts, GoogleInput::Document, &key(), context(&cancel))
                    .await,
            );
            assert!(matches!(
                e,
                EmbeddingError::Codec | EmbeddingError::Shape | EmbeddingError::NonFinite
            ));
            redacted(e);
            assert_eq!(fixture.server.await.unwrap().len(), 1);
        }
    }
}
#[tokio::test]
async fn status_transport_origin_response_caps_and_redacted_errors() {
    for status in [401, 429, 503] {
        let fixture = fixture(vec![Reply {
            status,
            body: "provider-secret-canary".to_owned(),
            delay: Duration::ZERO,
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoints = endpoints(&fixture.origin);
        let cancel = Cancellation::default();
        let e = error(
            adapter(&client, &endpoints, limits(), GoogleFamily::Gemini1)
                .embed(
                    &["fixture-input-canary"],
                    GoogleInput::Document,
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
    let endpoints = endpoints(&origin);
    let cancel = Cancellation::default();
    let e = error(
        adapter(&client, &endpoints, limits(), GoogleFamily::Gemini1)
            .embed(&["x"], GoogleInput::Document, &key(), context(&cancel))
            .await,
    );
    assert_eq!(e, EmbeddingError::Network(NetworkError::Connection));
    redacted(e);
    let wrong = Endpoints {
        single: endpoints.single.replace("127.0.0.1", "localhost"),
        batch: endpoints.batch.replace("127.0.0.1", "localhost"),
    };
    assert_eq!(
        error(
            adapter(&client, &wrong, limits(), GoogleFamily::Gemini1)
                .embed(&["x"], GoogleInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Network(NetworkError::Origin)
    );
    let fixture = fixture(vec![reply(rows(1))]).await;
    let client = client_fn(&fixture.origin);
    let endpoints = endpoints_fn(&fixture.origin);
    let policy = EmbeddingLimits::new(2, 100, 2, 4, 2, 1000, 10).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoints, policy, GoogleFamily::Gemini1)
                .embed(&["x"], GoogleInput::Document, &key(), context(&cancel))
                .await
        ),
        EmbeddingError::Limit
    );
    fixture.server.await.unwrap();
}
fn client_fn(origin: &str) -> BoundedClient {
    client(origin)
}
fn endpoints_fn(origin: &str) -> Endpoints {
    endpoints(origin)
}
#[tokio::test]
async fn later_failure_has_no_partial_result_or_third_request() {
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
    let endpoints = endpoints(&fixture.origin);
    let cancel = Cancellation::default();
    let policy = EmbeddingLimits::new(3, 100, 2, 6, 1, 1000, 1000).unwrap();
    assert_eq!(
        error(
            adapter(&client, &endpoints, policy, GoogleFamily::Gemini1)
                .embed(
                    &["first", "second", "third"],
                    GoogleInput::Document,
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
async fn shared_cancellation_stalled_body_and_original_interbatch_deadline() {
    for cancelled in [true, false] {
        let fixture = fixture(vec![Reply {
            status: 200,
            body: rows(1).to_string(),
            delay: Duration::from_millis(120),
        }])
        .await;
        let client = client(&fixture.origin);
        let endpoints = endpoints(&fixture.origin);
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
            adapter(&client, &endpoints, limits(), GoogleFamily::Gemini1)
                .embed(&["x"], GoogleInput::Document, &key(), context)
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
    let mut first = reply(rows(1));
    first.delay = Duration::from_millis(60);
    let mut second = reply(rows(1));
    second.delay = Duration::from_millis(60);
    let fixture = fixture(vec![first, second]).await;
    let client = client(&fixture.origin);
    let endpoints = endpoints(&fixture.origin);
    let cancel = Cancellation::default();
    let policy = EmbeddingLimits::new(3, 100, 2, 6, 1, 1000, 1000).unwrap();
    let context = InferenceContext::new(&cancel, Instant::now() + Duration::from_millis(100));
    // Total120 is after original100 but before renewedHTTP160; aftercheck alone would be Context(Deadline).
    assert_eq!(
        error(
            adapter(&client, &endpoints, policy, GoogleFamily::Gemini1)
                .embed(&["a", "b", "c"], GoogleInput::Document, &key(), context)
                .await
        ),
        EmbeddingError::Network(NetworkError::Deadline)
    );
    assert_eq!(fixture.server.await.unwrap().len(), 2);
}
#[tokio::test]
async fn exact_prefix_utf8_json_response_tensor_and_endpoint_bounds() {
    for (family, input) in [
        (GoogleFamily::Gemini1, GoogleInput::Document),
        (GoogleFamily::Gemini2, GoogleInput::Document),
        (GoogleFamily::Gemini2, GoogleInput::Query),
    ] {
        let request_bytes = request(family, input, &["é"]).to_string().len();
        let response_bytes = rows(1).to_string().len();
        let input_bytes = text(family, input, "é").len();
        for response_cap in [response_bytes, response_bytes - 1] {
            let fixture = fixture(vec![reply(rows(1))]).await;
            let client = client(&fixture.origin);
            let endpoints = endpoints(&fixture.origin);
            let cancel = Cancellation::default();
            let policy =
                EmbeddingLimits::new(1, input_bytes, 2, 2, 1, request_bytes, response_cap).unwrap();
            let result = adapter(&client, &endpoints, policy, family)
                .embed(&["é"], input, &key(), context(&cancel))
                .await;
            if response_cap == response_bytes {
                assert_eq!(result.unwrap().vectors(), &[vec![0.0, 1.0]]);
            } else {
                assert_eq!(error(result), EmbeddingError::Limit);
            }
            fixture.server.await.unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let client = client(&origin);
        let endpoints = endpoints(&origin);
        let cancel = Cancellation::default();
        for (utf8, req, texts) in [
            (input_bytes, request_bytes, vec!["éx"]),
            (input_bytes - 1, request_bytes, vec!["é"]),
            (input_bytes, request_bytes - 1, vec!["é"]),
            (input_bytes, request_bytes, vec!["a", "b"]),
        ] {
            let policy = EmbeddingLimits::new(1, utf8, 2, 2, 1, req, response_bytes).unwrap();
            assert_eq!(
                error(
                    adapter(&client, &endpoints, policy, family)
                        .embed(&texts, input, &key(), context(&cancel))
                        .await
                ),
                EmbeddingError::Limit
            );
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(25), listener.accept())
                .await
                .is_err()
        );
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let client = client(&origin);
    let endpoints = endpoints(&origin);
    for (single, batch, model, width, expected) in [
        (
            endpoints.single.as_str(),
            endpoints.batch.as_str(),
            "models/fixture-model",
            0,
            EmbeddingError::InvalidRequest,
        ),
        (
            endpoints.single.as_str(),
            endpoints.batch.as_str(),
            "models/fixture-model",
            5000,
            EmbeddingError::Limit,
        ),
        (
            "",
            endpoints.batch.as_str(),
            "models/fixture-model",
            2,
            EmbeddingError::InvalidRequest,
        ),
        (
            endpoints.single.as_str(),
            endpoints.batch.as_str(),
            "fixture-model",
            2,
            EmbeddingError::InvalidRequest,
        ),
        (
            endpoints.single.as_str(),
            endpoints.batch.as_str(),
            "models/other",
            2,
            EmbeddingError::InvalidRequest,
        ),
        (
            endpoints.batch.as_str(),
            endpoints.single.as_str(),
            "models/fixture-model",
            2,
            EmbeddingError::InvalidRequest,
        ),
    ] {
        assert_eq!(
            error(GoogleEmbeddings::new(
                &client,
                GoogleEndpoints { single, batch },
                model,
                width,
                Some(width),
                GoogleFamily::Gemini1,
                limits()
            )),
            expected
        );
    }
    for bad in [
        format!("{}?key=synthetic", endpoints.single),
        format!("{}?query", endpoints.single),
        "ftp://example/models/fixture-model:embedContent".to_owned(),
        "/models/fixture-model:embedContent".to_owned(),
    ] {
        assert_eq!(
            error(GoogleEmbeddings::new(
                &client,
                GoogleEndpoints {
                    single: &bad,
                    batch: &endpoints.batch
                },
                "models/fixture-model",
                2,
                Some(2),
                GoogleFamily::Gemini1,
                limits()
            )),
            EmbeddingError::InvalidRequest
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
async fn measure_google_loopback_fixture() {
    let fixture = fixture((0..128).map(|_| reply(rows(8))).collect()).await;
    let client = client(&fixture.origin);
    let endpoints = endpoints(&fixture.origin);
    let cancel = Cancellation::default();
    let adapter = adapter(&client, &endpoints, limits(), GoogleFamily::Gemini1);
    let mut samples = Vec::new();
    for _ in 0..128 {
        let start = Instant::now();
        let output = adapter
            .embed(&["x"; 8], GoogleInput::Document, &key(), context(&cancel))
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
    let routes = endpoints(&f.origin);
    let sdk = adapter(&transport, &routes, policy, GoogleFamily::Gemini1);
    let cancel = Cancellation::default();
    let auth = http::HeaderMap::from_iter([(
        http::HeaderName::from_static("x-goog-api-key"),
        http::HeaderValue::from_static("fixture-key-canary"),
    )]);
    let mut denied = || Err::<(), _>("synthetic-admission-denial");
    let failure = sdk
        .embed_guarded(
            &["text"],
            GoogleInput::Query,
            &auth,
            context(&cancel),
            &mut denied,
        )
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
    let routes = endpoints(&f.origin);
    let sdk = adapter(&transport, &routes, policy, GoogleFamily::Gemini1);
    let admitted = std::sync::atomic::AtomicBool::new(true);
    let mut gate = || {
        if admitted.load(std::sync::atomic::Ordering::SeqCst) {
            Ok(())
        } else {
            Err("synthetic-private-denial")
        }
    };
    let call = sdk.embed_guarded(
        &["text"],
        GoogleInput::Query,
        &auth,
        context(&cancel),
        &mut gate,
    );
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

#[tokio::test]
async fn configured_output_dimensions_are_optional_for_both_google_families() {
    for family in [GoogleFamily::Gemini1, GoogleFamily::Gemini2] {
        for request_dimensions in [None, Some(2)] {
            for count in [1, 2] {
                let response = if count == 1 {
                    json!({"embedding":{"values":[0.25,0.75]}})
                } else {
                    json!({"embeddings":[{"values":[0.25,0.75]},{"values":[0.25,0.75]}]})
                };
                let fixture = fixture(vec![reply(response)]).await;
                let client = client(&fixture.origin);
                let endpoints = endpoints(&fixture.origin);
                let sdk = GoogleEmbeddings::new(
                    &client,
                    GoogleEndpoints {
                        single: &endpoints.single,
                        batch: &endpoints.batch,
                    },
                    "models/fixture-model",
                    2,
                    request_dimensions,
                    family,
                    limits(),
                )
                .unwrap();
                let cancel = Cancellation::default();
                sdk.embed(
                    &["a", "b"][..count],
                    GoogleInput::Query,
                    &key(),
                    context(&cancel),
                )
                .await
                .unwrap();
                let requests = fixture.server.await.unwrap();
                let body = &requests[0].body;
                let items = if count == 1 {
                    vec![body]
                } else {
                    body["requests"].as_array().unwrap().iter().collect()
                };
                for item in items {
                    assert_eq!(
                        item["embedContentConfig"]
                            .get("outputDimensionality")
                            .and_then(Value::as_u64),
                        request_dimensions.map(|width| width as u64)
                    );
                }
            }
        }
    }
}
