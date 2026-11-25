//! Human Native record operations reuse the desktop producers and ordinary admissions.
//! This route never accepts managed-role or Web credentials and is not a Root MCP write catalogue.
use super::*;
use axum::{
    body::Bytes,
    response::{IntoResponse, Response},
};

fn failure(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({"error":{"code":code}}))).into_response()
}
pub(crate) async fn http(
    AxumState(server): AxumState<TaskServer>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if headers.contains_key(axum::http::header::ORIGIN) {
        return failure(StatusCode::FORBIDDEN, "native_credentials_required");
    }
    let credential = match Credential::read(&headers, &server.0.policy) {
        Ok(value) if matches!(value.origin, Origin::Runtime | Origin::Local) => value,
        _ => return failure(StatusCode::FORBIDDEN, "native_credentials_required"),
    };
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        #[serde(default)]
        selection: super::request::Selection,
        input: Value,
        #[serde(default)]
        scope: crate::memory_index_entry::Scope,
    }
    let input: Input = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return failure(StatusCode::BAD_REQUEST, "invalid_arguments"),
    };
    if input.scope == crate::memory_index_entry::Scope::User
        && (input.selection.project_dir.is_some()
            || input.selection.project_id.is_some()
            || input.selection.project_version.is_some())
    {
        return failure(
            StatusCode::BAD_REQUEST,
            "user_scope_rejects_project_selection",
        );
    }
    if input.scope == crate::memory_index_entry::Scope::User && !operation.starts_with("memory_") {
        return failure(StatusCode::BAD_REQUEST, "user_scope_requires_memory");
    }
    let query = match operation.as_str() {
        "task_references" | "knowledge_references" | "index_status" | "index_run" => input.input,
        "knowledge_list" => json!({"List": input.input}),
        "knowledge_lint" => json!({"Lint": input.input}),
        "memory_list" => json!({"List":input.input}),
        "memory_get" => json!({"Get":input.input}),
        "memory_history" => json!({"History":input.input}),
        "memory_search" => json!({"Search":input.input}),
        "memory_create" => json!({"Mutation":{"Create":input.input}}),
        "memory_update" => json!({"Mutation":{"Update":input.input}}),
        "memory_delete" => json!({"Mutation":{"Delete":input.input}}),
        _ => return failure(StatusCode::NOT_FOUND, "unknown_operation"),
    };
    let arguments = if matches!(
        operation.as_str(),
        "task_references" | "knowledge_references" | "index_status" | "index_run"
    ) {
        Arguments {
            selection: input.selection,
            input: Some(query),
        }
    } else if operation.starts_with("knowledge_") {
        let parsed: crate::knowledge_catalogue::Query = match serde_json::from_value(query) {
            Ok(v) => v,
            Err(_) => return failure(StatusCode::BAD_REQUEST, "invalid_arguments"),
        };
        Arguments {
            selection: input.selection,
            input: Some(serde_json::to_value(parsed).expect("typed catalogue query")),
        }
    } else {
        let parsed: crate::foreground_memory::Query = match serde_json::from_value(query) {
            Ok(v) => v,
            Err(_) => return failure(StatusCode::BAD_REQUEST, "invalid_arguments"),
        };
        Arguments {
            selection: input.selection,
            input: Some(serde_json::to_value(parsed).expect("typed Memory query")),
        }
    };
    let user = input.scope == crate::memory_index_entry::Scope::User;
    let cancel = Cancellation::default();
    struct Cancel(Cancellation);
    impl Drop for Cancel {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
    let _guard = Cancel(cancel.clone());
    let original_cancel = cancel.clone();
    let owner = server.clone();
    let deadline = Instant::now() + server.0.policy.timeout;
    let handle = Handle::current();
    let (send, mut receive) = oneshot::channel();
    {
        let mut jobs = server.0.jobs.lock().await;
        while let Some(result) = jobs.try_join_next() {
            if result.is_err() {
                return failure(StatusCode::INTERNAL_SERVER_ERROR, "native_job_failed");
            }
        }
        jobs.spawn_blocking(move || {
            let result = handle.block_on(async {
                let original =
                    match RequestContext::new(owner.0.policy.content, &original_cancel, deadline) {
                        Ok(value) => value,
                        Err(_) => {
                            return failure(
                                StatusCode::REQUEST_TIMEOUT,
                                "request_cancelled_or_expired",
                            );
                        }
                    };
                let issuer = owner.0.auth.issuer();
                let caller = match credential.origin {
                    Origin::Runtime => issuer.authenticate_runtime(&credential.bearer, &original),
                    Origin::Local => issuer.authenticate_local(&credential.bearer, &original),
                    _ => unreachable!("native credentials checked"),
                };
                let caller = match caller {
                    Ok(value) => value,
                    Err(_) => {
                        return failure(
                            StatusCode::FORBIDDEN,
                            "authentication_or_permission_denied",
                        );
                    }
                };
                if user {
                    return user_memory(&owner, arguments, caller, &original).await;
                }
                match owner
                    .read_local(arguments, &operation, &caller, &original)
                    .await
                {
                    Ok(value) => value.into_response(),
                    Err(error) => error.into_response(),
                }
            });
            let _ = send.send(result);
        });
    }
    // Preserve receipts for confirmed writes even when delivery is cancelled.
    tokio::select! {
        result=&mut receive=>return result.unwrap_or_else(|_|failure(StatusCode::INTERNAL_SERVER_ERROR,"native_job_failed")),
        ()=server.0.stop.cancelled()=>{},
        ()=tokio::time::sleep_until(deadline.into())=>{},
    }
    cancel.cancel();
    receive
        .await
        .unwrap_or_else(|_| failure(StatusCode::REQUEST_TIMEOUT, "request_cancelled_or_expired"))
}

async fn user_memory<'a>(
    server: &'a TaskServer,
    arguments: Arguments,
    caller: graphit_auth::AuthenticatedCaller<'a>,
    original: &'a RequestContext<'a>,
) -> Response {
    let issuer = server.0.auth.issuer();
    let identity = async {
        let denied = || failure(StatusCode::FORBIDDEN, "user_memory_access_denied");
        let verifier = crate::foreground_user_memory::native_verifier(
            issuer,
            &caller,
            server.0.native_verifiers.as_deref(),
        )
        .await
        .map_err(|_| denied())?;
        let caller = if let Some(verifier) = &verifier {
            caller
                .authenticate_native(verifier)
                .await
                .map_err(|_| denied())?
        } else {
            caller
        };
        server
            .execute_user_memory(arguments, &caller, original)
            .await
    }
    .await;
    identity.into_response()
}

impl TaskServer {
    #[expect(
        clippy::result_large_err,
        reason = "The ready HTTP response is returned to the router unchanged."
    )]
    pub(crate) async fn execute_user_memory<'a>(
        &'a self,
        arguments: Arguments,
        caller: &'a graphit_auth::AuthenticatedCaller<'a>,
        original: &'a RequestContext<'a>,
    ) -> Result<browser::BrowserReadOutput, Response> {
        let issuer = self.0.auth.issuer();
        let scope = caller
            .user_memory_scope()
            .map_err(|_| failure(StatusCode::FORBIDDEN, "user_memory_access_denied"))?;
        let query: crate::foreground_memory::Query = serde_json::from_value(
            arguments
                .input
                .ok_or_else(|| failure(StatusCode::FORBIDDEN, "invalid_arguments"))?,
        )
        .map_err(|_| failure(StatusCode::FORBIDDEN, "invalid_arguments"))?;
        let output = match query {
            crate::foreground_memory::Query::Mutation(input) => {
                let admission = issuer
                    .admit_authenticated_user_memory_write(
                        caller,
                        original,
                        &scope,
                        graphit_auth::UserMemoryWriteOperation::Memory,
                    )
                    .map_err(|_| {
                        failure(StatusCode::FORBIDDEN, "authentication_or_permission_denied")
                    })?;
                crate::foreground_memory::mutation::user(
                    original.snapshot(),
                    &admission,
                    &self.0.policy,
                    *input,
                )
                .await
                .map(browser::BrowserReadOutput::MemoryWrite)
                .map_err(|error| browser::ReadFailure::memory(error).into_response())?
            }
            query => {
                let admission = issuer
                    .admit_authenticated_user_memory_read(caller, original, &scope)
                    .map_err(|_| {
                        failure(StatusCode::FORBIDDEN, "authentication_or_permission_denied")
                    })?;
                let bytes = match query {
                    crate::foreground_memory::Query::Search(input) => {
                        crate::foreground_memory::search::user(
                            original,
                            &admission,
                            &self.0.policy,
                            input,
                            self.memory_model()
                                .map_err(|_| failure(StatusCode::FORBIDDEN, "model_failure"))?
                                .as_deref(),
                        )
                        .await
                        .map_err(|error| {
                            browser::ReadFailure::memory_search(error).into_response()
                        })?
                    }
                    query => crate::foreground_user_memory::read(
                        original.snapshot(),
                        &admission,
                        &self.0.policy,
                        query,
                    )
                    .await
                    .map_err(|error| browser::ReadFailure::memory(error).into_response())?,
                };
                admission.recheck().map_err(|_| {
                    failure(StatusCode::FORBIDDEN, "authentication_or_permission_denied")
                })?;
                browser::BrowserReadOutput::Encoded(bytes)
            }
        };
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use graphit_auth::Permission;
    use graphit_core::module_settings::Module;
    use tower::ServiceExt;
    async fn call(
        server: &TaskServer,
        raw: &str,
        operation: &str,
        input: Value,
    ) -> (StatusCode, Value) {
        let response = server
            .native_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/api/records/native/{operation}"))
                    .header("content-type", "application/json")
                    .header("x-graphit-auth-origin", "runtime")
                    .header("authorization", format!("Bearer {raw}"))
                    .body(Body::from(
                        serde_json::to_vec(&json!({"input":input})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        (
            status,
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap(),
        )
    }
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn terminal_memory_retains_confirmed_effect_headers_when_receipt_encoding_fails() {
        use graphit_core::{brand::Brand, project_paths::InstallationPaths};
        let mut relative = std::path::PathBuf::new();
        for _ in 0..30 {
            relative.push("checkout".repeat(10));
        }
        let (temp, mut server, _raw) = super::super::tests::fixture_with_root_names(
            &[Permission::Read(Module::Memory), Permission::WriteMemory],
            &[relative.to_str().unwrap()],
        );
        let client_limits = server.0.policy.content;
        Arc::get_mut(&mut server.0)
            .unwrap()
            .policy
            .content
            .max_output_bytes = 2048;
        let stop = Cancellation::default();
        let original = RequestContext::new(
            client_limits,
            &stop,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        // A legitimate long checkout locator exceeds delivery capacity while
        // the stored Memory record itself remains within the SDK read limit.
        let (_, project_id, _) = server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("native snapshot"))
            .projects[0]
            .clone();
        let long_root = temp.path().join(relative);
        let project = graphit_core::project_paths::ProjectPaths::at(&long_root, &paths).unwrap();
        graphit_core::project_identity::ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &stop)
            .unwrap()
            .ensure_with_id(
                &graphit_core::project_identity::ProjectMetadata::new("long-checkout").unwrap(),
                project_id,
            )
            .unwrap();
        let version = graphit_core::version_context::VersionContext::select(
            &project,
            graphit_core::version_context::Checkout::NonGit,
            "main",
            None,
            "",
        )
        .unwrap();
        server
            .0
            .live_projects
            .replace(Arc::new(vec![(project, project_id, version)]), 0)
            .unwrap();
        let listener = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &stop,
        )
        .unwrap();
        graphit_daemon::source_discovery::publish_discovery(
            &server.0.auth,
            &listener,
            client_limits.max_input_bytes,
            original.snapshot(),
        )
        .unwrap();
        let owner = server.clone();
        let serving = tokio::spawn(async move { owner.serve_listener(listener).await });
        let mut receipt = Vec::new();
        let failure=crate::memory_entry::call_native(&paths,"memory_create",json!({"input":{"idempotency_key":"encoding-failure-receipt","title":"Confirmed write","body":"Confirmed.","attributes":{"classification":"fact","mandatory":false,"important":false},"tags":[]}}),&original,&mut receipt).await.unwrap_err();
        assert!(
            failure
                .to_string()
                .contains("response delivery was incomplete"),
            "{failure}; receipt={}",
            String::from_utf8_lossy(&receipt)
        );
        let receipt: Value = serde_json::from_slice(&receipt).unwrap();
        assert_eq!(receipt["native"]["state"], "completed");
        assert_eq!(receipt["native"]["revision"], 1);
        assert!(receipt["native"]["id"].is_string());
        assert_eq!(receipt["native"]["receipt_source"], "response_headers");
        assert_eq!(receipt["native"]["response_error"], "response_limit");
        assert_eq!(receipt["response_complete"], false);
        assert!(receipt["native"]["idempotency_key"].is_null());
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn native_memory_crud_history_marks_cas_and_denial_use_real_store() {
        let (_temp, server, raw) = super::super::tests::fixture_with_root_names(
            &[Permission::Read(Module::Memory), Permission::WriteMemory],
            &[],
        );
        let (status, empty) = call(&server, &raw, "memory_list", json!({})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(empty["native"]["state"], "no_corpus");
        let input = json!({"idempotency_key":"native-memory-one","title":"Decision","body":"literal body Ω\nsecond line","attributes":{"classification":"decision","mandatory":false,"important":false},"tags":["cli"]});
        let (status, created) = call(&server, &raw, "memory_create", input.clone()).await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let id = created["native"]["id"].clone();
        assert!(id.is_string(), "{created}");
        let (_, repeat) = call(&server, &raw, "memory_create", input).await;
        assert_eq!(repeat["native"]["id"], id);
        let (status,marked)=call(&server,&raw,"memory_update",json!({"id":id,"expected_revision":1,"idempotency_key":"mark-one","reason":"Standing decision","mandatory":true,"important":true})).await;
        assert_eq!(status, StatusCode::OK, "{marked}");
        let (status,_)=call(&server,&raw,"memory_update",json!({"id":id,"expected_revision":1,"idempotency_key":"stale","reason":"Stale write","mandatory":false})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (_, listed) = call(
            &server,
            &raw,
            "memory_list",
            json!({"mandatory":true,"important":true}),
        )
        .await;
        assert_eq!(listed["native"]["memories"].as_array().unwrap().len(), 1);
        let (status, fetched) = call(&server, &raw, "memory_get", json!({"id":id})).await;
        assert_eq!(status, StatusCode::OK, "{fetched}");
        assert!(fetched.to_string().contains("literal body Ω"));
        let (status, found) = call(
            &server,
            &raw,
            "memory_search",
            json!({"text":"literal","top_k":5,"mode":"fts","filter":{}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert!(found.to_string().contains(id.as_str().unwrap()), "{found}");
        let (status, unmarked) = call(&server, &raw, "memory_update", json!({"id":id,"expected_revision":2,"idempotency_key":"unmark-one","reason":"Conditional decision","mandatory":false,"important":false})).await;
        assert_eq!(status, StatusCode::OK, "{unmarked}");
        let (status,deleted)=call(&server,&raw,"memory_delete",json!({"id":id,"expected_revision":3,"idempotency_key":"delete-one","reason":"Explicit removal"})).await;
        assert_eq!(status, StatusCode::OK, "{deleted}");
        let (status, history) = call(&server, &raw, "memory_history", json!({"id":id})).await;
        assert_eq!(status, StatusCode::OK, "{history}");
        assert!(history.to_string().contains("literal body Ω"));
        assert_eq!(
            call(&server, "wrong", "memory_list", json!({})).await.0,
            StatusCode::FORBIDDEN
        );
        server.shutdown().await.unwrap();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn audit_export_paginates_real_history_and_comments_and_publishes_only_after_success() {
        use crate::task_entry::{CallArguments, export};
        use graphit_core::brand::Brand;
        use graphit_core::project_paths::InstallationPaths;
        let (temp, server, raw) = super::super::tests::fixture_with_root_names(&[], &[]);
        async fn mutate(server: &TaskServer, raw: &str, operation: &str, input: Value) -> Value {
            server
                .call(
                    serde_json::from_value(json!({"input":input})).unwrap(),
                    operation,
                    true,
                    Credential {
                        role_binding: None,
                        bearer: zeroize::Zeroizing::new(raw.to_owned()),
                        origin: Origin::Runtime,
                    },
                    CancellationToken::new(),
                )
                .await
                .unwrap_or_else(|error| panic!("{}", error.code))
        }
        let session=mutate(&server,&raw,"create_session",json!({"idempotency_key":"audit-session","title":"Audit","description":"Complete native export","strategy":"Follow all pages"})).await;
        let sid = session["result"]["id"].clone();
        let task=mutate(&server,&raw,"create_task",json!({"session_id":sid,"idempotency_key":"audit-task","title":"Audit Task","description":"Comment paging","task_type":"feature","priority":1,"checks":[{"kind":"acceptance","text":"Export complete audit"},{"kind":"test","text":"Every comment retained"}]})).await;
        let id = task["result"]["specification"]["id"].clone();
        let claimed = mutate(
            &server,
            &raw,
            "claim_task",
            json!({"id":id,"lease_ms":60000}),
        )
        .await;
        let token = claimed["result"]["token"].clone();
        let mut revision = claimed["result"]["record"]["specification"]["revision"]
            .as_u64()
            .unwrap();
        for index in 0..3 {
            let result=mutate(&server,&raw,"add_task_comment",json!({"target":{"id":id,"claim":{"token":token,"expected_revision":revision}},"lease_ms":60000,"idempotency_key":format!("audit-comment-{index}"),"kind":"note","body":format!("Complete comment {index}")})).await;
            revision = result["result"]["result"]["task"]["specification"]["revision"]
                .as_u64()
                .unwrap();
        }
        let cancellation = Cancellation::default();
        let original = RequestContext::new(
            server.0.policy.content,
            &cancellation,
            Instant::now() + Duration::from_secs(90),
        )
        .unwrap();
        let paths = InstallationPaths::resolve(
            Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let listener = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &cancellation,
        )
        .unwrap();
        graphit_daemon::source_discovery::publish_discovery(
            &server.0.auth,
            &listener,
            server.0.policy.content.max_input_bytes,
            original.snapshot(),
        )
        .unwrap();
        let owner = server.clone();
        let serving = tokio::spawn(async move { owner.serve_listener(listener).await });
        let output_path = temp.path().join("audit.json");
        let args = export::Arguments {
            selection: CallArguments::default(),
            id: id.as_str().unwrap().to_owned(),
            output: output_path.clone(),
            page_size: std::num::NonZeroUsize::new(1).unwrap(),
        };
        let base = rmcp::model::CallToolRequestParams::new("graphit_task_get")
            .with_arguments(json!({"input":{"id":id}}).as_object().unwrap().clone());
        let mut confirmation = Vec::new();
        export::run(
            &paths,
            &reqwest::Client::new(),
            base.clone(),
            &args,
            &original,
            &mut confirmation,
        )
        .await
        .unwrap();
        let exported: Value =
            serde_json::from_slice(&std::fs::read(&output_path).unwrap()).unwrap();
        assert_eq!(
            exported["comments"]["comments"].as_array().unwrap().len(),
            3
        );
        assert_eq!(exported["history"]["events"].as_array().unwrap().len(), 4);

        let session_path = temp.path().join("session-audit.json");
        let session_args = export::Arguments {
            selection: CallArguments::default(),
            id: sid.as_str().unwrap().to_owned(),
            output: session_path.clone(),
            page_size: std::num::NonZeroUsize::new(1).unwrap(),
        };
        // More than one linked Task forces native catalogue continuation.
        mutate(&server,&raw,"create_task",json!({"session_id":sid,"idempotency_key":"audit-other-task","title":"Other Task","description":"Catalogue EOF","task_type":"feature","priority":1,"checks":[{"kind":"acceptance","text":"Retain linked Task"},{"kind":"test","text":"Read catalogue EOF"}]})).await;
        let session_base = rmcp::model::CallToolRequestParams::new("graphit_task_session_get")
            .with_arguments(json!({"input":{"id":sid}}).as_object().unwrap().clone());
        export::run(
            &paths,
            &reqwest::Client::new(),
            session_base,
            &session_args,
            &original,
            &mut Vec::new(),
        )
        .await
        .unwrap();
        let session_export: Value =
            serde_json::from_slice(&std::fs::read(&session_path).unwrap()).unwrap();
        assert_eq!(
            session_export["linked_tasks"]["tasks"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(session_export["linked_tasks"]["generation"].is_object());
        let previous = std::fs::read(&output_path).unwrap();
        let failed = rmcp::model::CallToolRequestParams::new("graphit_task_get").with_arguments(
            json!({"input":{"id":"00000000000000000000000001"}})
                .as_object()
                .unwrap()
                .clone(),
        );
        assert!(
            export::run(
                &paths,
                &reqwest::Client::new(),
                failed,
                &args,
                &original,
                &mut Vec::new()
            )
            .await
            .is_err()
        );
        assert_eq!(std::fs::read(&output_path).unwrap(), previous);
        // Cancel after an actual first history page has been streamed. The
        // destination remains the previous complete audit, never the temp data.
        let interrupted = Cancellation::default();
        let interrupted_request = RequestContext::new(
            server.0.policy.content,
            &interrupted,
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let complete = std::sync::atomic::AtomicBool::new(false);
        let observed_page = std::sync::atomic::AtomicBool::new(false);
        let exporting = async {
            let result = export::run(
                &paths,
                &reqwest::Client::new(),
                base.clone(),
                &args,
                &interrupted_request,
                &mut Vec::new(),
            )
            .await;
            complete.store(true, std::sync::atomic::Ordering::SeqCst);
            result
        };
        let interrupt = async {
            while !complete.load(std::sync::atomic::Ordering::SeqCst) {
                for file in std::fs::read_dir(temp.path()).unwrap().flatten() {
                    if file.file_name().to_string_lossy().starts_with(".audit-") {
                        let bytes = std::fs::read(file.path()).unwrap_or_default();
                        let body = String::from_utf8_lossy(&bytes);
                        if body.contains("\"history\":{\"before\":")
                            && body.contains("\"events\":[{")
                        {
                            observed_page.store(true, std::sync::atomic::Ordering::SeqCst);
                            interrupted.cancel();
                            return;
                        }
                    }
                }
                tokio::task::yield_now().await;
            }
        };
        let (cancelled, ()) = tokio::join!(exporting, interrupt);
        assert!(
            observed_page.load(std::sync::atomic::Ordering::SeqCst),
            "must cancel after a real history page"
        );
        assert!(cancelled.is_err());
        assert_eq!(std::fs::read(&output_path).unwrap(), previous);
        server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn knowledge_catalogue_lint_uses_actual_eof_generation_and_reports_locations() {
        let (_temp, server, raw) = super::super::tests::fixture_with_root_names(
            &[
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
            ],
            &[],
        );
        let (status, missing) = call(&server, &raw, "knowledge_list", json!({})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(missing["state"], "no_corpus");
        let first =
            super::super::browser::records_fixture::page("first", "First", &["missing-target"]);
        let mut second = super::super::browser::records_fixture::page("second", "Second", &[]);
        second.source_path = "docs/second.md";
        super::super::browser::knowledge_fixture::seed(&server, &raw, 0, &[first, second]).await;
        let (status, one) = call(&server, &raw, "knowledge_list", json!({"page_size":1})).await;
        assert_eq!(status, StatusCode::OK, "{one}");
        assert_eq!(one["documents"].as_array().unwrap().len(), 1);
        assert!(one["next_cursor"].is_object());
        let (_, two) = call(
            &server,
            &raw,
            "knowledge_list",
            json!({"page_size":1,"cursor":one["next_cursor"]}),
        )
        .await;
        assert_ne!(one["documents"][0]["slug"], two["documents"][0]["slug"]);
        assert_eq!(one["generation"], two["generation"]);
        assert!(two["next_cursor"].is_null());
        assert_eq!(
            call(
                &server,
                &raw,
                "knowledge_list",
                json!({"page_size":1,"filter":{"kind":"other"},"cursor":one["next_cursor"]})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let (status, lint) = call(&server, &raw, "knowledge_lint", json!({})).await;
        assert_eq!(status, StatusCode::OK, "{lint}");
        assert!(lint["issue_count"].as_u64().unwrap() > 0);
        assert!(
            lint["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["code"] == "broken_link" && f["location"]["source_path"].is_string())
        );
        server.shutdown().await.unwrap();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn references_keep_foreign_targets_as_data_and_retain_origin_admission() {
        let (_temp, server, raw) = super::super::tests::fixture_with_root_names(&[], &[]);
        let credential = || Credential {
            role_binding: None,
            bearer: zeroize::Zeroizing::new(raw.clone()),
            origin: Origin::Runtime,
        };
        let session=server.call(serde_json::from_value(json!({"input":{"idempotency_key":"reference-session","title":"Source","description":"Reference authority","strategy":"Never open target"}})).unwrap(),"create_session",true,credential(),CancellationToken::new()).await.unwrap_or_else(|e|panic!("{}",e.code));
        let foreign = "00000000000000000000000001";
        let task=server.call(serde_json::from_value(json!({"input":{"idempotency_key":"reference-task","session_id":session["result"]["id"],"title":"Source","description":"Logical target only","task_type":"feature","priority":1,"checks":[{"kind":"acceptance","text":"Preserve target scope"},{"kind":"test","text":"No foreign corpus read"}],"references":[{"target":{"type":"knowledge","id":"external-guide","scope":"project","scope_id":foreign,"context":"imported@1"},"relation":"depends_on"}]}})).unwrap(),"create_task",true,credential(),CancellationToken::new()).await.unwrap_or_else(|e|panic!("{}",e.code));
        let id = task["result"]["specification"]["id"].clone();
        let (status, references) = call(
            &server,
            &raw,
            "task_references",
            json!({"focus":{"task":id},"page_size":1}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{references}");
        let mut collected = references.to_string();
        let mut cursor = references["published"]["outgoing"]["next_cursor"].clone();
        while !cursor.is_null() {
            let (status, page) = call(
                &server,
                &raw,
                "task_references",
                json!({"focus":{"task":id},"page_size":1,"outgoing_cursor":cursor}),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            collected.push_str(&page.to_string());
            cursor = page["published"]["outgoing"]["next_cursor"].clone();
        }
        assert!(collected.contains("external-guide"), "{collected}");
        assert!(collected.contains(foreign));
        assert!(collected.contains("imported@1"));
        let (status, backlinks) = call(&server, &raw, "task_references", json!({"target":{"target_kind":"knowledge","scope":{"project":foreign},"id":"external-guide","context":"imported@1"},"page_size":1})).await;
        assert_eq!(status, StatusCode::OK, "{backlinks}");
        assert_eq!(
            backlinks["published"]["incoming"]["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(backlinks.to_string().contains(id.as_str().unwrap()));

        let (status, _) = call(
            &server,
            &raw,
            "knowledge_references",
            json!({"outgoing":"external-guide","page_size":1}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            call(
                &server,
                "wrong",
                "task_references",
                json!({"focus":{"task":id},"page_size":1})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        server.shutdown().await.unwrap();
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn index_requests_use_actual_supervisor_and_native_publication_without_claiming_early_completion()
     {
        use graphit_daemon::{
            supervisor::{ProjectContext, ProjectSupervisor},
            watcher::WatchConfig,
        };
        let (_temp, server, raw) = super::super::tests::fixture_with_root_names(
            &[
                Permission::Read(Module::Knowledge),
                Permission::MutateModule(Module::Knowledge),
                Permission::Configure,
            ],
            &[],
        );
        let snapshot = server.0.live_projects.snapshot().unwrap();
        let (paths, id, version) = snapshot.projects[0].clone();
        let context = ProjectContext { id, version };
        let key = context.key();
        let (status, missing) = call(&server, &raw, "index_status", json!({})).await;
        assert_eq!(status, StatusCode::OK, "{missing}");
        assert_eq!(missing["state"], "no_corpus");
        let control = Arc::clone(&server.0.indices);
        let owner = server.clone();
        let token = raw.clone();
        let mut initial = true;
        let supervisor = ProjectSupervisor::start(
            context,
            WatchConfig {
                root: paths.root().to_owned(),
                ast: Default::default(),
                wiki: Default::default(),
                control_files: vec![],
                control_directories: vec![],
                debounce: Duration::from_millis(10),
            },
            move |context, batch, _cancel| {
                let control = Arc::clone(&control);
                let owner = owner.clone();
                let token = token.clone();
                let first = std::mem::replace(&mut initial, false);
                async move {
                    let ticket = control.begin_batch(&context.key(), batch.rescan);
                    control.observe(
                        &context.key(),
                        "knowledge.documents_lexical",
                        "running",
                        None,
                        None,
                    );
                    let page =
                        super::super::browser::records_fixture::page("indexed", "Indexed", &[]);
                    // Reconcile the actual native Knowledge producer, not a fake completion counter.
                    if first {
                        super::super::browser::knowledge_fixture::seed(&owner, &token, 0, &[page])
                            .await;
                    }
                    control.observe(
                        &context.key(),
                        "knowledge.documents_lexical",
                        "completed",
                        None,
                        None,
                    );
                    control.end_batch(&context.key(), ticket);
                    Ok(())
                }
            },
        )
        .await
        .unwrap();
        server
            .0
            .indices
            .attach(Arc::from(vec![supervisor.rescan_handle()]))
            .unwrap();
        let (status, accepted) = call(&server, &raw, "index_run", json!({})).await;
        assert_eq!(status, StatusCode::OK, "{accepted}");
        assert_eq!(accepted["accepted"], true);
        let (status, followed) = call(&server, &raw, "index_run", json!({"wait_ms":2000})).await;
        assert_eq!(status, StatusCode::OK, "{followed}");
        assert_eq!(followed["processed"], true, "{followed}");
        assert_eq!(followed["timed_out"], false);
        let (_, catalogue) = call(&server, &raw, "knowledge_list", json!({})).await;
        assert_eq!(catalogue["documents"].as_array().unwrap().len(), 1);
        assert!(
            server.0.indices.snapshot(&key).unwrap().phases["knowledge.documents_lexical"]
                .last_completed_at
                .is_some()
        );
        supervisor.stop().await.unwrap();
        assert_eq!(
            call(&server, &raw, "index_run", json!({})).await.0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        server.shutdown().await.unwrap();
    }
}
