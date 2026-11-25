//! Root read tools delegate to the same authenticated Native record producers as CLI/UI.
use super::*;
use axum::extract::{Path, State as AxumState};
pub(super) const INDEX_STATUS: &str = "graphit_index_status";
pub(super) const INDEX_RUN: &str = "graphit_index_run";
pub(super) const LIST: &str = "graphit_knowledge_list";
pub(super) const TASK_REFERENCES: &str = "graphit_task_references";
pub(super) const KNOWLEDGE_REFERENCES: &str = "graphit_knowledge_references";
pub(super) const LINT: &str = "graphit_knowledge_lint";
pub(super) fn definitions() -> Vec<Tool> {
    let filter = json!({"type":"object","properties":{"kind":{"type":"string"},"source_prefix":{"type":"string"},"stale":{"type":"boolean"},"metadata_errors":{"type":"boolean"}},"additionalProperties":false});
    let mut tools:Vec<Tool>=[(LIST,"knowledge_list",json!({"filter":filter,"page_size":{"type":"integer","minimum":1,"default":25},"cursor":{"type":"object"}})),(LINT,"knowledge_lint",json!({"max_age_days":{"type":"integer","minimum":1}}))].into_iter().map(|(name,_,properties)|Tool::new(name,
        "Read the authorized PROJECT Knowledge catalogue or run complete published lint. Generation and no-corpus are explicit; published metadata locations retain slug/source/field without invented authored line numbers. Selectors grant no access.",
        json!({"type":"object","properties":{"selection":crate::task_server::catalog::selection(),"input":{"type":"object","properties":properties,"additionalProperties":false}},"required":["input"],"additionalProperties":false}).as_object().unwrap().clone())).collect();
    for (name, input) in [
        (
            TASK_REFERENCES,
            crate::role_tool_server::references::task_input_schema(),
        ),
        (
            KNOWLEDGE_REFERENCES,
            json!({"type":"object","properties":{"outgoing":{"type":["string","null"]},"incoming":{"type":["object","null"],"properties":{"target_kind":{"enum":["task","session","memory","knowledge"]},"scope":{"type":"object","properties":{"project":{"type":"string"},"user":{"type":"string"}},"oneOf":[{"required":["project"]},{"required":["user"]}],"additionalProperties":false},"id":{"type":"string"},"context":{"type":["string","null"]}},"required":["target_kind","scope","id"],"additionalProperties":false},"page_size":{"type":"integer","minimum":1},"outgoing_cursor":{"type":["object","null"]},"incoming_cursor":{"type":["object","null"]}},"required":["page_size"],"additionalProperties":false}),
        ),
    ] {
        tools.push(Tool::new(name,"Read incoming and outgoing references from the admitted origin corpus. Foreign targets remain logical data, never authority or an implicit content read.",json!({"type":"object","properties":{"selection":crate::task_server::catalog::selection(),"input":input},"required":["input"],"additionalProperties":false}).as_object().unwrap().clone()));
    }
    for (name, description) in [
        (
            INDEX_STATUS,
            "Observe actual AST/Knowledge producer phases and last completion/error without running an indexer.",
        ),
        (
            INDEX_RUN,
            "Request a coalesced full-project rescan from the current supervisor. Configure authority is required. Acceptance is not completion; optional wait_ms follows observations and never cancels shared processing.",
        ),
    ] {
        tools.push(Tool::new(name,description,json!({"type":"object","properties":{"selection":crate::task_server::catalog::selection(),"input":{"type":"object","properties":{"wait_ms":{"type":"integer","minimum":0},"module":{"type":["string","null"],"enum":["ast","knowledge",null]}},"additionalProperties":false}},"required":["input"],"additionalProperties":false}).as_object().unwrap().clone()));
    }
    tools
}
pub(super) async fn call(
    server: &SourceServer,
    call: CallToolRequestParams,
    context: McpRequestContext<RoleServer>,
) -> CallToolResult {
    let operation = match call.name.as_ref() {
        INDEX_STATUS => "index_status",
        INDEX_RUN => "index_run",
        LIST => "knowledge_list",
        LINT => "knowledge_lint",
        TASK_REFERENCES => "task_references",
        KNOWLEDGE_REFERENCES => "knowledge_references",
        _ => return failure("unknown record tool"),
    };
    let Some(parts) = context.extensions.get::<Parts>() else {
        return failure("native credentials required");
    };
    let tasks = match server.task_server() {
        Ok(value) => value,
        Err(_) => return failure("record service unavailable"),
    };
    let bytes = match serde_json::to_vec(&Value::Object(call.arguments.unwrap_or_default())) {
        Ok(value) => value,
        Err(_) => return failure("invalid arguments"),
    };
    if bytes.len() > server.0.policy.content.max_input_bytes {
        return failure("input limit");
    }
    let response = tokio::select! {
        response=crate::task_server::native_records::http(AxumState(tasks),Path(operation.to_owned()),parts.headers.clone(),bytes.into())=>response,
        ()=context.ct.cancelled()=>return failure("request cancelled"),
        ()=server.0.stop.cancelled()=>return failure("service stopped"),
    };
    let status = response.status();
    let bytes = match axum::body::to_bytes(
        response.into_body(),
        server.0.policy.content.max_output_bytes,
    )
    .await
    {
        Ok(value) => value,
        Err(_) => return failure("result limit"),
    };
    let value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return failure("invalid record response"),
    };
    if status.is_success() {
        crate::mcp_output::typed(value)
    } else {
        crate::mcp_output::typed_error(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphit_auth::Permission;
    use graphit_core::{brand::Brand, module_settings::Module};
    use graphit_hub::NativeBinding;
    use rmcp::ServiceExt;
    #[tokio::test]
    async fn documented_public_schemas_match_registered_definitions() {
        let fixture =
            super::super::test_fixture::fixture(&[], Brand::default(), NativeBinding::Codex);
        let mut definitions = definitions();
        definitions.extend(super::super::projects::definitions());
        definitions.extend(
            crate::task_server::catalog::tools()
                .iter()
                .filter(|tool| tool.name == "graphit_task_session_cancel")
                .cloned(),
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/reference/mcp-input-schemas.json");
        let snapshot: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for tool in definitions
            .into_iter()
            .map(crate::mcp_output::decorate_tool)
        {
            let documented = snapshot["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["name"] == tool.name.as_ref())
                .unwrap_or_else(|| panic!("missing documented tool {}", tool.name));
            assert_eq!(
                *documented,
                serde_json::to_value(&tool).unwrap(),
                "{}",
                tool.name
            );
            // Root get_tool applies the same formatting contract. Task tools are
            // attached separately in the execution test below.
            if tool.name != "graphit_task_session_cancel"
                && ![
                    LIST,
                    LINT,
                    TASK_REFERENCES,
                    KNOWLEDGE_REFERENCES,
                    INDEX_STATUS,
                    INDEX_RUN,
                ]
                .contains(&tool.name.as_ref())
            {
                assert_eq!(
                    fixture.server.get_tool(&tool.name).unwrap().input_schema,
                    tool.input_schema
                );
            }
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn root_and_stdio_announce_and_execute_actual_record_operations() {
        let mut source = super::super::test_fixture::fixture(
            &[
                Permission::Read(Module::Task),
                Permission::Read(Module::Knowledge),
                Permission::Read(Module::Ast),
                Permission::WriteTask,
                Permission::WriteOwnTask,
                Permission::CoordinateTask,
                Permission::Configure,
            ],
            Brand::default(),
            NativeBinding::Codex,
        );
        let policy = crate::foreground_tasks::task_policy(
            &source.server.0.policy.native_install.hub,
            NativeBinding::Codex,
        )
        .unwrap();
        source.server.attach_tasks(policy).unwrap();
        let cancel = Cancellation::default();
        let original = RequestContext::new(
            source.server.0.policy.content,
            &cancel,
            Instant::now() + Duration::from_secs(90),
        )
        .unwrap();
        let socket = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(5),
            },
            &cancel,
        )
        .unwrap();
        graphit_daemon::source_discovery::publish_discovery(
            &source.server.0.auth,
            &socket,
            source.server.0.policy.content.max_input_bytes,
            original.snapshot(),
        )
        .unwrap();
        let address = socket.address();
        let server = source.server.clone();
        let serving = tokio::spawn(async move { SourceServer::serve(&server, socket).await });
        let root = &source.projects[0].0;
        let proxy = crate::mcp_source::SourceProxy::new(
            source.paths.clone(),
            address,
            crate::source_server::test_http_client(),
            (
                source.server.0.policy.content,
                source.server.0.policy.timeout,
            ),
            Some((root.root(), Some("main"))),
            &original,
        )
        .await
        .unwrap();
        let (local, remote) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(remote);
        let owner = proxy.clone();
        let served_proxy = tokio::spawn(async move { owner.serve_stream(read, write).await });
        let client = ().serve(local).await.unwrap();
        let tools = client.peer().list_all_tools().await.unwrap();
        if std::env::var_os("GRAPHIT_UPDATE_MCP_SCHEMAS").is_some() {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/reference/mcp-input-schemas.json");
            let snapshot = json!({"description":"Rust Root MCP input catalogue captured from the actual authenticated SourceProxy tools/list using the Codex binding. Availability varies by transport, modules, permissions, platform and running version. Role/Live catalogues are separate.","tools":tools});
            std::fs::write(
                path,
                serde_json::to_string_pretty(&snapshot).unwrap() + "\n",
            )
            .unwrap();
        }

        for tool in [
            LIST,
            LINT,
            TASK_REFERENCES,
            KNOWLEDGE_REFERENCES,
            INDEX_STATUS,
            INDEX_RUN,
        ] {
            assert!(tools.iter().any(|v| v.name == tool));
            assert!(source.server.get_tool(tool).is_some());
        }
        async fn call(
            client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
            name: &str,
            input: Value,
        ) -> CallToolResult {
            client
                .peer()
                .call_tool(
                    CallToolRequestParams::new(name.to_owned()).with_arguments(
                        json!({"input":input,"ai_optimized":false})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await
                .unwrap()
        }
        let session=call(&client,"graphit_task_session_create",json!({"idempotency_key":"root-records","title":"Root fixture","description":"Real dispatch","strategy":"Use actual admission"})).await;
        assert_ne!(session.is_error, Some(true), "{session:?}");
        let id = session.structured_content.as_ref().unwrap()["result"]["id"].clone();
        for (name, input) in [
            (LIST, json!({})),
            (LINT, json!({})),
            (
                TASK_REFERENCES,
                json!({"focus":{"session":id},"page_size":1}),
            ),
            (
                KNOWLEDGE_REFERENCES,
                json!({"outgoing":"absent-document","page_size":1}),
            ),
            (INDEX_STATUS, json!({})),
        ] {
            let result = call(&client, name, input).await;
            assert_ne!(result.is_error, Some(true), "{name}: {result:?}");
            assert!(result.structured_content.is_some());
        }
        let request = call(&client, INDEX_RUN, json!({})).await;
        assert_eq!(request.is_error, Some(true));
        assert!(
            request
                .structured_content
                .as_ref()
                .unwrap()
                .to_string()
                .contains("supervisor_unavailable")
        );
        client.cancel().await.unwrap();
        served_proxy.await.unwrap().unwrap();
        proxy.shutdown().await.unwrap();
        source.server.shutdown().await.unwrap();
        serving.await.unwrap().unwrap();
    }
}
