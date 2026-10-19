use graphit_ai::{
    cohere_rerank::CohereReranker,
    context::{ContextError, InferenceContext},
    native_rerank::{IndexScore, NativeRerankError, NativeRerankLimits, validate_results},
    ranking::{RankingLimits, rank_order},
};
use graphit_core::file_lock::Cancellation;
use graphit_http::client::{BoundedClient, ClientPolicy, Limits};
use secrecy::SecretString;
use std::time::{Duration, Instant};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
fn limits() -> NativeRerankLimits {
    NativeRerankLimits::new(8, 2, 1000, 1000, 1000).unwrap()
}
fn ctx(cancel: &Cancellation) -> InferenceContext<'_> {
    InferenceContext::new(cancel, Instant::now() + Duration::from_secs(5))
}
#[test]
fn policy_and_complete_validator_preserve_native_precision_and_original_context() {
    for index in 0..5 {
        for invalid in [0, usize::MAX] {
            let mut f = [8, 2, 1000, 1000, 1000];
            f[index] = invalid;
            assert_eq!(
                NativeRerankLimits::new(f[0], f[1], f[2], f[3], f[4]).unwrap_err(),
                NativeRerankError::Policy
            );
        }
    }
    assert!(NativeRerankLimits::new(isize::MAX as usize, 1, 1, 1, 1).is_err());
    let cancel = Cancellation::default();
    let rows = [
        IndexScore {
            index: 1,
            relevance_score: -0.0,
        },
        IndexScore {
            index: 0,
            relevance_score: -1.0000000000001,
        },
    ];
    let scores = validate_results(2, &rows, limits(), ctx(&cancel)).unwrap();
    assert_eq!(scores, [-1.0000000000001, -0.0]);
    assert!(scores[1].is_sign_negative());
    assert!(
        validate_results(0, &[], limits(), ctx(&cancel))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        validate_results(9, &[], limits(), ctx(&cancel)).unwrap_err(),
        NativeRerankError::Limit
    );
    assert_eq!(
        validate_results(1, &[], limits(), ctx(&cancel)).unwrap_err(),
        NativeRerankError::Shape
    );
    for rows in [
        vec![
            IndexScore {
                index: 0,
                relevance_score: 1.0,
            },
            IndexScore {
                index: 0,
                relevance_score: 2.0,
            },
        ],
        vec![
            IndexScore {
                index: 0,
                relevance_score: 1.0,
            },
            IndexScore {
                index: 2,
                relevance_score: 2.0,
            },
        ],
    ] {
        assert_eq!(
            validate_results(2, &rows, limits(), ctx(&cancel)).unwrap_err(),
            NativeRerankError::Shape
        );
    }
    for score in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            validate_results(
                1,
                &[IndexScore {
                    index: 0,
                    relevance_score: score
                }],
                limits(),
                ctx(&cancel)
            )
            .unwrap_err(),
            NativeRerankError::Shape
        );
    }
    assert_eq!(
        validate_results(
            0,
            &[],
            limits(),
            InferenceContext::new(&cancel, Instant::now())
        )
        .unwrap_err(),
        NativeRerankError::Context(ContextError::Deadline)
    );
    cancel.cancel();
    assert_eq!(
        validate_results(0, &[], limits(), ctx(&cancel)).unwrap_err(),
        NativeRerankError::Context(ContextError::Cancelled)
    );
}
#[tokio::test]
async fn real_cohere_scores_feed_existing_rank_order_with_caller_indices_and_zero_ties() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = Vec::new();
        let end = loop {
            if let Some(i) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                break i + 4;
            }
            let mut buf = [0; 1024];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            assert!(bytes.len() < 4096);
        };
        let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
        let len: usize = head
            .lines()
            .find_map(|s| {
                s.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .map(str::to_owned)
            })
            .unwrap()
            .parse()
            .unwrap();
        while bytes.len() < end + len {
            let mut buf = [0; 1024];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            assert!(bytes.len() < 4096);
        }
        let request: serde_json::Value = serde_json::from_slice(&bytes[end..end + len]).unwrap();
        assert_eq!(request["top_n"], 5);
        let body = r#"{"results":[{"index":4,"relevance_score":-2},{"index":3,"relevance_score":0},{"index":1,"relevance_score":-0.0},{"index":2,"relevance_score":0},{"index":0,"relevance_score":0.5000000000001}],"meta":{"ignored":[1,2]}}"#;
        let header = format!(
            "HTTP/1.1 200 fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(header.as_bytes()).await.unwrap();
        socket.write_all(body.as_bytes()).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(25), listener.accept())
                .await
                .is_err()
        );
    });
    let client = BoundedClient::new(
        ClientPolicy {
            origin: &origin,
            allow_loopback_http: true,
            proxy: None,
            additional_roots: &[],
        },
        Limits {
            request_body_bytes: 1000,
            response_body_bytes: 1000,
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
    .unwrap();
    let endpoint = format!("{origin}/v2/rerank");
    let cancel = Cancellation::default();
    let policy = NativeRerankLimits::new(5, 5, 1000, 1000, 1000).unwrap();
    let scores = CohereReranker::new(&client, &endpoint, "fixture-model", policy)
        .unwrap()
        .score(
            "q",
            &["a", "b", "c", "d", "e"],
            &SecretString::from("synthetic-key"),
            ctx(&cancel),
        )
        .await
        .unwrap();
    assert_eq!(scores, [0.5000000000001, -0.0, 0.0, 0.0, -2.0]);
    assert!(scores[1].is_sign_negative());
    let indices = [9, 4, 2, 2, 0];
    assert_eq!(
        rank_order(
            &indices,
            &scores,
            RankingLimits::new(5, 1, 5).unwrap(),
            ctx(&cancel)
        )
        .unwrap(),
        [0, 2, 3, 1, 4]
    );
    server.await.unwrap();
}
