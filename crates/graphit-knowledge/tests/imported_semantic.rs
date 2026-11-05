//! Signed Web, registry and SDK fixtures for Imported semantic kernels.
//! Vectors are ordinary fixture data; App qualifies the actual Models.Query RPC.
#![cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "support/imported_knowledge_fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "imported_reference_query/sdk.rs"]
#[allow(dead_code)]
mod sdk;
use arrow_array::{
    ArrayRef, RecordBatch, StringArray,
    builder::{FixedSizeBinaryBuilder, FixedSizeListBuilder, Float32Builder},
};
use fixture::*;
use graphit_auth::broker::BrokerRequestContext;
use graphit_core::artifact_store::{
    CopyLimits, CopyPolicy, RetainedBytesLimits, RetainedFileSet, SnapshotPolicy, SourceReadLimits,
};
use graphit_core::file_lock::Cancellation;
use graphit_data::{
    knowledge_embeddings::*, knowledge_reference_query::KnowledgeReferenceOwnedScope,
    knowledge_references::*, knowledge_search::*, lance_module_store::LanceModuleStore,
};
use graphit_knowledge::{
    embedding_projection::KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE, imported_knowledge::*,
    preview::MarkdownPreviewLimits,
};
use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

fn limits() -> ImportedKnowledgeReadLimits {
    ImportedKnowledgeReadLimits {
        directory: native(),
        index: page_limits(),
    }
}
fn input<'a>(f: &'a Fixture, alias: Option<&'a str>) -> ImportedKnowledgeInput<'a> {
    ImportedKnowledgeInput {
        registry: &f.registry,
        id: &f.id,
        version: &f.artifact_version,
        context_alias: alias,
    }
}
fn identity() -> KnowledgeEmbeddingIdentity {
    KnowledgeEmbeddingIdentity::new(
        [8; 32],
        NonZeroU32::new(2).unwrap(),
        KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE.to_owned(),
    )
    .unwrap()
}
fn search_limits() -> KnowledgeSearchLimits {
    KnowledgeSearchLimits {
        read: page_limits(),
        max_query_bytes: n(4096),
        max_output_bytes: n(65536),
    }
}
fn preview_limits() -> MarkdownPreviewLimits {
    MarkdownPreviewLimits {
        max_input_bytes: n(65536),
        max_output_bytes: n(4096),
        max_body_scalars: n(256),
    }
}
fn page_query(cursor: Option<&KnowledgeSearchCursor>) -> KnowledgeLexicalQuery<'_> {
    KnowledgeLexicalQuery {
        text: "corpo",
        top_k: 0,
        page_size: n(1),
        cursor,
        caller_binding: "imported source selection",
        preview: true,
    }
}
fn vector_query<'a>(
    cursor: Option<&'a KnowledgeSearchCursor>,
    mode: KnowledgeVectorSearchMode,
    vector: Option<&'static [f32]>,
) -> KnowledgeVectorSearchQuery<'a> {
    KnowledgeVectorSearchQuery {
        page: page_query(cursor),
        mode,
        vector,
        metric: KnowledgeEmbeddingMetric::L2,
    }
}
async fn collect_inputs(
    capture: &CapturedImportedKnowledgeSemantic<'_, '_>,
    identity: &KnowledgeEmbeddingIdentity,
) -> Vec<ImportedKnowledgeEmbeddingInput> {
    let mut inputs = Vec::new();
    while let Some(batch) = capture.next_inputs(identity).await.unwrap() {
        assert!(batch.len() <= 1);
        inputs.extend(batch);
    }
    inputs.sort_unstable_by(|a, b| a.slug.cmp(&b.slug));
    inputs
}
fn batch(
    identity: &KnowledgeEmbeddingIdentity,
    rows: &[&ImportedKnowledgeEmbeddingInput],
    vectors: &[[f32; 2]],
) -> RecordBatch {
    assert_eq!(rows.len(), vectors.len());
    let mut fingerprints = FixedSizeBinaryBuilder::new(32);
    let mut configs = FixedSizeBinaryBuilder::new(32);
    let mut values = FixedSizeListBuilder::new(Float32Builder::new(), 2);
    for (row, vector) in rows.iter().zip(vectors) {
        fingerprints.append_value(row.input_fingerprint).unwrap();
        configs.append_value(identity.vector_id()).unwrap();
        for value in vector {
            values.values().append_value(*value);
        }
        values.append(true);
    }
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.slug.as_str()).collect::<Vec<_>>(),
        )),
        Arc::new(fingerprints.finish()),
        Arc::new(configs.finish()),
        Arc::new(StringArray::from(vec![
            identity.projection_recipe();
            rows.len()
        ])),
        Arc::new(StringArray::from(vec!["Document"; rows.len()])),
        Arc::new(values.finish()),
    ];
    RecordBatch::try_new(native_knowledge_embedding_schema(identity), arrays).unwrap()
}
/// Fixture publication changes the real documentary/vector dataset. It does
/// not change Registry authority or manufacture a reader pin.
async fn create_nn(
    f: &Fixture,
    identity: &KnowledgeEmbeddingIdentity,
    batches: Vec<RecordBatch>,
) -> LanceModuleStore {
    use futures::TryStreamExt;
    use graphit_data::lance_branch_store::BranchSnapshot;
    let uri = f
        .source
        .directory()
        .as_path()
        .join("index.lance")
        .to_str()
        .unwrap()
        .to_owned();
    let pin = BranchSnapshot::capture_observed(&uri, None, None)
        .await
        .unwrap();
    let existing = pin
        .scan()
        .try_into_stream()
        .await
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    let source_rows: Vec<_> = existing
        .iter()
        .flat_map(|batch| (0..batch.num_rows()).map(move |row| batch.slice(row, 1)))
        .collect();
    let mut fields = graphit_data::knowledge_index::physical_schema()
        .fields()
        .to_vec();
    fields.extend(
        native_knowledge_embedding_schema(identity).fields()[1..]
            .iter()
            .map(|field| Arc::new(field.as_ref().clone().with_nullable(true))),
    );
    let mut schema = graphit_data::knowledge_index::physical_schema()
        .as_ref()
        .clone();
    schema.fields = fields.into();
    let schema = Arc::new(schema);
    let mut image = Vec::new();
    let mut supplied = std::collections::BTreeSet::new();
    for batch in batches {
        let slugs = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for row in 0..batch.num_rows() {
            let slug = slugs.value(row);
            supplied.insert(slug.to_owned());
            let source = source_rows
                .iter()
                .find(|source| {
                    source
                        .column(0)
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .unwrap()
                        .value(0)
                        == slug
                })
                .unwrap();
            let mut columns = source.columns()[..18].to_vec();
            columns.extend(batch.slice(row, 1).columns()[1..].iter().cloned());
            image.push(RecordBatch::try_new(Arc::clone(&schema), columns).unwrap());
        }
    }
    for source in source_rows {
        let slug = source
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(0);
        if !supplied.contains(slug) {
            let mut columns = source.columns()[..18].to_vec();
            columns.extend(
                schema.fields()[18..]
                    .iter()
                    .map(|field| arrow_array::new_null_array(field.data_type(), 1)),
            );
            image.push(RecordBatch::try_new(Arc::clone(&schema), columns).unwrap());
        }
    }
    if image.is_empty() {
        image.push(RecordBatch::new_empty(Arc::clone(&schema)));
    }
    pin.prepare_overwrite(image)
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
    LanceModuleStore::open(&uri, schema).await.unwrap()
}
macro_rules! reopen {
    ($f:ident,$read:ident,$reader:ident) => {
        let ImportedKnowledgeOpen::Published($reader) =
            open_imported_knowledge(&$read, input(&$f, None), limits())
                .await
                .unwrap()
        else {
            panic!("actual unified corpus")
        };
    };
}
macro_rules! web_read {
    ($f:ident,$ctx:ident,$issuer:ident,$server:ident,$revision:ident,$verifier:ident,$flow:ident,$registered:ident,$raw:ident,$proof:ident,$read:ident) => {
        let $verifier = $server.verifier($revision);
        let $flow = web(&$verifier);
        let $registered = $flow
            .register(BrokerRequestContext::new($ctx.snapshot()))
            .await
            .unwrap();
        let $raw = $server.access($registered.resource());
        let $proof = $flow
            .verify_access(
                &$registered,
                &$raw,
                BrokerRequestContext::new($ctx.snapshot()),
            )
            .await
            .unwrap();
        let $read = $issuer
            .admit_web_imported_knowledge_read(&$proof, &$verifier, &$ctx, "service", $f.realm())
            .unwrap();
    };
}

#[test]
fn imported_single_corpus_recipe_rank_cursor_namespace_and_retained_pin() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context_deadline(&cancel, Instant::now() + Duration::from_secs(600));
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        let pages = [page("a", &[]), page("b", &[])];
        let entries = [
            fields("a", KnowledgeDocumentState::Active, &[]),
            fields("b", KnowledgeDocumentState::Active, &[]),
        ];
        f.seed(&issuer, &ctx, &pages, Some(&entries)).await;
        let payload_project = "01KSH1CRFFG8Z74B5ZS78WW809".parse().unwrap();
        let rows = sdk::rows(&f).await;
        sdk::replace(
            &f,
            rows.iter()
                .map(|row| {
                    sdk::namespace(
                        row,
                        KnowledgeReferenceScope::Project(payload_project),
                        Some(" payload source β "),
                        &[],
                        &[],
                    )
                })
                .collect(),
        )
        .await;
        web_read!(
            f, ctx, issuer, server, revision, verifier, flow, registered, raw, proof, read
        );
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, Some("mount A")), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        assert_ne!(reader.observation().producer, payload_project);
        let id = identity();
        let missing = reader.capture_semantic(page_limits()).await.unwrap();
        assert!(
            missing.search_readiness(&id, &[]).is_err(),
            "metadata audit is not the document EOF"
        );
        let current = collect_inputs(&missing, &id).await;
        let borrowed: Vec<_> = current.iter().map(|i| i.as_input()).collect();
        assert_eq!(
            missing.search_readiness(&id, &borrowed).unwrap(),
            KnowledgeEmbeddingSearchReadiness::Unavailable(
                KnowledgeEmbeddingUnavailable::MissingSemantic
            )
        );
        let store = create_nn(
            &f,
            &id,
            vec![batch(
                &id,
                &current.iter().collect::<Vec<_>>(),
                &[[1.0, 0.0], [0.0, 1.0]],
            )],
        )
        .await;
        reopen!(f, read, reader);
        let capture = reader.capture_semantic(page_limits()).await.unwrap();
        let current = collect_inputs(&capture, &id).await;
        let borrowed: Vec<_> = current.iter().map(|i| i.as_input()).collect();
        assert_eq!(
            capture.search_readiness(&id, &borrowed).unwrap(),
            KnowledgeEmbeddingSearchReadiness::Ready
        );
        let before = image(f.installation.owner_root());
        let keys = server.count("/keys");
        let first = capture
            .search(
                &id,
                &borrowed,
                vector_query(None, KnowledgeVectorSearchMode::Semantic, Some(&[1.0, 0.0])),
                search_limits(),
                preview_limits(),
            )
            .await
            .unwrap();
        assert_eq!(first.state, KnowledgeVectorSearchState::Hits);
        assert_eq!(first.total, 2);
        assert_eq!(first.generation, reader.observation().generation);
        assert_eq!(first.hits[0].source.slug, "a");
        assert_eq!(first.hits[0].semantic_distance, Some(0.0));
        assert!(first.hits[0].lexical_score.is_none());
        assert!(first.hits[0].rrf_score.is_none());
        let namespace = first.hits[0].source.namespace.as_ref().unwrap();
        assert_eq!(
            namespace.scope,
            KnowledgeReferenceOwnedScope::Project(payload_project)
        );
        assert_eq!(namespace.context.as_deref(), Some(" payload source β "));
        assert!(first.hits[0].source.witness.is_some());
        assert_eq!(first.hits[0].preview.as_ref().unwrap().text, "corpo");
        let cursor = first.next_cursor.as_ref().unwrap();
        let next = capture
            .search(
                &id,
                &borrowed,
                vector_query(
                    Some(cursor),
                    KnowledgeVectorSearchMode::Semantic,
                    Some(&[1.0, 0.0]),
                ),
                search_limits(),
                preview_limits(),
            )
            .await
            .unwrap();
        assert_eq!(next.hits[0].source.slug, "b");
        assert!(next.next_cursor.is_none());
        let hybrid = capture
            .search(
                &id,
                &borrowed,
                vector_query(None, KnowledgeVectorSearchMode::Hybrid, Some(&[1.0, 0.0])),
                search_limits(),
                preview_limits(),
            )
            .await
            .unwrap();
        assert!(hybrid.hits[0].lexical_score.is_some());
        assert!(hybrid.hits[0].semantic_distance.is_some());
        assert!(hybrid.hits[0].rrf_score.is_some());
        let no_model = capture
            .source_search(page_query(None), search_limits(), preview_limits())
            .await
            .unwrap();
        assert_eq!(no_model.total, 2);
        assert_eq!(
            no_model.hits[0].source.namespace.as_ref().unwrap().scope,
            KnowledgeReferenceOwnedScope::Project(payload_project)
        );
        let mut small = search_limits();
        small.max_output_bytes = n(1);
        assert!(
            capture
                .search(
                    &id,
                    &borrowed,
                    vector_query(None, KnowledgeVectorSearchMode::Semantic, Some(&[1.0, 0.0])),
                    small,
                    preview_limits()
                )
                .await
                .is_err()
        );
        assert_eq!(image(f.installation.owner_root()), before);
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
        let ImportedKnowledgeOpen::Published(alias_reader) =
            open_imported_knowledge(&read, input(&f, Some("different alias")), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let alias = alias_reader.capture_semantic(page_limits()).await.unwrap();
        let alias_current = collect_inputs(&alias, &id).await;
        let alias_borrowed: Vec<_> = alias_current.iter().map(|i| i.as_input()).collect();
        assert!(matches!(
            alias
                .search(
                    &id,
                    &alias_borrowed,
                    vector_query(
                        Some(cursor),
                        KnowledgeVectorSearchMode::Semantic,
                        Some(&[1.0, 0.0])
                    ),
                    search_limits(),
                    preview_limits()
                )
                .await,
            Err(ImportedKnowledgeSemanticError::Query(
                KnowledgeSearchError::InvalidCursor
            ))
        ));
        let changed = create_nn(
            &f,
            &id,
            vec![batch(
                &id,
                &current.iter().collect::<Vec<_>>(),
                &[[0., 1.], [1., 0.]],
            )],
        )
        .await;
        assert!(changed.snapshot().version() > store.snapshot().version());
        let pinned = capture
            .search(
                &id,
                &borrowed,
                vector_query(None, KnowledgeVectorSearchMode::Semantic, Some(&[1.0, 0.0])),
                search_limits(),
                preview_limits(),
            )
            .await
            .unwrap();
        assert_eq!(pinned.hits[0].source.slug, "a");
        assert_eq!(pinned.semantic_generation, first.semantic_generation);
        let physical = capture
            .capture_physical_source(&physical_policy().0, physical_policy().1)
            .await
            .unwrap();
        assert_eq!(
            physical.generation(),
            capture.source_generation(),
            "advancing NN does not invalidate the retained documentary snapshot"
        );
        f.remove_registration(&ctx);
        assert!(matches!(
            capture
                .source_search(page_query(None), search_limits(), preview_limits())
                .await,
            Err(ImportedKnowledgeSemanticError::Reader(
                ImportedKnowledgeReadError::SourceSelectionChanged
            ))
        ));
    });
}

#[test]
fn imported_empty_removed_and_missing_nn_keep_real_eof_without_identity_claims() {
    native_test(async {
        for removed in [false, true] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let revision = f.provider(&server, &ctx);
            let issuer = f.issuer(&ctx);
            if removed {
                f.seed(
                    &issuer,
                    &ctx,
                    &[page("removed", &[])],
                    Some(&[fields("removed", KnowledgeDocumentState::Removed, &[])]),
                )
                .await;
            } else {
                f.seed(&issuer, &ctx, &[], None).await;
            }
            web_read!(
                f, ctx, issuer, server, revision, verifier, flow, registered, raw, proof, read
            );
            let ImportedKnowledgeOpen::Published(_reader) =
                open_imported_knowledge(&read, input(&f, None), limits())
                    .await
                    .unwrap()
            else {
                panic!()
            };
            let id = identity();
            let store = create_nn(
                &f,
                &id,
                vec![RecordBatch::new_empty(native_knowledge_embedding_schema(
                    &id,
                ))],
            )
            .await;
            reopen!(f, read, reader);
            let capture = reader.capture_semantic(page_limits()).await.unwrap();
            assert!(capture.search_readiness(&id, &[]).is_err());
            if removed {
                assert!(capture.next_documents().await.unwrap().unwrap().is_empty());
            }
            assert!(capture.next_documents().await.unwrap().is_none());
            let source = capture
                .source_search(page_query(None), search_limits(), preview_limits())
                .await
                .unwrap();
            assert!(source.hits.is_empty());
            assert_eq!(
                capture.semantic_generation().unwrap().version,
                store.snapshot().version()
            );
            assert_eq!(
                capture.search_readiness(&id, &[]).unwrap(),
                KnowledgeEmbeddingSearchReadiness::PublishedEmpty
            );
            let page = capture
                .search(
                    &id,
                    &[],
                    vector_query(None, KnowledgeVectorSearchMode::Semantic, None),
                    search_limits(),
                    preview_limits(),
                )
                .await
                .unwrap();
            assert_eq!(page.state, KnowledgeVectorSearchState::PublishedEmpty);
            assert!(page.hits.is_empty());
            cancel.cancel();
            assert!(matches!(
                capture
                    .source_search(page_query(None), search_limits(), preview_limits())
                    .await,
                Err(ImportedKnowledgeSemanticError::Reader(
                    ImportedKnowledgeReadError::Auth(_)
                ))
            ));
        }
    });
}

#[test]
fn imported_stale_fp_is_excluded_and_late_invalid_nn_never_returns_a_healthy_prefix() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context_deadline(&cancel, Instant::now() + Duration::from_secs(600));
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        f.seed(&issuer, &ctx, &[page("a", &[]), page("b", &[])], None)
            .await;
        web_read!(
            f, ctx, issuer, server, revision, verifier, flow, registered, raw, proof, read
        );
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, None), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let id = identity();
        let initial = reader.capture_semantic(page_limits()).await.unwrap();
        let current = collect_inputs(&initial, &id).await;
        let mut stale: Vec<_> = current
            .iter()
            .map(|i| ImportedKnowledgeEmbeddingInput {
                slug: i.slug.clone(),
                input_fingerprint: i.input_fingerprint,
            })
            .collect();
        stale[1].input_fingerprint[0] ^= 1;
        create_nn(
            &f,
            &id,
            vec![batch(
                &id,
                &stale.iter().collect::<Vec<_>>(),
                &[[1.0, 0.0], [0.0, 1.0]],
            )],
        )
        .await;
        reopen!(f, read, reader);
        let stale_capture = reader.capture_semantic(page_limits()).await.unwrap();
        let actual = collect_inputs(&stale_capture, &id).await;
        let borrowed: Vec<_> = actual.iter().map(|i| i.as_input()).collect();
        assert_eq!(
            stale_capture.search_readiness(&id, &borrowed).unwrap(),
            KnowledgeEmbeddingSearchReadiness::Ready
        );
        let mut query = vector_query(None, KnowledgeVectorSearchMode::Hybrid, Some(&[1., 0.]));
        query.page.page_size = n(2);
        let mixed = stale_capture
            .search(&id, &borrowed, query, search_limits(), preview_limits())
            .await
            .unwrap();
        assert_eq!(mixed.state, KnowledgeVectorSearchState::Hits);
        assert_eq!(mixed.total, 2);
        assert_eq!(mixed.hits.len(), 2);
        assert_eq!(
            mixed
                .hits
                .iter()
                .filter(|hit| hit.semantic_distance.is_some())
                .count(),
            1
        );
        assert!(mixed.hits.iter().any(|hit| hit.semantic_distance.is_none()));
        assert!(mixed.hits[0].source.namespace.is_none());
        create_nn(
            &f,
            &id,
            vec![
                batch(&id, &[&current[0]], &[[1., 0.]]),
                batch(&id, &[&current[1]], &[[f32::INFINITY, 0.]]),
            ],
        )
        .await;
        reopen!(f, read, bad_reader);
        assert!(
            bad_reader.capture_semantic(page_limits()).await.is_err(),
            "a late invalid vector rejects the full semantic capture"
        );
        let source = bad_reader.capture_source(page_limits()).await.unwrap();
        while source.next_documents().await.unwrap().is_some() {}
        create_nn(
            &f,
            &id,
            vec![
                batch(&id, &[&current[0]], &[[1., 0.]]),
                batch(&id, &[&current[0]], &[[1., 0.]]),
            ],
        )
        .await;
        reopen!(f, read, duplicate_reader);
        assert!(
            duplicate_reader
                .capture_semantic(page_limits())
                .await
                .is_err(),
            "late duplicate identity cannot become a healthy prefix"
        );
    });
}

#[test]
fn imported_dropped_input_read_and_registry_change_preserve_terminal_causes() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        f.seed(&issuer, &ctx, &[page("a", &[])], None).await;
        web_read!(
            f, ctx, issuer, server, revision, verifier, flow, registered, raw, proof, read
        );
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, None), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let id = identity();
        let capture = reader.capture_semantic(page_limits()).await.unwrap();
        let mut pending = Box::pin(capture.next_inputs(&id));
        assert!(matches!(
            futures::poll!(&mut pending),
            std::task::Poll::Pending
        ));
        drop(pending);
        assert!(matches!(
            capture.next_inputs(&id).await,
            Err(ImportedKnowledgeSemanticError::Embedding(
                KnowledgeEmbeddingError::InvalidInput("invalid document cursor")
            ))
        ));
        assert!(capture.search_readiness(&id, &[]).is_err());
        let current = reader.capture_semantic(page_limits()).await.unwrap();
        f.remove_registration(&ctx);
        assert!(matches!(
            current.next_inputs(&id).await,
            Err(ImportedKnowledgeSemanticError::Reader(
                ImportedKnowledgeReadError::SourceSelectionChanged
            ))
        ));
        assert_eq!(server.count("/userinfo"), 0);
    });
}

#[test]
fn imported_source_export_ignores_invalid_vector_values_and_keeps_terminal_guards() {
    native_test(async {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let revision = f.provider(&server, &ctx);
        let issuer = f.issuer(&ctx);
        f.seed(
            &issuer,
            &ctx,
            &[page("active", &[]), page("removed", &[])],
            Some(&[
                fields("active", KnowledgeDocumentState::Active, &[]),
                fields("removed", KnowledgeDocumentState::Removed, &[]),
            ]),
        )
        .await;
        web_read!(
            f, ctx, issuer, server, revision, verifier, flow, registered, raw, proof, read
        );
        let ImportedKnowledgeOpen::Published(reader) =
            open_imported_knowledge(&read, input(&f, None), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let id = identity();
        let initial = reader.capture_semantic(page_limits()).await.unwrap();
        let inputs = collect_inputs(&initial, &id).await;
        create_nn(&f, &id, vec![batch(&id, &[&inputs[0]], &[[f32::NAN, 0.]])]).await;
        reopen!(f, read, reader);
        assert!(reader.capture_semantic(page_limits()).await.is_err());
        let mut read_limits = page_limits();
        read_limits.batch_rows = NonZeroU32::new(1).unwrap();
        let exports = tempfile::tempdir().unwrap();
        let outside = exports.path().join("outside.md");
        reader.validate_export_destination(&outside).unwrap();
        assert!(!outside.exists());
        assert!(
            matches!(reader.validate_export_destination(&f.source.directory().as_path().join("inside.md")),
            Err(ImportedKnowledgeReadError::Capture(error)) if error.kind() == std::io::ErrorKind::InvalidInput)
        );
        #[cfg(unix)]
        {
            let alias = exports.path().join("alias");
            std::os::unix::fs::symlink(f.source.directory().as_path(), &alias).unwrap();
            assert!(
                reader
                    .validate_export_destination(&alias.join("inside.md"))
                    .is_err()
            );
            let final_link = exports.path().join("link.md");
            std::os::unix::fs::symlink(f.source.directory().as_path(), &final_link).unwrap();
            assert!(reader.validate_export_destination(&final_link).is_err());
        }
        std::fs::write(&outside, b"occupied file").unwrap();
        reader.validate_export_destination(&outside).unwrap();
        assert_eq!(std::fs::read(&outside).unwrap(), b"occupied file");
        let occupied_directory = exports.path().join("occupied");
        std::fs::create_dir(&occupied_directory).unwrap();
        std::fs::write(occupied_directory.join("sentinel"), b"unchanged").unwrap();
        reader
            .validate_export_destination(&occupied_directory)
            .unwrap();
        assert_eq!(
            std::fs::read(occupied_directory.join("sentinel")).unwrap(),
            b"unchanged"
        );
        assert!(
            matches!(reader.validate_export_destination(&exports.path().join("absent/out.md")),
            Err(ImportedKnowledgeReadError::Capture(error)) if error.kind() == std::io::ErrorKind::NotFound)
        );
        let mut small = read_limits;
        small.content.max_identifier_bytes = n(1);
        assert!(matches!(
            reader.capture_source(small).await,
            Err(ImportedKnowledgeSourceError::Embedding(_))
        ));
        let capture = reader.capture_source(read_limits).await.unwrap();
        let generation = capture.source_generation().clone();
        let rows = sdk::rows(&f).await;
        assert!(sdk::replace(&f, rows.clone()).await > generation.version);
        let before = image(f.installation.owner_root());
        let mut documents = vec![];
        let mut saw_removed = false;
        while let Some(batch) = capture.next_documents().await.unwrap() {
            assert!(batch.len() <= 1);
            saw_removed |= batch.is_empty();
            documents.extend(batch);
        }
        assert!(saw_removed);
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].slug, "active");
        assert_eq!(documents[0].body, "# corpo integral é\n");
        assert_eq!(capture.source_generation(), &generation);
        assert_eq!(image(f.installation.owner_root()), before);
        sdk::replace(&f, vec![sdk::row(&rows, "removed").clone()]).await;
        let ImportedKnowledgeOpen::Published(removed_reader) =
            open_imported_knowledge(&read, input(&f, None), limits())
                .await
                .unwrap()
        else {
            panic!()
        };
        let removed = removed_reader.capture_source(read_limits).await.unwrap();
        assert!(removed.next_documents().await.unwrap().unwrap().is_empty());
        assert!(removed.next_documents().await.unwrap().is_none());
        let poisoned = reader.capture_source(read_limits).await.unwrap();
        let mut pending = Box::pin(poisoned.next_documents());
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(pending.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        drop(pending);
        assert!(matches!(
            poisoned.next_documents().await,
            Err(ImportedKnowledgeSourceError::Embedding(
                KnowledgeEmbeddingError::InvalidInput("invalid document cursor")
            ))
        ));
        f.remove_registration(&ctx);
        assert!(matches!(
            capture.next_documents().await,
            Err(ImportedKnowledgeSourceError::Reader(
                ImportedKnowledgeReadError::SourceSelectionChanged
            ))
        ));
        assert_eq!(server.count("/userinfo"), 0);
        cancel.cancel();
        assert!(matches!(
            reader.validate_export_destination(&outside),
            Err(ImportedKnowledgeReadError::Auth(_))
        ));
        let Err(ImportedKnowledgeSourceError::Embedding(error)) = capture.recheck() else {
            panic!("Original cancellation must retain its Data cause");
        };
        let mut cause: &(dyn std::error::Error + 'static) = &error;
        loop {
            if let Some(error) = cause.downcast_ref::<std::io::Error>() {
                assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
                break;
            }
            cause = cause
                .source()
                .expect("Original interruption cause must be retained");
        }
    });
}

fn physical_policy() -> (SnapshotPolicy, SourceReadLimits) {
    let copy = CopyLimits::new(16 * 1024 * 1024, 512, 32 * 1024, 16).unwrap();
    let retained = RetainedBytesLimits::new(65536, 128, 32768).unwrap();
    (
        SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            copy,
            RetainedFileSet::new([], retained).unwrap(),
            retained,
        )
        .unwrap(),
        SourceReadLimits::new(65536, 65536, 16 * 1024 * 1024).unwrap(),
    )
}

#[test]
fn imported_physical_export_rejects_actual_content_and_manifest_changes() {
    native_test(async {
        for change in ["head", "manifest"] {
            let server = Server::new().await;
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let revision = f.provider(&server, &ctx);
            let issuer = f.issuer(&ctx);
            f.seed(&issuer, &ctx, &[page("a", &[])], None).await;
            web_read!(
                f, ctx, issuer, server, revision, verifier, flow, registered, raw, proof, read
            );
            reopen!(f, read, reader);
            let semantic = reader.capture_semantic(page_limits()).await.unwrap();
            let (policy, limits) = physical_policy();
            let physical = semantic
                .capture_physical_source(&policy, limits)
                .await
                .unwrap();
            assert_eq!(physical.generation(), &reader.observation().generation);
            if change == "head" {
                let id = identity();
                let inputs = collect_inputs(&semantic, &id).await;
                create_nn(&f, &id, vec![batch(&id, &[&inputs[0]], &[[1., 0.]])]).await;
            } else {
                let root = f.source.directory().as_path().join("index.lance/_versions");
                let manifest = std::fs::read_dir(root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.extension()
                            .is_some_and(|extension| extension == "manifest")
                    })
                    .unwrap();
                std::fs::write(manifest, b"changed manifest").unwrap();
            }
            assert!(
                physical.recheck().await.is_err(),
                "physical export rejects a changed file set or manifest ({change})"
            );
        }
    });
}
