//! Mutable FILE publications over immutable Icebug Parquet shards.
//! The composing caller owns PROJECT/version admission and the exact private directory.
use crate::icebug_query::{QueryOptions, QueryResult};
use graphit_core::file_io::SnapshotContext;
use graphit_data::graph_writer::{GraphEdge, GraphNode, GraphValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, Value as Json, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
mod oracle;
mod pending;
mod portable;
mod scip;
mod scip_export;
mod snapshot;
#[cfg(test)]
#[path = "live_graph/tests.rs"]
mod tests;
mod validation;
use portable::{Catalog, FileFacts, Index};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiveGraphSchema {
    pub revision: u64,
    pub nodes: BTreeMap<String, BTreeMap<String, String>>,
    pub relationships: BTreeMap<String, BTreeSet<(String, String)>>,
}

#[derive(Clone, Copy, Debug)]
pub enum LiveGraphDirection {
    Incoming,
    Outgoing,
    Both,
}

pub struct LiveFileInput<'a> {
    pub path: &'a str,
    pub source_sha256: [u8; 32],
    pub raw_version: u64,
    pub nodes: &'a [GraphNode],
    pub edges: &'a [GraphEdge],
    pub definitions: &'a [crate::semantic_graph::scip_document::AstScipDocumentDefinition],
    pub references: &'a [crate::semantic_graph::scip_document::AstScipDocumentReference],
    pub scip_document: Option<&'a [u8]>,
    pub oracle: &'a [crate::semantic_graph::oracle::AstOracleGraphRows],
    pub oracle_file: Option<&'a crate::semantic_graph::AstGraphNode>,
    /// Original normalized parser batches awaiting their compiler; no tree or vectors.
    pub pending_compiler: Option<&'a str>,
    /// Exact selected Docker image; None for syntax-only extraction.
    pub scip_compiler: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LiveFileObservation {
    pub path: String,
    pub source_sha256: [u8; 32],
    pub file_revision: u64,
    pub raw_version: u64,
    pub scip_compiler: Option<String>,
}

#[derive(Debug)]
struct FileMutationError {
    path: String,
    cause: io::Error,
}
impl std::fmt::Display for FileMutationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "FILE {:?}: {}", self.path, self.cause)
    }
}
impl std::error::Error for FileMutationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

pub struct LiveGraph {
    directory: PathBuf,
    namespace: [u8; 32],
    state: Mutex<portable::State>,
    closed: tokio::sync::watch::Sender<()>,
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn ident(value: &str) -> io::Result<String> {
    if value.is_empty() || value.contains('\0') || value.starts_with("_Graphit") {
        return Err(invalid("invalid or reserved graph identifier"));
    }
    Ok(format!("`{}`", value.replace('`', "``")))
}
fn encoded<T: Serialize>(value: &T) -> io::Result<String> {
    serde_json::to_string(value).map_err(io::Error::other)
}
fn decoded<T: serde::de::DeserializeOwned>(value: &str) -> io::Result<T> {
    serde_json::from_str(value).map_err(|error| invalid(error.to_string()))
}
fn node_text<'a>(node: &'a GraphNode, key: &str) -> Option<&'a str> {
    match node.properties.get(key) {
        Some(GraphValue::String(value)) => Some(value),
        _ => None,
    }
}
fn ast_node(node: &GraphNode) -> io::Result<crate::semantic_graph::AstGraphNode> {
    use crate::semantic_graph::AstGraphValue;
    let properties = node
        .properties
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                match value {
                    GraphValue::String(value) => AstGraphValue::String(value.clone()),
                    GraphValue::Int64(value) => AstGraphValue::Int64(*value),
                    GraphValue::Bool(value) => AstGraphValue::Bool(*value),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    serde_json::from_value(json!({"label":node.label,"key":node.key,"properties":properties}))
        .map_err(io::Error::other)
}
fn update_outcomes(
    node: &mut GraphNode,
    file: &crate::semantic_graph::AstGraphNode,
) -> io::Result<()> {
    let Some(crate::semantic_graph::AstGraphValue::String(outcomes)) =
        file.properties().get("resolution_outcomes_json")
    else {
        return Err(invalid("resolution produced no FILE outcomes"));
    };
    node.properties.insert(
        "resolution_outcomes_json".into(),
        GraphValue::String(outcomes.clone()),
    );
    Ok(())
}
impl LiveGraph {
    pub fn open_path_owned(
        path: PathBuf,
        original: &Arc<SnapshotContext>,
        namespace: [u8; 32],
        create: bool,
    ) -> io::Result<Self> {
        original.check()?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => (),
            Ok(_) => return Err(invalid("local graph is not a private Icebug directory")),
            Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                std::fs::create_dir(&path)?
            }
            Err(error) => return Err(error),
        }
        let manifest = path.join("icebug.json");
        let catalog = match std::fs::File::open(&manifest) {
            Ok(file) => {
                let manifest: Value =
                    serde_json::from_reader(file).map_err(|error| invalid(error.to_string()))?;
                serde_json::from_value::<Catalog>(
                    manifest
                        .get("live")
                        .cloned()
                        .ok_or_else(|| invalid("local graph has no FILE catalogue"))?,
                )
                .map_err(|error| invalid(error.to_string()))?
            }
            Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                let catalog = Catalog::new(namespace);
                portable::publish(&path, &catalog, original)?;
                catalog
            }
            Err(error) => return Err(error),
        };
        if catalog.namespace != namespace {
            return Err(invalid("local graph belongs to another PROJECT/version"));
        }
        original.check()?;
        Ok(Self {
            directory: path,
            namespace,
            state: Mutex::new(portable::State {
                catalog,
                index: None,
            }),
            closed: tokio::sync::watch::channel(()).0,
        })
    }
    pub fn close_notification(&self) -> tokio::sync::watch::Receiver<()> {
        self.closed.subscribe()
    }
    pub fn revision(&self) -> io::Result<u64> {
        Ok(self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?
            .catalog
            .schema
            .revision)
    }
    pub fn schema(&self) -> io::Result<LiveGraphSchema> {
        Ok(self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?
            .catalog
            .schema
            .clone())
    }
    pub fn namespace(&self) -> [u8; 32] {
        self.namespace
    }
    pub fn generation(&self) -> io::Result<[u8; 32]> {
        let mut hash = Sha256::new();
        hash.update(self.namespace);
        hash.update(self.revision()?.to_le_bytes());
        Ok(hash.finalize().into())
    }
    pub fn files(&self, original: &Arc<SnapshotContext>) -> io::Result<Vec<LiveFileObservation>> {
        self.files_under("", original)
    }
    pub fn files_under(
        &self,
        prefix: &str,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Vec<LiveFileObservation>> {
        if prefix.contains('\0') || prefix.ends_with('/') {
            return Err(invalid("invalid FILE prefix"));
        }
        original.check()?;
        let state = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        state
            .catalog
            .files
            .range(prefix.to_owned()..)
            .take_while(|(path, _)| path.starts_with(prefix))
            .filter(|(path, _)| {
                prefix.is_empty()
                    || path.as_str() == prefix
                    || path
                        .strip_prefix(prefix)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
            .map(|(_, file)| {
                original.check()?;
                Ok(file.observation.clone())
            })
            .collect()
    }
    pub fn file_observation(
        &self,
        path: &str,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<Option<LiveFileObservation>> {
        original.check()?;
        Ok(self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?
            .catalog
            .files
            .get(path)
            .map(|file| file.observation.clone()))
    }
    pub fn replace_file(
        &self,
        input: LiveFileInput<'_>,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<u64> {
        self.mutate(input.path, Some(input), None, original)
    }
    pub fn enrich_file(
        &self,
        input: LiveFileInput<'_>,
        expected: u64,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<u64> {
        self.mutate(input.path, Some(input), Some(expected), original)
    }
    pub fn remove_file(&self, path: &str, original: &Arc<SnapshotContext>) -> io::Result<u64> {
        self.mutate(path, None, None, original)
    }
    fn mutate(
        &self,
        path: &str,
        replacement: Option<LiveFileInput<'_>>,
        expected: Option<u64>,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<u64> {
        portable::mutate(self, path, replacement, expected, original)
    }
    pub fn query(
        &self,
        query: &str,
        parameters: &BTreeMap<String, Json>,
        options: QueryOptions,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<QueryResult> {
        self.with_graph(original, |graph| {
            crate::icebug_query::execute_owned(
                graph,
                original,
                self.namespace,
                query,
                parameters,
                options,
            )
        })?
    }
    /// Perform one graph operation against the captured immutable shards.
    /// The guard prevents their reclamation during this operation; it does not
    /// coordinate Source, search or embedding publication.
    pub fn with_graph<T>(
        &self,
        original: &Arc<SnapshotContext>,
        read: impl FnOnce(&Arc<graphit_data::CapturedLocalGraph>) -> T,
    ) -> io::Result<T> {
        use graphit_core::{brand::Brand, config::ConfigSnapshot};
        original.check()?;
        let _snapshot = self
            .state
            .lock()
            .map_err(|_| invalid("graph writer poisoned"))?;
        let config = ConfigSnapshot::new(&Brand::default(), [])?;
        let policy = graphit_data::LocalGraphPolicy::resolve(&config, original)?;
        let graph = Arc::new(
            graphit_data::CapturedLocalGraph::capture_generated_metadata(
                &self.directory,
                Arc::clone(original),
                policy,
            )
            .map_err(io::Error::other)?,
        );
        graph
            .owned_generation(self.namespace)
            .map_err(io::Error::other)?;
        let result = read(&graph);
        original.check()?;
        Ok(result)
    }
    /// Schema inspection shares the native graph capture without a query.
    pub fn with_schema<T>(
        &self,
        original: &Arc<SnapshotContext>,
        read: impl FnOnce(&graphit_data::GraphSchema<'_>) -> T,
    ) -> io::Result<T> {
        self.with_graph(original, |graph| {
            let schema = graph.schema().map_err(io::Error::other)?;
            Ok(read(&schema))
        })?
    }
    pub fn sample(
        &self,
        limit: usize,
        options: QueryOptions,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<QueryResult> {
        if limit == 0 || limit > options.max_rows {
            return Err(invalid("invalid graph sample size"));
        }
        self.query(
            "MATCH (n) RETURN n LIMIT $limit",
            &BTreeMap::from([("limit".into(), limit.into())]),
            options,
            original,
        )
    }
    pub fn neighborhood(
        &self,
        label: &str,
        key: &str,
        direction: LiveGraphDirection,
        limit: usize,
        options: QueryOptions,
        original: &Arc<SnapshotContext>,
    ) -> io::Result<QueryResult> {
        if limit == 0 || limit > options.max_rows {
            return Err(invalid("invalid graph neighborhood size"));
        }
        let pattern = match direction {
            LiveGraphDirection::Outgoing => "-[r]->(m)",
            LiveGraphDirection::Incoming => "<-[r]-(m)",
            LiveGraphDirection::Both => "-[r]-(m)",
        };
        self.query(
            &format!(
                "MATCH (n:{} {{entity_key:$key}}){pattern} RETURN n,r,m LIMIT $limit",
                ident(label)?
            ),
            &BTreeMap::from([("key".into(), key.into()), ("limit".into(), limit.into())]),
            options,
            original,
        )
    }
}
