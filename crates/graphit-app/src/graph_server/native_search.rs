//! Native search keeps the incoming actor, Original and copied publication in one owned job.
use super::*;
use graphit_auth::{ProjectReadModule, ProjectRecordAuthority, ScopeInput};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::{Value, json};

impl GraphServer {
    pub(crate) async fn call_search_mcp(
        &self,
        sources: &crate::source_server::SourceServer,
        call: CallToolRequestParams,
        headers: HeaderMap,
        cancelled: CancellationToken,
    ) -> CallToolResult {
        let failure = |message: &str| CallToolResult::error(vec![ContentBlock::text(message)]);
        let credentials = match credential(&headers) {
            Ok(value) => value,
            Err(_) => return failure("AST search requires native credentials"),
        };
        let mut arguments = call.arguments.unwrap_or_default();
        let context = arguments.remove("context");
        let mut selection = serde_json::Map::new();
        for key in ["project_dir", "project_version"] {
            if let Some(value) = arguments.remove(key) {
                selection.insert(key.into(), value);
            }
        }
        if context.is_some() && !selection.is_empty() {
            return failure("AST search selection ambiguous");
        }
        let input = Value::Object(arguments);
        let body = json!({"context":context,"selection":selection,"search":input});
        let Ok(bytes) = serde_json::to_vec(&body) else {
            return failure("AST search input invalid");
        };
        if bytes.len() > self.0.policy.content.max_input_bytes {
            return failure("AST search input too large");
        }
        let input: search::Input = match serde_json::from_value(input) {
            Ok(value) => value,
            Err(_) => return failure("AST search input invalid"),
        };
        if input.validate(&self.0.policy).is_err() {
            return failure("AST search input invalid");
        }
        let selection: ProjectSelection = match serde_json::from_value(Value::Object(selection)) {
            Ok(value) => value,
            Err(_) => return failure("AST search selection invalid"),
        };
        let context = match context {
            None => None,
            Some(value) => match value.as_str().and_then(|value| value.rsplit_once('@')) {
                Some((id, version)) if !id.is_empty() && !version.is_empty() => {
                    Some((id.to_owned(), version.to_owned()))
                }
                _ => return failure("installed AST context must be id@version"),
            },
        };
        let cancel = Cancellation::default();
        let _guard = CancelOnDrop(cancel.clone());
        let Some(deadline) = Instant::now().checked_add(self.0.policy.timeout) else {
            return failure("AST search deadline invalid");
        };
        let state = Arc::clone(&self.0);
        let owner = self.clone();
        // Clone only at request composition, after SourceServer attachments are complete.
        let sources = sources.clone();
        let request_cancel = cancel.clone();
        let (send, receive) = oneshot::channel();
        {
            let mut jobs = tokio::select! {
                jobs=state.jobs.lock()=>jobs,
                ()=cancelled.cancelled()=>return failure("AST search cancelled"),
                ()=state.stop.cancelled()=>return failure("AST search stopped"),
                ()=tokio::time::sleep_until(deadline.into())=>return failure("AST search timed out"),
            };
            while jobs.try_join_next().is_some() {}
            jobs.spawn(async move {
                let result = owner
                    .search_native(
                        &sources,
                        (&credentials.0, credentials.1),
                        (context, selection),
                        input,
                        &request_cancel,
                        deadline,
                    )
                    .await;
                let _ = send.send(result);
            });
        }
        let result = tokio::select! {
            value=receive=>value.unwrap_or(Err(ReadFailure::Context)),
            ()=cancelled.cancelled()=>return failure("AST search cancelled"),
            ()=state.stop.cancelled()=>return failure("AST search stopped"),
            ()=tokio::time::sleep_until(deadline.into())=>return failure("AST search timed out"),
        };
        match result {
            Ok(value)
                if !cancel.is_cancelled()
                    && !state.stop.is_cancelled()
                    && Instant::now() < deadline =>
            {
                crate::mcp_output::typed(value)
            }
            Ok(_) => failure("AST search cancelled"),
            Err(error) => {
                let response = error.into_response();
                let status = response.status();
                let bytes = axum::body::to_bytes(
                    response.into_body(),
                    state.policy.content.max_output_bytes,
                )
                .await
                .unwrap_or_default();
                crate::mcp_output::typed_error(
                    json!({"status":"error","http_status":status.as_u16(),"message":String::from_utf8_lossy(&bytes)}),
                )
            }
        }
    }

    async fn search_native(
        &self,
        sources: &crate::source_server::SourceServer,
        credentials: (&str, bool),
        target: (Option<(String, String)>, ProjectSelection),
        input: search::Input,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<Value, ReadFailure> {
        let (credential, local) = credentials;
        let (context, selection) = target;
        let state = &self.0;
        let original = RequestContext::new(state.policy.content, cancel, deadline)
            .map_err(|_| ReadFailure::Context)?;
        original.check().map_err(|_| ReadFailure::Context)?;
        if state.stop.is_cancelled() {
            return Err(ReadFailure::Context);
        }
        let issuer = state.auth.issuer();
        let caller = if local {
            issuer.authenticate_local(credential, &original)
        } else {
            issuer.authenticate_runtime(credential, &original)
        }
        .map_err(|error| {
            ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
        })?;
        let value = if let Some((id, version)) = context {
            let read = issuer
                .admit_authenticated_imported_ast_read(&caller, state.auth.installation())
                .map_err(|error| {
                    ReadFailure::Imported(ImportedGraphError::Authentication(error))
                })?;
            let projection = state.policy.registry.limits().projection();
            let id = ArtifactId::new(&id, projection).map_err(|_| ReadFailure::BadInput)?;
            let version =
                ConcreteVersion::new(&version, projection).map_err(|_| ReadFailure::BadInput)?;
            if let Some(value) =
                super::remote::search_admitted(state, sources, &read, &id, &version, &input).await?
            {
                read.recheck().map_err(|error| {
                    ReadFailure::Imported(ImportedGraphError::Authentication(error))
                })?;
                original.check().map_err(|_| ReadFailure::Context)?;
                if state.stop.is_cancelled() {
                    return Err(ReadFailure::Context);
                }
                encode(&value, state.policy.content.max_output_bytes)?;
                return Ok(value);
            }
            let graph = graphit_ast::imported_graph::open_imported_graph_admitted(
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
                    request: &original,
                },
            )?;
            let value = search::imported(sources, &graph, &read, input).await?;
            graph.revalidate()?;
            read.recheck().map_err(|error| {
                ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
            })?;
            value
        } else {
            caller
                .authorize(graphit_auth::Permission::Read(
                    graphit_core::module_settings::Module::Ast,
                ))
                .map_err(|error| {
                    ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
                })?;
            let access = state.project.as_ref().ok_or(ReadFailure::Context)?;
            let snapshot = access.live.snapshot().map_err(ReadFailure::Output)?;
            let index = select_project(state, &selection, &snapshot)?;
            let selected = &snapshot.projects[index];
            let capture = crate::project_graph::PreparedRead::capture(
                &state.policy,
                &access.live,
                (&selected.0, selected.1, &selected.2),
                Arc::clone(original.snapshot()),
            )
            .map_err(ReadFailure::Project)?;
            capture
                .check_active(&access.live)
                .map_err(ReadFailure::Project)?;
            let copied = matches!(input.mode, search::Mode::Fts | search::Mode::Hybrid);
            if copied {
                capture
                    .check(&state.policy, &access.config, &access.live)
                    .map_err(ReadFailure::Project)?;
            }
            let authority = ProjectRecordAuthority::new(issuer);
            let read = authority
                .admit_authenticated_read(
                    &caller,
                    &original,
                    ScopeInput {
                        installation: state.auth.installation(),
                        project: Some(capture.project()),
                        providers: &[],
                        binding: state.policy.binding,
                        global_version: "",
                    },
                    ProjectReadModule::Ast,
                )
                .map_err(|error| {
                    ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
                })?;
            let value = if matches!(input.mode, search::Mode::Semantic | search::Mode::Vector) {
                sources
                    .ast_search_native(
                        selected,
                        serde_json::to_value(&input).map_err(|_| ReadFailure::BadInput)?,
                        &caller,
                        &read,
                        &original,
                    )
                    .await
                    .map_err(ReadFailure::Query)?
            } else {
                match self
                    .open_project_search(selected, &read, &original)
                    .await
                    .map_err(ReadFailure::Query)?
                {
                    None => json!({"status":"index_absent","results":[]}),
                    Some((search_capture, pin)) => {
                        let value = search::copied(
                            sources.ast_search_context().map_err(ReadFailure::Query)?,
                            &pin,
                            search::Authority::Native(&caller, &read),
                            &original,
                            input,
                        )
                        .await?;
                        self.recheck_project_search(&search_capture, &read, &original)
                            .map_err(ReadFailure::Query)?;
                        value
                    }
                }
            };
            read.recheck().map_err(|error| {
                ReadFailure::Project(crate::project_graph::ReadError::Authentication(error))
            })?;
            if copied {
                capture
                    .check(&state.policy, &access.config, &access.live)
                    .map_err(ReadFailure::Project)?;
            } else {
                capture
                    .check_active(&access.live)
                    .map_err(ReadFailure::Project)?;
            }
            value
        };
        original.check().map_err(|_| ReadFailure::Context)?;
        if state.stop.is_cancelled() {
            return Err(ReadFailure::Context);
        }
        encode(&value, state.policy.content.max_output_bytes)?;
        Ok(value)
    }
}
