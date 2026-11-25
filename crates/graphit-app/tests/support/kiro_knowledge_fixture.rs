//! Native ACP fixture using the pinned SDK dispatcher and stdio transport.
use agent_client_protocol::{
    Agent, Error, Stdio,
    schema::{ProtocolVersion, v1::*},
};
use std::{
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
const SESSION: &str = "private-kiro-fixture-session";
fn journal(event: &str) {
    if let Some(path) = std::env::var_os("KIRO_FIXTURE_JOURNAL") {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("fixture journal");
        // Other fixture children may append to this journal concurrently.
        f.write_all(format!("{event}\n").as_bytes())
            .expect("fixture journal write");
    }
}
fn offer(model: &str, current: &str) -> SessionConfigOption {
    SessionConfigOption::select(
        "model",
        "Model",
        SessionConfigValueId::new(current),
        SessionConfigSelectOptions::Ungrouped(vec![SessionConfigSelectOption::new(
            model.to_owned(),
            "Fixture model",
        )]),
    )
}
fn text(session: impl Into<SessionId>, text: impl Into<String>) -> SessionNotification {
    SessionNotification::new(
        session,
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            text,
        )))),
    )
}
fn check_session(cwd: &Path, meta: &Option<Meta>, model: &str) -> Result<(), Error> {
    if cwd != std::env::current_dir().map_err(Error::into_internal_error)?
        || std::env::var("KIRO_CAPTURED").as_deref() != Ok("literal-captured")
    {
        return Err(Error::internal_error());
    }
    if meta
        .as_ref()
        .and_then(|m| m.get("kiro"))
        .and_then(|m| m.get("modelId"))
        .and_then(|m| m.as_str())
        != Some(model)
    {
        return Err(Error::internal_error());
    }
    journal("literal_inputs_checked");
    Ok(())
}

/// Controlled ACP peer qualification: real rendered assets and the real Graphit stdio proxy.
async fn check_live_http(cwd: &Path) -> Result<(), Error> {
    use rmcp::service::{ClientLifecycleMode, ClientServiceExt};
    use std::process::Stdio;
    let document = std::fs::read_to_string(cwd.join(".kiro/agents/live-agent.md"))
        .map_err(Error::into_internal_error)?;
    let (frontmatter, prompt) = document
        .strip_prefix("---\n")
        .and_then(|content| content.split_once("\n---\n"))
        .ok_or_else(Error::internal_error)?;
    let profile: serde_json::Value =
        serde_saphyr::from_str(frontmatter).map_err(Error::into_internal_error)?;
    if std::fs::read_to_string(cwd.join(".kiro/agents/prompt.md"))
        .map_err(Error::into_internal_error)?
        != "agent-auxiliary-marker"
        || std::fs::read_to_string(cwd.join(".kiro/skills/live-skill/references/reference.md"))
            .map_err(Error::into_internal_error)?
            != "skill-auxiliary-marker"
        || !prompt.contains("rule-from-installed-source")
        || profile["includeMcpJson"] != false
        || profile["includePowers"] != false
    {
        return Err(Error::internal_error());
    }
    journal("live_assets_checked");
    let servers = profile["mcpServers"]
        .as_object()
        .ok_or_else(Error::internal_error)?;
    if servers.len() != 1 {
        return Err(Error::internal_error());
    }
    let (namespace, server) = servers.iter().next().ok_or_else(Error::internal_error)?;
    let ast_context = std::env::var("KIRO_FIXTURE_AST_CONTEXT")
        .ok()
        .map(|value| serde_json::from_str::<serde_json::Value>(&value))
        .transpose()
        .map_err(Error::into_internal_error)?;
    let mut expected = vec![
        "graphit_live_context",
        "graphit_knowledge_heads",
        "graphit_knowledge_page",
        "graphit_knowledge_search",
    ];
    if ast_context.is_some() {
        expected.extend([
            "graphit_ast_source",
            "graphit_ast_schema",
            "graphit_ast_query",
            "graphit_ast_search",
        ]);
    }
    let rules = profile["permissions"]["rules"]
        .as_array()
        .ok_or_else(Error::internal_error)?;
    if rules.len() != expected.len() + 2
        || expected.iter().any(|tool| {
            !rules.iter().any(|rule| {
                rule["capability"] == "mcp"
                    && rule["effect"] == "allow"
                    && rule["match"] == serde_json::json!([format!("{namespace}/{tool}")])
            })
        })
        || ["fs_write", "shell"].iter().any(|capability| {
            !rules
                .iter()
                .any(|rule| rule["capability"] == *capability && rule["effect"] == "deny")
        })
    {
        return Err(Error::internal_error());
    }
    let command = server["command"]
        .as_str()
        .ok_or_else(Error::internal_error)?;
    let args = server["args"]
        .as_array()
        .ok_or_else(Error::internal_error)?
        .iter()
        .map(|value| value.as_str().ok_or_else(Error::internal_error))
        .collect::<Result<Vec<_>, _>>()?;
    if command != env!("GRAPHIT_COMPILED_BUILD_BRAND") || args != ["mcp", "--stdio", "--live-user"]
    {
        return Err(Error::internal_error());
    }
    // Exercise the same named environment filter without requiring a vendor host.
    let brand = graphit_core::brand::Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))
        .map_err(Error::into_internal_error)?;
    let inherited = graphit_app::project_hook_install::client::mcp_environment_variables(
        &brand,
        graphit_hub::NativeBinding::Codex,
    )
    .map_err(Error::into_internal_error)?;
    let environment = std::env::vars_os().filter(|(name, _)| {
        name == "PATH" || name == "HOME" || inherited.iter().any(|allowed| name == allowed.as_str())
    });
    let mut child = tokio::process::Command::new(command)
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(Error::into_internal_error)?;
    let input = child.stdin.take().ok_or_else(Error::internal_error)?;
    let output = child.stdout.take().ok_or_else(Error::internal_error)?;
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let connection = ()
            .serve_with_lifecycle(
                (output, input),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::LATEST],
                },
            )
            .await
            .map_err(Error::into_internal_error)?;
        let calls = async {
            let tools = connection
                .peer()
                .list_all_tools()
                .await
                .map_err(Error::into_internal_error)?;
            if tools
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>()
                != expected
            {
                return Err(Error::internal_error());
            }
            if let Some(context) = &ast_context {
                for tool in &tools[4..] {
                    if tool.input_schema.get("properties")
                        .and_then(|v| v.get("context"))
                        .and_then(|v| v.get("enum"))
                        .and_then(serde_json::Value::as_array)
                        .map(Vec::as_slice) != Some(std::slice::from_ref(context))
                    {
                        return Err(Error::internal_error());
                    }
                }
            }
            for (name, arguments) in [
                (expected[0], serde_json::json!({})),
                (expected[1], serde_json::json!({})),
                (expected[2], serde_json::json!({"slug":"same"})),
                (
                    expected[3],
                    serde_json::json!({"search":{"query":"Live","mode":"fts"}}),
                ),
            ] {
                let result = connection
                    .peer()
                    .call_tool(
                        rmcp::model::CallToolRequestParams::new(name).with_arguments(
                            arguments
                                .as_object()
                                .ok_or_else(Error::internal_error)?
                                .clone(),
                        ),
                    )
                    .await
                    .map_err(Error::into_internal_error)?;
                if result.is_error == Some(true) {
                    return Err(Error::internal_error());
                }
            }
            if let Some(context) = &ast_context {
                for (name, arguments) in [
                    ("graphit_ast_source", serde_json::json!({"ai_optimized":false,"context":context,"path":"browser_search.rs","head":1})),
                    ("graphit_ast_schema", serde_json::json!({"ai_optimized":false,"context":context})),
                    ("graphit_ast_query", serde_json::json!({"ai_optimized":false,"context":context,"query":"MATCH (n:Function) RETURN n.name","parameters":{}})),
                    ("graphit_ast_search", serde_json::json!({"ai_optimized":false,"context":context,"query":"browser_search_sum","mode":"fts","k":8})),
                ] {
                    let reply = connection.peer().call_tool(
                        rmcp::model::CallToolRequestParams::new(name)
                            .with_arguments(arguments.as_object().ok_or_else(Error::internal_error)?.clone())
                    ).await.map_err(Error::into_internal_error)?;
                    if reply.is_error == Some(true) {
                        journal(&format!("live_mcp_tool_error:{name}"));
                        return Err(Error::internal_error());
                    }
                    let value = reply.structured_content.as_ref().ok_or_else(|| {
                        journal(&format!("live_mcp_missing_json:{name}"));
                        Error::internal_error()
                    })?;
                    let observed = match name {
                        "graphit_ast_source" => value["selection"]["source"].as_str().is_some_and(|s| s.contains("browser_search_sum")),
                        "graphit_ast_schema" => value.to_string().contains("Function"),
                        "graphit_ast_query" => value.to_string().contains("browser_search_sum"),
                        "graphit_ast_search" => value["results"].as_array().is_some_and(|hits| hits.iter().any(|h| h["name"] == "browser_search_sum")),
                        _ => false,
                    };
                    if !observed {
                        journal(&format!("live_mcp_unexpected_result:{name}"));
                        return Err(Error::internal_error());
                    }
                }
            }
            Ok(())
        }
        .await;
        let closed = connection
            .cancel()
            .await
            .map_err(Error::into_internal_error);
        calls?;
        closed?;
        Ok(())
    })
    .await
    .map_err(Error::into_internal_error)
    .and_then(|result| result);
    let status = match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(status) => status.map_err(Error::into_internal_error)?,
        Err(_) => {
            child.kill().await.map_err(Error::into_internal_error)?;
            child.wait().await.map_err(Error::into_internal_error)?;
            return Err(Error::internal_error());
        }
    };
    result?;
    if !status.success() {
        return Err(Error::internal_error());
    }
    journal(if ast_context.is_some() {
        "live_mcp_eight_tools_joined"
    } else {
        "live_mcp_four_tools_joined"
    });
    Ok(())
}

async fn fixture() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() != ["acp", "--agent-engine=v3"] {
        return Err(Error::internal_error());
    }
    let model = std::env::var("KIRO_FIXTURE_MODEL").map_err(Error::into_internal_error)?;
    journal("engine_v3");
    let mode = std::env::var("KIRO_FIXTURE_MODE").unwrap_or_else(|_| "answer".into());
    // These scenarios deliberately retain the child through TERM until the
    // actual Process owner completes its grace/kill/join path.
    #[cfg(unix)]
    let _term = if matches!(mode.as_str(), "cancel_delay_exit" | "clear_delay_exit") {
        Some(
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(Error::into_internal_error)?,
        )
    } else {
        None
    };
    #[cfg(unix)]
    let _term_ack = if mode == "answer_exit_gate" {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(Error::into_internal_error)?;
        Some(tokio::spawn(async move {
            while signal.recv().await.is_some() {
                journal("term_received");
            }
        }))
    } else {
        None
    };
    let requested = Arc::new(model);
    let selected = Arc::new(Mutex::new(String::new()));
    let new_model = Arc::clone(&requested);
    let load_model = Arc::clone(&requested);
    let set_model = Arc::clone(&requested);
    let set_selected = Arc::clone(&selected);
    let prompt_selected = Arc::clone(&selected);
    let prompt_mode = mode.clone();
    let load_mode = mode.clone();
    let new_mode = mode.clone();
    let initialize_mode = mode.clone();
    let delete_mode = mode.clone();
    let result = Agent
        .builder()
        .on_receive_request(
            async move |_: InitializeRequest, responder, cx| {
                journal("initialize");
                let sessions = if initialize_mode == "clear_unsupported" {
                    SessionCapabilities::new()
                } else {
                    SessionCapabilities::new().delete(SessionDeleteCapabilities::new())
                };
                let response = InitializeResponse::new(ProtocolVersion::V1).agent_capabilities(
                    AgentCapabilities::new()
                        .load_session(true)
                        .session_capabilities(sessions),
                );
                if initialize_mode == "clear_init_gate" {
                    journal("initialize_gate");
                    return cx.spawn(async move {
                        let path = std::env::var_os("KIRO_FIXTURE_JOURNAL")
                            .ok_or_else(Error::internal_error)?;
                        let gate = std::path::PathBuf::from(path).with_extension("init");
                        tokio::time::timeout(Duration::from_secs(10), async {
                            while !gate.exists() {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        })
                        .await
                        .map_err(Error::into_internal_error)?;
                        responder.respond(response)
                    });
                }
                responder.respond(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: DeleteSessionRequest, responder, cx| {
                if request.session_id.0.as_ref() != SESSION
                    || request.meta.is_some()
                    || std::env::var("KIRO_CAPTURED").as_deref() != Ok("literal-captured")
                {
                    return Err(Error::internal_error());
                }
                journal("delete");
                if delete_mode == "clear_error" {
                    return responder
                        .respond_with_internal_error("fixture native deletion failure");
                }
                if delete_mode == "clear_pending" {
                    return cx.spawn(async move {
                        std::future::pending::<()>().await;
                        responder.respond(DeleteSessionResponse::new())
                    });
                }
                let gated = delete_mode == "clear_gate" || delete_mode == "clear_delay_exit";
                if !gated {
                    cx.send_notification(text(SESSION, "native list deletion update"))?;
                }
                cx.spawn(async move {
                    if gated {
                        let path = std::env::var_os("KIRO_FIXTURE_JOURNAL")
                            .ok_or_else(Error::internal_error)?;
                        let gate = std::path::PathBuf::from(path).with_extension("clear");
                        tokio::time::timeout(Duration::from_secs(10), async {
                            while !gate.exists() {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        })
                        .await
                        .map_err(Error::into_internal_error)?;
                    }
                    journal("delete_response");
                    responder.respond(DeleteSessionResponse::new())
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: NewSessionRequest, responder, _| {
                check_session(&request.cwd, &request.meta, &new_model)?;
                if !request.mcp_servers.is_empty() || !request.additional_directories.is_empty() {
                    return Err(Error::internal_error());
                }
                if new_mode == "live_http" || new_mode == "live_browser" {
                    check_live_http(&request.cwd).await?;
                }
                journal("new");
                let response = NewSessionResponse::new(if new_mode == "id_limit" {
                    "x".repeat(4097)
                } else {
                    SESSION.into()
                });
                responder.respond(response.config_options(vec![offer(
                        if new_mode == "offer_mismatch" {
                            "other-model"
                        } else {
                            &new_model
                        },
                        "before-select",
                    )]))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: LoadSessionRequest, responder, cx| {
                check_session(&request.cwd, &request.meta, &load_model)?;
                if request.session_id.0.as_ref() != SESSION
                    || !request.mcp_servers.is_empty()
                    || !request.additional_directories.is_empty()
                {
                    return Err(Error::internal_error());
                }
                if load_mode == "live_http" || load_mode == "live_browser" {
                    check_live_http(&request.cwd).await?;
                }
                journal("load");
                if load_mode == "load_error" {
                    return responder.respond_with_internal_error("fixture Load failure");
                }
                cx.send_notification(text(
                    SESSION,
                    "old history must not become the fresh answer",
                ))?;
                let response = LoadSessionResponse::new();
                responder.respond(response.config_options(vec![offer(&load_model, "before-select")]))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: SetSessionConfigOptionRequest, responder, _| {
                journal("select");
                if request.session_id.0.as_ref() != SESSION
                    || request.config_id.0.as_ref() != "model"
                    || request.value.as_value_id().map(|id| id.0.as_ref())
                        != Some(set_model.as_str())
                {
                    return Err(Error::internal_error());
                }
                *set_selected.lock().expect("fixture selected") = set_model.to_string();
                responder.respond(SetSessionConfigOptionResponse::new(vec![offer(
                    &set_model, &set_model,
                )]))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, cx| {
                journal("prompt");
                if request.session_id.0.as_ref() != SESSION
                    || prompt_selected.lock().expect("fixture selected").as_str() != requested.as_str()
                {
                    return Err(Error::internal_error());
                }
                let [ContentBlock::Text(content)] = request.prompt.as_slice() else {
                    return Err(Error::internal_error());
                };
                if prompt_mode == "live_browser" {
                    if content.text.trim().is_empty() {
                        return Err(Error::internal_error());
                    }
                    journal("live_browser_prompt_checked");
                } else {
                    let rendered: serde_json::Value =
                        serde_json::from_str(&content.text).map_err(Error::into_internal_error)?;
                    if rendered.get("question").and_then(|v| v.as_str()).is_none()
                        || rendered
                            .get("pages")
                            .and_then(|v| v.as_array())
                            .is_none_or(Vec::is_empty)
                    {
                        return Err(Error::internal_error());
                    }
                    if let Ok(expected) = std::env::var("KIRO_FIXTURE_EXPECTED_QUESTION") {
                        if rendered.get("question").and_then(|v| v.as_str())
                            != Some(expected.as_str())
                            || rendered["pages"][0]["slug"].as_str() != Some("Paris")
                        {
                            return Err(Error::internal_error());
                        }
                        journal("expected_question_checked");
                    }
                    journal("authorized_render_checked");
                }
                if prompt_mode == "stderr_limit" {
                    std::io::stderr()
                        .write_all(&vec![b'x'; 70 * 1024])
                        .map_err(Error::into_internal_error)?;
                }
                cx.send_notification(SessionNotification::new(
                    SESSION,
                    SessionUpdate::ToolCall(
                        ToolCall::new("literal-tool-id", "Fixture observed tool")
                            .raw_input(serde_json::json!({"native":true})),
                    ),
                ))?;
                let chunk = if prompt_mode == "large_frame" {
                    "x".repeat(2 * 1024 * 1024)
                } else if prompt_mode == "live_browser" {
                    "Read the real [Live productive Knowledge](same).\n\n**Browser citation marker.**".into()
                } else {
                    "fresh fixture answer".into()
                };
                cx.send_notification(text(SESSION, chunk))?;
                if prompt_mode == "stream_gate" {
                    // This witnesses the peer enqueue; the stdio test observes actual wire progress.
                    journal(&format!("peer_pid:{}", std::process::id()));
                    journal("progress_sent");
                }
                if prompt_mode == "partial" {
                    return responder.respond_with_internal_error("fixture partial failure");
                }
                if prompt_mode == "cancel" || prompt_mode == "cancel_delay_exit" {
                    return cx.spawn(async move {
                        std::future::pending::<()>().await;
                        responder.respond(PromptResponse::new(StopReason::EndTurn))
                    });
                }
                let gated = prompt_mode == "stream_gate" || prompt_mode == "live_http";
                let exit_gated = prompt_mode == "answer_exit_gate";
                if prompt_mode == "live_http" {
                    journal("live_quiet_wait");
                }
                cx.spawn(async move {
                    if gated {
                        let path = std::env::var_os("KIRO_FIXTURE_JOURNAL")
                            .ok_or_else(Error::internal_error)?;
                        let gate = std::path::PathBuf::from(path).with_extension("finish");
                        tokio::time::timeout(Duration::from_secs(10), async {
                            while !gate.exists() {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        })
                        .await
                        .map_err(Error::into_internal_error)?;
                    } else if !exit_gated {
                        tokio::time::sleep(Duration::from_millis(120)).await;
                    }
                    journal("terminal");
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async |_: CancelNotification, _| {
                journal("cancel");
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(Stdio::new())
        .await;
    journal("driver_closed");
    if mode == "answer_exit_gate" {
        let path = std::env::var_os("KIRO_FIXTURE_JOURNAL").ok_or_else(Error::internal_error)?;
        let gate = std::path::PathBuf::from(path).with_extension("exit");
        journal("exit_gate");
        while !gate.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    if mode == "delay_exit" || mode == "cancel_delay_exit" || mode == "clear_delay_exit" {
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
    // The fixture never prints SDK-owned request payloads or causes.
    if result.is_err() {
        std::process::exit(1);
    }
    Ok(())
}

fn profile_hook_commands(path: &Path) -> Result<(), Error> {
    // Test-only adapter: decode the actual Rust-published profile for the controlled peer.
    // This makes no claim about an installed host loading or perceiving that profile.
    let bytes = std::fs::read(path).map_err(Error::into_internal_error)?;
    let text = std::str::from_utf8(&bytes).map_err(Error::into_internal_error)?;
    let (header, _) = text
        .strip_prefix("---\n")
        .and_then(|text| text.split_once("\n---\n"))
        .ok_or_else(Error::internal_error)?;
    let metadata: serde_json::Value =
        serde_saphyr::from_str(header).map_err(|_| Error::internal_error())?;
    let hooks = metadata["hooks"]
        .as_array()
        .ok_or_else(Error::internal_error)?;
    let commands = ["SessionStart", "UserPromptSubmit"]
        .into_iter()
        .map(|event| {
            let hook = hooks
                .iter()
                .find(|hook| hook["trigger"] == event)
                .ok_or_else(Error::internal_error)?;
            let command = hook["action"]["command"]
                .as_str()
                .ok_or_else(Error::internal_error)?;
            Ok(serde_json::json!({"event":event,"command":command}))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    serde_json::to_writer(std::io::stdout().lock(), &commands).map_err(Error::into_internal_error)
}
#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg == "--profile-hook-commands")
    {
        if args.len() != 2 || profile_hook_commands(Path::new(&args[1])).is_err() {
            std::process::exit(1);
        }
        return;
    }
    if fixture().await.is_err() {
        std::process::exit(1);
    }
}
