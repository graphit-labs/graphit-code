//! Browser catalogue from one readonly Imported Source capture, without NN or Model.
use super::*;
use graphit_data::knowledge_index::KnowledgeHead;
use graphit_knowledge::imported_knowledge::ImportedKnowledgeSourceError;

#[derive(Serialize)]
pub(super) struct Response<'a> {
    pub(super) source: &'a SourceKey,
    pub(super) native: Read<'a>,
}
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(super) enum Read<'a> {
    RegistryAbsent,
    Published {
        generation: foreground_knowledge::Generation<'a>,
        heads: foreground_knowledge::Heads<'a>,
    },
}

pub(super) async fn read(
    opened: &ImportedKnowledgeOpen<'_>,
    source: &SourceKey,
    resources: &Resources<'_>,
    original: &RequestContext<'_>,
) -> Result<Vec<u8>, Failure> {
    resources.check(original)?;
    match opened {
        ImportedKnowledgeOpen::RegistryAbsent => encode(
            &Response {
                source,
                native: Read::RegistryAbsent,
            },
            original.snapshot(),
            resources.stop,
            resources.policy.content.max_output_bytes,
        ),
        ImportedKnowledgeOpen::Published(reader) => {
            let capture = reader
                .capture_source(foreground_knowledge::limits(resources.policy)?)
                .await?;
            let mut heads = Vec::new();
            while let Some(documents) = capture.next_documents().await? {
                resources.check(original)?;
                heads.extend(documents.into_iter().map(|document| KnowledgeHead {
                    slug: document.slug,
                    title: document.title,
                    updated_at: document.updated_at,
                }));
            }
            heads.sort_unstable_by(|left, right| left.slug.cmp(&right.slug));
            capture.recheck()?;
            let bytes = encode(
                &Response {
                    source,
                    native: Read::Published {
                        generation: capture.source_generation().into(),
                        heads: foreground_knowledge::Heads(&heads),
                    },
                },
                original.snapshot(),
                resources.stop,
                resources.policy.content.max_output_bytes,
            )?;
            capture.recheck()?;
            Ok(bytes)
        }
    }
}

pub(super) fn response(
    error: &ImportedKnowledgeSourceError,
) -> (axum::http::StatusCode, &'static str) {
    use graphit_data::knowledge_embeddings::KnowledgeEmbeddingError as E;
    match error {
        ImportedKnowledgeSourceError::Reader(error) => Failure::imported_response(error),
        ImportedKnowledgeSourceError::Query(error) => foreground_knowledge::search::response(error),
        ImportedKnowledgeSourceError::Embedding(E::Index(error)) => {
            Failure::knowledge_response(error)
        }
        ImportedKnowledgeSourceError::Embedding(E::Io(error)) => Failure::io_response(error),
        ImportedKnowledgeSourceError::Embedding(E::Conflict) => (
            axum::http::StatusCode::CONFLICT,
            "knowledge_generation_changed",
        ),
        _ => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "knowledge_capture_failure",
        ),
    }
}
