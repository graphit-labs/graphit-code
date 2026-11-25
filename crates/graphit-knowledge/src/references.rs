//! Bounded references over a complete corpus, including dangling destinations.

use graphit_core::file_io::SnapshotContext;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt, io,
    num::NonZeroUsize,
};

/// Producer-resolved page links; this layer does not parse or resolve Markdown.
#[derive(Clone, Copy, Debug)]
pub struct PageReferences<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub targets: &'a [&'a str],
}

/// Identifier byte limits; result counts are bounded by each query.
/// These limits do not claim a bound on the standard allocator's RSS.
#[derive(Clone, Copy, Debug)]
pub struct ReferenceLimits {
    pub max_identifier_bytes: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Inbound,
    Outbound,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReferenceStats {
    pub pages: usize,
    pub links: usize,
    pub inbound_pages: usize,
    pub orphans: usize,
    pub broken_edges: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrokenLink<'a> {
    pub source: &'a str,
    pub target: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceHit<'a> {
    pub slug: &'a str,
    /// `None` for a dangling destination; known pages use their slug if untitled.
    pub title: Option<&'a str>,
    pub depth: u8,
}

#[derive(Debug)]
pub enum ReferenceError {
    LimitExceeded(&'static str),
    InvalidIdentifier,
    DuplicatePage,
    UnknownPage,
    InvalidDepth,
    DifferentRequest,
    Request(io::Error),
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitExceeded(boundary) => write!(f, "reference limit exceeded: {boundary}"),
            Self::InvalidIdentifier => f.write_str("invalid page identifier"),
            Self::DuplicatePage => f.write_str("duplicate page identifier"),
            Self::UnknownPage => f.write_str("page is not present in the corpus"),
            Self::InvalidDepth => f.write_str("reference depth must be between 1 and 3"),
            Self::DifferentRequest => f.write_str("reference graph belongs to another request"),
            Self::Request(error) => write!(f, "reference request: {error}"),
        }
    }
}

impl Error for ReferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ReferenceError {
    fn from(error: io::Error) -> Self {
        Self::Request(error)
    }
}

/// A request-bound view of complete producer-resolved links.
///
/// Identity checks do not grant access or prove the corpus generation. The caller
/// owns that evidence. All text remains borrowed; graph allocations contain only
/// references and collection bookkeeping. No filesystem I/O is performed.
pub struct ReferenceGraph<'context, 'data> {
    context: &'context SnapshotContext,
    limits: ReferenceLimits,
    titles: BTreeMap<&'data str, &'data str>,
    outbound: BTreeMap<&'data str, BTreeSet<&'data str>>,
    inbound: BTreeMap<&'data str, BTreeSet<&'data str>>,
    stats: ReferenceStats,
}

impl<'context, 'data> ReferenceGraph<'context, 'data> {
    /// Validate every identifier before constructing the complete graph.
    pub fn build(
        context: &'context SnapshotContext,
        pages: &[PageReferences<'data>],
        limits: ReferenceLimits,
    ) -> Result<Self, ReferenceError> {
        context.check()?;
        for page in pages {
            context.check()?;
            identifier(page.slug, limits)?;
            for target in page.targets {
                context.check()?;
                if !target.is_empty() {
                    identifier(target, limits)?;
                }
            }
        }

        let mut graph = Self {
            context,
            limits,
            titles: BTreeMap::new(),
            outbound: BTreeMap::new(),
            inbound: BTreeMap::new(),
            stats: ReferenceStats::default(),
        };
        for page in pages {
            context.check()?;
            if graph.titles.insert(page.slug, page.title).is_some() {
                return Err(ReferenceError::DuplicatePage);
            }
            for &target in page.targets {
                context.check()?;
                if target.is_empty() || target == page.slug {
                    continue;
                }
                if graph.outbound.entry(page.slug).or_default().insert(target) {
                    graph.inbound.entry(target).or_default().insert(page.slug);
                    graph.stats.links += 1;
                }
            }
        }
        graph.stats.pages = graph.titles.len();
        for slug in graph.titles.keys() {
            context.check()?;
            if graph.inbound.contains_key(slug) {
                graph.stats.inbound_pages += 1;
            } else if !graph.outbound.contains_key(slug) {
                graph.stats.orphans += 1;
            }
        }
        for targets in graph.outbound.values() {
            for target in targets {
                context.check()?;
                if !graph.titles.contains_key(target) {
                    graph.stats.broken_edges += 1;
                }
            }
        }
        context.check()?;
        Ok(graph)
    }

    /// Cached statistics still require a live original request.
    pub fn stats(&self, context: &SnapshotContext) -> Result<ReferenceStats, ReferenceError> {
        self.check(context)?;
        self.context.check()?;
        Ok(self.stats)
    }

    /// Isolated pages have neither inbound nor outbound non-self references.
    pub fn orphans(
        &self,
        context: &SnapshotContext,
        max_results: NonZeroUsize,
    ) -> Result<Vec<&'data str>, ReferenceError> {
        self.check(context)?;
        let mut result = Vec::new();
        for &slug in self.titles.keys() {
            self.context.check()?;
            if !self.inbound.contains_key(slug) && !self.outbound.contains_key(slug) {
                push_bounded(&mut result, slug, max_results)?;
            }
        }
        self.context.check()?;
        Ok(result)
    }

    /// Preserve every broken source-target pair, ordered by source then target.
    pub fn broken_links(
        &self,
        context: &SnapshotContext,
        max_results: NonZeroUsize,
    ) -> Result<Vec<BrokenLink<'data>>, ReferenceError> {
        self.check(context)?;
        let mut result = Vec::new();
        for (&source, targets) in &self.outbound {
            for &target in targets {
                self.context.check()?;
                if !self.titles.contains_key(target) {
                    push_bounded(&mut result, BrokenLink { source, target }, max_results)?;
                }
            }
        }
        self.context.check()?;
        Ok(result)
    }

    /// Iterate the already-resolved direct adjacency in lexical order without
    /// allocating a result vector. Every item retains the original request guard.
    pub fn direct_neighbors<'view>(
        &'view self,
        context: &SnapshotContext,
        slug: &str,
        direction: Direction,
    ) -> Result<
        impl Iterator<Item = Result<ReferenceHit<'data>, ReferenceError>> + 'view,
        ReferenceError,
    > {
        self.check(context)?;
        identifier(slug, self.limits)?;
        let (&origin, _) = self
            .titles
            .get_key_value(slug)
            .ok_or(ReferenceError::UnknownPage)?;
        let adjacency = match direction {
            Direction::Inbound => &self.inbound,
            Direction::Outbound => &self.outbound,
        };
        Ok(adjacency
            .get(origin)
            .into_iter()
            .flatten()
            .map(move |&target| {
                self.context.check()?;
                Ok(ReferenceHit {
                    slug: target,
                    title: self.titles.get(target).copied(),
                    depth: 1,
                })
            }))
    }

    /// Traverse one fixed direction, with results ordered by minimum depth/slug.
    /// The origin is excluded. Dangling destinations are returned but not expanded.
    pub fn neighbors(
        &self,
        context: &SnapshotContext,
        slug: &str,
        direction: Direction,
        depth: u8,
        max_results: NonZeroUsize,
    ) -> Result<Vec<ReferenceHit<'data>>, ReferenceError> {
        self.check(context)?;
        identifier(slug, self.limits)?;
        if !(1..=3).contains(&depth) {
            return Err(ReferenceError::InvalidDepth);
        }
        let (&origin, _) = self
            .titles
            .get_key_value(slug)
            .ok_or(ReferenceError::UnknownPage)?;
        let adjacency = match direction {
            Direction::Inbound => &self.inbound,
            Direction::Outbound => &self.outbound,
        };
        let mut visited = BTreeSet::from([origin]);
        let mut queue = VecDeque::from([(origin, 0_u8)]);
        let mut result = Vec::new();
        while let Some((current, hop)) = queue.pop_front() {
            self.context.check()?;
            if hop == depth {
                continue;
            }
            if let Some(targets) = adjacency.get(current) {
                for &target in targets {
                    self.context.check()?;
                    if visited.contains(target) {
                        continue;
                    }
                    let title = self
                        .titles
                        .get(target)
                        .map(|&title| if title.is_empty() { target } else { title });
                    push_bounded(
                        &mut result,
                        ReferenceHit {
                            slug: target,
                            title,
                            depth: hop + 1,
                        },
                        max_results,
                    )?;
                    visited.insert(target);
                    if title.is_some() && hop + 1 < depth {
                        queue.push_back((target, hop + 1));
                    }
                }
            }
        }
        self.context.check()?;
        result.sort_unstable_by_key(|hit| (hit.depth, hit.slug));
        self.context.check()?;
        Ok(result)
    }

    fn check(&self, context: &SnapshotContext) -> Result<(), ReferenceError> {
        self.context.check()?;
        if !self.context.same_request(context) {
            return Err(ReferenceError::DifferentRequest);
        }
        Ok(())
    }

    pub(crate) fn community_titles(
        &self,
        context: &SnapshotContext,
    ) -> Result<&BTreeMap<&'data str, &'data str>, ReferenceError> {
        self.check(context)?;
        Ok(&self.titles)
    }

    pub(crate) fn community_outbound(
        &self,
        context: &SnapshotContext,
    ) -> Result<&BTreeMap<&'data str, BTreeSet<&'data str>>, ReferenceError> {
        self.check(context)?;
        Ok(&self.outbound)
    }
}

fn identifier(value: &str, limits: ReferenceLimits) -> Result<(), ReferenceError> {
    bounded(value.len(), limits.max_identifier_bytes, "identifier bytes")?;
    if value.is_empty()
        || value.starts_with("..")
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(ReferenceError::InvalidIdentifier);
    }
    Ok(())
}

fn bounded(
    value: usize,
    limit: NonZeroUsize,
    boundary: &'static str,
) -> Result<(), ReferenceError> {
    if value > limit.get() {
        Err(ReferenceError::LimitExceeded(boundary))
    } else {
        Ok(())
    }
}

fn push_bounded<T>(
    result: &mut Vec<T>,
    value: T,
    limit: NonZeroUsize,
) -> Result<(), ReferenceError> {
    if result.len() == limit.get() {
        return Err(ReferenceError::LimitExceeded("results"));
    }
    result
        .try_reserve(1)
        .map_err(|_| ReferenceError::Request(io::ErrorKind::OutOfMemory.into()))?;
    result.push(value);
    Ok(())
}
