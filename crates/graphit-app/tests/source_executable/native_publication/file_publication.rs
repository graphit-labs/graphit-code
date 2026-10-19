//! Real FILE CLI/MCP publication followed by the maintained install/update reader.
use super::*;
use graphit_core::native_path::NativePath;
use graphit_hub::{SourceRef, registry::HubRegistry};

const FILE_ID: &str = "fixture/file-skill";
const FILE_VERSION: &str = "release/one";

fn resources(f: &Fixture, index: usize, policy: &HubResourcePolicy) -> Result<std::path::PathBuf> {
    let producer = ProjectIdentityStore::at(&f.projects[index])
        .load()?
        .ok_or("missing producer")?
        .id();
    let source = SourceRef::resources(
        &f.projects[index],
        producer,
        ArtifactType::Skill,
        ArtifactId::new(FILE_ID, policy.projection())?,
        ConcreteVersion::new(FILE_VERSION, policy.projection())?,
        policy.projection(),
    )?;
    Ok(source.directory().as_path().to_owned())
}
fn seed(root: &std::path::Path, body: &str) -> Result {
    fs::create_dir_all(root.join("assets"))?;
    fs::write(
        root.join("SKILL.md"),
        format!("---\nname: file-skill\ndescription: Published FILE journey\n---\n{body}\n"),
    )?;
    fs::write(root.join("assets/literal.txt"), body)?;
    Ok(())
}
fn cli(f: &Fixture, args: &[&str]) -> Result<serde_json::Value> {
    let output = f.command().args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "FILE command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
async fn mcp_publish(f: &Fixture, expected: Option<&str>) -> Result<rmcp::model::CallToolResult> {
    mcp_call(f, "graphit_hub_publish_file", json!({
        "project_dir":NativePath::new(f.projects[0].root().to_owned())?,
        "project_version":"main",
        "publication":{"provider":PROVIDER,"artifact_type":"skill","id":FILE_ID,"version":FILE_VERSION,
            "name":"Published FILE journey","expected_sha256":expected,"set_latest":true}
    }).as_object().unwrap().clone()).await
}

async fn authored_untrusted_caller(f: &Fixture) -> Result {
    use rmcp::transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    };
    let discovery: serde_json::Value =
        serde_json::from_slice(&fs::read(f.paths.root().join("daemon/mcp.json"))?)?;
    let address = discovery["address"]
        .as_str()
        .ok_or("missing live Source address")?;
    let address: std::net::SocketAddr = address.parse()?;
    if !address.ip().is_loopback() {
        return Err("fixture Source is not loopback".into());
    }
    let config = StreamableHttpClientTransportConfig::with_uri(format!("http://{address}/mcp"))
        .custom_headers(std::collections::HashMap::from([
            (
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_static("Bearer untrusted-runtime-credential"),
            ),
            (
                reqwest::header::HeaderName::from_static("x-graphit-auth-origin"),
                reqwest::header::HeaderValue::from_static("runtime"),
            ),
        ]));
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        config,
    );
    let connection = ().serve(transport).await?;
    let result = connection
        .peer()
        .call_tool(
            CallToolRequestParams::new("graphit_hub_authored_list").with_arguments(json_arguments(
                json!({"project_dir":NativePath::new(f.projects[0].root().to_owned())?,
            "project_version":"main","query":{"limit":1}})
                .as_object()
                .unwrap()
                .clone(),
            )),
        )
        .await;
    let joined = connection.cancel().await;
    let denied = result?;
    joined?;
    if denied.is_error != Some(true)
        || denied
            .structured_content
            .as_ref()
            .map(|body| body["code"].as_str())
            != Some(Some("authentication_or_permission_denied"))
    {
        return Err(io::Error::other(format!(
            "untrusted Native caller was not denied: {:?}",
            denied.structured_content
        ))
        .into());
    }
    Ok(())
}

async fn authored_selection(
    f: &Fixture,
    root: &std::path::Path,
) -> Result<(String, String, String)> {
    let literal_relative = "assets/a:b\\literal.txt";
    fs::write(root.join(literal_relative), b"literal-native-name")?;
    let listed = cli(
        f,
        &[
            "--project-version",
            "main",
            "hub",
            "authored",
            "list",
            "--limit",
            "1",
        ],
    )?;
    let entry = listed["entries"]
        .as_array()
        .and_then(|entries| entries.first())
        .ok_or("missing authored tuple")?;
    let kind = entry["artifact_type"]
        .as_str()
        .ok_or("missing authored kind")?
        .to_owned();
    let id = entry["id"]
        .as_str()
        .ok_or("missing authored id")?
        .to_owned();
    let version = entry["version"]
        .as_str()
        .ok_or("missing authored version")?
        .to_owned();
    if kind != "skill"
        || id != FILE_ID
        || version != FILE_VERSION
        || !listed["next_cursor"].is_null()
    {
        return Err("authored CLI list did not enumerate the real Resources identity".into());
    }
    let reviewed = cli(
        f,
        &[
            "--project-version",
            "main",
            "hub",
            "authored",
            "review",
            "--kind",
            &kind,
            "--id",
            &id,
            "--version",
            &version,
            "--limit",
            "100",
        ],
    )?;
    let entries = reviewed["entries"]
        .as_array()
        .ok_or("missing authored review entries")?;
    let selected = entries
        .iter()
        .find(|entry| entry["path"] == "assets/literal.txt" && entry["kind"] == "File")
        .ok_or("authored review lost the companion file")?;
    let relative = selected["path"]
        .as_str()
        .ok_or("missing authored relative file")?;
    if reviewed["source"] != "Resources" || !reviewed["metadata"].is_null() {
        return Err("authored review changed the explicit Resources source".into());
    }
    let content = cli(
        f,
        &[
            "--project-version",
            "main",
            "hub",
            "authored",
            "content",
            "--kind",
            &kind,
            "--id",
            &id,
            "--version",
            &version,
            "--path",
            relative,
        ],
    )?;
    if content["content"]["content"] != "published_one" || content["content"]["bytes"] != 13 {
        return Err("authored CLI content did not read the selected companion".into());
    }
    if !entries
        .iter()
        .any(|entry| entry["path"] == literal_relative && entry["kind"] == "File")
    {
        return Err("review did not preserve the literal Unix file name".into());
    }
    let literal_content = cli(
        f,
        &[
            "--project-version",
            "main",
            "hub",
            "authored",
            "content",
            "--kind",
            &kind,
            "--id",
            &id,
            "--version",
            &version,
            "--path",
            literal_relative,
        ],
    )?;
    if literal_content["content"]["content"] != "literal-native-name" {
        return Err("CLI could not read the literal Unix path returned by review".into());
    }
    let project_dir = NativePath::new(f.projects[0].root().to_owned())?;
    let list = mcp_call(
        f,
        "graphit_hub_authored_list",
        json!({"project_dir":project_dir,
        "project_version":"main","query":{"limit":1}})
        .as_object()
        .unwrap()
        .clone(),
    )
    .await?;
    if list.is_error == Some(true)
        || list
            .structured_content
            .as_ref()
            .map(|body| &body["entries"])
            != Some(&listed["entries"])
    {
        return Err(io::Error::other(format!(
            "authored MCP list failed: {:?}",
            list.structured_content
        ))
        .into());
    }
    let review = mcp_call(f,"graphit_hub_authored_review",json!({"project_dir":project_dir,
        "project_version":"main","query":{"artifact_type":kind,"id":id,"version":version,"limit":1}}).as_object().unwrap().clone()).await?;
    if review.is_error == Some(true)
        || review.structured_content.as_ref().is_none_or(|body| {
            body["entries"]
                .as_array()
                .is_none_or(|entries| entries.len() != 1)
                || body["next_offset"] != 1
        })
    {
        return Err(io::Error::other(format!(
            "authored MCP review did not page: {:?}",
            review.structured_content
        ))
        .into());
    }
    let read = mcp_call(f,"graphit_hub_authored_content",json!({"project_dir":project_dir,
        "project_version":"main","query":{"artifact_type":kind,"id":id,"version":version,"path":relative}}).as_object().unwrap().clone()).await?;
    if read.is_error == Some(true)
        || read
            .structured_content
            .as_ref()
            .map(|body| &body["content"])
            != Some(&content["content"])
    {
        return Err(io::Error::other(format!(
            "authored MCP content failed: {:?}",
            read.structured_content
        ))
        .into());
    }
    let literal_read = mcp_call(f,"graphit_hub_authored_content",json!({"project_dir":project_dir,
        "project_version":"main","query":{"artifact_type":kind,"id":id,"version":version,"path":literal_relative}}).as_object().unwrap().clone()).await?;
    if literal_read.is_error == Some(true)
        || literal_read
            .structured_content
            .as_ref()
            .map(|body| &body["content"])
            != Some(&literal_content["content"])
    {
        return Err(io::Error::other(format!(
            "MCP could not read literal Unix path: {:?}",
            literal_read.structured_content
        ))
        .into());
    }
    authored_untrusted_caller(f).await?;
    let invalid = mcp_call(f,"graphit_hub_authored_content",json!({"project_dir":project_dir,
        "project_version":"main","query":{"artifact_type":kind,"id":id,"version":version,"path":"../SKILL.md"}}).as_object().unwrap().clone()).await?;
    if invalid.is_error != Some(true)
        || invalid
            .structured_content
            .as_ref()
            .map(|body| body["code"].as_str())
            != Some(Some("invalid_arguments"))
    {
        return Err("authored content accepted a parent traversal".into());
    }
    fs::remove_file(root.join(literal_relative))?;
    Ok((kind, id, version))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built real Broker STSv3/MinIO fixture"]
async fn compiled_file_publish_cli_mcp_installs_and_replaces_same_version() -> Result {
    let f = Fixture::new().await?;
    let broker = BrokerFixture::start(&f).await?;
    if let Err(primary) = install_active_profile(&f, &broker.ready, false) {
        let cleanup = broker.shutdown().await;
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
            let cleanup = broker.shutdown().await;
            return match cleanup {
                Ok(()) => Err(primary),
                Err(cleanup) => {
                    Err(io::Error::other(format!("{primary}; Broker cleanup: {cleanup}")).into())
                }
            };
        }
    };
    let result: Result = async {
        let config = ConfigSnapshot::new(
            f.paths.brand(),
            [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
        )?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let producer = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .ok_or("missing producer")?
            .id();
        let root = resources(&f, 0, &policy)?;
        seed(&root, "published_one")?;
        seed(&resources(&f, 1, &policy)?, "foreign_grant_denied")?;
        let (authored_kind, authored_id, authored_version) = authored_selection(&f, &root).await?;
        let denied = f
            .command()
            .args([
                "--project-version",
                "main",
                "hub",
                "publish",
                "--provider",
                PROVIDER,
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                FILE_VERSION,
                "--name",
                "Denied",
                "--project-dir",
            ])
            .arg(f.projects[1].root())
            .output()?;
        if denied.status.success()
            || !denied.stdout.is_empty()
            || !String::from_utf8_lossy(&denied.stderr)
                .contains("authentication_or_permission_denied")
        {
            return Err(
                "foreign producer publication was not denied at the real authority boundary".into(),
            );
        }
        let first = cli(
            &f,
            &[
                "--project-version",
                "main",
                "hub",
                "publish",
                "--provider",
                PROVIDER,
                "--kind",
                &authored_kind,
                "--id",
                &authored_id,
                "--version",
                &authored_version,
                "--name",
                "Published FILE journey",
                "--set-latest",
            ],
        )?;
        if first["status"] != "published"
            || first["publication"]["catalog"] != "published"
            || first["project_metadata"] != "published"
        {
            return Err("CLI did not publish FILE catalog".into());
        }
        let project_identity = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .ok_or("missing actual producer")?;
        let metadata_store = broker.store(producer).await?;
        let project_key = object_store::path::Path::from(format!(
            "{}/{}",
            broker.ready.prefix,
            graphit_hub::project_metadata::project_metadata_key(producer)
        ));
        let metadata_object = metadata_store.get(&project_key).await?;
        let initial_etag = metadata_object.meta.e_tag.clone();
        let metadata = graphit_hub::project_metadata::HubProjectMetadata::decode(
            &metadata_object.bytes().await?,
            producer,
            policy.content().max_input_bytes,
        )?;
        assert_eq!(metadata.name, project_identity.metadata().name);
        let mut catalog_url = reqwest::Url::parse(&format!(
            "{}/v1/hub/catalog/projects",
            broker.ready.endpoint.trim_end_matches('/')
        ))?;
        catalog_url
            .query_pairs_mut()
            .append_pair("id", &producer.to_string());
        let catalog_projects: serde_json::Value = reqwest::Client::new()
            .get(catalog_url)
            .bearer_auth(&broker.ready.access_bearer)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(
            catalog_projects["projects"],
            json!([{"id":producer,"name":metadata.name}])
        );
        let first_hash = first["publication"]["archive_sha256"]
            .as_str()
            .ok_or("missing archive hash")?
            .to_owned();
        let installed = cli(
            &f,
            &[
                "--project-version",
                "main",
                "hub",
                "install",
                "--provider",
                PROVIDER,
                "--producer",
                &producer.to_string(),
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                FILE_VERSION,
                "--binding",
                "codex",
                "--alias",
                "published-file",
            ],
        )?;
        if installed["status"] != "ok" {
            return Err("published FILE was not installed".into());
        }
        let projected = f.projects[0].root().join(".agents/skills/published-file");
        if fs::read_to_string(projected.join("assets/literal.txt"))? != "published_one" {
            return Err("FILE companion content lost".into());
        }
        let identity = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .ok_or("missing identity")?;
        let registry = HubRegistry::project(&f.projects[0], &identity, policy.hub())?;
        let document = registry.load()?;
        let artifact = document
            .get(ArtifactType::Skill, FILE_ID)
            .ok_or("missing installed FILE")?;
        let original = artifact
            .installations
            .first()
            .ok_or("missing FILE installation")?
            .original_source
            .directory()
            .as_path()
            .to_owned();
        let content = cli(
            &f,
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
                "--path",
                "SKILL.md",
                "--binding",
                "codex",
            ],
        )?;
        if !content["content"]["content"]
            .as_str()
            .ok_or("missing installed content")?
            .contains("published_one")
        {
            return Err("installed reader lost published bytes".into());
        }

        seed(&root, "published_two")?;
        let conflict = mcp_publish(&f, None).await?;
        if conflict.is_error != Some(true)
            || conflict
                .structured_content
                .as_ref()
                .ok_or("missing conflict")?["code"]
                != "file_publication_conflict"
        {
            return Err("MCP same-version replacement did not require expected hash".into());
        }
        let mut renamed_metadata = project_identity.metadata().clone();
        renamed_metadata.name = "renamed-file-producer".into();
        ProjectIdentityStore::at(&f.projects[0])
            .writer(Duration::from_secs(5), &Cancellation::default())?
            .update(producer, &renamed_metadata)?;
        let second = mcp_publish(&f, Some(&first_hash)).await?;
        if second.is_error == Some(true) {
            return Err("explicit MCP FILE replacement failed".into());
        }
        let second = second
            .structured_content
            .ok_or("missing MCP publication receipt")?;
        assert_eq!(
            second["project_metadata"], "published",
            "MCP rename has a confirmed metadata effect"
        );
        if second["publication"]["archive_sha256"] == first_hash
            || second["publication"]["catalog"] != "published"
        {
            return Err("MCP replacement did not publish a new digest".into());
        }
        let renamed_object = metadata_store.get(&project_key).await?;
        assert_ne!(
            renamed_object.meta.e_tag, initial_etag,
            "metadata rename must use actual conditional storage update"
        );
        let renamed = graphit_hub::project_metadata::HubProjectMetadata::decode(
            &renamed_object.bytes().await?,
            producer,
            policy.content().max_input_bytes,
        )?;
        assert_eq!(renamed.name, "renamed-file-producer");
        let mut catalog_url = reqwest::Url::parse(&format!(
            "{}/v1/hub/catalog/projects",
            broker.ready.endpoint.trim_end_matches('/')
        ))?;
        catalog_url
            .query_pairs_mut()
            .append_pair("name_prefix", "renamed-file");
        let renamed_page: serde_json::Value = reqwest::Client::new()
            .get(catalog_url)
            .bearer_auth(&broker.ready.access_bearer)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        assert_eq!(
            renamed_page["projects"],
            json!([{"id":producer,"name":renamed.name}])
        );
        let updated = cli(
            &f,
            &[
                "--project-version",
                "main",
                "hub",
                "update",
                "--provider",
                PROVIDER,
                "--producer",
                &producer.to_string(),
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                FILE_VERSION,
                "--binding",
                "codex",
                "--alias",
                "published-file",
            ],
        )?;
        if updated["status"] != "ok"
            || fs::read_to_string(projected.join("assets/literal.txt"))? != "published_two"
        {
            return Err("same-version FILE update did not reach consumer".into());
        }
        if fs::read_to_string(original.join("assets/literal.txt"))? != "published_one" {
            return Err("FILE replacement overwrote the retained original cache".into());
        }
        let remote = cli(
            &f,
            &[
                "hub",
                "content",
                "remote",
                "--provider",
                PROVIDER,
                "--producer",
                &producer.to_string(),
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                FILE_VERSION,
                "--path",
                "SKILL.md",
            ],
        )?;
        if !remote["content"]["content"]
            .as_str()
            .ok_or("missing remote content")?
            .contains("published_two")
        {
            return Err("remote content did not resolve updated digest".into());
        }
        let store = broker.store(producer).await?;
        let id = ArtifactId::new(FILE_ID, policy.projection())?;
        let key = object_store::path::Path::parse(format!(
            "{}/{}",
            broker.ready.prefix,
            graphit_hub::remote_storage::registry_key(producer, ArtifactType::Skill, &id)?
        ))?;
        let catalog: serde_json::Value =
            serde_json::from_slice(&store.get(&key).await?.bytes().await?)?;
        if catalog["versions"]
            .as_array()
            .ok_or("missing versions")?
            .len()
            != 1
            || catalog["versions"][0]["hash"] != second["publication"]["archive_sha256"]
        {
            return Err("FILE replacement catalog has incorrect version/hash".into());
        }
        Ok(())
    }
    .await;
    let daemon_cleanup = daemon.stop("-TERM").await;
    let broker_cleanup = broker.shutdown().await;
    let cleanup: Result = match (daemon_cleanup, broker_cleanup) {
        (Err(a), Err(b)) => {
            Err(io::Error::other(format!("daemon cleanup: {a}; Broker cleanup: {b}")).into())
        }
        (Err(e), _) | (_, Err(e)) => Err(e),
        (Ok(()), Ok(())) => Ok(()),
    };
    match (result, cleanup) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; cleanup: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), cleanup) => cleanup,
    }
}
