//! Lexical-only Native publication reaches the actual Source HTTP rerank consumer.
use super::*;
use graphit_auth::profile::{
    AiService, DirectProtocol, LocalCredentials, ProfileCredentials, ProfileData,
    ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source, Secret,
};
use graphit_auth::store::AuthStore;
use graphit_auth::{CredentialSource, ExternalEffect, RemoteServiceBindingInput};
use graphit_core::{
    artifact_store::StoreOwner,
    auth_document::{AuthDocumentLimits, AuthDocumentStore},
};

#[allow(
    dead_code,
    reason = "shared bounded rerank TCP fixture includes optional barriers"
)]
use crate::rerank_test_http as wire;

const TEXT: &str = "pub fn lexical_first() -> i32 { 1 }\npub fn lexical_second() -> i32 { 2 }\n";
const KEY: &str = "lexical-rerank-fixture-key";
const QUERY: &str = "pub";

fn profile(source: &super::super::super::test_fixture::Fixture, endpoint: &str) {
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        source.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let installation = source.auth.installation();
    let StoreOwner::Installation { installation_id } = installation.live.owner() else {
        panic!("fixture installation")
    };
    let scope = CapturedPublicationScope::installation(
        &source.paths,
        installation_id.clone(),
        installation.live.allowed_roots().map(ToOwned::to_owned),
        source.server.0.policy.scope,
        request.snapshot(),
    )
    .unwrap();
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(1048576, 65536, 256, 65536).unwrap(),
            request.snapshot(),
        )
        .unwrap(),
        ProfileLimits {
            bytes: 1048576,
            nodes: 65536,
            scalar_bytes: 65536,
            collection_entries: 1024,
            providers: 16,
            profiles: 16,
        },
        &request,
    )
    .unwrap();
    let outcome = store
        .mutate(store.load(&request).unwrap(), &request, |doc, request| {
            doc.add_provider(
                "lexical".into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: false,
                        broker: None,
                    },
                    embedding: AiService::Disabled,
                    rerank: AiService::Direct {
                        protocol: DirectProtocol::Cohere,
                        endpoint: endpoint.into(),
                        model: "lexical-model".into(),
                        model_revision: "fixture-v1".into(),
                        recipe: "lexical-fixture-v1".into(),
                        dimensions: 0,
                        request_dimensions: None,
                        batch_endpoint: None,
                    },
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::LocalCredentials,
                    },
                },
                doc.revision(),
                1,
                request,
            )?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("lexical").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: "lexical".into(),
                    provider: "lexical".into(),
                    username: "fixture".into(),
                    organization: None,
                    teams: vec![],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: Some(Secret::new("lexical-fixture-mcp".into())),
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: Some(Secret::new(KEY.into())),
                        s3: None,
                    }),
                },
                expected,
                1,
                request,
            )?;
            doc.activate("lexical", doc.revision(), request)?;
            Ok(())
        })
        .unwrap();
    assert!(outcome.failure().is_none());
    assert!(outcome.cleanup_failure().is_none());
}

async fn publish(source: &super::super::super::test_fixture::Fixture) {
    raw_file_for_source(source, 0, "code.rs", Some(TEXT), TEXT).await;
    let cancel = Cancellation::default();
    let request = RequestContext::new(
        source.server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let original = request.snapshot();
    let (paths, id, version) = &source.projects[0];
    let scope =
        CapturedPublicationScope::project(paths, *id, source.server.0.policy.scope, original)
            .unwrap();
    let binding = scope.binding().unwrap();
    let project = ProjectCapture {
        scope: ScopeCapture {
            live: &scope,
            record: &binding,
        },
        version,
    };
    let SourceIndexPresence::Available(raw_index) = open_local_source_index(
        source.auth.installation(),
        project,
        original,
        source.server.0.policy.index,
    )
    .await
    .unwrap() else {
        panic!("published Raw")
    };
    let raw = raw_index.read("code.rs", original).await.unwrap();
    let IndexedSource::Available(text) = raw.source(original).unwrap() else {
        panic!("published Raw text")
    };
    let draft = semantic_catalog("lexical-rerank", b"language: rust\ngrammar: tree-sitter-rust\nembed_labels: [Function]\ncontext_types: {function_item: Function}\nqueries:\n  - {data_key: functions, graph_label: Function, pattern: '(function_item name: (identifier) @name)'}\n");
    let prepared = PreparedTreeSitterContexts::prepare(
        &draft.profiles()[0],
        GrammarLanguage::from_language(tree_sitter_rust::LANGUAGE.into()).unwrap(),
        original,
    )
    .unwrap();
    let span = ByteSpan {
        start: 0,
        end: text.len() as u64,
    };
    let segments = [OriginSegment::Direct {
        prepared: span,
        original: span,
    }];
    let input = PreparedInput::new(
        text.as_bytes(),
        text.as_bytes(),
        OriginPlan {
            segments: &segments,
            removed_original: &[],
            anchors: &[],
        },
        original,
    )
    .unwrap();
    let parsed = ParsedTreeSitterDocument::collect(
        &prepared,
        &input,
        &["code.rs"],
        semantic_identity_limits(),
    )
    .unwrap();
    let document = parsed.assemble().unwrap();
    let projections = document.code_projections().unwrap();
    assert_eq!(projections.len(), 2);
    assert!(
        projections
            .iter()
            .all(|projection| matches!(projection, TsProjection::Available(_)))
    );
    let graph = assemble_graph_ts(
        AstGraphTsInput {
            document: &document,
            source: NativeNnSource::Indexed(&raw),
        },
        original,
    )
    .unwrap();
    // Preserve the genuine FILE association and published text while modeling a
    // lexical observation whose parser cannot establish an exact Source span.
    publish_graph_for_source(source, 0, graph, text, original, true).await;
    // Establish the exact query's two rows in the published eligible corpus,
    // independently of the HTTP consumer and its subsequent rerank composition.
    let branch = version.lance_branch();
    let path = source_path(source, 0, "corpus.lance");
    let corpus = graphit_data::lance_branch_store::BranchSnapshot::capture_observed(
        path.to_str().unwrap(),
        (!branch.is_empty()).then_some(branch.as_str()),
        None,
    )
    .await
    .unwrap();
    let hits = graphit_ast::entity_search::query(&corpus.module_snapshot(), QUERY, 2, original)
        .await
        .unwrap();
    assert_eq!(
        hits.len(),
        2,
        "exact content token must exist in eligible corpus"
    );
    assert!(hits.iter().all(|hit| hit.entity.extent_status == "Unknown"));
}

#[tokio::test]
async fn native_source_http_reranks_lexical_unknown_extents_and_reports_endpoint_errors() {
    let wire = wire::fixture_with_deadline(vec![
        wire::reply(json!({"results":[{"index":0,"relevance_score":0.2},{"index":1,"relevance_score":0.9}]})),
        wire::Reply { status: 503, body: "{\"error\":\"fixture unavailable\"}".into(), delay: Duration::ZERO },
    ], Instant::now() + Duration::from_secs(30)).await;
    let endpoint = format!("{}/lexical/rerank", wire.origin);
    let rules = [ServiceRuleInput::Remote {
        origin: ServiceOrigin::Runtime,
        binding: RemoteServiceBindingInput::Direct {
            provider: "lexical",
            purpose: ServicePurpose::Rerank,
            protocol: DirectProtocol::Cohere,
            endpoint: &endpoint,
            model: "lexical-model",
            model_revision: "fixture-v1",
            recipe: "lexical-fixture-v1",
            dimensions: 0,
            request_dimensions: None,
            batch_endpoint: None,
        },
        credential: CredentialSource::DirectKey,
        incoming: None,
    }];
    let source = super::super::super::test_fixture::fixture_with_projects_and_rules(
        &[
            Permission::Read(Module::Ast),
            Permission::External(ExternalEffect::RemoteRequest),
        ],
        Brand::default(),
        NativeBinding::KiroV3,
        &["project"],
        &rules,
    );
    profile(&source, &endpoint);
    let global = ConfigStore::global(&source.paths).unwrap();
    for (key, value) in [
        ("broker.allow_loopback_http", true),
        ("search.rerank", true),
    ] {
        global
            .set(
                ConfigKey::new(key).unwrap(),
                json!(value),
                Duration::from_secs(5),
                &Cancellation::default(),
            )
            .unwrap();
    }
    global
        .set(
            Module::Embedding.key(),
            json!(false),
            Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    publish(&source).await;
    let (client, serving) = client(&source.server, &source.raw, false).await;
    let off = client
        .call_tool(params(
            json!({"query":QUERY,"mode":"fts","k":2,"rerank":false}),
        ))
        .await
        .unwrap();
    assert_eq!(off.is_error, Some(false), "{off:?}");
    let off = off.structured_content.unwrap();
    let candidates = off["results"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    for candidate in candidates {
        assert_eq!(candidate["extent_status"], "Unknown");
        assert_eq!(candidate["data_key"], "functions");
        assert!(candidate["rerank_score"].is_null());
    }
    let on = client
        .call_tool(params(
            json!({"query":QUERY,"mode":"fts","k":2,"rerank":true}),
        ))
        .await
        .unwrap();
    assert_eq!(on.is_error, Some(false), "{on:?}");
    let on = on.structured_content.unwrap();
    assert!(on.get("graph_generation").is_none());
    assert!(on.get("search_generation").is_none());
    assert_eq!(on["results"].as_array().unwrap().len(), 2);
    for (position, original, score) in [(0, 1, 0.9), (1, 0, 0.2)] {
        let mut expected = candidates[original].clone();
        expected["rerank_score"] = json!(score);
        expected["rerank_rank"] = json!(position + 1);
        assert_eq!(on["results"][position], expected);
    }
    // Omission uses the configured setting and propagates a real endpoint error.
    let error = client
        .call_tool(params(json!({"query":QUERY,"mode":"fts","k":2})))
        .await
        .unwrap();
    assert_eq!(error.is_error, Some(true), "{error:?}");
    let error = error.structured_content.unwrap();
    assert_eq!(error["status"], "error");
    assert_eq!(error["http_status"], 409);
    assert_eq!(error["message"], "search reranking failed");
    assert!(error["results"].is_null());
    let requests = wire.server.await.unwrap();
    assert_eq!(
        requests.len(),
        2,
        "explicit off must send no rerank request"
    );
    for request in requests {
        assert!(request.head.starts_with("POST /lexical/rerank HTTP/1.1"));
        assert!(request.head.contains(&format!("Bearer {KEY}")));
        assert!(!request.head.contains(&source.raw));
        assert_eq!(request.body["query"], QUERY);
        assert_eq!(request.body["documents"].as_array().unwrap().len(), 2);
        for (position, candidate) in candidates.iter().enumerate() {
            let text = match candidate["name"].as_str().unwrap() {
                "lexical_first" => "pub fn lexical_first() -> i32 { 1 }",
                "lexical_second" => "pub fn lexical_second() -> i32 { 2 }",
                name => panic!("unexpected lexical candidate {name}"),
            };
            assert_eq!(request.body["documents"][position], text);
        }
    }
    client.cancel().await.unwrap();
    source.server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
