//! Model queries over one exact remote documentary/vector pin.
use super::Opened;
use crate::{
    foreground_knowledge::search::{SearchOutcome, WithoutModelPage},
    foreground_memory_jobs::MemoryJobResources,
    knowledge_semantic::{KnowledgeSemanticError, KnowledgeSemanticSearchOptions},
    remote_native_mount::ErrorKind,
};
use graphit_auth::{ImportedKnowledgeReadAdmission, ServiceCredentialLease};
use graphit_daemon::model_service::EmbedInput;
use graphit_data::{
    knowledge_embeddings::{
        KnowledgeEmbeddingError, KnowledgeEmbeddingIdentity, KnowledgeEmbeddingInput,
        KnowledgeEmbeddingSearchReadiness, capture_native_knowledge_embedding_snapshots_guarded,
        query_knowledge_embedding_snapshot_source_search_guarded,
        query_knowledge_vector_snapshots_guarded,
    },
    knowledge_search::{
        KnowledgeSearchError, KnowledgeVectorSearchMode, KnowledgeVectorSearchQuery,
    },
};
use graphit_knowledge::{embedding_projection as projection, search::render_preview};
use std::sync::Arc;

fn embedding(error: KnowledgeEmbeddingError) -> ErrorKind {
    ErrorKind::Embedding(Box::new(error))
}
fn semantic(error: impl Into<KnowledgeSemanticError>) -> ErrorKind {
    ErrorKind::Semantic(Box::new(error.into()))
}

pub(crate) async fn query(
    opened: &Opened<'_>,
    lease: &ServiceCredentialLease<'_>,
    admission: &ImportedKnowledgeReadAdmission<'_>,
    resources: Option<&MemoryJobResources>,
    rerank: &crate::foreground_knowledge::imported_search::ImportedRerankContext<'_>,
    input: &crate::foreground_knowledge::search::Input,
    options: KnowledgeSemanticSearchOptions<'_>,
) -> Result<SearchOutcome, ErrorKind> {
    opened.check(lease)?;
    admission.recheck().map_err(ErrorKind::Auth)?;
    let original = admission.request().snapshot();
    if !Arc::ptr_eq(original, opened.selection.request().snapshot()) {
        return Err(ErrorKind::InvalidBinding);
    }
    let source = opened.mount.wiki(&opened.selection, lease).await?;
    let vector_observation =
        graphit_data::knowledge_embeddings::native_knowledge_embedding_dimensions(source.schema())
            .map_err(embedding)?
            .map(
                |_| graphit_data::knowledge_embeddings::KnowledgeEmbeddingGeneration {
                    version: source.version(),
                    physical_branch: None,
                    inherited_target: None,
                },
            );
    let nn = if resources.is_some() {
        opened.mount.semantic(&opened.selection, lease).await?
    } else {
        None
    };
    let guard = || {
        opened
            .data_check(lease)
            .map_err(KnowledgeEmbeddingError::from)
    };
    let capture = capture_native_knowledge_embedding_snapshots_guarded(
        original,
        source,
        nn,
        options.limits.read,
        guard,
    )
    .await
    .map_err(embedding)?;
    let binding =
        opened.search_binding(options.page.caller_binding, options.limits, options.preview)?;
    let page_query = graphit_data::knowledge_search::KnowledgeLexicalQuery {
        caller_binding: &binding,
        ..options.page
    };
    let Some(resources) = resources else {
        if options.mode == KnowledgeVectorSearchMode::Semantic && options.page.cursor.is_some() {
            return Err(ErrorKind::Search(KnowledgeSearchError::InvalidCursor));
        }
        let mut empty = true;
        while let Some(documents) = capture
            .next_documents_guarded(original, guard)
            .await
            .map_err(embedding)?
        {
            empty &= documents.is_empty();
        }
        let lexical = if options.mode == KnowledgeVectorSearchMode::Hybrid {
            Some(
                query_knowledge_embedding_snapshot_source_search_guarded(
                    original,
                    &capture,
                    page_query,
                    options.limits,
                    guard,
                    |body, summary, remaining| {
                        render_preview(original, body, summary, options.preview, remaining)
                    },
                )
                .await
                .map_err(ErrorKind::Search)?,
            )
        } else {
            None
        };
        opened.check(lease)?;
        let outcome = SearchOutcome::WithoutModel(Some(WithoutModelPage {
            generation: capture.source_generation().clone(),
            semantic_generation: vector_observation,
            lexical,
            empty,
        }));
        let result = rerank
            .snapshot(admission, &capture, input, outcome, guard)
            .await;
        opened.check(lease)?;
        return result.map_err(semantic);
    };
    let mut service = resources
        .auth
        .issuer()
        .capture_imported_knowledge_embedding_service(admission)
        .map_err(semantic)?;
    if !Arc::ptr_eq(original, service.original()?.snapshot()) {
        return Err(ErrorKind::InvalidBinding);
    }
    let metadata = crate::embedding_service::metadata(&mut service, &resources.execution()).await;
    capture
        .recheck_guarded(original, guard)
        .map_err(embedding)?;
    let metadata = metadata.map_err(semantic)?;
    let dimensions = u32::try_from(metadata.dimensions)
        .ok()
        .and_then(std::num::NonZeroU32::new)
        .ok_or_else(|| semantic(KnowledgeSemanticError::MissingEmbeddingDimensions))?;
    let identity = KnowledgeEmbeddingIdentity::new(
        metadata.vector_id,
        dimensions,
        projection::KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE.to_owned(),
    )
    .map_err(embedding)?;
    let mut observed = Vec::new();
    while let Some(documents) = capture
        .next_documents_guarded(original, guard)
        .await
        .map_err(embedding)?
    {
        for document in documents {
            opened.check(lease)?;
            let text = projection::document_text(original, &document).map_err(semantic)?;
            let fingerprint = projection::fingerprint(&text, &identity).map_err(semantic)?;
            observed.push((document.slug, fingerprint));
        }
    }
    let inputs: Vec<_> = observed
        .iter()
        .map(|(slug, fingerprint)| KnowledgeEmbeddingInput {
            slug,
            input_fingerprint: *fingerprint,
        })
        .collect();
    let readiness = capture
        .search_readiness_guarded(original, &identity, &inputs, guard)
        .map_err(embedding)?;
    let output = if readiness == KnowledgeEmbeddingSearchReadiness::Ready {
        opened.check(lease)?;
        let result = resources
            .embed(
                &mut service,
                &metadata,
                graphit_daemon::model_service::EmbedOperation {
                    input: EmbedInput::Query,
                    texts: vec![options.page.text.to_owned()],
                    deadline: original.required_deadline().map_err(semantic)?,
                },
            )
            .await;
        capture
            .recheck_guarded(original, guard)
            .map_err(embedding)?;
        opened.check(lease)?;
        Some(result.map_err(semantic)?)
    } else {
        None
    };
    let page = query_knowledge_vector_snapshots_guarded(
        original,
        &capture,
        &identity,
        &inputs,
        KnowledgeVectorSearchQuery {
            page: page_query,
            mode: options.mode,
            metric: options.metric,
            vector: output.as_ref().map(|output| output.vectors[0].as_slice()),
        },
        options.limits,
        guard,
        |body, summary, remaining| {
            render_preview(original, body, summary, options.preview, remaining)
        },
    )
    .await
    .map_err(ErrorKind::Search)?;
    opened.check(lease)?;
    let result = rerank
        .snapshot(
            admission,
            &capture,
            input,
            SearchOutcome::Vector(Some(page)),
            guard,
        )
        .await;
    opened.check(lease)?;
    result.map_err(semantic)
}
