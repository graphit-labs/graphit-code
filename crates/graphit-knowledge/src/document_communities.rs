//! Communities derived from one complete, request-bound reference graph.
//!
//! Classification creates no documents, grants, stored fields, or fingerprints.
//! The caller associates the owned report with its observed corpus generation.

use crate::references::{ReferenceError, ReferenceGraph};
use graphit_core::file_io::SnapshotContext;
use graphrs::{Edge, Graph, GraphSpecs, Node, algorithms::community::louvain::louvain_communities};
use std::{collections::HashSet, collections::TryReserveError, error::Error, fmt, io};

#[derive(Debug, PartialEq)]
pub struct DocumentCommunity {
    pub members: Vec<String>,
    pub label: String,
    /// Density of distinct known undirected pairs, rounded to two decimals.
    pub cohesion: f64,
}

#[derive(Debug, PartialEq)]
pub struct DocumentCommunities {
    /// Descending size, then lexical members. Singletons are omitted.
    pub groups: Vec<DocumentCommunity>,
}

#[derive(Debug)]
pub enum DocumentCommunityError {
    References(ReferenceError),
    Request(io::Error),
    Library(graphrs::Error),
    Allocation(TryReserveError),
    InvalidPartition,
}

impl fmt::Display for DocumentCommunityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::References(_) => f.write_str("document community references failed"),
            Self::Request(_) => f.write_str("document community request failed"),
            Self::Library(e) => write!(f, "document community classification failed: {e}"),
            Self::Allocation(_) => f.write_str("document community allocation failed"),
            Self::InvalidPartition => f.write_str("document community partition is inconsistent"),
        }
    }
}

impl Error for DocumentCommunityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::References(e) => Some(e),
            Self::Request(e) => Some(e),
            // The SDK error remains typed in the variant but does not implement StdError.
            Self::Library(_) => None,
            Self::Allocation(e) => Some(e),
            Self::InvalidPartition => None,
        }
    }
}

type Result<T> = std::result::Result<T, DocumentCommunityError>;

fn check(original: &SnapshotContext) -> Result<()> {
    original.check().map_err(DocumentCommunityError::Request)
}

fn reserve<T>(values: &mut Vec<T>, additional: usize) -> Result<()> {
    values
        .try_reserve(additional)
        .map_err(DocumentCommunityError::Allocation)
}

fn owned(text: &str) -> Result<String> {
    let mut value = String::new();
    value
        .try_reserve(text.len())
        .map_err(DocumentCommunityError::Allocation)?;
    value.push_str(text);
    Ok(value)
}

// Only metadata needed by the algorithm; no bodies, authority, or corpus proof.
struct AlgorithmInput<'data> {
    pages: Vec<(&'data str, &'data str)>,
    pairs: Vec<(usize, usize)>,
    degrees: Vec<usize>,
    graph: Graph<usize, ()>,
}

fn prepare<'data>(
    original: &SnapshotContext,
    graph: &ReferenceGraph<'_, 'data>,
) -> Result<AlgorithmInput<'data>> {
    check(original)?;
    let titles = graph
        .community_titles(original)
        .map_err(DocumentCommunityError::References)?;
    let outbound = graph
        .community_outbound(original)
        .map_err(DocumentCommunityError::References)?;
    let mut pages = Vec::new();
    reserve(&mut pages, titles.len())?;
    for (&slug, &title) in titles {
        check(original)?;
        pages.push((slug, title));
    }
    let mut pairs = Vec::new();
    for (&source, targets) in outbound {
        check(original)?;
        let from = pages
            .binary_search_by_key(&source, |p| p.0)
            .map_err(|_| DocumentCommunityError::InvalidPartition)?;
        for &target in targets {
            check(original)?;
            let Ok(to) = pages.binary_search_by_key(&target, |p| p.0) else {
                continue; // A dangling reference remains in the original graph.
            };
            reserve(&mut pairs, 1)?;
            pairs.push((from.min(to), from.max(to)));
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    check(original)?;
    let mut degrees = Vec::new();
    reserve(&mut degrees, pages.len())?;
    degrees.resize(pages.len(), 0usize);
    let mut sdk = Graph::new(GraphSpecs::undirected());
    for node in 0..pages.len() {
        check(original)?;
        sdk.add_node(Node::from_name(node));
    }
    for &(from, to) in &pairs {
        check(original)?;
        sdk.add_edge(Edge::new(from, to))
            .map_err(DocumentCommunityError::Library)?;
        // A simple known graph has degree at most pages.len() - 1.
        degrees[from] += 1;
        degrees[to] += 1;
    }
    check(original)?;
    Ok(AlgorithmInput {
        pages,
        pairs,
        degrees,
        graph: sdk,
    })
}

fn finish(
    original: &SnapshotContext,
    input: &AlgorithmInput<'_>,
    partition: Vec<HashSet<usize>>,
) -> Result<DocumentCommunities> {
    check(original)?;
    // One compact lookup serves both partition integrity and density calculation.
    let mut membership = Vec::new();
    reserve(&mut membership, input.pages.len())?;
    membership.resize(input.pages.len(), None);
    for (group, nodes) in partition.iter().enumerate() {
        check(original)?;
        if nodes.is_empty() {
            return Err(DocumentCommunityError::InvalidPartition);
        }
        for &node in nodes {
            check(original)?;
            let slot = membership
                .get_mut(node)
                .ok_or(DocumentCommunityError::InvalidPartition)?;
            if slot.replace(group).is_some() {
                return Err(DocumentCommunityError::InvalidPartition);
            }
        }
    }
    if membership.iter().any(Option::is_none) {
        return Err(DocumentCommunityError::InvalidPartition);
    }
    let mut internal_pairs = Vec::new();
    reserve(&mut internal_pairs, partition.len())?;
    internal_pairs.resize(partition.len(), 0usize);
    for &(from, to) in &input.pairs {
        check(original)?;
        if membership[from] == membership[to] {
            let group = membership[from].ok_or(DocumentCommunityError::InvalidPartition)?;
            internal_pairs[group] += 1;
        }
    }
    let mut groups = Vec::new();
    for (group, nodes) in partition.into_iter().enumerate() {
        check(original)?;
        if nodes.len() == 1 {
            continue;
        }
        let mut ordered = Vec::new();
        reserve(&mut ordered, nodes.len())?;
        for node in nodes {
            check(original)?;
            ordered.push(node);
        }
        ordered.sort_unstable();
        check(original)?;
        let mut representative = ordered[0];
        let mut members = Vec::new();
        reserve(&mut members, ordered.len())?;
        for &node in &ordered {
            check(original)?;
            if input.degrees[node] > input.degrees[representative] {
                representative = node;
            }
            members.push(owned(input.pages[node].0)?);
        }
        let title = input.pages[representative].1;
        let label = owned(if title.is_empty() {
            input.pages[representative].0
        } else {
            title
        })?;
        let n = ordered.len() as f64;
        let possible = n * (n - 1.0) / 2.0;
        let cohesion = (internal_pairs[group] as f64 / possible * 100.0).round() / 100.0;
        reserve(&mut groups, 1)?;
        groups.push(DocumentCommunity {
            members,
            label,
            cohesion,
        });
    }
    groups.sort_unstable_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then_with(|| a.members.cmp(&b.members))
    });
    check(original)?;
    Ok(DocumentCommunities { groups })
}

/// Classify the complete existing graph without changing it or its identities.
///
/// The SDK call is synchronous and has no cancellation callback. The original
/// request is checked before and after it; cancellation discards the report.
/// SDK allocations and sorting do not promise hard preemption or an RSS ceiling.
pub fn classify_document_communities(
    original: &SnapshotContext,
    graph: &ReferenceGraph<'_, '_>,
) -> Result<DocumentCommunities> {
    let input = prepare(original, graph)?;
    check(original)?;
    let result = louvain_communities(&input.graph, false, None, None, Some(0));
    check(original)?;
    let partition = result.map_err(DocumentCommunityError::Library)?;
    finish(original, &input, partition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::references::{PageReferences, ReferenceLimits};
    use graphit_core::file_lock::Cancellation;
    use std::{
        num::NonZeroUsize,
        time::{Duration, Instant},
    };

    #[test]
    fn cancellation_after_actual_sdk_partition_discards_the_entire_report() {
        let cancel = Cancellation::default();
        let original =
            SnapshotContext::new(Instant::now() + Duration::from_secs(30), cancel.clone());
        let pages = [
            PageReferences {
                slug: "a",
                title: "A",
                targets: &["b"],
            },
            PageReferences {
                slug: "b",
                title: "B",
                targets: &[],
            },
        ];
        let graph = ReferenceGraph::build(
            &original,
            &pages,
            ReferenceLimits {
                max_identifier_bytes: NonZeroUsize::new(1).unwrap(),
            },
        )
        .unwrap();
        let input = prepare(&original, &graph).unwrap();
        let partition = louvain_communities(&input.graph, false, None, None, Some(0)).unwrap();
        assert_eq!(partition.len(), 1);
        cancel.cancel();
        let Err(DocumentCommunityError::Request(cause)) = finish(&original, &input, partition)
        else {
            panic!("post-SDK cancellation must reject the report");
        };
        assert_eq!(cause.kind(), io::ErrorKind::Interrupted);
    }
}
