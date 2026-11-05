//! Executable composition: active Broker -> sync -> installed skills and live MCP.
use super::*;
use rmcp::{RoleClient, service::RunningService};

const ENTRY: &str = "skills/memory/entry.md";

async fn cli_output(f: &Fixture, args: &[&str]) -> Result<Output> {
    let mut command = tokio::process::Command::from(f.command());
    // This shared fixture retains main and feature for the same checkout path.
    // Select the intended version explicitly, as the MCP calls below also do.
    if matches!(args.first(), Some(&"sync" | &"skills")) {
        command.args(["--project-version", "main"]);
    }
    command.args(args).kill_on_drop(true);
    Ok(
        tokio::time::timeout(Duration::from_secs(30), command.output())
            .await
            .map_err(|cause| {
                io::Error::new(io::ErrorKind::TimedOut, format!("CLI {args:?}: {cause}"))
            })??,
    )
}

async fn cli_success(f: &Fixture, args: &[&str]) -> Result<Output> {
    let output = cli_output(f, args).await?;
    assert!(
        output.status.success(),
        "{args:?}: {}; daemon: {}",
        String::from_utf8_lossy(&output.stderr),
        fs::read_to_string(f._temp.path().join("daemon.log")).unwrap_or_default()
    );
    Ok(output)
}

async fn cli(f: &Fixture, args: &[&str]) -> Result<serde_json::Value> {
    let output = cli_success(f, args).await?;
    Ok(serde_json::from_slice(&output.stdout).map_err(|cause| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "CLI {args:?} did not return JSON: {cause}; stdout: {}",
                String::from_utf8_lossy(&output.stdout)
            ),
        )
    })?)
}

async fn skill(client: &RunningService<RoleClient, ()>, f: &Fixture) -> Result<String> {
    let result = client.peer().call_tool(
        CallToolRequestParams::new(f.paths.brand().mcp_tool_name(&["module", "skill"])?).with_arguments(json_arguments(
            json!({"module":"memory", "project_dir":f.projects[0].root(), "project_version":"main"})
                .as_object().unwrap().clone(),
        )),
    ).await?;
    assert_ne!(result.is_error, Some(true), "{result:?}");
    Ok(result
        .structured_content
        .as_ref()
        .and_then(|value| value["content"].as_str())
        .ok_or("module skill content missing")?
        .to_owned())
}

fn installed(f: &Fixture) -> Result<String> {
    Ok(fs::read_to_string(
        f.projects[0]
            .root()
            .join(".agents/skills")
            .join(f.paths.brand().skill_name("memory")?)
            .join("SKILL.md"),
    )?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_broker_instructions_sync_specific_general_precedence_errors_and_local_isolation()
-> Result {
    let f = Fixture::new().await?;
    let cancellation = Cancellation::default();
    let config = ConfigStore::global(&f.paths)?;
    for (key, enabled) in [
        ("modules.embedding", false),
        ("modules.knowledge", false),
        ("modules.dream", false),
        ("broker.allow_loopback_http", true),
    ] {
        config.set(
            ConfigKey::new(key)?,
            json!(enabled),
            Duration::from_secs(5),
            &cancellation,
        )?;
    }
    let broker = memory_index_auth::Broker::start().await;
    broker.instruction_reply(
        200,
        json!({"version":1,"templates":{ENTRY:"Broker first {{ brand }}."}}),
    );
    activate_memory_index_native_profile(&f, &broker)?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let mut command = tokio::process::Command::from(f.command());
    command
        .args(["mcp", "--stdio", "--project-dir"])
        .arg(f.projects[0].root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let client = ()
        .serve((
            child.stdout.take().ok_or("MCP stdout")?,
            child.stdin.take().ok_or("MCP stdin")?,
        ))
        .await?;
    let result: Result = async {
        let default = skill(&client, &f).await?;
        let first = format!("Broker first {}.", f.paths.brand().key());
        let updated = format!("Broker updated {}.", f.paths.brand().key());
        let global_body = format!("Global selected {}.", f.paths.brand().key());
        let project_body = format!("Project selected {}.", f.paths.brand().key());
        assert!(!default.contains("Broker first"));
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates/agent-instructions/coordinator").canonicalize()?;
        let initial_requests = broker.instruction_requests();
        let synced = cli(&f, &["sync", "--agent", "codex", "--coordinator-source", source.to_str().ok_or("non-Unicode source")?]).await?;
        assert_eq!(synced["steps"]["broker_instructions"]["status"], "reconciled");
        assert_eq!(broker.instruction_requests(), initial_requests + 1);
        assert_eq!(skill(&client, &f).await?, first);
        assert!(installed(&f)?.contains(&first));

        let global = f.paths.root().join("rules").join(ENTRY);
        let project = f.projects[0].root().join(f.paths.brand().directory()).join("rules").join(ENTRY);
        fs::create_dir_all(global.parent().ok_or("global parent")?)?;
        fs::write(&global, "Global selected {{ brand }}.")?;
        cli(&f, &["skills", "sync", "--agent", "codex"]).await?;
        assert_eq!(skill(&client, &f).await?, global_body);
        assert!(installed(&f)?.contains(&global_body));
        fs::create_dir_all(project.parent().ok_or("project parent")?)?;
        fs::write(&project, "Project selected {{ brand }}.")?;
        cli(&f, &["skills", "sync", "--agent", "codex"]).await?;
        assert_eq!(skill(&client, &f).await?, project_body);
        assert!(installed(&f)?.contains(&project_body));
        fs::remove_file(project)?;
        assert_eq!(skill(&client, &f).await?, global_body);
        fs::remove_file(global)?;
        assert_eq!(skill(&client, &f).await?, first);

        broker.instruction_reply(200, json!({"version":1,"templates":{ENTRY:"Broker updated {{ brand }}."}}));
        cli(&f, &["skills", "sync", "--agent", "codex"]).await?;
        assert_eq!(skill(&client, &f).await?, updated);
        assert!(installed(&f)?.contains(&updated));
        let cache = f.paths.root().join("broker-agent-instructions.json");
        let valid_cache = fs::read(&cache)?;
        let valid_installed = installed(&f)?;
        broker.instruction_reply(401, json!({"error":"unauthorized"}));
        let requests_before_denial = broker.instruction_requests();
        let denied = cli_output(&f, &["skills", "sync", "--agent", "codex"]).await?;
        assert!(!denied.status.success());
        assert_eq!(broker.instruction_requests(), requests_before_denial + 1);
        assert!(String::from_utf8_lossy(&denied.stdout).contains("401") || String::from_utf8_lossy(&denied.stderr).contains("401"), "{denied:?}");
        assert_eq!(fs::read(&cache)?, valid_cache);
        assert_eq!(installed(&f)?, valid_installed);
        assert_eq!(skill(&client, &f).await?, updated);

        broker.instruction_reply(503, json!({"error":"unavailable"}));
        let requests_before_unavailable = broker.instruction_requests();
        let failed = client.peer().call_tool(CallToolRequestParams::new(f.paths.brand().mcp_tool_name(&["sync"])?).with_arguments(
            json_arguments(json!({"project_dir":f.projects[0].root(),"project_version":"main","agent":"codex","coordinator_source":source})
                .as_object().unwrap().clone()),
        )).await?;
        assert_eq!(failed.is_error, Some(true), "{failed:?}");
        assert_eq!(broker.instruction_requests(), requests_before_unavailable + 1);
        let failure = failed.structured_content.as_ref().ok_or("sync error result missing")?;
        assert_eq!(failure["code"], "file_installation_failed");
        assert_eq!(failure["outcome"]["failed_step"], "broker_instructions");
        assert_eq!(fs::read(&cache)?, valid_cache);
        assert_eq!(installed(&f)?, valid_installed);
        assert_eq!(skill(&client, &f).await?, updated);

        broker.instruction_reply(200, json!({"version":1,"templates":{}}));
        cli(&f, &["skills", "sync", "--agent", "codex"]).await?;
        assert_eq!(skill(&client, &f).await?, default);
        assert!(!installed(&f)?.contains("Broker updated"));
        broker.instruction_reply(200, json!({"version":1,"templates":{ENTRY:"Broker updated {{ brand }}."}}));
        cli(&f, &["skills", "sync", "--agent", "codex"]).await?;
        assert_eq!(skill(&client, &f).await?, updated);
        let restored_cache = fs::read(&cache)?;
        cli_success(&f, &["provider", "add", "local", "--type", "local"]).await?;
        cli_success(&f, &["login", "--provider", "local", "--profile", "local-test", "--username", "fixture", "--mcp-key", "local-fixture-only"]).await?;
        let requests_before_local = broker.instruction_requests();
        assert_eq!(skill(&client, &f).await?, default);
        cli(&f, &["skills", "sync", "--agent", "codex"]).await?;
        assert_eq!(broker.instruction_requests(), requests_before_local);
        assert_eq!(fs::read(&cache)?, restored_cache);
        assert_eq!(skill(&client, &f).await?, default);
        assert!(!installed(&f)?.contains("Broker updated"));
        // A different provider must not parse or depend on another provider's cache.
        fs::write(&cache, b"not-a-json-broker-cache")?;
        assert_eq!(skill(&client, &f).await?, default);
        Ok(())
    }.await;
    client.cancel().await?;
    let status = tokio::time::timeout(Duration::from_secs(30), child.wait()).await??;
    assert!(status.success());
    let stopped = daemon.stop("-TERM").await;
    broker.shutdown().await;
    match (result, stopped) {
        (Err(primary), _) => Err(primary),
        (Ok(()), stopped) => stopped,
    }
}
