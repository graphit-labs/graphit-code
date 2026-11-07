//! Canonical Document inputs derived from one captured Knowledge corpus.
//!
//! These observations do not authorize Models, reads, writes or publication.
//! The caller keeps the original admissions and consumes the same Data capture.

use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_embeddings::{
    KnowledgeEmbeddingCapture, KnowledgeEmbeddingIdentity, KnowledgeEmbeddingInput,
};
use graphit_data::knowledge_index::KnowledgeDocument;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::TryReserveError,
    error::Error,
    fmt,
    io::{self, Write},
    num::NonZeroU32,
};

pub const KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE: &str = "knowledge-document-json-v1";

#[derive(Debug)]
pub enum KnowledgeEmbeddingProjectionError {
    Request(io::Error),
    Serialization(serde_json::Error),
    Document(crate::document::DocumentParseError),
    InvalidDimensions,
    InvalidRecipe,
    ConstructionOverflow,
    Allocation(TryReserveError),
}
impl fmt::Display for KnowledgeEmbeddingProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions => f.write_str("invalid Knowledge embedding dimensions"),
            Self::InvalidRecipe => f.write_str("invalid Knowledge document embedding recipe"),
            Self::ConstructionOverflow => f.write_str("Knowledge embedding input length overflow"),
            _ => f.write_str("Knowledge embedding projection failed"),
        }
    }
}
impl Error for KnowledgeEmbeddingProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::Document(error) => Some(error),
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for KnowledgeEmbeddingProjectionError {
    fn from(error: io::Error) -> Self {
        Self::Request(error)
    }
}
impl From<serde_json::Error> for KnowledgeEmbeddingProjectionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}
impl From<TryReserveError> for KnowledgeEmbeddingProjectionError {
    fn from(error: TryReserveError) -> Self {
        Self::Allocation(error)
    }
}

pub struct ProjectedKnowledgeDocument<'a> {
    slug: &'a str,
    text: String,
    input_fingerprint: [u8; 32],
    pending: bool,
}
impl ProjectedKnowledgeDocument<'_> {
    pub fn slug(&self) -> &str {
        self.slug
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn input_fingerprint(&self) -> [u8; 32] {
        self.input_fingerprint
    }
    pub fn is_pending(&self) -> bool {
        self.pending
    }
    /// Consume the projected input so the caller can move it into Models.
    pub fn into_text(self) -> String {
        self.text
    }
}
impl fmt::Debug for ProjectedKnowledgeDocument<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProjectedKnowledgeDocument")
            .field("text_bytes", &self.text.len())
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

pub struct KnowledgeEmbeddingProjection<'a> {
    capture: &'a KnowledgeEmbeddingCapture,
    identity: KnowledgeEmbeddingIdentity,
    documents: Vec<ProjectedKnowledgeDocument<'a>>,
}
impl<'a> KnowledgeEmbeddingProjection<'a> {
    pub fn capture(&self) -> &'a KnowledgeEmbeddingCapture {
        self.capture
    }
    pub fn identity(&self) -> &KnowledgeEmbeddingIdentity {
        &self.identity
    }
    pub fn documents(&self) -> &[ProjectedKnowledgeDocument<'a>] {
        &self.documents
    }
    pub fn into_documents(self) -> Vec<ProjectedKnowledgeDocument<'a>> {
        self.documents
    }
    pub fn pending_documents(&self) -> impl Iterator<Item = &ProjectedKnowledgeDocument<'a>> {
        self.documents.iter().filter(|document| document.pending)
    }
    pub fn current_inputs(&self) -> impl Iterator<Item = KnowledgeEmbeddingInput<'_>> {
        self.documents
            .iter()
            .map(|document| KnowledgeEmbeddingInput {
                slug: document.slug,
                input_fingerprint: document.input_fingerprint,
            })
    }
}
impl fmt::Debug for KnowledgeEmbeddingProjection<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KnowledgeEmbeddingProjection")
            .field("dimensions", &self.identity.dimensions())
            .field("documents", &self.documents.len())
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct DocumentText<'a> {
    recipe: &'static str,
    title: &'a str,
    kind: &'a str,
    description: Option<&'a str>,
    summary: &'a str,
}
/// Build the maintained document recipe from one borrowed SDK document.
pub fn document_text(
    original: &SnapshotContext,
    document: &KnowledgeDocument,
) -> Result<String, KnowledgeEmbeddingProjectionError> {
    let body = crate::document::without_document_navigation(original, &document.body)
        .map_err(KnowledgeEmbeddingProjectionError::Document)?;
    let mut text = serde_json::to_string(&DocumentText {
        recipe: KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE,
        title: &document.title,
        kind: &document.kind,
        description: document.description.as_deref(),
        summary: &document.summary,
    })?;
    let additional = body
        .len()
        .checked_add(1)
        .ok_or(KnowledgeEmbeddingProjectionError::ConstructionOverflow)?;
    text.try_reserve_exact(additional)?;
    text.push('\n');
    text.push_str(&body);
    Ok(text)
}

/// The Data replacement passes its actual observed old/new bodies under CAS.
/// Only navigation entries are excluded; this never calls Models or a service.
pub fn equivalent_body(
    original: &SnapshotContext,
    old: &str,
    new: &str,
) -> Result<bool, graphit_data::knowledge_index::KnowledgeIndexError> {
    original.check()?;
    if old == new {
        return Ok(true);
    }
    let old = crate::document::without_document_navigation(original, old)
        .map_err(graphit_data::knowledge_index::KnowledgeIndexError::from)?;
    let new = crate::document::without_document_navigation(original, new)
        .map_err(graphit_data::knowledge_index::KnowledgeIndexError::from)?;
    original.check()?;
    Ok(old == new)
}

#[derive(Serialize)]
struct Fingerprint<'a> {
    recipe: &'static str,
    role: &'static str,
    vector_id: [u8; 32],
    dimensions: u32,
    text: &'a str,
}
struct DigestWriter<'a>(&'a mut Sha256);
impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Fingerprint recipe text against the actual selected embedding identity.
pub fn fingerprint(
    text: &str,
    identity: &KnowledgeEmbeddingIdentity,
) -> Result<[u8; 32], KnowledgeEmbeddingProjectionError> {
    let mut hash = Sha256::new();
    serde_json::to_writer(
        DigestWriter(&mut hash),
        &Fingerprint {
            recipe: KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE,
            role: "Document",
            vector_id: identity.vector_id(),
            dimensions: identity.dimensions().get(),
            text,
        },
    )?;
    Ok(hash.finalize().into())
}

/// Project one supplied document batch, keeping the producer capture borrowed.
///
/// The document DTOs are ordinary caller input. The borrow and its lifetime do
/// not prove that those DTOs came from the pin or passed Data's read limits.
/// This pure helper grants no authority and checks no document provenance.
/// The productive consumer supplies batches from Data's authorized private
/// cursor. Context checks do not replace Data's admission, same-Arc, pin, EOF
/// or publication checks. Dimensions and portable vector identity must come from Models.
pub fn prepare_knowledge_embedding_projection<'a>(
    original: &SnapshotContext,
    capture: &'a KnowledgeEmbeddingCapture,
    documents: &'a [KnowledgeDocument],
    vector_id: [u8; 32],
    dimensions: u64,
) -> Result<KnowledgeEmbeddingProjection<'a>, KnowledgeEmbeddingProjectionError> {
    original.check()?;
    let dimensions = u32::try_from(dimensions)
        .ok()
        .filter(|value| i32::try_from(*value).is_ok())
        .and_then(NonZeroU32::new)
        .ok_or(KnowledgeEmbeddingProjectionError::InvalidDimensions)?;
    let identity = KnowledgeEmbeddingIdentity::new(
        vector_id,
        dimensions,
        KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE.to_owned(),
    )
    .map_err(|_| KnowledgeEmbeddingProjectionError::InvalidDimensions)?;
    let mut projected = Vec::new();
    projected.try_reserve_exact(documents.len())?;
    for document in documents {
        projected.push(prepare_knowledge_embedding_document(
            original, capture, document, &identity,
        )?);
    }
    original.check()?;
    Ok(KnowledgeEmbeddingProjection {
        capture,
        identity,
        documents: projected,
    })
}

/// Project a single ordinary document with the established Document recipe.
/// This checks request liveness, not provenance; the productive caller reads the
/// same Data capture and preserves its admissions, pin and complete cursor.
pub fn prepare_knowledge_embedding_document<'a>(
    original: &SnapshotContext,
    capture: &KnowledgeEmbeddingCapture,
    document: &'a KnowledgeDocument,
    identity: &KnowledgeEmbeddingIdentity,
) -> Result<ProjectedKnowledgeDocument<'a>, KnowledgeEmbeddingProjectionError> {
    original.check()?;
    if identity.projection_recipe() != KNOWLEDGE_DOCUMENT_EMBEDDING_RECIPE {
        return Err(KnowledgeEmbeddingProjectionError::InvalidRecipe);
    }
    let text = document_text(original, document)?;
    let input_fingerprint = fingerprint(&text, identity)?;
    let pending = !capture.stored_inputs().any(|(stored, stored_identity)| {
        stored.slug == document.slug
            && stored.input_fingerprint == input_fingerprint
            && stored_identity == identity
    });
    original.check()?;
    Ok(ProjectedKnowledgeDocument {
        slug: &document.slug,
        text,
        input_fingerprint,
        pending,
    })
}
