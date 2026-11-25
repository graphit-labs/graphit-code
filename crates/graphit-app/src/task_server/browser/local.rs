//! Local desktop reads retain the real installation caller and native admissions.
use super::*;
use graphit_auth::AuthenticatedCaller;

impl TaskServer {
    pub(crate) async fn read_local<'a>(
        &'a self,
        arguments: Arguments,
        operation: &str,
        caller: &'a AuthenticatedCaller<'a>,
        original: &'a RequestContext<'a>,
    ) -> Result<BrowserReadOutput, ReadFailure> {
        let fail = |code| ReadFailure::task(Failure::new(code));
        let snapshot = self.record_snapshot()?;
        let module = if operation.starts_with("memory_") {
            ProjectReadModule::Memory
        } else if operation.starts_with("knowledge_") {
            ProjectReadModule::Knowledge
        } else {
            ProjectReadModule::Task
        };
        if matches!(
            operation,
            "contexts"
                | "memory_contexts"
                | "knowledge_contexts"
                | "ast_contexts"
                | "hub_contexts"
                | "now_contexts"
        ) {
            if arguments.input.is_some()
                || arguments.selection.project_dir.is_some()
                || arguments.selection.project_id.is_some()
                || arguments.selection.project_version.is_some()
            {
                return Err(fail("invalid_arguments"));
            }
            let module = match operation {
                "ast_contexts" => ProjectReadModule::Ast,
                "hub_contexts" => ProjectReadModule::Hub,
                _ => module,
            };
            let authority = ProjectRecordAuthority::new(self.0.auth.issuer());
            let mut contexts = Vec::new();
            let mut current = None;
            for (index, (paths, id, version)) in snapshot.projects.iter().enumerate() {
                if !self
                    .0
                    .live_projects
                    .is_active(paths, *id, version)
                    .map_err(|_| fail("selection_unavailable"))?
                {
                    continue;
                }
                let project = PublicationScope::project(paths, *id)
                    .map_err(|_| fail("selection_unavailable"))?;
                let admission = authority
                    .admit_authenticated_read(
                        caller,
                        original,
                        ScopeInput {
                            installation: self.0.auth.installation(),
                            project: Some(ProjectSelection {
                                scope: &project,
                                version,
                            }),
                            providers: &[],
                            binding: self.0.policy.binding,
                            global_version: "",
                        },
                        module,
                    )
                    .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
                admission
                    .recheck()
                    .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
                if contexts.len() >= self.0.policy.content.max_collection_entries {
                    return Err(fail("response_limit"));
                }
                if snapshot.current == Some(index) {
                    current = Some(contexts.len());
                }
                contexts.push(json!({"project_dir":paths.root(),"project_id":id,"project_version":version.version()}));
            }
            let names = crate::live_project_selections::project_names(
                snapshot.projects.iter(),
                original.snapshot(),
            )
            .map_err(|_| fail("selection_unavailable"))?;
            let value = json!({"current":current.and_then(|i|contexts.get(i)),"contexts":contexts,"projects":names});
            self.check_browser_response(&value)?;
            return Ok(BrowserReadOutput::Json(value));
        }
        if operation == "now_snapshot"
            && (arguments.input.is_some()
                || arguments.selection.project_id.is_none()
                || arguments.selection.project_dir.is_none()
                || arguments.selection.project_version.is_none())
        {
            return Err(fail("invalid_selection"));
        }
        let selected = arguments
            .selection
            .select_read(&snapshot.projects, snapshot.current.unwrap_or(0))
            .map_err(fail)?;
        let (paths, id, version) = snapshot
            .projects
            .get(selected)
            .ok_or_else(|| fail("selection_unavailable"))?;
        if !self
            .0
            .live_projects
            .is_active(paths, *id, version)
            .map_err(|_| fail("selection_unavailable"))?
        {
            return Err(fail("selection_unavailable"));
        }
        let project =
            PublicationScope::project(paths, *id).map_err(|_| fail("selection_unavailable"))?;
        let scope = || ScopeInput {
            installation: self.0.auth.installation(),
            project: Some(ProjectSelection {
                scope: &project,
                version,
            }),
            providers: &[],
            binding: self.0.policy.binding,
            global_version: "",
        };
        let authority = ProjectRecordAuthority::new(self.0.auth.issuer());
        let selection = codec::AdmittedSelection {
            project_dir: graphit_core::native_path::NativePath::new(paths.root().to_owned())
                .map_err(|_| fail("invalid_selection"))?,
            project_id: *id,
            project_version: version.version().to_owned(),
        };
        let input = arguments.input;
        if operation == "now_snapshot" {
            use graphit_daemon::project_now::{ProjectNowLimits, read_project_now};
            let batch_rows = self.0.policy.read.batch_rows;
            let limits = ProjectNowLimits {
                task: self.0.policy.read,
                memory: graphit_records::memory_store::MemoryHeadReadLimits { batch_rows },
                knowledge: graphit_data::knowledge_index::KnowledgeHeadReadLimits {
                    max_slug_bytes: std::num::NonZeroUsize::new(
                        self.0.policy.content.max_scalar_bytes,
                    )
                    .ok_or_else(|| fail("invalid_arguments"))?,
                    batch_rows,
                },
            };
            let snapshot = read_project_now(&authority, caller, original, scope(), limits)
                .await
                .map_err(|_| fail("activity_unavailable"))?;
            let value = json!({"selection": selection, "snapshot": snapshot});
            self.check_browser_response(&value)?;
            return Ok(BrowserReadOutput::Json(value));
        }
        if operation == "index_status" || operation == "index_run" {
            let input = serde_json::from_value(input.ok_or_else(|| fail("invalid_arguments"))?)
                .map_err(|_| fail("invalid_arguments"))?;
            let key = graphit_daemon::supervisor::SupervisionKey {
                id: *id,
                checkout: paths.root().to_owned(),
                version: version.version().to_owned(),
            };
            let value = crate::index_control::query(
                &self.0.indices,
                &key,
                operation == "index_run",
                input,
                self.0.auth.issuer(),
                caller,
                original,
                scope(),
                &self.0.policy,
            )
            .await
            .map_err(ReadFailure::native)?;
            return Ok(BrowserReadOutput::Json(value));
        }

        if operation.starts_with("memory_") {
            let query: crate::foreground_memory::Query =
                serde_json::from_value(input.ok_or_else(|| fail("invalid_arguments"))?)
                    .map_err(|_| fail("invalid_arguments"))?;
            if let crate::foreground_memory::Query::Mutation(input) = query {
                let admission = authority
                    .admit_authenticated_write(
                        caller,
                        original,
                        scope(),
                        graphit_auth::ProjectWriteOperation::Memory,
                    )
                    .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
                let result = Box::pin(crate::foreground_memory::mutation::project(
                    original.snapshot(),
                    &admission,
                    &self.0.policy,
                    selection,
                    *input,
                ))
                .await;
                if result.is_ok()
                    || result.as_ref().is_err_and(|error| {
                        matches!(
                            error,
                            graphit_records::memory_store::MemoryStoreError::Confirmed { .. }
                        )
                    })
                {
                    TaskServer::wake_embeddings(&self.0, paths, *id, version);
                }
                return result
                    .map(BrowserReadOutput::MemoryWrite)
                    .map_err(ReadFailure::memory);
            }
            let admission = authority
                .admit_authenticated_read(caller, original, scope(), module)
                .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
            let bytes = match query {
                crate::foreground_memory::Query::Search(input) => {
                    let resources = self.memory_model().map_err(|_| fail("model_failure"))?;
                    Box::pin(crate::foreground_memory::search::project(
                        original,
                        &admission,
                        &self.0.policy,
                        &selection,
                        input,
                        resources.as_deref(),
                    ))
                    .await
                    .map_err(ReadFailure::memory_search)?
                }
                query => Box::pin(crate::foreground_memory::read(
                    original.snapshot(),
                    &admission,
                    &self.0.policy,
                    &selection,
                    query,
                ))
                .await
                .map_err(ReadFailure::memory)?,
            };
            admission
                .recheck()
                .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
            return Ok(BrowserReadOutput::Encoded(bytes));
        }
        let admission = authority
            .admit_authenticated_read(caller, original, scope(), module)
            .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
        if operation == "task_references" || operation == "knowledge_references" {
            let value = crate::role_tool_server::references::read_admitted(
                &admission,
                operation == "knowledge_references",
                input,
                original,
                self.0.policy.read,
                crate::foreground_knowledge::limits(&self.0.policy)
                    .map_err(|error| ReadFailure::knowledge(error.into()))?,
            )
            .await
            .map_err(ReadFailure::native)?;
            return Ok(BrowserReadOutput::Json(value));
        }
        if operation == "knowledge_list" || operation == "knowledge_lint" {
            let query = serde_json::from_value(input.ok_or_else(|| fail("invalid_arguments"))?)
                .map_err(|_| fail("invalid_arguments"))?;
            let value = crate::knowledge_catalogue::read(
                original.snapshot(),
                &admission,
                &self.0.policy,
                query,
                json!({"project_id":id,"project_version":version.version()}),
            )
            .await
            .map_err(ReadFailure::knowledge)?;
            return Ok(BrowserReadOutput::Json(value));
        }
        let output = if operation == "context" {
            if input.is_some() {
                return Err(fail("invalid_arguments"));
            }
            BrowserReadOutput::Json(
                serde_json::to_value(&selection).map_err(|_| fail("response_encoding_failed"))?,
            )
        } else if operation.starts_with("knowledge_") {
            let query = serde_json::from_value(input.ok_or_else(|| fail("invalid_arguments"))?)
                .map_err(|_| fail("invalid_arguments"))?;
            let resources = self.memory_model().map_err(|_| fail("model_failure"))?;
            let context = crate::foreground_knowledge::search::BrowserSearchContext {
                server: self,
                auth: &self.0.auth,
                project: paths,
                verifiers: self.0.native_verifiers.as_deref(),
            };
            BrowserReadOutput::Encoded(
                crate::foreground_knowledge::read(
                    original,
                    crate::foreground_knowledge::search::ReadIdentity::Native(caller),
                    &admission,
                    &self.0.policy,
                    &selection,
                    query,
                    resources.as_deref(),
                    &context,
                )
                .await
                .map_err(ReadFailure::knowledge)?,
            )
        } else if operation == "references" {
            let query = serde_json::from_value(input.ok_or_else(|| fail("invalid_arguments"))?)
                .map_err(|_| fail("invalid_arguments"))?;
            BrowserReadOutput::Json(
                crate::foreground_task_references::read(
                    original.snapshot(),
                    &admission,
                    &self.0.policy,
                    query,
                )
                .await
                .map_err(|e| ReadFailure::task(Failure::store(e)))?,
            )
        } else {
            let Operation::Read(query) = Operation::parse(operation, false, input).map_err(fail)?
            else {
                unreachable!()
            };
            let semantic = matches!(&query,graphit_records::task_protocol::ReadRequest::Search(input) if input.mode != graphit_records::task_store::TaskSearchMode::Fts);
            let service = if semantic {
                Some(
                    self.0
                        .auth
                        .issuer()
                        .capture_service(caller, graphit_auth::ServicePurpose::Embedding)
                        .map_err(|_| fail("authentication_or_permission_denied"))?,
                )
            } else {
                None
            };
            let mut value = bounded_value(
                execute_task_read(&self.0, paths, query, original, &admission, service)
                    .await
                    .map_err(ReadFailure::task)?,
                self.0.policy.content.max_output_bytes,
            )
            .map_err(|_| fail("response_limit"))?;
            BrowserReadOutput::Json(value["result"].take())
        };
        admission
            .recheck()
            .map_err(|e| ReadFailure::task(Failure::auth(e)))?;
        original
            .check()
            .map_err(|_| fail("request_cancelled_or_expired"))?;
        if self.0.stop.is_cancelled() {
            return Err(fail("stopped"));
        }
        Ok(output)
    }
}
