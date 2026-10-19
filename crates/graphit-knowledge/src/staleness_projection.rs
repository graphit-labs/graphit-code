//! Compose authored-document rows with the reference and staleness kernels.
//!
//! The caller supplies complete, authorized published/current data and the clock.
//! This pure adapter neither establishes that provenance nor changes stored flags.

use crate::{
    references::{PageReferences, ReferenceError, ReferenceGraph, ReferenceLimits},
    staleness::{
        PublishedPage, SourceFingerprint, SourceObservation, SourceState, StalePage,
        StalenessError, StalenessLimits, assess,
    },
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_document_projection::{
    CompilationObservationState, KnowledgeDocumentProjectionError, MemberRow,
    PreparedKnowledgeDocumentRows,
};
use std::{error::Error, fmt, time::SystemTime};

#[derive(Clone, Copy, Debug)]
pub struct StalenessProjectionLimits {
    pub references: ReferenceLimits,
    pub staleness: StalenessLimits,
}

#[derive(Debug)]
pub enum StalenessProjectionError {
    Projection(KnowledgeDocumentProjectionError),
    References(ReferenceError),
    Staleness(StalenessError),
}

impl fmt::Display for StalenessProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Projection(error) => write!(f, "knowledge projection: {error}"),
            Self::References(error) => write!(f, "knowledge references: {error}"),
            Self::Staleness(error) => write!(f, "knowledge staleness: {error}"),
        }
    }
}

impl Error for StalenessProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Projection(error) => error,
            Self::References(error) => error,
            Self::Staleness(error) => error,
        })
    }
}

impl From<KnowledgeDocumentProjectionError> for StalenessProjectionError {
    fn from(error: KnowledgeDocumentProjectionError) -> Self {
        Self::Projection(error)
    }
}

impl From<ReferenceError> for StalenessProjectionError {
    fn from(error: ReferenceError) -> Self {
        Self::References(error)
    }
}

impl From<StalenessError> for StalenessProjectionError {
    fn from(error: StalenessError) -> Self {
        Self::Staleness(error)
    }
}

/// A request-bound assessment borrowing the original projection.
///
/// The assessment cannot outlive the supplied projection:
/// ```compile_fail
/// use graphit_core::file_io::SnapshotContext;
/// use graphit_data::knowledge_document_projection::{prepare, AuthoredPageFields,
///     CompilationObservationFields, KnowledgeDocumentProjectionLimits};
/// use graphit_knowledge::staleness_projection::{assess_projection,
///     PreparedKnowledgeAssessment, StalenessProjectionLimits};
/// use std::time::SystemTime;
/// fn invalid<'data, 'context>(context: &'context SnapshotContext,
///     documents: &[AuthoredPageFields<'data>],
///     observations: &[CompilationObservationFields<'data>],
///     input_limits: KnowledgeDocumentProjectionLimits,
///     limits: StalenessProjectionLimits,
/// ) -> PreparedKnowledgeAssessment<'context, 'data, 'context> {
///     let projection = prepare(context, documents, observations, input_limits).unwrap();
///     assess_projection(context, &projection, SystemTime::UNIX_EPOCH, limits).unwrap()
/// }
/// ```
pub struct PreparedKnowledgeAssessment<'projection, 'data, 'context> {
    projection: &'projection PreparedKnowledgeDocumentRows<'data, 'context>,
    graph: ReferenceGraph<'context, 'data>,
    stale_pages: Vec<StalePage<'data>>,
}

impl<'data, 'context> PreparedKnowledgeAssessment<'_, 'data, 'context> {
    pub fn graph(
        &self,
        context: &SnapshotContext,
    ) -> Result<&ReferenceGraph<'context, 'data>, StalenessProjectionError> {
        self.projection.supplied_context(context)?;
        Ok(&self.graph)
    }

    pub fn stale_pages(
        &self,
        context: &SnapshotContext,
    ) -> Result<&[StalePage<'data>], StalenessProjectionError> {
        self.projection.supplied_context(context)?;
        Ok(&self.stale_pages)
    }
}

/// Evaluate exactly the supplied corpus using compilation digests and an explicit clock.
///
/// Missing observations remain errors, never implicit removals or partial results.
pub fn assess_projection<'projection, 'data, 'context>(
    context: &SnapshotContext,
    projection: &'projection PreparedKnowledgeDocumentRows<'data, 'context>,
    evaluated_at: SystemTime,
    limits: StalenessProjectionLimits,
) -> Result<PreparedKnowledgeAssessment<'projection, 'data, 'context>, StalenessProjectionError> {
    let retained_context = projection.supplied_context(context)?;
    let mut references = Vec::new();
    let mut published = Vec::new();
    let mut observations = Vec::new();
    for row in projection.rows(context)? {
        retained_context
            .check()
            .map_err(KnowledgeDocumentProjectionError::from)?;
        match row {
            MemberRow::DocumentHeader(header) => {
                references.push(PageReferences {
                    slug: header.slug,
                    title: header.title,
                    targets: header.resolved_wiki_targets.as_slice(),
                });
                published.push(PublishedPage {
                    slug: header.slug,
                    source: Some(SourceFingerprint {
                        path: header.source_path,
                        digest: *header.compilation_digest.as_bytes(),
                    }),
                    updated_at: header.updated_at,
                    stale_after: header.stale_after,
                    stale_since: header.stale_since,
                });
            }
            MemberRow::CompilationObservation(observation) => {
                observations.push(SourceObservation {
                    path: observation.path,
                    state: match observation.state {
                        CompilationObservationState::Present {
                            compilation_digest, ..
                        } => SourceState::Present(*compilation_digest.as_bytes()),
                        CompilationObservationState::Removed => SourceState::Removed,
                    },
                });
            }
            // The immutable header already retains this exact raw target group.
            MemberRow::WikiTarget { .. } => {}
        }
    }
    let graph = ReferenceGraph::build(retained_context, &references, limits.references)?;
    retained_context
        .check()
        .map_err(KnowledgeDocumentProjectionError::from)?;
    let stale_pages = assess(
        retained_context,
        &graph,
        &published,
        &observations,
        evaluated_at,
        limits.staleness,
    )?;
    projection.supplied_context(context)?;
    Ok(PreparedKnowledgeAssessment {
        projection,
        graph,
        stale_pages,
    })
}
