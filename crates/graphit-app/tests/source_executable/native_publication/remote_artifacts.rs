//! Actual available-manifest CLI/MCP journey using Rust FILE and Native publishers.
use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use graphit_core::project_identity::ProjectId;
use graphit_hub::{SourceRef, remote_storage::registry_key};
use rmcp::{ServiceExt, model::CallToolRequestParams};

fn finish<T>(result: Result<T>, cleanup: Result) -> Result<T> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; cleanup: {cleanup}")).into())
        }
    }
}
pub(super) fn search(f: &Fixture, arguments: &[&str]) -> Result<serde_json::Value> {
    let output = f.command().args(arguments).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "search CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    if value["status"] != "ok" || !value["artifacts"].is_array() {
        return Err("search returned no real page".into());
    }
    for row in value["artifacts"].as_array().unwrap() {
        let keys: std::collections::BTreeSet<_> = row
            .as_object()
            .ok_or("manifest DTO is not object")?
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "artifact_type",
                "id",
                "name",
                "description",
                "tags",
                "author",
                "latest",
                "versions"
            ]
            .into_iter()
            .collect()
        );
    }
    Ok(value)
}
fn refused(f: &Fixture, arguments: &[&str], code: &str, exit: i32) -> Result {
    let output = f.command().args(arguments).output()?;
    if output.status.code() != Some(exit)
        || !String::from_utf8_lossy(&output.stderr).contains(code)
        || !output.stdout.is_empty()
    {
        return Err(io::Error::other(format!(
            "search refusal diverged: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}
async fn mcp_search(f: &Fixture, producer: ProjectId) -> Result<serde_json::Value> {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
    command
        .env_clear()
        .env("HOME", f._temp.path())
        .env(
            f.paths.brand().environment_variable("GLOBAL_DIR")?,
            f.paths.root(),
        )
        .current_dir(f.projects[0].root())
        .args(["mcp", "--stdio", "--non-interactive"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let sdk = match ()
        .serve((
            child.stdout.take().ok_or("MCP stdout")?,
            child.stdin.take().ok_or("MCP stdin")?,
        ))
        .await
    {
        Ok(sdk) => sdk,
        Err(cause) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(cause.into());
        }
    };
    let result: Result<_> = async {
        let tools = sdk.peer().list_tools(Default::default()).await?;
        assert!(tools.tools.iter().any(|tool| tool.name == "graphit_hub_search"));
        let response = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_search").with_arguments(json_arguments(json!({
            "transport":"broker", "provider":PROVIDER, "producer":producer,
            "artifact_type":"skill", "id":"fixture/search-alpha", "version":"release/one", "limit":2
        }).as_object().unwrap().clone()))).await?;
        if response.is_error == Some(true) { return Err("actual MCP search failed".into()) }
        let page = response.structured_content.ok_or("missing MCP structured search page")?;
        let denied = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_search").with_arguments(json_arguments(json!({
            "transport":"broker", "provider":PROVIDER, "producer":producer, "limit":2, "q":"invented full text"
        }).as_object().unwrap().clone()))).await?;
        assert_eq!(denied.is_error, Some(true));
        assert_eq!(denied.structured_content.ok_or("missing structured invalid input")?["code"], "invalid_arguments");
        Ok(page)
    }.await;
    let cancelled = sdk.cancel().await;
    let reaped = match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
        Ok(status) => status,
        Err(_) => {
            child.kill().await?;
            child.wait().await
        }
    };
    let cleanup: Result = async {
        cancelled?;
        if !reaped?.success() {
            return Err("MCP process did not join successfully".into());
        }
        Ok(())
    }
    .await;
    finish(result, cleanup)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built real Broker STSv3/MinIO fixture"]
async fn compiled_hub_search_reads_real_file_native_manifests_with_paging_and_current_authority()
-> Result {
    let f = Fixture::new().await?;
    fs::write(
        f.projects[0].root().join("initial.rs"),
        "pub fn search_native_manifest() {}\n",
    )?;
    let broker = BrokerFixture::start(&f).await?;
    let setup: Result = (|| {
        install_active_profile(&f, &broker.ready, false)?;
        for module in [
            "agents",
            "hooks",
            "memory",
            "knowledge",
            "embedding",
            "dream",
        ] {
            ConfigStore::global(&f.paths)?.set(
                ConfigKey::new(&format!("modules.{module}"))?,
                json!(false),
                Duration::from_secs(5),
                &Cancellation::default(),
            )?;
        }
        Ok(())
    })();
    if let Err(cause) = setup {
        return finish(Err(cause), broker.shutdown().await);
    }
    let mut command = f.command();
    command.env("GRAPHIT_MODULES__AST", "true").args([
        "daemon",
        "foreground",
        "--project-version",
        "main",
    ]);
    let daemon = match f.daemon_command(command).await {
        Ok(daemon) => daemon,
        Err(cause) => return finish(Err(cause), broker.shutdown().await),
    };
    let result: Result = async {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let output = f.command().env("GRAPHIT_MODULES__AST", "true").args(["ast", "schema", "--project-version", "main"]).output()?;
            if output.status.success() && serde_json::from_slice::<serde_json::Value>(&output.stdout)?["nodes"].as_array().is_some_and(|nodes| !nodes.is_empty()) { break }
            if Instant::now() >= until { return Err("real AST unavailable for search Native publisher".into()) }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let producer = ProjectIdentityStore::at(&f.projects[0]).load()?.ok_or("missing producer")?.id();
        let foreign = ProjectIdentityStore::at(&f.projects[1]).load()?.ok_or("missing sibling")?.id();
        let producer_text = producer.to_string();
        let foreign_text = foreign.to_string();
        let policy = HubResourcePolicy::resolve(&ConfigSnapshot::new(f.paths.brand(), [HubResourcePolicy::distribution_defaults(f.paths.brand())?])?)?;
        let base = ["hub", "search", "--provider", PROVIDER, "--producer", &producer_text];
        let empty = search(&f, &base)?;
        assert_eq!(empty["artifacts"], json!([]));
        for (id, version, latest) in [("fixture/search-alpha", "release/one", false), ("fixture/search-alpha", "release/two", true), ("fixture/search-zulu", "release/one", false)] {
            let source = SourceRef::resources(&f.projects[0], producer, ArtifactType::Skill, ArtifactId::new(id, policy.projection())?, ConcreteVersion::new(version, policy.projection())?, policy.projection())?;
            fs::create_dir_all(source.directory().as_path())?;
            fs::write(source.directory().as_path().join("SKILL.md"), format!("---\nname: search-fixture\ndescription: Real search publication\n---\n{id} {version}\n"))?;
            let mut args = vec!["--project-version", "main", "hub", "publish", "--provider", PROVIDER, "--kind", "skill", "--id", id, "--version", version, "--name", "Real search skill"];
            if latest { args.push("--set-latest") }
            let output = f.command().args(args).output()?;
            if !output.status.success() { return Err(io::Error::other(format!("real FILE search producer failed: {}", String::from_utf8_lossy(&output.stderr))).into()) }
        }
        publish_command(&f, "ast", "fixture/search-native", "release/native")?;
        let all = search(&f, &base)?;
        assert_eq!(all["artifacts"].as_array().unwrap().len(), 3);
        assert_eq!(all["artifacts"][0]["artifact_type"], "ast");
        let mut exact = base.to_vec(); exact.extend(["--kind", "skill", "--id", "fixture/search-alpha", "--version", "release/one"]);
        let page = search(&f, &exact)?;
        assert_eq!(page["artifacts"].as_array().unwrap().len(), 1);
        assert_eq!(page["artifacts"][0]["versions"], json!(["release/one", "release/two"]));
        assert_eq!(page["artifacts"][0]["latest"], "release/two");
        let mut absent = base.to_vec(); absent.extend(["--version", "latest"]);
        assert_eq!(search(&f, &absent)?["artifacts"], json!([]), "version is literal, not a selector");
        let mut zulu = base.to_vec(); zulu.extend(["--id", "fixture/search-zulu"]);
        assert!(search(&f, &zulu)?["artifacts"][0]["latest"].is_null());
        let mut args = base.to_vec(); args.extend(["--limit", "1"]);
        let first = search(&f, &args)?;
        let cursor = first["next_cursor"].as_str().ok_or("missing manifest next cursor")?.to_owned();
        let decoded: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&cursor)?)?;
        assert_eq!(decoded["after"]["artifact_type"], "ast");
        let mut next = args.clone(); next.extend(["--cursor", &cursor]);
        let second = search(&f, &next)?;
        let next_cursor = second["next_cursor"].as_str().ok_or("missing second manifest cursor")?.to_owned();
        let mut last_args = args.clone(); last_args.extend(["--cursor", &next_cursor]);
        let last = search(&f, &last_args)?;
        assert!(last["next_cursor"].is_null());
        assert_ne!(second["artifacts"][0]["id"], last["artifacts"][0]["id"]);
        let mut changed = base.to_vec(); changed.extend(["--limit", "2", "--cursor", &cursor]);
        refused(&f, &changed, "catalog_restart_required", 1)?;
        refused(&f, &["hub", "search", "--provider", PROVIDER, "--producer", &producer_text, "--limit", "0"], "invalid_arguments", 2)?;
        refused(&f, &["hub", "search", "--provider", PROVIDER, "--producer", &foreign_text], "authentication_or_permission_denied", 1)?;
        let mcp = mcp_search(&f, producer).await?;
        assert_eq!(mcp["artifacts"], page["artifacts"]);
        // Malformed current metadata at an authorized key must not become empty success.
        let store = broker.store(producer).await?;
        let alpha = ArtifactId::new("fixture/search-alpha", policy.projection())?;
        let key = object_store::path::Path::from(format!("{}/{}", broker.ready.prefix, registry_key(producer, ArtifactType::Skill, &alpha)?));
        store.put(&key, b"{invalid current manifest".to_vec().into()).await?;
        refused(&f, &exact, "invalid_remote_catalog", 1)?;
        Ok(())
    }.await;
    let daemon_cleanup = daemon.stop("-TERM").await;
    let broker_cleanup = broker.shutdown().await;
    match (result, daemon_cleanup, broker_cleanup) {
        (Ok(()), Ok(()), Ok(())) => Ok(()),
        (primary, daemon, broker) => Err(io::Error::other(format!(
            "search journey: {primary:?}; daemon: {daemon:?}; Broker: {broker:?}"
        ))
        .into()),
    }
}
