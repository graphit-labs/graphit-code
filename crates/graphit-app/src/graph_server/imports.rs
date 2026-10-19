//! Web imports use installation AST authority, independently of PROJECT grants.
use super::*;
use graphit_ast::imported_graph::{
    installed_ast_contexts, open_imported_graph_admitted, resolve_imported_entity_window,
};
use graphit_ast::imported_source::{ImportedSourceInput, inspect_imported_source_web};
use graphit_auth::broker::{BrokerVerifier, oauth::web::AuthenticatedWebCaller};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    source: Selection,
    query: Option<QueryData>,
    neighborhood: Option<NeighborhoodData>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceRequest {
    source: Selection,
    input: crate::ast_source::SourceParameters,
}
enum JobOutput {
    Graph(Vec<u8>),
    Extent((usize, usize)),
}

impl GraphServer {
    pub(super) async fn read_imported_broker<'a>(
        &'a self,
        body: Bytes,
        operation: BrowserOperation,
        identity: (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str),
        sources: &'a crate::source_server::SourceServer,
        original: &'a RequestContext<'a>,
    ) -> Result<Vec<u8>, ReadFailure> {
        let state = &self.0;
        let (caller, verifier, provider) = identity;
        original
            .snapshot()
            .check()
            .map_err(|_| ReadFailure::Context)?;
        if state.stop.is_cancelled() {
            return Err(ReadFailure::Context);
        }
        let read = state
            .auth
            .issuer()
            .admit_web_imported_ast_read(
                caller,
                verifier,
                original,
                provider,
                state.auth.installation(),
            )
            .map_err(|e| ReadFailure::Imported(ImportedGraphError::Authentication(e)))?;
        if matches!(operation, BrowserOperation::Imports) {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Empty {}
            serde_json::from_slice::<Empty>(&body).map_err(|_| ReadFailure::BadInput)?;
            let imports = installed_ast_contexts(
                &state.policy.registry,
                read.installation(),
                original.snapshot(),
            )?;
            if imports.len() > state.policy.content.max_collection_entries {
                return Err(ReadFailure::BadInput);
            }
            let bytes = encode(
                &serde_json::json!({"imports":imports}),
                state.policy.content.max_output_bytes,
            )?;
            read.recheck()
                .map_err(|e| ReadFailure::Imported(ImportedGraphError::Authentication(e)))?;
            original
                .snapshot()
                .check()
                .map_err(|_| ReadFailure::Context)?;
            if state.stop.is_cancelled() {
                return Err(ReadFailure::Context);
            }
            return Ok(bytes);
        }
        if matches!(operation, BrowserOperation::ImportedSearch) {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct SearchRequest {
                source: Selection,
                search: super::search::Input,
            }
            let input: SearchRequest =
                serde_json::from_slice(&body).map_err(|_| ReadFailure::BadInput)?;
            input.search.validate(&state.policy)?;
            let projection = state.policy.registry.limits().projection();
            let id =
                ArtifactId::new(&input.source.id, projection).map_err(|_| ReadFailure::BadInput)?;
            let version = ConcreteVersion::new(&input.source.version, projection)
                .map_err(|_| ReadFailure::BadInput)?;
            if let Some(result) =
                super::remote::search_admitted(state, sources, &read, &id, &version, &input.search)
                    .await?
            {
                read.recheck().map_err(|error| {
                    ReadFailure::Imported(ImportedGraphError::Authentication(error))
                })?;
                return encode(&result, state.policy.content.max_output_bytes);
            }
            let graph = open_imported_graph_admitted(
                &read,
                ImportedGraphInput {
                    registry: &state.policy.registry,
                    id: &id,
                    version: &version,
                    installation: read.installation(),
                    file_policy: &state.policy.file,
                    read_limits: state.policy.read,
                    graph_policy: state.policy.native_install.graph,
                    directory_limits: state.policy.index,
                    request: original,
                },
            )?;
            let result = super::search::imported(sources, &graph, &read, input.search).await?;
            graph.revalidate()?;
            read.recheck()
                .map_err(|e| ReadFailure::Imported(ImportedGraphError::Authentication(e)))?;
            original.check().map_err(|_| ReadFailure::Context)?;
            if state.stop.is_cancelled() {
                return Err(ReadFailure::Context);
            }
            return encode(&result, state.policy.content.max_output_bytes);
        }
        let (selection, query, neighborhood, source) = match operation {
            BrowserOperation::Imported(op) => {
                let input: Request =
                    serde_json::from_slice(&body).map_err(|_| ReadFailure::BadInput)?;
                if !matches!(
                    (&op, &input.query, &input.neighborhood),
                    (Operation::Query, Some(_), None)
                        | (Operation::Neighborhood, None, Some(_))
                        | (Operation::Schema | Operation::Sample, None, None)
                ) {
                    return Err(ReadFailure::BadInput);
                }
                (input.source, input.query, input.neighborhood, None)
            }
            BrowserOperation::ImportedSource => {
                let input: SourceRequest =
                    serde_json::from_slice(&body).map_err(|_| ReadFailure::BadInput)?;
                input.input.validate().map_err(|_| ReadFailure::BadInput)?;
                (input.source, None, None, Some(input.input))
            }
            _ => return Err(ReadFailure::BadInput),
        };
        let projection = state.policy.registry.limits().projection();
        let id = ArtifactId::new(&selection.id, projection).map_err(|_| ReadFailure::BadInput)?;
        let version = ConcreteVersion::new(&selection.version, projection)
            .map_err(|_| ReadFailure::BadInput)?;
        if let Some(bytes) = super::remote::read_admitted(
            state,
            &read,
            &id,
            &version,
            operation,
            query.clone(),
            neighborhood.clone(),
            source.as_ref(),
        )
        .await?
        {
            return Ok(bytes);
        }
        let needs_graph = source.as_ref().is_none_or(|source| source.entity.is_some());
        let graph = if needs_graph {
            Some(open_imported_graph_admitted(
                &read,
                ImportedGraphInput {
                    registry: &state.policy.registry,
                    id: &id,
                    version: &version,
                    installation: read.installation(),
                    file_policy: &state.policy.file,
                    read_limits: state.policy.read,
                    graph_policy: state.policy.native_install.graph,
                    directory_limits: state.policy.index,
                    request: original,
                },
            )?)
        } else {
            None
        };
        let output = if let Some(graph) = &graph {
            let reader = Arc::clone(graph.reader()?);
            let snapshot = Arc::clone(original.snapshot());
            let owner = Arc::clone(state);
            let entity = source.as_ref().and_then(|source| {
                source.entity.as_ref().map(|name| {
                    (
                        source.path.clone(),
                        name.clone(),
                        source.entity_type.clone(),
                    )
                })
            });
            let (send, receive) = oneshot::channel();
            let deadline = snapshot
                .required_deadline()
                .map_err(|_| ReadFailure::Context)?;
            {
                let mut jobs = tokio::select! {
                    jobs = state.jobs.lock() => jobs,
                    () = state.stop.cancelled() => return Err(ReadFailure::Context),
                    () = tokio::time::sleep_until(deadline.into()) => return Err(ReadFailure::Context),
                };
                while jobs.try_join_next().is_some() {}
                jobs.spawn_blocking(move || {
                    let result = if let Some((path, name, kind)) = entity {
                        let runtime = tokio::runtime::Handle::current();
                        resolve_imported_entity_window(
                            &reader,
                            &snapshot,
                            &runtime,
                            &path,
                            &name,
                            kind.as_deref(),
                            owner.policy.graph_query,
                        )
                        .map(JobOutput::Extent)
                        .map_err(ReadFailure::Entity)
                    } else if let BrowserOperation::Imported(op) = operation {
                        render_operation(
                            &owner,
                            &GraphView::WebInstalled(reader, snapshot),
                            op,
                            query,
                            neighborhood,
                            owner.policy.native_install.graph,
                        )
                        .map(JobOutput::Graph)
                    } else {
                        Err(ReadFailure::BadInput)
                    };
                    let _ = send.send(result);
                });
            }
            let result = tokio::select! {
                result = receive => result.map_err(|_| ReadFailure::Context)??,
                () = state.stop.cancelled() => return Err(ReadFailure::Context),
                () = tokio::time::sleep_until(deadline.into()) => return Err(ReadFailure::Context),
            };
            graph.revalidate()?;
            Some(result)
        } else {
            None
        };
        let bytes = if let Some(source) = source {
            let extent = match output {
                Some(JobOutput::Extent(extent)) => Some(extent),
                None => None,
                Some(JobOutput::Graph(_)) => return Err(ReadFailure::BadInput),
            };
            let selection = source.selection();
            let result = inspect_imported_source_web(
                &read,
                ImportedSourceInput {
                    registry: &state.policy.registry,
                    id: &id,
                    version: &version,
                    installation: read.installation(),
                    fallback: None,
                    requested_relative: std::path::Path::new(&source.path),
                    selection: &selection,
                    file_policy: &state.policy.file,
                    read_limits: state.policy.read,
                    path_limits: state.policy.path,
                    index_limits: state.policy.index,
                    request: original,
                },
                extent,
            )
            .await
            .map_err(ReadFailure::ImportedSource)?;
            if let Some(graph) = &graph {
                graph.revalidate()?;
            }
            encode(&result, state.policy.content.max_output_bytes)?
        } else if let Some(JobOutput::Graph(bytes)) = output {
            bytes
        } else {
            return Err(ReadFailure::BadInput);
        };
        read.recheck()
            .map_err(|e| ReadFailure::Imported(ImportedGraphError::Authentication(e)))?;
        original
            .snapshot()
            .check()
            .map_err(|_| ReadFailure::Context)?;
        if state.stop.is_cancelled() {
            return Err(ReadFailure::Context);
        }
        Ok(bytes)
    }
}
