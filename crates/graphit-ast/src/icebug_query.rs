//! Operation-local Cypher through the official engine and captured file handles.
use graphit_core::{
    artifact_store::{CapturedSourceFile, SnapshotContext},
    file_lock::Cancellation,
};
use graphit_data::{
    CapturedLocalGraph,
    remote_graph::{RemoteGraphFile, RemoteIcebugGraph},
};
// Retain the SDK crate's standard native link metadata for this CXX consumer.
use lbug as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
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
    files: BTreeMap<String, String>,
}
enum QueryFile {
    Local(CapturedSourceFile),
    Remote {
        file: RemoteGraphFile,
        original: Arc<SnapshotContext>,
        runtime: Handle,
    },
}
impl GraphIo {
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
    fn identity(
        &mut self,
        field: &str,
        generation: &[u8],
        index: u64,
        row: u64,
        edge: bool,
    ) -> io::Result<()> {
        if generation.len() != 32 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let key = if edge { "member" } else { "table" };
        let value = json!({"type":"INTERNAL_ID","value":{"generation":generation,key:index,"row":row.to_string()}});
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
        fn open(self: &GraphIo, handle: &str) -> Result<Box<QueryFile>>;
        fn contains(self: &GraphIo, handle: &str) -> bool;
        fn len(self: &QueryFile) -> u64;
        fn read_at(self: &QueryFile, offset: u64, bytes: &mut [u8]) -> Result<usize>;
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
            generation: &[u8],
            index: u64,
            row: u64,
            edge: bool,
        ) -> Result<()>;
        fn end_row(self: &mut ResultSink) -> Result<()>;
    }
    unsafe extern "C++" {
        include!("native/icebug_query.hpp");
        fn run_query(
            io: Box<GraphIo>,
            signal: Box<QuerySignal>,
            schema: &str,
            query: &str,
            parameters: &str,
            options: &QueryOptions,
            result: &mut ResultSink,
        ) -> Result<()>;
    }
}

fn mount_schema(
    graph: QueryGraph,
    generation: Option<[u8; 32]>,
) -> io::Result<(Box<GraphIo>, String)> {
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
    let mut handle = |relative: &str| {
        let opaque = format!("graphit-captured://{}.parquet", files.len());
        files.insert(opaque.clone(), relative.to_owned());
        opaque
    };
    let nodes = metadata
        .node_tables()
        .enumerate()
        .map(|(table, node)| {
            json!({"table":table,"label":node.label(),"primary_key":node.primary_key(),
            "rows":node.row_extent(),"shards":node.shards().map(|shard| json!({
                "file":handle(shard.storage_file()),"row_base":shard.row_base(),"rows":shard.rows()
            })).collect::<Vec<_>>(),
            "columns":node.columns().collect::<Vec<_>>()})
        })
        .collect::<Vec<_>>();
    let members = metadata.relationships().enumerate().map(|(member, edge)| {
        json!({"member":member,"kind":edge.kind(),"from":edge.from_label(),"to":edge.to_label(),
            "rows":edge.rows(),"indices":handle(edge.indices_file()),"indptr":handle(edge.indptr_file()),
            "source_row_base":edge.source_row_base(),"source_rows":edge.source_rows(),
            "target_row_base":edge.target_row_base(),"target_rows":edge.target_rows(),
            "properties":edge.properties().collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    let generation = generation.unwrap_or_else(|| metadata.generation().bytes());
    let schema =
        serde_json::to_string(&json!({"generation":generation,"nodes":nodes,"members":members}))
            .map_err(io::Error::other)?;
    Ok((Box::new(GraphIo { graph, files }), schema))
}

/// Execute against genuine captured files on the caller's blocking job.
/// Keep the same Original and live Tokio runtime through completion; PROJECT
/// or installed-artifact authorization belongs to the composing consumer.
pub fn execute(
    graph: &Arc<CapturedLocalGraph>,
    original: &Arc<SnapshotContext>,
    runtime: &Handle,
    query: &str,
    parameters: &BTreeMap<String, Value>,
    options: QueryOptions,
) -> io::Result<QueryResult> {
    execute_graph(
        QueryGraph::Local(Arc::clone(graph)),
        original,
        runtime,
        query,
        parameters,
        options,
        None,
    )
}

/// Execute on a captured producer-owned graph. The caller retains the graph's
/// shard lifetime for this operation; unrelated derived corpora are independent.
pub fn execute_owned(
    graph: &Arc<CapturedLocalGraph>,
    original: &Arc<SnapshotContext>,
    namespace: [u8; 32],
    query: &str,
    parameters: &BTreeMap<String, Value>,
    options: QueryOptions,
) -> io::Result<QueryResult> {
    let generation = graph
        .owned_generation(namespace)
        .map_err(io::Error::other)?;
    execute_graph(
        QueryGraph::Local(Arc::clone(graph)),
        original,
        &Handle::try_current().map_err(io::Error::other)?,
        query,
        parameters,
        options,
        Some(generation),
    )
}

/// Execute the same official engine directly over captured remote range handles.
/// The consumer must retain its current admission/lease and own this blocking job
/// through completion. No graph download or ambient storage credentials are used.
pub fn execute_remote(
    graph: &Arc<RemoteIcebugGraph>,
    original: &Arc<SnapshotContext>,
    runtime: &Handle,
    query: &str,
    parameters: &BTreeMap<String, Value>,
    options: QueryOptions,
) -> io::Result<QueryResult> {
    execute_graph(
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
        None,
    )
}

fn execute_graph(
    graph: QueryGraph,
    original: &Arc<SnapshotContext>,
    runtime: &Handle,
    query: &str,
    parameters: &BTreeMap<String, Value>,
    options: QueryOptions,
    generation: Option<[u8; 32]>,
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
    let (io, schema) = mount_schema(graph, generation)?;
    let signal = Box::new(QuerySignal {
        original: Arc::clone(original),
        complete: Cancellation::default(),
        runtime: runtime.clone(),
    });
    let mut result = ResultSink::new(options)?;
    let value = ffi::run_query(
        io,
        signal,
        &schema,
        query,
        &parameters,
        &options,
        &mut result,
    );
    original.check()?;
    value.map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    Ok(result.result)
}

#[cfg(test)]
mod tests;
