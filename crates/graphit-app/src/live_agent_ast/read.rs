//! Reads capture only the graph, Source or corpus required by the operation.
use super::*;
use crate::{
    ast_source::SourceParameters,
    graph_server::{
        QueryData,
        search::{Authority, Input},
    },
};
use graphit_ast::{
    indexed_source::{MissingSourceReason, read_indexed_source},
    source_inspection::{
        IndexedSource, PhysicalSource, SourceInspection, SourceInspectionError, SourceOrigin,
        inspect_project_entity_source, inspect_source,
    },
};
use graphit_core::artifact_store::CapturedSourceReader;
use serde_json::{Value, json};
use std::path::Path;

#[cfg(all(test, target_os = "linux"))]
pub(crate) struct QueryJoin {
    /// CXX returned a successful real query before the blocking closure is held.
    pub(crate) started: tokio::sync::oneshot::Receiver<bool>,
    pub(crate) release: std::sync::mpsc::Sender<()>,
    pub(crate) finished: tokio::sync::oneshot::Receiver<()>,
}
#[cfg(all(test, target_os = "linux"))]
struct QueryJoinGate {
    original: Arc<SnapshotContext>,
    started: tokio::sync::oneshot::Sender<bool>,
    release: std::sync::mpsc::Receiver<()>,
    finished: tokio::sync::oneshot::Sender<()>,
}
#[cfg(all(test, target_os = "linux"))]
static QUERY_JOIN: std::sync::Mutex<Option<QueryJoinGate>> = std::sync::Mutex::new(None);

/// One targeted barrier in the real query closure. It adds no production state.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn test_query_join(original: &Arc<SnapshotContext>) -> io::Result<QueryJoin> {
    let mut installed = QUERY_JOIN.lock().expect("AST query test gate");
    if installed.is_some() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    let (started, entered) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let (finished, done) = tokio::sync::oneshot::channel();
    *installed = Some(QueryJoinGate {
        original: Arc::clone(original),
        started,
        release: wait,
        finished,
    });
    Ok(QueryJoin {
        started: entered,
        release,
        finished: done,
    })
}
#[cfg(all(test, target_os = "linux"))]
fn take_query_join(original: &Arc<SnapshotContext>) -> Option<QueryJoinGate> {
    let mut installed = QUERY_JOIN.lock().expect("AST query test gate");
    if installed
        .as_ref()
        .is_some_and(|gate| Arc::ptr_eq(&gate.original, original))
    {
        installed.take()
    } else {
        None
    }
}
#[cfg(all(test, target_os = "linux"))]
impl QueryJoinGate {
    fn after_execute(self, executed: bool) -> io::Result<()> {
        let _ = self.started.send(executed);
        let released = self.release.recv().map_err(io::Error::other);
        let _ = self.finished.send(());
        released
    }
}

impl CapturedAstContext<'_> {
    async fn project_index(
        &self,
    ) -> io::Result<Arc<crate::foreground_ast_sources::progressive::LiveIndex>> {
        let Reader::Project {
            capture,
            installation,
            ..
        } = &self.reader
        else {
            return Err(invalid());
        };
        capture
            .open_live(
                *installation,
                &self.server.live_ast(),
                self.source.policy(),
                self.server.project_config()?,
                &self.source.live_projects(),
            )
            .await
            .map_err(io::Error::other)?;
        self.recheck()?;
        capture.live_index().map(Arc::clone).ok_or_else(absent)
    }

    pub(crate) async fn schema(&self) -> io::Result<Value> {
        self.recheck()?;
        let value = match &self.graph {
            GraphPin::Project => {
                let index = self.project_index().await?;
                self.recheck()?;
                let bytes = crate::graph_server::live::render_live(
                    &index.graph,
                    &self.original,
                    crate::graph_server::Operation::Schema,
                    None,
                    None,
                    self.source.policy().graph_query,
                    self.source.policy().content.max_output_bytes,
                )
                .map_err(|error| match error {
                    crate::graph_server::ReadFailure::Query(error)
                    | crate::graph_server::ReadFailure::Output(error) => error,
                    _ => invalid(),
                })?;
                serde_json::from_slice(&bytes).map_err(io::Error::other)?
            }
            GraphPin::Local(graph) => crate::graph_server::schema_value(
                &graph.schema().map_err(io::Error::other)?,
                self.source.policy().content.max_output_bytes,
            )?,
            GraphPin::Remote(graph) => crate::graph_server::schema_value(
                &graph.schema(),
                self.source.policy().content.max_output_bytes,
            )?,
        };
        self.recheck()?;
        Ok(value)
    }
    pub(crate) async fn query(&self, input: QueryData) -> io::Result<Value> {
        self.recheck()?;
        let original = Arc::clone(&self.original);
        let runtime = tokio::runtime::Handle::current();
        let options = self.source.policy().graph_query;
        #[cfg(all(test, target_os = "linux"))]
        let join_gate = take_query_join(&original);
        let result = match &self.graph {
            GraphPin::Project => {
                let index = self.project_index().await?;
                self.recheck()?;
                let index = Arc::clone(&index);
                tokio::task::spawn_blocking(move || {
                    let result =
                        index
                            .graph
                            .query(&input.query, &input.parameters, options, &original);
                    #[cfg(all(test, target_os = "linux"))]
                    if let Some(gate) = join_gate {
                        gate.after_execute(result.is_ok())?;
                    }
                    result
                })
                .await
                .map_err(io::Error::other)??
            }
            GraphPin::Local(graph) => {
                let graph = Arc::clone(graph);
                tokio::task::spawn_blocking(move || {
                    let result = graphit_ast::icebug_query::execute(
                        &graph,
                        &original,
                        &runtime,
                        &input.query,
                        &input.parameters,
                        options,
                    );
                    #[cfg(all(test, target_os = "linux"))]
                    if let Some(gate) = join_gate {
                        gate.after_execute(result.is_ok())?;
                    }
                    result
                })
                .await
                .map_err(io::Error::other)??
            }
            GraphPin::Remote(graph) => {
                let graph = Arc::clone(graph);
                tokio::task::spawn_blocking(move || {
                    let result = graphit_ast::icebug_query::execute_remote(
                        &graph,
                        &original,
                        &runtime,
                        &input.query,
                        &input.parameters,
                        options,
                    );
                    #[cfg(all(test, target_os = "linux"))]
                    if let Some(gate) = join_gate {
                        gate.after_execute(result.is_ok())?;
                    }
                    result
                })
                .await
                .map_err(io::Error::other)??
            }
        };
        self.recheck()?;
        serde_json::to_value(result).map_err(io::Error::other)
    }
    pub(crate) async fn source(&self, input: &SourceParameters) -> io::Result<SourceInspection> {
        input
            .validate()
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
        self.recheck()?;
        let selectors = input.selection();
        let result = match &self.reader {
            Reader::Project {
                read, installation, ..
            } => {
                let raw = match open_local_source_index(
                    *installation,
                    *read.project().map_err(io::Error::other)?,
                    &self.original,
                    self.source.policy().index,
                )
                .await
                .map_err(io::Error::other)?
                {
                    SourceIndexPresence::Absent => None,
                    SourceIndexPresence::Available(raw) => Some(raw),
                };
                let entity = if input.entity.is_some() {
                    Some(self.project_entity(input).await?)
                } else {
                    None
                };
                let indexed = match &raw {
                    Some(raw) => Some(
                        raw.read(&input.path, &self.original)
                            .await
                            .map_err(io::Error::other)?,
                    ),
                    None => None,
                };
                if entity.is_some()
                    && indexed.as_ref().is_none_or(|indexed| {
                        indexed.missing_reason() == Some(MissingSourceReason::FileAbsent)
                    })
                {
                    return Err(io::Error::other(SourceInspectionError::Association));
                }
                let source = match &indexed {
                    Some(indexed) => indexed.source(&self.original).map_err(io::Error::other)?,
                    None => IndexedSource::Missing,
                };
                let physical = if matches!(source, IndexedSource::Missing) {
                    Some(CapturedSourceReader::capture_project(
                        read.project().map_err(io::Error::other)?.scope.live,
                        Path::new(&input.path),
                        graphit_fs::NodeKind::File,
                        &self.source.policy().file,
                        self.source.policy().read,
                        &self.original,
                    )?)
                } else {
                    None
                };
                let physical = physical.as_ref().map(|reader| PhysicalSource {
                    reader,
                    relative: Path::new(""),
                });
                if let Some(entity) = &entity {
                    inspect_project_entity_source(
                        source,
                        physical,
                        &selectors,
                        entity,
                        &self.original,
                    )
                    .map_err(io::Error::other)?
                } else {
                    inspect_source(source, physical, &selectors, &self.original)
                        .map_err(io::Error::other)?
                }
            }
            Reader::Imported { .. } | Reader::Remote { .. } => {
                let raw = match &self.reader {
                    Reader::Imported { graph, .. } => {
                        graph
                            .reader()
                            .map_err(io::Error::other)?
                            .open_lance_sidecar(
                                "source.lance",
                                graphit_ast::indexed_source::source_schema(),
                                &self.original,
                            )
                            .await?
                    }
                    Reader::Remote { opened, lease, .. } => opened
                        .mount
                        .source(&opened.selection, lease)
                        .await
                        .map_err(io::Error::other)?,
                    Reader::Project { .. } => unreachable!(),
                }
                .ok_or_else(absent)?;
                let window = if input.entity.is_some() {
                    Some(self.entity_window(input).await?)
                } else {
                    None
                };
                let indexed = read_indexed_source(&raw, &input.path, &self.original)
                    .await
                    .map_err(io::Error::other)?;
                let source = indexed.source(&self.original).map_err(io::Error::other)?;
                if let Some((start, end)) = window {
                    let IndexedSource::Available(text) = source else {
                        return Err(absent());
                    };
                    SourceInspection {
                        origin: SourceOrigin::Indexed,
                        selection: graphit_ast::entity_source::select_entity_source(
                            text,
                            start,
                            end,
                            &selectors,
                            &self.original,
                        )
                        .map_err(io::Error::other)?,
                    }
                } else {
                    inspect_source(source, None, &selectors, &self.original)
                        .map_err(io::Error::other)?
                }
            }
        };
        self.recheck()?;
        Ok(result)
    }
    async fn project_entity(
        &self,
        input: &SourceParameters,
    ) -> io::Result<graphit_ast::project_entity_source::ProjectEntitySource> {
        if let GraphPin::Project = &self.graph {
            let index = self.project_index().await?;
            self.recheck()?;
            let index = Arc::clone(&index);
            let original = Arc::clone(&self.original);
            let input = input.clone();
            let options = self.source.policy().graph_query;
            return tokio::task::spawn_blocking(move || {
                graphit_ast::project_entity_source::resolve_live_project_entity_source(
                    &index.graph,
                    &original,
                    &input.path,
                    input.entity.as_deref().ok_or_else(invalid)?,
                    input.entity_type.as_deref(),
                    options,
                )
                .map_err(io::Error::other)
            })
            .await
            .map_err(io::Error::other)?;
        }
        let GraphPin::Local(graph) = &self.graph else {
            return Err(absent());
        };
        let graph = Arc::clone(graph);
        let original = Arc::clone(&self.original);
        let input = input.clone();
        let options = self.source.policy().graph_query;
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            graphit_ast::project_entity_source::resolve_project_entity_source(
                &graph,
                &original,
                &runtime,
                &input.path,
                input.entity.as_deref().ok_or_else(invalid)?,
                input.entity_type.as_deref(),
                options,
            )
            .map_err(io::Error::other)
        })
        .await
        .map_err(io::Error::other)?
    }
    async fn entity_window(&self, input: &SourceParameters) -> io::Result<(usize, usize)> {
        let original = Arc::clone(&self.original);
        let input = input.clone();
        let options = self.source.policy().graph_query;
        let runtime = tokio::runtime::Handle::current();
        match &self.graph {
            GraphPin::Project => Err(invalid()),
            GraphPin::Local(graph) => {
                let graph = Arc::clone(graph);
                tokio::task::spawn_blocking(move || {
                    graphit_ast::imported_graph::resolve_imported_entity_window(
                        &graph,
                        &original,
                        &runtime,
                        &input.path,
                        input.entity.as_deref().ok_or_else(invalid)?,
                        input.entity_type.as_deref(),
                        options,
                    )
                    .map_err(io::Error::other)
                })
                .await
                .map_err(io::Error::other)?
            }
            GraphPin::Remote(graph) => {
                let graph = Arc::clone(graph);
                tokio::task::spawn_blocking(move || {
                    graphit_ast::imported_graph::resolve_remote_entity_window(
                        &graph,
                        &original,
                        &runtime,
                        &input.path,
                        input.entity.as_deref().ok_or_else(invalid)?,
                        input.entity_type.as_deref(),
                        options,
                    )
                    .map_err(io::Error::other)
                })
                .await
                .map_err(io::Error::other)?
            }
        }
    }
    pub(crate) async fn search<'a>(
        &'a self,
        caller: &'a graphit_auth::broker::oauth::web::AuthenticatedWebCaller<'a>,
        input: Input,
    ) -> io::Result<Value> {
        self.recheck()?;
        let pin = match &self.reader {
            Reader::Project { capture, read, .. } => {
                let project = capture.project();
                let StoreOwner::Project { project_id } = project.scope.live.owner() else {
                    return Err(denied());
                };
                let selected = (
                    ProjectPaths::at(
                        project.scope.live.owner_root(),
                        &self.source.policy().installation,
                    )?,
                    *project_id,
                    project.version.clone(),
                );
                self.server
                    .open_project_search(&selected, read, self.request)
                    .await?
                    .map(|(_, search)| search)
            }
            Reader::Imported { graph, .. } => {
                EntitySearch::open(
                    Arc::clone(graph.reader().map_err(io::Error::other)?),
                    Arc::clone(&self.original),
                )
                .await?
            }
            Reader::Remote { opened, lease, .. } => opened
                .mount
                .entity_search(&opened.selection, lease)
                .await
                .map_err(io::Error::other)?
                .map(|corpus| EntitySearch::from_corpus(corpus, Arc::clone(&self.original)))
                .transpose()?,
        };
        let Some(pin) = pin else {
            input
                .validate(self.source.policy())
                .map_err(|_| invalid())?;
            return Ok(json!({"status":"index_absent","results":[]}));
        };
        let authority = match &self.reader {
            Reader::Project { read, .. } => Authority::Web(caller, read),
            Reader::Imported { read, .. } | Reader::Remote { read, .. } => {
                Authority::Imported(read)
            }
        };
        // The retained corpus already includes nullable vector metadata; the
        // request can use that exact pin for lexical, semantic and rerank reads.
        let value = self
            .server
            .search_captured(self.source, &pin, authority, self.request, input)
            .await?;
        self.recheck()?;
        Ok(value)
    }
}
