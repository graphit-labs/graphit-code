//! Real FILE lifecycle after publication, with the remote provider and source cache absent.
use super::*;
use graphit_hub::{InstallationTarget, NativeBinding, SourceRef, registry::HubRegistry};
use rmcp::{ServiceExt, model::CallToolRequestParams};

const FIRST: &str = "fixture/lifecycle-first";
const SECOND: &str = "fixture/lifecycle-second";
const VERSION: &str = "release/one";

fn cli(f: &Fixture, project: &ProjectPaths, arguments: &[&str]) -> Result<std::process::Output> {
    Ok(f.command()
        .current_dir(project.root())
        .args(["--project-version", "main"])
        .args(arguments)
        .output()?)
}
fn success(f: &Fixture, project: &ProjectPaths, arguments: &[&str]) -> Result<serde_json::Value> {
    let output = cli(f, project, arguments)?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "lifecycle CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_ne!(value["status"], "error");
    Ok(value)
}
fn refused(
    f: &Fixture,
    project: &ProjectPaths,
    arguments: &[&str],
    code: &str,
    exit: i32,
) -> Result {
    let output = cli(f, project, arguments)?;
    if output.status.code() != Some(exit)
        || !output.stdout.is_empty()
        || !String::from_utf8_lossy(&output.stderr).contains(code)
    {
        return Err(io::Error::other(format!(
            "lifecycle refusal diverged (expected {code} / exit {exit}, got {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}
fn destination(
    store: &ArtifactStore,
    target: InstallationTarget,
    id: &str,
) -> Result<std::path::PathBuf> {
    let key = target.key(ArtifactType::Skill, id)?;
    Ok(store
        .load()?
        .ok_or("missing physical catalog")?
        .entries()
        .iter()
        .find(|entry| entry.key() == &key)
        .ok_or("missing physical installation")?
        .destination()
        .to_owned())
}

async fn mcp_lifecycle(
    f: &Fixture,
    project: &ProjectPaths,
    first_other: &std::path::Path,
    foreign: &std::path::Path,
) -> Result {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
    command
        .env_clear()
        .env("HOME", f._temp.path())
        .env(
            f.paths.brand().environment_variable("GLOBAL_DIR")?,
            f.paths.root(),
        )
        .current_dir(project.root())
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
    let result: Result = async {
        let tools = sdk.peer().list_tools(Default::default()).await?;
        for tool in ["graphit_hub_remove", "graphit_hub_recover", "graphit_hub_forget"] {
            assert!(tools.tools.iter().any(|advertised| advertised.name == tool));
        }
        let selection = json!({"artifact_type":"skill","scope":"project","id":SECOND,"version":VERSION,"project_dir":project.root(),"project_version":"main","binding":"codex"});
        let recovered = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_recover")
            .with_arguments(json_arguments(selection.as_object().unwrap().clone()))).await?;
        assert_eq!(recovered.is_error, Some(true));
        let recovered = recovered.structured_content.ok_or("missing recovery failure")?;
        assert_eq!(recovered["outcome"]["publication"]["hub_pending"], true);
        assert_eq!(fs::read_to_string(foreign)?, "foreign data must remain");
        let mut other = selection.clone();
        other["id"] = FIRST.into();
        other["binding"] = "kiro-v3".into();
        let blocked = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_forget")
            .with_arguments(json_arguments(other.as_object().unwrap().clone()))).await?;
        assert_eq!(blocked.is_error, Some(true));
        assert_eq!(blocked.structured_content.ok_or("missing pending-key refusal")?["outcome"]["blocked_by_pending"], true);
        assert!(first_other.join("SKILL.md").is_file(), "another target must not be forgotten as a recovery side effect");
        let mut invalid = selection.clone();
        invalid["global"] = true.into();
        let invalid = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_forget")
            .with_arguments(json_arguments(invalid.as_object().unwrap().clone()))).await?;
        assert_eq!(invalid.is_error, Some(true));
        assert_eq!(invalid.structured_content.ok_or("missing strict input failure")?["code"], "invalid_arguments");
        let forgotten = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_forget")
            .with_arguments(json_arguments(selection.as_object().unwrap().clone()))).await?;
        assert_ne!(forgotten.is_error, Some(true));
        let forgotten = forgotten.structured_content.ok_or("missing Forget result")?;
        assert_eq!(forgotten["publication"]["change"], "Forgotten");
        assert!(!forgotten["forget_preview"].is_null(), "physical leftovers require the actual writer's preview");
        assert_eq!(fs::read_to_string(foreign)?, "foreign data must remain");
        let removed = sdk.peer().call_tool(CallToolRequestParams::new("graphit_hub_remove")
            .with_arguments(json_arguments(other.as_object().unwrap().clone()))).await?;
        assert_ne!(removed.is_error, Some(true));
        assert!(!first_other.exists());
        Ok(())
    }.await;
    let cancel = sdk.cancel().await;
    let reaped = match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
        Ok(status) => status,
        Err(_) => {
            child.kill().await?;
            child.wait().await
        }
    };
    let cleanup: Result = async {
        cancel?;
        if !reaped?.success() {
            return Err("MCP lifecycle process did not join successfully".into());
        }
        Ok(())
    }
    .await;
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; cleanup: {cleanup}")).into())
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built real Broker STSv3/MinIO fixture"]
async fn compiled_hub_file_lifecycle_is_source_free_and_preserves_foreign_pending() -> Result {
    let f = Fixture::new().await?;
    let mut broker = Some(BrokerFixture::start(&f).await?);
    let setup: Result = (|| {
        install_active_profile(&f, &broker.as_ref().unwrap().ready, false)?;
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
        Ok(())
    })();
    if let Err(primary) = setup {
        let cleanup = broker.take().unwrap().shutdown().await;
        return match cleanup {
            Ok(()) => Err(primary),
            Err(cleanup) => {
                Err(io::Error::other(format!("{primary}; Broker cleanup: {cleanup}")).into())
            }
        };
    }
    let daemon = match f.daemon(&["--project-version", "main"]).await {
        Ok(daemon) => daemon,
        Err(primary) => {
            let cleanup = broker.take().unwrap().shutdown().await;
            return match cleanup {
                Ok(()) => Err(primary),
                Err(cleanup) => {
                    Err(io::Error::other(format!("{primary}; Broker cleanup: {cleanup}")).into())
                }
            };
        }
    };
    let result: Result = async {
        let policy = HubResourcePolicy::resolve(&ConfigSnapshot::new(
            f.paths.brand(),
            [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
        )?)?;
        let producer = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .ok_or("missing producer")?
            .id();
        let consumer = &f.projects[1];
        let identity = ProjectIdentityStore::at(consumer)
            .load()?
            .ok_or("missing consumer")?;
        let producer_text = producer.to_string();
        for id in [FIRST, SECOND] {
            let source = SourceRef::resources(
                &f.projects[0],
                producer,
                ArtifactType::Skill,
                ArtifactId::new(id, policy.projection())?,
                ConcreteVersion::new(VERSION, policy.projection())?,
                policy.projection(),
            )?;
            fs::create_dir_all(source.directory().as_path())?;
            fs::write(
                source.directory().as_path().join("SKILL.md"),
                format!(
                    "---\nname: lifecycle-fixture\ndescription: Real local lifecycle\n---\n{id}\n"
                ),
            )?;
            success(
                &f,
                &f.projects[0],
                &[
                    "hub",
                    "publish",
                    "--provider",
                    PROVIDER,
                    "--kind",
                    "skill",
                    "--id",
                    id,
                    "--version",
                    VERSION,
                    "--name",
                    "Lifecycle fixture",
                ],
            )?;
        }
        for (id, binding, alias) in [
            (FIRST, "codex", "lifecycle-first"),
            (FIRST, "kiro-v3", "lifecycle-other"),
            (SECOND, "codex", "lifecycle-second"),
        ] {
            let installed = success(
                &f,
                consumer,
                &[
                    "hub",
                    "install",
                    "--provider",
                    PROVIDER,
                    "--producer",
                    &producer_text,
                    "--kind",
                    "skill",
                    "--id",
                    id,
                    "--version",
                    VERSION,
                    "--exact",
                    "--binding",
                    binding,
                    "--alias",
                    alias,
                ],
            )?;
            assert_eq!(installed["status"], "ok");
        }
        let cancellation = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancellation)?;
        let scope = CapturedPublicationScope::project(
            consumer,
            identity.id(),
            ScopeLimits::new(16, 65536, 256)?,
            request.snapshot(),
        )?;
        let physical = ArtifactStore::new(scope, policy.artifacts());
        let codex = InstallationTarget::for_artifact(
            "codex".parse::<NativeBinding>()?,
            ArtifactType::Skill,
        );
        let kiro = InstallationTarget::for_artifact(
            "kiro-v3".parse::<NativeBinding>()?,
            ArtifactType::Skill,
        );
        let first = destination(&physical, codex, FIRST)?;
        let first_other = destination(&physical, kiro, FIRST)?;
        let second = destination(&physical, codex, SECOND)?;
        let registry = HubRegistry::project(consumer, &identity, policy.hub())?;
        let document = registry.load()?;
        for id in [FIRST, SECOND] {
            let source = &document
                .get(ArtifactType::Skill, id)
                .ok_or("missing installed source")?
                .installations[0]
                .original_source;
            assert!(source.directory().as_path().starts_with(f.paths.root()));
            fs::remove_dir_all(source.directory().as_path())?;
        }
        broker.take().unwrap().shutdown().await?;
        // All operations below must finish without contacting that unavailable provider.
        refused(
            &f,
            consumer,
            &[
                "hub",
                "remove",
                "--kind",
                "skill",
                "--id",
                "../invalid",
                "--binding",
                "codex",
            ],
            "invalid_arguments",
            2,
        )?;
        refused(
            &f,
            consumer,
            &[
                "hub",
                "remove",
                "--kind",
                "skill",
                "--id",
                FIRST,
                "--binding",
                "codex",
                "--version",
                "release/other",
            ],
            "installation_conflict",
            1,
        )?;
        refused(
            &f,
            &f.projects[0],
            &[
                "hub",
                "remove",
                "--kind",
                "skill",
                "--id",
                FIRST,
                "--binding",
                "codex",
            ],
            "artifact_not_found",
            1,
        )?;
        refused(
            &f,
            consumer,
            &[
                "hub",
                "remove",
                "--kind",
                "skill",
                "--id",
                SECOND,
                "--binding",
                "kiro-v3",
            ],
            "artifact_not_found",
            1,
        )?;
        let recovered = success(
            &f,
            consumer,
            &[
                "hub",
                "recover",
                "--kind",
                "skill",
                "--id",
                FIRST,
                "--binding",
                "codex",
                "--version",
                VERSION,
            ],
        )?;
        assert_eq!(recovered["publication"]["change"], "Recovered");
        assert!(first.join("SKILL.md").is_file());
        let removed = success(
            &f,
            consumer,
            &[
                "hub",
                "remove",
                "--kind",
                "skill",
                "--id",
                FIRST,
                "--binding",
                "codex",
                "--version",
                VERSION,
            ],
        )?;
        assert_eq!(removed["operation"], "remove");
        assert!(!first.exists());
        assert!(first_other.join("SKILL.md").is_file());
        assert!(second.join("SKILL.md").is_file());
        let foreign = second.join("foreign.txt");
        fs::write(&foreign, "foreign data must remain")?;
        refused(
            &f,
            consumer,
            &[
                "hub",
                "remove",
                "--kind",
                "skill",
                "--id",
                SECOND,
                "--binding",
                "codex",
            ],
            "hub_pending",
            1,
        )?;
        assert!(
            registry.load()?.intent().is_some(),
            "actual failed removal keeps its recoverable intent"
        );
        assert_eq!(fs::read_to_string(&foreign)?, "foreign data must remain");
        mcp_lifecycle(&f, consumer, &first_other, &foreign).await?;
        let final_state = registry.load()?;
        assert!(final_state.intent().is_none());
        assert!(final_state.get(ArtifactType::Skill, FIRST).is_none());
        assert!(final_state.get(ArtifactType::Skill, SECOND).is_none());
        assert!(
            physical
                .load()?
                .ok_or("missing final physical catalog")?
                .pending()
                .is_none()
        );
        assert_eq!(fs::read_to_string(&foreign)?, "foreign data must remain");
        Ok(())
    }
    .await;
    let daemon_cleanup = daemon.stop("-TERM").await;
    let broker_cleanup = match broker.take() {
        Some(broker) => broker.shutdown().await,
        None => Ok(()),
    };
    match (result, daemon_cleanup, broker_cleanup) {
        (Ok(()), Ok(()), Ok(())) => Ok(()),
        (primary, daemon, broker) => Err(io::Error::other(format!(
            "FILE lifecycle: {primary:?}; daemon: {daemon:?}; Broker: {broker:?}"
        ))
        .into()),
    }
}
