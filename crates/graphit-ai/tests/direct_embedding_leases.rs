use graphit_ai::{
    context::ContextError,
    direct_embedding::{self, DirectEmbeddingError, DirectEmbeddingInput},
    embedding::{EmbeddingError, EmbeddingLimits},
};
use graphit_auth::{
    ServicePurpose, ServiceUseError,
    profile::{DirectProtocol, ProfileError},
};
use graphit_core::file_lock::Cancellation;
use graphit_http::client::{BoundedClient, ClientPolicy, Limits, NetworkEffect, NetworkError};
use http::StatusCode;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
#[path = "support/direct_rerank_auth.rs"]
mod auth;
#[path = "support/rerank_http.rs"]
mod tcp;
use auth::{AuthFixture, EMBEDDING_KEY, EmbeddingConfig, MCP_KEY, RERANK_KEY, request};
use tcp::{Reply, fixture, fixture_with_barriers, reply};

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
fn limits(batch: usize) -> EmbeddingLimits {
    EmbeddingLimits::new(100, 10000, 16, 1600, batch, 10000, 10000).unwrap()
}
const PROTOCOLS: [DirectProtocol; 6] = [
    DirectProtocol::Cohere,
    DirectProtocol::Voyage,
    DirectProtocol::OpenAi,
    DirectProtocol::OpenAiCompatible,
    DirectProtocol::GoogleGemini1,
    DirectProtocol::GoogleGemini2,
];
fn embedding_fixture(
    ctx: &graphit_hub::policy::RequestContext<'_>,
    protocol: DirectProtocol,
    origin: &str,
) -> AuthFixture {
    if matches!(
        protocol,
        DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
    ) {
        AuthFixture::embedding_config(
            ctx,
            EmbeddingConfig {
                protocol,
                endpoint: &format!("{origin}/v1beta/models/fixture-model:embedContent"),
                model: "models/fixture-model",
                dimensions: 3,
                request_dimensions: None,
                batch_endpoint: Some(&format!(
                    "{origin}/v1beta/models/fixture-model:batchEmbedContents"
                )),
            },
        )
    } else {
        AuthFixture::embedding(ctx, protocol, origin)
    }
}

fn rows(protocol: DirectProtocol, vectors: &[[f32; 3]]) -> Value {
    if matches!(
        protocol,
        DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
    ) {
        if vectors.len() == 1 {
            json!({"embedding":{"values":vectors[0]}})
        } else {
            json!({"embeddings":vectors.iter().map(|v|json!({"values":v})).collect::<Vec<_>>()})
        }
    } else if protocol == DirectProtocol::Cohere {
        json!({"embeddings":{"float":vectors}})
    } else {
        json!({"data":vectors.iter().enumerate().rev().map(|(index, v)| json!({"index":index,"embedding":v})).collect::<Vec<_>>()})
    }
}
fn error(result: Result<Vec<Vec<f32>>, DirectEmbeddingError>) -> DirectEmbeddingError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected typed embedding failure"),
    }
}
fn redacted(error: &DirectEmbeddingError) {
    let text = format!("{error:?} {error}");
    for canary in [
        MCP_KEY,
        EMBEDDING_KEY,
        RERANK_KEY,
        "fixture-embedding-model",
        "fixture-private-text",
        "127.0.0.1",
    ] {
        assert!(!text.contains(canary));
    }
}
fn require_send<T: Send>(value: T) -> T {
    value
}

#[tokio::test]
async fn real_leases_preserve_batched_vectors_roles_literal_binding_and_service_key() {
    for protocol in [DirectProtocol::Cohere, DirectProtocol::Voyage] {
        for input in [DirectEmbeddingInput::Query, DirectEmbeddingInput::Document] {
            let vectors = [[0.5, -0.0, 1.0], [0.25, 0.75, 1.0], [0.125, 0.875, 1.0]];
            let tcp = fixture(vec![
                reply(rows(protocol, &vectors[..2])),
                reply(rows(protocol, &vectors[2..])),
            ])
            .await;
            let endpoint = format!("{}/literal/embedding?captured=yes", tcp.origin);
            let cancel = Cancellation::default();
            let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
            let auth = AuthFixture::embedding(&ctx, protocol, &endpoint);
            let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
            let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
            let mut selection = issuer
                .capture_service(&caller, ServicePurpose::Embedding)
                .unwrap();
            let lease = selection.credential().unwrap();
            let transport = client(&tcp.origin);
            let output = require_send(direct_embedding::embed(
                &lease,
                &["first", "second", "third"],
                input,
                &transport,
                limits(2),
            ))
            .await
            .unwrap();
            assert_eq!(output, vectors);
            assert!(output[0][1].is_sign_negative());
            let requests = tcp.server.await.unwrap();
            assert_eq!(requests.len(), 2);
            for (index, captured) in requests.iter().enumerate() {
                assert!(
                    captured
                        .head
                        .starts_with("POST /literal/embedding?captured=yes HTTP/1.1")
                );
                assert!(captured.head.contains(&format!("Bearer {EMBEDDING_KEY}")));
                assert!(!captured.head.contains(MCP_KEY));
                assert!(!captured.head.contains(RERANK_KEY));
                assert_eq!(captured.body["model"], "fixture-embedding-model");
                let field = if protocol == DirectProtocol::Cohere {
                    "texts"
                } else {
                    "input"
                };
                assert_eq!(
                    captured.body[field],
                    if index == 0 {
                        json!(["first", "second"])
                    } else {
                        json!(["third"])
                    }
                );
                let role = match (protocol, input) {
                    (DirectProtocol::Cohere, DirectEmbeddingInput::Query) => "search_query",
                    (DirectProtocol::Cohere, DirectEmbeddingInput::Document) => "search_document",
                    (_, DirectEmbeddingInput::Query) => "query",
                    (_, DirectEmbeddingInput::Document) => "document",
                };
                assert_eq!(captured.body["input_type"], role);
                assert!(captured.body.get("output_dimension").is_none());
                assert!(captured.body.get("encoding_format").is_none());
                if protocol == DirectProtocol::Cohere {
                    assert_eq!(captured.body["embedding_types"], json!(["float"]));
                } else {
                    assert_eq!(captured.body["output_dtype"], "float");
                }
            }
        }
    }
}

#[tokio::test]
async fn incompatible_real_bindings_do_not_send() {
    for mode in 0..2 {
        let tcp = fixture(vec![]).await;
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = if mode == 0 {
            AuthFixture::broker(&ctx, &tcp.origin)
        } else {
            AuthFixture::new(&ctx, DirectProtocol::Cohere, &tcp.origin)
        };
        let purpose = if mode == 1 {
            ServicePurpose::Rerank
        } else {
            ServicePurpose::Embedding
        };
        let issuer = auth.issuer(&ctx, purpose);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer.capture_service(&caller, purpose).unwrap();
        let lease = selection.credential().unwrap();
        let result = error(
            direct_embedding::embed(
                &lease,
                &["fixture-private-text"],
                DirectEmbeddingInput::Query,
                &client(&tcp.origin),
                limits(1),
            )
            .await,
        );
        assert!(matches!(result, DirectEmbeddingError::UnsupportedBinding));
        assert_eq!(result.effect(), NetworkEffect::NotSent);
        redacted(&result);
        assert!(tcp.server.await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn empty_total_limits_and_late_preflight_do_not_send_and_empty_still_checks_lease() {
    for protocol in PROTOCOLS {
        let tcp = fixture(vec![]).await;
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = embedding_fixture(&ctx, protocol, &tcp.origin);
        let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selection.credential().unwrap();
        let transport = client(&tcp.origin);
        assert!(
            direct_embedding::embed(
                &lease,
                &[],
                DirectEmbeddingInput::Query,
                &transport,
                limits(1)
            )
            .await
            .unwrap()
            .is_empty()
        );
        let narrow = EmbeddingLimits::new(100, 10000, 16, 1600, 1, 180, 10000).unwrap();
        let escaped = "\\".repeat(100);
        let result = error(
            direct_embedding::embed(
                &lease,
                &["small", &escaped],
                DirectEmbeddingInput::Document,
                &transport,
                narrow,
            )
            .await,
        );
        assert!(matches!(
            result,
            DirectEmbeddingError::Embedding {
                source: EmbeddingError::Limit,
                ..
            }
        ));
        assert_eq!(result.effect(), NetworkEffect::NotSent);
        let narrow = EmbeddingLimits::new(100, 10000, 2, 1600, 1, 10000, 10000).unwrap();
        let result = error(
            direct_embedding::embed(
                &lease,
                &["small"],
                DirectEmbeddingInput::Query,
                &transport,
                narrow,
            )
            .await,
        );
        assert!(matches!(
            result,
            DirectEmbeddingError::Embedding {
                source: EmbeddingError::Limit,
                ..
            }
        ));
        auth.rotate_key(&ctx);
        for texts in [&[][..], &["fixture-private-text"][..]] {
            let result = error(
                direct_embedding::embed(
                    &lease,
                    texts,
                    DirectEmbeddingInput::Query,
                    &transport,
                    limits(1),
                )
                .await,
            );
            assert!(matches!(result, DirectEmbeddingError::Service { .. }));
            assert_eq!(result.effect(), NetworkEffect::NotSent);
            assert!(
                std::error::Error::source(&result)
                    .unwrap()
                    .downcast_ref::<ServiceUseError>()
                    .is_some()
            );
            redacted(&result);
        }
        assert!(tcp.server.await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn physical_revocation_after_successful_batch_discards_vectors_and_prevents_next_batch() {
    for protocol in PROTOCOLS {
        let first = Arc::new(Semaphore::new(0));
        let second = Arc::new(Semaphore::new(0));
        let tcp = fixture_with_barriers(
            vec![
                reply(rows(protocol, &[[1.0, 0.0, 0.0]])),
                reply(rows(protocol, &[[0.0, 1.0, 0.0]])),
            ],
            vec![first.clone(), second.clone()],
        )
        .await;
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = embedding_fixture(&ctx, protocol, &tcp.origin);
        let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selection.credential().unwrap();
        let transport = client(&tcp.origin);
        let call = direct_embedding::embed(
            &lease,
            &["first", "second", "must-not-send"],
            DirectEmbeddingInput::Document,
            &transport,
            limits(1),
        );
        let revoke = async {
            tcp.observed.notified().await;
            first.add_permits(1);
            tcp.observed.notified().await;
            auth.rotate_key(&ctx);
            let expected = lease.check().expect_err("rotation must invalidate lease");
            second.add_permits(1);
            expected
        };
        let (result, expected) = tokio::join!(call, revoke);
        let result = error(result);
        match &result {
            DirectEmbeddingError::Service { source, effect } => {
                assert_eq!(*effect, NetworkEffect::MayHaveApplied);
                assert_eq!(
                    std::mem::discriminant(source),
                    std::mem::discriminant(&expected)
                );
            }
            _ => panic!("expected typed Auth failure"),
        }
        redacted(&result);
        assert_eq!(tcp.server.await.unwrap().len(), 2);
    }
}

#[tokio::test]
async fn original_context_before_entry_preserves_auth_cause_and_zero_send() {
    for cancelled in [true, false] {
        let tcp = fixture(vec![]).await;
        let cancel = Cancellation::default();
        let setup = request(&cancel, Instant::now() + Duration::from_secs(30));
        let auth = AuthFixture::embedding(&setup, DirectProtocol::Cohere, &tcp.origin);
        let deadline = Instant::now() + Duration::from_secs(1);
        let ctx = request(&cancel, deadline);
        let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selection.credential().unwrap();
        if cancelled {
            cancel.cancel();
        } else {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        }
        let result = error(
            direct_embedding::embed(
                &lease,
                &["text"],
                DirectEmbeddingInput::Query,
                &client(&tcp.origin),
                limits(1),
            )
            .await,
        );
        match result {
            DirectEmbeddingError::Service {
                source: ServiceUseError::Profile(source),
                effect,
            } => {
                assert_eq!(effect, NetworkEffect::NotSent);
                assert_eq!(
                    source,
                    if cancelled {
                        ProfileError::Cancelled
                    } else {
                        ProfileError::Deadline
                    }
                );
            }
            _ => panic!("expected original context failure from Auth"),
        }
        assert!(tcp.server.await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn original_cancel_and_deadline_during_response_preserve_remote_effect() {
    for protocol in PROTOCOLS {
        for cancelled in [true, false] {
            let release = Arc::new(Semaphore::new(0));
            let tcp = fixture_with_barriers(
                vec![reply(rows(protocol, &[[1.0, 0.0, 0.0]]))],
                vec![release.clone()],
            )
            .await;
            let cancel = Cancellation::default();
            let setup = request(&cancel, Instant::now() + Duration::from_secs(30));
            let auth = embedding_fixture(&setup, protocol, &tcp.origin);
            let ctx = request(
                &cancel,
                Instant::now() + Duration::from_secs(if cancelled { 30 } else { 1 }),
            );
            let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
            let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
            let mut selection = issuer
                .capture_service(&caller, ServicePurpose::Embedding)
                .unwrap();
            let lease = selection.credential().unwrap();
            let transport = client(&tcp.origin);
            let call = direct_embedding::embed(
                &lease,
                &["text"],
                DirectEmbeddingInput::Query,
                &transport,
                limits(1),
            );
            let observe = async {
                tcp.observed.notified().await;
                if cancelled {
                    cancel.cancel();
                }
            };
            let (result, ()) = tokio::join!(call, observe);
            let result = error(result);
            assert_eq!(result.effect(), NetworkEffect::MayHaveApplied);
            match result {
                DirectEmbeddingError::Embedding {
                    source: EmbeddingError::Context(source),
                    ..
                } => assert_eq!(
                    source,
                    if cancelled {
                        ContextError::Cancelled
                    } else {
                        ContextError::Deadline
                    }
                ),
                _ => panic!("expected original inference context failure"),
            }
            release.add_permits(1);
            assert_eq!(tcp.server.await.unwrap().len(), 1);
        }
    }
}

#[tokio::test]
async fn later_status_codec_shape_and_finite_errors_preserve_typed_cause_without_partial_vectors() {
    for protocol in PROTOCOLS {
        let bad_width = if matches!(
            protocol,
            DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
        ) {
            json!({"embedding":{"values":[1.0,0.0]}})
        } else if protocol == DirectProtocol::Cohere {
            json!({"embeddings":{"float":[[1.0,0.0]]}})
        } else {
            json!({"data":[{"index":0,"embedding":[1.0,0.0]}]})
        };
        let overflow = if matches!(
            protocol,
            DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
        ) {
            "{\"embedding\":{\"values\":[1e100,0,0]}}"
        } else if protocol == DirectProtocol::Cohere {
            "{\"embeddings\":{\"float\":[[1e100,0,0]]}}"
        } else {
            "{\"data\":[{\"index\":0,\"embedding\":[1e100,0,0]}]}"
        };
        for (status, body, expected) in [
            (
                429,
                "private-status-body".into(),
                EmbeddingError::Status(StatusCode::TOO_MANY_REQUESTS),
            ),
            (200, "{".into(), EmbeddingError::Codec),
            (200, bad_width.to_string(), EmbeddingError::Shape),
            (200, overflow.into(), EmbeddingError::NonFinite),
        ] {
            let tcp = fixture(vec![
                reply(rows(protocol, &[[1.0, 0.0, 0.0]])),
                Reply {
                    status,
                    body,
                    delay: Duration::ZERO,
                },
            ])
            .await;
            let cancel = Cancellation::default();
            let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
            let auth = embedding_fixture(&ctx, protocol, &tcp.origin);
            let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
            let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
            let mut selection = issuer
                .capture_service(&caller, ServicePurpose::Embedding)
                .unwrap();
            let lease = selection.credential().unwrap();
            let result = error(
                direct_embedding::embed(
                    &lease,
                    &["first", "fixture-private-text", "must-not-send"],
                    DirectEmbeddingInput::Document,
                    &client(&tcp.origin),
                    limits(1),
                )
                .await,
            );
            match &result {
                DirectEmbeddingError::Embedding { source, effect } => {
                    assert_eq!(*source, expected);
                    assert_eq!(*effect, NetworkEffect::MayHaveApplied);
                }
                _ => panic!("expected typed SDK error"),
            }
            redacted(&result);
            assert!(
                std::error::Error::source(&result)
                    .unwrap()
                    .downcast_ref::<EmbeddingError>()
                    .is_some()
            );
            assert_eq!(tcp.server.await.unwrap().len(), 2);
        }
    }
}

#[tokio::test]
async fn transport_origin_rejection_retains_not_sent_and_network_cause() {
    let tcp = fixture(vec![]).await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::embedding(&ctx, DirectProtocol::Voyage, &tcp.origin);
    let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.credential().unwrap();
    let result = error(
        direct_embedding::embed(
            &lease,
            &["text"],
            DirectEmbeddingInput::Query,
            &client("http://127.0.0.1:1"),
            limits(1),
        )
        .await,
    );
    assert!(matches!(
        result,
        DirectEmbeddingError::Embedding {
            source: EmbeddingError::Network(NetworkError::Origin),
            ..
        }
    ));
    assert_eq!(result.effect(), NetworkEffect::NotSent);
    redacted(&result);
    assert!(tcp.server.await.unwrap().is_empty());
}

#[tokio::test]
async fn openai_compatible_google_leases_preserve_literal_options_routes_and_retrieval_modes() {
    for protocol in [
        DirectProtocol::OpenAi,
        DirectProtocol::OpenAiCompatible,
        DirectProtocol::GoogleGemini1,
        DirectProtocol::GoogleGemini2,
    ] {
        for input in [DirectEmbeddingInput::Query, DirectEmbeddingInput::Document] {
            let options: &[Option<u32>] = if protocol == DirectProtocol::OpenAi {
                &[None, Some(3)]
            } else {
                &[None]
            };
            for request_dimensions in options {
                let vectors = [[0.5, -0.0, 1.0], [0.25, 0.75, 1.0], [0.125, 0.875, 1.0]];
                let tcp = fixture(vec![
                    reply(rows(protocol, &vectors[..2])),
                    reply(rows(protocol, &vectors[2..])),
                ])
                .await;
                let google = matches!(
                    protocol,
                    DirectProtocol::GoogleGemini1 | DirectProtocol::GoogleGemini2
                );
                let endpoint = if google {
                    format!("{}/v1beta/models/fixture-model:embedContent", tcp.origin)
                } else {
                    format!("{}/literal/embedding?captured=yes", tcp.origin)
                };
                let batch_endpoint = format!(
                    "{}/v1beta/models/fixture-model:batchEmbedContents",
                    tcp.origin
                );
                let model = if google {
                    "models/fixture-model"
                } else if protocol == DirectProtocol::OpenAi {
                    "text-embedding-3-small"
                } else {
                    "literal-compatible-model"
                };
                let cancel = Cancellation::default();
                let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
                let auth = AuthFixture::embedding_config(
                    &ctx,
                    EmbeddingConfig {
                        protocol,
                        endpoint: &endpoint,
                        model,
                        dimensions: 3,
                        request_dimensions: *request_dimensions,
                        batch_endpoint: google.then_some(batch_endpoint.as_str()),
                    },
                );
                let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
                let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
                let mut selection = issuer
                    .capture_service(&caller, ServicePurpose::Embedding)
                    .unwrap();
                let lease = selection.credential().unwrap();
                let transport = client(&tcp.origin);
                let output = require_send(direct_embedding::embed(
                    &lease,
                    &["first", "second", "third"],
                    input,
                    &transport,
                    limits(2),
                ))
                .await
                .unwrap();
                assert_eq!(output, vectors);
                assert!(output[0][1].is_sign_negative());
                let requests = tcp.server.await.unwrap();
                assert_eq!(requests.len(), 2);
                for (index, captured) in requests.iter().enumerate() {
                    assert!(!captured.head.contains(MCP_KEY));
                    assert!(!captured.head.contains(RERANK_KEY));
                    let texts = if index == 0 {
                        vec!["first", "second"]
                    } else {
                        vec!["third"]
                    };
                    if google {
                        assert!(
                            captured
                                .head
                                .to_ascii_lowercase()
                                .contains(&format!("x-goog-api-key: {EMBEDDING_KEY}"))
                        );
                        let route = if index == 0 {
                            ":batchEmbedContents"
                        } else {
                            ":embedContent"
                        };
                        assert!(captured.head.starts_with(&format!(
                            "POST /v1beta/models/fixture-model{route} HTTP/1.1"
                        )));
                        let items: Vec<&Value> = if index == 0 {
                            captured.body["requests"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .collect()
                        } else {
                            vec![&captured.body]
                        };
                        for (item, text) in items.iter().zip(texts) {
                            if index == 0 {
                                assert_eq!(item["model"], model);
                            }
                            assert_eq!(item["embedContentConfig"]["outputDimensionality"], 3);
                            let prefix = if protocol == DirectProtocol::GoogleGemini2 {
                                if input == DirectEmbeddingInput::Query {
                                    "task: search result | query: "
                                } else {
                                    "title: none | text: "
                                }
                            } else {
                                ""
                            };
                            assert_eq!(
                                item["content"]["parts"][0]["text"],
                                format!("{prefix}{text}")
                            );
                            if protocol == DirectProtocol::GoogleGemini1 {
                                assert_eq!(
                                    item["embedContentConfig"]["taskType"],
                                    if input == DirectEmbeddingInput::Query {
                                        "RETRIEVAL_QUERY"
                                    } else {
                                        "RETRIEVAL_DOCUMENT"
                                    }
                                );
                            } else {
                                assert!(item["embedContentConfig"].get("taskType").is_none());
                            }
                            assert!(item.get("taskType").is_none());
                            assert!(item.get("outputDimensionality").is_none());
                        }
                    } else {
                        assert!(
                            captured
                                .head
                                .starts_with("POST /literal/embedding?captured=yes HTTP/1.1")
                        );
                        assert!(captured.head.contains(&format!("Bearer {EMBEDDING_KEY}")));
                        assert_eq!(captured.body["model"], model);
                        assert_eq!(captured.body["input"], json!(texts));
                        assert_eq!(captured.body["encoding_format"], "float");
                        if let Some(width) = request_dimensions {
                            assert_eq!(captured.body["dimensions"], *width);
                        } else {
                            assert!(captured.body.get("dimensions").is_none());
                        }
                        assert!(captured.body.get("input_type").is_none());
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn changed_captured_openai_dimension_or_google_batch_route_invalidates_lease_before_send() {
    for protocol in [
        DirectProtocol::OpenAi,
        DirectProtocol::GoogleGemini1,
        DirectProtocol::GoogleGemini2,
    ] {
        let tcp = fixture(vec![]).await;
        let cancel = Cancellation::default();
        let ctx = request(&cancel, Instant::now() + Duration::from_secs(30));
        let google = protocol != DirectProtocol::OpenAi;
        let endpoint = if google {
            format!("{}/v1beta/models/fixture-model:embedContent", tcp.origin)
        } else {
            tcp.origin.clone()
        };
        let batch = format!(
            "{}/v1beta/models/fixture-model:batchEmbedContents",
            tcp.origin
        );
        let auth = AuthFixture::embedding_config(
            &ctx,
            EmbeddingConfig {
                protocol,
                endpoint: &endpoint,
                model: if google {
                    "models/fixture-model"
                } else {
                    "text-embedding-3-small"
                },
                dimensions: 3,
                request_dimensions: None,
                batch_endpoint: google.then_some(batch.as_str()),
            },
        );
        let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
        let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::Embedding)
            .unwrap();
        let lease = selection.credential().unwrap();
        auth.change_embedding_options(&ctx);
        assert!(lease.check().is_err());
        for texts in [&[][..], &["fixture-private-text"][..]] {
            let result = error(
                direct_embedding::embed(
                    &lease,
                    texts,
                    DirectEmbeddingInput::Query,
                    &client(&tcp.origin),
                    limits(2),
                )
                .await,
            );
            assert!(matches!(result, DirectEmbeddingError::Service { .. }));
            assert_eq!(result.effect(), NetworkEffect::NotSent);
            redacted(&result);
        }
        assert!(tcp.server.await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn service_original_without_deadline_is_causal_not_sent() {
    let tcp = fixture(vec![]).await;
    let cancel = Cancellation::default();
    let setup = request(&cancel, Instant::now() + Duration::from_secs(30));
    let auth = AuthFixture::embedding(&setup, DirectProtocol::Cohere, &tcp.origin);
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
    let issuer = auth.issuer(&ctx, ServicePurpose::Embedding);
    let caller = issuer.authenticate_local(MCP_KEY, &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::Embedding)
        .unwrap();
    let lease = selection.credential().unwrap();
    assert_eq!(lease.original().unwrap().deadline(), None);
    let failure = error(
        direct_embedding::embed(
            &lease,
            &["fixture-private-text"],
            DirectEmbeddingInput::Query,
            &client(&tcp.origin),
            limits(1),
        )
        .await,
    );
    assert_eq!(failure.effect(), NetworkEffect::NotSent);
    assert!(
        matches!(&failure, DirectEmbeddingError::Context(source) if source.kind() == std::io::ErrorKind::InvalidInput)
    );
    let cause = std::error::Error::source(&failure)
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), std::io::ErrorKind::InvalidInput);
    redacted(&failure);
    assert!(tcp.server.await.unwrap().is_empty());
}
