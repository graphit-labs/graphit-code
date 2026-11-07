//! Icebug reads, native storage locations, record schemas and Lance snapshot/publication adapters.
//! Authorization and domain effects remain with callers; this library does not execute Cypher.
mod code_identity;
mod error;
pub mod graph_search_sidecar;
pub mod graph_writer;
pub mod knowledge_branch_materialization;
pub mod knowledge_document_lifecycle;
pub mod knowledge_document_projection;
pub mod knowledge_document_update;
pub mod knowledge_embeddings;
pub mod knowledge_first_publication;
pub mod knowledge_index;
pub mod knowledge_reference_query;
pub mod knowledge_references;
pub mod knowledge_search;
pub mod knowledge_snapshot;
pub mod knowledge_sync_history;
pub mod lance_branch_store;
pub mod lance_module_store;
mod manifest;
pub mod native_storage_uri;
mod neighborhood;
mod parquet;
mod policy;
mod protocol;
pub mod remote_graph;
pub mod vector_distance;

use ::parquet::record::{Field, Row};
pub use code_identity::{
    CODE_ID_TEXT_BYTES, CodeDeclaration, CodeFileId, CodeIdentityError, CodePrimaryKey,
    CodeRelationOccurrence, CodeRelationUid, CodeScope, CodeUid,
};
pub use error::DataReadError;
use error::{Result, add, bounded};
use graphit_core::artifact_store::{
    CapturedSourceFile, CapturedSourceReader, SnapshotContext, SnapshotPolicy, SourceReadLimits,
};
use graphit_fs::NodeKind;
use manifest::Manifest;
pub use neighborhood::{
    GraphAnchorKey, GraphDirection, GraphNeighborhoodCursor, GraphNeighborhoodLimits,
    GraphNeighborhoodPage, GraphNeighborhoodPosition, GraphNeighborhoodRequest, GraphProperty,
    GraphPropertyEdge, GraphPropertyNode, GraphPropertyValueRef,
    fingerprint as neighborhood_fingerprint,
};
use parquet::{DecodeRequest, Scalar, Shape, Table, Work};
pub use policy::{GraphSampleLimits, LocalGraphLimits, LocalGraphPolicy};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
    sync::Arc,
};

/// Physical table/row coordinates in a sample or page, not a portable lookup key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId {
    table: u32,
    row: u64,
}
/// A resolved ordinal borrowed from its genuine graph capture, not a portable ID.
/// ```compile_fail
/// fn detach(graph: graphit_data::CapturedLocalGraph, limits: graphit_data::GraphNeighborhoodLimits)
///     -> graphit_data::ResolvedGraphNode<'static> {
///     graph.resolve_node("File", graphit_data::GraphAnchorKey::String("a.rs"), limits).unwrap()
/// }
/// ```
pub struct ResolvedGraphNode<'a> {
    graph: GraphSchemaSource<'a>,
    id: NodeId,
}
impl ResolvedGraphNode<'_> {
    pub fn id(&self) -> NodeId {
        self.id
    }
}
impl NodeId {
    pub fn table(self) -> u32 {
        self.table
    }
    pub fn row(self) -> u64 {
        self.row
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EdgeId {
    member: u32,
    row: u64,
}
impl EdgeId {
    pub fn member(self) -> u32 {
        self.member
    }
    pub fn row(self) -> u64 {
        self.row
    }
}
/// The sample owns response strings; summaries expose borrowed views.
/// ```compile_fail
/// fn detach(node: graphit_data::GraphNodeSummary) { let _ = node.name; }
/// ```
pub struct GraphNodeSummary {
    id: NodeId,
    label: String,
    name: Option<String>,
    path: Option<String>,
    uid: Option<String>,
    cluster: Option<String>,
    lang: Option<String>,
    line: Option<i64>,
    primary_key: Option<GraphProperty>,
}
impl GraphNodeSummary {
    /// The observed scalar primary key, when supplied by the local reader.
    pub fn primary_key(&self) -> Option<&GraphProperty> {
        self.primary_key.as_ref()
    }
    pub fn id(&self) -> NodeId {
        self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
    pub fn uid(&self) -> Option<&str> {
        self.uid.as_deref()
    }
    pub fn cluster(&self) -> Option<&str> {
        self.cluster.as_deref()
    }
    pub fn lang(&self) -> Option<&str> {
        self.lang.as_deref()
    }
    pub fn line(&self) -> Option<i64> {
        self.line
    }
}
pub struct GraphEdgeSummary {
    id: EdgeId,
    from: NodeId,
    to: NodeId,
    relationship: String,
    uid: String,
}
impl GraphEdgeSummary {
    pub fn id(&self) -> EdgeId {
        self.id
    }
    pub fn from(&self) -> NodeId {
        self.from
    }
    pub fn to(&self) -> NodeId {
        self.to
    }
    pub fn relationship(&self) -> &str {
        &self.relationship
    }
    pub fn uid(&self) -> &str {
        &self.uid
    }
}
/// The sample owns its vectors and strings and exposes borrowed results.
/// ```compile_fail
/// fn copy(sample: &graphit_data::GraphSample) { let _: graphit_data::GraphSample = sample.clone(); }
/// ```
/// ```compile_fail
/// fn detach(sample: graphit_data::GraphSample) { let _ = sample.nodes; }
/// ```
pub struct GraphSample {
    nodes: Vec<GraphNodeSummary>,
    edges: Vec<GraphEdgeSummary>,
}
impl GraphSample {
    pub fn nodes(&self) -> &[GraphNodeSummary] {
        &self.nodes
    }
    pub fn edges(&self) -> &[GraphEdgeSummary] {
        &self.edges
    }
}
struct MemberTable {
    from: usize,
    to: usize,
    kind: String,
    rows: u64,
    source_row_base: u64,
    source_rows: u64,
    target_row_base: u64,
    target_rows: u64,
    indices: Table,
    indptr: Table,
}

/// Metadata borrowed from an already validated local graph; it grants no access.
/// ```compile_fail
/// fn escape(graph: graphit_data::CapturedLocalGraph) -> graphit_data::GraphSchema<'static> {
///     graph.schema().unwrap()
/// }
/// ```
/// ```compile_fail
/// fn fabricate(graph: &graphit_data::CapturedLocalGraph) {
///     let _ = graphit_data::GraphSchema(graph);
/// }
/// ```
enum GraphSchemaSource<'a> {
    Local(&'a CapturedLocalGraph),
    Remote(&'a remote_graph::RemoteIcebugGraph),
}
pub struct GraphSchema<'a>(GraphSchemaSource<'a>);
impl GraphSchema<'_> {
    fn manifest(&self) -> &Manifest {
        match self.0 {
            GraphSchemaSource::Local(graph) => &graph.manifest,
            GraphSchemaSource::Remote(graph) => graph.manifest(),
        }
    }
    /// Validated relative schema reference; this does not open or execute it.
    pub fn schema_file(&self) -> &str {
        &self.manifest().schema_file
    }
    /// Declared graph files in validated manifest order; metadata grants no access.
    pub fn referenced_files(&self) -> impl Iterator<Item = &str> {
        let mut seen = HashSet::new();
        ["icebug.json", self.schema_file()]
            .into_iter()
            .chain(
                self.manifest()
                    .node_tables
                    .iter()
                    .flat_map(|table| table.shards.iter().map(|shard| shard.file.as_str())),
            )
            .chain(
                self.manifest()
                    .relationships
                    .iter()
                    .flat_map(|group| group.members.iter().chain(&group.reverse_members))
                    .flat_map(|member| [member.indices.as_str(), member.indptr.as_str()]),
            )
            .filter(move |path| seen.insert(*path))
    }
    pub fn node_tables(&self) -> impl ExactSizeIterator<Item = GraphNodeSchema<'_>> {
        self.manifest().node_tables.iter().map(GraphNodeSchema)
    }
    /// Forward members only: reverse storage does not create duplicate relationships.
    pub fn relationships(
        &self,
    ) -> Box<dyn ExactSizeIterator<Item = GraphRelationshipSchema<'_>> + '_> {
        match self.0 {
            GraphSchemaSource::Local(graph) => {
                Box::new(graph.members.iter().map(|member| GraphRelationshipSchema {
                    indices: member.indices.relative_path(),
                    indptr: member.indptr.relative_path(),
                    indices_row_offset: member.indices.row_offset(),
                    indptr_row_offset: member.indptr.row_offset(),
                    kind: &member.kind,
                    from: &graph.manifest.node_tables[member.from].label,
                    to: &graph.manifest.node_tables[member.to].label,
                    rows: member.rows,
                    fields: &member.indices.fields,
                    source_row_base: member.source_row_base,
                    source_rows: member.source_rows,
                    target_row_base: member.target_row_base,
                    target_rows: member.target_rows,
                }))
            }
            GraphSchemaSource::Remote(graph) => Box::new(graph.relationship_schemas()),
        }
    }
}

pub struct GraphNodeSchema<'a>(&'a manifest::NodeTable);
impl GraphNodeSchema<'_> {
    pub fn shards(&self) -> impl ExactSizeIterator<Item = GraphNodeShard<'_>> {
        self.0.shards.iter().map(GraphNodeShard)
    }
    pub fn row_extent(&self) -> u64 {
        self.0.row_extent()
    }
    pub fn label(&self) -> &str {
        &self.0.label
    }
    pub fn primary_key(&self) -> &str {
        &self.0.primary_key
    }
    pub fn rows(&self) -> u64 {
        self.0.rows
    }
    pub fn columns(&self) -> impl ExactSizeIterator<Item = (&str, &str)> {
        self.0
            .columns
            .iter()
            .map(|column| (column.name.as_str(), column.kind.as_str()))
    }
    pub fn language_counts(&self) -> impl ExactSizeIterator<Item = (&str, u64)> {
        self.0
            .lang_counts
            .iter()
            .map(|count| (count.lang.as_str(), count.rows))
    }
}

pub struct GraphNodeShard<'a>(&'a manifest::NodeShard);
impl<'a> GraphNodeShard<'a> {
    pub fn storage_file(&self) -> &'a str {
        &self.0.file
    }
    pub fn row_base(&self) -> u64 {
        self.0.row_base
    }
    pub fn row_offset(&self) -> u64 {
        self.0.row_offset
    }
    pub fn rows(&self) -> u64 {
        self.0.rows
    }
}

pub struct GraphRelationshipSchema<'a> {
    indices: &'a str,
    indptr: &'a str,
    indices_row_offset: u64,
    indptr_row_offset: u64,
    kind: &'a str,
    from: &'a str,
    to: &'a str,
    rows: u64,
    fields: &'a [(String, Scalar)],
    source_row_base: u64,
    source_rows: u64,
    target_row_base: u64,
    target_rows: u64,
}
impl GraphRelationshipSchema<'_> {
    pub fn indices_row_offset(&self) -> u64 {
        self.indices_row_offset
    }
    pub fn indptr_row_offset(&self) -> u64 {
        self.indptr_row_offset
    }
    pub fn source_row_base(&self) -> u64 {
        self.source_row_base
    }
    pub fn source_rows(&self) -> u64 {
        self.source_rows
    }
    pub fn target_row_base(&self) -> u64 {
        self.target_row_base
    }
    pub fn target_rows(&self) -> u64 {
        self.target_rows
    }

    pub fn indices_file(&self) -> &str {
        self.indices
    }
    pub fn indptr_file(&self) -> &str {
        self.indptr
    }
    pub fn kind(&self) -> &str {
        self.kind
    }
    pub fn from_label(&self) -> &str {
        self.from
    }
    pub fn to_label(&self) -> &str {
        self.to
    }
    pub fn rows(&self) -> u64 {
        self.rows
    }
    pub fn properties(&self) -> impl Iterator<Item = (&str, &'static str)> {
        self.fields
            .iter()
            .filter(|(name, _)| name != "target")
            .map(|(name, kind)| {
                let kind = match kind {
                    Scalar::String => "STRING",
                    Scalar::Int64 => "INT64",
                    Scalar::Bool => "BOOL",
                    Scalar::UInt64 => "UINT64",
                };
                (name.as_str(), kind)
            })
    }
}

/// Original context/source are private; serialized identities never recreate authority.
/// ```compile_fail
/// fn bypass(graph: &graphit_data::CapturedLocalGraph) { let _ = &graph.reader; }
/// ```
pub struct CapturedLocalGraph {
    reader: CapturedSourceReader,
    context: Arc<SnapshotContext>,
    policy: LocalGraphPolicy,
    manifest: Manifest,
    nodes: Vec<Table>,
    members: Vec<MemberTable>,
    reverse_members: Vec<MemberTable>,
}
fn generated_manifest_bytes(
    file: &CapturedSourceFile,
    context: &SnapshotContext,
) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut input = file.stream(0)?;
    let mut bytes = Vec::new();
    let mut block = [0; 64 * 1024];
    loop {
        context.check()?;
        let read = input.read(&mut block)?;
        if read == 0 {
            break;
        }
        bytes
            .try_reserve(read)
            .map_err(|_| DataReadError::LimitExceeded("manifest allocation"))?;
        bytes.extend_from_slice(&block[..read]);
    }
    context.check()?;
    Ok(bytes)
}

impl CapturedLocalGraph {
    /// Validate the producer namespace after admission, without deriving a
    /// generation shared with other captures or independently published corpora.
    pub fn validate_namespace(&self, namespace: [u8; 32]) -> Result<()> {
        self.context.check()?;
        let live = self
            .manifest
            .live
            .as_ref()
            .ok_or(DataReadError::InvalidManifest("local graph metadata"))?;
        if live.namespace != Some(namespace) {
            return Err(DataReadError::InvalidManifest("local graph namespace"));
        }
        Ok(())
    }
    /// Borrow the genuine Core capture for exporting this same graph publication.
    /// The caller retains its admission; the reader supplies no authorization.
    pub fn source_reader(&self, context: &SnapshotContext) -> Result<&CapturedSourceReader> {
        self.revalidate(context)?;
        Ok(&self.reader)
    }

    /// Check the retained native source and the original operation, without rehashing it.
    pub fn revalidate(&self, context: &SnapshotContext) -> Result<()> {
        self.reader.check_request(context)?;
        self.reader.revalidate_bindings()?;
        Ok(())
    }
    /// Open a declared Lance sidecar from this exact captured graph tree.
    /// SDK pathname reads require this owner to remain alive and be revalidated
    /// around their operation. Neither schema nor locator supplies authorization.
    pub async fn open_lance_sidecar(
        &self,
        directory: &str,
        schema: Arc<arrow_schema::Schema>,
        context: &SnapshotContext,
    ) -> std::io::Result<Option<lance_module_store::ModuleSnapshot>> {
        self.reader.check_request(context)?;
        if directory != "source.lance" {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        let root = self.reader.source_path(context)?;
        let Some(node) = self
            .reader
            .nodes()
            .find(|node| node.relative() == Path::new(directory))
        else {
            return Ok(None);
        };
        if node.kind() != NodeKind::Directory {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        let path = root.join(directory);
        let uri = path.to_str().ok_or(std::io::ErrorKind::InvalidInput)?;
        let opened = lance_module_store::LanceModuleStore::open(uri, schema).await;
        self.reader.revalidate_bindings()?;
        context.check()?;
        opened
            .map(|store| Some(store.snapshot()))
            .map_err(std::io::Error::other)
    }

    pub fn capture(
        source: &Path,
        snapshot_policy: &SnapshotPolicy,
        read_limits: SourceReadLimits,
        context: Arc<SnapshotContext>,
        policy: LocalGraphPolicy,
    ) -> Result<Self> {
        context.check()?;
        let reader = CapturedSourceReader::capture(
            source,
            NodeKind::Directory,
            snapshot_policy,
            read_limits,
            Arc::clone(&context),
        )?;
        Self::from_reader(reader, context, policy)
    }
    /// Capture only this owned publication's declared Icebug files. The
    /// consumer owns PROJECT admission and its publication read fence. Parquet
    /// bodies are read on demand; neither the directory nor old FILE shards are walked.
    pub fn capture_generated_metadata(
        source: &Path,
        context: Arc<SnapshotContext>,
        policy: LocalGraphPolicy,
    ) -> Result<Self> {
        let read = SourceReadLimits::generated(policy.0.max_chunk_bytes, 64 * 1024)?;
        let manifest_reader = CapturedSourceReader::capture_generated_metadata(
            source,
            &["icebug.json".into()],
            read,
            Arc::clone(&context),
        )?;
        let file = manifest_reader.open_file(Path::new("icebug.json"))?;
        let bytes = generated_manifest_bytes(&file, &context)?;
        let manifest = Manifest::parse_generated(&bytes, policy.0)?;
        let files = ["icebug.json", manifest.schema_file.as_str()]
            .into_iter()
            .chain(
                manifest
                    .node_tables
                    .iter()
                    .flat_map(|table| table.shards.iter().map(|shard| shard.file.as_str())),
            )
            .chain(
                manifest
                    .relationships
                    .iter()
                    .flat_map(|group| group.members.iter().chain(&group.reverse_members))
                    .flat_map(|member| [member.indices.as_str(), member.indptr.as_str()]),
            )
            .map(std::path::PathBuf::from)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let reader = CapturedSourceReader::capture_generated_metadata(
            source,
            &files,
            read,
            Arc::clone(&context),
        )?;
        // A concurrent manifest publication cannot combine two generations.
        let observed = reader.open_file(Path::new("icebug.json"))?;
        if observed.len() != file.len() || generated_manifest_bytes(&observed, &context)? != bytes {
            return Err(DataReadError::InvalidManifest(
                "publication changed during capture",
            ));
        }
        drop(observed);
        drop(file);
        drop(manifest_reader);
        drop(bytes);
        Self::from_manifest(reader, context, policy, manifest)
    }
    /// Consume a genuine Core reader, preserving its original operation and source guards.
    /// This supplies no authorization and does not restore a reader from a generation.
    pub fn from_reader(
        reader: CapturedSourceReader,
        context: Arc<SnapshotContext>,
        policy: LocalGraphPolicy,
    ) -> Result<Self> {
        reader.check_request(&context)?;
        let limits = policy.0;
        let manifest_file = reader.open_file(Path::new("icebug.json"))?;
        let length = usize::try_from(manifest_file.len())
            .map_err(|_| DataReadError::LimitExceeded("manifest bytes"))?;
        bounded(length, limits.manifest_bytes, "manifest bytes")?;
        let bytes = manifest_file.read_range(0, length)?;
        let manifest = Manifest::parse(bytes.as_ref(), limits)?;
        drop(bytes);
        drop(manifest_file);
        Self::from_manifest(reader, context, policy, manifest)
    }
    fn from_manifest(
        reader: CapturedSourceReader,
        context: Arc<SnapshotContext>,
        policy: LocalGraphPolicy,
        manifest: Manifest,
    ) -> Result<Self> {
        reader.check_request(&context)?;
        let limits = policy.0;
        let mut captured = BTreeMap::new();
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(manifest.node_tables.len())
            .map_err(|_| DataReadError::LimitExceeded("tables allocation"))?;
        for node in &manifest.node_tables {
            context.check()?;
            let table = Table::capture_nodes(&reader, node, limits, &mut captured)?;
            for (name, kind) in &table.fields {
                if matches!(name.as_str(), "name" | "path" | "uid" | "cluster" | "lang")
                    && *kind != Scalar::String
                    || name == "line_number" && *kind != Scalar::Int64
                {
                    return Err(DataReadError::InvalidParquet("summary column type"));
                }
            }
            nodes.push(table);
        }
        let mut members = Vec::new();
        let mut reverse_members = Vec::new();
        for group in &manifest.relationships {
            for member in &group.members {
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
                let pointer_rows = member
                    .source_rows
                    .checked_add(1)
                    .ok_or(DataReadError::InvalidManifest("pointer rows"))?;
                let indices = Table::capture(
                    &reader,
                    &member.indices,
                    member.indices_row_offset,
                    member.rows,
                    Shape::Indices,
                    limits,
                    &mut captured,
                )?;
                let indptr = Table::capture(
                    &reader,
                    &member.indptr,
                    member.indptr_row_offset,
                    pointer_rows,
                    Shape::Indptr,
                    limits,
                    &mut captured,
                )?;
                members
                    .try_reserve(1)
                    .map_err(|_| DataReadError::LimitExceeded("members allocation"))?;
                members.push((
                    member.table.as_str(),
                    MemberTable {
                        from,
                        to,
                        kind: copy_text(&group.kind)?,
                        rows: member.rows,
                        source_row_base: member.source_row_base,
                        source_rows: member.source_rows,
                        target_row_base: member.target_row_base,
                        target_rows: member.target_rows,
                        indices,
                        indptr,
                    },
                ));
            }
            for member in &group.reverse_members {
                context.check()?;
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
                let pointer_rows = member
                    .source_rows
                    .checked_add(1)
                    .ok_or(DataReadError::InvalidManifest("pointer rows"))?;
                let indices = Table::capture(
                    &reader,
                    &member.indices,
                    member.indices_row_offset,
                    member.rows,
                    Shape::Indices,
                    limits,
                    &mut captured,
                )?;
                let indptr = Table::capture(
                    &reader,
                    &member.indptr,
                    member.indptr_row_offset,
                    pointer_rows,
                    Shape::Indptr,
                    limits,
                    &mut captured,
                )?;
                reverse_members.push(MemberTable {
                    from,
                    to,
                    kind: copy_text(&group.kind)?,
                    rows: member.rows,
                    source_row_base: member.source_row_base,
                    source_rows: member.source_rows,
                    target_row_base: member.target_row_base,
                    target_rows: member.target_rows,
                    indices,
                    indptr,
                });
            }
        }
        members.sort_by(|a, b| a.0.cmp(b.0));
        let mut ordered = Vec::new();
        ordered
            .try_reserve_exact(members.len())
            .map_err(|_| DataReadError::LimitExceeded("member ordering"))?;
        for (_, member) in members {
            ordered.push(member);
        }
        let members = ordered;
        reader.revalidate_bindings()?;
        context.check()?;
        Ok(Self {
            reader,
            context,
            policy,
            manifest,
            nodes,
            members,
            reverse_members,
        })
    }
    /// Checks the captured operation before exposing its complete metadata.
    pub fn schema(&self) -> Result<GraphSchema<'_>> {
        self.context.check()?;
        self.reader.revalidate_bindings()?;
        Ok(GraphSchema(GraphSchemaSource::Local(self)))
    }
    /// Opens a relative file through the retained capture and original operation.
    /// The returned Core owner keeps range/stream reads bound to that operation.
    pub fn open_file(&self, relative: &Path) -> Result<CapturedSourceFile> {
        self.context.check()?;
        let file = self.reader.open_file(relative)?;
        self.context.check()?;
        Ok(file)
    }
    pub fn sample_graph(&self, limits: GraphSampleLimits) -> Result<GraphSample> {
        self.sample_graph_seeds(limits, None)
    }
    /// Exclude rows whose boolean marker is true from the seed selection only.
    /// Missing/NULL/false markers remain eligible, regardless of table label.
    /// Real relationship endpoints remain in the sample and scans still reach EOF.
    pub fn sample_graph_excluding_marked(
        &self,
        limits: GraphSampleLimits,
        marker: &str,
    ) -> Result<GraphSample> {
        if marker.is_empty() {
            return Err(DataReadError::InvalidPolicy("sample marker"));
        }
        self.sample_graph_seeds(limits, Some(marker))
    }
    fn sample_graph_seeds(
        &self,
        limits: GraphSampleLimits,
        marker: Option<&str>,
    ) -> Result<GraphSample> {
        self.context.check()?;

        self.reader.revalidate_bindings()?;
        let work = Work::new(limits.into());
        let request = DecodeRequest {
            source: &self.reader,
            context: &self.context,

            limits: self.policy.0,
            decode: limits.into(),
            work: &work,
        };
        let mut selected = HashSet::new();
        let mut seeds = limits.seed_nodes;
        for (index, table) in self.nodes.iter().enumerate() {
            if seeds == 0 {
                break;
            }
            if let Some(marker) =
                marker.filter(|marker| table.fields.iter().any(|(name, _)| name == marker))
            {
                for range in table.row_group_ranges()? {
                    if seeds == 0 {
                        break;
                    }
                    let count = usize::try_from(range.end - range.start)
                        .map_err(|_| DataReadError::LimitExceeded("decoded rows"))?;
                    bounded(count, request.decode.max_decoded_rows, "decoded rows")?;
                    let rows = range.collect::<Vec<_>>();
                    table.scan_selected(&request, &[marker], &rows, |row, record| {
                        let excluded = match field(record, marker)? {
                            Field::Bool(value) => *value,
                            Field::Null => false,
                            _ => {
                                return Err(DataReadError::InvalidParquet("sample boolean marker"));
                            }
                        };
                        if !excluded && seeds > 0 {
                            admit_node(&mut selected, self.node_id(index, row), &limits)?;
                            seeds -= 1;
                        }
                        Ok(())
                    })?;
                }
            } else {
                let count = table.rows.min(seeds as u64);
                for row in table.seed_rows(count)? {
                    admit_node(&mut selected, self.node_id(index, row), &limits)?;
                }
                seeds -= count as usize;
            }
        }
        let mut edges = Vec::new();
        let mut remaining = limits.global_edges;
        for (index, member) in self.members.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            let count = member.rows.min(remaining as u64) as usize;
            if count == 0 {
                continue;
            }
            let rows = (0..count as u64).collect::<Vec<_>>();
            let edge_start = edges.len();
            member
                .indices
                .scan_selected(&request, &["target", "uid"], &rows, |row, record| {
                    let local_target = uint(field(record, "target")?)?;
                    if local_target >= member.target_rows {
                        return Err(DataReadError::InvalidCsr("target shard bounds"));
                    }
                    let target = member
                        .target_row_base
                        .checked_add(local_target)
                        .ok_or(DataReadError::InvalidCsr("target bounds"))?;
                    if !self.nodes[member.to].contains(target) {
                        return Err(DataReadError::InvalidCsr("target bounds"));
                    }
                    let uid = string(field(record, "uid")?)?
                        .ok_or(DataReadError::InvalidCsr("NULL edge UID"))?;
                    if uid.is_empty() {
                        return Err(DataReadError::InvalidCsr("empty edge UID"));
                    }
                    bounded(uid.len(), limits.max_string_bytes, "edge UID")?;
                    if row < count as u64 {
                        edges
                            .try_reserve(1)
                            .map_err(|_| DataReadError::LimitExceeded("edge allocation"))?;
                        edges.push(GraphEdgeSummary {
                            id: EdgeId {
                                member: index as u32,
                                row,
                            },
                            from: self.node_id(member.from, 0),
                            to: self.node_id(member.to, target),
                            relationship: limits.text(&member.kind)?,
                            uid: limits.text(uid)?,
                        });
                    }
                    Ok(())
                })?;
            let mut previous = 0;
            let mut next_edge = edge_start;
            member.indptr.scan(&request, &["ptr"], |row, record| {
                let pointer = uint(field(record, "ptr")?)?;
                if row == 0 && pointer != 0 || pointer < previous || pointer > member.rows {
                    return Err(DataReadError::InvalidCsr("row pointer"));
                }
                if row > 0 {
                    while next_edge < edge_start + count && edges[next_edge].id.row < pointer {
                        if edges[next_edge].id.row < previous {
                            return Err(DataReadError::InvalidCsr("edge alignment"));
                        }
                        edges[next_edge].from = self.node_id(
                            member.from,
                            member
                                .source_row_base
                                .checked_add(row - 1)
                                .ok_or(DataReadError::InvalidCsr("source bounds"))?,
                        );
                        next_edge += 1;
                    }
                }
                previous = pointer;
                Ok(())
            })?;
            if previous != member.rows || next_edge != edge_start + count {
                return Err(DataReadError::InvalidCsr("final pointer"));
            }
            for edge in &edges[edge_start..] {
                admit_node(&mut selected, edge.from, &limits)?;
                admit_node(&mut selected, edge.to, &limits)?;
            }
            remaining -= count;
        }
        bounded(selected.len(), limits.max_nodes, "sample nodes")?;
        let mut nodes = Vec::new();
        for (index, table) in self.nodes.iter().enumerate() {
            let descriptor = &self.manifest.node_tables[index];
            let mut rows = selected
                .iter()
                .filter(|id| id.table as usize == index)
                .map(|id| id.row)
                .collect::<Vec<_>>();
            if rows.is_empty() {
                continue;
            }
            rows.sort_unstable();
            let names: Vec<_> = table
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
                .collect();
            let mut primary_keys = HashSet::new();
            table.scan_selected(&request, &names, &rows, |row, record| {
                let key = match field(record, &descriptor.primary_key)? {
                    Field::Str(value) => {
                        bounded(value.len(), limits.max_string_bytes, "primary key")?;
                        PrimaryKey::String(copy_text(value)?)
                    }
                    Field::Long(value) => PrimaryKey::Int64(*value),
                    _ => return Err(DataReadError::InvalidParquet("nonnull primary key")),
                };
                primary_keys
                    .try_reserve(1)
                    .map_err(|_| DataReadError::LimitExceeded("primary keys"))?;
                if !primary_keys.insert(key) {
                    return Err(DataReadError::DuplicatePrimaryKey);
                }
                let id = self.node_id(index, row);
                if selected.contains(&id) {
                    let optional = |name| -> Result<Option<String>> {
                        match record
                            .get_column_iter()
                            .find(|(key, _)| key.as_str() == name)
                        {
                            None => Ok(None),
                            Some((_, field)) => {
                                string(field)?.map(|value| limits.text(value)).transpose()
                            }
                        }
                    };
                    let name = optional("name")?;
                    let path = optional("path")?;
                    let uid = optional("uid")?;
                    let cluster = optional("cluster")?;
                    let lang = optional("lang")?;
                    let line = match record
                        .get_column_iter()
                        .find(|(name, _)| name.as_str() == "line_number")
                    {
                        None | Some((_, Field::Null)) | Some((_, Field::Long(0))) => None,
                        Some((_, Field::Long(line))) => Some(*line),
                        _ => return Err(DataReadError::InvalidParquet("line number")),
                    };
                    nodes
                        .try_reserve(1)
                        .map_err(|_| DataReadError::LimitExceeded("nodes allocation"))?;
                    nodes.push(GraphNodeSummary {
                        id,
                        label: limits.text(&descriptor.label)?,
                        primary_key: Some(GraphProperty {
                            name: limits.text(&descriptor.primary_key)?,
                            value: match field(record, &descriptor.primary_key)? {
                                Field::Str(value) => {
                                    neighborhood::PropertyValue::String(limits.text(value)?)
                                }
                                Field::Long(value) => neighborhood::PropertyValue::Int64(*value),
                                _ => {
                                    return Err(DataReadError::InvalidParquet(
                                        "nonnull primary key",
                                    ));
                                }
                            },
                        }),
                        name,
                        path,
                        uid,
                        cluster,
                        lang,
                        line,
                    });
                }
                Ok(())
            })?;
        }
        if nodes.len() != selected.len() {
            return Err(DataReadError::InvalidCsr("missing endpoint"));
        }
        self.reader.revalidate_bindings()?;
        self.context.check()?;
        Ok(GraphSample { nodes, edges })
    }
    fn node_id(&self, table: usize, row: u64) -> NodeId {
        NodeId {
            table: table as u32,
            row,
        }
    }
}
#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum PrimaryKey {
    String(String),
    Int64(i64),
}
impl PrimaryKey {
    fn borrowed(&self) -> GraphAnchorKey<'_> {
        match self {
            Self::String(key) => GraphAnchorKey::String(key),
            Self::Int64(key) => GraphAnchorKey::Int64(*key),
        }
    }
}
impl GraphSampleLimits {
    fn text(&self, value: &str) -> Result<String> {
        bounded(value.len(), self.max_string_bytes, "output string")?;
        copy_text(value)
    }
}
fn admit_node(
    selected: &mut HashSet<NodeId>,
    id: NodeId,
    limits: &GraphSampleLimits,
) -> Result<()> {
    if selected.contains(&id) {
        return Ok(());
    }
    bounded(
        add(selected.len(), 1, "sample nodes")?,
        limits.max_nodes,
        "sample nodes",
    )?;
    selected
        .try_reserve(1)
        .map_err(|_| DataReadError::LimitExceeded("node identities"))?;
    selected.insert(id);
    Ok(())
}
fn copy_text(value: &str) -> Result<String> {
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| DataReadError::LimitExceeded("string allocation"))?;
    output.push_str(value);
    Ok(output)
}
fn field<'a>(row: &'a Row, name: &str) -> Result<&'a Field> {
    row.get_column_iter()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, field)| field)
        .ok_or(DataReadError::InvalidParquet("missing projected field"))
}
fn uint(field: &Field) -> Result<u64> {
    match field {
        Field::ULong(value) => Ok(*value),
        _ => Err(DataReadError::InvalidCsr("nonnull UInt64")),
    }
}
fn string(field: &Field) -> Result<Option<&str>> {
    match field {
        Field::Null => Ok(None),
        Field::Str(value) => Ok(Some(value)),
        _ => Err(DataReadError::InvalidParquet("STRING field")),
    }
}

#[cfg(test)]
mod native_graph_metadata_tests;
#[cfg(test)]
mod schema_tests;
#[cfg(test)]
mod storage_tests;
#[cfg(test)]
mod tests;
