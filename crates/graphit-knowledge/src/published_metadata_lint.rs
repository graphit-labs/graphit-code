//! Quality diagnostics for effective metadata in a complete published corpus.
//!
//! The caller retains Data's documents and generation after an authorized EOF read.
//! This stage does not reconstruct authored provenance or perform storage I/O.

use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_index::KnowledgeDocument;
use std::{collections::TryReserveError, error::Error, fmt, io, num::NonZeroUsize};

/// Logical capacities for the fields inspected and the findings returned.
#[derive(Clone, Copy, Debug)]
pub struct PublishedMetadataLintLimits {
    pub max_pages: NonZeroUsize,
    /// Aggregate UTF-8 bytes of slug, title, kind and present description, before trim.
    pub max_input_bytes: NonZeroUsize,
    pub max_findings: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishedMetadataField {
    Kind,
    Title,
    Description,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishedMetadataSeverity {
    Required,
    Recommended,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedMetadataFinding<'document> {
    pub slug: &'document str,
    pub field: PublishedMetadataField,
    pub severity: PublishedMetadataSeverity,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PublishedMetadataLintReport<'document> {
    pub total_pages: usize,
    /// Counts missing effective kind values; recommended fields do not increment it.
    pub required_errors: usize,
    pub findings: Vec<PublishedMetadataFinding<'document>>,
}

#[derive(Debug)]
pub enum PublishedMetadataLintError {
    Request(io::Error),
    NoDocuments,
    LimitExceeded(&'static str),
    Overflow(&'static str),
    Allocation(TryReserveError),
}

impl fmt::Display for PublishedMetadataLintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => write!(f, "published metadata request: {e}"),
            Self::NoDocuments => f.write_str("published metadata lint requires documents"),
            Self::LimitExceeded(name) => write!(f, "published metadata limit exceeded: {name}"),
            Self::Overflow(name) => write!(f, "published metadata count overflow: {name}"),
            Self::Allocation(e) => write!(f, "published metadata allocation failed: {e}"),
        }
    }
}

impl Error for PublishedMetadataLintError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}

/// Diagnose kind, title and the integral nullable description without mutation.
///
/// A missing/blank kind is required; blank title and missing/blank description
/// are recommended. Summary is an independent editorial preview, never a
/// replacement for description. Order follows input pages, then kind, title,
/// description. An error returns no partial report.
///
/// Cancellation is cooperative at field/operation boundaries. Capacities are
/// logical counts, not allocator quotas or preemption inside `str::trim`.
pub fn lint_published_metadata<'document>(
    original: &SnapshotContext,
    documents: &'document [KnowledgeDocument],
    limits: PublishedMetadataLintLimits,
) -> Result<PublishedMetadataLintReport<'document>, PublishedMetadataLintError> {
    check(original)?;
    if documents.is_empty() {
        return Err(PublishedMetadataLintError::NoDocuments);
    }
    bounded_add(0, documents.len(), limits.max_pages, "pages")?;
    let mut input_bytes = 0;
    for document in documents {
        for field in [
            document.slug.as_str(),
            &document.title,
            &document.kind,
            document.description.as_deref().unwrap_or(""),
        ] {
            check(original)?;
            input_bytes = bounded_add(
                input_bytes,
                field.len(),
                limits.max_input_bytes,
                "input bytes",
            )?;
        }
    }
    let mut report = PublishedMetadataLintReport {
        total_pages: documents.len(),
        required_errors: 0,
        findings: Vec::new(),
    };
    for document in documents {
        for (field, text, severity) in [
            (
                PublishedMetadataField::Kind,
                document.kind.as_str(),
                PublishedMetadataSeverity::Required,
            ),
            (
                PublishedMetadataField::Title,
                document.title.as_str(),
                PublishedMetadataSeverity::Recommended,
            ),
            (
                PublishedMetadataField::Description,
                document.description.as_deref().unwrap_or(""),
                PublishedMetadataSeverity::Recommended,
            ),
        ] {
            check(original)?;
            let blank = text.trim().is_empty();
            check(original)?;
            if blank {
                bounded_add(report.findings.len(), 1, limits.max_findings, "findings")?;
                report
                    .findings
                    .try_reserve(1)
                    .map_err(PublishedMetadataLintError::Allocation)?;
                check(original)?;
                report.findings.push(PublishedMetadataFinding {
                    slug: &document.slug,
                    field,
                    severity,
                });
                if severity == PublishedMetadataSeverity::Required {
                    report.required_errors = report
                        .required_errors
                        .checked_add(1)
                        .ok_or(PublishedMetadataLintError::Overflow("required errors"))?;
                }
            }
        }
    }
    check(original)?;
    Ok(report)
}

fn check(original: &SnapshotContext) -> Result<(), PublishedMetadataLintError> {
    original
        .check()
        .map_err(PublishedMetadataLintError::Request)
}

fn bounded_add(
    current: usize,
    amount: usize,
    limit: NonZeroUsize,
    name: &'static str,
) -> Result<usize, PublishedMetadataLintError> {
    let total = current
        .checked_add(amount)
        .ok_or(PublishedMetadataLintError::Overflow(name))?;
    if total > limit.get() {
        return Err(PublishedMetadataLintError::LimitExceeded(name));
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_overflow_retains_a_distinct_error_from_capacity() {
        let maximum = NonZeroUsize::new(usize::MAX).unwrap();
        assert_eq!(
            bounded_add(usize::MAX - 1, 1, maximum, "input bytes").unwrap(),
            usize::MAX
        );
        assert!(matches!(
            bounded_add(usize::MAX, 1, maximum, "input bytes"),
            Err(PublishedMetadataLintError::Overflow("input bytes"))
        ));
        assert!(matches!(
            bounded_add(1, 1, NonZeroUsize::new(1).unwrap(), "findings"),
            Err(PublishedMetadataLintError::LimitExceeded("findings"))
        ));
    }
}
