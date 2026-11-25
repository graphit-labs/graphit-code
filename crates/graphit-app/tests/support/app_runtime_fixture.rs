//! Cargo-built native peers for Runtime consumer journeys.
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[path = "app_runtime_fixture/kiro.rs"]
mod kiro;

const ANSWER: &str = "actual executable Role payload";
const PROMPT: &str = "Read the actual source";

struct Peer {
    directory: PathBuf,
    case: Value,
    stage: &'static str,
}
impl Peer {
    fn check(&self, condition: bool) -> io::Result<()> {
        if condition {
            Ok(())
        } else {
            Err(io::ErrorKind::InvalidData.into())
        }
    }
    fn string<'a>(&self, value: &'a Value) -> io::Result<&'a str> {
        value
            .as_str()
            .ok_or_else(|| io::ErrorKind::InvalidData.into())
    }
    fn binding(&self) -> io::Result<&str> {
        self.string(&self.case["binding"])
    }
    fn role(&self) -> io::Result<&str> {
        self.string(&self.case["role"])
    }
    fn profile(&self, name: &str) -> io::Result<String> {
        self.check(name.ends_with(&format!("-{}", self.role()?)))?;
        let relative = self.case["agent_directory"]
            .as_str()
            .unwrap_or(".opencode/agents");
        let body = fs::read_to_string(Path::new(relative).join(format!("{name}.md")))?;
        self.check(body.contains(self.role()?))?;
        for key in ["GRAPHIT_ROLE_BEARER", "GRAPHIT_ROLE_REFERENCE"] {
            self.check(!body.contains(&std::env::var(key).map_err(io::Error::other)?))?;
        }
        Ok(body)
    }
    fn prompt(&self, text: &str) -> io::Result<()> {
        if self.role()? == "tracker" {
            return self.check(text == PROMPT);
        }
        let remainder = text
            .strip_prefix("[Standing context]\nPROJECT mandatory Memory:\n")
            .ok_or(io::ErrorKind::InvalidData)?;
        let (context, body) = remainder
            .split_once("\n\n")
            .ok_or(io::ErrorKind::InvalidData)?;
        self.check(
            serde_json::from_str::<Value>(context)?
                == json!({"project_version":"main","memory_state":"no_corpus","memories":[]}),
        )?;
        self.check(body == PROMPT)
    }
    async fn authority(&mut self) -> io::Result<()> {
        self.stage = "role-authority";
        self.check(std::env::var("GRAPHIT_ROLE_BINDING").ok().as_deref() == Some(self.binding()?))?;
        let reference: Value = serde_json::from_str(
            &std::env::var("GRAPHIT_ROLE_REFERENCE").map_err(io::Error::other)?,
        )?;
        self.check(reference["role"].as_str() == Some(self.role()?))?;
        self.check(
            !std::env::var("GRAPHIT_ROLE_BEARER")
                .map_err(io::Error::other)?
                .is_empty(),
        )?;
        for wrong in [Some("not-a-binding"), Some("kiro-v3"), None] {
            self.proxy(wrong).await?;
        }
        fs::write(
            self.directory.join("proof.json"),
            serde_json::to_vec(&json!({
                "binding": self.binding()?, "model":self.case["model"], "role_mcp":true,
                "tampered_binding_denied":true, "configured_mcp":self.binding()?=="opencode"
            }))?,
        )
    }
    async fn proxy(&self, tampered: Option<&str>) -> io::Result<()> {
        let mut argv = vec!["graphit".to_owned(), "mcp".to_owned(), "--stdio".to_owned()];
        let mut environment: Vec<_> = std::env::vars_os().collect();
        if let Some(binding) = tampered {
            environment.retain(|(key, _)| key != "GRAPHIT_ROLE_BINDING");
            environment.push(("GRAPHIT_ROLE_BINDING".into(), binding.into()));
        }
        let mut cwd = std::env::current_dir()?;
        if self.binding()? == "opencode" {
            let mut server: Value =
                serde_json::from_slice(&fs::read(self.string(&self.case["mcp_config"])?)?)?;
            for key in self.case["mcp_entry"]
                .as_array()
                .ok_or(io::ErrorKind::InvalidData)?
            {
                server = server
                    .get(self.string(key)?)
                    .ok_or(io::ErrorKind::InvalidData)?
                    .clone();
            }
            argv = serde_json::from_value(server["command"].clone())?;
            self.check(
                argv == [
                    self.string(&self.case["graphit_program"])?,
                    "mcp",
                    "--stdio",
                ],
            )?;
            self.check(server["type"] == "local")?;
            if let Some(extra) = server["environment"].as_object() {
                for (key, value) in extra {
                    self.check(!key.starts_with("GRAPHIT_ROLE_"))?;
                    environment.retain(|(existing, _)| existing != key.as_str());
                    environment.push((key.into(), self.string(value)?.into()));
                }
            }
            let executable = which::which(&argv[0]).map_err(io::Error::other)?;
            self.check(
                fs::canonicalize(executable)?
                    == fs::canonicalize(self.string(&self.case["graphit_executable"])?)?,
            )?;
            if let Some(selected) = server["cwd"].as_str() {
                cwd = selected.into();
            }
            self.check(fs::canonicalize(&cwd)? == std::env::current_dir()?)?;
        }
        let mut child = tokio::process::Command::new(&argv[0])
            .args(&argv[1..])
            .env_clear()
            .envs(environment)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut input = child.stdin.take().ok_or(io::ErrorKind::BrokenPipe)?;
        let mut output = BufReader::new(child.stdout.take().ok_or(io::ErrorKind::BrokenPipe)?);
        let result = async {
            let initial = request(
                &mut input,
                &mut output,
                "initialize",
                json!({
                    "protocolVersion":"2025-03-26","capabilities":{},
                    "clientInfo":{"name":"native-role-peer","version":"1"}
                }),
                1,
            )
            .await?;
            if tampered.is_some() && initial.get("error").is_some() {
                return Ok(());
            }
            self.check(initial.get("result").is_some())?;
            write_json(
                &mut input,
                &json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
            )
            .await?;
            let value = request(
                &mut input,
                &mut output,
                "tools/call",
                json!({
                    "name":"graphit_ast_source","arguments":{"input":{"path":"raw"}}
                }),
                2,
            )
            .await?;
            let denied = value.get("error").is_some() || value["result"]["isError"] == true;
            if tampered.is_some() {
                self.check(denied && !value.to_string().contains(ANSWER))?;
            } else {
                self.check(!denied && value.to_string().contains(ANSWER))?;
                if self.role()? == "tracker" {
                    let git = request(&mut input, &mut output, "tools/call", json!({
                        "name":"graphit_git_status","arguments":{"ai_optimized":false,"input":{}}
                    }), 3).await?;
                    self.check(
                        git["result"]["isError"] != true
                            && git["result"]["structuredContent"]["object_format"] == "sha1",
                    )?;
                }
                if self.role()? == "scout" {
                    let shape = request(
                        &mut input,
                        &mut output,
                        "tools/call",
                        json!({
                            "name":"graphit_hub_type_path","arguments":{"ai_optimized":false,
                            "input":{"artifact_type":"agent","name":"observer"}}
                        }),
                        4,
                    )
                    .await?;
                    self.check(shape["result"]["isError"] != true)?;
                    let metadata = &shape["result"]["structuredContent"];
                    self.check(
                        metadata["binding"].as_str() == Some(self.binding()?)
                            && metadata["target"]
                                == json!({"kind":"native_binding","binding":self.binding()?}),
                    )?;
                }
            }
            Ok::<(), io::Error>(())
        }
        .await;
        drop(input);
        let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
        if tampered.is_some()
            && !status.success()
            && result.as_ref().is_err_and(|error| {
                matches!(
                    error.kind(),
                    io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
                )
            })
        {
            return Ok(());
        }
        result?;
        self.check(tampered.is_some() || status.success())
    }
    async fn acp(&mut self, args: &[String]) -> io::Result<()> {
        self.stage = "opencode-argv";
        self.check(args == ["acp"])?;
        self.stage = "opencode-profile-path";
        let directory = Path::new(".opencode/agents");
        let mut profiles = fs::read_dir(directory)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.ends_with(&format!("-{}.md", self.case["role"].as_str().unwrap_or("")))
                    })
            });
        let path = profiles.next().ok_or(io::ErrorKind::NotFound)?;
        self.check(profiles.next().is_none())?;
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or(io::ErrorKind::InvalidData)?;
        let body = self.profile(name)?;
        self.stage = "opencode-profile-mode";
        let frontmatter = body
            .split("---\n")
            .nth(1)
            .ok_or(io::ErrorKind::InvalidData)?;
        let document: Value = serde_saphyr::from_str(frontmatter).map_err(io::Error::other)?;
        self.check(document["mode"] == "all")?;
        self.stage = "opencode-profile-permissions";
        let server = self.string(&self.case["mcp_server"])?;
        let namespace: String = server
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.check(
            frontmatter.contains("permissions:")
                && frontmatter.contains("execute")
                && frontmatter.contains(&format!("{namespace}_*")),
        )?;
        let mut mode = "build".to_owned();
        let mut model = "provider/default".to_owned();
        let mut selected_mode = false;
        let mut selected_model = false;
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mut output = tokio::io::stdout();
        while let Some(line) = lines.next_line().await? {
            let message: Value = serde_json::from_str(&line)?;
            let params = &message["params"];
            let result = match message["method"].as_str() {
                Some("initialize") => {
                    self.stage = "initialize";
                    json!({"protocolVersion":1,"agentInfo":{"name":"OpenCode","version":"2.0.0"},
                        "agentCapabilities":{"mcpCapabilities":{"http":true,"sse":false}}})
                }
                Some("session/new") => {
                    self.stage = "session/new";
                    self.check(
                        Path::new(self.string(&params["cwd"])?) == std::env::current_dir()?,
                    )?;
                    json!({"sessionId":"native-role","configOptions":options(name,self.case["model"].clone(),&mode,&model)})
                }
                Some("session/set_config_option") => {
                    self.stage = "session/set_config_option";
                    match params["configId"].as_str() {
                        Some("mode") => {
                            self.check(params["value"] == name)?;
                            mode = name.into();
                            selected_mode = true;
                        }
                        Some("model") => {
                            self.check(params["value"] == self.case["model"])?;
                            model = self.string(&params["value"])?.into();
                            selected_model = true;
                        }
                        _ => return Err(io::ErrorKind::InvalidData.into()),
                    }
                    json!({"configOptions":options(name,self.case["model"].clone(),&mode,&model)})
                }
                Some("session/prompt") => {
                    self.stage = "session/prompt";
                    self.check(selected_mode && selected_model)?;
                    let prompt: String = params["prompt"]
                        .as_array()
                        .ok_or(io::ErrorKind::InvalidData)?
                        .iter()
                        .filter(|item| item["type"] == "text")
                        .filter_map(|item| item["text"].as_str())
                        .collect();
                    self.prompt(&prompt)?;
                    self.authority().await?;
                    write_json(&mut output,&json!({"jsonrpc":"2.0","method":"session/update",
                        "params":{"sessionId":"native-role","update":{"sessionUpdate":"agent_message_chunk",
                        "content":{"type":"text","text":ANSWER}}}})).await?;
                    json!({"stopReason":"end_turn"})
                }
                _ => continue,
            };
            write_json(
                &mut output,
                &json!({"jsonrpc":"2.0","id":message["id"],"result":result}),
            )
            .await?;
        }
        Ok(())
    }
    async fn cli(&mut self, args: &[String]) -> io::Result<()> {
        self.stage = "native-arguments";
        let model = self.case["model"].as_str();
        let expected_model: Vec<&str> = model
            .map(|value| vec!["--model", value])
            .unwrap_or_default();
        let mut input = String::new();
        let text = match self.binding()? {
            "codex" => {
                self.check(
                    args.starts_with(&[
                        "exec".into(),
                        "--json".into(),
                        "--skip-git-repo-check".into(),
                    ]) && args.last().map(String::as_str) == Some("-"),
                )?;
                let mut rest = &args[3..args.len() - 1];
                let mut settings = std::collections::HashMap::new();
                while rest.first().map(String::as_str) == Some("-c") {
                    let (key, value) = rest
                        .get(1)
                        .and_then(|value| value.split_once('='))
                        .ok_or(io::ErrorKind::InvalidData)?;
                    settings.insert(key, serde_json::from_str::<Value>(value)?);
                    rest = &rest[2..];
                }
                self.check(
                    rest == expected_model
                        && settings
                            .get("developer_instructions")
                            .and_then(Value::as_str)
                            .is_some_and(|body| {
                                body.contains(self.case["role"].as_str().unwrap_or(""))
                            }),
                )?;
                self.check(
                    settings.get("features.shell_tool") == Some(&json!(false))
                        && !settings.contains_key("sandbox_mode"),
                )?;
                tokio::io::AsyncReadExt::read_to_string(&mut tokio::io::stdin(), &mut input)
                    .await?;
                input
            }
            "claude" | "antigravity" => {
                let prefix: &[&str] = if self.binding()? == "claude" {
                    &[
                        "--print",
                        "--output-format",
                        "stream-json",
                        "--verbose",
                        "--include-partial-messages",
                        "--agent",
                    ]
                } else {
                    &[
                        "--print",
                        "--input-format",
                        "stream-json",
                        "--output-format",
                        "stream-json",
                        "--agent",
                    ]
                };
                self.check(
                    args.starts_with(
                        &prefix
                            .iter()
                            .map(|value| value.to_string())
                            .collect::<Vec<_>>(),
                    ),
                )?;
                self.profile(args.get(prefix.len()).ok_or(io::ErrorKind::InvalidData)?)?;
                self.check(args[prefix.len() + 1..] == expected_model)?;
                tokio::io::AsyncReadExt::read_to_string(&mut tokio::io::stdin(), &mut input)
                    .await?;
                if self.binding()? == "antigravity" {
                    let record: Value = serde_json::from_str(&input)?;
                    self.check(record["event"] == "user")?;
                    self.string(&record["message"]["content"])?.into()
                } else {
                    input
                }
            }
            "cursor" => {
                let mut expected = vec!["--print", "--output-format", "stream-json"];
                expected.extend(expected_model);
                self.check(args[..args.len() - 1] == expected)?;
                let text = args.last().ok_or(io::ErrorKind::InvalidData)?;
                let body = text
                    .strip_suffix(PROMPT)
                    .ok_or(io::ErrorKind::InvalidData)?;
                self.check(body.contains(self.role()?))?;
                PROMPT.into()
            }
            "qwen" => {
                let prefix = [
                    "--input-format",
                    "text",
                    "--output-format",
                    "stream-json",
                    "--include-partial-messages",
                    "--system-prompt",
                ];
                self.check(args.starts_with(&prefix.map(str::to_owned)))?;
                self.check(
                    args.get(prefix.len()).is_some_and(|body| {
                        body.contains(self.case["role"].as_str().unwrap_or(""))
                    }) && args[prefix.len() + 1..] == expected_model,
                )?;
                tokio::io::AsyncReadExt::read_to_string(&mut tokio::io::stdin(), &mut input)
                    .await?;
                input
            }
            "kimi" => {
                self.check(args.first().map(String::as_str) == Some("--agent-file"))?;
                let path = Path::new(args.get(1).ok_or(io::ErrorKind::InvalidData)?);
                self.check(
                    path.is_absolute()
                        && path.parent()
                            == Some(std::env::current_dir()?.join(".kimi-code/agents").as_path())
                        && path.extension().is_some_and(|ext| ext == "md"),
                )?;
                let body = self.profile(
                    path.file_stem()
                        .and_then(|name| name.to_str())
                        .ok_or(io::ErrorKind::InvalidData)?,
                )?;
                self.check(
                    body.starts_with("---\n")
                        && ["Read", "Grep", "Glob", "mcp__"]
                            .iter()
                            .all(|token| body.contains(token)),
                )?;
                let mut expected = vec!["--output-format", "stream-json"];
                if let Some(value) = model {
                    expected.extend(["-m", value]);
                }
                expected.push("-p");
                self.check(args[2..args.len() - 1] == expected)?;
                tokio::io::AsyncReadExt::read_to_string(&mut tokio::io::stdin(), &mut input)
                    .await?;
                self.check(input.is_empty())?;
                fs::write(
                    self.directory.join("native.pid"),
                    std::process::id().to_string(),
                )?;
                args.last().ok_or(io::ErrorKind::InvalidData)?.clone()
            }
            _ => return Err(io::ErrorKind::InvalidData.into()),
        };
        self.stage = "prompt";
        self.prompt(&text)?;
        self.authority().await?;
        if self.case["eof"] == true {
            return Ok(());
        }
        if self.case["hold"] == true {
            fs::write(
                self.directory.join("holding.pid"),
                std::process::id().to_string(),
            )?;
            return std::future::pending().await;
        }
        let observed = self.case["model"].as_str().unwrap_or("observed");
        let mut output = tokio::io::stdout();
        match self.binding()? {
            "codex" => {
                for value in [
                    json!({"type":"turn.started"}),
                    json!({"type":"item.completed","item":{"id":"native-role","type":"agent_message","text":ANSWER}}),
                    json!({"type":"turn.completed","usage":{"input_tokens":8,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}),
                ] {
                    write_json(&mut output, &value).await?;
                }
            }
            "antigravity" => {
                write_json(&mut output,&json!({"event":"init","conversation_id":"native-role","init":{"model":observed}})).await?;
                write_json(&mut output,&json!({"event":"result","result":{"conversation_id":"native-role","status":"SUCCESS","response":ANSWER}})).await?;
            }
            "kimi" => {
                for value in [
                    json!({"role":"tool","tool_call_id":"native-source","content":"excluded tool answer"}),
                    json!({"role":"assistant","content":"actual executable "}),
                    json!({"role":"assistant","content":"Role payload"}),
                ] {
                    write_json(&mut output, &value).await?;
                }
            }
            binding => {
                write_json(&mut output,&json!({"type":"system","subtype":"init","uuid":"native-role","session_id":"native-role","model":observed})).await?;
                let mut result = json!({"type":"result","subtype":"success","is_error":false,"duration_ms":8,"duration_api_ms":5,"num_turns":1,"session_id":"native-role","result":ANSWER});
                if binding == "qwen" || binding == "claude" {
                    result["usage"] = json!({"input_tokens":5,"output_tokens":3,"cache_read_input_tokens":2,"total_tokens":10});
                    result["modelUsage"] = json!({observed:{"inputTokens":5,"outputTokens":3,"cacheReadInputTokens":2,"cacheCreationInputTokens":0,"webSearchRequests":0,"contextWindow":128}});
                }
                if binding == "qwen" {
                    result["uuid"] = json!("end");
                    result["permission_denials"] = json!([]);
                }
                if binding == "claude" {
                    result["total_cost_usd"] = json!(0.0001);
                }
                if binding == "cursor" {
                    result["request_id"] = json!("native-request");
                }
                write_json(&mut output, &result).await?;
            }
        }
        Ok(())
    }
}
fn options(name: &str, offered_model: Value, mode: &str, model: &str) -> Value {
    json!([
        {"id":"mode","name":"Mode","category":"mode","type":"select","currentValue":mode,"options":[{"value":name,"name":name}]},
        {"id":"model","name":"Model","category":"model","type":"select","currentValue":model,"options":[{"value":offered_model,"name":offered_model}]}
    ])
}
async fn write_json(writer: &mut (impl AsyncWriteExt + Unpin), value: &Value) -> io::Result<()> {
    writer.write_all(&serde_json::to_vec(value)?).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}
async fn request(
    input: &mut tokio::process::ChildStdin,
    output: &mut BufReader<tokio::process::ChildStdout>,
    method: &str,
    params: Value,
    id: u64,
) -> io::Result<Value> {
    write_json(
        input,
        &json!({"jsonrpc":"2.0","method":method,"params":params,"id":id}),
    )
    .await?;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let mut line = String::new();
            if output.read_line(&mut line).await? == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let value: Value = serde_json::from_str(&line)?;
            if value["id"] == id {
                return Ok(value);
            }
        }
    })
    .await
    .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
}
fn configuration(mut executable: PathBuf) -> io::Result<(PathBuf, Value)> {
    let mut aliases = std::collections::HashSet::new();
    loop {
        let directory = executable
            .parent()
            .ok_or(io::ErrorKind::InvalidInput)?
            .canonicalize()?;
        executable = directory.join(executable.file_name().ok_or(io::ErrorKind::InvalidInput)?);
        if !aliases.insert(executable.clone()) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let bytes = fs::read(directory.join("case.json")).or_else(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                fs::read(directory.join("peer.json"))
            } else {
                Err(error)
            }
        });
        match bytes {
            Ok(bytes) => {
                return Ok((directory, serde_json::from_slice(&bytes)?));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let target = fs::read_link(&executable)?;
                executable = directory.join(target);
            }
            Err(error) => return Err(error),
        }
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args();
    let executable = which::which(args.next().expect("fixture executable"))
        .expect("fixture executable in its private PATH");
    let (directory, case) = configuration(executable).expect("private fixture configuration");
    let mut peer = Peer {
        directory,
        case,
        stage: "startup",
    };
    fs::write(
        peer.directory.join("phase.json"),
        br#"{"stage":"started","class":null}"#,
    )
    .expect("private fixture startup marker");
    if peer.case["fixture"].is_string() {
        let reference = std::env::var("GRAPHIT_ROLE_REFERENCE")
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        peer.case["role"] = reference
            .map(|value| value["role"].clone())
            .unwrap_or(Value::Null);
    }
    let args: Vec<_> = args.collect();
    if peer.case["fixture"] == "service-manager" {
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
        assert!(address.ends_with("/absent-bus"));
        assert_eq!(args.first().map(String::as_str), Some("--user"));
        match args.get(1).map(String::as_str) {
            Some("show") => {
                assert_eq!(args.len(), 4);
                assert_eq!(
                    args[2],
                    "--property=LoadState,ActiveState,UnitFileState,MainPID"
                );
            }
            Some("start") => assert_eq!(args.len(), 3),
            _ => panic!("unexpected service-manager invocation"),
        }
        assert!(args.last().is_some_and(|arg| arg.ends_with(".service")));
        eprintln!("private service bus unavailable");
        std::process::exit(69);
    }
    let result = if peer.case["fixture"].is_string() {
        peer.kiro(&args).await
    } else if peer.binding().ok() == Some("opencode") {
        peer.acp(&args).await
    } else {
        peer.cli(&args).await
    };
    if let Err(error) = result {
        let _ = fs::write(
            peer.case["failure_file"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or_else(|| peer.directory.join("phase.json")),
            serde_json::to_vec(&json!({"stage":peer.stage,"class":format!("{:?}",error.kind())}))
                .unwrap(),
        );
        std::process::exit(1);
    }
}
