//! Borrowed authored-document data with input validation and ordinary cancellation.
//!
//! Text and target groups borrow their inputs; the request outlives the projection.
//! Digest values and explicit removals are observations, not provenance proofs.
use graphit_core::file_io::SnapshotContext;
use serde::Serialize;
use std::{
    error::Error,
    fmt, io,
    num::NonZeroUsize,
    time::{Duration, SystemTime},
};

pub const AUTHORED_RECIPE_ID: &str = "authored-document-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalSourceDigest32([u8; 32]);
impl PhysicalSourceDigest32 {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A supplied compilation value; construction does not compute or verify a hash.
/// Physical and compilation digests cannot be interchanged.
/// ```compile_fail
/// use graphit_data::knowledge_document_projection::{CompilationDigest32, PhysicalSourceDigest32};
/// let physical = PhysicalSourceDigest32::from_bytes([1; 32]);
/// let _: CompilationDigest32 = physical.into();
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CompilationDigest32([u8; 32]);
impl CompilationDigest32 {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy)]
pub struct AuthoredParameters<'data> {
    pub fallback_title: &'data str,
    pub kind_on_absent: &'data str,
    pub max_summary_bytes: NonZeroUsize,
}

#[derive(Clone, Copy)]
pub struct RecipeFields<'data> {
    pub id: &'data str,
    pub parameters: Option<AuthoredParameters<'data>>,
}

#[derive(Clone, Copy)]
pub struct AuthoredPageFields<'data> {
    pub slug: Option<&'data str>,
    pub title: Option<&'data str>,
    pub body: Option<&'data str>,
    pub kind: Option<&'data str>,
    pub summary: Option<&'data str>,
    pub description: Option<&'data str>,
    pub summary_truncated: bool,
    pub source_path: &'data str,
    pub physical_digest: Option<PhysicalSourceDigest32>,
    pub compilation_digest: CompilationDigest32,
    pub recipe: RecipeFields<'data>,
    pub updated_at: Option<SystemTime>,
    pub stale_since: Option<SystemTime>,
    pub stale_after: Option<Duration>,
    pub resolved_wiki_targets: &'data [&'data str],
}

#[derive(Clone, Copy)]
pub enum CompilationObservationState<'data> {
    Present {
        compilation_digest: CompilationDigest32,
        recipe: RecipeFields<'data>,
    },
    Removed,
}

#[derive(Clone, Copy)]
pub struct CompilationObservationFields<'data> {
    pub path: &'data str,
    pub state: CompilationObservationState<'data>,
}

#[derive(Clone, Copy)]
pub struct AuthoredRecipe<'data> {
    id: &'data str,
    parameters: AuthoredParameters<'data>,
}
impl<'data> AuthoredRecipe<'data> {
    pub const fn id(&self) -> &'data str {
        self.id
    }
    pub const fn parameters(&self) -> AuthoredParameters<'data> {
        self.parameters
    }
}

/// A stable target group from the retained raw input, not from a temporary Vec.
#[derive(Clone, Copy)]
pub struct ResolvedTargets<'data>(&'data [&'data str]);
impl<'data> ResolvedTargets<'data> {
    pub const fn as_slice(&self) -> &'data [&'data str] {
        self.0
    }
}

#[derive(Clone, Copy)]
pub struct DocumentHeader<'data> {
    pub document_index: usize,
    pub slug: &'data str,
    pub title: &'data str,
    pub body: &'data str,
    pub kind: &'data str,
    pub summary: &'data str,
    pub description: Option<&'data str>,
    pub summary_truncated: bool,
    pub source_path: &'data str,
    pub physical_digest: Option<PhysicalSourceDigest32>,
    pub compilation_digest: CompilationDigest32,
    pub recipe: AuthoredRecipe<'data>,
    pub updated_at: Option<SystemTime>,
    pub stale_since: Option<SystemTime>,
    pub stale_after: Option<Duration>,
    pub resolved_wiki_targets: ResolvedTargets<'data>,
}

#[derive(Clone)]
pub enum MemberRow<'data> {
    DocumentHeader(Box<DocumentHeader<'data>>),
    WikiTarget {
        document_index: usize,
        ordinal: usize,
        target: &'data str,
    },
    CompilationObservation(CompilationObservationFields<'data>),
}

#[derive(Clone, Copy, Debug)]
pub struct KnowledgeDocumentProjectionLimits {
    pub max_documents: NonZeroUsize,
    pub max_observations: NonZeroUsize,
    pub max_rows: NonZeroUsize,
    pub max_field_bytes: NonZeroUsize,
    pub max_text_bytes: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionLimit {
    Documents,
    Observations,
    Rows,
    FieldBytes,
    TextBytes,
}

#[derive(Debug)]
pub enum KnowledgeDocumentProjectionError {
    RequiredField {
        field: &'static str,
        document_index: usize,
    },
    InvalidRecipe,
    InvalidPolicy,
    InvalidPath,
    DifferentRequest,
    LimitExceeded {
        kind: ProjectionLimit,
    },
    Overflow {
        kind: ProjectionLimit,
    },
    Request(io::Error),
}
impl fmt::Display for KnowledgeDocumentProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequiredField {
                field,
                document_index,
            } => write!(formatter, "missing {field} in document {document_index}"),
            Self::InvalidRecipe => formatter.write_str("unsupported or incomplete authored recipe"),
            Self::InvalidPolicy => formatter.write_str("invalid authored age policy"),
            Self::InvalidPath => formatter.write_str("empty source or observation path"),
            Self::DifferentRequest => formatter.write_str("different projection request"),
            Self::LimitExceeded { kind } => {
                write!(formatter, "projection limit exceeded: {kind:?}")
            }
            Self::Overflow { kind } => {
                write!(formatter, "projection arithmetic overflow: {kind:?}")
            }
            Self::Request(_) => formatter.write_str("projection request interrupted or expired"),
        }
    }
}
impl Error for KnowledgeDocumentProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(error) => Some(error),
            _ => None,
        }
    }
}
impl From<io::Error> for KnowledgeDocumentProjectionError {
    fn from(error: io::Error) -> Self {
        Self::Request(error)
    }
}

/// Opaque row owner. It retains supplied inputs/context, not their historical origin.
/// A borrowed row slice cannot outlive its collection.
/// ```compile_fail
/// use graphit_core::file_io::SnapshotContext;
/// use graphit_data::knowledge_document_projection::*;
/// fn escape(context: &SnapshotContext, limits: KnowledgeDocumentProjectionLimits) {
///     let documents = [];
///     let observations = [];
///     let projection = prepare(context, &documents, &observations, limits).unwrap();
///     let rows = projection.rows(context).unwrap();
///     drop(projection);
///     let _ = rows.len();
/// }
/// ```
/// The supplied context cannot be dropped while the projection is used.
/// ```compile_fail
/// use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
/// use graphit_data::knowledge_document_projection::*;
/// use std::time::{Duration, Instant};
/// fn escape(limits: KnowledgeDocumentProjectionLimits) {
///     let documents = [];
///     let observations = [];
///     let context = SnapshotContext::new(Instant::now()+Duration::from_secs(10), Cancellation::default());
///     let projection = prepare(&context, &documents, &observations, limits).unwrap();
///     drop(context);
///     drop(projection);
/// }
/// ```
pub struct PreparedKnowledgeDocumentRows<'data, 'context> {
    context: &'context SnapshotContext,
    rows: Vec<MemberRow<'data>>,
}
impl<'data, 'context> PreparedKnowledgeDocumentRows<'data, 'context> {
    pub fn rows(
        &self,
        context: &SnapshotContext,
    ) -> Result<&[MemberRow<'data>], KnowledgeDocumentProjectionError> {
        self.check(context)?;
        Ok(&self.rows)
    }
    pub fn supplied_context(
        &self,
        context: &SnapshotContext,
    ) -> Result<&'context SnapshotContext, KnowledgeDocumentProjectionError> {
        self.check(context)?;
        Ok(self.context)
    }
    fn check(&self, context: &SnapshotContext) -> Result<(), KnowledgeDocumentProjectionError> {
        self.context.check()?;
        if !self.context.same_request(context) {
            return Err(KnowledgeDocumentProjectionError::DifferentRequest);
        }
        context.check()?;
        Ok(())
    }
}

/// Validate and prepare authored data without I/O, hashing or text copies.
pub fn prepare<'data, 'context>(
    context: &'context SnapshotContext,
    documents: &[AuthoredPageFields<'data>],
    observations: &[CompilationObservationFields<'data>],
    limits: KnowledgeDocumentProjectionLimits,
) -> Result<PreparedKnowledgeDocumentRows<'data, 'context>, KnowledgeDocumentProjectionError> {
    context.check()?;
    bounded(
        documents.len(),
        limits.max_documents,
        ProjectionLimit::Documents,
    )?;
    bounded(
        observations.len(),
        limits.max_observations,
        ProjectionLimit::Observations,
    )?;
    let mut row_count = sum(documents.len(), observations.len(), ProjectionLimit::Rows)?;
    for document in documents {
        context.check()?;
        row_count = sum(
            row_count,
            document.resolved_wiki_targets.len(),
            ProjectionLimit::Rows,
        )?;
    }
    bounded(row_count, limits.max_rows, ProjectionLimit::Rows)?;

    let mut rows = Vec::with_capacity(row_count);
    let mut text_bytes = 0;
    let mut field = |text: &str| -> Result<(), KnowledgeDocumentProjectionError> {
        context.check()?;
        bounded(
            text.len(),
            limits.max_field_bytes,
            ProjectionLimit::FieldBytes,
        )?;
        text_bytes = sum(text_bytes, text.len(), ProjectionLimit::TextBytes)?;
        bounded(
            text_bytes,
            limits.max_text_bytes,
            ProjectionLimit::TextBytes,
        )
    };
    for (document_index, document) in documents.iter().enumerate() {
        context.check()?;
        let header = header(*document, document_index)?;
        for text in [
            header.slug,
            header.title,
            header.body,
            header.kind,
            header.summary,
            header.source_path,
            header.recipe.id(),
            header.recipe.parameters().fallback_title,
            header.recipe.parameters().kind_on_absent,
        ] {
            field(text)?;
        }
        if let Some(description) = header.description {
            field(description)?;
        }
        rows.push(MemberRow::DocumentHeader(Box::new(header)));
        for (ordinal, &target) in document.resolved_wiki_targets.iter().enumerate() {
            field(target)?;
            rows.push(MemberRow::WikiTarget {
                document_index,
                ordinal,
                target,
            });
        }
    }
    for &observation in observations {
        context.check()?;
        if observation.path.is_empty() {
            return Err(KnowledgeDocumentProjectionError::InvalidPath);
        }
        field(observation.path)?;
        if let CompilationObservationState::Present { recipe: raw, .. } = observation.state {
            let recipe = recipe(raw)?;
            field(recipe.id())?;
            field(recipe.parameters().fallback_title)?;
            field(recipe.parameters().kind_on_absent)?;
        }
        rows.push(MemberRow::CompilationObservation(observation));
    }
    context.check()?;
    Ok(PreparedKnowledgeDocumentRows { context, rows })
}

fn required<'data>(
    value: Option<&'data str>,
    field: &'static str,
    document_index: usize,
) -> Result<&'data str, KnowledgeDocumentProjectionError> {
    value.ok_or(KnowledgeDocumentProjectionError::RequiredField {
        field,
        document_index,
    })
}
fn recipe(
    fields: RecipeFields<'_>,
) -> Result<AuthoredRecipe<'_>, KnowledgeDocumentProjectionError> {
    if fields.id != AUTHORED_RECIPE_ID {
        return Err(KnowledgeDocumentProjectionError::InvalidRecipe);
    }
    let parameters = fields
        .parameters
        .ok_or(KnowledgeDocumentProjectionError::InvalidRecipe)?;
    Ok(AuthoredRecipe {
        id: fields.id,
        parameters,
    })
}
fn header(
    document: AuthoredPageFields<'_>,
    document_index: usize,
) -> Result<DocumentHeader<'_>, KnowledgeDocumentProjectionError> {
    let slug = required(document.slug, "slug", document_index)?;
    if slug.is_empty() {
        return Err(KnowledgeDocumentProjectionError::RequiredField {
            field: "nonempty slug",
            document_index,
        });
    }
    if document.source_path.is_empty() {
        return Err(KnowledgeDocumentProjectionError::InvalidPath);
    }
    if document
        .stale_after
        .is_some_and(|duration| duration.is_zero())
        || (document.stale_after.is_some() && document.updated_at.is_none())
    {
        return Err(KnowledgeDocumentProjectionError::InvalidPolicy);
    }
    Ok(DocumentHeader {
        document_index,
        slug,
        title: required(document.title, "title", document_index)?,
        body: required(document.body, "body", document_index)?,
        kind: required(document.kind, "kind", document_index)?,
        summary: required(document.summary, "summary", document_index)?,
        description: document.description,
        summary_truncated: document.summary_truncated,
        source_path: document.source_path,
        physical_digest: document.physical_digest,
        compilation_digest: document.compilation_digest,
        recipe: recipe(document.recipe)?,
        updated_at: document.updated_at,
        stale_since: document.stale_since,
        stale_after: document.stale_after,
        resolved_wiki_targets: ResolvedTargets(document.resolved_wiki_targets),
    })
}
fn sum(
    a: usize,
    b: usize,
    kind: ProjectionLimit,
) -> Result<usize, KnowledgeDocumentProjectionError> {
    a.checked_add(b)
        .ok_or(KnowledgeDocumentProjectionError::Overflow { kind })
}
fn bounded(
    value: usize,
    limit: NonZeroUsize,
    kind: ProjectionLimit,
) -> Result<(), KnowledgeDocumentProjectionError> {
    if value > limit.get() {
        Err(KnowledgeDocumentProjectionError::LimitExceeded { kind })
    } else {
        Ok(())
    }
}
