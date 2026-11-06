use graphit_ai::{
    cohere_rerank::CohereReranker,
    context::{ContextError, InferenceContext},
    direct_native_rerank::{self, DirectNativeRerankError},
    native_rerank::{GuardedRerankError, NativeRerankError, NativeRerankLimits},
};
use graphit_auth::{
    ServicePurpose, ServiceUseError,
    profile::{DirectProtocol, ProfileError},
};
use graphit_core::file_lock::Cancellation;
use graphit_http::client::{BoundedClient, ClientPolicy, Limits, NetworkEffect};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde_json::{Value, json};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
#[path = "support/direct_rerank_auth.rs"]
mod auth;
#[path = "support/rerank_http.rs"]
mod wire;
use auth::{AuthFixture, MCP_KEY, RERANK_KEY, request};
use wire::{fixture, fixture_with_barriers, reply};

fn configured_key() -> HeaderMap {
    let mut headers = HeaderMap::new();
    let mut value = HeaderValue::from_str(&format!("Bearer {RERANK_KEY}")).unwrap();
    value.set_sensitive(true);
    headers.insert(header::AUTHORIZATION, value);
    headers
}
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
fn rows(protocol: DirectProtocol, scores: &[f64]) -> Value {
    let values: Vec<_> = scores
        .iter()
        .enumerate()
        .rev()
        .map(|(index, score)| json!({"index":index,"relevance_score":score}))
        .collect();
    if matches!(protocol, DirectProtocol::Voyage | DirectProtocol::VoyageV1) {
        json!({"data":values})
    } else {
        json!({"results":values})
    }
}
fn error<T>(result: Result<T, DirectNativeRerankError>) -> DirectNativeRerankError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected typed consumer failure"),
    }
}
fn require_send<T: Send>(future: T) -> T {
    future
}
fn redacted(error: &DirectNativeRerankError) {
    let display = format!("{error:?} {error}");
    for private in [
        MCP_KEY,
        RERANK_KEY,
        "fixture-native-model",
        "fixture-private-query",
        "127.0.0.1",
    ] {
        assert!(!display.contains(private));
    }
}

async fn await_response_headers(
    tcp: &mut wire::Fixture,
    call: Pin<&mut impl Future<Output = Result<Vec<f64>, DirectNativeRerankError>>>,
) {
    tokio::select! {
        biased;
        observed = tokio::time::timeout(Duration::from_secs(6), tcp.observed.notified()) => {
            if observed.is_err() {
                tcp.server.abort();
                let _ = (&mut tcp.server).await;
                panic!("rerank fixture did not send response headers");
            }
        }
        result = call => {
            tcp.server.abort();
            let _ = (&mut tcp.server).await;
            panic!("rerank completed before fixture headers: {result:?}");
        }
        result = &mut tcp.server => panic!(
            "rerank fixture stopped before sending headers: {result:?}",
            result = result.map(|requests| requests.len()),
        ),
    }
}

async fn finish_fixture(tcp: wire::Fixture) -> usize {
    let mut server = tcp.server;
    match tokio::time::timeout(Duration::from_secs(6), &mut server).await {
        Ok(result) => result.unwrap().len(),
        Err(_) => {
            server.abort();
            let _ = server.await;
            panic!("rerank fixture did not shut down");
        }
    }
}

#[tokio::test]
async fn six_protocols_use_literal_binding_real_direct_key_and_complete_f64_batches() {
    for protocol in [
        DirectProtocol::Cohere,
        DirectProtocol::CohereV2,
        DirectProtocol::Voyage,
        DirectProtocol::VoyageV1,
        DirectProtocol::Jina,
        DirectProtocol::JinaV1,
    ] {
        let tcp = fixture(vec![
            reply(rows(protocol, &[-0.123456789012345, -0.0])),
            reply(rows(protocol, &[0.987654321098765])),
        ])
        .await;
        let endpoint = format!("{}/literal/rerank?captured=yes", tcp.origin);
        let cancellation = Cancellation::default();
        let ctx = request(&cancellation, Instant::now() + Duration::from_secs(30));
        let auth = AuthFixture::new(&ctx, protocol, &endpoint);
        let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Rerank)
            .unwrap();
        let lease = selection.credential().unwrap();
        let transport = client(&tcp.origin);
        let scores = require_send(direct_native_rerank::score(
            &lease,
            "fixture-private-query",
            &["first", "second", "third"],
            &transport,
            limits(2),
        ))
        .await
        .unwrap();
        assert_eq!(scores, [-0.123456789012345, -0.0, 0.987654321098765]);
        assert!(scores[1].is_sign_negative());
        let captured = tcp.server.await.unwrap();
        assert_eq!(captured.len(), 2);
        for captured in captured {
            assert!(
                captured
                    .head
                    .starts_with("POST /literal/rerank?captured=yes HTTP/1.1")
            );
            assert!(captured.head.contains(&format!("Bearer {RERANK_KEY}")));
            assert!(!captured.head.contains(MCP_KEY));
            assert_eq!(captured.body["model"], "fixture-native-model");
            assert_eq!(captured.body["query"], "fixture-private-query");
            assert_eq!(
                captured.body["documents"].as_array().unwrap().len(),
                captured.body[if matches!(
                    protocol,
                    DirectProtocol::Voyage | DirectProtocol::VoyageV1
                ) {
                    "top_k"
                } else {
                    "top_n"
                }]
                .as_u64()
                .unwrap() as usize
            );
        }
    }
}

#[tokio::test]
async fn real_embedding_and_broker_leases_are_rejected_without_native_send() {
    for broker in [false, true] {
        let tcp = fixture(vec![]).await;
        let cancellation = Cancellation::default();
        let ctx = request(&cancellation, Instant::now() + Duration::from_secs(30));
        let auth = if broker {
            AuthFixture::broker(&ctx, &tcp.origin)
        } else {
            AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin)
        };
        let purpose = if broker {
            ServicePurpose::Rerank
        } else {
            ServicePurpose::Embedding
        };
        let issuer = auth.issuer(&ctx, purpose);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer.capture_service(&caller, purpose).unwrap();
        let lease = selection.credential().unwrap();
        let result = error(
            direct_native_rerank::score(
                &lease,
                "query",
                &["document"],
                &client(&tcp.origin),
                limits(1),
            )
            .await,
        );
        assert!(matches!(
            result,
            DirectNativeRerankError::UnsupportedBinding
        ));
        assert_eq!(result.effect(), NetworkEffect::NotSent);
        redacted(&result);
        assert!(tcp.server.await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn physical_key_rotation_before_entry_prevents_send() {
    let tcp = fixture(vec![]).await;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.credential().unwrap();
    auth.rotate_key(&ctx);
    assert!(lease.check().is_err());
    let result = error(
        direct_native_rerank::score(
            &lease,
            "query",
            &["document"],
            &client(&tcp.origin),
            limits(1),
        )
        .await,
    );
    assert!(matches!(result, DirectNativeRerankError::Service { .. }));
    assert_eq!(result.effect(), NetworkEffect::NotSent);
    redacted(&result);
    assert!(
        std::error::Error::source(&result)
            .unwrap()
            .downcast_ref::<ServiceUseError>()
            .is_some()
    );
    assert!(tcp.server.await.unwrap().is_empty());
}

#[tokio::test]
async fn physical_rotation_during_response_discards_all_output_with_remote_effect() {
    let release = Arc::new(Semaphore::new(0));
    let mut tcp = fixture_with_barriers(
        vec![reply(rows(DirectProtocol::Cohere, &[0.8]))],
        vec![release.clone()],
    )
    .await;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.credential().unwrap();
    let transport = client(&tcp.origin);
    let call = direct_native_rerank::score(&lease, "query", &["document"], &transport, limits(1));
    tokio::pin!(call);
    await_response_headers(&mut tcp, call.as_mut()).await;
    auth.rotate_key(&ctx);
    let expected = lease.check().expect_err("rotation must revoke lease");
    release.add_permits(1);
    let result = error(call.await);
    match &result {
        DirectNativeRerankError::Service { source, effect } => {
            assert_eq!(*effect, NetworkEffect::MayHaveApplied);
            assert_eq!(
                std::mem::discriminant(source),
                std::mem::discriminant(&expected)
            );
        }
        _ => panic!("expected typed Auth revocation"),
    }
    redacted(&result);
    assert_eq!(finish_fixture(tcp).await, 1);
}

#[tokio::test]
async fn successful_prior_batch_and_later_physical_revocation_never_return_partial_scores() {
    let first = Arc::new(Semaphore::new(0));
    let second = Arc::new(Semaphore::new(0));
    let mut tcp = fixture_with_barriers(
        vec![
            reply(rows(DirectProtocol::Voyage, &[0.8])),
            reply(rows(DirectProtocol::Voyage, &[0.7])),
        ],
        vec![first.clone(), second.clone()],
    )
    .await;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&ctx, DirectProtocol::Voyage, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.credential().unwrap();
    let transport = client(&tcp.origin);
    let call = direct_native_rerank::score(
        &lease,
        "query",
        &["first", "second", "must-not-send"],
        &transport,
        limits(1),
    );
    tokio::pin!(call);
    await_response_headers(&mut tcp, call.as_mut()).await;
    first.add_permits(1);
    await_response_headers(&mut tcp, call.as_mut()).await;
    auth.rotate_key(&ctx);
    assert!(lease.check().is_err());
    second.add_permits(1);
    let result = error(call.await);
    assert!(matches!(result, DirectNativeRerankError::Service { .. }));
    assert_eq!(result.effect(), NetworkEffect::MayHaveApplied);
    assert_eq!(finish_fixture(tcp).await, 2);
}

#[tokio::test]
async fn original_pre_cancel_and_deadline_fail_before_send_with_real_causes() {
    for cancel in [true, false] {
        let tcp = fixture(vec![]).await;
        let cancellation = Cancellation::default();
        let setup = request(&cancellation, Instant::now() + Duration::from_secs(30));
        let auth = AuthFixture::new(&setup, DirectProtocol::Jina, &tcp.origin);
        let deadline = Instant::now() + Duration::from_secs(1);
        let ctx = request(&cancellation, deadline);
        let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Rerank)
            .unwrap();
        let lease = selection.credential().unwrap();
        if cancel {
            cancellation.cancel();
        } else {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        }
        let result = error(
            direct_native_rerank::score(
                &lease,
                "query",
                &["document"],
                &client(&tcp.origin),
                limits(1),
            )
            .await,
        );
        match result {
            DirectNativeRerankError::Service {
                source: ServiceUseError::Profile(source),
                effect,
            } => {
                assert_eq!(effect, NetworkEffect::NotSent);
                assert_eq!(
                    source,
                    if cancel {
                        ProfileError::Cancelled
                    } else {
                        ProfileError::Deadline
                    }
                );
            }
            _ => panic!("expected original context cause from Auth"),
        }
        assert!(tcp.server.await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn original_cancel_and_deadline_after_observed_send_preserve_effect() {
    for cancel in [true, false] {
        let release = Arc::new(Semaphore::new(0));
        let mut tcp = fixture_with_barriers(
            vec![reply(rows(DirectProtocol::Jina, &[0.8]))],
            vec![release.clone()],
        )
        .await;
        let cancellation = Cancellation::default();
        let setup = request(&cancellation, Instant::now() + Duration::from_secs(30));
        let auth = AuthFixture::new(&setup, DirectProtocol::Jina, &tcp.origin);
        let ctx = request(
            &cancellation,
            Instant::now() + Duration::from_secs(if cancel { 30 } else { 1 }),
        );
        let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Rerank)
            .unwrap();
        let lease = selection.credential().unwrap();
        let transport = client(&tcp.origin);
        let call =
            direct_native_rerank::score(&lease, "query", &["document"], &transport, limits(1));
        tokio::pin!(call);
        await_response_headers(&mut tcp, call.as_mut()).await;
        if cancel {
            cancellation.cancel();
        }
        let result = error(call.await);
        assert_eq!(result.effect(), NetworkEffect::MayHaveApplied);
        match result {
            DirectNativeRerankError::Rerank {
                source: NativeRerankError::Context(source),
                ..
            } => assert_eq!(
                source,
                if cancel {
                    ContextError::Cancelled
                } else {
                    ContextError::Deadline
                }
            ),
            _ => panic!("expected original inference context failure"),
        }
        release.add_permits(1);
        assert_eq!(finish_fixture(tcp).await, 1);
    }
}

#[tokio::test]
async fn empty_and_full_preflight_preserve_zero_send_and_admission() {
    let tcp = fixture(vec![]).await;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.credential().unwrap();
    let transport = client(&tcp.origin);
    assert!(
        direct_native_rerank::score(&lease, "query", &[], &transport, limits(1))
            .await
            .unwrap()
            .is_empty()
    );
    let policy = NativeRerankLimits::new(100, 1, 10000, 220, 10000).unwrap();
    let escaped = "\\".repeat(100);
    let result = error(
        direct_native_rerank::score(&lease, "query", &["small", &escaped], &transport, policy)
            .await,
    );
    assert!(matches!(
        result,
        DirectNativeRerankError::Rerank {
            source: NativeRerankError::Limit,
            ..
        }
    ));
    assert_eq!(result.effect(), NetworkEffect::NotSent);
    assert!(tcp.server.await.unwrap().is_empty());
}

#[tokio::test]
async fn late_http_status_preserves_typed_native_cause_and_no_following_batch() {
    let mut failure = reply(json!({"error":"private-vendor-canary"}));
    failure.status = 502;
    let tcp = fixture(vec![reply(rows(DirectProtocol::Cohere, &[0.8])), failure]).await;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.credential().unwrap();
    let result = error(
        direct_native_rerank::score(
            &lease,
            "query",
            &["first", "second", "must-not-send"],
            &client(&tcp.origin),
            limits(1),
        )
        .await,
    );
    assert!(matches!(
        result,
        DirectNativeRerankError::Rerank {
            source: NativeRerankError::Status(StatusCode::BAD_GATEWAY),
            ..
        }
    ));
    assert_eq!(result.effect(), NetworkEffect::MayHaveApplied);
    redacted(&result);
    assert_eq!(tcp.server.await.unwrap().len(), 2);
}

#[tokio::test]
async fn dropping_consumer_future_releases_the_existing_client_permit() {
    let release = Arc::new(Semaphore::new(0));
    let tcp = fixture_with_barriers(
        vec![
            reply(rows(DirectProtocol::Cohere, &[0.8])),
            reply(rows(DirectProtocol::Cohere, &[0.7])),
        ],
        vec![release.clone()],
    )
    .await;
    let cancellation = Cancellation::default();
    let ctx = request(&cancellation, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.credential().unwrap();
    let transport = client(&tcp.origin);
    let mut pending = Box::pin(direct_native_rerank::score(
        &lease,
        "query",
        &["document"],
        &transport,
        limits(1),
    ));
    tokio::select! { _ = tcp.observed.notified() => {}, _ = &mut pending => panic!("response fixture must stall") }
    drop(pending);
    release.add_permits(1);
    assert_eq!(
        direct_native_rerank::score(&lease, "query", &["document"], &transport, limits(1))
            .await
            .unwrap(),
        [0.7]
    );
    assert_eq!(tcp.server.await.unwrap().len(), 2);
}

#[derive(Debug)]
struct PrivateCause;
impl std::fmt::Display for PrivateCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("private-admission-canary")
    }
}
impl std::error::Error for PrivateCause {}
#[tokio::test]
async fn guarded_sdk_retains_opaque_typed_cause_and_rechecks_context_after_callback() {
    let tcp = fixture(vec![]).await;
    let transport = client(&tcp.origin);
    let cancellation = Cancellation::default();
    let context = InferenceContext::new(&cancellation, Instant::now() + Duration::from_secs(30));
    let adapter =
        CohereReranker::new(&transport, &tcp.origin, "fixture-native-model", limits(1)).unwrap();
    let mut deny = || Err(PrivateCause);
    let result = adapter
        .score_guarded(
            "query",
            &["document"],
            &configured_key(),
            context,
            &mut deny,
        )
        .await
        .err()
        .unwrap();
    assert!(!format!("{result:?} {result}").contains("private-admission-canary"));
    assert!(
        std::error::Error::source(&result)
            .unwrap()
            .downcast_ref::<PrivateCause>()
            .is_some()
    );
    assert_eq!(result.effect(), NetworkEffect::NotSent);
    let mut cancel = || {
        cancellation.cancel();
        Ok::<(), PrivateCause>(())
    };
    assert!(matches!(
        adapter
            .score_guarded(
                "query",
                &["document"],
                &configured_key(),
                context,
                &mut cancel
            )
            .await,
        Err(GuardedRerankError::Rerank {
            source: NativeRerankError::Context(ContextError::Cancelled),
            effect: NetworkEffect::NotSent
        })
    ));

    let fresh = Cancellation::default();
    let deadline = Instant::now() + Duration::from_millis(100);
    let context = InferenceContext::new(&fresh, deadline);
    let mut triggered = false;
    let mut expire = || {
        triggered = true;
        std::thread::sleep(
            deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(5),
        );
        Ok::<(), PrivateCause>(())
    };
    assert!(matches!(
        adapter
            .score_guarded(
                "query",
                &["document"],
                &configured_key(),
                context,
                &mut expire
            )
            .await,
        Err(GuardedRerankError::Rerank {
            source: NativeRerankError::Context(ContextError::Deadline),
            effect: NetworkEffect::NotSent
        })
    ));
    assert!(triggered);
    assert!(tcp.server.await.unwrap().is_empty());
}

#[tokio::test]
async fn service_original_without_deadline_is_causal_not_sent() {
    let tcp = fixture(vec![]).await;
    let cancel = Cancellation::default();
    let setup = request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::new(&setup, DirectProtocol::Cohere, &tcp.origin);
    let ctx = graphit_hub::policy::RequestContext::service(
        graphit_hub::native::ContentLimits {
            max_input_bytes: 4096,
            max_output_bytes: 4096,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 256,
            max_collection_entries: 64,
        },
        &cancel,
    )
    .unwrap();
    let issuer = auth.issuer(&ctx, ServicePurpose::Rerank);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Rerank)
        .unwrap();
    let lease = selection.credential().unwrap();
    assert_eq!(lease.original().unwrap().deadline(), None);
    let failure = error(
        direct_native_rerank::score(
            &lease,
            "fixture-private-query",
            &["first"],
            &client(&tcp.origin),
            limits(1),
        )
        .await,
    );
    assert_eq!(failure.effect(), NetworkEffect::NotSent);
    assert!(
        matches!(&failure, DirectNativeRerankError::Context(source) if source.kind() == std::io::ErrorKind::InvalidInput)
    );
    let cause = std::error::Error::source(&failure)
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), std::io::ErrorKind::InvalidInput);
    redacted(&failure);
    assert!(tcp.server.await.unwrap().is_empty());
}
