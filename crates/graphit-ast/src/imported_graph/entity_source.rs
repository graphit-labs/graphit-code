//! Entity windows over the graph and raw Source of one captured native payload.
use super::{ImportedGraph, ImportedGraphError, observe_directory};
use crate::{
    entity_source::select_entity_source,
    icebug_query::{QueryOptions, QueryResult},
    indexed_source::{IndexedSourceError, read_indexed_source, source_schema},
    source_inspection::{IndexedSource, SourceInspection, SourceInspectionError, SourceOrigin},
};
use graphit_core::{native_document_file::CapturedDocumentDirectory, text_slice};
use graphit_data::lance_module_store::{LanceModuleStore, StoreError};
use serde_json::Value;
use std::{collections::BTreeMap, error::Error, fmt, io};
use tokio::runtime::Handle;

#[derive(Debug)]
pub enum EntitySourceError {
    InvalidRequest,
    Unavailable,
    Ambiguous,
    UnknownType,
    InvalidExtent,
    Imported(Box<ImportedGraphError>),
    Storage(Box<StoreError>),
    Indexed(Box<IndexedSourceError>),
    Selection(SourceInspectionError),
    Operation(io::Error),
}
impl fmt::Display for EntitySourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidRequest => "invalid entity Source request",
            Self::Unavailable => "entity or bound Source unavailable",
            Self::Ambiguous => "entity Source selection is ambiguous",
            Self::UnknownType => "entity Source type unavailable",
            Self::InvalidExtent => "invalid entity Source extent",
            Self::Imported(_) => "entity Source graph acquisition or query failed",
            Self::Storage(_) => "entity Source storage failed",
            Self::Indexed(_) => "entity Source indexed text failed",
            Self::Selection(_) => "entity Source selection failed",
            Self::Operation(_) => "entity Source operation failed",
        })
    }
}
impl Error for EntitySourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Imported(e) => Some(e.as_ref()),
            Self::Storage(e) => Some(e.as_ref()),
            Self::Indexed(e) => Some(e.as_ref()),
            Self::Selection(e) => Some(e),
            Self::Operation(e) => Some(e),
            _ => None,
        }
    }
}
impl From<ImportedGraphError> for EntitySourceError {
    fn from(e: ImportedGraphError) -> Self {
        Self::Imported(Box::new(e))
    }
}
impl From<io::Error> for EntitySourceError {
    fn from(e: io::Error) -> Self {
        Self::Operation(e)
    }
}

fn line(cell: &Value) -> Result<usize, EntitySourceError> {
    let object = cell.as_object().ok_or(EntitySourceError::InvalidExtent)?;
    if object.len() != 2 || object.get("type").and_then(Value::as_str) != Some("INT64") {
        return Err(EntitySourceError::InvalidExtent);
    }
    let value = object
        .get("value")
        .and_then(Value::as_str)
        .ok_or(EntitySourceError::InvalidExtent)?;
    let line = value
        .parse::<u64>()
        .ok()
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v > 0)
        .ok_or(EntitySourceError::InvalidExtent)?;
    Ok(line)
}

fn rows(result: QueryResult) -> Result<Vec<(usize, usize)>, EntitySourceError> {
    if result.columns != ["source_start", "source_end"] || result.rows.len() > 2 {
        return Err(EntitySourceError::InvalidExtent);
    }
    result
        .rows
        .into_iter()
        .map(|row| {
            if row.len() != 2 {
                return Err(EntitySourceError::InvalidExtent);
            }
            let start = line(&row[0])?;
            let end = line(&row[1])?;
            if start > end {
                return Err(EntitySourceError::InvalidExtent);
            }
            Ok((start, end))
        })
        .collect()
}

/// Resolve an observed extent from the supplied captured imported graph.
/// Run inside the transport's existing Data job while its read grant is retained.
pub fn resolve_imported_entity_window(
    graph: &std::sync::Arc<graphit_data::CapturedLocalGraph>,
    original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
    runtime: &Handle,
    path: &str,
    entity: &str,
    entity_type: Option<&str>,
    options: QueryOptions,
) -> Result<(usize, usize), EntitySourceError> {
    validate_entity_request(path, entity, entity_type, options)?;
    original.check()?;
    let schema = graph.schema().map_err(ImportedGraphError::Graph)?;
    resolve_window(
        schema,
        original,
        path,
        entity,
        entity_type,
        |query, parameters| {
            crate::icebug_query::execute(graph, original, runtime, query, parameters, options)
        },
    )
}

/// Resolve the same entity extent over captured remote table range handles.
/// The composing installed consumer retains its actual admission and service lease.
pub fn resolve_remote_entity_window(
    graph: &std::sync::Arc<graphit_data::remote_graph::RemoteIcebugGraph>,
    original: &std::sync::Arc<graphit_core::file_io::SnapshotContext>,
    runtime: &Handle,
    path: &str,
    entity: &str,
    entity_type: Option<&str>,
    options: QueryOptions,
) -> Result<(usize, usize), EntitySourceError> {
    validate_entity_request(path, entity, entity_type, options)?;
    original.check()?;
    resolve_window(
        graph.schema(),
        original,
        path,
        entity,
        entity_type,
        |query, parameters| {
            crate::icebug_query::execute_remote(
                graph, original, runtime, query, parameters, options,
            )
        },
    )
}

fn validate_entity_request(
    path: &str,
    entity: &str,
    entity_type: Option<&str>,
    options: QueryOptions,
) -> Result<(), EntitySourceError> {
    if path.is_empty()
        || path.contains('\0')
        || entity.is_empty()
        || entity.contains('\0')
        || entity_type.is_some_and(|s| s.is_empty() || s.contains('\0'))
        || options.max_rows < 2
    {
        return Err(EntitySourceError::InvalidRequest);
    }
    Ok(())
}

fn resolve_window(
    schema: graphit_data::GraphSchema<'_>,
    original: &graphit_core::file_io::SnapshotContext,
    path: &str,
    entity: &str,
    entity_type: Option<&str>,
    mut execute: impl FnMut(&str, &BTreeMap<String, Value>) -> io::Result<QueryResult>,
) -> Result<(usize, usize), EntitySourceError> {
    original.check()?;
    let mut labels = Vec::new();
    let mut known_type = false;
    for table in schema.node_tables() {
        if let Some(kind) = entity_type {
            if table.label() != kind {
                continue;
            }
            known_type = true;
        }
        let fields: BTreeMap<_, _> = table.columns().collect();
        if fields.get("name") == Some(&"STRING")
            && fields.get("path") == Some(&"STRING")
            && fields.get("start_line") == Some(&"INT64")
            && fields.get("end_line") == Some(&"INT64")
            && fields.get("available") == Some(&"BOOL")
            && fields.get("extent_status") == Some(&"STRING")
        {
            labels.push(table.label().to_owned());
        } else if entity_type.is_some() {
            return Err(EntitySourceError::InvalidExtent);
        }
    }
    if entity_type.is_some() && !known_type {
        return Err(EntitySourceError::UnknownType);
    }
    let parameters = BTreeMap::from([
        ("path".into(), Value::String(path.to_owned())),
        ("entity".into(), Value::String(entity.to_owned())),
    ]);
    for folded in [false, true] {
        let mut found = None;
        for label in &labels {
            original.check()?;
            // Values are parameters; only captured schema identifiers are quoted.
            let label = label.replace('`', "``");
            let name = if folded {
                "lower(n.name) = lower($entity)"
            } else {
                "n.name = $entity"
            };
            let query = format!(
                "MATCH (n:`{label}`) WHERE n.path = $path AND {name} AND n.available = true AND n.extent_status = 'Exact' RETURN n.start_line AS source_start, n.end_line AS source_end LIMIT 2"
            );
            for window in rows(execute(&query, &parameters)?)? {
                if found.replace(window).is_some() {
                    return Err(EntitySourceError::Ambiguous);
                }
            }
        }
        if let Some(window) = found {
            return Ok(window);
        }
    }
    Err(EntitySourceError::Unavailable)
}

impl ImportedGraph<'_, '_> {
    fn entity_window(
        &self,
        runtime: &Handle,
        path: &str,
        entity: &str,
        entity_type: Option<&str>,
        options: QueryOptions,
    ) -> Result<(usize, usize), EntitySourceError> {
        self.revalidate()?;
        let result = resolve_imported_entity_window(
            &self.graph,
            self.input.request.snapshot(),
            runtime,
            path,
            entity,
            entity_type,
            options,
        );
        self.revalidate()?;
        result
    }

    /// Select an entity from this same registered native graph/raw payload.
    ///
    /// Call on the caller's owned blocking job. Missing raw text does not permit
    /// applying artifact coordinates to unrelated physical workspace bytes.
    pub async fn entity_source(
        &self,
        runtime: &Handle,
        path: &str,
        entity: &str,
        entity_type: Option<&str>,
        selection: &text_slice::Request,
        options: QueryOptions,
    ) -> Result<SourceInspection, EntitySourceError> {
        self.input.request.snapshot().check()?;
        self.revalidate()?;
        if path.is_empty()
            || path.contains('\0')
            || entity.is_empty()
            || entity.contains('\0')
            || entity_type.is_some_and(|v| v.is_empty() || v.contains('\0'))
            || options.max_rows < 2
        {
            return Err(EntitySourceError::InvalidRequest);
        }
        let original = self.input.request.snapshot();
        let source_path = self.directory.canonical_path().join("source.lance");
        match std::fs::symlink_metadata(&source_path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(EntitySourceError::Unavailable);
            }
            Err(e) => return Err(e.into()),
            Ok(_) => (),
        }
        observe_directory(&source_path)?;
        let directory = CapturedDocumentDirectory::capture(
            &source_path,
            self.input.directory_limits,
            original,
        )?;
        if directory.canonical_path() != source_path {
            return Err(ImportedGraphError::InvalidOrigin.into());
        }
        self.revalidate()?;
        let uri = source_path
            .to_str()
            .ok_or(EntitySourceError::InvalidRequest)?;
        let store = LanceModuleStore::open(uri, source_schema()).await;
        directory.revalidate(original)?;
        self.revalidate()?;
        let snapshot = store
            .map_err(|e| EntitySourceError::Storage(Box::new(e)))?
            .snapshot();
        let (start, end) = self.entity_window(runtime, path, entity, entity_type, options)?;
        let indexed = read_indexed_source(&snapshot, path, original).await;
        directory.revalidate(original)?;
        self.revalidate()?;
        let indexed = indexed.map_err(|e| EntitySourceError::Indexed(Box::new(e)))?;
        let IndexedSource::Available(text) = indexed
            .source(original)
            .map_err(|e| EntitySourceError::Indexed(Box::new(e)))?
        else {
            return Err(EntitySourceError::Unavailable);
        };
        let result = select_entity_source(text, start, end, selection, original);
        directory.revalidate(original)?;
        self.schema()?;
        self.revalidate()?;
        original.check()?;
        Ok(SourceInspection {
            origin: SourceOrigin::Indexed,
            selection: result.map_err(EntitySourceError::Selection)?,
        })
    }
}
