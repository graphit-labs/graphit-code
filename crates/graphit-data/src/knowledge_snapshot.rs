//! One selected Knowledge document decoded from a caller-owned SDK pin.
use crate::{
    knowledge_index::{
        KnowledgeDocument, KnowledgeHead, KnowledgeHeadLookup, KnowledgeHeadReadLimits,
        KnowledgeIndexError, KnowledgeIndexReadLimits, NamespaceValidation, decode_batch_matching,
        decode_head_batch, validate_full_batch, validate_slug,
    },
    lance_module_store::{ModuleSnapshot, StoreError},
};
use arrow_array::{Array, RecordBatch, StringArray};
use futures::{Stream, TryStreamExt};
use graphit_core::{file_io::SnapshotContext, project_identity::ProjectId};
use lance::dataset::scanner::Scanner;
use std::{collections::BTreeSet, sync::Arc};

pub use crate::knowledge_search::search_knowledge_snapshot_guarded;

/// One selected document and its provenance after selected-stream EOF.
/// This grants no access and does not validate unrelated documents.
#[derive(Debug, Eq, PartialEq)]
pub enum KnowledgeSnapshotPage {
    PublishedEmpty,
    PageAbsent,
    Found {
        document: Box<KnowledgeDocument>,
        focused_source: Option<Box<crate::knowledge_reference_query::KnowledgeReferenceSource>>,
    },
}

/// Decode one exact slug from the provided snapshot through selected-stream EOF.
///
/// Only matching records are decoded. Duplicate selected identities are errors.
/// The caller retains the snapshot's actual version and its own authorization.
/// This function does not open a path, select another version, or authorize I/O.
pub async fn read_knowledge_snapshot(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    slug: &str,
    limits: KnowledgeIndexReadLimits,
) -> Result<KnowledgeSnapshotPage, KnowledgeIndexError> {
    read_knowledge_snapshot_guarded(original, snapshot, slug, limits, || Ok(())).await
}

/// Read a caller-owned pin with external authorization checks through row/EOF boundaries.
/// This does not grant access, open a path, or select another generation.
pub async fn read_knowledge_snapshot_guarded(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    slug: &str,
    limits: KnowledgeIndexReadLimits,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeSnapshotPage, KnowledgeIndexError> {
    read_page_guarded(
        original,
        snapshot.schema(),
        || snapshot.scan(),
        slug,
        limits,
        None,
        before,
    )
    .await
}

/// PROJECT and installed snapshots share the same selected query and decoder.
pub(crate) async fn read_page_guarded(
    original: &Arc<SnapshotContext>,
    schema: &arrow_schema::Schema,
    mut scan: impl FnMut() -> Scanner,
    slug: &str,
    limits: KnowledgeIndexReadLimits,
    expected_project: Option<ProjectId>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeSnapshotPage, KnowledgeIndexError> {
    original.check()?;
    before()?;
    validate_slug(slug)?;
    if slug.len() > limits.content.max_identifier_bytes.get() {
        return Err(KnowledgeIndexError::LimitExceeded("identifier_bytes"));
    }
    crate::knowledge_index::semantic::dimensions(schema)?;
    let mut before = || {
        original.check()?;
        before()?;
        Ok(())
    };
    let mut scanner = scan();
    scanner
        .project(&crate::knowledge_index::semantic::DOCUMENT_COLUMNS)
        .map_err(StoreError::from)?;
    scanner.filter_expr(
        lance::deps::datafusion::logical_expr::col("slug")
            .eq(lance::deps::datafusion::logical_expr::lit(slug)),
    );
    scanner.batch_size(limits.batch_rows.get() as usize);
    before()?;
    let mut stream = scanner.try_into_stream().await.map_err(StoreError::from)?;
    before()?;
    let mut page =
        collect_selected_bound(&mut stream, slug, limits, expected_project, &mut before).await?;
    drop(stream);
    drop(scanner);
    before()?;
    if matches!(page, KnowledgeSnapshotPage::PublishedEmpty) {
        // An empty selected stream is not an empty corpus. Inspect at most one
        // physical identity, without projecting or validating another body.
        let mut probe = scan();
        probe
            .project(&["slug"])
            .map_err(StoreError::from)?
            .limit(Some(1), None)
            .map_err(StoreError::from)?;
        before()?;
        let mut stream = probe.try_into_stream().await.map_err(StoreError::from)?;
        before()?;
        loop {
            before()?;
            let next = stream.try_next().await.map_err(StoreError::from)?;
            before()?;
            let Some(batch) = next else { break };
            if batch.num_rows() != 0 {
                page = KnowledgeSnapshotPage::PageAbsent;
                break;
            }
        }
        drop(stream);
        drop(probe);
        before()?;
    }
    Ok(page)
}

/// Scan head/witness metadata from one caller-owned pin, without projecting body.
/// The supplied guard runs before/after SDK awaits, on each row, EOF and delivery.
pub async fn read_knowledge_heads_snapshot_guarded(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    limits: KnowledgeHeadReadLimits,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<Vec<KnowledgeHead>, KnowledgeIndexError> {
    read_heads_selected(original, snapshot, limits, None, before).await
}

/// Resolve one authored source identity on the caller-owned snapshot. Only
/// head/witness columns are projected, with an SDK equality predicate.
pub async fn read_knowledge_head_by_source_path_snapshot_guarded(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    source_path: &str,
    limits: KnowledgeHeadReadLimits,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<Option<KnowledgeHead>, KnowledgeIndexError> {
    if source_path.is_empty()
        || source_path.contains('\0')
        || source_path.len() > limits.max_slug_bytes.get()
    {
        return Err(KnowledgeIndexError::InvalidField("source_path"));
    }
    let mut heads = read_heads_selected(
        original,
        snapshot,
        limits,
        Some(KnowledgeHeadLookup::SourcePath(source_path)),
        before,
    )
    .await?;
    if heads.len() > 1 {
        return Err(KnowledgeIndexError::InvalidField("ambiguous_source_path"));
    }
    Ok(heads.pop())
}

/// Select one literal destination slug on the existing snapshot, without its body.
pub async fn read_knowledge_head_by_slug_snapshot_guarded(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    slug: &str,
    limits: KnowledgeHeadReadLimits,
    before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<Option<KnowledgeHead>, KnowledgeIndexError> {
    validate_slug(slug)?;
    if slug.len() > limits.max_slug_bytes.get() {
        return Err(KnowledgeIndexError::LimitExceeded("slug_bytes"));
    }
    let mut heads = read_heads_selected(
        original,
        snapshot,
        limits,
        Some(KnowledgeHeadLookup::Slug(slug)),
        before,
    )
    .await?;
    if heads.len() > 1 {
        return Err(KnowledgeIndexError::DuplicateSlug);
    }
    Ok(heads.pop())
}

async fn read_heads_selected(
    original: &Arc<SnapshotContext>,
    snapshot: &ModuleSnapshot,
    limits: KnowledgeHeadReadLimits,
    selection: Option<KnowledgeHeadLookup<'_>>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<Vec<KnowledgeHead>, KnowledgeIndexError> {
    original.check()?;
    before()?;
    crate::knowledge_index::semantic::dimensions(snapshot.schema())?;
    let mut before = || {
        original.check()?;
        before()?;
        Ok(())
    };
    before()?;
    let mut scanner = snapshot.scan();
    if let Some(selection) = selection {
        let (column, value) = match selection {
            KnowledgeHeadLookup::Slug(value) => ("slug", value),
            KnowledgeHeadLookup::SourcePath(value) => ("source_path", value),
        };
        scanner.filter_expr(
            lance::deps::datafusion::logical_expr::col(column)
                .eq(lance::deps::datafusion::logical_expr::lit(value)),
        );
    }
    scanner
        .project(&[
            "slug",
            "title",
            "updated_at",
            "source_path",
            "compilation_digest",
            "resolved_wiki_targets",
            "document_state",
            "reference_projection",
        ])
        .map_err(StoreError::from)?
        .batch_size(limits.batch_rows.get() as usize);
    before()?;
    let mut stream = scanner.try_into_stream().await.map_err(StoreError::from)?;
    before()?;
    let mut counts = NamespaceValidation::bound(None);
    let mut seen = BTreeSet::new();
    let mut heads = Vec::new();
    loop {
        before()?;
        let next = stream.try_next().await.map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        if batch.num_rows() > limits.batch_rows.get() as usize {
            return Err(KnowledgeIndexError::LimitExceeded("batch_rows"));
        }
        for row in 0..batch.num_rows() {
            before()?;
            let one = batch.slice(row, 1);
            if let Some(selection) = selection {
                let (column, value) = match selection {
                    KnowledgeHeadLookup::Slug(value) => (0, value),
                    KnowledgeHeadLookup::SourcePath(value) => (3, value),
                };
                let origins = one
                    .column(column)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or(KnowledgeIndexError::InvalidField("array_type"))?;
                if origins.is_null(0) || origins.value(0) != value {
                    return Err(KnowledgeIndexError::InvalidField("selected_membership"));
                }
            }
            decode_head_batch(&one, limits, &mut counts, &mut heads, &mut before)?;
            let slugs = one
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or(KnowledgeIndexError::InvalidField("array_type"))?;
            if !seen.insert(slugs.value(0).to_owned()) {
                return Err(KnowledgeIndexError::DuplicateSlug);
            }
        }
    }
    drop(stream);
    drop(scanner);
    before()?;
    heads.sort_unstable_by(|a, b| a.slug.cmp(&b.slug));
    before()?;
    Ok(heads)
}

pub(crate) async fn collect_selected_bound<S, E>(
    stream: &mut S,
    slug: &str,
    limits: KnowledgeIndexReadLimits,
    expected_project: Option<ProjectId>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<KnowledgeSnapshotPage, KnowledgeIndexError>
where
    S: Stream<Item = Result<RecordBatch, E>> + Unpin,
    StoreError: From<E>,
{
    let mut seen = false;
    let mut page = KnowledgeSnapshotPage::PublishedEmpty;
    loop {
        before()?;
        let next = stream.try_next().await.map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        if batch.num_rows() > limits.batch_rows.get() as usize {
            return Err(KnowledgeIndexError::LimitExceeded("batch_rows"));
        }
        for row in 0..batch.num_rows() {
            before()?;
            let one = batch.slice(row, 1);
            let identities = one
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or(KnowledgeIndexError::InvalidField("array_type"))?;
            if identities.is_null(0) || identities.value(0) != slug {
                return Err(KnowledgeIndexError::InvalidField("selected_membership"));
            }
            if seen {
                return Err(KnowledgeIndexError::DuplicateSlug);
            }
            seen = true;
            let mut documents = Vec::new();
            decode_batch_matching(
                &one,
                limits.content,
                &mut NamespaceValidation::bound(expected_project),
                &mut documents,
                Some(slug),
                &mut before,
            )?;
            page = match documents.pop() {
                Some(document) => {
                    let source = crate::knowledge_search::source(
                        &one,
                        expected_project,
                        limits,
                        &mut before,
                    )?;
                    let focused_source = source.namespace.is_some().then(|| Box::new(source));
                    KnowledgeSnapshotPage::Found {
                        document: Box::new(document),
                        focused_source,
                    }
                }
                None => KnowledgeSnapshotPage::PageAbsent,
            };
        }
    }
    before()?;
    Ok(page)
}

/// One EOF collector for singleton pages and bounded selected-document reads.
/// Every row is validated; only matching Active documents become owned DTOs.
pub(crate) async fn collect_matching_bound<S, E>(
    original: &SnapshotContext,
    stream: &mut S,
    wanted: impl Fn(&str) -> bool,
    limits: KnowledgeIndexReadLimits,
    expected_project: Option<ProjectId>,
    mut before: impl FnMut() -> Result<(), KnowledgeIndexError>,
) -> Result<(usize, Vec<KnowledgeDocument>), KnowledgeIndexError>
where
    S: Stream<Item = Result<RecordBatch, E>> + Unpin,
    StoreError: From<E>,
{
    let deadline = async {
        match original.deadline() {
            Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::pin!(deadline);
    let mut slugs = BTreeSet::new();
    let mut selected = Vec::new();
    loop {
        before()?;
        let next = tokio::select! {
            biased;
            () = original.cancellation().cancelled() => return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into()),
            () = &mut deadline => return Err(std::io::Error::from(std::io::ErrorKind::TimedOut).into()),
            next = stream.try_next() => next,
        }.map_err(StoreError::from)?;
        before()?;
        let Some(batch) = next else { break };
        if batch.num_rows() > limits.batch_rows.get() as usize {
            return Err(KnowledgeIndexError::LimitExceeded("batch_rows"));
        }
        for row in 0..batch.num_rows() {
            original.check()?;
            let one = batch.slice(row, 1);
            let identities = one
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or(KnowledgeIndexError::InvalidField("array_type"))?;
            if !identities.is_null(0) && wanted(identities.value(0)) {
                // The existing decoder is singleton-oriented. A row-local
                // destination preserves its duplicate contract without a
                // second decoder or retaining unselected bodies.
                let mut document = Vec::new();
                decode_batch_matching(
                    &one,
                    limits.content,
                    &mut NamespaceValidation::bound(expected_project),
                    &mut document,
                    Some(identities.value(0)),
                    || original.check().map_err(KnowledgeIndexError::from),
                )?;
                selected.append(&mut document);
            } else {
                validate_full_batch(
                    &one,
                    limits.content,
                    &mut NamespaceValidation::bound(expected_project),
                    || original.check().map_err(KnowledgeIndexError::from),
                )?;
            }
            let identity = identities.value(0);
            if !slugs.insert(identity.to_owned()) {
                return Err(KnowledgeIndexError::DuplicateSlug);
            }
        }
    }
    before()?;
    Ok((slugs.len(), selected))
}
