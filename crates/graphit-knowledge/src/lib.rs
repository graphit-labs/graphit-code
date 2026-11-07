//! Document compilation and inspection contracts for Graphit.
//!
//! Resolved references operate on a complete borrowed corpus. Storage, parsing,
//! source authorization and model inference belong to their respective producers.
//!
//! ```
//! use graphit_core::{file_io::SnapshotContext, file_lock::Cancellation};
//! use graphit_knowledge::references::{PageReferences, ReferenceGraph, ReferenceLimits};
//! use std::{num::NonZeroUsize, time::{Duration, Instant}};
//!
//! let context = SnapshotContext::new(Instant::now() + Duration::from_secs(30), Cancellation::default());
//! let limit = NonZeroUsize::new(100).unwrap();
//! let pages = [PageReferences { slug: "guide", title: "Guide", targets: &["missing"] }];
//! let graph = ReferenceGraph::build(&context, &pages, ReferenceLimits {
//!     max_identifier_bytes: limit,
//! })?;
//! assert_eq!(graph.broken_links(&context, limit)?[0].target, "missing");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod authored_first_publication;
pub mod branch_materialization;
pub mod compilation_fingerprint;
pub mod desired_corpus;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod desired_reconciliation;
pub mod document;
pub mod document_authored_references;
pub mod document_communities;
pub mod document_republication;
pub mod document_source;
pub mod embedding_projection;
pub mod ignore_policy;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod imported_knowledge;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod imported_reference_query;
pub mod markdown_export;
pub mod native_document_mapping;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod native_index_publication;
pub mod native_reference_query;
pub mod navigation;
pub mod okf_export;
pub mod preview;
pub mod publication_metadata;
pub mod published_lint;
pub mod published_metadata_lint;
pub mod references;
pub mod settings;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod source_stream;
pub mod staleness;
pub mod staleness_projection;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod search;

#[cfg(test)]
use crate as knowledge;
#[cfg(test)]
#[allow(
    dead_code,
    reason = "shared setup also serves separate integration test binaries"
)]
#[path = "../tests/support/desired_reconciliation_fixture.rs"]
mod desired_reconciliation_fixture;
