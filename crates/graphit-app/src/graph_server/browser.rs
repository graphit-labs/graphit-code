//! Web PROJECT reads keep the verified Web admission on the async owner.
use super::*;
use crate::project_graph::{PreparedRead, ReadError};
use graphit_auth::{
    AuthError, ProjectReadModule, ProjectRecordAuthority,
    broker::{
        BrokerVerifier,
        oauth::web::AuthenticatedWebCaller,
        project_access::{
            BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
            BrokerRecordRead,
        },
    },
};

#[derive(Clone, Copy)]
pub(crate) enum BrowserOperation {
    Contexts,
    Source,
    Search,
    Query,
    Schema,
    Sample,
    Neighborhood,
    Imports,
    Imported(Operation),
    ImportedSource,
    ImportedSearch,
}
impl BrowserOperation {
    fn graph(self) -> Option<Operation> {
        match self {
            Self::Contexts
            | Self::Source
            | Self::Search
            | Self::Imports
            | Self::ImportedSource
            | Self::ImportedSearch => None,
            Self::Imported(operation) => Some(operation),
            Self::Query => Some(Operation::Query),
            Self::Schema => Some(Operation::Schema),
            Self::Sample => Some(Operation::Sample),
            Self::Neighborhood => Some(Operation::Neighborhood),
        }
    }
}
#[derive(Serialize)]
struct SelectionView {
    project_dir: graphit_core::native_path::NativePath,
    project_id: graphit_core::project_identity::ProjectId,
    project_version: String,
}
impl GraphServer {
    pub(crate) fn browser_policy(&self) -> (&SourcePolicy, &CancellationToken) {
        (&self.0.policy, &self.0.stop)
    }
    pub(crate) async fn read_broker<'a>(
        &'a self,
        body: Bytes,
        operation: BrowserOperation,
        identity: (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str),
        backend: &'a BrokerProjectAccess,
        sources: &'a crate::source_server::SourceServer,
        original: &'a RequestContext<'a>,
    ) -> Response {
        match self
            .read_web(body, operation, identity, backend, sources, original)
            .await
        {
            Ok(bytes) => ([(header::CONTENT_TYPE, "application/json")], bytes).into_response(),
            Err(error) => error.into_response(),
        }
    }
    async fn read_web<'a>(
        &'a self,
        body: Bytes,
        operation: BrowserOperation,
        identity: (&'a AuthenticatedWebCaller<'a>, &'a BrokerVerifier, &'a str),
        backend: &'a BrokerProjectAccess,
        sources: &'a crate::source_server::SourceServer,
        original: &'a RequestContext<'a>,
    ) -> Result<Vec<u8>, ReadFailure> {
        if matches!(
            operation,
            BrowserOperation::Imports
                | BrowserOperation::Imported(_)
                | BrowserOperation::ImportedSource
                | BrowserOperation::ImportedSearch
        ) {
            return self
                .read_imported_broker(body, operation, identity, sources, original)
                .await;
        }
        let state = &self.0;
        let deadline = original
            .snapshot()
            .required_deadline()
            .map_err(|_| ReadFailure::Context)?;
        original
            .snapshot()
            .check()
            .map_err(|_| ReadFailure::Context)?;
        if state.stop.is_cancelled() {
            return Err(ReadFailure::Context);
        }
        let access = state.project.as_ref().ok_or(ReadFailure::Context)?;
        let snapshot = access
            .live
            .snapshot()
            .map_err(|error| ReadFailure::Project(ReadError::Operation(error)))?;
        let authority = ProjectRecordAuthority::new(state.auth.issuer());
        let (caller, verifier, provider) = identity;
        if matches!(operation, BrowserOperation::Contexts) {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Empty {}
            serde_json::from_slice::<Empty>(&body).map_err(|_| ReadFailure::BadInput)?;
            let mut captures = Vec::new();
            for (index, (paths, id, version)) in snapshot.projects.iter().enumerate() {
                match PreparedRead::capture(
                    &state.policy,
                    &access.live,
                    (paths, *id, version),
                    Arc::clone(original.snapshot()),
                )
                .and_then(|capture| {
                    capture.check(&state.policy, &access.config, &access.live)?;
                    Ok(capture)
                }) {
                    Ok(capture) => captures.push((index, capture)),
                    Err(ReadError::Inactive | ReadError::Disabled) => {}
                    Err(error) => return Err(ReadFailure::Project(error)),
                }
            }
            let mut admissions = Vec::new();
            let mut contexts = Vec::new();
            let mut admitted = Vec::new();
            let mut admitted_indices = Vec::new();
            let mut current = None;
            for (index, capture) in &captures {
                let admission = match authority
                    .admit_broker_read(BrokerRecordRead {
                        caller: BrokerProjectCaller::Web(caller),
                        verifier,
                        backend,
                        original,
                        scope: BrokerProjectScope {
                            installation: state.auth.installation(),
                            project: capture.project(),
                            providers: &[],
                        },
                        provider,
                        module: ProjectReadModule::Ast,
                    })
                    .await
                {
                    Ok(admission) => admission,
                    Err(
                        BrokerProjectError::Denied
                        | BrokerProjectError::Authority(AuthError::Denied),
                    ) => continue,
                    Err(error) => return Err(ReadFailure::Broker(error)),
                };
                if contexts.len() == state.policy.content.max_collection_entries {
                    return Err(ReadFailure::BadInput);
                }
                let (paths, id, version) = &snapshot.projects[*index];
                if snapshot.current == Some(*index) {
                    current = Some(contexts.len());
                }
                contexts.push(SelectionView {
                    project_dir: graphit_core::native_path::NativePath::new(
                        paths.root().to_owned(),
                    )
                    .map_err(ReadFailure::Output)?,
                    project_id: *id,
                    project_version: version.version().to_owned(),
                });
                admitted.push(capture);
                admitted_indices.push(*index);
                admissions.push(admission);
            }
            for (admission, capture) in admissions.iter().zip(&admitted) {
                admission
                    .recheck()
                    .map_err(|error| ReadFailure::Project(ReadError::Authentication(error)))?;
                capture
                    .check(&state.policy, &access.config, &access.live)
                    .map_err(ReadFailure::Project)?;
            }
            #[derive(Serialize)]
            struct Output<'a> {
                contexts: &'a [SelectionView],
                current: Option<&'a SelectionView>,
                projects: Vec<crate::live_project_selections::ProjectName>,
            }
            let bytes = encode(
                &Output {
                    contexts: &contexts,
                    current: current.map(|index| &contexts[index]),
                    projects: crate::live_project_selections::project_names(
                        admitted_indices
                            .iter()
                            .map(|index| &snapshot.projects[*index]),
                        original.snapshot(),
                    )
                    .map_err(ReadFailure::Output)?,
                },
                state.policy.content.max_output_bytes,
            )?;
            for (admission, capture) in admissions.iter().zip(&admitted) {
                admission
                    .recheck()
                    .map_err(|error| ReadFailure::Project(ReadError::Authentication(error)))?;
                capture
                    .check(&state.policy, &access.config, &access.live)
                    .map_err(ReadFailure::Project)?;
            }
            original
                .snapshot()
                .check()
                .map_err(|_| ReadFailure::Context)?;
            if state.stop.is_cancelled() {
                return Err(ReadFailure::Context);
            }
            return Ok(bytes);
        };
        let graph_operation = operation.graph();
        let (input, source, search) = if matches!(operation, BrowserOperation::Source) {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct SourceRequest {
                selection: ProjectSelection,
                source: crate::ast_source::SourceParameters,
            }
            let input: SourceRequest =
                serde_json::from_slice(&body).map_err(|_| ReadFailure::BadInput)?;
            input.source.validate().map_err(|_| ReadFailure::BadInput)?;
            (
                ProjectRequest {
                    selection: input.selection,
                    query: None,
                    neighborhood: None,
                },
                Some(input.source),
                None,
            )
        } else if matches!(operation, BrowserOperation::Search) {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct SearchRequest {
                selection: ProjectSelection,
                search: serde_json::Value,
            }
            let input: SearchRequest =
                serde_json::from_slice(&body).map_err(|_| ReadFailure::BadInput)?;
            (
                ProjectRequest {
                    selection: input.selection,
                    query: None,
                    neighborhood: None,
                },
                None,
                Some(input.search),
            )
        } else {
            (
                parse_project(&body, graph_operation.ok_or(ReadFailure::BadInput)?)?,
                None,
                None,
            )
        };
        let index = select_project(state, &input.selection, &snapshot)?;
        let requires_graph = source.as_ref().is_none_or(|source| source.entity.is_some());
        let (paths, id, version) = &snapshot.projects[index];
        let capture = Arc::new(
            PreparedRead::capture(
                &state.policy,
                &access.live,
                (paths, *id, version),
                Arc::clone(original.snapshot()),
            )
            .map_err(ReadFailure::Project)?,
        );
        if requires_graph {
            capture
                .check(&state.policy, &access.config, &access.live)
                .map_err(ReadFailure::Project)?;
        }
        let admission = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(caller),
                verifier,
                backend,
                original,
                scope: BrokerProjectScope {
                    installation: state.auth.installation(),
                    project: capture.project(),
                    providers: &[],
                },
                provider,
                module: ProjectReadModule::Ast,
            })
            .await
            .map_err(ReadFailure::Broker)?;
        admission
            .recheck()
            .map_err(|error| ReadFailure::Project(ReadError::Authentication(error)))?;
        if let Some(search) = search {
            let selected = (paths.clone(), *id, version.clone());
            let mode = search.get("mode").and_then(serde_json::Value::as_str);
            let result = if matches!(mode, Some("fts" | "hybrid")) {
                let input: super::search::Input =
                    serde_json::from_value(search).map_err(|_| ReadFailure::BadInput)?;
                input.validate(&state.policy)?;
                match self
                    .open_project_search(&selected, &admission, original)
                    .await
                    .map_err(ReadFailure::Query)?
                {
                    None => serde_json::json!({"status":"index_absent","results":[]}),
                    Some((search_capture, pin)) => {
                        let value = super::search::copied(
                            sources.ast_search_context().map_err(ReadFailure::Query)?,
                            &pin,
                            super::search::Authority::Web(caller, &admission),
                            original,
                            input,
                        )
                        .await?;
                        self.recheck_project_search(&search_capture, &admission, original)
                            .map_err(ReadFailure::Query)?;
                        value
                    }
                }
            } else {
                sources
                    .ast_search_web(&selected, search, caller, &admission, original)
                    .await
                    .map_err(ReadFailure::Query)?
            };
            let bytes = encode(&result, state.policy.content.max_output_bytes)?;
            admission
                .recheck()
                .map_err(|e| ReadFailure::Project(ReadError::Authentication(e)))?;
            capture
                .check(&state.policy, &access.config, &access.live)
                .map_err(ReadFailure::Project)?;
            original
                .snapshot()
                .check()
                .map_err(|_| ReadFailure::Context)?;
            if state.stop.is_cancelled() {
                return Err(ReadFailure::Context);
            }
            return Ok(bytes);
        }
        let (send, receive) = oneshot::channel();
        {
            let mut jobs = tokio::select! {
                jobs = state.jobs.lock() => jobs,
                () = state.stop.cancelled() => return Err(ReadFailure::Context),
                () = tokio::time::sleep_until(deadline.into()) => return Err(ReadFailure::Context),
            };
            while jobs.try_join_next().is_some() {}
            let owner = Arc::clone(state);
            let capture = Arc::clone(&capture);
            jobs.spawn_blocking(move || {
                let access = owner.project.as_ref().expect("captured PROJECT access");
                let result = if let Some(source) = source {
                    let runtime = tokio::runtime::Handle::current();
                    runtime
                        .block_on(capture.inspect_source(
                            &owner.policy,
                            &access.config,
                            &access.live,
                            owner.auth.installation(),
                            &owner.live_ast,
                            &source,
                        ))
                        .map_err(ReadFailure::Project)
                        .and_then(|result| encode(&result, owner.policy.content.max_output_bytes))
                } else {
                    render_project_capture(
                        &owner,
                        &capture,
                        graph_operation.expect("non-Source graph operation"),
                        input.query,
                        input.neighborhood,
                    )
                };
                let _ = send.send(result);
            });
        }
        let bytes = tokio::select! {
            value = receive => value.map_err(|_| ReadFailure::Context)??,
            () = state.stop.cancelled() => return Err(ReadFailure::Context),
            () = tokio::time::sleep_until(deadline.into()) => return Err(ReadFailure::Context),
        };
        admission
            .recheck()
            .map_err(|error| ReadFailure::Project(ReadError::Authentication(error)))?;
        if requires_graph {
            capture
                .check(&state.policy, &access.config, &access.live)
                .map_err(ReadFailure::Project)?;
        } else {
            capture
                .check_active(&access.live)
                .map_err(ReadFailure::Project)?;
        }
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
