//! Imported reads retain one registered documentary/vector SDK snapshot.
use super::*;
use graphit_data::{
    knowledge_embeddings::*,
    knowledge_index::KnowledgeDocument,
    knowledge_search::{self as search, KnowledgeSearchError},
};

#[derive(Debug)]
pub enum ImportedKnowledgeSemanticError {
    Reader(ImportedKnowledgeReadError),
    Embedding(KnowledgeEmbeddingError),
    Query(KnowledgeSearchError),
    Projection(crate::embedding_projection::KnowledgeEmbeddingProjectionError),
}
impl fmt::Display for ImportedKnowledgeSemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("imported Knowledge semantic operation failed")
    }
}
impl Error for ImportedKnowledgeSemanticError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reader(e) => Some(e),
            Self::Embedding(e) => Some(e),
            Self::Query(e) => Some(e),
            Self::Projection(e) => Some(e),
        }
    }
}
type Result<T> = std::result::Result<T, ImportedKnowledgeSemanticError>;
use ImportedKnowledgeSemanticError as E;

/// Current recipe observations only, never copied from the stored NN fingerprints.
pub struct ImportedKnowledgeEmbeddingInput {
    pub slug: String,
    pub input_fingerprint: [u8; 32],
}
impl ImportedKnowledgeEmbeddingInput {
    pub fn as_input(&self) -> KnowledgeEmbeddingInput<'_> {
        KnowledgeEmbeddingInput {
            slug: &self.slug,
            input_fingerprint: self.input_fingerprint,
        }
    }
}

pub struct CapturedImportedKnowledgeSemantic<'r, 'a> {
    reader: &'r CapturedImportedKnowledge<'a>,
    capture: KnowledgeEmbeddingSnapshotCapture,
}

fn external_failure(
    result: std::result::Result<(), ImportedKnowledgeReadError>,
    first: &mut Option<ImportedKnowledgeReadError>,
) -> std::result::Result<(), KnowledgeEmbeddingError> {
    result.map_err(|error| {
        if first.is_none() {
            *first = Some(error);
        }
        KnowledgeIndexError::InvalidField("external_guard").into()
    })
}
impl<'a> CapturedImportedKnowledge<'a> {
    /// Observe the optional vector extension on this reader's retained Source pin.
    pub async fn capture_semantic(
        &self,
        limits: KnowledgeIndexReadLimits,
    ) -> Result<CapturedImportedKnowledgeSemantic<'_, 'a>> {
        self.revalidate().map_err(E::Reader)?;
        let original = self.admission.request().snapshot();
        let check = || self.revalidate();
        let semantic = native_knowledge_embedding_dimensions(self.snapshot.schema())
            .map_err(E::Embedding)?
            .map(|_| self.pin.clone());
        let mut first_failure = None;
        let result = capture_knowledge_embedding_snapshots_guarded(
            original,
            self.snapshot.clone(),
            semantic,
            limits,
            || external_failure(check(), &mut first_failure),
        )
        .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        let capture = result.map_err(E::Embedding)?;
        check().map_err(E::Reader)?;
        Ok(CapturedImportedKnowledgeSemantic {
            reader: self,
            capture,
        })
    }
}

impl<'r, 'a> CapturedImportedKnowledgeSemantic<'r, 'a> {
    pub(super) fn imported_reader(&self) -> &'r CapturedImportedKnowledge<'a> {
        self.reader
    }
    pub(super) fn recheck_pins(&self) -> Result<()> {
        let mut first_failure = None;
        let result = self
            .capture
            .recheck_guarded(self.reader.admission.request().snapshot(), || {
                external_failure(self.recheck(), &mut first_failure)
            });
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        result.map_err(E::Embedding)
    }
    pub fn recheck(&self) -> std::result::Result<(), ImportedKnowledgeReadError> {
        self.reader.revalidate()
    }
    pub fn source_generation(&self) -> &KnowledgeGeneration {
        self.capture.source_generation()
    }
    pub fn semantic_generation(&self) -> Option<KnowledgeEmbeddingGeneration> {
        self.capture.semantic_generation()
    }
    /// Read selected bodies from this capture's retained documentary/vector pin.
    pub async fn read_selected_documents(
        &self,
        slugs: &[&str],
    ) -> Result<search::KnowledgeSelectedDocuments> {
        self.recheck_pins()?;
        let mut first_failure = None;
        let result = self
            .capture
            .read_selected_documents_guarded(
                self.reader.admission.request().snapshot(),
                slugs,
                || external_failure(self.recheck(), &mut first_failure),
            )
            .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        self.recheck_pins()?;
        result.map_err(E::Query)
    }

    /// Read documents from this capture without requiring a model identity.
    /// Some(empty) is a Removed batch; only None completes the actual Data cursor.
    pub async fn next_documents(&self) -> Result<Option<Vec<KnowledgeDocument>>> {
        self.recheck().map_err(E::Reader)?;
        let mut first_failure = None;
        let result = self
            .capture
            .next_documents_guarded(self.reader.admission.request().snapshot(), || {
                external_failure(self.recheck(), &mut first_failure)
            })
            .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        result.map_err(E::Embedding)
    }
    /// Stream recipe inputs, discarding each document's body and temporary text.
    /// This consumes the same cursor as next_documents.
    pub async fn next_inputs(
        &self,
        identity: &KnowledgeEmbeddingIdentity,
    ) -> Result<Option<Vec<ImportedKnowledgeEmbeddingInput>>> {
        use crate::embedding_projection as projection;
        self.recheck().map_err(E::Reader)?;
        if identity.projection_recipe() != projection::KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE {
            return Err(E::Projection(
                projection::KnowledgeEmbeddingProjectionError::InvalidRecipe,
            ));
        }
        let Some(documents) = self.next_documents().await? else {
            return Ok(None);
        };
        let mut inputs = Vec::with_capacity(documents.len());
        for document in documents {
            self.recheck().map_err(E::Reader)?;
            let text =
                projection::document_text(self.reader.admission.request().snapshot(), &document)
                    .map_err(E::Projection)?;
            let input_fingerprint =
                projection::fingerprint(&text, identity).map_err(E::Projection)?;
            inputs.push(ImportedKnowledgeEmbeddingInput {
                slug: document.slug,
                input_fingerprint,
            });
        }
        self.recheck().map_err(E::Reader)?;
        Ok(Some(inputs))
    }
    pub fn search_readiness(
        &self,
        identity: &KnowledgeEmbeddingIdentity,
        current_inputs: &[KnowledgeEmbeddingInput<'_>],
    ) -> Result<KnowledgeEmbeddingSearchReadiness> {
        let mut first_failure = None;
        let result = self.capture.search_readiness_guarded(
            self.reader.admission.request().snapshot(),
            identity,
            current_inputs,
            || external_failure(self.recheck(), &mut first_failure),
        );
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        result.map_err(E::Embedding)
    }
    fn binding(
        &self,
        caller: &str,
        limits: search::KnowledgeSearchLimits,
        preview: crate::preview::MarkdownPreviewLimits,
    ) -> Result<String> {
        if caller.len() > limits.max_query_bytes.get() {
            return Err(E::Query(KnowledgeSearchError::InvalidQuery("query_bytes")));
        }
        let selection = crate::imported_reference_query::selection_binding(
            self.reader.admission.request().snapshot(),
            &self.reader.installation,
            self.reader.input,
        )
        .map_err(|e| {
            E::Query(KnowledgeSearchError::Data(KnowledgeIndexError::Request(
                io::Error::other(e),
            )))
        })?;
        let selection = serde_json::to_string(&selection).map_err(|e| {
            E::Query(KnowledgeSearchError::Data(
                KnowledgeIndexError::ReferenceSerialization(e),
            ))
        })?;
        Ok(crate::search::preview_binding(
            &(format!("{}:{}", selection.len(), selection) + caller),
            preview,
        ))
    }
    pub async fn search(
        &self,
        identity: &KnowledgeEmbeddingIdentity,
        current_inputs: &[KnowledgeEmbeddingInput<'_>],
        query: search::KnowledgeVectorSearchQuery<'_>,
        limits: search::KnowledgeSearchLimits,
        preview_limits: crate::preview::MarkdownPreviewLimits,
    ) -> Result<search::KnowledgeVectorSearchPage> {
        self.recheck().map_err(E::Reader)?;
        let binding = self.binding(query.page.caller_binding, limits, preview_limits)?;
        let query = search::KnowledgeVectorSearchQuery {
            page: search::KnowledgeLexicalQuery {
                caller_binding: &binding,
                ..query.page
            },
            ..query
        };
        let mut first_failure = None;
        let result = query_knowledge_vector_snapshots_guarded(
            self.reader.admission.request().snapshot(),
            &self.capture,
            identity,
            current_inputs,
            query,
            limits,
            || external_failure(self.recheck(), &mut first_failure),
            |body, summary, remaining| {
                crate::search::render_preview(
                    self.reader.admission.request().snapshot(),
                    body,
                    summary,
                    preview_limits,
                    remaining,
                )
            },
        )
        .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        result.map_err(E::Query)
    }
    pub async fn source_search(
        &self,
        query: search::KnowledgeLexicalQuery<'_>,
        limits: search::KnowledgeSearchLimits,
        preview_limits: crate::preview::MarkdownPreviewLimits,
    ) -> Result<search::KnowledgeSearchPage> {
        self.recheck().map_err(E::Reader)?;
        let binding = self.binding(query.caller_binding, limits, preview_limits)?;
        let query = search::KnowledgeLexicalQuery {
            caller_binding: &binding,
            ..query
        };
        let mut first_failure = None;
        let result = query_knowledge_embedding_snapshot_source_search_guarded(
            self.reader.admission.request().snapshot(),
            &self.capture,
            query,
            limits,
            || external_failure(self.recheck(), &mut first_failure),
            |body, summary, remaining| {
                crate::search::render_preview(
                    self.reader.admission.request().snapshot(),
                    body,
                    summary,
                    preview_limits,
                    remaining,
                )
            },
        )
        .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        result.map_err(E::Query)
    }
}
