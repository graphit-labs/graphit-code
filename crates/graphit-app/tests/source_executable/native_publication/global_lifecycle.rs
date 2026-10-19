//! Real global metadata removal preserves cached payloads and sibling versions.
use super::*;
use graphit_hub::{InstallationTarget, SourceRef, registry::HubRegistry};
use rmcp::{ServiceExt, model::CallToolRequestParams};
const CACHE_ID: &str = "fixture/global-cache";
const NATIVE_ID: &str = "fixture/global-native";
const VERSIONS: [&str; 2] = ["release/one", "release/two"];

fn output(f: &Fixture, args: &[&str]) -> Result<std::process::Output> {
    Ok(f.command()
        .current_dir(f.paths.root())
        .args(args)
        .output()?)
}
fn success(f: &Fixture, args: &[&str]) -> Result<serde_json::Value> {
    let result = output(f, args)?;
    if !result.status.success() || !result.stderr.is_empty() {
        return Err(io::Error::other(format!(
            "global lifecycle CLI status {}; stderr {}",
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ))
        .into());
    }
    Ok(serde_json::from_slice(&result.stdout)?)
}
fn refused(f: &Fixture, args: &[&str], exit: i32, code: &str) -> Result {
    let result = output(f, args)?;
    assert_eq!(
        result.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(code),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}
fn present(
    registry: &HubRegistry,
    kind: ArtifactType,
    id: &str,
    version: &str,
    expected: bool,
) -> Result {
    let version = ConcreteVersion::new(version, registry.limits().projection())?;
    assert_eq!(
        registry.load()?.get_version(kind, id, &version).is_some(),
        expected,
        "metadata {id}@{}",
        version.as_str()
    );
    Ok(())
}
async fn denied_runtime(uri: &str) -> Result {
    use rmcp::{
        model::ProtocolVersion,
        service::{ClientLifecycleMode, ClientServiceExt},
        transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        },
    };
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
                StreamableHttpClientTransportConfig::with_uri(uri.to_owned())
                    .custom_headers(headers),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::LATEST],
            },
        )
        .await?;
    let result: Result = async {
        let denied = client.peer().call_tool(CallToolRequestParams::new("graphit_hub_remove").with_arguments(json_arguments(json!({"scope":"global_native","artifact_type":"ast","id":NATIVE_ID,"version":VERSIONS[0]}).as_object().unwrap().clone()))).await?;
        assert_eq!(denied.is_error, Some(true));
        let denied = denied.structured_content.ok_or("missing authorization error")?;
        assert_eq!(denied["code"], "authentication_or_permission_denied");
        assert_eq!(denied["outcome"]["publication"], "not_attempted");
        Ok(())
    }.await;
    let cleanup: Result = client.cancel().await.map(|_| ()).map_err(Into::into);
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; MCP cleanup: {cleanup}")).into())
        }
    }
}

async fn mcp_remove(f: &Fixture) -> Result {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
    command
        .env_clear()
        .env("HOME", f._temp.path())
        .env(
            f.paths.brand().environment_variable("GLOBAL_DIR")?,
            f.paths.root(),
        )
        .current_dir(f.paths.root())
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
        for name in ["graphit_hub_recover", "graphit_hub_forget"] {
            let tool = tools
                .tools
                .iter()
                .find(|tool| tool.name == name)
                .ok_or("missing PROJECT lifecycle tool")?;
            assert_eq!(
                tool.input_schema["properties"]["scope"]["enum"],
                json!(["project"])
            );
            assert!(
                !tool.input_schema["properties"]["artifact_type"]["enum"]
                    .as_array()
                    .ok_or("kind enumeration")?
                    .iter()
                    .any(|kind| kind == "ast" || kind == "knowledge")
            );
        }
        for (scope, kind, id) in [
            ("global_cache", "skill", CACHE_ID),
            ("global_native", "ast", NATIVE_ID),
        ] {
            let selection =
                json!({"scope":scope,"artifact_type":kind,"id":id,"version":VERSIONS[1]});
            for tool in ["graphit_hub_recover", "graphit_hub_forget"] {
                let result = sdk
                    .peer()
                    .call_tool(
                        CallToolRequestParams::new(tool)
                            .with_arguments(json_arguments(selection.as_object().unwrap().clone())),
                    )
                    .await?;
                assert_eq!(result.is_error, Some(true));
                assert_eq!(
                    result
                        .structured_content
                        .ok_or("invalid global operation result")?["code"],
                    "invalid_arguments"
                );
            }
            let removed = sdk
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_hub_remove")
                        .with_arguments(json_arguments(selection.as_object().unwrap().clone())),
                )
                .await?;
            assert_ne!(removed.is_error, Some(true));
            let removed = removed.structured_content.ok_or("global removal result")?;
            assert_eq!(removed["scope"], scope);
            assert_eq!(removed["version"], VERSIONS[1]);
            assert_eq!(removed["publication"]["change"], "MetadataRemoved");
            assert_eq!(removed["publication"]["committed"], true);
            assert_eq!(removed["payload_preserved"], true);
        }
        Ok(())
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
            return Err("global MCP child did not join successfully".into());
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
async fn compiled_global_hub_removal_is_exact_source_free_and_preserves_payloads() -> Result {
    let f = Fixture::new().await?;
    let mut broker = Some(BrokerFixture::start(&f).await?);
    let setup: Result = async {
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
        f.imported(
            "global-lifecycle-source",
            "native metadata removal keeps this source",
        )
        .await?;
        Ok(())
    }
    .await;
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
        let policy = HubResourcePolicy::resolve(&ConfigSnapshot::new(f.paths.brand(), [HubResourcePolicy::distribution_defaults(f.paths.brand())?])?)?;
        let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
        let registry = HubRegistry::global(&f.paths, identity.unit_id().ok_or("missing installation")?, policy.hub())?;
        let producer = ProjectIdentityStore::at(&f.projects[0]).load()?.ok_or("missing producer")?.id();
        let producer_text = producer.to_string();
        let native_source = f._temp.path().join("imported/global-lifecycle-source");
        let package = f._temp.path().join("global-native.package");
        let packed = output(&f, &["package", "export", "--kind", "ast", "--source", native_source.to_str().ok_or("package source path")?, "--output", package.to_str().ok_or("package path")?])?;
        assert!(packed.status.success(), "{}", String::from_utf8_lossy(&packed.stderr));
        let mut payloads = Vec::new();
        let mut upstream = Vec::new();
        for version in VERSIONS {
            let source = SourceRef::resources(&f.projects[0], producer, ArtifactType::Skill, ArtifactId::new(CACHE_ID, policy.projection())?, ConcreteVersion::new(version, policy.projection())?, policy.projection())?;
            fs::create_dir_all(source.directory().as_path())?;
            fs::write(source.directory().as_path().join("SKILL.md"), "---\nname: global-cache-fixture\ndescription: Concrete global cache\n---\nPayload must remain\n")?;
            let published = f.command().current_dir(f.projects[0].root()).args(["--project-version", "main", "hub", "publish", "--provider", PROVIDER, "--kind", "skill", "--id", CACHE_ID, "--version", version, "--name", "Global cache fixture"]).output()?;
            assert!(published.status.success(), "{}", String::from_utf8_lossy(&published.stderr));
            success(&f, &["hub", "install", "--global", "--provider", PROVIDER, "--producer", &producer_text, "--kind", "skill", "--id", CACHE_ID, "--version", version, "--exact"])?;
            upstream.push(source.directory().as_path().to_owned());
            let installed = success(&f, &["package", "install", "--input", package.to_str().ok_or("package path")?, "--producer", &producer_text, "--kind", "ast", "--id", NATIVE_ID, "--version", version])?;
            assert_eq!(installed["payload"], "Published");
            assert_eq!(installed["registry"], "Registered");
            let concrete = ConcreteVersion::new(version, policy.projection())?;
            for (kind, id, target) in [(ArtifactType::Skill, CACHE_ID, InstallationTarget::GlobalCache), (ArtifactType::Ast, NATIVE_ID, InstallationTarget::GlobalNative)] {
                let document = registry.load()?;
                let entry = document.get_version(kind, id, &concrete).ok_or("missing installed version")?;
                assert_eq!(entry.installations[0].target, target);
                let path = entry.installations[0].original_source.directory().as_path().to_owned();
                assert!(path.is_dir());
                payloads.push(path);
            }
        }
        // A genuine Native authentication warms its existing verifier, but the missing version changes nothing.
        refused(&f, &["hub", "remove", "--scope", "global-native", "--kind", "ast", "--id", NATIVE_ID, "--version", "missing"], 1, "artifact_not_found")?;
        let discovery: serde_json::Value = serde_json::from_slice(&fs::read(f.paths.root().join("daemon/mcp.json"))?)?;
        let address = discovery["address"].as_str().ok_or("discovery address")?;
        denied_runtime(&format!("http://{address}/mcp")).await?;
        for version in VERSIONS {present(&registry, ArtifactType::Ast, NATIVE_ID, version, true)?;}
        broker.take().unwrap().shutdown().await?;
        for path in upstream {fs::remove_dir_all(path)?;}
        fs::remove_file(&package)?;
        for (scope, kind, id) in [("global-cache", "skill", CACHE_ID), ("global-native", "ast", NATIVE_ID)] {
            refused(&f, &["hub", "remove", "--scope", scope, "--kind", kind, "--id", id], 2, "invalid_arguments")?;
            refused(&f, &["hub", "remove", "--scope", scope, "--kind", kind, "--id", id, "--version", VERSIONS[0], "--binding", "codex"], 2, "invalid_arguments")?;
            let removed = success(&f, &["hub", "remove", "--scope", scope, "--kind", kind, "--id", id, "--version", VERSIONS[0]])?;
            assert_eq!(removed["publication"]["change"], "MetadataRemoved");
            assert_eq!(removed["payload_preserved"], true);
        }
        for (kind, id) in [(ArtifactType::Skill, CACHE_ID), (ArtifactType::Ast, NATIVE_ID)] {
            present(&registry, kind, id, VERSIONS[0], false)?;
            present(&registry, kind, id, VERSIONS[1], true)?;
        }
        assert!(payloads.iter().all(|path| path.is_dir()));
        mcp_remove(&f).await?;
        for (kind, id) in [(ArtifactType::Skill, CACHE_ID), (ArtifactType::Ast, NATIVE_ID)] {
            for version in VERSIONS {present(&registry, kind, id, version, false)?;}
        }
        assert!(payloads.iter().all(|path| path.is_dir()), "metadata removal does not delete cached Native/FILE payloads");
        Ok(())
    }.await;
    let cleanup = daemon.stop("-INT").await;
    let broker_cleanup = if let Some(broker) = broker {
        broker.shutdown().await
    } else {
        Ok(())
    };
    let mut failure = result.err();
    for cleanup in [cleanup, broker_cleanup] {
        if let Err(cause) = cleanup {
            failure = Some(match failure {
                Some(primary) => io::Error::other(format!("{primary}; cleanup: {cause}")).into(),
                None => cause,
            });
        }
    }
    failure.map_or(Ok(()), Err)
}
