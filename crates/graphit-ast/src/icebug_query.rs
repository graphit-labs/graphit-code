//! Operation-local Cypher through the official engine and captured file handles.
use graphit_core::{
    artifact_store::{CapturedSourceFile, SnapshotContext},
    file_lock::Cancellation,
};
use graphit_data::{
    CapturedLocalGraph, GraphAnchorKey, ResolvedGraphNode,
    remote_graph::{RemoteGraphFile, RemoteIcebugGraph},
};
// Retain the SDK crate's standard native link metadata for this CXX consumer.
use lbug as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read},
    path::Path,
    sync::Arc,
};
use tokio::runtime::Handle;

/// Native cache/parallelism and actual request/response bounds selected by the caller.
pub use ffi::QueryOptions;
impl QueryOptions {
    fn check(self) -> io::Result<()> {
        if (self.buffer_pool_bytes != 0 && self.buffer_pool_bytes < 4096)
            || self.query_threads == 0
            || self.max_rows == 0
            || self.max_bytes == 0
            || self.max_depth == 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid query options",
            ));
        }
        Ok(())
    }
}

/// Cells retain their native type; exact integers/temporal units use decimal text.
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Clone)]
enum QueryGraph {
    Local(Arc<CapturedLocalGraph>),
    Remote {
        graph: Arc<RemoteIcebugGraph>,
        original: Arc<SnapshotContext>,
        runtime: Handle,
    },
}
struct GraphIo {
    graph: QueryGraph,
    files: Arc<BTreeMap<String, String>>,
}
enum QueryFile {
    Local(CapturedSourceFile),
    Remote {
        file: RemoteGraphFile,
        original: Arc<SnapshotContext>,
        runtime: Handle,
    },
}
fn node_lookup(node: Option<ResolvedGraphNode<'_>>) -> ffi::NodeLookup {
    match node {
        Some(node) => ffi::NodeLookup {
            found: true,
            table: u64::from(node.id().table()),
            row: node.id().row(),
        },
        None => ffi::NodeLookup {
            found: false,
            table: 0,
            row: 0,
        },
    }
}

impl GraphIo {
    fn lookup_string(&self, label: &str, key: &str, batch: usize) -> io::Result<ffi::NodeLookup> {
        self.lookup(label, GraphAnchorKey::String(key), batch)
    }
    fn lookup_int64(&self, label: &str, key: i64, batch: usize) -> io::Result<ffi::NodeLookup> {
        self.lookup(label, GraphAnchorKey::Int64(key), batch)
    }
    fn lookup(
        &self,
        label: &str,
        key: GraphAnchorKey<'_>,
        batch: usize,
    ) -> io::Result<ffi::NodeLookup> {
        match &self.graph {
            QueryGraph::Local(graph) => Ok(node_lookup(
                graph
                    .resolve_query_node(label, key, batch)
                    .map_err(io::Error::other)?,
            )),
            QueryGraph::Remote {
                graph,
                original,
                runtime,
            } => runtime.block_on(async {
                original.check()?;
                let node = graph
                    .resolve_query_node(label, key, batch, Arc::clone(original))
                    .await
                    .map_err(io::Error::other)?;
                original.check()?;
                Ok(node_lookup(node))
            }),
        }
    }
    fn open(&self, handle: &str) -> io::Result<Box<QueryFile>> {
        let relative = self.files.get(handle).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "query path is not captured",
            )
        })?;
        let file = match &self.graph {
            QueryGraph::Local(graph) => QueryFile::Local(
                graph
                    .open_file(Path::new(relative))
                    .map_err(io::Error::other)?,
            ),
            QueryGraph::Remote {
                graph,
                original,
                runtime,
            } => {
                original.check()?;
                QueryFile::Remote {
                    file: graph.open_query_file(relative).map_err(io::Error::other)?,
                    original: Arc::clone(original),
                    runtime: runtime.clone(),
                }
            }
        };
        Ok(Box::new(file))
    }
    fn contains(&self, handle: &str) -> bool {
        self.files.contains_key(handle)
    }
}
impl QueryFile {
    fn validate_binding(&self) -> io::Result<()> {
        match self {
            Self::Local(file) => {
                drop(file.stream(0)?);
                Ok(())
            }
            Self::Remote { original, .. } => original.check(),
        }
    }
    fn revalidate(&self) -> io::Result<()> {
        match self {
            Self::Local(_) => self.validate_binding(),
            Self::Remote {
                file,
                original,
                runtime,
            } => {
                original.check()?;
                // Cached metadata still observes the captured object's ETag/version.
                // A nonempty conditional range uses the existing SDK read boundary.
                let prefix = runtime
                    .block_on(file.read_range(0, 1, original))
                    .map_err(io::Error::other)?;
                if prefix.len() != 1 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                original.check()
            }
        }
    }
    fn len(&self) -> u64 {
        match self {
            Self::Local(file) => file.len(),
            Self::Remote { file, .. } => file.len(),
        }
    }
    fn read_at(&self, offset: u64, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Local(file) => {
                let mut stream = file.stream(offset)?;
                let mut filled = 0;
                while filled < bytes.len() {
                    // Interrupted denotes cancellation here; read_exact would retry it.
                    let read = stream.read(&mut bytes[filled..])?;
                    if read == 0 {
                        return Err(io::ErrorKind::UnexpectedEof.into());
                    }
                    filled += read;
                }
            }
            Self::Remote {
                file,
                original,
                runtime,
            } => {
                // Invoked only inside the caller's blocking query job. The async
                // object-store request retains the captured ETag/version and Original.
                let value = runtime
                    .block_on(file.read_range(offset, bytes.len(), original))
                    .map_err(io::Error::other)?;
                original.check()?;
                if value.len() != bytes.len() {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                bytes.copy_from_slice(&value);
            }
        }
        Ok(bytes.len())
    }
}

struct QuerySignal {
    #[cfg(feature = "test-support")]
    observer: Option<Arc<QueryCatalogObserver>>,
    original: Arc<SnapshotContext>,
    complete: Cancellation,
    runtime: Handle,
}

struct Frame {
    native_type: String,
    field: String,
    value: Value,
}
struct ResultSink {
    result: QueryResult,
    row: Vec<Value>,
    stack: Vec<Frame>,
    encoded_bytes: usize,
    options: QueryOptions,
}
impl ResultSink {
    fn new(options: QueryOptions) -> io::Result<Self> {
        let result = QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
        };
        let encoded_bytes = serde_json::to_vec(&result).map_err(io::Error::other)?.len();
        Ok(Self {
            result,
            row: Vec::new(),
            stack: Vec::new(),
            encoded_bytes,
            options,
        })
    }
    fn charge(&mut self, encoded: usize) -> io::Result<()> {
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(encoded)
            .ok_or(io::ErrorKind::FileTooLarge)?;
        if self.encoded_bytes > self.options.max_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        Ok(())
    }
    fn columns(&mut self, columns: Vec<String>) -> io::Result<()> {
        self.charge(
            serde_json::to_vec(&columns)
                .map_err(io::Error::other)?
                .len()
                .saturating_sub(2),
        )?;
        self.result.columns = columns;
        Ok(())
    }
    fn begin_row(&mut self) -> io::Result<()> {
        if !self.row.is_empty()
            || !self.stack.is_empty()
            || self.result.rows.len() >= self.options.max_rows
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid native row boundary",
            ));
        }
        self.charge(2 + usize::from(!self.result.rows.is_empty()))
    }
    fn prefix(&mut self, field: &str) -> io::Result<()> {
        let amount = if let Some(frame) = self.stack.last() {
            if let Some(object) = frame.value.as_object() {
                if object.contains_key(field) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "duplicate native field",
                    ));
                }
                serde_json::to_vec(field).map_err(io::Error::other)?.len()
                    + 1
                    + usize::from(!object.is_empty())
            } else {
                usize::from(
                    frame
                        .value
                        .as_array()
                        .is_some_and(|array| !array.is_empty()),
                )
            }
        } else {
            usize::from(!self.row.is_empty())
        };
        self.charge(amount)
    }
    fn attach(&mut self, field: &str, value: Value) -> io::Result<()> {
        if let Some(frame) = self.stack.last_mut() {
            if let Some(object) = frame.value.as_object_mut() {
                object.insert(field.to_owned(), value);
            } else if let Some(array) = frame.value.as_array_mut() {
                array.push(value);
            } else {
                return Err(io::ErrorKind::InvalidData.into());
            }
        } else {
            self.row.push(value);
        }
        Ok(())
    }
    fn begin_cell(&mut self, native_type: &str, field: &str, object: bool) -> io::Result<()> {
        if self.stack.len() >= self.options.max_depth {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let value = if object { json!({}) } else { json!([]) };
        self.prefix(field)?;
        self.charge(
            serde_json::to_vec(&json!({"type":native_type,"value":value}))
                .map_err(io::Error::other)?
                .len(),
        )?;
        self.stack.push(Frame {
            native_type: native_type.to_owned(),
            field: field.to_owned(),
            value,
        });
        Ok(())
    }
    fn end_cell(&mut self) -> io::Result<()> {
        let frame = self.stack.pop().ok_or(io::ErrorKind::InvalidData)?;
        self.attach(
            &frame.field,
            json!({"type":frame.native_type,"value":frame.value}),
        )
    }
    fn atom(
        &mut self,
        native_type: &str,
        field: &str,
        repr: ffi::AtomKind,
        text: &str,
    ) -> io::Result<()> {
        if text.len() > self.options.max_bytes || native_type.len() > self.options.max_bytes {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let value = match repr {
            ffi::AtomKind::Null => Value::Null,
            ffi::AtomKind::Boolean => match text {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return Err(io::ErrorKind::InvalidData.into()),
            },
            ffi::AtomKind::Number => serde_json::from_str::<serde_json::Number>(text)
                .map(Value::Number)
                .map_err(io::Error::other)?,
            ffi::AtomKind::Text => Value::String(text.to_owned()),
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };
        let value = json!({"type":native_type,"value":value});
        self.prefix(field)?;
        self.charge(serde_json::to_vec(&value).map_err(io::Error::other)?.len())?;
        self.attach(field, value)
    }
    fn identity(&mut self, field: &str, index: u64, row: u64, edge: bool) -> io::Result<()> {
        let key = if edge { "member" } else { "table" };
        let value = json!({"type":"INTERNAL_ID","value":{key:index,"row":row.to_string()}});
        self.prefix(field)?;
        self.charge(serde_json::to_vec(&value).map_err(io::Error::other)?.len())?;
        self.attach(field, value)
    }
    fn end_row(&mut self) -> io::Result<()> {
        if !self.stack.is_empty() || self.row.len() != self.result.columns.len() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.result.rows.push(std::mem::take(&mut self.row));
        Ok(())
    }
}
impl QuerySignal {
    #[cfg(feature = "test-support")]
    fn on_database_created(&self) {
        if let Some(observer) = &self.observer {
            observer
                .databases
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    #[cfg(feature = "test-support")]
    fn on_ddl_executed(&self) {
        if let Some(observer) = &self.observer {
            observer
                .ddl
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn check(&self) -> io::Result<()> {
        self.original.check()
    }
    fn remaining_ms(&self) -> io::Result<u64> {
        self.check()?;
        let Some(deadline) = self.original.deadline() else {
            // The native SDK disables its timeout when the value is zero.
            return Ok(0);
        };
        let duration = deadline.saturating_duration_since(std::time::Instant::now());
        // The SDK interprets zero as an unlimited query, so round positive fractions up.
        u64::try_from(duration.as_millis().saturating_add(u128::from(
            !duration.subsec_nanos().is_multiple_of(1_000_000),
        )))
        .map(|value| value.max(1))
        .map_err(|_| io::ErrorKind::InvalidInput.into())
    }
    fn wait_interrupt(&self) -> bool {
        self.runtime.block_on(async {
            if let Some(deadline) = self.original.deadline() {
                tokio::select! {
                    () = self.complete.cancelled() => false,
                    () = self.original.cancellation().cancelled() => true,
                    () = tokio::time::sleep_until(deadline.into()) => true,
                }
            } else {
                tokio::select! {
                    () = self.complete.cancelled() => false,
                    () = self.original.cancellation().cancelled() => true,
                }
            }
        })
    }
    fn finish(&self) {
        self.complete.cancel();
    }
}

#[allow(unsafe_code)]
#[cxx::bridge(namespace = "graphit::icebug")]
mod ffi {
    #[derive(Clone, Copy, Debug)]
    struct QueryOptions {
        /// Zero selects the SDK's automatic pool; positive values are bytes.
        buffer_pool_bytes: u64,
        query_threads: u64,
        max_rows: usize,
        max_bytes: usize,
        max_depth: usize,
    }
    struct NodeLookup {
        found: bool,
        table: u64,
        row: u64,
    }
    struct PropertyType {
        name: String,
        data_type: String,
    }
    struct ShardMount {
        file: String,
        row_base: u64,
        row_offset: u64,
        rows: u64,
    }
    struct NodeMount {
        table: u64,
        rows: u64,
        shards: Vec<ShardMount>,
    }
    struct MemberMount {
        member: u64,
        kind: String,
        from_label: String,
        to_label: String,
        rows: u64,
        indices: String,
        indptr: String,
        indices_row_offset: u64,
        indptr_row_offset: u64,
        source_row_base: u64,
        source_rows: u64,
        target_row_base: u64,
        target_rows: u64,
        properties: Vec<PropertyType>,
    }
    struct GraphMount {
        nodes: Vec<NodeMount>,
        members: Vec<MemberMount>,
    }
    enum AtomKind {
        Null,
        Boolean,
        Number,
        Text,
    }
    extern "Rust" {
        type GraphIo;
        type QueryFile;
        type QuerySignal;
        type ResultSink;
        fn lookup_string(
            self: &GraphIo,
            label: &str,
            key: &str,
            batch: usize,
        ) -> Result<NodeLookup>;
        fn lookup_int64(self: &GraphIo, label: &str, key: i64, batch: usize) -> Result<NodeLookup>;
        fn open(self: &GraphIo, handle: &str) -> Result<Box<QueryFile>>;
        fn contains(self: &GraphIo, handle: &str) -> bool;
        fn len(self: &QueryFile) -> u64;
        fn read_at(self: &QueryFile, offset: u64, bytes: &mut [u8]) -> Result<usize>;
        fn validate_binding(self: &QueryFile) -> Result<()>;
        fn revalidate(self: &QueryFile) -> Result<()>;
        #[cfg(feature = "test-support")]
        fn on_database_created(self: &QuerySignal);
        #[cfg(feature = "test-support")]
        fn on_ddl_executed(self: &QuerySignal);
        fn check(self: &QuerySignal) -> Result<()>;
        fn remaining_ms(self: &QuerySignal) -> Result<u64>;
        fn wait_interrupt(self: &QuerySignal) -> bool;
        fn finish(self: &QuerySignal);
        fn columns(self: &mut ResultSink, columns: Vec<String>) -> Result<()>;
        fn begin_row(self: &mut ResultSink) -> Result<()>;
        fn begin_cell(
            self: &mut ResultSink,
            native_type: &str,
            field: &str,
            object: bool,
        ) -> Result<()>;
        fn end_cell(self: &mut ResultSink) -> Result<()>;
        fn atom(
            self: &mut ResultSink,
            native_type: &str,
            field: &str,
            repr: AtomKind,
            text: &str,
        ) -> Result<()>;
        fn identity(
            self: &mut ResultSink,
            field: &str,
            index: u64,
            row: u64,
            edge: bool,
        ) -> Result<()>;
        fn end_row(self: &mut ResultSink) -> Result<()>;
    }
    unsafe extern "C++" {
        include!("native/icebug_query.hpp");
        type NativeCatalog;
        fn prepare_catalog(
            io: Box<GraphIo>,
            signal: Box<QuerySignal>,
            schema: &str,
            mount: GraphMount,
            options: &QueryOptions,
        ) -> Result<UniquePtr<NativeCatalog>>;
        fn rebind_catalog(
            catalogue: Pin<&mut NativeCatalog>,
            io: Box<GraphIo>,
            signal: Box<QuerySignal>,
            mount: GraphMount,
        ) -> Result<()>;
        fn run_query(
            catalogue: &NativeCatalog,
            io: Box<GraphIo>,
            signal: Box<QuerySignal>,
            query: &str,
            parameters: &str,
            options: &QueryOptions,
            result: &mut ResultSink,
        ) -> Result<()>;
    }
}

#[derive(PartialEq, Eq, Serialize)]
struct NodeLayout {
    table: u64,
    label: String,
    primary_key: String,
    columns: Vec<(String, String)>,
}
#[derive(PartialEq, Eq, Serialize)]
struct MemberLayout {
    kind: String,
    from: String,
    to: String,
    properties: Vec<(String, String)>,
}
#[derive(PartialEq, Eq, Serialize)]
struct GraphLayout {
    nodes: Vec<NodeLayout>,
    members: Vec<MemberLayout>,
}
fn capture_mount(graph: QueryGraph) -> io::Result<(Box<GraphIo>, GraphLayout, ffi::GraphMount)> {
    let metadata = match &graph {
        QueryGraph::Local(graph) => graph.schema().map_err(io::Error::other)?,
        QueryGraph::Remote {
            graph, original, ..
        } => {
            original.check()?;
            graph.schema()
        }
    };
    let mut files = BTreeMap::new();
    let mut handles = BTreeMap::<String, String>::new();
    let mut handle = |relative: &str| {
        if let Some(handle) = handles.get(relative) {
            return handle.clone();
        }
        let opaque = format!("graphit-captured://{}.parquet", files.len());
        files.insert(opaque.clone(), relative.to_owned());
        handles.insert(relative.to_owned(), opaque.clone());
        opaque
    };
    let mut layout = GraphLayout {
        nodes: Vec::new(),
        members: Vec::new(),
    };
    let mut mount = ffi::GraphMount {
        nodes: Vec::new(),
        members: Vec::new(),
    };
    for (table, node) in metadata.node_tables().enumerate() {
        let table = u64::try_from(table).map_err(io::Error::other)?;
        layout.nodes.push(NodeLayout {
            table,
            label: node.label().to_owned(),
            primary_key: node.primary_key().to_owned(),
            columns: node
                .columns()
                .map(|(name, kind)| (name.to_owned(), kind.to_owned()))
                .collect(),
        });
        mount.nodes.push(ffi::NodeMount {
            table,
            rows: node.row_extent(),
            shards: node
                .shards()
                .map(|shard| ffi::ShardMount {
                    file: handle(shard.storage_file()),
                    row_base: shard.row_base(),
                    row_offset: shard.row_offset(),
                    rows: shard.rows(),
                })
                .collect(),
        });
    }
    let mut relationships = BTreeMap::<(String, String, String), BTreeSet<(String, String)>>::new();
    for (member, edge) in metadata.relationships().enumerate() {
        let kind = edge.kind().to_owned();
        let from = edge.from_label().to_owned();
        let to = edge.to_label().to_owned();
        let properties = edge
            .properties()
            .map(|(name, kind)| (name.to_owned(), kind.to_owned()))
            .collect::<Vec<_>>();
        relationships
            .entry((kind.clone(), from.clone(), to.clone()))
            .or_default()
            .extend(properties.iter().cloned());
        mount.members.push(ffi::MemberMount {
            member: u64::try_from(member).map_err(io::Error::other)?,
            kind,
            from_label: from,
            to_label: to,
            rows: edge.rows(),
            indices: handle(edge.indices_file()),
            indptr: handle(edge.indptr_file()),
            indices_row_offset: edge.indices_row_offset(),
            indptr_row_offset: edge.indptr_row_offset(),
            source_row_base: edge.source_row_base(),
            source_rows: edge.source_rows(),
            target_row_base: edge.target_row_base(),
            target_rows: edge.target_rows(),
            properties: properties
                .into_iter()
                .map(|(name, data_type)| ffi::PropertyType { name, data_type })
                .collect(),
        });
    }
    layout.members = relationships
        .into_iter()
        .map(|((kind, from, to), properties)| MemberLayout {
            kind,
            from,
            to,
            properties: properties.into_iter().collect(),
        })
        .collect();
    Ok((
        Box::new(GraphIo {
            graph,
            files: Arc::new(files),
        }),
        layout,
        mount,
    ))
}

// SAFETY: the official Database is not thread-affine. The slot moves it only
// while its mutex grants exclusive ownership; every operation joins the native
// observer and releases connection, readers and IO before the guard is dropped.
#[allow(unsafe_code)]
unsafe impl Send for ffi::NativeCatalog {}

/// Test-only evidence from actual native constructions/DDL and the last call's
/// weak reader/Original witnesses. It retains no request or reader owner.
#[cfg(feature = "test-support")]
#[derive(Default)]
pub struct QueryCatalogObserver {
    witness: std::sync::Mutex<Option<QueryCatalogWitness>>,
    databases: std::sync::atomic::AtomicUsize,
    ddl: std::sync::atomic::AtomicUsize,
}
#[cfg(feature = "test-support")]
#[derive(Clone)]
pub struct QueryCatalogWitness {
    pub original: std::sync::Weak<SnapshotContext>,
    pub local: Option<std::sync::Weak<CapturedLocalGraph>>,
    pub remote: Option<std::sync::Weak<RemoteIcebugGraph>>,
}
#[cfg(feature = "test-support")]
impl QueryCatalogObserver {
    /// Last operation only; these weak references grant no authority and retain no reader.
    pub fn last_witness(&self) -> Option<QueryCatalogWitness> {
        self.witness
            .lock()
            .expect("query test observer poisoned")
            .clone()
    }
    fn record(&self, graph: &QueryGraph, original: &Arc<SnapshotContext>) {
        let (local, remote) = match graph {
            QueryGraph::Local(graph) => (Some(Arc::downgrade(graph)), None),
            QueryGraph::Remote { graph, .. } => (None, Some(Arc::downgrade(graph))),
        };
        *self.witness.lock().expect("query test observer poisoned") = Some(QueryCatalogWitness {
            original: Arc::downgrade(original),
            local,
            remote,
        });
    }
    /// Successful native Database constructions and completed CREATE DDL statements.
    pub fn counts(&self) -> (usize, usize) {
        (
            self.databases.load(std::sync::atomic::Ordering::SeqCst),
            self.ddl.load(std::sync::atomic::Ordering::SeqCst),
        )
    }
}
/// Internal lazy execution state for a service owner. The same native Database
/// survives compatible calls; each call binds its own admitted capture and readers.
pub struct QueryCatalog {
    #[cfg(feature = "test-support")]
    observer: Arc<QueryCatalogObserver>,
    state: tokio::sync::Mutex<CatalogState>,
}
struct CatalogState {
    native: cxx::UniquePtr<ffi::NativeCatalog>,
    logical_schema: Option<GraphLayout>,
    native_settings: Option<(u64, u64)>,
}
impl Default for CatalogState {
    fn default() -> Self {
        Self {
            native: cxx::UniquePtr::null(),
            logical_schema: None,
            native_settings: None,
        }
    }
}
impl CatalogState {
    fn clear(&mut self) {
        self.native = cxx::UniquePtr::null();
        self.logical_schema = None;
        self.native_settings = None;
    }
}
fn signal(original: &Arc<SnapshotContext>, runtime: &Handle) -> Box<QuerySignal> {
    Box::new(QuerySignal {
        #[cfg(feature = "test-support")]
        observer: None,
        original: Arc::clone(original),
        complete: Cancellation::default(),
        runtime: runtime.clone(),
    })
}
impl QueryGraph {
    fn revalidate(&self, original: &SnapshotContext) -> io::Result<()> {
        original.check()?;
        match self {
            Self::Local(graph) => graph.revalidate(original).map_err(io::Error::other),
            Self::Remote { .. } => Ok(()),
        }
    }
}
impl Default for QueryCatalog {
    fn default() -> Self {
        Self::new()
    }
}
impl QueryCatalog {
    /// Empty execution state. No IO, selection, request or credentials are captured.
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "test-support")]
            observer: Arc::new(QueryCatalogObserver::default()),
            state: tokio::sync::Mutex::new(CatalogState::default()),
        }
    }
    #[cfg(feature = "test-support")]
    pub fn test_observer(&self) -> Arc<QueryCatalogObserver> {
        Arc::clone(&self.observer)
    }
    fn signal(&self, original: &Arc<SnapshotContext>, runtime: &Handle) -> Box<QuerySignal> {
        let control = signal(original, runtime);
        #[cfg(feature = "test-support")]
        let control = {
            let mut control = control;
            control.observer = Some(Arc::clone(&self.observer));
            control
        };
        control
    }
    /// Called on the consumer's blocking job while it retains admission and shards.
    pub fn execute(
        &self,
        graph: &Arc<CapturedLocalGraph>,
        original: &Arc<SnapshotContext>,
        runtime: &Handle,
        query: &str,
        parameters: &BTreeMap<String, Value>,
        options: QueryOptions,
    ) -> io::Result<QueryResult> {
        self.execute_graph(
            QueryGraph::Local(Arc::clone(graph)),
            original,
            runtime,
            query,
            parameters,
            options,
        )
    }
    pub fn execute_remote(
        &self,
        graph: &Arc<RemoteIcebugGraph>,
        original: &Arc<SnapshotContext>,
        runtime: &Handle,
        query: &str,
        parameters: &BTreeMap<String, Value>,
        options: QueryOptions,
    ) -> io::Result<QueryResult> {
        self.execute_graph(
            QueryGraph::Remote {
                graph: Arc::clone(graph),
                original: Arc::clone(original),
                runtime: runtime.clone(),
            },
            original,
            runtime,
            query,
            parameters,
            options,
        )
    }
    fn execute_graph(
        &self,
        graph: QueryGraph,
        original: &Arc<SnapshotContext>,
        runtime: &Handle,
        query: &str,
        parameters: &BTreeMap<String, Value>,
        options: QueryOptions,
    ) -> io::Result<QueryResult> {
        original.check()?;
        options.check()?;
        if query.len() > options.max_bytes {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let parameters = serde_json::to_string(parameters).map_err(io::Error::other)?;
        if parameters.len() > options.max_bytes {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        // Keep the existing runtime and Original while waiting; never block a
        // cancelled/expired request behind another operation's native query.
        let mut state = runtime.block_on(async {
            tokio::select! {
                biased;
                () = original.cancellation().cancelled() => Err(io::Error::from(io::ErrorKind::Interrupted)),
                result = async {
                    match original.deadline() {
                        Some(deadline) => tokio::time::timeout_at(deadline.into(),self.state.lock())
                            .await.map_err(|_| io::Error::from(io::ErrorKind::TimedOut)),
                        None => Ok(self.state.lock().await),
                    }
                } => result,
            }
        })?;
        graph.revalidate(original)?;
        #[cfg(feature = "test-support")]
        self.observer.record(&graph, original);
        let current = graph.clone();
        let (io, logical, mount) = capture_mount(graph)?;
        let query_io = Box::new(GraphIo {
            graph: current.clone(),
            files: Arc::clone(&io.files),
        });
        if state.native.is_null()
            || state.logical_schema.as_ref() != Some(&logical)
            || state.native_settings != Some((options.buffer_pool_bytes, options.query_threads))
        {
            state.clear();
            let schema = serde_json::to_string(&logical).map_err(io::Error::other)?;
            let native =
                ffi::prepare_catalog(io, self.signal(original, runtime), &schema, mount, &options);
            original.check()?;
            let native = native.map_err(io::Error::other)?;
            if native.is_null() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            state.native = native;
            state.logical_schema = Some(logical);
            state.native_settings = Some((options.buffer_pool_bytes, options.query_threads));
        } else {
            // Capture A's tables/readers leave before capture B is bound, even
            // when their relative paths happen to be equal. No snapshot key is used.
            let rebound = ffi::rebind_catalog(
                state.native.pin_mut(),
                io,
                self.signal(original, runtime),
                mount,
            );
            if let Err(error) = original
                .check()
                .and_then(|()| rebound.map_err(io::Error::other))
            {
                state.clear();
                return Err(error);
            }
        }
        let mut result = ResultSink::new(options)?;
        let value = ffi::run_query(
            &state.native,
            query_io,
            self.signal(original, runtime),
            query,
            &parameters,
            &options,
            &mut result,
        );
        current.revalidate(original)?;
        value.map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        Ok(result.result)
    }
}

/// Execute against captured files in the caller's blocking job. Callers with an
/// existing snapshot owner retain QueryCatalog to amortize catalogue preparation.
pub fn execute(
    graph: &Arc<CapturedLocalGraph>,
    original: &Arc<SnapshotContext>,
    runtime: &Handle,
    query: &str,
    parameters: &BTreeMap<String, Value>,
    options: QueryOptions,
) -> io::Result<QueryResult> {
    QueryCatalog::new().execute(graph, original, runtime, query, parameters, options)
}

/// Execute the same official engine directly over captured remote range handles.
/// The consumer retains its current admission/lease through the blocking job.
pub fn execute_remote(
    graph: &Arc<RemoteIcebugGraph>,
    original: &Arc<SnapshotContext>,
    runtime: &Handle,
    query: &str,
    parameters: &BTreeMap<String, Value>,
    options: QueryOptions,
) -> io::Result<QueryResult> {
    QueryCatalog::new().execute_remote(graph, original, runtime, query, parameters, options)
}

#[cfg(test)]
mod tests;
