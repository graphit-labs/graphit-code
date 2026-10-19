//! Main FILE consumers share captured versions, bound origins and the verified cache.
use super::*;
use graphit_hub::{
    InstallationTarget, RemoteFileOrigin, SourceRef,
    native::{DocumentFormat, FieldValue, NativeDocument},
    registry::{HubRegistry, InstalledBackend},
};

use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, CallToolResult},
};

async fn mcp(
    fixture: &Fixture,
    project: &ProjectPaths,
    tool: &str,
    input: serde_json::Value,
) -> Result<CallToolResult> {
    let mut command = tokio::process::Command::from(fixture.command());
    command
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
    let result: Result<CallToolResult> =
        async {
            let catalog = sdk.peer().list_tools(Default::default()).await?;
            let advertised = catalog
                .tools
                .iter()
                .find(|entry| entry.name == tool)
                .ok_or("missing FILE tool")?;
            let schema = serde_json::to_value(&advertised.input_schema)?;
            if tool == "graphit_hub_sync" {
                assert_eq!(schema["properties"]["global"]["type"], "boolean");
            } else {
                assert_eq!(
                    schema["properties"]["mode"]["enum"],
                    json!(["copy", "link"])
                );
            }
            Ok(sdk
                .peer()
                .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(
                    json_arguments(input.as_object().ok_or("MCP object")?.clone()),
                ))
                .await?)
        }
        .await;
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
            return Err("MCP physical-mode process did not join successfully".into());
        }
        Ok(())
    }
    .await;
    match (result, cleanup) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; cleanup: {cleanup}")).into())
        }
    }
}

pub(super) fn project_consumer(
    project: &ProjectPaths,
    policy: &HubResourcePolicy,
) -> Result<graphit_core::artifact_store::PublicationSlot> {
    let identity = ProjectIdentityStore::at(project)
        .load()?
        .ok_or("missing consumer identity")?;
    let cancel = Cancellation::default();
    let request = policy.request_context(Instant::now(), &cancel)?;
    Ok(CapturedPublicationScope::project(
        project,
        identity.id(),
        ScopeLimits::new(16, 65536, 256)?,
        request.snapshot(),
    )?
    .publication_slot())
}

const FILE_ID: &str = "fixture/file-consumers";
const ONE: &str = "release/one";
const TWO: &str = "release/two";
const BODY_ONE: &str = "journey_one {{brand}} {{artifact_version}} {{mcp_tool_ast_source}}";
const BODY_TWO: &str = "journey_two {{brand}} {{artifact_version}} {{mcp_tool_ast_source}}";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_update_all_cli_mcp_preserves_local_and_reports_unattempted() -> Result {
    let fixture = Fixture::new().await?;
    let broker = BrokerFixture::start(&fixture).await?;
    if let Err(error) = install_active_profile(&fixture, &broker.ready, false) {
        return joined(Err(error), broker.shutdown().await);
    }
    let daemon = match fixture.daemon(&["--project-version", "main"]).await {
        Ok(daemon) => daemon,
        Err(error) => return joined(Err(error), broker.shutdown().await),
    };
    let result: Result = async {
        let consumer = &fixture.projects[0];
        let identity = ProjectIdentityStore::at(consumer).load()?.ok_or("consumer identity")?;
        let producer = identity.id();
        let producer_text = producer.to_string();
        let policy = HubResourcePolicy::resolve(&ConfigSnapshot::new(fixture.paths.brand(), [])?)?;
        for (id, alias) in [("fixture/update-a", "update-a"), ("fixture/update-b", "update-b")] {
            seed(&broker, producer, id, ArtifactType::Skill, &[(ONE, BODY_ONE), (TWO, BODY_TWO)], &policy).await?;
            cli(&fixture, &["--project-version", "main", "hub", "install", "--provider", PROVIDER,
                "--producer", &producer_text, "--kind", "skill", "--id", id, "--version", ONE,
                "--binding", "codex", "--alias", alias])?;
        }
        let local = consumer.root().join("authored-update-local");
        fs::create_dir(&local)?;
        fs::write(local.join("SKILL.md"), "---\nname: local-stays\ndescription: Local update exclusion\n---\nlocal must remain unchanged\n")?;
        cli(&fixture, &["--project-version", "main", "hub", "install-local", "--source-dir", local.to_str().ok_or("local encoding")?,
            "--project-dir", consumer.root().to_str().ok_or("consumer encoding")?, "--producer", &producer_text,
            "--kind", "skill", "--id", "fixture/local-stays", "--version", ONE, "--binding", "codex"])?;
        let registry = HubRegistry::project(consumer, &identity, policy.hub())?;
        let slot = project_consumer(consumer, &policy)?;
        let local_before = registry.load()?.get_for_consumer(&slot, ArtifactType::Skill, "fixture/local-stays").cloned().ok_or("local installation")?;
        let synced = cli(&fixture, &["--project-version", "main", "hub", "sync"])?;
        assert_eq!(synced["operation"], "sync");
        assert_eq!(synced["items"].as_array().ok_or("sync items")?.len(), 3);
        for id in ["fixture/update-a", "fixture/update-b"] {
            let document = registry.load()?;
            let installed = &document.get_for_consumer(&slot, ArtifactType::Skill, id).ok_or("pinned installation")?.installations[0];
            assert_eq!(installed.original_source.version().as_str(), ONE);
        }
        let synced = mcp(&fixture, consumer, "graphit_hub_sync", json!({"global":false,"project_dir":consumer.root(),"project_version":"main"})).await?;
        assert_ne!(synced.is_error, Some(true), "{synced:?}");
        let updated = cli(&fixture, &["--project-version", "main", "hub", "update", "--binding", "codex"])?;
        let items = updated["items"].as_array().ok_or("collective items")?;
        assert_eq!(items.len(), 2);
        for (index, id) in ["fixture/update-a", "fixture/update-b"].iter().enumerate() {
            assert_eq!(items[index]["id"], *id);
            assert_eq!(items[index]["status"], "ok");
            assert_eq!(items[index]["outcome"]["version"], TWO);
            let document = registry.load()?;
            let installed = &document.get_for_consumer(&slot, ArtifactType::Skill, id).ok_or("updated installation")?.installations[0];
            assert_eq!(installed.alias.as_deref(), Some(if index == 0 { "update-a" } else { "update-b" }));
            assert_eq!(installed.original_source.remote_file_origin(), Some(&RemoteFileOrigin::Broker { provider: PROVIDER.into() }));
        }
        assert_eq!(registry.load()?.get_for_consumer(&slot, ArtifactType::Skill, "fixture/local-stays"), Some(&local_before));
        let repeated = mcp(&fixture, consumer, "graphit_hub_update", json!({"global":false,"project_dir":consumer.root(),"project_version":"main","binding":"codex"})).await?;
        assert_ne!(repeated.is_error, Some(true), "{repeated:?}");
        assert_eq!(repeated.structured_content.ok_or("collective MCP result")?["items"].as_array().ok_or("MCP items")?.len(), 2);
        let store = broker.store(producer).await?;
        let id = ArtifactId::new("fixture/update-a", policy.projection())?;
        // The publication grant supports PUT, not DELETE; corrupt metadata tests
        // a real first-item read failure without expanding fixture permissions.
        store.put(&object_key(&broker.ready.prefix, &registry_key(producer, ArtifactType::Skill, &id)?), b"{}".to_vec().into()).await?;
        let refused = mcp(&fixture, consumer, "graphit_hub_update", json!({"global":false,"project_dir":consumer.root(),"project_version":"main","binding":"codex"})).await?;
        assert_eq!(refused.is_error, Some(true));
        let failure = refused.structured_content.ok_or("collective failure result")?;
        assert_eq!(failure["outcome"]["items"][0]["status"], "error");
        assert_eq!(failure["outcome"]["items"][0]["outcome"]["publication"], "not_attempted");
        assert_eq!(failure["outcome"]["items"][1]["status"], "not_attempted");
        assert_eq!(registry.load()?.get_for_consumer(&slot, ArtifactType::Skill, "fixture/local-stays"), Some(&local_before));
        Ok(())
    }.await;
    let (daemon_cleanup, broker_cleanup) = tokio::join!(daemon.stop("-TERM"), broker.shutdown());
    joined(result, joined(daemon_cleanup, broker_cleanup))
}

fn cli(fixture: &Fixture, arguments: &[&str]) -> Result<serde_json::Value> {
    let output = fixture.command().args(arguments).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "FILE consumer failed ({arguments:?}): {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    if value["status"] != "ok" {
        return Err(io::Error::other(format!("FILE consumer returned {value}")).into());
    }
    Ok(value)
}

fn rejected(fixture: &Fixture, arguments: &[&str]) -> Result {
    let output = fixture.command().args(arguments).output()?;
    if output.status.success() {
        return Err(
            io::Error::other(format!("FILE consumer unexpectedly accepted {arguments:?}")).into(),
        );
    }
    Ok(())
}

fn raw_content(value: &serde_json::Value, marker: &str) -> Result {
    let content = value["content"]["content"]
        .as_str()
        .ok_or("FILE content response has no literal content")?;
    if value["content"]["path"] != "SKILL.md"
        || !content.contains(marker)
        || !content.contains("{{brand}}")
        || !content.contains("{{artifact_version}}")
        || !content.contains("{{mcp_tool_ast_source}}")
    {
        return Err(
            "installed/remote FILE content must retain the original Markdown template".into(),
        );
    }
    Ok(())
}

async fn seed(
    broker: &BrokerFixture,
    producer: graphit_core::project_identity::ProjectId,
    id: &str,
    artifact_type: ArtifactType,
    versions: &[(&str, &str)],
    policy: &HubResourcePolicy,
) -> Result {
    let store = broker.store(producer).await?;
    let id = ArtifactId::new(id, policy.projection())?;
    let mut digests = Vec::with_capacity(versions.len());
    for &(version, body) in versions {
        let version = ConcreteVersion::new(version, policy.projection())?;
        let bytes = if artifact_type == ArtifactType::Skill {
            payload(body)?
        } else {
            let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let entry = if artifact_type == ArtifactType::Agent {
                "AGENT.md"
            } else {
                "fallback.md"
            };
            archive.start_file(entry, zip::write::SimpleFileOptions::default())?;
            let text = if artifact_type == ArtifactType::Agent {
                format!(
                    "---\nname: file-reviewer\ndescription: FILE Agent fixture\nread_only: true\nallows_shell: false\n---\n{body}"
                )
            } else {
                body.to_owned()
            };
            std::io::Write::write_all(&mut archive, text.as_bytes())?;
            archive.finish()?.into_inner()
        };
        let hash: [u8; 32] = Sha256::digest(&bytes).into();
        let digest = hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        store
            .put(
                &object_key(
                    &broker.ready.prefix,
                    &format!(
                        "{}payload.zip",
                        release_prefix(producer, artifact_type, &id, &version, &hash)?
                    ),
                ),
                bytes.into(),
            )
            .await?;
        digests.push((version, digest));
    }
    let content = policy.content();
    let cancellation = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let versions: Vec<_> = digests
        .iter()
        .map(|(version, hash)| VersionInput {
            version: version.as_str(),
            hash,
        })
        .collect();
    let metadata = RemoteArtifactManifest::encode(
        &ManifestInput {
            producer,
            artifact_type,
            id: id.as_str(),
            name: "FILE consumer journeys",
            description: Some("Actual Main install, update, content and list"),
            tags: &[],
            author: None,
            latest: Some(versions.last().ok_or("empty version fixture")?.version),
            versions: &versions,
            dependencies: &[],
        },
        MetadataLimits {
            bytes: content.max_input_bytes,
            nodes: content.max_nodes,
            scalar_bytes: content.max_scalar_bytes,
            collection_entries: content.max_collection_entries,
            versions: content.max_collection_entries,
            dependencies: content.max_collection_entries,
            projection: policy.projection(),
        },
        &original,
    )?;
    store
        .put(
            &object_key(
                &broker.ready.prefix,
                &registry_key(producer, artifact_type, &id)?,
            ),
            metadata.into(),
        )
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_file_consumers_preserve_project_updates_global_versions_and_raw_content() -> Result
{
    let fixture = Fixture::new().await?;
    let broker = BrokerFixture::start(&fixture).await?;
    if let Err(error) = install_active_profile(&fixture, &broker.ready, false) {
        return joined(Err(error), broker.shutdown().await);
    }
    let daemon = match fixture.daemon(&["--project-version", "main"]).await {
        Ok(daemon) => daemon,
        Err(error) => return joined(Err(error), broker.shutdown().await),
    };
    let result: Result = async {
        let producer = ProjectIdentityStore::at(&fixture.projects[0])
            .load()?
            .ok_or("missing producer")?
            .id();
        let producer_text = producer.to_string();
        let config = ConfigSnapshot::new(fixture.paths.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        seed(
            &broker,
            producer,
            FILE_ID,
            ArtifactType::Skill,
            &[(ONE, BODY_ONE), (TWO, BODY_TWO)],
            &policy,
        )
        .await?;
        let install = cli(
            &fixture,
            &[
                "--project-version",
                "main",
                "hub",
                "install",
                "--provider",
                PROVIDER,
                "--producer",
                &producer_text,
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                ONE,
                "--binding",
                "codex",
                "--alias",
                "consumer-skill",
            ],
        )?;
        if install["version"] != ONE {
            return Err("PROJECT install resolved the wrong version".into());
        }
        raw_content(
            &cli(
                &fixture,
                &[
                    "--project-version",
                    "main",
                    "hub",
                    "content",
                    "installed",
                    "--kind",
                    "skill",
                    "--id",
                    FILE_ID,
                    "--binding",
                    "codex",
                ],
            )?,
            "journey_one",
        )?;
        let update = cli(
            &fixture,
            &[
                "--project-version",
                "main",
                "hub",
                "update",
                "--provider",
                PROVIDER,
                "--producer",
                &producer_text,
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                TWO,
                "--binding",
                "codex",
            ],
        )?;
        if update["version"] != TWO {
            return Err("PROJECT update resolved the wrong version".into());
        }
        let projected = fixture.projects[0]
            .root()
            .join(".agents/skills/consumer-skill/SKILL.md");
        let rendered = fs::read_to_string(&projected)?;
        let expected = format!(
            "journey_two {} {TWO} {}",
            fixture.paths.brand().key(),
            fixture.paths.brand().mcp_tool_name(&["ast", "source"])?
        );
        if !rendered.contains("name: consumer-skill")
            || !rendered.contains(&expected)
            || rendered.contains("{{")
            || fixture.projects[0]
                .root()
                .join(".agents/skills/file-consumers")
                .exists()
        {
            return Err("update lost the existing alias or captured template variables".into());
        }
        raw_content(
            &cli(
                &fixture,
                &[
                    "--project-version",
                    "main",
                    "hub",
                    "content",
                    "installed",
                    "--kind",
                    "skill",
                    "--id",
                    FILE_ID,
                    "--version",
                    TWO,
                    "--binding",
                    "codex",
                ],
            )?,
            "journey_two",
        )?;
        let listed = cli(
            &fixture,
            &[
                "--project-version",
                "main",
                "hub",
                "list",
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--binding",
                "codex",
                "--offset",
                "0",
                "--limit",
                "10",
            ],
        )?;
        let entries = listed["entries"]
            .as_array()
            .ok_or("missing PROJECT entries")?;
        if entries.len() != 1
            || entries[0]["version"] != TWO
            || entries[0]["alias"] != "consumer-skill"
            || entries[0]["origin"] != json!({"transport":"broker","provider":PROVIDER})
            || !listed["next_offset"].is_null()
        {
            return Err("PROJECT list lost the installed version, alias or bound origin".into());
        }
        rejected(
            &fixture,
            &[
                "--project-version",
                "main",
                "hub",
                "update",
                "--provider",
                "replacement-not-configured",
                "--producer",
                &producer_text,
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                ONE,
                "--binding",
                "codex",
            ],
        )?;
        if fs::read_to_string(&projected)? != rendered {
            return Err("failed origin replacement changed the installed projection".into());
        }

        for kind in [ArtifactType::Command, ArtifactType::Workflow] {
            let id = format!("fixture/{}-template", kind.as_str());
            seed(&broker, producer, &id, kind, &[(ONE, BODY_ONE)], &policy).await?;
            cli(
                &fixture,
                &[
                    "--project-version",
                    "main",
                    "hub",
                    "install",
                    "--provider",
                    PROVIDER,
                    "--producer",
                    &producer_text,
                    "--kind",
                    kind.as_str(),
                    "--id",
                    &id,
                    "--version",
                    ONE,
                    "--binding",
                    "claude",
                    "--alias",
                    &format!("{}-template", kind.as_str()),
                ],
            )?;
            let projected = fixture.projects[0]
                .root()
                .join(format!(".claude/commands/{}-template.md", kind.as_str()));
            let text = fs::read_to_string(projected)?;
            let expected = format!(
                "journey_one {} {ONE} {}",
                fixture.paths.brand().key(),
                fixture.paths.brand().mcp_tool_name(&["ast", "source"])?
            );
            if text != expected || text.contains("{{") {
                return Err(
                    "physical instruction did not render the SDK-selected fallback Markdown".into(),
                );
            }
        }

        const AGENT: &str = "fixture/file-agent";
        seed(&broker, producer, AGENT, ArtifactType::Agent, &[(ONE, BODY_ONE)], &policy).await?;
        for (binding, relative) in [
            ("claude", ".claude/agents/file-agent.md"),
            ("codex", ".codex/agents/file-agent.toml"),
            ("kiro-v3", ".kiro/agents/file-agent.md"),
        ] {
            cli(&fixture, &[
                "--project-version", "main", "hub", "install", "--provider", PROVIDER,
                "--producer", &producer_text, "--kind", "agent", "--id", AGENT,
                "--version", ONE, "--binding", binding,
            ])?;
            let text = fs::read_to_string(fixture.projects[0].root().join(relative))?;
            let expected = format!("journey_one {} {ONE} {}", fixture.paths.brand().key(), fixture.paths.brand().mcp_tool_name(&["ast", "source"])?);
            if !text.contains(&expected) || text.contains("{{") {
                return Err("Agent instructions did not render the captured variables".into());
            }
            if binding == "codex" {
                let cancel = Cancellation::default();
                let request = policy.request_context(Instant::now(), &cancel)?;
                let document = NativeDocument::parse(
                    DocumentFormat::Toml, text.as_bytes(), policy.shared(), &request,
                )?;
                let name = document.get(&["name"])?;
                let sandbox = document.get(&["sandbox_mode"])?;
                let shell = document.get(&["features", "shell_tool"])?;
                let instructions = document.get(&["developer_instructions"])?;
                if name != Some(FieldValue::String("file-agent".into()))
                    || sandbox.is_some()
                    || shell != Some(FieldValue::Boolean(false))
                    || !matches!(instructions, Some(FieldValue::String(ref body))
                    if body.contains(&expected) && !body.contains("{{"))
                {
                    return Err(io::Error::other(format!(
                    "Codex Agent changed its official name/shell projection or rendered instructions, or gained sandbox_mode: {}",
                    json!({"name": name, "sandbox_mode": sandbox, "shell_tool": shell}),
                )).into());
                }
            } else {
                let header = text.strip_prefix("---\n").ok_or("missing Agent frontmatter")?
                    .split("\n---\n").next().ok_or("unterminated Agent frontmatter")?;
                let metadata: serde_json::Value = serde_saphyr::from_str(header)?;
                let expected_tools = if binding == "claude" { json!(["Read", "Grep", "Glob"]) } else { json!(["read", "web"]) };
                if metadata["name"] != "file-agent" || metadata["tools"] != expected_tools {
                    return Err("Agent native grants exceed readonly/no-shell or use the wrong binding names".into());
                }
            }
        }

        let current_archive = payload(BODY_TWO)?;
        let current_hash: [u8; 32] = Sha256::digest(&current_archive).into();
        let payload_key = object_key(&broker.ready.prefix, &format!("{}payload.zip", release_prefix(
            producer, ArtifactType::Skill, &ArtifactId::new(FILE_ID, policy.projection())?,
            &ConcreteVersion::new(TWO, policy.projection())?, &current_hash,
        )?));
        let storage = broker.store(producer).await?;
        storage.put(&payload_key, b"corrupt payload; the verified copy must be used".as_slice().into()).await?;
        for version in [ONE, TWO] {
            cli(
                &fixture,
                &[
                    "hub",
                    "install",
                    "--global",
                    "--provider",
                    PROVIDER,
                    "--producer",
                    &producer_text,
                    "--kind",
                    "skill",
                    "--id",
                    FILE_ID,
                    "--version",
                    version,
                ],
            )?;
        }
        storage.put(&payload_key, current_archive.into()).await?;
        if fs::read_to_string(&projected)? != rendered
            || fixture._temp.path().join(".agents/skills").exists()
            || fixture.paths.root().join(".agents").exists()
            || fixture.paths.root().join(".codex").exists()
        {
            return Err(
                "GLOBAL FILE install must remain cache-only without native HOME/project effects"
                    .into(),
            );
        }
        let first = cli(
            &fixture,
            &[
                "hub", "list", "--global", "--kind", "skill", "--id", FILE_ID, "--offset", "0",
                "--limit", "1",
            ],
        )?;
        let second = cli(
            &fixture,
            &[
                "hub", "list", "--global", "--kind", "skill", "--id", FILE_ID, "--offset", "1",
                "--limit", "1",
            ],
        )?;
        let page_one = first["entries"]
            .as_array()
            .ok_or("missing first GLOBAL page")?;
        let page_two = second["entries"]
            .as_array()
            .ok_or("missing second GLOBAL page")?;
        if page_one.len() != 1
            || page_two.len() != 1
            || first["next_offset"] != 1
            || !second["next_offset"].is_null()
            || page_one[0]["version"] == page_two[0]["version"]
        {
            return Err(
                "GLOBAL pagination did not preserve two independent concrete versions".into(),
            );
        }
        for item in page_one.iter().chain(page_two) {
            if item["target"] != json!({"kind":"global_cache"})
                || item["alias"] != serde_json::Value::Null
                || item["origin"] != json!({"transport":"broker","provider":PROVIDER})
            {
                return Err("GLOBAL list exposed a native target or lost origin".into());
            }
        }
        rejected(
            &fixture,
            &[
                "hub",
                "content",
                "installed",
                "--global",
                "--kind",
                "skill",
                "--id",
                FILE_ID,
            ],
        )?;
        for (version, marker) in [(ONE, "journey_one"), (TWO, "journey_two")] {
            raw_content(
                &cli(
                    &fixture,
                    &[
                        "hub",
                        "content",
                        "installed",
                        "--global",
                        "--kind",
                        "skill",
                        "--id",
                        FILE_ID,
                        "--version",
                        version,
                    ],
                )?,
                marker,
            )?;
        }
        for invalid in ["--binding", "--alias"] {
            rejected(
                &fixture,
                &[
                    "hub",
                    "install",
                    "--global",
                    "--provider",
                    PROVIDER,
                    "--producer",
                    &producer_text,
                    "--kind",
                    "skill",
                    "--id",
                    FILE_ID,
                    "--version",
                    TWO,
                    invalid,
                    "codex",
                ],
            )?;
        }
        let identity = InstallationIdentityStore::new(&fixture.paths)?.load()?;
        let global = HubRegistry::global(
            &fixture.paths,
            identity.unit_id().ok_or("missing installation")?,
            policy.hub(),
        )?;
        let version = ConcreteVersion::new(TWO, policy.projection())?;
        let document = global.load()?;
        let entry = document
            .get_version(ArtifactType::Skill, FILE_ID, &version)
            .ok_or("missing GLOBAL FILE version")?;
        let installed = entry
            .installations
            .first()
            .ok_or("missing GLOBAL installation")?;
        if installed.target != InstallationTarget::GlobalCache
            || installed.backend != InstalledBackend::GlobalCache
        {
            return Err("GLOBAL FILE was recorded as a native publication".into());
        }
        let directory = installed.original_source.directory().as_path().to_owned();
        let held = directory
            .parent()
            .ok_or("cache parent")?
            .join("held-consumer-cache");
        fs::rename(&directory, &held)?;
        rejected(
            &fixture,
            &[
                "hub",
                "content",
                "installed",
                "--global",
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                TWO,
            ],
        )?;
        if directory.exists() {
            return Err("installed content hydrated a missing cache implicitly".into());
        }
        raw_content(
            &cli(
                &fixture,
                &[
                    "hub",
                    "content",
                    "remote",
                    "--provider",
                    PROVIDER,
                    "--producer",
                    &producer_text,
                    "--kind",
                    "skill",
                    "--id",
                    FILE_ID,
                    "--version",
                    TWO,
                    "--path",
                    "SKILL.md",
                ],
            )?,
            "journey_two",
        )?;
        if directory.exists() {
            return Err("ephemeral remote content published a physical cache".into());
        }
        fs::rename(held, &directory)?;
        const EPHEMERAL: &str = "fixture/ephemeral-file";
        seed(
            &broker,
            producer,
            EPHEMERAL,
            ArtifactType::Skill,
            &[(TWO, BODY_TWO)],
            &policy,
        )
        .await?;
        raw_content(
            &cli(
                &fixture,
                &[
                    "hub",
                    "content",
                    "remote",
                    "--provider",
                    PROVIDER,
                    "--producer",
                    &producer_text,
                    "--kind",
                    "skill",
                    "--id",
                    EPHEMERAL,
                    "--version",
                    TWO,
                    "--path",
                    "SKILL.md",
                ],
            )?,
            "journey_two",
        )?;
        if global.load()?.entries().count() != 2
            || global.load()?.get(ArtifactType::Skill, EPHEMERAL).is_some()
        {
            return Err("ephemeral remote content changed GLOBAL membership".into());
        }
        let ephemeral = SourceRef::hub_cache(
            &fixture.paths,
            producer,
            ArtifactType::Skill,
            ArtifactId::new(EPHEMERAL, policy.projection())?,
            None,
            version.clone(),
            policy.projection(),
        )?
        .with_remote_file_origin(RemoteFileOrigin::Broker { provider: PROVIDER.into() },
            Sha256::digest(payload(BODY_TWO)?).into(), policy.projection())?;
        if ephemeral.directory().as_path().exists() {
            return Err("uninstalled remote content leaked a cache artifact".into());
        }
        let project = HubRegistry::project(
            &fixture.projects[0],
            &ProjectIdentityStore::at(&fixture.projects[0])
                .load()?
                .ok_or("missing project identity")?,
            policy.hub(),
        )?
        .load()?;
        if project.get_for_consumer(&project_consumer(&fixture.projects[0], &policy)?, ArtifactType::Skill, EPHEMERAL).is_some() {
            return Err("ephemeral remote content changed PROJECT membership".into());
        }
        // A changed digest for the same concrete version creates another immutable copy.
        const REVISED: &str = "journey_revised {{brand}} {{artifact_version}} {{mcp_tool_ast_source}}";
        let old_source = installed.original_source.clone();
        let old_bytes = fs::read(old_source.directory().as_path().join("SKILL.md"))?;
        let mut old_open_projection = fs::File::open(&projected)?;
        seed(&broker, producer, FILE_ID, ArtifactType::Skill, &[(ONE, BODY_ONE), (TWO, REVISED)], &policy).await?;
        let installed_project = ["--project-version", "main", "hub", "content", "installed", "--kind", "skill", "--id", FILE_ID, "--binding", "codex"];
        raw_content(&cli(&fixture, &installed_project)?, "journey_two")?;
        raw_content(&cli(&fixture, &["hub", "content", "installed", "--global", "--kind", "skill", "--id", FILE_ID, "--version", TWO])?, "journey_two")?;
        raw_content(&cli(&fixture, &["hub", "content", "remote", "--provider", PROVIDER, "--producer", &producer_text, "--kind", "skill", "--id", FILE_ID, "--version", TWO, "--path", "SKILL.md"])?, "journey_revised")?;
        cli(&fixture, &["hub", "update", "--global", "--provider", PROVIDER, "--producer", &producer_text, "--kind", "skill", "--id", FILE_ID, "--version", TWO])?;
        let updated_global = global.load()?;
        let updated_source = &updated_global.get_version(ArtifactType::Skill, FILE_ID, &version)
            .ok_or("same-version update lost GLOBAL version")?.installations[0].original_source;
        if updated_source.id() != old_source.id() || updated_source.version() != old_source.version()
            || updated_source.physical_id() != old_source.physical_id()
            || updated_source.archive_sha256() == old_source.archive_sha256()
            || updated_source.directory() == old_source.directory()
            || fs::read(old_source.directory().as_path().join("SKILL.md"))? != old_bytes
            || fs::read_to_string(&projected)? != rendered {
            return Err("same-version GLOBAL update replaced an old snapshot, identity or PROJECT projection".into());
        }
        raw_content(&cli(&fixture, &installed_project)?, "journey_two")?;
        raw_content(&cli(&fixture, &["hub", "content", "installed", "--global", "--kind", "skill", "--id", FILE_ID, "--version", TWO])?, "journey_revised")?;
        cli(&fixture, &["--project-version", "main", "hub", "update", "--provider", PROVIDER, "--producer", &producer_text, "--kind", "skill", "--id", FILE_ID, "--version", TWO, "--binding", "codex"])?;
        let current = fs::read_to_string(&projected)?;
        let mut retained = String::new();
        std::io::Read::read_to_string(&mut old_open_projection, &mut retained)?;
        if !current.contains("journey_revised") || !current.contains("name: consumer-skill")
            || retained != rendered || fs::read(old_source.directory().as_path().join("SKILL.md"))? != old_bytes {
            return Err("same-version PROJECT update lost alias or invalidated an open/old snapshot".into());
        }
        raw_content(&cli(&fixture, &installed_project)?, "journey_revised")?;
        if global.load()?.entries().count() != 2 {
            return Err("a new digest must not fabricate another concrete version".into());
        }

        Ok(())
    }
    .await;
    let (daemon_cleanup, broker_cleanup) = tokio::join!(daemon.stop("-TERM"), broker.shutdown());
    joined(result, joined(daemon_cleanup, broker_cleanup))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_files_preserve_updates_retarget_forget_and_reject_generated_links() -> Result {
    use graphit_core::artifact_store::{ArtifactMode, CopyPolicy};
    use graphit_hub::{
        NativeBinding,
        registry::{Installation, InstalledPayload},
    };
    use std::path::PathBuf;

    const ID: &str = "fixture/project-physical-mode";
    fn installed(project: &ProjectPaths, policy: &HubResourcePolicy) -> Result<Installation> {
        let identity = ProjectIdentityStore::at(project)
            .load()?
            .ok_or("missing consumer identity")?;
        let document = HubRegistry::project(project, &identity, policy.hub())?.load()?;
        let target = InstallationTarget::for_artifact(NativeBinding::Codex, ArtifactType::Skill);
        document
            .get_for_consumer(&project_consumer(project, policy)?, ArtifactType::Skill, ID)
            .and_then(|artifact| {
                artifact
                    .installations
                    .iter()
                    .find(|entry| entry.target == target)
            })
            .cloned()
            .ok_or_else(|| "missing physical-mode installation".into())
    }
    fn projection(
        installation: &Installation,
        mode: ArtifactMode,
        rendered: &str,
    ) -> Result<PathBuf> {
        let InstalledBackend::Physical {
            mode: actual,
            destination,
            ..
        } = &installation.backend
        else {
            return Err("PROJECT mode did not produce a physical installation".into());
        };
        assert_eq!(*actual, mode);
        let destination = destination.as_path();
        assert_eq!(
            fs::symlink_metadata(destination)?.file_type().is_symlink(),
            mode == ArtifactMode::Link
        );
        let text = fs::read_to_string(destination.join("SKILL.md"))?;
        assert!(
            text.contains(rendered) && !text.contains("{{"),
            "physical body did not retain its rendered instructions"
        );
        assert!(
            matches!(&installation.payload, InstalledPayload::Derived(_)),
            "rendered publication must retain its origin and hashes"
        );
        Ok(destination.to_owned())
    }
    fn output_hash(installation: &Installation) -> Result<String> {
        let InstalledBackend::Physical { destination, .. } = &installation.backend else {
            return Err("missing effective rendered installation".into());
        };
        Ok(
            Sha256::digest(fs::read(destination.as_path().join("SKILL.md"))?)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
    }
    let fixture = Fixture::new().await?;
    let broker = BrokerFixture::start(&fixture).await?;
    if let Err(error) = install_active_profile(&fixture, &broker.ready, false) {
        return joined(Err(error), broker.shutdown().await);
    }
    let daemon = match fixture.daemon(&["--project-version", "main"]).await {
        Ok(daemon) => daemon,
        Err(error) => return joined(Err(error), broker.shutdown().await),
    };
    let result: Result = async {
        let producer = ProjectIdentityStore::at(&fixture.projects[0]).load()?.ok_or("missing producer")?.id();
        let consumer = &fixture.projects[1];
        assert_ne!(producer, ProjectIdentityStore::at(consumer).load()?.ok_or("missing consumer")?.id());
        let producer_text = producer.to_string();
        let consumer_path = consumer.root().to_str().ok_or("consumer path encoding")?;
        let config = ConfigSnapshot::new(fixture.paths.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        seed(&broker, producer, ID, ArtifactType::Skill, &[(ONE, BODY_ONE), (TWO, BODY_TWO)], &policy).await?;
        let publish = |operation: &str, version: &str, mode: Option<&str>, alias: Option<&str>| -> Result<serde_json::Value> {
            let mut args = vec!["--project-version", "main", "hub", operation, "--provider", PROVIDER, "--producer", &producer_text,
                "--kind", "skill", "--id", ID, "--version", version, "--binding", "codex", "--project-dir", consumer_path];
            if let Some(mode) = mode { args.extend(["--mode", mode]); }
            if let Some(alias) = alias { args.extend(["--alias", alias]); }
            cli(&fixture, &args)
        };
        let mut update = json!({"selection":{"transport":"broker","provider":PROVIDER,"producer":producer,"artifact_type":"skill","id":ID,"version":ONE,"exact":true},
            "global":false,"project_dir":consumer.root(),"project_version":"main","binding":"codex","mode":"link"});
        for mode in ["copy", "link"] {
            let rejected = mcp(&fixture, consumer, "graphit_hub_install", json!({"selection":update["selection"],"global":true,"mode":mode})).await?;
            assert_eq!(rejected.is_error, Some(true));
            assert_eq!(rejected.structured_content.ok_or("missing global mode refusal")?["outcome"]["cache"], "not_attempted");
        }
        let rendered = |marker: &str, version: &str| -> Result<String> {
            Ok(format!("{marker} {} {version} {}", fixture.paths.brand().key(), fixture.paths.brand().mcp_tool_name(&["ast", "source"])?))
        };
        publish("install", ONE, None, Some("mode-first"))?;
        let first = installed(consumer, &policy)?;
        assert_eq!(first.original_source.producer(), producer);
        assert_eq!(first.original_source.version().as_str(), ONE);
        let first_path = projection(&first, ArtifactMode::Copy(CopyPolicy::RejectFileLinks), &rendered("journey_one", ONE)?)?;
        let linked = mcp(&fixture, consumer, "graphit_hub_update", update.clone()).await?;
        assert_eq!(linked.is_error, Some(true), "generated links must be rejected: {linked:?}");
        let first_link = installed(consumer, &policy)?;
        assert_eq!(projection(&first_link, ArtifactMode::Copy(CopyPolicy::RejectFileLinks), &rendered("journey_one", ONE)?)?, first_path);
        assert_eq!(first_link.alias.as_deref(), Some("mode-first"));
        let first_payload = output_hash(&first_link)?;
        publish("update", TWO, None, None)?;
        let second_link = installed(consumer, &policy)?;
        projection(&second_link, ArtifactMode::Copy(CopyPolicy::RejectFileLinks), &rendered("journey_two", TWO)?)?;
        assert_eq!(second_link.original_source.producer(), producer);
        assert_eq!(second_link.original_source.version().as_str(), TWO);
        assert_eq!(second_link.alias.as_deref(), Some("mode-first"));
        assert!(output_hash(&second_link)? != first_payload, "changed rendered version must record its new output hash");
        update["selection"]["version"] = TWO.into();
        update.as_object_mut().unwrap().remove("mode");
        update["alias"] = "mode-next".into();
        let retargeted = mcp(&fixture, consumer, "graphit_hub_update", update.clone()).await?;
        assert_ne!(retargeted.is_error, Some(true), "{retargeted:?}");
        let retargeted = installed(consumer, &policy)?;
        let final_path = projection(&retargeted, ArtifactMode::Copy(CopyPolicy::RejectFileLinks), &rendered("journey_two", TWO)?)?;
        assert_ne!(final_path, first_path);
        assert!(fs::symlink_metadata(&first_path).is_err(), "retarget must detach only its previous native destination");
        assert_eq!(retargeted.alias.as_deref(), Some("mode-next"));
        publish("update", TWO, Some("copy"), None)?;
        projection(&installed(consumer, &policy)?, ArtifactMode::Copy(CopyPolicy::RejectFileLinks), &rendered("journey_two", TWO)?)?;
        update["mode"] = "link".into();
        let linked = mcp(&fixture, consumer, "graphit_hub_update", update.clone()).await?;
        assert_eq!(linked.is_error, Some(true), "generated links must be rejected: {linked:?}");
        let linked = installed(consumer, &policy)?;
        let final_path = projection(&linked, ArtifactMode::Copy(CopyPolicy::RejectFileLinks), &rendered("journey_two", TWO)?)?;
        cli(&fixture, &["--project-version", "main", "hub", "forget", "--scope", "project", "--kind", "skill", "--id", ID,
            "--version", TWO, "--binding", "codex", "--project-dir", consumer_path])?;
        assert!(!fs::symlink_metadata(&final_path)?.file_type().is_symlink());
        assert!(fs::read_to_string(final_path.join("SKILL.md"))?.contains(&rendered("journey_two", TWO)?));
        let identity = ProjectIdentityStore::at(consumer).load()?.ok_or("consumer identity after Forget")?;
        let document = HubRegistry::project(consumer, &identity, policy.hub())?.load()?;
        assert!(document.get_for_consumer(&project_consumer(consumer, &policy)?, ArtifactType::Skill, ID).is_none_or(|artifact| artifact.installations.is_empty()));
        let foreign = final_path.parent().ok_or("native parent")?.join("mode-foreign");
        fs::create_dir_all(&foreign)?;
        fs::write(foreign.join("SKILL.md"), "foreign bytes must remain")?;
        update["alias"] = "mode-foreign".into();
        update["mode"] = "copy".into();
        let refused = mcp(&fixture, consumer, "graphit_hub_install", update).await?;
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(fs::read_to_string(foreign.join("SKILL.md"))?, "foreign bytes must remain");
        assert!(final_path.is_dir(), "forget preserves the effective files");
        assert!(fs::read_to_string(final_path.join("SKILL.md"))?.contains(&rendered("journey_two", TWO)?));
        Ok(())
    }.await;
    joined(
        joined(result, daemon.stop("-TERM").await),
        broker.shutdown().await,
    )
}
