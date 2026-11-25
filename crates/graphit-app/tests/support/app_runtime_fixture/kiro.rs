use super::*;
use base64::Engine;

struct Mcp {
    child: tokio::process::Child,
    input: Option<tokio::process::ChildStdin>,
    output: BufReader<tokio::process::ChildStdout>,
}
impl Mcp {
    async fn start(
        program: &str,
        args: &[String],
        environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    ) -> io::Result<Self> {
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .env_clear()
            .envs(environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let input = child.stdin.take().ok_or(io::ErrorKind::BrokenPipe)?;
        let output = BufReader::new(child.stdout.take().ok_or(io::ErrorKind::BrokenPipe)?);
        let mut mcp = Self {
            child,
            input: Some(input),
            output,
        };
        let initial=request(mcp.input.as_mut().unwrap(),&mut mcp.output,"initialize",json!({
            "protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"runtime-kiro-peer","version":"1"}
        }),1).await?;
        if initial.get("result").is_none() {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        write_json(
            mcp.input.as_mut().unwrap(),
            &json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
        )
        .await?;
        Ok(mcp)
    }
    async fn call(&mut self, name: &str, arguments: Value, id: u64) -> io::Result<Value> {
        request(
            self.input.as_mut().unwrap(),
            &mut self.output,
            "tools/call",
            json!({"name":name,"arguments":arguments}),
            id,
        )
        .await
    }
    async fn finish(mut self) -> io::Result<()> {
        drop(self.input.take());
        let output = tokio::time::timeout(Duration::from_secs(15), self.child.wait_with_output())
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
        for key in ["GRAPHIT_ROLE_BEARER", "GRAPHIT_ROLE_REFERENCE"] {
            if let Ok(value) = std::env::var(key)
                && String::from_utf8_lossy(&output.stderr).contains(&value)
            {
                return Err(io::ErrorKind::InvalidData.into());
            }
        }
        if String::from_utf8_lossy(&output.stderr).contains("MANDATORY-3180") {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if output.status.success() {
            Ok(())
        } else {
            Err(io::ErrorKind::Other.into())
        }
    }
}
impl Peer {
    fn mark(&mut self, stage: &'static str) -> io::Result<()> {
        self.stage = stage;
        fs::write(
            self.directory.join("phase.json"),
            serde_json::to_vec(&json!({"stage":stage,"class":null}))?,
        )?;
        if let Some(path) = self.case["journal"].as_str() {
            fs::write(
                path,
                serde_json::to_vec(&json!({"stage":stage,"role":self.role()?}))?,
            )?;
        }
        Ok(())
    }
    pub(super) async fn kiro(&mut self, args: &[String]) -> io::Result<()> {
        self.check(args == ["acp", "--agent-engine=v3"])?;
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mut output = tokio::io::stdout();
        while let Some(line) = lines.next_line().await? {
            let message: Value = serde_json::from_str(&line)?;
            let params = &message["params"];
            let result = match message["method"].as_str() {
                Some("initialize") => {
                    self.mark("initialize")?;
                    json!({"protocolVersion":1,"agentCapabilities":{}})
                }
                Some("session/new") => {
                    self.mark("session/new")?;
                    self.check(
                        Path::new(self.string(&params["cwd"])?) == std::env::current_dir()?,
                    )?;
                    self.check(
                        params["_meta"]["kiro"]["modeId"]
                            == format!("{}-{}", self.string(&self.case["brand"])?, self.role()?),
                    )?;
                    json!({"sessionId":"native-role"})
                }
                Some("session/prompt") => {
                    self.mark("session/prompt")?;
                    let reference: Value = serde_json::from_str(
                        &std::env::var("GRAPHIT_ROLE_REFERENCE").map_err(io::Error::other)?,
                    )?;
                    self.check(reference["role"].as_str() == Some(self.role()?))?;
                    for key in ["GRAPHIT_ROLE_BEARER", "GRAPHIT_ROLE_REFERENCE"] {
                        self.check(!std::env::var(key).map_err(io::Error::other)?.is_empty())?;
                    }
                    let answer = match self.case["fixture"].as_str() {
                        Some("kiro-run") => {
                            self.kiro_run(params).await?;
                            ANSWER.to_owned()
                        }
                        Some("coordinator") => {
                            self.coordinator().await?;
                            "Role hook boundary verified".to_owned()
                        }
                        Some("hooks") => {
                            self.hooks().await?;
                            "Role hook boundary verified".to_owned()
                        }
                        Some("role-hooks") => {
                            self.role_hooks().await?;
                            "Role hooks verified".to_owned()
                        }
                        Some("role-http") => self.role_http().await?,
                        Some("role-prompt") => {
                            self.role_prompt(params)?;
                            "context verified".to_owned()
                        }
                        _ => return Err(io::ErrorKind::InvalidData.into()),
                    };
                    write_json(&mut output,&json!({"jsonrpc":"2.0","method":"session/update","params":{
                        "sessionId":"native-role","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":answer}}
                    }})).await?;
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
    async fn role_http(&mut self) -> io::Result<String> {
        use rmcp::{
            ServiceExt,
            model::CallToolRequestParams,
            transport::{
                StreamableHttpClientTransport,
                streamable_http_client::StreamableHttpClientTransportConfig,
            },
        };
        if let Some(phase) = self.case.get("phase").and_then(Value::as_str) {
            let address: std::net::SocketAddr = phase.parse().map_err(io::Error::other)?;
            self.mark("role-http-phase")?;
            self.check(address.ip().is_loopback())?;
            tokio::time::timeout(Duration::from_secs(10), async {
                use tokio::io::AsyncReadExt;
                let mut peer = tokio::net::TcpStream::connect(address).await?;
                peer.write_all(b"R").await?;
                let mut release = [0];
                peer.read_exact(&mut release).await?;
                self.check(release == *b"C")
            })
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??;
        } else {
            self.mark("role-http-initialize")?;
            let endpoint = reqwest::Url::parse(self.string(&self.case["endpoint"])?)
                .map_err(io::Error::other)?;
            let host = endpoint.host_str().ok_or(io::ErrorKind::InvalidInput)?;
            let address: std::net::IpAddr = host
                .trim_matches(['[', ']'])
                .parse()
                .map_err(io::Error::other)?;
            self.check(
                endpoint.scheme() == "http"
                    && address.is_loopback()
                    && endpoint.path() == "/roles/mcp",
            )?;
            let mut headers = std::collections::HashMap::new();
            for (header, name) in [
                ("authorization", "GRAPHIT_ROLE_BEARER"),
                ("x-graphit-role-reference", "GRAPHIT_ROLE_REFERENCE"),
                ("x-graphit-role-selection", "GRAPHIT_ROLE_SELECTION"),
            ] {
                let value = std::env::var(name).map_err(io::Error::other)?;
                self.check(!value.is_empty())?;
                let value = if header == "authorization" {
                    format!("Bearer {value}")
                } else {
                    value
                };
                let mut value =
                    reqwest::header::HeaderValue::from_str(&value).map_err(io::Error::other)?;
                value.set_sensitive(true);
                headers.insert(reqwest::header::HeaderName::from_static(header), value);
            }
            headers.insert(
                reqwest::header::HeaderName::from_static("x-graphit-auth-origin"),
                reqwest::header::HeaderValue::from_static("role"),
            );
            let http = reqwest::Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .map_err(io::Error::other)?;
            let transport = StreamableHttpClientTransport::with_client(
                http,
                StreamableHttpClientTransportConfig::with_uri(endpoint.as_str().to_owned())
                    .custom_headers(headers),
            );
            let client = tokio::time::timeout(Duration::from_secs(10), ().serve(transport))
                .await
                .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
                .map_err(io::Error::other)?;
            self.mark("role-http-source")?;
            let result = tokio::time::timeout(
                Duration::from_secs(10),
                client.peer().call_tool(
                    CallToolRequestParams::new("graphit_ast_source").with_arguments(
                        json!({"ai_optimized":false,"input":{"path":"raw"}})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                ),
            )
            .await;
            self.mark("role-http-close")?;
            let closed = tokio::time::timeout(Duration::from_secs(10), client.cancel()).await;
            let result = result
                .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
                .map_err(io::Error::other)?;
            closed
                .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
                .map_err(io::Error::other)?;
            self.check(
                result.is_error != Some(true)
                    && serde_json::to_string(&result)?.contains("actual native payload"),
            )?;
        }
        Ok(format!(
            "actual Role source {}",
            std::env::var("GRAPHIT_ROLE_BEARER").map_err(io::Error::other)?
        ))
    }
    fn role_prompt(&self, params: &Value) -> io::Result<()> {
        let prompt: String = params["prompt"]
            .as_array()
            .ok_or(io::ErrorKind::InvalidData)?
            .iter()
            .filter(|item| item["type"] == "text")
            .filter_map(|item| item["text"].as_str())
            .collect();
        let expected = self
            .case
            .get("expected_context")
            .ok_or(io::ErrorKind::InvalidInput)?;
        if expected.is_null() {
            return self.check(prompt == "Read the actual native source");
        }
        let text = prompt
            .strip_prefix("[Standing context]\nPROJECT mandatory Memory:\n")
            .ok_or(io::ErrorKind::InvalidData)?;
        let (context, suffix) = text.split_once("\n\n").ok_or(io::ErrorKind::InvalidData)?;
        self.check(
            serde_json::from_str::<Value>(context)? == *expected
                && suffix == "Read the actual native source",
        )
    }
    async fn role_mcp(&self, explicit: bool) -> io::Result<Mcp> {
        let mut args = vec!["mcp".into(), "--stdio".into()];
        if explicit {
            args.push("--role".into());
        }
        Mcp::start(
            self.case["graphit"].as_str().unwrap_or("graphit"),
            &args,
            std::env::vars_os().collect(),
        )
        .await
    }
    async fn kiro_run(&mut self, params: &Value) -> io::Result<()> {
        let prompt: String = params["prompt"]
            .as_array()
            .ok_or(io::ErrorKind::InvalidData)?
            .iter()
            .filter(|item| item["type"] == "text")
            .filter_map(|item| item["text"].as_str())
            .collect();
        self.prompt(&prompt)?;
        self.mark("stdio-initialize")?;
        let mut mcp = self.role_mcp(true).await?;
        self.mark("stdio-source")?;
        let source = mcp
            .call("graphit_ast_source", json!({"input":{"path":"raw"}}), 2)
            .await?;
        self.check(
            source.get("result").is_some()
                && source["result"]["isError"] != true
                && source.to_string().contains(ANSWER),
        )?;
        if self.role()? == "tracker" {
            self.mark("stdio-git-status")?;
            let status = mcp
                .call(
                    "graphit_git_status",
                    json!({"ai_optimized":false,"input":{}}),
                    3,
                )
                .await?;
            self.check(
                status["result"]["isError"] != true
                    && status["result"]["structuredContent"]["object_format"] == "sha1",
            )?;
            self.mark("stdio-git-diff")?;
            let diff = mcp
                .call(
                    "graphit_git_diff",
                    json!({"ai_optimized":false,"input":{}}),
                    4,
                )
                .await?;
            self.check(diff["result"]["isError"] != true)?;
            let patches = diff["result"]["structuredContent"]["patches"]
                .as_array()
                .ok_or(io::ErrorKind::InvalidData)?;
            let tracked = patches
                .iter()
                .find(|patch| patch["path_base64"] == "dHJhY2tlZA==")
                .ok_or(io::ErrorKind::InvalidData)?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(self.string(&tracked["hunks_base64"])?)
                .map_err(io::Error::other)?;
            self.check(
                bytes
                    .windows(b"+bravo".len())
                    .any(|window| window == b"+bravo"),
            )?;
        }
        self.mark("stdio-close")?;
        mcp.finish().await
    }
    async fn insert(
        &self,
        mcp: &mut Mcp,
        id: &str,
        body: &str,
        mandatory: bool,
        number: u64,
    ) -> io::Result<()> {
        let value = mcp
            .call(
                &format!("{}_memory_insert", self.string(&self.case["brand"])?),
                json!({"input":{
                    "idempotency_key":id,"title":"PROJECT mandatory hook qualification","body":body,
                    "classification":"convention","mandatory":mandatory,"important":false,"tags":[]
                }}),
                number,
            )
            .await?;
        self.check(value.get("result").is_some() && value["result"]["isError"] != true)
    }
    async fn coordinator(&mut self) -> io::Result<()> {
        self.check(std::env::var_os("GRAPHIT_ROLE_SELECTION").is_some())?;
        if self.role()? != "scribe" {
            return Ok(());
        }
        let brand = self.string(&self.case["brand"])?;
        let document: Value = serde_json::from_slice(&fs::read(".kiro/settings/mcp.json")?)?;
        let server = &document["mcpServers"][brand];
        self.check(server["command"] == brand)?;
        let args: Vec<String> = serde_json::from_value(server["args"].clone())?;
        self.check(
            !args.iter().any(|arg| {
                ["--project-dir", "--project-version", "--role"].contains(&arg.as_str())
            }),
        )?;
        let allowed: Vec<String> = serde_json::from_value(self.case["mcp_environment"].clone())?;
        let environment = std::env::vars_os()
            .filter(|(key, _)| {
                key == "PATH"
                    || key == "HOME"
                    || key
                        .to_str()
                        .is_some_and(|key| allowed.iter().any(|allowed| allowed == key))
            })
            .collect();
        let mut mcp = Mcp::start(brand, &args, environment).await?;
        self.insert(
            &mut mcp,
            "coordinator-native-mandatory-be6e",
            self.string(&self.case["mandatory"])?,
            true,
            2,
        )
        .await?;
        mcp.finish().await
    }
    async fn hooks(&mut self) -> io::Result<()> {
        self.mark("hooks-role-selection")?;
        self.check(std::env::var_os("GRAPHIT_ROLE_SELECTION").is_some())?;
        self.mark("hooks-mcp-open")?;
        let mut mcp = self.role_mcp(self.role()? == "scribe").await?;
        if self.role()? == "scribe" {
            self.mark("hooks-memory-insert")?;
            self.insert(
                &mut mcp,
                "hook-native-mandatory-3180",
                "MANDATORY-3180 literal {{ untrusted }}",
                true,
                2,
            )
            .await?;
        } else {
            self.mark("hooks-tracker-tools")?;
            let brand = self.string(&self.case["brand"])?;
            let answer = mcp
                .call(
                    &format!("{brand}_module_skill"),
                    json!({"input":{"module":"task"}}),
                    2,
                )
                .await?;
            self.check(
                answer.get("result").is_some()
                    && answer["result"]["isError"] != true
                    && answer["result"]
                        .to_string()
                        .contains(&format!("{brand}_task")),
            )?;
            let denied = mcp
                .call(&format!("{brand}_memory_list"), json!({"input":{}}), 3)
                .await?;
            self.check(denied.get("result").is_some() && denied["result"]["isError"] == true)?;
            let denied=mcp.call(&format!("{brand}_memory_insert"),json!({"input":{
                "idempotency_key":"hook-native-tracker-denied","title":"Tracker must not write Memory",
                "body":"must remain denied","classification":"convention","mandatory":false,"important":false,"tags":[]
            }}),4).await?;
            self.check(denied.get("result").is_some() && denied["result"]["isError"] == true)?;
        }
        self.mark("hooks-mcp-close")?;
        mcp.finish().await?;
        if self.role()? == "tracker" {
            let hooks: Vec<String> =
                serde_json::from_slice(&fs::read(self.string(&self.case["hooks_file"])?)?)?;
            for hook in hooks {
                let result = command_output("/bin/sh", &["-c".into(), hook], None).await?;
                self.check(!result.status.success() && result.stdout.is_empty())?;
                self.check(String::from_utf8_lossy(&result.stderr).contains(
                    "Root session hook is unavailable in a managed-role transport context",
                ))?;
                self.private_output(&result)?;
            }
        }
        Ok(())
    }
    fn private_output(&self, output: &std::process::Output) -> io::Result<()> {
        for key in ["GRAPHIT_ROLE_BEARER", "GRAPHIT_ROLE_REFERENCE"] {
            if let Ok(value) = std::env::var(key) {
                self.check(
                    !String::from_utf8_lossy(&output.stdout).contains(&value)
                        && !String::from_utf8_lossy(&output.stderr).contains(&value),
                )?;
            }
        }
        self.check(!String::from_utf8_lossy(&output.stderr).contains("MANDATORY-3180"))
    }
    async fn role_hooks(&mut self) -> io::Result<()> {
        self.check(std::env::var_os("GRAPHIT_ROLE_SELECTION").is_some())?;
        let graphit = self.string(&self.case["graphit"])?.to_owned();
        if self.role()? == "scribe" {
            for case in [
                "invalid-bearer",
                "bearer-only",
                "reference-only",
                "explicit-missing",
                "version-pin",
                "explicit-version-pin",
            ] {
                let mut environment: Vec<_> = std::env::vars_os()
                    .filter(|(key, _)| !key.to_string_lossy().starts_with("GRAPHIT_ROLE_"))
                    .collect();
                let mut args = vec!["mcp".into(), "--stdio".into()];
                let role_env = |name: &str| {
                    std::env::var_os(name)
                        .map(|value| (name.into(), value))
                        .ok_or(io::ErrorKind::InvalidData)
                };
                match case {
                    "invalid-bearer" => {
                        environment.push((
                            "GRAPHIT_ROLE_BEARER".into(),
                            "invalid-role-credential".into(),
                        ));
                        environment.push(role_env("GRAPHIT_ROLE_REFERENCE")?);
                    }
                    "bearer-only" => environment.push(role_env("GRAPHIT_ROLE_BEARER")?),
                    "reference-only" => environment.push(role_env("GRAPHIT_ROLE_REFERENCE")?),
                    "version-pin" | "explicit-version-pin" => {
                        environment.extend(
                            std::env::vars_os().filter(|(key, _)| {
                                key.to_string_lossy().starts_with("GRAPHIT_ROLE_")
                            }),
                        );
                        args = vec![
                            "--project-version".into(),
                            "foreign-role-version".into(),
                            "mcp".into(),
                            "--stdio".into(),
                        ];
                        if case == "explicit-version-pin" {
                            args.push("--role".into());
                        }
                    }
                    _ => args.push("--role".into()),
                }
                let result = command_output(&graphit, &args, Some(environment)).await?;
                self.check(!result.status.success() && result.stdout.is_empty())?;
                let stderr = String::from_utf8_lossy(&result.stderr);
                self.check(!stderr.contains("Root Source startup is unavailable"))?;
                if case.ends_with("version-pin") {
                    self.check(stderr.contains(
                        "managed-role MCP cannot override its captured transport or project",
                    ))?;
                }
                self.private_output(&result)?;
                self.check(!stderr.contains("invalid-role-credential"))?;
            }
            self.mark("mcp-initialize")?;
            let mut mcp = self.role_mcp(false).await?;
            self.mark("mcp-write")?;
            for (number, mandatory) in [(1, true), (2, true), (3, false)] {
                self.insert(
                    &mut mcp,
                    &format!("role-hooks-{number}"),
                    &format!("MANDATORY-b9fb literal {{{{ data }}}} {number}"),
                    mandatory,
                    number + 1,
                )
                .await?;
            }
            self.mark("mcp-close")?;
            mcp.finish().await?;
            self.mark("mcp-joined")?;
        }
        let brand = self.string(&self.case["brand"])?;
        let profile = Path::new(".kiro/agents").join(format!("{brand}-{}.md", self.role()?));
        let body = fs::read_to_string(&profile)?;
        self.check(
            !body.contains("Standing context")
                && !body.contains(&graphit)
                && !body.contains(
                    self.directory
                        .parent()
                        .and_then(|path| path.to_str())
                        .ok_or(io::ErrorKind::InvalidData)?,
                ),
        )?;
        let inspected = command_output(
            self.string(&self.case["inspector"])?,
            &[
                "--profile-hook-commands".into(),
                profile.to_string_lossy().into_owned(),
            ],
            None,
        )
        .await?;
        self.check(inspected.status.success())?;
        let commands: Vec<Value> = serde_json::from_slice(&inspected.stdout)?;
        self.check(
            commands
                .iter()
                .map(|entry| entry["event"].as_str())
                .collect::<Vec<_>>()
                == [Some("SessionStart"), Some("UserPromptSubmit")],
        )?;
        for entry in commands {
            let hook = self.string(&entry["command"])?;
            let argv = shlex::split(hook).ok_or(io::ErrorKind::InvalidData)?;
            self.check(argv.first().map(String::as_str) == self.case["program"].as_str())?;
            let recipient = argv
                .iter()
                .position(|arg| arg == "--recipient")
                .ok_or(io::ErrorKind::InvalidData)?;
            self.check(
                argv.get(recipient + 1).map(String::as_str) == Some("role")
                    && !argv.iter().any(|arg| arg == "--global-dir"),
            )?;
            let selected = argv
                .iter()
                .position(|arg| arg == "--project-dir")
                .ok_or(io::ErrorKind::InvalidData)?;
            self.check(argv.get(selected + 1).map(String::as_str) == Some("."))?;
            self.check(
                fs::canonicalize(which::which(&argv[0]).map_err(io::Error::other)?)?
                    == fs::canonicalize(&graphit)?,
            )?;
            let result = command_output("/bin/sh", &["-c".into(), hook.into()], None).await?;
            self.check(result.status.success() && result.stderr.is_empty())?;
            if self.role()? == "tracker" {
                self.check(result.stdout.is_empty())?;
            } else {
                let text = std::str::from_utf8(&result.stdout).map_err(io::Error::other)?;
                let context: Value = serde_json::from_str(
                    text.strip_prefix("[Standing context]\nPROJECT mandatory Memory:\n")
                        .ok_or(io::ErrorKind::InvalidData)?,
                )?;
                let memories = context["memories"]
                    .as_array()
                    .ok_or(io::ErrorKind::InvalidData)?;
                self.check(
                    context["memory_state"] == "published"
                        && memories.len() == 2
                        && memories.iter().all(|row| row["mandatory"] == true)
                        && context.get("task").is_none()
                        && context.get("user").is_none(),
                )?;
                let bodies: std::collections::BTreeSet<_> = memories
                    .iter()
                    .filter_map(|row| row["body"].as_str())
                    .collect();
                self.check(
                    bodies
                        == [
                            "MANDATORY-b9fb literal {{ data }} 1",
                            "MANDATORY-b9fb literal {{ data }} 2",
                        ]
                        .into_iter()
                        .collect(),
                )?;
            }
            for key in [
                "GRAPHIT_ROLE_BEARER",
                "GRAPHIT_ROLE_REFERENCE",
                "GRAPHIT_ROLE_SELECTION",
            ] {
                for empty in [false, true] {
                    let mut environment: Vec<_> = std::env::vars_os()
                        .filter(|(name, _)| name != key)
                        .collect();
                    if empty {
                        environment.push((key.into(), "".into()));
                    }
                    let result = command_output(&argv[0], &argv[1..], Some(environment)).await?;
                    self.check(!result.status.success() && result.stdout.is_empty())?;
                }
            }
            let mut coordinator = argv.clone();
            coordinator.drain(recipient..recipient + 2);
            let rejected = command_output(&coordinator[0], &coordinator[1..], None).await?;
            self.check(!rejected.status.success() && rejected.stdout.is_empty())?;
        }
        Ok(())
    }
}
async fn command_output(
    program: &str,
    args: &[String],
    environment: Option<Vec<(std::ffi::OsString, std::ffi::OsString)>>,
) -> io::Result<std::process::Output> {
    let mut command = tokio::process::Command::new(program);
    command.args(args).stdin(Stdio::null()).kill_on_drop(true);
    if let Some(environment) = environment {
        command.env_clear().envs(environment);
    }
    tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
}
