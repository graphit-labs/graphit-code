//! Complete quality diagnostics for an authorized, published Knowledge snapshot.
//!
//! Persisted markers and age are snapshot facts, not a fresh source assessment.
//! The caller retains Data's EOF documents, observed generation and original request.

use crate::{
    document::{DocumentParseError, DocumentParseLimits, parse_document},
    published_metadata_lint::{
        PublishedMetadataLintError, PublishedMetadataLintLimits, PublishedMetadataLintReport,
        lint_published_metadata,
    },
    references::{PageReferences, ReferenceError, ReferenceGraph, ReferenceLimits},
};
use graphit_core::file_io::SnapshotContext;
use graphit_data::knowledge_index::{KnowledgeDocument, KnowledgeGeneration};
use std::{
    collections::TryReserveError,
    error::Error,
    fmt, io,
    num::NonZeroUsize,
    time::{Duration, SystemTime},
};

#[derive(Clone, Copy, Debug)]
pub struct PublishedLintPolicy {
    pub evaluated_at: SystemTime,
    /// None disables age only; persisted markers are still reported.
    pub max_age: Option<Duration>,
}

#[derive(Clone, Copy, Debug)]
pub struct PublishedLintLimits {
    pub max_pages: NonZeroUsize,
    /// Raw UTF-8 of all inspected fields, including body, fallback title and targets.
    pub max_input_bytes: NonZeroUsize,
    /// All findings, including recommended metadata fields.
    pub max_findings: NonZeroUsize,
    /// Owned orphan slugs and both strings of each broken source-target pair.
    pub max_reference_result_bytes: NonZeroUsize,
    pub document: DocumentParseLimits,
    pub references: ReferenceLimits,
    pub metadata: PublishedMetadataLintLimits,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PublishedBrokenLink {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShortPublishedPage<'document> {
    pub slug: &'document str,
    /// Complete count for a short page; long pages do not return a count.
    pub word_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishedStaleReason {
    Persisted { since: SystemTime },
    Age { updated_at: SystemTime },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedStaleFinding<'document> {
    pub slug: &'document str,
    pub reason: PublishedStaleReason,
}

#[derive(Debug)]
pub struct PublishedLintReport<'document, 'generation> {
    pub generation: &'generation KnowledgeGeneration,
    pub total_pages: usize,
    /// Required metadata, orphans, broken pairs, short pages and stale pages.
    /// Recommended metadata findings do not increment this count.
    pub issue_count: usize,
    pub metadata: PublishedMetadataLintReport<'document>,
    pub orphans: Vec<String>,
    pub broken_links: Vec<PublishedBrokenLink>,
    pub short_pages: Vec<ShortPublishedPage<'document>>,
    pub stale_pages: Vec<PublishedStaleFinding<'document>>,
}

#[derive(Debug)]
pub enum PublishedLintError {
    Request(io::Error),
    Metadata(PublishedMetadataLintError),
    References(ReferenceError),
    Parse(DocumentParseError),
    NoDocuments,
    InvalidAgePolicy,
    Limit(&'static str),
    Overflow(&'static str),
    Allocation(TryReserveError),
}

impl fmt::Display for PublishedLintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => write!(f, "published lint request: {e}"),
            Self::Metadata(e) => write!(f, "published lint metadata: {e}"),
            Self::References(e) => write!(f, "published lint references: {e}"),
            Self::Parse(e) => write!(f, "published lint document: {e}"),
            Self::NoDocuments => f.write_str("published lint requires documents"),
            Self::InvalidAgePolicy => f.write_str("published lint age must be positive"),
            Self::Limit(name) => write!(f, "published lint limit exceeded: {name}"),
            Self::Overflow(name) => write!(f, "published lint count overflow: {name}"),
            Self::Allocation(e) => write!(f, "published lint allocation: {e}"),
        }
    }
}

impl Error for PublishedLintError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::Metadata(e) => Some(e),
            Self::References(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}

/// Diagnose one complete published snapshot without I/O or mutation.
///
/// Metadata, short and stale findings follow input order; reference findings use
/// the graph's UTF-8 order. Generation is borrowed observation data, not authority.
/// An error exposes no partial report. SDK allocations and synchronous scans are
/// governed by logical producer limits, not hard RSS or preemption guarantees.
pub fn lint_published_snapshot<'document, 'generation>(
    original: &SnapshotContext,
    generation: &'generation KnowledgeGeneration,
    documents: &'document [KnowledgeDocument],
    policy: PublishedLintPolicy,
    limits: PublishedLintLimits,
) -> Result<PublishedLintReport<'document, 'generation>, PublishedLintError> {
    check(original)?;
    if documents.is_empty() {
        return Err(PublishedLintError::NoDocuments);
    }
    if policy.max_age.is_some_and(|age| age.is_zero()) {
        return Err(PublishedLintError::InvalidAgePolicy);
    }
    preflight(original, documents, limits)?;
    let metadata = lint_published_metadata(original, documents, limits.metadata)
        .map_err(PublishedLintError::Metadata)?;
    check(original)?;

    let mut groups = Vec::new();
    reserve(&mut groups, documents.len())?;
    for document in documents {
        check(original)?;
        let mut targets = Vec::new();
        reserve(&mut targets, document.resolved_wiki_targets.len())?;
        for target in &document.resolved_wiki_targets {
            check(original)?;
            targets.push(target.as_str());
        }
        groups.push(targets);
    }
    let mut pages = Vec::new();
    reserve(&mut pages, documents.len())?;
    for (document, targets) in documents.iter().zip(&groups) {
        check(original)?;
        pages.push(PageReferences {
            slug: &document.slug,
            title: &document.title,
            targets,
        });
    }
    let graph = ReferenceGraph::build(original, &pages, limits.references)
        .map_err(PublishedLintError::References)?;
    let stats = graph
        .stats(original)
        .map_err(PublishedLintError::References)?;
    let mut findings = bounded_add(0, metadata.findings.len(), limits.max_findings, "findings")?;
    findings = bounded_add(findings, stats.orphans, limits.max_findings, "findings")?;
    findings = bounded_add(
        findings,
        stats.broken_edges,
        limits.max_findings,
        "findings",
    )?;

    // Graph outputs borrow local target groups. Own reference text explicitly.
    let mut reference_bytes = 0;
    let mut orphans = Vec::new();
    for slug in graph
        .orphans(original, limits.max_findings)
        .map_err(PublishedLintError::References)?
    {
        check(original)?;
        let slug = copy_reference(original, slug, &mut reference_bytes, limits)?;
        reserve(&mut orphans, 1)?;
        orphans.push(slug);
    }
    let mut broken_links = Vec::new();
    for link in graph
        .broken_links(original, limits.max_findings)
        .map_err(PublishedLintError::References)?
    {
        check(original)?;
        let source = copy_reference(original, link.source, &mut reference_bytes, limits)?;
        let target = copy_reference(original, link.target, &mut reference_bytes, limits)?;
        reserve(&mut broken_links, 1)?;
        broken_links.push(PublishedBrokenLink { source, target });
    }

    let mut short_pages = Vec::new();
    let mut stale_pages = Vec::new();
    for document in documents {
        check(original)?;
        let parsed = parse_document(
            original,
            document.body.as_bytes(),
            &document.recipe_parameters.fallback_title,
            limits.document,
        )
        .map_err(PublishedLintError::Parse)?;
        check(original)?;
        if let Some(word_count) = short_word_count(original, &parsed.source[parsed.envelope.body])?
        {
            findings = bounded_add(findings, 1, limits.max_findings, "findings")?;
            reserve(&mut short_pages, 1)?;
            short_pages.push(ShortPublishedPage {
                slug: &document.slug,
                word_count,
            });
        }
        check(original)?;
        let reason = document
            .stale_since
            .map(|since| PublishedStaleReason::Persisted { since })
            .or_else(|| {
                let age = policy.max_age?;
                let updated_at = document.updated_at?;
                let elapsed = policy.evaluated_at.duration_since(updated_at).ok()?;
                (elapsed > age).then_some(PublishedStaleReason::Age { updated_at })
            });
        if let Some(reason) = reason {
            findings = bounded_add(findings, 1, limits.max_findings, "findings")?;
            reserve(&mut stale_pages, 1)?;
            stale_pages.push(PublishedStaleFinding {
                slug: &document.slug,
                reason,
            });
        }
    }
    let issue_count = [
        orphans.len(),
        broken_links.len(),
        short_pages.len(),
        stale_pages.len(),
    ]
    .into_iter()
    .try_fold(metadata.required_errors, |count, amount| {
        checked_add(count, amount, "issues")
    })?;
    check(original)?;
    Ok(PublishedLintReport {
        generation,
        total_pages: documents.len(),
        issue_count,
        metadata,
        orphans,
        broken_links,
        short_pages,
        stale_pages,
    })
}

fn preflight(
    original: &SnapshotContext,
    documents: &[KnowledgeDocument],
    limits: PublishedLintLimits,
) -> Result<(), PublishedLintError> {
    bounded_add(0, documents.len(), limits.max_pages, "pages")?;
    let mut bytes = 0;
    for document in documents {
        for field in [
            document.slug.as_str(),
            document.title.as_str(),
            document.kind.as_str(),
            document.description.as_deref().unwrap_or(""),
            document.body.as_str(),
            document.recipe_parameters.fallback_title.as_str(),
        ] {
            check(original)?;
            bytes = bounded_add(bytes, field.len(), limits.max_input_bytes, "input bytes")?;
        }
        for target in &document.resolved_wiki_targets {
            check(original)?;
            bytes = bounded_add(bytes, target.len(), limits.max_input_bytes, "input bytes")?;
        }
    }
    check(original)
}

fn short_word_count(
    original: &SnapshotContext,
    body: &str,
) -> Result<Option<usize>, PublishedLintError> {
    let mut words = 0;
    for _ in body.split_whitespace() {
        check(original)?;
        words = checked_add(words, 1, "words")?;
        if words > 10 {
            return Ok(None);
        }
    }
    check(original)?;
    Ok(Some(words))
}

fn copy_reference(
    original: &SnapshotContext,
    text: &str,
    bytes: &mut usize,
    limits: PublishedLintLimits,
) -> Result<String, PublishedLintError> {
    check(original)?;
    *bytes = bounded_add(
        *bytes,
        text.len(),
        limits.max_reference_result_bytes,
        "reference result bytes",
    )?;
    let mut owned = String::new();
    owned
        .try_reserve_exact(text.len())
        .map_err(PublishedLintError::Allocation)?;
    check(original)?;
    owned.push_str(text);
    check(original)?;
    Ok(owned)
}

fn reserve<T>(items: &mut Vec<T>, amount: usize) -> Result<(), PublishedLintError> {
    items
        .try_reserve(amount)
        .map_err(PublishedLintError::Allocation)
}
fn check(original: &SnapshotContext) -> Result<(), PublishedLintError> {
    original.check().map_err(PublishedLintError::Request)
}
fn checked_add(
    current: usize,
    amount: usize,
    name: &'static str,
) -> Result<usize, PublishedLintError> {
    current
        .checked_add(amount)
        .ok_or(PublishedLintError::Overflow(name))
}
fn bounded_add(
    current: usize,
    amount: usize,
    limit: NonZeroUsize,
    name: &'static str,
) -> Result<usize, PublishedLintError> {
    let value = checked_add(current, amount, name)?;
    if value > limit.get() {
        return Err(PublishedLintError::Limit(name));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_core::file_lock::Cancellation;
    use std::time::Instant;

    #[test]
    fn arithmetic_overflow_is_distinct_from_capacity_and_exact_capacity_passes() {
        let maximum = NonZeroUsize::new(usize::MAX).unwrap();
        assert_eq!(
            bounded_add(usize::MAX - 1, 1, maximum, "findings").unwrap(),
            usize::MAX
        );
        assert!(matches!(
            bounded_add(usize::MAX, 1, maximum, "findings"),
            Err(PublishedLintError::Overflow("findings"))
        ));
        assert!(matches!(
            bounded_add(1, 1, NonZeroUsize::new(1).unwrap(), "findings"),
            Err(PublishedLintError::Limit("findings"))
        ));
    }

    #[test]
    fn canceled_word_scan_preserves_original_io_cause() {
        let cancel = Cancellation::default();
        let original =
            SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
        cancel.cancel();
        let error = short_word_count(&original, "one two three").unwrap_err();
        assert!(
            matches!(&error, PublishedLintError::Request(e) if e.kind() == io::ErrorKind::Interrupted)
        );
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .is_some()
        );
    }
}
