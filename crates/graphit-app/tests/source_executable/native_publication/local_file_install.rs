//! Compiled CLI and MCP use a real local source and the actual PROJECT writer.
use super::*;
use graphit_hub::{
    InstallationTarget, SourceOrigin,
    registry::{HubRegistry, InstalledBackend, InstalledPayload},
};
const SKILL: &str = "fixture/local-skill";
const AGENT: &str = "fixture/local-agent";
const VERSION: &str = "release/one";
fn success(f: &Fixture, args: &[&str]) -> Result<serde_json::Value> {
    let output = f
        .command()
        .args(["--project-version", "main"])
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "local install CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["status"], "ok");
    Ok(value)
}
fn physical(
    f: &Fixture,
    kind: ArtifactType,
    id: &str,
) -> Result<(graphit_hub::registry::Installation, std::path::PathBuf)> {
    let project = &f.projects[0];
    let identity = ProjectIdentityStore::at(project)
        .load()?
        .ok_or("consumer identity")?;
    let policy = HubResourcePolicy::resolve(&ConfigSnapshot::new(
        f.paths.brand(),
        [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
    )?)?;
    let registry = HubRegistry::project(project, &identity, policy.hub())?;
    let cancel = Cancellation::default();
    let request = policy.request_context(Instant::now(), &cancel)?;
    let scope = CapturedPublicationScope::project(
        project,
        identity.id(),
        ScopeLimits::new(16, 65536, 256)?,
        request.snapshot(),
    )?;
    let store = ArtifactStore::new(scope, policy.artifacts());
    let slot = store.publication_slot();
    let entry = registry
        .load()?
        .get_for_consumer(&slot, kind, id)
        .and_then(|entry| {
            entry.installations.iter().find(|entry| {
                entry.target == InstallationTarget::for_artifact(NativeBinding::Codex, kind)
            })
        })
        .cloned()
        .ok_or("missing local installation")?;
    let key = InstallationTarget::for_artifact(NativeBinding::Codex, kind).key(kind, id)?;
    let destination = store
        .load()?
        .ok_or("missing physical catalog")?
        .entries()
        .iter()
        .find(|entry| entry.key() == &key && entry.publication_slot() == slot)
        .ok_or("missing local physical entry")?
        .destination()
        .to_owned();
    Ok((entry, destination))
}
async fn denied_runtime(f: &Fixture, selection: serde_json::Value) -> Result {
    use rmcp::{
        model::ProtocolVersion,
        service::{ClientLifecycleMode, ClientServiceExt},
        transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        },
    };
    let discovery: serde_json::Value =
        serde_json::from_slice(&fs::read(f.paths.root().join("daemon/mcp.json"))?)?;
    let uri = format!(
        "http://{}/mcp",
        discovery["address"].as_str().ok_or("discovery address")?
    );
    let mut headers = std::collections::HashMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        axum::http::HeaderValue::from_static("Bearer invalid-runtime-fixture-credential"),
    );
    headers.insert(
        axum::http::HeaderName::from_static("x-graphit-auth-origin"),
        axum::http::HeaderValue::from_static("runtime"),
    );
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                reqwest::Client::builder().no_proxy().build()?,
                StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::LATEST],
            },
        )
        .await?;
    let result: Result = async {
        let denied = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_hub_install_local")
                    .with_arguments(json_arguments(selection.as_object().unwrap().clone())),
            )
            .await?;
        assert_eq!(denied.is_error, Some(true));
        let denied = denied
            .structured_content
            .ok_or("missing authority refusal")?;
        assert_eq!(denied["code"], "authentication_or_permission_denied");
        assert_eq!(denied["outcome"]["publication"], "not_attempted");
        let denied = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_sync").with_arguments(json_arguments(
                    json!({"project_dir":f.projects[0].root()})
                        .as_object()
                        .unwrap()
                        .clone(),
                )),
            )
            .await?;
        assert_eq!(denied.is_error, Some(true));
        let denied = denied.structured_content.ok_or("sync authority refusal")?;
        assert_eq!(denied["code"], "authentication_or_permission_denied");
        Ok(())
    }
    .await;
    let cleanup: Result = client.cancel().await.map(|_| ()).map_err(Into::into);
    result.and(cleanup)
}
async fn mcp_agent(f: &Fixture, source: &std::path::Path, producer: &str) -> Result {
    let mut command = tokio::process::Command::from(f.command());
    command
        .args([
            "--project-version",
            "main",
            "mcp",
            "--stdio",
            "--non-interactive",
        ])
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
    let result: Result = async {
        assert!(sdk.peer().list_tools(Default::default()).await?.tools.iter().any(|tool| tool.name == "graphit_hub_install_local"));
        let selection = json!({"source_dir":source,"producer":producer,"artifact_type":"agent","id":AGENT,"version":VERSION,"project_dir":f.projects[0].root(),"project_version":"main","binding":"codex","alias":"local-worker","mode":"copy"});
        let canonical_agent = fs::read(source.join("AGENT.md"))?;
        let mut denied_selection = selection.clone();
        denied_selection["source_dir"] = source.join("missing-source").to_string_lossy().as_ref().into();
        denied_runtime(f, denied_selection).await?;
        let (kind, root) = (ArtifactType::Agent, f.projects[0].root());
        for (field, value) in [("global",json!(true)), ("project_version",json!("not-monitored"))] {
            let mut invalid = selection.clone(); invalid[field] = value;
            invalid["source_dir"] = source.join("missing-source").to_string_lossy().as_ref().into();
            let denied = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_install_local").with_arguments(json_arguments(invalid.as_object().unwrap().clone()))).await?;
            assert_eq!(denied.is_error, Some(true));
            let denied = denied.structured_content.ok_or("missing selection refusal")?;
            assert_ne!(denied["code"], "artifact_not_found", "selection must fail before reading a missing source");
            assert!(physical(f, kind, AGENT).is_err());
        }
        // Generated/rendered publications have only their effective destination;
        // no retained payload exists to serve as a symlink target.
        let mut linked = selection.clone();
        linked["mode"] = "link".into();
        let refused = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_install_local").with_arguments(json_arguments(linked.as_object().unwrap().clone()))).await?;
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(refused.structured_content.ok_or("missing linked publication refusal")?["outcome"]["publication"], "not_attempted");
        assert!(physical(f, kind, AGENT).is_err());
        assert_eq!(fs::read(source.join("AGENT.md"))?, canonical_agent);
        let installed = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_install_local").with_arguments(json_arguments(selection.as_object().unwrap().clone()))).await?;
        assert_ne!(installed.is_error, Some(true), "{:?}", installed.structured_content);
        assert_eq!(installed.structured_content.ok_or("missing local result")?["status"], "ok");
        let (entry, destination) = physical(f, kind, AGENT)?;
        assert_eq!(entry.original_source.origin(), SourceOrigin::LocalLink);
        assert_eq!(entry.original_source.directory().as_path(), source);
        assert!(matches!(entry.backend, InstalledBackend::Physical { mode: ArtifactMode::Copy(_), .. }));
        assert!(matches!(entry.payload, InstalledPayload::Derived(_)));
        let body = fs::read_to_string(&destination)?;
        assert!(body.contains("local agent rendered"));
        assert!(!body.contains("{{brand}}"));
        assert!(fs::symlink_metadata(&destination)?.file_type().is_file());
        let removed = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_remove").with_arguments(json_arguments(json!({"artifact_type":"agent","scope":"project","id":AGENT,"version":VERSION,"project_dir":root,"project_version":"main","binding":"codex"}).as_object().unwrap().clone()))).await?;
        assert_ne!(removed.is_error, Some(true));
        assert!(!destination.try_exists()?);
        assert_eq!(fs::read(source.join("AGENT.md"))?, canonical_agent);
        Ok(())
    }.await;
    let cancelled = sdk.cancel().await;
    let joined = match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
        Ok(status) => status,
        Err(_) => {
            child.kill().await?;
            child.wait().await
        }
    };
    let cleanup: Result = async {
        cancelled?;
        if !joined?.success() {
            return Err("MCP local process failed to join".into());
        }
        Ok(())
    }
    .await;
    match (result, cleanup) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; MCP cleanup: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), cleanup) => cleanup,
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_local_hub_install_reapplies_and_removes_without_touching_source() -> Result {
    let f = Fixture::new().await?;
    for module in [
        "agents",
        "hooks",
        "memory",
        "ast",
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
    let skill = f._temp.path().join("canonical-skill");
    let agent = f._temp.path().join("canonical-agent");
    fs::create_dir_all(skill.join("assets"))?;
    fs::create_dir(&agent)?;
    let skill = fs::canonicalize(skill)?;
    let agent = fs::canonicalize(agent)?;
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: local-skill\ndescription: Local canonical instructions\n---\nfirst {{brand}} local instructions\n",
    )?;
    fs::write(skill.join("assets/companion.txt"), "companion one")?;
    fs::write(
        agent.join("AGENT.md"),
        "---\nname: local-worker\ndescription: Canonical local worker\nread_only: true\n---\nlocal agent rendered {{brand}}\n",
    )?;
    let producer = ProjectIdentityStore::at(&f.projects[1])
        .load()?
        .ok_or("producer identity")?
        .id()
        .to_string();
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let result: Result = async {
        let selected_source = skill.join("..").join(skill.file_name().ok_or("source directory name")?);
        let source = selected_source.to_str().ok_or("source UTF8")?;
        let project = f.projects[0].root().to_str().ok_or("project UTF8")?;
        let args = ["hub","install-local","--source-dir",source,"--producer",&producer,"--kind","skill","--id",SKILL,"--version",VERSION,"--project-dir",project,"--binding","codex"];
        let installed = success(&f, &[&args[..], &["--alias","local-alias"]].concat())?;
        assert_eq!(installed["source"], "local");
        let (entry, destination) = physical(&f, ArtifactType::Skill, SKILL)?;
        assert_eq!(entry.original_source.origin(), SourceOrigin::LocalLink);
        assert_eq!(entry.original_source.producer().to_string(), producer);
        assert_eq!(entry.original_source.directory().as_path(), skill);
        assert_eq!(entry.original_source.version().as_str(), VERSION);
        assert_eq!(entry.alias.as_deref(), Some("local-alias"));
        assert!(matches!(entry.backend, InstalledBackend::Physical { mode: ArtifactMode::Copy(_), .. }));
        assert!(fs::read_to_string(destination.join("SKILL.md"))?.contains(&format!("first {} local instructions", f.paths.brand().key())));
        assert_eq!(fs::read_to_string(destination.join("assets/companion.txt"))?, "companion one");
        fs::write(skill.join("SKILL.md"), "---\nname: local-skill\ndescription: Updated canonical instructions\n---\nsecond {{brand}} local instructions\n")?;
        fs::write(skill.join("assets/companion.txt"), "companion two")?;
        let synced = success(&f, &["hub", "sync", "--binding", "codex"])?;
        assert_eq!(synced["operation"], "sync");
        assert_eq!(synced["items"].as_array().ok_or("synced local artifacts")?.len(), 1);
        let (entry, reapplied) = physical(&f, ArtifactType::Skill, SKILL)?;
        assert_eq!(entry.alias.as_deref(), Some("local-alias"));
        assert_eq!(reapplied, destination);
        assert!(fs::read_to_string(destination.join("SKILL.md"))?.contains(&format!("second {} local instructions", f.paths.brand().key())));
        assert_eq!(fs::read_to_string(destination.join("assets/companion.txt"))?, "companion two");
        let rule = f._temp.path().join("canonical-rule");
        fs::create_dir_all(&rule)?;
        fs::write(rule.join("RULE.md"), "Local rule original, not a SkillDescriptor")?;
        success(&f, &["hub","install-local","--source-dir",rule.to_str().ok_or("rule path")?,"--producer",&producer,"--kind","rule","--id","fixture/local-rule","--version",VERSION,"--project-dir",project,"--binding","codex"])?;
        let synced = success(&f, &["hub","sync","--binding","codex"])?;
        assert_eq!(synced["items"].as_array().ok_or("original FILE sync items")?.len(), 2);
        assert!(synced["items"].as_array().unwrap().iter().all(|item| item["status"] == "ok"));
        success(&f, &["hub","remove","--kind","skill","--id",SKILL,"--version",VERSION,"--project-dir",project,"--binding","codex"])?;
        assert!(!destination.exists());
        assert!(skill.join("SKILL.md").exists());
        assert_eq!(fs::read_to_string(skill.join("assets/companion.txt"))?, "companion two");
        mcp_agent(&f, &agent, &producer).await?;
        fs::create_dir_all(&destination)?;
        fs::write(destination.join("foreign.txt"), "must remain")?;
        let refused = f.command().args(["--project-version","main"]).args(args).args(["--alias", "local-alias"]).output()?;
        assert!(!refused.status.success());
        assert_eq!(fs::read_to_string(destination.join("foreign.txt"))?, "must remain");
        Ok(())
    }.await;
    let stopped = daemon.stop("-TERM").await;
    match (result, stopped) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; daemon cleanup: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), stopped) => stopped,
    }
}
