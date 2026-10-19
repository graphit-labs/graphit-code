//! Controlled native ACP peer; this executable does not qualify an installed vendor.
use agent_client_protocol::{
    Agent, Error, Stdio, UntypedMessage,
    schema::{ProtocolVersion, v1::*},
};
use serde_json::json;
use std::{
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};
const SESSION: &str = "opencode-fixture-main";
fn journal(event: &str) {
    let path = std::env::var_os("OPENCODE_JOURNAL").expect("private fixture journal");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(file, "{event}").unwrap();
}
fn model_option(current: &str) -> SessionConfigOption {
    serde_json::from_value(json!({"id":"model","name":"Model","category":"model","type":"select",
        "currentValue":current,"options":[{"value":"provider/literal;$(opencode)","name":"Fixture model"}]})).unwrap()
}
fn text(session: &str, value: &str) -> UntypedMessage {
    UntypedMessage::new(
        "session/update",
        json!({"sessionId":session,
        "update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":value}}}),
    )
    .unwrap()
}
#[cfg(target_os = "linux")]
fn marker(extension: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("OPENCODE_JOURNAL").expect("private fixture journal"))
        .with_extension(extension)
}
#[cfg(target_os = "linux")]
async fn descendant() -> std::io::Result<()> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut heartbeat = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(marker("descendant.heartbeat"))?;
    std::fs::write(marker("descendant.pid"), std::process::id().to_string())?;
    std::fs::write(marker("descendant.ready"), b"ready")?;
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    loop {
        tokio::select! {
            _ = term.recv() => std::fs::write(marker("descendant.term"), b"TERM")?,
            _ = interval.tick() => heartbeat.write_all(b".")?,
        }
    }
}
#[cfg(target_os = "linux")]
async fn spawn_descendant() -> Result<(), Error> {
    // Inherit the real ACP child's group and both output pipes. Dropping this std
    // Child does not stop it; Graphit's actual group owner must retire the tree.
    let child =
        std::process::Command::new(std::env::current_exe().map_err(Error::into_internal_error)?)
            .arg("--fixture-descendant")
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(Error::into_internal_error)?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !marker("descendant.ready").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(Error::into_internal_error)?;
    drop(child);
    journal("descendant_ready");
    Ok(())
}
fn native_session_id() -> Result<String, Error> {
    match std::env::var_os("OPENCODE_SESSION_STORE") {
        Some(path) => std::fs::read_to_string(path).map_err(Error::into_internal_error),
        None => Ok(SESSION.into()),
    }
}
async fn control_gate(name: &str) -> Result<(), Error> {
    let path = std::path::PathBuf::from(
        std::env::var_os("OPENCODE_JOURNAL").ok_or_else(Error::internal_error)?,
    )
    .with_extension(format!("{name}.finish"));
    tokio::time::timeout(Duration::from_secs(15), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(Error::into_internal_error)
}
async fn fixture() -> Result<(), Error> {
    // Record entry before validating inputs so a bad launch cannot mimic no spawn.
    journal("spawn");
    if std::env::args().skip(1).collect::<Vec<_>>() != ["acp"]
        || std::env::var("OPENCODE_CAPTURED").as_deref() != Ok("startup literal;$(value)")
    {
        return Err(Error::internal_error());
    }
    let mode = match std::env::var_os("OPENCODE_CONTROL_MODE") {
        Some(path) => std::fs::read_to_string(path).map_err(Error::into_internal_error)?,
        None => std::env::var("OPENCODE_MODE").unwrap_or_else(|_| "answer".into()),
    };
    let initialize_mode = mode.clone();
    let load_mode = mode.clone();
    let selected = Arc::new(Mutex::new(false));
    let set = Arc::clone(&selected);
    let prompt_mode = mode.clone();
    let result = Agent
        .builder()
        .on_receive_request(
            async move |request: InitializeRequest, responder, cx| {
                if request.protocol_version != ProtocolVersion::V1 {
                    return Err(Error::internal_error());
                }
                let value = serde_json::to_value(&request).map_err(Error::into_internal_error)?;
                if value.pointer("/clientCapabilities/terminal") == Some(&json!(true))
                    || value.pointer("/clientCapabilities/fs/readTextFile") == Some(&json!(true))
                    || value.pointer("/clientCapabilities/fs/writeTextFile") == Some(&json!(true))
                {
                    return Err(Error::internal_error());
                }
                journal("initialize");
                let mut native = json!({
                    "protocolVersion":1,"agentInfo":{"name":"OpenCode","version":"2.0.0"},
                    "agentCapabilities":{"mcpCapabilities":{"http":true,"sse":false}}
                });
                if std::env::var_os("OPENCODE_SESSION_STORE").is_some()
                    && initialize_mode != "load-missing-capability"
                {
                    native["agentCapabilities"]["loadSession"] = json!(initialize_mode != "load-no-capability");
                }
                let response = serde_json::from_value::<InitializeResponse>(native)
                    .map_err(Error::into_internal_error)?;
                if initialize_mode == "initialize_gate" {
                    journal("initialize_waiting");
                    cx.spawn(async move {
                        control_gate("initialize").await?;
                        responder.respond(response)
                    })
                } else {
                    responder.respond(response)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: NewSessionRequest, responder, _| {
                if request.cwd != std::env::current_dir().map_err(Error::into_internal_error)?
                    || !request.mcp_servers.is_empty()
                    || !request.additional_directories.is_empty()
                    || request.meta.is_some()
                {
                    return Err(Error::internal_error());
                }
                journal("new");
                let id = if let Some(path) = std::env::var_os("OPENCODE_SESSION_STORE") {
                    let id = format!("{SESSION}-{}", std::process::id());
                    std::fs::write(path, &id).map_err(Error::into_internal_error)?;
                    id
                } else {
                    SESSION.into()
                };
                responder.respond(
                    NewSessionResponse::new(id)
                        .config_options(vec![model_option("provider/before")]),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: LoadSessionRequest, responder, cx| {
                journal("load");
                if request.cwd != std::env::current_dir().map_err(Error::into_internal_error)?
                    || !request.mcp_servers.is_empty() || !request.additional_directories.is_empty()
                    || request.meta.is_some() {
                    return Err(Error::internal_error());
                }
                let id = match native_session_id() {
                    Ok(id) if id == request.session_id.0.as_ref() && load_mode != "load-native-error" => id,
                    _ => return responder.respond_with_internal_error("private fixture native session unavailable"),
                };
                journal("load_exact");
                cx.send_notification(UntypedMessage::new("session/update", json!({
                    "sessionId":id,"update":{"sessionUpdate":"user_message_chunk",
                    "content":{"type":"text","text":"private historical query"}}
                }))?)?;
                cx.send_notification(text(if load_mode == "load-foreign" {"foreign-replay"} else {&id}, "private historical answer"))?;
                cx.send_notification(UntypedMessage::new("session/update", json!({
                    "sessionId":id,"update":{"sessionUpdate":"native_usage_extension","opaque":{"historical_tokens":999}}
                }))?)?;
                journal("replay");
                let response = LoadSessionResponse::new().config_options(vec![model_option(
                    if load_mode == "load-drift" {"provider/changed"} else {"provider/literal;$(opencode)"},
                )]);
                if load_mode == "load_gate" {
                    journal("load_waiting");
                    cx.spawn(async move {
                        control_gate("load").await?;
                        responder.respond(response)
                    })
                } else {
                    responder.respond(response)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: SetSessionConfigOptionRequest, responder, _| {
                if request.session_id.0.as_ref() != native_session_id()?
                    || request.config_id.0.as_ref() != "model"
                    || serde_json::to_value(request.value).map_err(Error::into_internal_error)?
                        != json!({"value":"provider/literal;$(opencode)"})
                {
                    return Err(Error::internal_error());
                }
                *set.lock().unwrap() = true;
                journal("select");
                responder.respond(SetSessionConfigOptionResponse::new(vec![model_option(
                    "provider/literal;$(opencode)",
                )]))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, cx| {
                if !*selected.lock().unwrap() || request.session_id.0.as_ref() != native_session_id()? {
                    return Err(Error::internal_error());
                }
                let [ContentBlock::Text(content)] = request.prompt.as_slice() else {
                    return Err(Error::internal_error());
                };
                let prompt: serde_json::Value =
                    serde_json::from_str(&content.text).map_err(Error::into_internal_error)?;
                if prompt["question"] != "  literal β\n  " {
                    return Err(Error::internal_error());
                }
                if std::env::var_os("OPENCODE_SESSION_STORE").is_some() {
                    if prompt["pages"].as_array().is_none_or(|pages| pages.is_empty()) {
                        return Err(Error::internal_error());
                    }
                } else if prompt["pages"][0]["slug"] != "Paris"
                    || prompt["pages"][0]["body"] != "Paris é a capital.\n" {
                    return Err(Error::internal_error());
                }
                journal("prompt_literal");
                if prompt_mode == "shutdown" {
                    #[cfg(target_os = "linux")]
                    spawn_descendant().await?;
                    #[cfg(not(target_os = "linux"))]
                    return Err(Error::internal_error());
                }
                let id = native_session_id()?;
                if prompt_mode == "child" {
                    cx.send_notification(agent_client_protocol::UntypedMessage::new("session/update", json!({"sessionId":id,"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"child answer must stay progress"},"_meta":{"opencode/child-session":{"id":"child-session","parentID":id,"depth":1}}}})).map_err(Error::into_internal_error)?)?;
                }
                cx.send_notification(text(
                    if prompt_mode == "foreign" {
                        "foreign-child"
                    } else {
                        &id
                    },
                    "  actual β [[Paris]]\n\t",
                ))?;
                journal("chunk");
                let prompt_mode = prompt_mode.clone();
                cx.spawn(async move {
                    if matches!(prompt_mode.as_str(), "gate" | "cancel" | "shutdown") {
                        let path = std::path::PathBuf::from(
                            std::env::var_os("OPENCODE_JOURNAL")
                                .ok_or_else(Error::internal_error)?,
                        )
                        .with_extension("finish");
                        tokio::time::timeout(Duration::from_secs(15), async {
                            while !path.exists() {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        })
                        .await
                        .map_err(Error::into_internal_error)?;
                    }
                    if prompt_mode == "native_error" {
                        return responder.respond_with_internal_error("fixture native failure");
                    }
                    journal("terminal");
                    responder.respond(PromptResponse::new(if prompt_mode == "cancelled" {
                        StopReason::Cancelled
                    } else {
                        StopReason::EndTurn
                    }))
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
    if mode == "nonzero" {
        std::process::exit(9);
    }
    result
}
#[tokio::main]
async fn main() {
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some("--fixture-descendant") {
        if descendant().await.is_err() {
            std::process::exit(2);
        }
        return;
    }
    if fixture().await.is_err() {
        std::process::exit(1);
    }
}
