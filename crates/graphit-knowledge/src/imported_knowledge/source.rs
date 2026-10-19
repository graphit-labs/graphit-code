//! Source-only document cursors and guarded physical export of Imported Knowledge.
use super::*;
use graphit_core::artifact_store::{CapturedSourceReader, SnapshotPolicy, SourceReadLimits};
use graphit_data::{
    knowledge_embeddings::{
        KnowledgeEmbeddingError, KnowledgeEmbeddingSnapshotCapture,
        capture_knowledge_embedding_snapshots_guarded,
    },
    knowledge_index::KnowledgeDocument,
};
use std::sync::Arc;

#[derive(Debug)]
pub enum ImportedKnowledgeSourceError {
    Reader(ImportedKnowledgeReadError),
    Embedding(KnowledgeEmbeddingError),
    Query(graphit_data::knowledge_search::KnowledgeSearchError),
}
impl fmt::Display for ImportedKnowledgeSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("imported Knowledge source operation failed")
    }
}
impl Error for ImportedKnowledgeSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reader(error) => Some(error),
            Self::Embedding(error) => Some(error),
            Self::Query(error) => Some(error),
        }
    }
}
use ImportedKnowledgeSourceError as E;
type Result<T> = std::result::Result<T, E>;

pub struct CapturedImportedKnowledgeSource<'r, 'a> {
    reader: &'r CapturedImportedKnowledge<'a>,
    capture: KnowledgeEmbeddingSnapshotCapture,
}
fn guard(
    reader: &CapturedImportedKnowledge<'_>,
    first_failure: &mut Option<ImportedKnowledgeReadError>,
) -> std::result::Result<(), KnowledgeEmbeddingError> {
    if let Err(error) = reader.revalidate() {
        if first_failure.is_none() {
            *first_failure = Some(error);
        }
        return Err(KnowledgeEmbeddingError::InvalidInput("external_guard"));
    }
    Ok(())
}
impl<'a> CapturedImportedKnowledge<'a> {
    /// Validate a destination outside this readonly Imported payload before staging.
    pub fn validate_export_destination(
        &self,
        destination: &Path,
    ) -> std::result::Result<(), ImportedKnowledgeReadError> {
        self.revalidate()?;
        graphit_data::knowledge_search::validate_knowledge_export_destination_path(
            self.root.canonical_path(),
            destination,
        )
        .map_err(ImportedKnowledgeReadError::Capture)?;
        self.revalidate()
    }
    /// Retain this exact Source pin without opening NN or decoding Pair metadata.
    pub async fn capture_source(
        &self,
        limits: KnowledgeIndexReadLimits,
    ) -> Result<CapturedImportedKnowledgeSource<'_, 'a>> {
        self.revalidate().map_err(E::Reader)?;
        let mut first_failure = None;
        let result = capture_knowledge_embedding_snapshots_guarded(
            self.admission.request().snapshot(),
            self.snapshot.clone(),
            None,
            limits,
            || guard(self, &mut first_failure),
        )
        .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        let capture = result.map_err(E::Embedding)?;
        self.revalidate().map_err(E::Reader)?;
        Ok(CapturedImportedKnowledgeSource {
            reader: self,
            capture,
        })
    }
}
impl CapturedImportedKnowledgeSource<'_, '_> {
    pub fn source_generation(&self) -> &KnowledgeGeneration {
        self.capture.source_generation()
    }
    pub fn recheck(&self) -> Result<()> {
        let mut first_failure = None;
        let result = self
            .capture
            .recheck_guarded(self.reader.admission.request().snapshot(), || {
                guard(self.reader, &mut first_failure)
            });
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        result.map_err(E::Embedding)
    }
    /// Read selected bodies from this exact retained Source pin.
    pub async fn read_selected_documents(
        &self,
        slugs: &[&str],
    ) -> Result<graphit_data::knowledge_search::KnowledgeSelectedDocuments> {
        self.recheck()?;
        let mut first_failure = None;
        let result = self
            .capture
            .read_selected_documents_guarded(
                self.reader.admission.request().snapshot(),
                slugs,
                || guard(self.reader, &mut first_failure),
            )
            .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        self.recheck()?;
        result.map_err(E::Query)
    }

    /// Some(empty) is a Removed batch; only None completes this cursor.
    /// An abandoned or failed read cannot be resumed as a healthy EOF.
    pub async fn next_documents(&self) -> Result<Option<Vec<KnowledgeDocument>>> {
        self.recheck()?;
        let mut first_failure = None;
        let result = self
            .capture
            .next_documents_guarded(self.reader.admission.request().snapshot(), || {
                guard(self.reader, &mut first_failure)
            })
            .await;
        if let Some(error) = first_failure {
            return Err(E::Reader(error));
        }
        result.map_err(E::Embedding)
    }
}

/// Keeps physical content, the single pin and real Imported authority alive.
pub struct ImportedKnowledgePhysicalSource<'s, 'r, 'a> {
    semantic: &'s CapturedImportedKnowledgeSemantic<'r, 'a>,
    source: CapturedSourceReader,
}
impl<'r, 'a> CapturedImportedKnowledgeSemantic<'r, 'a> {
    /// Capture physical content under the retained SDK snapshot and Imported authority.
    pub async fn capture_physical_source(
        &self,
        policy: &SnapshotPolicy,
        limits: SourceReadLimits,
    ) -> std::result::Result<
        ImportedKnowledgePhysicalSource<'_, 'r, 'a>,
        ImportedKnowledgeSemanticError,
    > {
        self.recheck_pins()?;
        let reader = self.imported_reader();
        let source = CapturedSourceReader::capture(
            reader.root.canonical_path(),
            graphit_fs::NodeKind::Directory,
            policy,
            limits,
            Arc::clone(reader.admission.request().snapshot()),
        )
        .map_err(ImportedKnowledgeReadError::Capture)
        .map_err(ImportedKnowledgeSemanticError::Reader)?;
        let captured = ImportedKnowledgePhysicalSource {
            semantic: self,
            source,
        };
        captured.recheck().await?;
        Ok(captured)
    }
}
impl ImportedKnowledgePhysicalSource<'_, '_, '_> {
    /// Revalidate before and after the caller's joined transport operation.
    pub async fn recheck(&self) -> std::result::Result<(), ImportedKnowledgeSemanticError> {
        self.semantic.recheck_pins()?;
        self.source
            .revalidate_content()
            .map_err(ImportedKnowledgeReadError::Capture)
            .map_err(ImportedKnowledgeSemanticError::Reader)?;
        self.semantic.recheck_pins()
    }
    pub fn reader(
        &self,
    ) -> std::result::Result<&CapturedSourceReader, ImportedKnowledgeSemanticError> {
        self.semantic
            .recheck()
            .map_err(ImportedKnowledgeSemanticError::Reader)?;
        self.source
            .revalidate_bindings()
            .map_err(ImportedKnowledgeReadError::Capture)
            .map_err(ImportedKnowledgeSemanticError::Reader)?;
        Ok(&self.source)
    }
    pub fn generation(&self) -> &graphit_data::knowledge_index::KnowledgeGeneration {
        self.semantic.source_generation()
    }
}
