//! Raw AST source lookup within an already acquired Lance snapshot.
//!
//! The caller selects and authorizes the corpus. This module neither opens a
//! locator nor grants authority. An owned result retains the observed Dataset,
//! including any reader lease already held by that snapshot. A bare SDK capture
//! without a coordinated reader lease is not protected from external cleanup.

use arrow_array::{Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::{Stream, TryStreamExt};
use graphit_core::file_io::SnapshotContext;
use graphit_data::{lance_branch_store::BranchSnapshot, lance_module_store::ModuleSnapshot};
use lance::dataset::scanner::Scanner;
use lance::deps::datafusion::logical_expr::{col, lit};
use std::{
    collections::{HashMap, HashSet, TryReserveError},
    error::Error,
    fmt, io,
    sync::Arc,
};

use crate::source_inspection::IndexedSource;

/// Complete schema of the new raw-source corpus; no legacy search document.
pub fn source_schema() -> Arc<Schema> {
    Arc::new(Schema::new_with_metadata(
        vec![
            Field::new("path", DataType::Utf8, false),
            Field::new("source", DataType::Utf8, true),
        ],
        HashMap::from([
            ("graphit.module".into(), "ast-source".into()),
            ("graphit.ast.source.format".into(), "1".into()),
        ]),
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingSourceReason {
    FileAbsent,
    TextNotStored,
}

enum StoredSource {
    Available(String),
    Missing(MissingSourceReason),
}

#[derive(Clone)]
enum SourceSnapshot {
    Module(ModuleSnapshot),
    Branch(BranchSnapshot),
}

impl SourceSnapshot {
    fn version(&self) -> u64 {
        match self {
            Self::Module(snapshot) => snapshot.version(),
            Self::Branch(snapshot) => snapshot.version(),
        }
    }

    fn schema(&self) -> &Schema {
        match self {
            Self::Module(snapshot) => snapshot.schema(),
            Self::Branch(snapshot) => snapshot.schema(),
        }
    }

    fn scan(&self) -> Scanner {
        match self {
            Self::Module(snapshot) => snapshot.scan(),
            Self::Branch(snapshot) => snapshot.scan(),
        }
    }
}

/// Observed SDK branch identity, not authorization or a GC pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BranchSourceObservation<'a> {
    pub physical_branch: Option<&'a str>,
    pub inherited_target: Option<&'a str>,
}

/// Payload and original storage generation, not a scope or authorization grant.
pub struct IndexedSourceSnapshot {
    snapshot: SourceSnapshot,
    original: Arc<SnapshotContext>,
    path: String,
    source: StoredSource,
}

impl IndexedSourceSnapshot {
    pub fn version(&self) -> u64 {
        self.snapshot.version()
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// Module snapshots do not expose branch observations. Default is instead
    /// `Some` with both fields absent; a missing observation is not Default.
    pub fn branch_observation(&self) -> Option<BranchSourceObservation<'_>> {
        match &self.snapshot {
            SourceSnapshot::Module(_) => None,
            SourceSnapshot::Branch(snapshot) => Some(BranchSourceObservation {
                physical_branch: snapshot.physical_branch(),
                inherited_target: snapshot.inherited_target(),
            }),
        }
    }

    pub fn missing_reason(&self) -> Option<MissingSourceReason> {
        match self.source {
            StoredSource::Available(_) => None,
            StoredSource::Missing(reason) => Some(reason),
        }
    }

    /// Borrow the raw text while retaining the Dataset and original request.
    pub fn source(
        &self,
        original: &Arc<SnapshotContext>,
    ) -> Result<IndexedSource<'_>, IndexedSourceError> {
        if !Arc::ptr_eq(&self.original, original) {
            return Err(IndexedSourceError::RequestMismatch);
        }
        original.check().map_err(IndexedSourceError::Operation)?;
        Ok(match &self.source {
            StoredSource::Available(text) => IndexedSource::Available(text),
            StoredSource::Missing(_) => IndexedSource::Missing,
        })
    }
}

#[derive(Debug)]
pub enum IndexedSourceError {
    InvalidPath,
    SchemaMismatch,
    DuplicatePath,
    InvalidRow,
    RequestMismatch,
    Operation(io::Error),
    Allocation(TryReserveError),
    Sdk(Box<lance::Error>),
}

impl fmt::Display for IndexedSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => formatter.write_str("source path is empty or contains NUL"),
            Self::SchemaMismatch => formatter.write_str("raw AST source schema mismatch"),
            Self::DuplicatePath => formatter.write_str("duplicate indexed source path"),
            Self::InvalidRow => formatter.write_str("invalid indexed source row"),
            Self::RequestMismatch => formatter.write_str("indexed source request mismatch"),
            Self::Operation(error) => write!(formatter, "indexed source operation: {error}"),
            Self::Allocation(error) => write!(formatter, "indexed source allocation: {error}"),
            Self::Sdk(error) => write!(formatter, "indexed source storage: {error}"),
        }
    }
}

impl Error for IndexedSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Operation(error) => Some(error),
            Self::Allocation(error) => Some(error),
            Self::Sdk(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

fn sdk(error: lance::Error) -> IndexedSourceError {
    IndexedSourceError::Sdk(Box::new(error))
}

fn columns(batch: &RecordBatch) -> Result<(&StringArray, &StringArray), IndexedSourceError> {
    if batch.num_columns() != 2
        || batch.schema().field(0).name() != "path"
        || batch.schema().field(1).name() != "source"
    {
        return Err(IndexedSourceError::InvalidRow);
    }
    let paths = batch.column(0).as_any().downcast_ref::<StringArray>();
    let sources = batch.column(1).as_any().downcast_ref::<StringArray>();
    match (paths, sources) {
        (Some(paths), Some(sources)) => Ok((paths, sources)),
        _ => Err(IndexedSourceError::InvalidRow),
    }
}

/// Query an exact corpus key without parsing or normalizing it.
///
/// A zero-row result and a NULL source are explicit misses with different
/// reasons. An empty string is available. Storage errors and duplicates never
/// become misses. Success consumes the query to EOF, so it cannot hide a late
/// stream failure after one matching row.
///
/// Request checks surround SDK awaits and row decoding; they are cooperative,
/// not hard preemption of SDK work or allocations. Scope and admission
/// revalidation before delivery remain the caller's responsibility.
pub async fn read_indexed_source(
    snapshot: &ModuleSnapshot,
    path: &str,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, IndexedSourceError> {
    read_source(SourceSnapshot::Module(snapshot.clone()), path, original).await
}

/// Read the exact already captured branch generation without resolving latest
/// or materializing an inherited target. Uses the same strict raw decoder as
/// module snapshots, with the same original request and error semantics.
pub async fn read_branch_source(
    snapshot: &BranchSnapshot,
    path: &str,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, IndexedSourceError> {
    read_source(SourceSnapshot::Branch(snapshot.clone()), path, original).await
}

/// Enumerate literal keys in the retained Module generation, without source bodies.
///
/// The caller retains and authorizes the actual snapshot through delivery. NULL
/// source values do not remove a key. Success requires the complete stream's EOF.
pub async fn indexed_source_paths(
    snapshot: &ModuleSnapshot,
    original: &Arc<SnapshotContext>,
) -> Result<Vec<String>, IndexedSourceError> {
    source_paths(SourceSnapshot::Module(snapshot.clone()), original).await
}

/// Enumerate the already captured branch, without opening latest or materializing.
pub async fn branch_source_paths(
    snapshot: &BranchSnapshot,
    original: &Arc<SnapshotContext>,
) -> Result<Vec<String>, IndexedSourceError> {
    source_paths(SourceSnapshot::Branch(snapshot.clone()), original).await
}

async fn source_paths(
    snapshot: SourceSnapshot,
    original: &Arc<SnapshotContext>,
) -> Result<Vec<String>, IndexedSourceError> {
    original.check().map_err(IndexedSourceError::Operation)?;
    let result = async {
        if snapshot.schema() != source_schema().as_ref() {
            return Err(IndexedSourceError::SchemaMismatch);
        }
        let mut scanner = snapshot.scan();
        scanner.project(&["path"]).map_err(sdk)?;
        original.check().map_err(IndexedSourceError::Operation)?;
        let stream = scanner.try_into_stream().await;
        original.check().map_err(IndexedSourceError::Operation)?;
        collect_paths(stream.map_err(sdk)?, original).await
    }
    .await;
    original.check().map_err(IndexedSourceError::Operation)?;
    result
}

async fn collect_paths<S>(
    mut stream: S,
    original: &SnapshotContext,
) -> Result<Vec<String>, IndexedSourceError>
where
    S: Stream<Item = Result<RecordBatch, lance::Error>> + Unpin,
{
    let mut paths = HashSet::new();
    let path_field = source_schema().field(0).clone();
    loop {
        original.check().map_err(IndexedSourceError::Operation)?;
        let next = stream.try_next().await;
        original.check().map_err(IndexedSourceError::Operation)?;
        let Some(batch) = next.map_err(sdk)? else {
            break;
        };
        let schema = batch.schema();
        if schema.fields().len() != 1 || schema.field(0) != &path_field {
            return Err(IndexedSourceError::InvalidRow);
        }
        let values = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(IndexedSourceError::InvalidRow)?;
        for row in 0..batch.num_rows() {
            original.check().map_err(IndexedSourceError::Operation)?;
            if values.is_null(row) {
                return Err(IndexedSourceError::InvalidRow);
            }
            let value = values.value(row);
            if value.is_empty() || value.contains('\0') {
                return Err(IndexedSourceError::InvalidPath);
            }
            if paths.contains(value) {
                return Err(IndexedSourceError::DuplicatePath);
            }
            paths
                .try_reserve(1)
                .map_err(IndexedSourceError::Allocation)?;
            let mut path = String::new();
            path.try_reserve_exact(value.len())
                .map_err(IndexedSourceError::Allocation)?;
            path.push_str(value);
            paths.insert(path);
        }
    }
    original.check().map_err(IndexedSourceError::Operation)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(paths.len())
        .map_err(IndexedSourceError::Allocation)?;
    result.extend(paths);
    result.sort_unstable();
    original.check().map_err(IndexedSourceError::Operation)?;
    Ok(result)
}

async fn read_source(
    snapshot: SourceSnapshot,
    path: &str,
    original: &Arc<SnapshotContext>,
) -> Result<IndexedSourceSnapshot, IndexedSourceError> {
    original.check().map_err(IndexedSourceError::Operation)?;
    if path.is_empty() || path.contains('\0') {
        return Err(IndexedSourceError::InvalidPath);
    }
    if snapshot.schema() != source_schema().as_ref() {
        return Err(IndexedSourceError::SchemaMismatch);
    }
    let mut scanner = snapshot.scan();
    scanner.filter_expr(col("path").eq(lit(path)));
    scanner.project(&["path", "source"]).map_err(sdk)?;
    scanner.limit(Some(2), None).map_err(sdk)?;
    original.check().map_err(IndexedSourceError::Operation)?;
    let stream = scanner.try_into_stream().await;
    original.check().map_err(IndexedSourceError::Operation)?;
    let mut stream = stream.map_err(sdk)?;
    let mut found = None;
    loop {
        original.check().map_err(IndexedSourceError::Operation)?;
        let next = stream.try_next().await;
        original.check().map_err(IndexedSourceError::Operation)?;
        let Some(batch) = next.map_err(sdk)? else {
            break;
        };
        let (paths, sources) = columns(&batch)?;
        for row in 0..batch.num_rows() {
            original.check().map_err(IndexedSourceError::Operation)?;
            if paths.is_null(row) || paths.value(row) != path {
                return Err(IndexedSourceError::InvalidRow);
            }
            if found.is_some() {
                return Err(IndexedSourceError::DuplicatePath);
            }
            found = Some(if sources.is_null(row) {
                StoredSource::Missing(MissingSourceReason::TextNotStored)
            } else {
                StoredSource::Available(sources.value(row).to_owned())
            });
        }
    }
    original.check().map_err(IndexedSourceError::Operation)?;
    Ok(IndexedSourceSnapshot {
        snapshot,
        original: Arc::clone(original),
        path: path.to_owned(),
        source: found.unwrap_or(StoredSource::Missing(MissingSourceReason::FileAbsent)),
    })
}

#[cfg(test)]
mod inventory_stream_tests {
    use super::*;
    use futures::stream;
    use std::time::{Duration, Instant};

    // Collector boundary, not a claim of a native late read race.
    #[tokio::test]
    async fn inventory_consumes_a_tail_error_instead_of_returning_first_batch() {
        let original = SnapshotContext::new(
            Instant::now() + Duration::from_secs(60),
            graphit_core::file_lock::Cancellation::default(),
        );
        let temp = tempfile::TempDir::new().unwrap();
        let missing = temp.path().join("missing.lance");
        let failure = lance::Dataset::open(missing.to_str().unwrap())
            .await
            .err()
            .unwrap();
        let expected = failure.to_string();
        let schema = Arc::new(Schema::new(vec![source_schema().field(0).clone()]));
        let batch =
            RecordBatch::try_new(schema, vec![Arc::new(StringArray::from(vec!["first"]))]).unwrap();
        let error = collect_paths(stream::iter(vec![Ok(batch), Err(failure)]), &original)
            .await
            .unwrap_err();
        assert!(error.source().is_some());
        match error {
            IndexedSourceError::Sdk(cause) => assert_eq!(cause.to_string(), expected),
            other => panic!("actual tail cause required: {other:?}"),
        }
    }
}
