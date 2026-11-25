//! Icebug sampling over an admitted SDK object store and immutable release prefix.
//! Object metadata and generation identify data; the caller owns authorization.
mod neighborhood;
mod parquet_reader;
use crate::{
    DataReadError, EdgeId, GraphEdgeSummary, GraphNodeSummary, GraphProperty, GraphSample,
    GraphSampleLimits, LocalGraphPolicy, NodeId, PrimaryKey, admit_node, bounded,
    manifest::Manifest,
    parquet::{Scalar, Shape, Work},
};
use arrow_array_parquet::{Array, Int64Array, RecordBatch, StringArray, UInt64Array};
use futures::{StreamExt, future::Future};
use graphit_core::file_io::SnapshotContext;
use object_store::{ObjectStore, path::Path};
use parquet::arrow::arrow_reader::RowSelection;
use parquet_reader::{Object, Table};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    error::Error,
    fmt, io,
    sync::Arc,
};

type Result<T> = std::result::Result<T, RemoteGraphError>;

#[derive(Debug)]
pub enum RemoteGraphError {
    Data(DataReadError),
    ObjectStore(object_store::Error),
    Parquet(parquet::errors::ParquetError),
    Original(io::Error),
    ManifestDigest,
}
impl fmt::Display for RemoteGraphError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => write!(out, "remote Icebug: {error}"),
            Self::ObjectStore(error) => write!(out, "remote Icebug object: {error}"),
            Self::Parquet(error) => write!(out, "remote Icebug Parquet: {error}"),
            Self::Original(error) => write!(out, "remote Icebug request: {error}"),
            Self::ManifestDigest => out.write_str("remote Icebug manifest digest mismatch"),
        }
    }
}
impl Error for RemoteGraphError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Data(error) => Some(error),
            Self::ObjectStore(error) => Some(error),
            Self::Parquet(error) => Some(error),
            Self::Original(error) => Some(error),
            Self::ManifestDigest => None,
        }
    }
}
macro_rules! from_error {
    ($type:ty, $variant:ident) => {
        impl From<$type> for RemoteGraphError {
            fn from(error: $type) -> Self {
                Self::$variant(error)
            }
        }
    };
}
from_error!(DataReadError, Data);
from_error!(object_store::Error, ObjectStore);
from_error!(parquet::errors::ParquetError, Parquet);
from_error!(io::Error, Original);

async fn wait<T>(original: &SnapshotContext, future: impl Future<Output = Result<T>>) -> Result<T> {
    original.check()?;
    let result = tokio::select! {
        biased;
        () = original.cancellation().cancelled() => Err(io::Error::from(io::ErrorKind::Interrupted).into()),
        result = async {
            match original.deadline() {
                Some(deadline) => tokio::time::timeout_at(deadline.into(), future).await
                    .unwrap_or_else(|_| Err(io::Error::from(io::ErrorKind::TimedOut).into())),
                None => future.await,
            }
        } => result,
    };
    original.check()?;
    result
}

struct Member {
    from: usize,
    to: usize,
    relationship: String,
    indices_path: String,
    indptr_path: String,
    indices: Table,
    indptr: Table,
    source_row_base: u64,
    source_rows: u64,
    target_row_base: u64,
    target_rows: u64,
}
/// Owns only immutable manifest/metadata and SDK store handles, not a request.
pub struct RemoteIcebugGraph {
    manifest: Manifest,
    nodes: Vec<Table>,
    members: Vec<Member>,
    reverse_members: Vec<Member>,
    policy: LocalGraphPolicy,
}
/// An immutable object metadata pin; callers supply the live request on every read.
pub struct RemoteGraphFile(Arc<Object>);
impl RemoteGraphFile {
    pub fn len(&self) -> u64 {
        self.0.size()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub async fn read_range(
        &self,
        offset: u64,
        length: usize,
        original: &SnapshotContext,
    ) -> Result<bytes::Bytes> {
        let end = offset
            .checked_add(length as u64)
            .ok_or(DataReadError::InvalidParquet("object range"))?;
        self.0.range(offset..end, original).await
    }
}
impl RemoteIcebugGraph {
    pub async fn open(
        store: Arc<dyn ObjectStore>,
        prefix: Path,
        expected_manifest_digest: [u8; 32],
        policy: LocalGraphPolicy,
        original: Arc<SnapshotContext>,
    ) -> Result<Self> {
        original.check()?;
        let object = Object::open(
            Arc::clone(&store),
            prefix.clone().join("icebug.json"),
            &original,
        )
        .await?;
        let length = usize::try_from(object.size())
            .map_err(|_| DataReadError::LimitExceeded("manifest bytes"))?;
        bounded(length, policy.0.manifest_bytes, "manifest bytes")?;
        let bytes = object.range(0..object.size(), &original).await?;
        if <[u8; 32]>::from(Sha256::digest(&bytes)) != expected_manifest_digest {
            return Err(RemoteGraphError::ManifestDigest);
        }
        let manifest = Manifest::parse(&bytes, policy.0)?;
        let mut captured = BTreeMap::new();
        let mut nodes = Vec::new();
        for node in &manifest.node_tables {
            let table = Table::open_nodes(
                Arc::clone(&store),
                &prefix,
                node,
                policy.0,
                &original,
                &mut captured,
            )
            .await?;
            for (name, kind) in &table.fields {
                if matches!(name.as_str(), "name" | "path" | "uid" | "cluster" | "lang")
                    && *kind != Scalar::String
                    || name == "line_number" && *kind != Scalar::Int64
                {
                    return Err(DataReadError::InvalidParquet("summary column type").into());
                }
            }
            nodes.push(table);
        }
        let mut descriptors = manifest
            .relationships
            .iter()
            .flat_map(|group| group.members.iter().map(move |member| (group, member)))
            .collect::<Vec<_>>();
        descriptors.sort_unstable_by(|(_, a), (_, b)| a.table.cmp(&b.table));
        let mut members = Vec::new();
        for (group, member) in descriptors {
            let find = |label: &str| {
                manifest
                    .node_tables
                    .iter()
                    .position(|node| node.label == label)
                    .ok_or(DataReadError::InvalidManifest("member label"))
            };
            let from = find(&member.from)?;
            let to = find(&member.to)?;
            let indices = Table::open(
                Arc::clone(&store),
                object_path(&prefix, &member.indices),
                member.indices_row_offset,
                member.rows,
                Shape::Indices,
                policy.0,
                &original,
                &mut captured,
            )
            .await?;
            let indptr = Table::open(
                Arc::clone(&store),
                object_path(&prefix, &member.indptr),
                member.indptr_row_offset,
                member
                    .source_rows
                    .checked_add(1)
                    .ok_or(DataReadError::InvalidManifest("pointer rows"))?,
                Shape::Indptr,
                policy.0,
                &original,
                &mut captured,
            )
            .await?;
            members.push(Member {
                from,
                to,
                relationship: group.kind.clone(),
                indices_path: member.indices.clone(),
                indptr_path: member.indptr.clone(),
                indices,
                indptr,
                source_row_base: member.source_row_base,
                source_rows: member.source_rows,
                target_row_base: member.target_row_base,
                target_rows: member.target_rows,
            });
        }
        // Retain reverse CSR for incoming adjacency; it adds no sample edges.
        let mut reverse_members = Vec::new();
        for group in &manifest.relationships {
            for member in &group.reverse_members {
                let from = manifest
                    .node_tables
                    .iter()
                    .position(|node| node.label == member.from)
                    .ok_or(DataReadError::InvalidManifest("from label"))?;
                let to = manifest
                    .node_tables
                    .iter()
                    .position(|node| node.label == member.to)
                    .ok_or(DataReadError::InvalidManifest("to label"))?;
                let rows = member
                    .source_rows
                    .checked_add(1)
                    .ok_or(DataReadError::InvalidManifest("pointer rows"))?;
                let indices = Table::open(
                    Arc::clone(&store),
                    object_path(&prefix, &member.indices),
                    member.indices_row_offset,
                    member.rows,
                    Shape::Indices,
                    policy.0,
                    &original,
                    &mut captured,
                )
                .await?;
                let indptr = Table::open(
                    Arc::clone(&store),
                    object_path(&prefix, &member.indptr),
                    member.indptr_row_offset,
                    rows,
                    Shape::Indptr,
                    policy.0,
                    &original,
                    &mut captured,
                )
                .await?;
                reverse_members.push(Member {
                    from,
                    to,
                    relationship: group.kind.clone(),
                    indices_path: member.indices.clone(),
                    indptr_path: member.indptr.clone(),
                    indices,
                    indptr,
                    source_row_base: member.source_row_base,
                    source_rows: member.source_rows,
                    target_row_base: member.target_row_base,
                    target_rows: member.target_rows,
                });
            }
        }
        original.check()?;
        Ok(Self {
            manifest,
            nodes,
            members,
            reverse_members,
            policy,
        })
    }
    /// Immutable metadata only; composing consumers retain their live authorization.
    pub fn schema(&self) -> crate::GraphSchema<'_> {
        crate::GraphSchema(crate::GraphSchemaSource::Remote(self))
    }
    pub(super) fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub(super) fn relationship_schemas(
        &self,
    ) -> impl ExactSizeIterator<Item = crate::GraphRelationshipSchema<'_>> {
        self.members
            .iter()
            .map(|member| crate::GraphRelationshipSchema {
                indices: &member.indices_path,
                indptr: &member.indptr_path,
                indices_row_offset: member.indices.row_offset,
                indptr_row_offset: member.indptr.row_offset,
                kind: &member.relationship,
                from: &self.manifest.node_tables[member.from].label,
                to: &self.manifest.node_tables[member.to].label,
                rows: member.indices.rows,
                fields: &member.indices.fields,
                source_row_base: member.source_row_base,
                source_rows: member.source_rows,
                target_row_base: member.target_row_base,
                target_rows: member.target_rows,
            })
    }
    /// Open only a table already captured and validated by this graph. Query engines
    /// receive range handles rather than downloading the graph into local files.
    pub fn open_query_file(&self, relative: &str) -> Result<RemoteGraphFile> {
        for (description, table) in self.manifest.node_tables.iter().zip(&self.nodes) {
            if let Some(index) = description
                .shards
                .iter()
                .position(|shard| shard.file == relative)
            {
                return Ok(RemoteGraphFile(table.shard_object(index)?));
            }
        }
        for member in &self.members {
            if member.indices_path == relative {
                return Ok(RemoteGraphFile(Arc::clone(&member.indices.object)));
            }
            if member.indptr_path == relative {
                return Ok(RemoteGraphFile(Arc::clone(&member.indptr.object)));
            }
        }
        Err(DataReadError::InvalidManifest("uncaptured query file").into())
    }
    fn node_id(&self, table: usize, row: u64) -> NodeId {
        NodeId {
            table: table as u32,
            row,
        }
    }
    pub async fn sample_graph(
        &self,
        limits: GraphSampleLimits,
        original: Arc<SnapshotContext>,
    ) -> Result<GraphSample> {
        original.check()?;
        let work = Work::new(limits.into());
        let mut selected = HashSet::new();
        let mut seeds = limits.seed_nodes;
        for (table, data) in self.nodes.iter().enumerate() {
            let count = data.rows.min(seeds as u64);
            for row in 0..count {
                admit_node(
                    &mut selected,
                    self.node_id(table, data.offset(row)?),
                    &limits,
                )?;
            }
            seeds -= count as usize;
        }
        let mut edges = Vec::new();
        let mut remaining = limits.global_edges;
        for (index, member) in self.members.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            let count = member.indices.rows.min(remaining as u64) as usize;
            if count == 0 {
                continue;
            }
            let edge_selection = RowSelection::from_consecutive_ranges(
                std::iter::once(0..count),
                usize::try_from(member.indices.rows)
                    .map_err(|_| DataReadError::LimitExceeded("row selection"))?,
            );
            let start = edges.len();
            let mut stream = member.indices.stream(
                &["target", "uid"],
                Some(edge_selection),
                self.policy.0,
                limits.row_batch_size,
                Arc::clone(&work),
                Arc::clone(&original),
            )?;
            let mut row = 0;
            while let Some(batch) = wait(&original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for offset in 0..batch.num_rows() {
                    let local_target = uint(&batch, "target", offset)?;
                    if local_target >= member.target_rows {
                        return Err(DataReadError::InvalidCsr("target shard bounds").into());
                    }
                    let target = member.target_row_base + local_target;
                    if !self.nodes[member.to].contains(target) {
                        return Err(DataReadError::InvalidCsr("target bounds").into());
                    }
                    let uid = text(&batch, "uid", offset)?
                        .filter(|uid| !uid.is_empty())
                        .ok_or(DataReadError::InvalidCsr("edge UID"))?;
                    bounded(uid.len(), limits.max_string_bytes, "edge UID")?;
                    edges.push(GraphEdgeSummary {
                        id: EdgeId {
                            member: index as u32,
                            row,
                        },
                        from: self.node_id(member.from, 0),
                        to: self.node_id(member.to, target),
                        relationship: limits.text(&member.relationship)?,
                        uid: limits.text(uid)?,
                    });
                    row += 1;
                }
                original.check()?;
            }
            if row != count as u64 {
                return Err(DataReadError::InvalidCsr("edge row count").into());
            }
            let mut pointers = member.indptr.stream(
                &["ptr"],
                None,
                self.policy.0,
                limits.row_batch_size,
                Arc::clone(&work),
                Arc::clone(&original),
            )?;
            let mut source = 0;
            let mut previous = 0;
            let mut next = start;
            'pointers: while let Some(batch) = wait(&original, async {
                pointers.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for offset in 0..batch.num_rows() {
                    let pointer = uint(&batch, "ptr", offset)?;
                    if source == 0 && pointer != 0
                        || pointer < previous
                        || pointer > member.indices.rows
                    {
                        return Err(DataReadError::InvalidCsr("row pointer").into());
                    }
                    if source > 0 {
                        while next < start + count && edges[next].id.row < pointer {
                            edges[next].from =
                                self.node_id(member.from, member.source_row_base + source - 1);
                            next += 1;
                        }
                    }
                    previous = pointer;
                    source += 1;
                    if next == start + count && count as u64 != member.indices.rows {
                        break 'pointers;
                    }
                }
                original.check()?;
            }
            if next != start + count
                || count as u64 == member.indices.rows
                    && (previous != member.indices.rows || source != member.indptr.rows)
            {
                return Err(DataReadError::InvalidCsr("final pointer").into());
            }
            for edge in &edges[start..] {
                admit_node(&mut selected, edge.from, &limits)?;
                admit_node(&mut selected, edge.to, &limits)?;
            }
            remaining -= count;
        }
        let mut ids = selected.into_iter().collect::<Vec<_>>();
        ids.sort_unstable();
        let mut nodes = Vec::new();
        for (index, table) in self.nodes.iter().enumerate() {
            let rows = ids
                .iter()
                .filter(|id| id.table as usize == index)
                .map(|id| id.row)
                .collect::<Vec<_>>();
            if rows.is_empty() {
                continue;
            }
            let descriptor = &self.manifest.node_tables[index];
            let names = table
                .fields
                .iter()
                .filter(|(name, _)| {
                    name == &descriptor.primary_key
                        || matches!(
                            name.as_str(),
                            "name" | "path" | "uid" | "cluster" | "lang" | "line_number"
                        )
                })
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            let mut stream = table.stream(
                &names,
                Some(table.select_rows(&rows)?),
                self.policy.0,
                limits.row_batch_size,
                Arc::clone(&work),
                Arc::clone(&original),
            )?;
            let mut keys = HashSet::new();
            let mut next = 0;
            while let Some(batch) = wait(&original, async {
                stream.next().await.transpose().map_err(Into::into)
            })
            .await?
            {
                work.rows(batch.num_rows())?;
                for row in 0..batch.num_rows() {
                    let key = match table
                        .fields
                        .iter()
                        .find(|(name, _)| name == &descriptor.primary_key)
                    {
                        Some((_, Scalar::String)) => {
                            let key = text(&batch, &descriptor.primary_key, row)?
                                .ok_or(DataReadError::InvalidParquet("nonnull primary key"))?;
                            bounded(key.len(), limits.max_string_bytes, "primary key")?;
                            PrimaryKey::String(key.to_owned())
                        }
                        Some((_, Scalar::Int64)) => PrimaryKey::Int64(
                            integer(&batch, &descriptor.primary_key, row)?
                                .ok_or(DataReadError::InvalidParquet("nonnull primary key"))?,
                        ),
                        _ => return Err(DataReadError::InvalidParquet("primary key type").into()),
                    };
                    let primary_key = GraphProperty {
                        name: limits.text(&descriptor.primary_key)?,
                        value: match &key {
                            PrimaryKey::String(value) => {
                                crate::neighborhood::PropertyValue::String(limits.text(value)?)
                            }
                            PrimaryKey::Int64(value) => {
                                crate::neighborhood::PropertyValue::Int64(*value)
                            }
                        },
                    };
                    if !keys.insert(key) {
                        return Err(DataReadError::DuplicatePrimaryKey.into());
                    }
                    let optional = |name| -> Result<Option<String>> {
                        if names.contains(&name) {
                            text(&batch, name, row)?
                                .map(|text| limits.text(text))
                                .transpose()
                                .map_err(Into::into)
                        } else {
                            Ok(None)
                        }
                    };
                    let line = if names.contains(&"line_number") {
                        integer(&batch, "line_number", row)?.filter(|line| *line != 0)
                    } else {
                        None
                    };
                    let physical = *rows
                        .get(next)
                        .ok_or(DataReadError::InvalidParquet("selected row count"))?;
                    nodes.push(GraphNodeSummary {
                        primary_key: Some(primary_key),
                        id: self.node_id(index, physical),
                        label: limits.text(&descriptor.label)?,
                        name: optional("name")?,
                        path: optional("path")?,
                        uid: optional("uid")?,
                        cluster: optional("cluster")?,
                        lang: optional("lang")?,
                        line,
                    });
                    next += 1;
                }
                original.check()?;
            }
            if next != rows.len() {
                return Err(DataReadError::InvalidParquet("selected row count").into());
            }
        }
        original.check()?;
        Ok(GraphSample { nodes, edges })
    }
}

fn object_path(prefix: &Path, relative: &str) -> Path {
    let mut path = prefix.clone();
    path.extend(relative.split('/'));
    path
}

fn column<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a dyn Array> {
    batch
        .column_by_name(name)
        .map(|column| column.as_ref())
        .ok_or_else(|| DataReadError::InvalidParquet("projected column").into())
}
fn text<'a>(batch: &'a RecordBatch, name: &str, row: usize) -> Result<Option<&'a str>> {
    let array = column(batch, name)?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or(DataReadError::InvalidParquet("STRING column"))?;
    Ok((!array.is_null(row)).then(|| array.value(row)))
}
fn integer(batch: &RecordBatch, name: &str, row: usize) -> Result<Option<i64>> {
    let array = column(batch, name)?
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or(DataReadError::InvalidParquet("INT64 column"))?;
    Ok((!array.is_null(row)).then(|| array.value(row)))
}
fn uint(batch: &RecordBatch, name: &str, row: usize) -> Result<u64> {
    let array = column(batch, name)?
        .as_any()
        .downcast_ref::<UInt64Array>()
        .filter(|array| !array.is_null(row))
        .ok_or(DataReadError::InvalidCsr("nonnull UInt64"))?;
    Ok(array.value(row))
}

#[cfg(test)]
mod tests;
