//! Bounded staleness over published pages and complete current observations.
//!
//! This module performs no I/O and grants no authority. The caller supplies the
//! evaluation clock and proves the capture, generation and source permissions.
//!
//! ```
//! use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
//! use graphit_knowledge::{references::{PageReferences, ReferenceGraph, ReferenceLimits},
//!     staleness::{assess, PublishedPage, SourceFingerprint, SourceObservation,
//!         SourceState, StaleReason, StalenessLimits}};
//! use std::{num::NonZeroUsize, time::{Duration, Instant, UNIX_EPOCH}};
//! let n = |value| NonZeroUsize::new(value).unwrap();
//! let ctx = SnapshotContext::new(Instant::now() + Duration::from_secs(30), Cancellation::default());
//! let graph = ReferenceGraph::build(&ctx, &[
//!     PageReferences { slug: "guide", title: "Guide", targets: &["api"] },
//!     PageReferences { slug: "api", title: "API", targets: &[] },
//! ], ReferenceLimits { max_identifier_bytes: n(10) })?;
//! let pages = [
//!     PublishedPage { slug: "guide", source: None, updated_at: None,
//!         stale_after: None, stale_since: None },
//!     PublishedPage { slug: "api", source: Some(SourceFingerprint {
//!         path: "api.md", digest: [1; 32] }), updated_at: None,
//!         stale_after: None, stale_since: None },
//! ];
//! let result = assess(&ctx, &graph, &pages, &[SourceObservation {
//!     path: "api.md", state: SourceState::Removed }], UNIX_EPOCH,
//!     StalenessLimits { max_pages: n(2), max_graph_edges: n(1),
//!         max_observations: n(1), max_path_bytes: n(6), max_metadata_bytes: n(20),
//!         max_results: n(2) })?;
//! assert_eq!(result.iter().map(|page| page.slug).collect::<Vec<_>>(), ["api", "guide"]);
//! assert_eq!(result[0].reason, StaleReason::SourceRemoved { path: "api.md" });
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::references::{Direction, ReferenceError, ReferenceGraph, ReferenceHit};
use graphit_core::file_io::SnapshotContext;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt, io,
    num::NonZeroUsize,
    time::{Duration, SystemTime},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFingerprint<'a> {
    pub path: &'a str,
    pub digest: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
pub struct PublishedPage<'a> {
    pub slug: &'a str,
    pub source: Option<SourceFingerprint<'a>>,
    pub updated_at: Option<SystemTime>,
    pub stale_after: Option<Duration>,
    pub stale_since: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceState {
    Present([u8; 32]),
    Removed,
}

#[derive(Clone, Copy, Debug)]
pub struct SourceObservation<'a> {
    pub path: &'a str,
    pub state: SourceState,
}

/// Bounds input data and result size.
#[derive(Clone, Copy, Debug)]
pub struct StalenessLimits {
    pub max_pages: NonZeroUsize,
    /// All unique graph links, including dangling links, bound the inbound cache.
    pub max_graph_edges: NonZeroUsize,
    pub max_observations: NonZeroUsize,
    pub max_path_bytes: NonZeroUsize,
    /// Raw UTF-8 bytes of page slugs, source paths and all observation paths.
    pub max_metadata_bytes: NonZeroUsize,
    pub max_results: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootCause<'a> {
    SourceChanged { path: &'a str },
    SourceRemoved { path: &'a str },
    Age,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaleReason<'a> {
    SourceChanged {
        path: &'a str,
    },
    SourceRemoved {
        path: &'a str,
    },
    Age,
    Dependency {
        root_slug: &'a str,
        root_cause: RootCause<'a>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StalePage<'a> {
    pub slug: &'a str,
    pub since: SystemTime,
    pub reason: StaleReason<'a>,
}

#[derive(Debug)]
pub enum StalenessError {
    LimitExceeded(&'static str),
    InvalidPath,
    DuplicatePage,
    DuplicateObservation,
    CorpusMismatch,
    UnobservedSource,
    InvalidTimestamp,
    InvalidAgeLimit,
    References(ReferenceError),
    Request(io::Error),
}

impl fmt::Display for StalenessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded(boundary) => write!(f, "staleness limit exceeded: {boundary}"),
            Self::InvalidPath => f.write_str("invalid relative source path"),
            Self::DuplicatePage => f.write_str("duplicate published page"),
            Self::DuplicateObservation => f.write_str("duplicate source observation"),
            Self::CorpusMismatch => f.write_str("published pages and reference corpus differ"),
            Self::UnobservedSource => f.write_str("published source has no current observation"),
            Self::InvalidTimestamp => f.write_str("invalid staleness timestamp"),
            Self::InvalidAgeLimit => {
                f.write_str("age requires a positive duration and update time")
            }
            Self::References(error) => write!(f, "staleness references: {error}"),
            Self::Request(error) => write!(f, "staleness request: {error}"),
        }
    }
}

impl Error for StalenessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::References(error) => Some(error),
            Self::Request(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ReferenceError> for StalenessError {
    fn from(error: ReferenceError) -> Self {
        Self::References(error)
    }
}

impl From<io::Error> for StalenessError {
    fn from(error: io::Error) -> Self {
        Self::Request(error)
    }
}

struct CachedPage<'a> {
    published: PublishedPage<'a>,
    inbound: Vec<ReferenceHit<'a>>,
    direct: Option<(RootCause<'a>, SystemTime)>,
}

/// Compare complete published/current inputs, then propagate inbound transitively.
///
/// Source drift takes precedence over age; all timestamps are still validated.
/// Direct reasons prevail over dependencies. A dependency names the smallest
/// reachable root slug, independently of input order. The result is sorted by
/// slug. Any error discards the entire result. No clock or filesystem is read.
pub fn assess<'a>(
    context: &SnapshotContext,
    graph: &ReferenceGraph<'_, 'a>,
    pages: &[PublishedPage<'a>],
    observations: &[SourceObservation<'a>],
    evaluated_at: SystemTime,
    limits: StalenessLimits,
) -> Result<Vec<StalePage<'a>>, StalenessError> {
    // The real producer checks its ctx request before any consumer work.
    let stats = graph.stats(context)?;
    context.check()?;
    bounded(pages.len(), limits.max_pages, "pages")?;
    bounded(stats.links, limits.max_graph_edges, "graph edges")?;
    bounded(observations.len(), limits.max_observations, "observations")?;
    preflight(context, pages, observations, limits)?;

    let mut cache = BTreeMap::new();
    for &published in pages {
        context.check()?;
        if cache
            .insert(
                published.slug,
                CachedPage {
                    published,
                    inbound: Vec::new(),
                    direct: None,
                },
            )
            .is_some()
        {
            return Err(StalenessError::DuplicatePage);
        }
    }
    if stats.pages != cache.len() {
        return Err(StalenessError::CorpusMismatch);
    }
    let mut current = BTreeMap::new();
    for observation in observations {
        context.check()?;
        if current
            .insert(observation.path, observation.state)
            .is_some()
        {
            return Err(StalenessError::DuplicateObservation);
        }
    }

    let mut cached_edges = 0;
    for (&slug, page) in &mut cache {
        context.check()?;
        page.inbound = match graph.neighbors(context, slug, Direction::Inbound, 1, limits.max_pages)
        {
            Err(ReferenceError::UnknownPage) => return Err(StalenessError::CorpusMismatch),
            result => result?,
        };
        add_bounded(
            &mut cached_edges,
            page.inbound.len(),
            limits.max_graph_edges,
            "graph edges",
        )?;
        let expiry = age_expiry(page.published, evaluated_at)?;
        let source_cause = match page.published.source {
            None => None,
            Some(source) => match current
                .get(source.path)
                .ok_or(StalenessError::UnobservedSource)?
            {
                SourceState::Present(digest) if *digest != source.digest => {
                    Some(RootCause::SourceChanged { path: source.path })
                }
                SourceState::Removed => Some(RootCause::SourceRemoved { path: source.path }),
                SourceState::Present(_) => None,
            },
        };
        page.direct = match source_cause {
            Some(cause) => Some((cause, page.published.stale_since.unwrap_or(evaluated_at))),
            None => {
                expiry.map(|since| (RootCause::Age, page.published.stale_since.unwrap_or(since)))
            }
        };
    }

    let mut stale = BTreeMap::new();
    for (&slug, page) in &cache {
        context.check()?;
        if let Some((cause, since)) = page.direct {
            insert_stale(
                &mut stale,
                StalePage {
                    slug,
                    since,
                    reason: match cause {
                        RootCause::SourceChanged { path } => StaleReason::SourceChanged { path },
                        RootCause::SourceRemoved { path } => StaleReason::SourceRemoved { path },
                        RootCause::Age => StaleReason::Age,
                    },
                },
                limits.max_results,
            )?;
        }
    }

    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    if !stale.is_empty() {
        queue.try_reserve(cache.len()).map_err(allocation_error)?;
    }
    for (&root_slug, root) in &cache {
        context.check()?;
        let Some((root_cause, _)) = root.direct else {
            continue;
        };
        if !visited.insert(root_slug) {
            continue;
        }
        queue.push_back(root_slug);
        while let Some(&slug) = queue.front() {
            context.check()?;
            let _ = queue.pop_front();
            let page = cache.get(slug).ok_or(StalenessError::CorpusMismatch)?;
            for hit in &page.inbound {
                context.check()?;
                if !visited.insert(hit.slug) {
                    continue;
                }
                let dependent = cache.get(hit.slug).ok_or(StalenessError::CorpusMismatch)?;
                insert_stale(
                    &mut stale,
                    StalePage {
                        slug: hit.slug,
                        since: dependent.published.stale_since.unwrap_or(evaluated_at),
                        reason: StaleReason::Dependency {
                            root_slug,
                            root_cause,
                        },
                    },
                    limits.max_results,
                )?;
                queue.push_back(hit.slug);
            }
        }
    }

    context.check()?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(stale.len())
        .map_err(allocation_error)?;
    for page in stale.into_values() {
        context.check()?;
        result.push(page);
    }
    context.check()?;
    Ok(result)
}

fn preflight(
    context: &SnapshotContext,
    pages: &[PublishedPage<'_>],
    observations: &[SourceObservation<'_>],
    limits: StalenessLimits,
) -> Result<(), StalenessError> {
    let mut bytes = 0;
    for page in pages {
        context.check()?;
        add_bounded(
            &mut bytes,
            page.slug.len(),
            limits.max_metadata_bytes,
            "metadata bytes",
        )?;
        if let Some(source) = page.source {
            path(source.path, limits.max_path_bytes)?;
            add_bounded(
                &mut bytes,
                source.path.len(),
                limits.max_metadata_bytes,
                "metadata bytes",
            )?;
        }
    }
    for observation in observations {
        context.check()?;
        path(observation.path, limits.max_path_bytes)?;
        add_bounded(
            &mut bytes,
            observation.path.len(),
            limits.max_metadata_bytes,
            "metadata bytes",
        )?;
    }
    context.check()?;
    Ok(())
}

fn path(value: &str, max_bytes: NonZeroUsize) -> Result<(), StalenessError> {
    bounded(value.len(), max_bytes, "path bytes")?;
    if value.is_empty()
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '\\'))
        || value
            .split('/')
            .any(|component| matches!(component, "" | "." | ".."))
    {
        return Err(StalenessError::InvalidPath);
    }
    Ok(())
}

fn age_expiry(
    page: PublishedPage<'_>,
    evaluated_at: SystemTime,
) -> Result<Option<SystemTime>, StalenessError> {
    if page.updated_at.is_some_and(|time| time > evaluated_at)
        || page.stale_since.is_some_and(|time| time > evaluated_at)
    {
        return Err(StalenessError::InvalidTimestamp);
    }
    let Some(duration) = page.stale_after else {
        return Ok(None);
    };
    if duration.is_zero() {
        return Err(StalenessError::InvalidAgeLimit);
    }
    let updated_at = page.updated_at.ok_or(StalenessError::InvalidAgeLimit)?;
    let expiry = updated_at
        .checked_add(duration)
        .ok_or(StalenessError::InvalidTimestamp)?;
    Ok((expiry <= evaluated_at).then_some(expiry))
}

fn insert_stale<'a>(
    stale: &mut BTreeMap<&'a str, StalePage<'a>>,
    page: StalePage<'a>,
    limit: NonZeroUsize,
) -> Result<(), StalenessError> {
    if !stale.contains_key(page.slug) {
        let count = stale
            .len()
            .checked_add(1)
            .ok_or(StalenessError::LimitExceeded("results"))?;
        bounded(count, limit, "results")?;
        stale.insert(page.slug, page);
    }
    Ok(())
}

fn bounded(
    value: usize,
    limit: NonZeroUsize,
    boundary: &'static str,
) -> Result<(), StalenessError> {
    if value > limit.get() {
        Err(StalenessError::LimitExceeded(boundary))
    } else {
        Ok(())
    }
}

fn add_bounded(
    used: &mut usize,
    additional: usize,
    limit: NonZeroUsize,
    boundary: &'static str,
) -> Result<(), StalenessError> {
    let next = used
        .checked_add(additional)
        .ok_or(StalenessError::LimitExceeded(boundary))?;
    bounded(next, limit, boundary)?;
    *used = next;
    Ok(())
}

fn allocation_error(_: std::collections::TryReserveError) -> StalenessError {
    StalenessError::Request(io::ErrorKind::OutOfMemory.into())
}
