//! Real CLI → Runtime MCP → Auth SDK → Broker STSv3 → S3 publication.
use super::*;
use graphit_core::{
    artifact_store::{
        ArtifactKey, ArtifactMode, ArtifactStore, CapturedPublicationScope, CopyPolicy,
        PhysicalSource, ScopeLimits,
    },
    installation_identity::InstallationIdentityStore,
};
use graphit_hub::{ArtifactId, ArtifactType, ConcreteVersion};
use object_store::{ObjectStore, ObjectStoreExt};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use zeroize::Zeroize;

const PROVIDER: &str = "publication-broker";
const ARTIFACT: &str = "fixture/native-context";
const KNOWLEDGE: &str = "fixture/native-knowledge";
const AST_ONE: &str = "published source one from genuine SDK";
const AST_TWO: &str = "published source two from genuine SDK";
const KNOWLEDGE_ONE: &str = "exportableword first literal body.";
const KNOWLEDGE_TWO: &str = "exportableword second literal body.";
const NN_SOURCE: &str = "pub fn published_nn() -> i32 { 42 }\n";

#[derive(Deserialize)]
struct Ready {
    event: String,
    endpoint: String,
    issuer: String,
    resource: String,
    subject: String,
    expires_at: u64,
    access_bearer: String,
    standalone_storage: Option<file_installation::standalone_journeys::StandaloneStorage>,
    bucket: String,
    region: String,
    storage_endpoint: String,
    prefix: String,
}
impl Ready {
    /// The Broker fixture advertises JWT expiry in seconds; named OAuth profiles
    /// persist UTC microseconds, independently of STS and Device timestamps.
    fn oauth_expires_at_micros(
        &self,
    ) -> std::result::Result<i64, graphit_auth::profile::ProfileError> {
        i64::try_from(self.expires_at)
            .ok()
            .and_then(|seconds| seconds.checked_mul(1_000_000))
            .ok_or(graphit_auth::profile::ProfileError::InvalidDocument)
    }
}
impl Drop for Ready {
    fn drop(&mut self) {
        self.access_bearer.zeroize();
    }
}
struct BrokerFixture {
    child: tokio::process::Child,
    input: tokio::process::ChildStdin,
    output: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    ready: Ready,
}
impl BrokerFixture {
    async fn start(f: &Fixture) -> Result<Self> {
        Self::start_with(f, false, false).await
    }
    async fn start_standalone(
        f: &Fixture,
        catalog: bool,
    ) -> Result<(
        Self,
        file_installation::standalone_journeys::StandaloneStorage,
    )> {
        let mut fixture = Self::start_with(f, true, catalog).await?;
        let Some(storage) = fixture.ready.standalone_storage.take() else {
            fixture.shutdown().await?;
            return Err(
                "Broker fixture omitted the explicitly requested standalone storage".into(),
            );
        };
        Ok((fixture, storage))
    }
    async fn start_with(f: &Fixture, standalone: bool, catalog: bool) -> Result<Self> {
        let executable = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
            .ok_or("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN must name the built real Broker STSv3/MinIO fixture")?;
        let id = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .ok_or("missing producer")?
            .id();
        let second = ProjectIdentityStore::at(&f.projects[1])
            .load()?
            .ok_or("missing monitored sibling")?
            .id();
        let mut child = tokio::process::Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(f._temp.path().join("broker.log"))?)
            .kill_on_drop(true)
            .spawn()?;
        let mut input = child.stdin.take().ok_or("fixture stdin")?;
        let mut output =
            tokio::io::BufReader::new(child.stdout.take().ok_or("fixture stdout")?).lines();
        input
            .write_all(
                format!(
                    "{}\n",
                    json!({"command":"start","project_id":id,"second_project_id":second,"standalone":standalone,"standalone_catalog":catalog})
                )
                .as_bytes(),
            )
            .await?;
        input.flush().await?;
        let readiness: Result<Ready> = async {
            let line = zeroize::Zeroizing::new(
                tokio::time::timeout(Duration::from_secs(300), output.next_line())
                    .await??
                    .ok_or("fixture exited before Ready; see private broker.log")?,
            );
            let ready: Ready = serde_json::from_str(&line)?;
            if ready.event != "ready" {
                return Err("fixture did not send Ready".into());
            }
            Ok(ready)
        }
        .await;
        let ready = match readiness {
            Ok(ready) => ready,
            Err(cause) => {
                // EOF lets the public fixture join its owned children even before Ready.
                drop(input);
                match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
                    Ok(Ok(_)) => {}
                    _ => {
                        let _ = child.kill().await;
                    }
                }
                return Err(cause);
            }
        };
        Ok(Self {
            child,
            input,
            output,
            ready,
        })
    }
    async fn shutdown(mut self) -> Result {
        let command = self.input.write_all(b"{\"command\":\"shutdown\"}\n").await;
        drop(self.input);
        let acknowledged: Result = async {
            command?;
            let line = tokio::time::timeout(Duration::from_secs(300), self.output.next_line())
                .await??
                .ok_or("fixture exited before Shutdown acknowledgment")?;
            let ack: serde_json::Value = serde_json::from_str(&line)?;
            if ack != json!({"event":"ack","command":"shutdown","ok":true}) {
                return Err("Broker fixture sent an invalid Shutdown acknowledgment".into());
            }
            Ok(())
        }
        .await;
        let joined: Result = async {
            let status =
                tokio::time::timeout(Duration::from_secs(300), self.child.wait()).await??;
            if !status.success() {
                return Err(
                    io::Error::other(format!("Broker fixture exited with {status}")).into(),
                );
            }
            Ok(())
        }
        .await;
        match (acknowledged, joined) {
            (Err(primary), Err(cleanup)) => Err(io::Error::other(format!(
                "{primary}; Broker process join failed: {cleanup}"
            ))
            .into()),
            (Err(primary), _) => Err(primary),
            (Ok(()), joined) => joined,
        }
    }

    async fn store(
        &self,
        producer: graphit_core::project_identity::ProjectId,
    ) -> Result<Arc<dyn ObjectStore>> {
        let response = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .post(format!(
                "{}/v1/s3/credentials",
                self.ready.endpoint.trim_end_matches('/')
            ))
            .bearer_auth(&self.ready.access_bearer)
            .json(&json!({"scope":"project","project_id":producer,"module":"hub"}))
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        let field = |name: &str| {
            response[name].as_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid actual STSv3 response")
            })
        };
        let store = object_store::aws::AmazonS3Builder::new()
            .with_bucket_name(&self.ready.bucket)
            .with_region(&self.ready.region)
            .with_endpoint(&self.ready.storage_endpoint)
            .with_allow_http(true)
            .with_access_key_id(field("access_key_id")?)
            .with_secret_access_key(field("secret_access_key")?)
            .with_token(field("session_token")?)
            .build()?;
        Ok(Arc::new(store))
    }
}

fn install_active_profile(f: &Fixture, ready: &Ready, local_model: bool) -> Result {
    use graphit_auth::{
        profile::{
            AiService, BrokerService, LocalDevice, LocalExecution, OidcTokens, ProfileCredentials,
            ProfileData, ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind,
            S3Location, S3Source, Secret,
        },
        store::AuthStore,
    };
    use graphit_core::auth_document::{
        AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect,
    };
    let config = ConfigSnapshot::new(
        f.paths.brand(),
        [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
    )?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel)?;
    let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
    let scope = CapturedPublicationScope::installation(
        &f.paths,
        identity.unit_id().ok_or("missing installation")?.clone(),
        [f.paths.root().to_owned()],
        ScopeLimits::new(16, 65536, 256)?,
        original.snapshot(),
    )?;
    let store = AuthStore::new(
        AuthDocumentStore::capture(
            scope,
            AuthDocumentLimits::new(65536, 65536, 256, 65536)?,
            original.snapshot(),
        )?,
        ProfileLimits {
            bytes: 65536,
            nodes: 4096,
            scalar_bytes: 4096,
            collection_entries: 64,
            providers: 16,
            profiles: 16,
        },
        &original,
    )?;
    let expires_at = ready.oauth_expires_at_micros()?;
    let publication = store.mutate(store.load(&original)?, &original, |doc, ctx| {
        let config = ProviderConfig {
            kind: ProviderKind::Broker(BrokerService {
                endpoint: ready.endpoint.clone(),
                mcp_resource: Some(ready.resource.clone()),
                allow_anonymous: false,
            }),
            embedding: if local_model {
                AiService::Local(LocalExecution {
                    device: LocalDevice::Cpu,
                    device_id: 0,
                })
            } else {
                AiService::Disabled
            },
            rerank: AiService::Disabled,
            s3: S3Location {
                bucket: None,
                region: None,
                endpoint: None,
                prefix: None,
                source: S3Source::BrokerScope,
            },
        };
        if let Some(provider) = doc.provider(PROVIDER) {
            let (revision, updated) = (provider.revision(), provider.updated_at() + 1);
            doc.update_provider(PROVIDER, config, doc.revision(), revision, updated, ctx)?;
        } else {
            doc.add_provider(PROVIDER.into(), config, doc.revision(), 1, ctx)?;
        }
        let expected = ProfileExpectation {
            document_revision: doc.revision(),
            provider_revision: doc.provider(PROVIDER).unwrap().revision(),
            profile_revision: doc.profile("publication").map(|profile| profile.revision()),
        };
        let updated = doc
            .profile("publication")
            .map_or(1, |profile| profile.updated_at() + 1);
        doc.replace_profile_data(
            ProfileData {
                name: "publication".into(),
                provider: PROVIDER.into(),
                username: "display-only".into(),
                organization: None,
                teams: vec![],
                credentials: ProfileCredentials::Broker {
                    issuer: ready.issuer.clone(),
                    subject: ready.subject.clone(),
                    s3_disabled: false,
                    tokens: OidcTokens {
                        access_token: Secret::new(ready.access_bearer.clone().into()),
                        refresh_token: None,
                        // Persisted historical login metadata; access authenticity is verified by discovery/JWKS.
                        id_token: Secret::new("fixture-historical-id-token".into()),
                        expires_at,
                    },
                },
            },
            expected,
            updated,
            ctx,
        )?;
        doc.activate("publication", doc.revision(), ctx)?;
        Ok(())
    })?;
    if publication.effect() != AuthPublicationEffect::Applied
        || publication.failure().is_some()
        || publication.cleanup_failure().is_some()
    {
        return Err("active fixture profile was not published cleanly".into());
    }
    // Validate the actual installed document with the same Auth SDK before
    // launching the separate executable. Publishing bytes alone is insufficient.
    let installed = store.load(&original)?;
    let (provider, profile) = installed.document().active(&original)?;
    if profile.name() != "publication" || provider.name() != PROVIDER {
        return Err("fixture active profile does not match its Broker provider".into());
    }
    for (key, value) in [
        ("broker.allow_loopback_http", json!(true)),
        ("modules.ast", json!(true)),
        ("modules.embedding", json!(local_model)),
        ("modules.knowledge", json!(true)),
        ("modules.memory", json!(false)),
        ("modules.dream", json!(false)),
    ] {
        ConfigStore::global(&f.paths)?.set(
            ConfigKey::new(key)?,
            value,
            Duration::from_secs(5),
            &cancel,
        )?;
    }
    Ok(())
}

async fn publish_project_payload(f: &Fixture, seed: &str, text: &str) -> Result {
    // Reuse the existing genuine Icebug + Raw + indexed FTS builder. Installation
    // metadata is not an authorization proof; the PROJECT Copy below is the producer.
    f.imported(seed, text).await?;
    let source = f
        ._temp
        .path()
        .join("imported")
        .join(seed)
        .join("graph.icebug");
    let config = ConfigSnapshot::new(
        f.paths.brand(),
        [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
    )?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel)?;
    let project = &f.projects[0];
    let id = ProjectIdentityStore::at(project)
        .load()?
        .ok_or("missing project")?
        .id();
    let scope = CapturedPublicationScope::project(
        project,
        id,
        ScopeLimits::new(16, 65536, 256)?,
        original.snapshot(),
    )?;
    let store = ArtifactStore::new(scope, policy.artifacts());
    let destination = project.brand_directory().join("ast-graphs").join(
        Sha256::digest(b"main")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    );
    let mut writer = store.writer_with_context(original.snapshot())?;
    writer.ensure_publication_parent(&destination)?;
    writer.publish(
        &ArtifactKey::new("ast", "graph", "main")?,
        PhysicalSource::ExternalSource(&source),
        &destination,
        graphit_fs::NodeKind::Directory,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
    )?;
    Ok(())
}

fn publish_command(
    f: &Fixture,
    kind: &str,
    id: &str,
    artifact_version: &str,
) -> Result<serde_json::Value> {
    let output = f
        .command()
        .env("GRAPHIT_MODULES__AST", "true")
        .args([
            "hub",
            "publish-native",
            "--kind",
            kind,
            "--provider",
            PROVIDER,
            "--id",
            id,
            "--version",
            artifact_version,
            "--name",
            "NativeFixture",
            "--description",
            "**Native publication** from the real Hub CLI.",
            "--tag",
            "native-fixture",
            "--tag",
            "ação",
            "--project-version",
            "main",
        ])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()).into());
    }
    if !output.stderr.is_empty() {
        return Err(io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()).into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

async fn wait_productive_ast(f: &Fixture) -> Result {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let graph = f
            .command()
            .env("GRAPHIT_MODULES__AST", "true")
            .args(["ast", "schema", "--project-version", "main"])
            .output()?;
        if graph.status.success() {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(io::Error::other(format!(
                "productive AST publication did not become ready: status {}; stderr {}; payload {}",
                graph.status,
                String::from_utf8_lossy(&graph.stderr),
                String::from_utf8_lossy(&graph.stdout),
            ))
            .into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_productive_sources(f: &Fixture, expected_knowledge: &str) -> Result {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let graph = f
            .command()
            .env("GRAPHIT_MODULES__AST", "true")
            .args(["ast", "schema", "--project-version", "main"])
            .output()?;
        let knowledge = f
            .command()
            .env("GRAPHIT_MODULES__AST", "true")
            .args([
                "knowledge",
                "search",
                "exportableword",
                "--mode",
                "fts",
                "--preview",
                "--project-version",
                "main",
            ])
            .output()?;
        let payload = serde_json::from_slice::<serde_json::Value>(&knowledge.stdout).ok();
        if graph.status.success()
            && knowledge.status.success()
            && payload.as_ref().is_some_and(|p| {
                p["native"]["state"] == "hits"
                    && p["native"]["hits"].as_array().is_some_and(|hits| {
                        hits.iter().any(|hit| {
                            hit["preview"]["text"]
                                .as_str()
                                .is_some_and(|text| text.contains(expected_knowledge))
                        })
                    })
            })
        {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(io::Error::other(format!(
                "productive source publication did not reach expected Knowledge preview {expected_knowledge:?}: graph status: {}; stderr: {}; payload: {}; knowledge status: {}; stderr: {}; payload: {}; daemon: {}",
                graph.status,
                String::from_utf8_lossy(&graph.stderr),
                String::from_utf8_lossy(&graph.stdout),
                knowledge.status,
                String::from_utf8_lossy(&knowledge.stderr),
                String::from_utf8_lossy(&knowledge.stdout),
                fs::read_to_string(f._temp.path().join("daemon.log"))
                    .unwrap_or_else(|error| format!("unavailable: {error}"))
            ))
            .into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn root_digest(receipt: &serde_json::Value) -> Result<[u8; 32]> {
    let text = receipt["publication"]["root_sha256"]
        .as_str()
        .ok_or("published root digest")?;
    if text.len() != 64 {
        return Err("invalid native root digest".into());
    }
    Ok((0..32)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| "invalid root digest")?)
}
fn install_remote_versions(f: &Fixture, receipts: &[&serde_json::Value]) -> Result {
    use graphit_hub::registry::HubRegistry;
    let config = ConfigSnapshot::new(
        f.paths.brand(),
        [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
    )?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
    let registry = HubRegistry::global(
        &f.paths,
        identity.unit_id().ok_or("missing installation")?,
        policy.hub(),
    )?;
    let producer = ProjectIdentityStore::at(&f.projects[0])
        .load()?
        .ok_or("missing producer")?
        .id();
    for receipt in receipts {
        let publication = &receipt["publication"];
        let kind: ArtifactType = serde_json::from_value(publication["artifact_type"].clone())?;
        let id = publication["id"].as_str().ok_or("artifact id")?;
        let version = publication["version"]
            .as_str()
            .ok_or("publication version")?;
        let kind_name = match kind {
            ArtifactType::Ast => "ast",
            ArtifactType::Knowledge => "knowledge",
            _ => return Err("unexpected native kind".into()),
        };
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args([
                "hub",
                "install-native",
                "--provider",
                PROVIDER,
                "--producer",
                &producer.to_string(),
                "--kind",
                kind_name,
                "--id",
                id,
                "--version",
                version,
            ])
            .output()?;
        if !output.status.success() || !output.stderr.is_empty() {
            return Err(io::Error::other(format!(
                "native install {id}@{version}: status {}; stderr {}; body {}",
                output.status,
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout),
            ))
            .into());
        }
        let installed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if installed["status"] != "installed"
            || installed["producer"] != producer.to_string()
            || installed["id"] != id
            || installed["version"] != version
        {
            return Err(io::Error::other(format!("native install result: {installed}")).into());
        }
        let concrete = ConcreteVersion::new(version, policy.projection())?;
        let current = registry.load()?;
        let entry = current
            .get_version(kind, id, &concrete)
            .ok_or("installation missing")?;
        let expected_digest = root_digest(receipt)?;
        let installation = entry.installations.iter().find(|installation| {
            matches!(&installation.backend, graphit_hub::registry::InstalledBackend::RemoteNative(record)
                if record.root_digest() == &expected_digest)
        }).ok_or("installed release digest differs")?;
        let display = installation
            .display
            .as_ref()
            .ok_or("remote display missing")?;
        if display.name() != "NativeFixture"
            || display.description() != Some("**Native publication** from the real Hub CLI.")
            || display.tags() != ["native-fixture".to_owned(), "ação".to_owned()]
        {
            return Err("remote manifest display was not preserved".into());
        }
    }
    Ok(())
}

async fn assert_remote_consumers(f: &Fixture) -> Result {
    for (version, expected) in [("release/one", AST_ONE), ("release/two", AST_TWO)] {
        let context = format!("{ARTIFACT}@{version}");
        for entity in [false, true] {
            let mut command = f.command();
            command
                .current_dir(f.paths.root())
                .env("GRAPHIT_MODULES__AST", "true")
                .args(["ast", "source", "same", "--context", &context]);
            if entity {
                command.args(["--entity", "same", "--entity-type", "File"]);
            }
            let output = command.output()?;
            if !output.status.success()
                || !output.stderr.is_empty()
                || output.stdout != expected.as_bytes()
            {
                return Err(io::Error::other(format!(
                    "remote AST source {version}, entity={entity}: status {}; stderr {}; body {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr),
                    String::from_utf8_lossy(&output.stdout)
                ))
                .into());
            }
        }
        let output = f
            .command()
            .current_dir(f.paths.root())
            .env("GRAPHIT_MODULES__AST", "true")
            .args([
                "ast",
                "search",
                "published",
                "--context",
                &context,
                "--mode",
                "fts",
                "--k",
                "8",
            ])
            .output()?;
        if !output.status.success() {
            return Err(
                io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()).into(),
            );
        }
        let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if result["status"] != "available"
            || !result["results"]
                .as_array()
                .is_some_and(|hits| hits.iter().any(|hit| hit["path"] == "same"))
        {
            return Err(io::Error::other(format!("remote AST FTS {version}: {result}")).into());
        }
    }
    for version in ["release/one", "release/two"] {
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args([
                "knowledge",
                "search",
                "exportableword",
                "--artifact-id",
                KNOWLEDGE,
                "--artifact-version",
                version,
                "--mode",
                "fts",
                "--preview",
            ])
            .output()?;
        if !output.status.success() {
            return Err(
                io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()).into(),
            );
        }
        let payload: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if payload["native"]["state"] != "hits"
            || payload["native"]["total"] != 1
            || !payload["native"]["hits"][0]["preview"]["text"]
                .as_str()
                .is_some_and(|text| text.contains("exportableword"))
        {
            return Err(
                io::Error::other(format!("remote Knowledge FTS {version}: {payload}")).into(),
            );
        }
    }
    let mut command = tokio::process::Command::from(f.command());
    command
        .current_dir(f.paths.root())
        .env("GRAPHIT_MODULES__AST", "true")
        .args(["mcp", "--stdio", "--non-interactive"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let sdk = ()
        .serve((
            child.stdout.take().ok_or("MCP stdout")?,
            child.stdin.take().ok_or("MCP stdin")?,
        ))
        .await?;
    let result: Result = async {
        for version in ["release/one", "release/two"] {
            let context = format!("{ARTIFACT}@{version}");
            let response = sdk.peer().call_tool(CallToolRequestParams::new("graphit_ast_source").with_arguments(json_arguments(json!({"path":"same","context":context}).as_object().unwrap().clone()))).await?;
            let actual: String = response.content.iter().filter_map(|c|c.as_text()).map(|t|t.text.as_str()).collect();
            let expected = if version == "release/one" {AST_ONE} else {AST_TWO};
            if response.is_error == Some(true) || actual != expected {
                return Err(io::Error::other(format!("remote MCP AST source {version}: {response:?}")).into());
            }
            let schema = sdk.peer().call_tool(CallToolRequestParams::new("graphit_ast_schema").with_arguments(json_arguments(json!({"context":context}).as_object().unwrap().clone()))).await?;
            if schema.is_error == Some(true) { return Err(io::Error::other(format!("remote MCP schema: {schema:?}")).into()); }
            let queried = sdk.peer().call_tool(CallToolRequestParams::new("graphit_ast_query").with_arguments(json_arguments(json!({"context":context,"query":"MATCH (a:File) RETURN a.path","parameters":{}}).as_object().unwrap().clone()))).await?;
            if queried.is_error == Some(true) || !queried.structured_content.as_ref().is_some_and(|payload|payload.to_string().contains("same")) {
                return Err(io::Error::other(format!("remote MCP Cypher: {queried:?}")).into());
            }
            let knowledge_source = json!({"kind":"imported_knowledge","artifact":{"id":KNOWLEDGE,"version":version}});
            for (mode,state) in [("fts","hits"),("semantic","unavailable")] {
                let response = sdk.peer().call_tool(CallToolRequestParams::new("graphit_knowledge_search").with_arguments(json_arguments(json!({"source":knowledge_source,"search":{"query":"exportableword","mode":mode,"preview":true}}).as_object().unwrap().clone()))).await?;
                if response.is_error == Some(true) { return Err(io::Error::other(format!("remote MCP Knowledge: {response:?}")).into()); }
                let payload = response.structured_content.ok_or("Knowledge result")?;
                if payload["native"]["state"] != state || (mode == "semantic" && (payload["native"]["reason"] != "model_unavailable" || !payload["native"]["semantic_generation"].is_null()))
                    || (mode == "fts" && payload["native"]["total"] != 1) {
                    return Err(io::Error::other(format!("remote MCP Knowledge {mode}: {payload}")).into());
                }
            }
        }
        Ok(())
    }.await;
    let disconnected = sdk.cancel().await;
    let joined = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
    let cleanup: Result = async {
        disconnected?;
        let status = joined??;
        if !status.success() {
            return Err(io::Error::other(format!("MCP child exited with {status}")).into());
        }
        Ok(())
    }
    .await;
    match (result, cleanup) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; MCP cleanup failed: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), cleanup) => cleanup,
    }
}

async fn install_cpu_model(f: &Fixture) -> Result {
    use graphit_app::model_provision::{
        InstallSource, ModelProvisioner, PinnedModel, ProvisionLimits,
    };
    use graphit_core::artifact_store::CapturedSourceReader;
    let source = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL")
            .ok_or("missing GRAPHIT_EMBEDDINGGEMMA2_MODEL")?,
    );
    let original = Arc::new(graphit_core::file_io::SnapshotContext::new(
        Instant::now() + Duration::from_secs(60),
        Cancellation::default(),
    ));
    let producer =
        ModelProvisioner::capture(&f.paths, original.clone(), ProvisionLimits::product()?)?;
    let reader = CapturedSourceReader::capture(
        &source,
        graphit_fs::NodeKind::File,
        &producer.limits().snapshot,
        producer.limits().read,
        original.clone(),
    )?;
    let relative = reader
        .nodes()
        .next()
        .ok_or("missing captured GGUF")?
        .relative()
        .to_owned();
    producer
        .install(
            PinnedModel::EmbeddingGemma2,
            InstallSource::Captured {
                reader: &reader,
                relative: &relative,
            },
            &mut || original.check(),
        )
        .await?;
    Ok(())
}
fn model_daemon_command(f: &Fixture) -> Result<Command> {
    let model = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_TEST_MODEL_SERVER")
            .ok_or("GRAPHIT_TEST_MODEL_SERVER must name the compiled normal model server")?,
    );
    if !model.is_absolute() || !model.is_file() {
        return Err("invalid real model server path".into());
    }
    let mut task_model_paths = vec![model.parent().ok_or("model server parent")?.to_owned()];
    task_model_paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut command = f.command();
    command
        .env("PATH", std::env::join_paths(task_model_paths)?)
        .env("GRAPHIT_MODULES__AST", "true")
        .args(["daemon", "foreground", "--project-version", "main"]);
    Ok(command)
}
async fn wait_main_nn(f: &Fixture) -> Result {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let ast = f
            .command()
            .env("GRAPHIT_MODULES__AST", "true")
            .args([
                "ast",
                "search",
                "published_nn",
                "--mode",
                "semantic",
                "--project-version",
                "main",
            ])
            .output()?;
        let knowledge = f
            .command()
            .env("GRAPHIT_MODULES__AST", "true")
            .args([
                "knowledge",
                "search",
                "exportableword",
                "--mode",
                "semantic",
                "--project-version",
                "main",
            ])
            .output()?;
        let ast_payload = serde_json::from_slice::<serde_json::Value>(&ast.stdout).ok();
        let knowledge_payload = serde_json::from_slice::<serde_json::Value>(&knowledge.stdout).ok();
        if ast.status.success()
            && knowledge.status.success()
            && ast_payload.as_ref().is_some_and(|payload| {
                payload["status"] == "available"
                    && payload["results"]
                        .as_array()
                        .is_some_and(|hits| hits.iter().any(|hit| hit["path"] == "model.rs"))
            })
            && knowledge_payload
                .as_ref()
                .is_some_and(|payload| payload["native"]["state"] == "hits")
        {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(io::Error::other(format!(
                "actual Main Source/NN not ready: AST payload {ast_payload:?}, error {}; Knowledge payload {knowledge_payload:?}, error {}; daemon: {}",
                String::from_utf8_lossy(&ast.stderr),
                String::from_utf8_lossy(&knowledge.stderr),
                fs::read_to_string(f._temp.path().join("daemon.log"))
                    .unwrap_or_else(|error| format!("unavailable: {error}"))
            ))
            .into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
async fn assert_remote_nn(f: &Fixture, retained_versions: &[&str]) -> Result<serde_json::Value> {
    for &version in retained_versions {
        let expected = if version == "release/one" {
            AST_ONE
        } else {
            AST_TWO
        };
        let context = format!("{ARTIFACT}@{version}");
        let old = f
            .command()
            .current_dir(f.paths.root())
            .env("GRAPHIT_MODULES__AST", "true")
            .args(["ast", "source", "same", "--context", &context])
            .output()?;
        if !old.status.success() || old.stdout != expected.as_bytes() {
            return Err(io::Error::other(format!(
                "retained AST version {version}: {}",
                String::from_utf8_lossy(&old.stderr)
            ))
            .into());
        }
        let old = f
            .command()
            .current_dir(f.paths.root())
            .args([
                "knowledge",
                "search",
                "exportableword",
                "--artifact-id",
                KNOWLEDGE,
                "--artifact-version",
                version,
                "--mode",
                "fts",
                "--preview",
            ])
            .output()?;
        let payload: serde_json::Value = serde_json::from_slice(&old.stdout)?;
        let expected = if version == "release/one" {
            KNOWLEDGE_ONE
        } else {
            KNOWLEDGE_TWO
        };
        if !old.status.success()
            || !payload["native"]["hits"].as_array().is_some_and(|hits| {
                hits.iter().any(|hit| {
                    hit["preview"]["text"]
                        .as_str()
                        .is_some_and(|text| text.contains(expected))
                })
            })
        {
            return Err(io::Error::other(format!(
                "retained Knowledge version {version}: {}",
                String::from_utf8_lossy(&old.stderr)
            ))
            .into());
        }
    }

    let context = format!("{ARTIFACT}@release/nn");
    let output = f
        .command()
        .current_dir(f.paths.root())
        .env("GRAPHIT_MODULES__AST", "true")
        .args(["ast", "source", "model.rs", "--context", &context])
        .output()?;
    if !output.status.success() || output.stdout != NN_SOURCE.as_bytes() {
        return Err(io::Error::other(format!(
            "remote NN graph Raw source: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let mut observed_generation = None;
    for mode in ["semantic", "hybrid"] {
        let ast = f
            .command()
            .current_dir(f.paths.root())
            .env("GRAPHIT_MODULES__AST", "true")
            .args([
                "ast",
                "search",
                "published_nn",
                "--context",
                &context,
                "--mode",
                mode,
            ])
            .output()?;
        if !ast.status.success() {
            return Err(io::Error::other(String::from_utf8_lossy(&ast.stderr).into_owned()).into());
        }
        let payload: serde_json::Value = serde_json::from_slice(&ast.stdout)?;
        if payload["status"] != "available"
            || !payload["results"]
                .as_array()
                .is_some_and(|hits| hits.iter().any(|hit| hit["path"] == "model.rs"))
        {
            return Err(io::Error::other(format!("remote AST {mode}: {payload}")).into());
        }
        let knowledge = f
            .command()
            .current_dir(f.paths.root())
            .args([
                "knowledge",
                "search",
                "exportableword",
                "--artifact-id",
                KNOWLEDGE,
                "--artifact-version",
                "release/nn",
                "--mode",
                mode,
                "--preview",
            ])
            .output()?;
        if !knowledge.status.success() {
            return Err(
                io::Error::other(String::from_utf8_lossy(&knowledge.stderr).into_owned()).into(),
            );
        }
        let payload: serde_json::Value = serde_json::from_slice(&knowledge.stdout)?;
        let generation = &payload["native"]["semantic_generation"];
        if generation["version"]
            .as_u64()
            .is_none_or(|version| version == 0)
            || !generation["physical_branch"].is_null()
            || !generation["inherited_target"].is_null()
            || observed_generation
                .as_ref()
                .is_some_and(|observed| observed != generation)
        {
            return Err(io::Error::other(format!(
                "remote Knowledge NN generation {mode}: {payload}"
            ))
            .into());
        }
        observed_generation = Some(generation.clone());
        if payload["native"]["state"] != "hits"
            || !payload["native"]["hits"].as_array().is_some_and(|hits| {
                hits.iter().any(|hit| {
                    hit["preview"]["text"]
                        .as_str()
                        .is_some_and(|text| text.contains("exportableword"))
                })
            })
        {
            return Err(io::Error::other(format!("remote Knowledge {mode}: {payload}")).into());
        }
    }
    observed_generation.ok_or_else(|| "missing observed NN generation".into())
}
async fn main_nn_journey(
    f: &Fixture,
    broker: &BrokerFixture,
    retained_versions: &[&str],
) -> Result {
    install_cpu_model(f).await?;
    install_active_profile(f, &broker.ready, true)?;
    fs::write(f.projects[0].root().join("model.rs"), NN_SOURCE)?;
    let daemon = f.daemon_command(model_daemon_command(f)?).await?;
    let result: Result<serde_json::Value> = async {
        wait_main_nn(f).await?;
        let ast = publish_command(f, "ast", ARTIFACT, "release/nn")?;
        let knowledge = publish_command(f, "knowledge", KNOWLEDGE, "release/nn")?;
        if ast["status"] != "published" || knowledge["status"] != "published" {
            return Err("actual NN publication did not complete".into());
        }
        install_remote_versions(f, &[&ast])?;
        install_remote_versions(f, &[&knowledge])?;
        assert_remote_nn(f, retained_versions).await
    }
    .await;
    let stopped = daemon.stop("-TERM").await;
    match (result, stopped) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; Main NN stop failed: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Ok(generation), Ok(())) => main_disabled_embedding_journey(f, broker, &generation).await,
    }
}

async fn main_disabled_embedding_journey(
    f: &Fixture,
    broker: &BrokerFixture,
    generation: &serde_json::Value,
) -> Result {
    install_active_profile(f, &broker.ready, false)?;
    let daemon = f.daemon_command(model_daemon_command(f)?).await?;
    let result: Result = async {
        for (mode, state) in [("semantic", "unavailable"), ("hybrid", "partial")] {
            let output = f
                .command()
                .current_dir(f.paths.root())
                .args([
                    "knowledge",
                    "search",
                    "exportableword",
                    "--artifact-id",
                    KNOWLEDGE,
                    "--artifact-version",
                    "release/nn",
                    "--mode",
                    mode,
                    "--preview",
                ])
                .output()?;
            if !output.status.success() {
                return Err(
                    io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()).into(),
                );
            }
            let payload: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            let native = &payload["native"];
            if native["state"] != state
                || native["reason"] != "model_unavailable"
                || &native["semantic_generation"] != generation
            {
                return Err(io::Error::other(format!(
                    "published NN with embedding disabled {mode}: {payload}"
                ))
                .into());
            }
            let hits = native["hits"].as_array().ok_or("missing native hits")?;
            if mode == "semantic" {
                if native["total"] != 0 || !hits.is_empty() {
                    return Err("semantic without model returned hits".into());
                }
            } else if native["total"] != 1
                || !hits.iter().any(|hit| {
                    hit["preview"]["text"]
                        .as_str()
                        .is_some_and(|text| text.contains("exportableword"))
                })
            {
                return Err(
                    io::Error::other(format!("hybrid without model lost FTS: {payload}")).into(),
                );
            }
        }
        Ok(())
    }
    .await;
    let stopped = daemon.stop("-TERM").await;
    match (result, stopped) {
        (Err(primary), Err(cleanup)) => Err(io::Error::other(format!(
            "{primary}; embedding-disabled Main stop failed: {cleanup}"
        ))
        .into()),
        (Err(primary), _) => Err(primary),
        (Ok(()), stopped) => stopped,
    }
}

fn joined(primary: Result, cleanup: Result) -> Result {
    match (primary, cleanup) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; cleanup: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), cleanup) => cleanup,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built real Broker STSv3/MinIO fixture"]
async fn compiled_native_update_all_replaces_selected_latest_and_preserves_other_artifacts()
-> Result {
    let f = Fixture::new().await?;
    fs::write(
        f.projects[0].root().join("initial.rs"),
        "pub fn initial() {}\n",
    )?;
    let broker = BrokerFixture::start(&f).await?;
    if let Err(error) = install_active_profile(&f, &broker.ready, false) {
        return joined(Err(error), broker.shutdown().await);
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
        Err(error) => return joined(Err(error), broker.shutdown().await),
    };
    let result: Result = async {
        // MCP discovery precedes the bootstrap index publication. Let that writer
        // finish before this fixture deliberately replaces its AST snapshot.
        wait_productive_ast(&f).await?;
        publish_project_payload(&f, "update-native-one", AST_ONE).await?;
        let first = publish_command(&f, "ast", "fixture/native-update-a", "release/one")?;
        install_remote_versions(&f, &[&first])?;
        publish_project_payload(&f, "update-native-two", AST_TWO).await?;
        let second = publish_command(&f, "ast", "fixture/native-update-a", "release/two")?;
        let sibling = publish_command(&f, "ast", "fixture/native-update-b", "release/one")?;
        install_remote_versions(&f, &[&sibling])?;
        let config = ConfigSnapshot::new(
            f.paths.brand(),
            [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
        )?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
        let registry = graphit_hub::registry::HubRegistry::global(
            &f.paths,
            identity.unit_id().ok_or("installation identity")?,
            policy.hub(),
        )?;
        let one = ConcreteVersion::new("release/one", policy.projection())?;
        let two = ConcreteVersion::new("release/two", policy.projection())?;
        let before = registry.load()?;
        let baseline_count = before.entries().count();
        let unselected: Vec<_> = before
            .entries()
            .filter(|(_, id, _)| {
                !matches!(*id, "fixture/native-update-a" | "fixture/native-update-b")
            })
            .map(|(kind, id, artifact)| (kind, id.to_owned(), artifact.clone()))
            .collect();
        let sibling_before = before
            .get_version(ArtifactType::Ast, "fixture/native-update-b", &one)
            .cloned()
            .ok_or("sibling Native installation")?;
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args(["hub", "update", "--global"])
            .output()?;
        assert!(
            output.status.success(),
            "Native collective update failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let updated: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(updated["status"], "ok");
        let items = updated["items"]
            .as_array()
            .ok_or("Native collective items")?;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["id"], "fixture/native-update-a");
        assert_eq!(items[0]["outcome"]["version"], "release/two");
        assert_eq!(items[0]["status"], "ok");
        assert_eq!(items[1]["status"], "ok");
        let after = registry.load()?;
        assert!(
            after
                .get_version(ArtifactType::Ast, "fixture/native-update-a", &one)
                .is_none()
        );
        let selected = after
            .get_version(ArtifactType::Ast, "fixture/native-update-a", &two)
            .ok_or("updated Native installation")?;
        assert_eq!(selected.installations.len(), 1);
        assert_eq!(
            selected.installations[0].target,
            graphit_hub::InstallationTarget::GlobalNative
        );
        let graphit_hub::registry::InstalledBackend::RemoteNative(record) =
            &selected.installations[0].backend
        else {
            return Err("Native update used a FILE backend".into());
        };
        assert_eq!(record.root_digest(), &root_digest(&second)?);
        assert_eq!(
            after.get_version(ArtifactType::Ast, "fixture/native-update-b", &one),
            Some(&sibling_before)
        );
        assert_eq!(after.entries().count(), baseline_count);
        for (kind, id, artifact) in &unselected {
            assert_eq!(after.get(*kind, id), Some(artifact));
        }
        // All concrete selections are reported, including an already installed latest.
        install_remote_versions(&f, &[&first])?;
        assert_eq!(registry.load()?.entries().count(), baseline_count + 1);
        let output = f
            .command()
            .current_dir(f.paths.root())
            .args(["hub", "update", "--global"])
            .output()?;
        assert!(
            output.status.success(),
            "Native convergence failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let converged: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let items = converged["items"]
            .as_array()
            .ok_or("Native convergence items")?;
        assert_eq!(items.len(), 3);
        assert!(items.iter().all(|item| item["status"] == "ok"));
        let current = registry.load()?;
        assert!(
            current
                .get_version(ArtifactType::Ast, "fixture/native-update-a", &one)
                .is_none()
        );
        assert_eq!(
            current.get_version(ArtifactType::Ast, "fixture/native-update-a", &two),
            Some(selected)
        );
        assert_eq!(
            current.get_version(ArtifactType::Ast, "fixture/native-update-b", &one),
            Some(&sibling_before)
        );
        assert_eq!(current.entries().count(), baseline_count);
        for (kind, id, artifact) in &unselected {
            assert_eq!(current.get(*kind, id), Some(artifact));
        }
        Ok(())
    }
    .await;
    let (daemon_cleanup, broker_cleanup) = tokio::join!(daemon.stop("-TERM"), broker.shutdown());
    joined(result, joined(daemon_cleanup, broker_cleanup))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built real Broker STSv3/MinIO fixture, Gemma BF16 and model server; cargo xtask test graphit-app/native-hub-publication"]
async fn compiled_hub_publish_native_uses_broker_sts_and_preserves_versions() -> Result {
    // A missing fixture is a failed precondition when this explicit selector runs.
    std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .ok_or("missing real Broker fixture executable")?;
    std::env::var_os("GRAPHIT_EMBEDDINGGEMMA2_MODEL")
        .ok_or("missing real pinned Gemma BF16 model")?;
    std::env::var_os("GRAPHIT_TEST_MODEL_SERVER").ok_or("missing compiled real model server")?;
    let f = Fixture::new().await?;
    fs::write(
        f.projects[0].root().join("initial.rs"),
        "pub fn initial() {}\n",
    )?;
    fs::create_dir(f.projects[0].root().join("docs"))?;
    fs::write(
        f.projects[0].root().join("docs/published.md"),
        format!(
            "---\nid: published-note\ntitle: Published Knowledge\n---\n# Published Knowledge\n{KNOWLEDGE_ONE}\n"
        ),
    )?;
    let broker = BrokerFixture::start(&f).await?;
    let setup = install_active_profile(&f, &broker.ready, false);
    if let Err(error) = setup {
        let cleanup = broker.shutdown().await;
        return match cleanup {
            Ok(()) => Err(error),
            Err(cleanup) => {
                Err(io::Error::other(format!("{error}; Broker shutdown failed: {cleanup}")).into())
            }
        };
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
        Err(error) => {
            let cleanup = broker.shutdown().await;
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup) => Err(io::Error::other(format!(
                    "{error}; Broker shutdown failed: {cleanup}"
                ))
                .into()),
            };
        }
    };
    let result: Result = async {
        wait_productive_sources(&f,KNOWLEDGE_ONE).await?;

        let denied = f.command().env("GRAPHIT_MODULES__AST", "true")
            .args(["hub", "publish-native", "--kind", "ast", "--provider", PROVIDER,
                "--id", "fixture/denied", "--version", "release/denied", "--name", "Denied", "--project-dir"])
            .arg(f.projects[1].root()).output()?;
        if denied.status.success() || !denied.stdout.is_empty()
            || !String::from_utf8_lossy(&denied.stderr).contains("authentication_or_permission_denied") {
            return Err(io::Error::other(format!("foreign Hub producer was not denied causally: {}", String::from_utf8_lossy(&denied.stderr))).into());
        }

        publish_project_payload(&f, "producer-one", AST_ONE).await?;
        let first = publish_command(&f, "ast", ARTIFACT, "release/one")?;
        if first["status"] != "published" { return Err(io::Error::other(format!("first publication: {first}")).into()); }
        let duplicate = publish_command(&f, "ast", ARTIFACT, "release/one")?;
        if first["status"] != duplicate["status"] || first["publication"] != duplicate["publication"] {
            return Err("same captured release was not idempotent".into());
        }
        if first["project_metadata"] != "published" || duplicate["project_metadata"] != "unchanged" {
            return Err("metadata create and unchanged effects were not distinguished".into());
        }
        publish_project_payload(&f, "producer-two", AST_TWO).await?;
        let second = publish_command(&f, "ast", ARTIFACT, "release/two")?;
        if second["status"] != "published" { return Err(io::Error::other(format!("second publication: {second}")).into()); }
        install_remote_versions(&f, &[&first, &second])?;
        eprintln!("native publication: both AST releases published and installed; publishing Knowledge versions");
        let knowledge_first = publish_command(&f, "knowledge", KNOWLEDGE, "release/one")?;
        fs::write(f.projects[0].root().join("docs/published.md"),format!("---\nid: published-note\ntitle: Published Knowledge\n---\n# Published Knowledge\n{KNOWLEDGE_TWO}\n"))?;
        wait_productive_sources(&f,KNOWLEDGE_TWO).await?;
        let knowledge_second = publish_command(&f, "knowledge", KNOWLEDGE, "release/two")?;
        if knowledge_first["status"] != "published" || knowledge_second["status"] != "published" { return Err("Knowledge retained publications did not complete".into()); }
        install_remote_versions(&f, &[&knowledge_first, &knowledge_second])?;
        assert_remote_consumers(&f).await?;
        let producer = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .unwrap()
            .id();
        let store = broker.store(producer).await?;
        let config = ConfigSnapshot::new(
            f.paths.brand(),
            [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
        )?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let artifact = ArtifactId::new(ARTIFACT, policy.projection())?;
        let registry = format!(
            "{}/{}",
            broker.ready.prefix,
            graphit_hub::remote_storage::registry_key(producer, ArtifactType::Ast, &artifact)?
        );
        let catalog = store
            .get(&object_store::path::Path::parse(registry)?)
            .await?
            .bytes()
            .await?;
        let catalog: serde_json::Value = serde_json::from_slice(&catalog)?;
        let versions = catalog["versions"]
            .as_array()
            .ok_or("actual registry versions")?;
        if versions.len() != 2 { return Err("actual catalog did not preserve both versions".into()); }
        for version in ["release/one", "release/two"] {
            if !versions.iter().any(|entry| entry["version"] == version) { return Err(io::Error::other(format!("catalog missing retained version {version}")).into()); }
            let version = ConcreteVersion::new(version, policy.projection())?;
            let receipt = if version.as_str() == "release/one" {
                &first
            } else {
                &second
            };
            let digest = root_digest(receipt)?;
            let root = format!(
                "{}/{}",
                broker.ready.prefix,
                graphit_hub::remote_storage::release_prefix(
                    producer,
                    ArtifactType::Ast,
                    &artifact,
                    &version,
                    &digest
                )?
            );
            let descriptor = store
                .get(&object_store::path::Path::parse(format!(
                    "{root}native.json"
                ))?)
                .await?
                .bytes()
                .await?;
            let descriptor: serde_json::Value = serde_json::from_slice(&descriptor)?;
            let components = descriptor["components"]
                .as_array()
                .ok_or("native components")?;
            if !components.iter().any(|component| component["role"] == "graph") { return Err("native descriptor missing graph component".into()); }
            if !components.iter().any(|component| component["role"] == "source") { return Err("native descriptor missing source component".into()); }
            if !components.iter().any(|component| component["role"] == "entity_search") { return Err("native descriptor missing entity_search component".into()); }
            if version.as_str() == "release/one" {
                // A hostile catalog points at bytes under a mismatched descriptor digest.
                // Use the same issued producer STS, without changing immutable good releases.
                let rejected_id = "fixture/rejected-native";
                let rejected_version = ConcreteVersion::new("release/invalid", policy.projection())?;
                let rejected_artifact = ArtifactId::new(rejected_id, policy.projection())?;
                let mut invalid_root = descriptor.clone();
                invalid_root["id"] = json!(rejected_id);
                invalid_root["version"] = json!(rejected_version.as_str());
                let invalid_bytes = serde_json::to_vec(&invalid_root)?;
                let mut wrong_digest = [0u8;32];
                if <[u8;32]>::from(Sha256::digest(&invalid_bytes)) == wrong_digest { wrong_digest[0] = 1; }
                let invalid_release = graphit_hub::remote_storage::release_prefix(
                    producer, ArtifactType::Ast, &rejected_artifact, &rejected_version, &wrong_digest,
                )?;
                store.put_opts(&object_store::path::Path::parse(format!("{}/{invalid_release}native.json", broker.ready.prefix))?,
                    invalid_bytes.into(), object_store::PutOptions { mode: object_store::PutMode::Create, ..Default::default() }).await?;
                let mut invalid_catalog = catalog.clone();
                invalid_catalog["id"] = json!(rejected_id);
                invalid_catalog["latest"] = json!(rejected_version.as_str());
                invalid_catalog["versions"] = json!([{ "version":rejected_version.as_str(),
                    "hash":wrong_digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>() }]);
                store.put_opts(&object_store::path::Path::parse(format!("{}/{}", broker.ready.prefix,
                    graphit_hub::remote_storage::registry_key(producer, ArtifactType::Ast, &rejected_artifact)?))?,
                    serde_json::to_vec(&invalid_catalog)?.into(), object_store::PutOptions { mode: object_store::PutMode::Create, ..Default::default() }).await?;
                let identity = InstallationIdentityStore::new(&f.paths)?.load()?;
                let installed_registry = graphit_hub::registry::HubRegistry::global(&f.paths,
                    identity.unit_id().ok_or("missing installation")?, policy.hub())?;
                let before = fs::read(installed_registry.path())?;
                let rejected = f.command().current_dir(f.paths.root()).args([
                    "hub", "install-native", "--provider", PROVIDER, "--producer", &producer.to_string(),
                    "--kind", "ast", "--id", rejected_id, "--version", rejected_version.as_str(),
                ]).output()?;
                if rejected.status.success() || !rejected.stdout.is_empty()
                    || !String::from_utf8_lossy(&rejected.stderr).contains("invalid_remote_root_or_manifest")
                    || fs::read(installed_registry.path())? != before {
                    return Err(io::Error::other(format!("invalid remote descriptor affected installation: {}; {}",
                        rejected.status, String::from_utf8_lossy(&rejected.stderr))).into());
                }
            }

        }
        Ok(())
    }
    .await;
    let stopped = daemon.stop("-TERM").await;
    let result = match (result, stopped) {
        (Err(primary), Err(cleanup)) => Err(io::Error::other(format!(
            "{primary}; Main SourceOnly stop failed: {cleanup}"
        ))
        .into()),
        (Err(primary), _) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(cleanup),
        (Ok(()), Ok(())) => main_nn_journey(&f, &broker, &["release/one", "release/two"]).await,
    };
    let shutdown = broker.shutdown().await;
    match (result, shutdown) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; Broker shutdown failed: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), shutdown) => shutdown,
    }
}

fn ast_daemon_without_compiler(f: &Fixture) -> Result<Command> {
    let git_path = f._temp.path().join("git-only-bin");
    fs::create_dir(&git_path)?;
    std::os::unix::fs::symlink(which::which("git")?, git_path.join("git"))?;
    let mut command = f.command();
    command
        .env("PATH", git_path)
        .env("GRAPHIT_MODULES__AST", "true")
        .args(["daemon", "foreground", "--project-version", "main"]);
    Ok(command)
}

/// The same productive AST prerequisite, isolated from remote Hub startup and shutdown.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_native_ast_initial_source_publishes_before_remote_hub() -> Result {
    let f = Fixture::new().await?;
    fs::write(
        f.projects[0].root().join("initial.rs"),
        "pub fn initial() {}\n",
    )?;
    let cancel = Cancellation::default();
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
            &cancel,
        )?;
    }
    let daemon = f.daemon_command(ast_daemon_without_compiler(&f)?).await?;
    let result: Result = async {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let graph = f.command().env("GRAPHIT_MODULES__AST", "true")
                .args(["ast", "schema", "--project-version", "main"]).output()?;
            if graph.status.success() {
                let schema:serde_json::Value=serde_json::from_slice(&graph.stdout)?;
                if schema["nodes"].as_array().is_some_and(|nodes|!nodes.is_empty()) { return Ok(()); }
            }
            if Instant::now() >= until {
                return Err(io::Error::other(format!("productive AST schema unavailable: status {}; stderr {}; payload {}; daemon {}",
                    graph.status,String::from_utf8_lossy(&graph.stderr),String::from_utf8_lossy(&graph.stdout),
                    fs::read_to_string(f._temp.path().join("daemon.log")).unwrap_or_else(|cause|cause.to_string()))).into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }.await;
    let stopped = daemon.stop("-TERM").await;
    match (result, stopped) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; Main stop failed: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), stopped) => stopped,
    }
}

/// Real Source watcher update, independent of remote publication and Broker startup.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_native_knowledge_file_update_reaches_current_fts() -> Result {
    let f = Fixture::new().await?;
    fs::write(
        f.projects[0].root().join("initial.rs"),
        "pub fn initial() {}\n",
    )?;
    fs::create_dir(f.projects[0].root().join("docs"))?;
    let document = f.projects[0].root().join("docs/published.md");
    let body = |text: &str| {
        format!(
            "---\nid: published-note\ntitle: Published Knowledge\n---\n# Published Knowledge\n{text}\n"
        )
    };
    fs::write(&document, body(KNOWLEDGE_ONE))?;
    let cancel = Cancellation::default();
    for module in ["agents", "hooks", "memory", "embedding", "dream"] {
        ConfigStore::global(&f.paths)?.set(
            ConfigKey::new(&format!("modules.{module}"))?,
            json!(false),
            Duration::from_secs(5),
            &cancel,
        )?;
    }
    ConfigStore::global(&f.paths)?.set(
        ConfigKey::new("modules.knowledge")?,
        json!(true),
        Duration::from_secs(5),
        &cancel,
    )?;
    let daemon = f.daemon_command(ast_daemon_without_compiler(&f)?).await?;
    let result: Result = async {
        wait_productive_sources(&f, KNOWLEDGE_ONE).await?;
        fs::write(&document, body(KNOWLEDGE_TWO))?;
        wait_productive_sources(&f, KNOWLEDGE_TWO).await
    }
    .await;
    let stopped = daemon.stop("-TERM").await;
    match (result, stopped) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; Main stop failed: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), stopped) => stopped,
    }
}

/// Real local-model admission, native NN publication and remote degraded reads.
/// SourceOnly retained versions are covered by the full publication journey.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built real Broker STSv3/MinIO fixture, real Gemma BF16 and model server"]
async fn compiled_hub_publish_native_nn_and_degraded_consumers() -> Result {
    for key in [
        "GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN",
        "GRAPHIT_EMBEDDINGGEMMA2_MODEL",
        "GRAPHIT_TEST_MODEL_SERVER",
    ] {
        std::env::var_os(key).ok_or_else(|| format!("missing required fixture {key}"))?;
    }
    let f = Fixture::new().await?;
    fs::create_dir(f.projects[0].root().join("docs"))?;
    fs::write(
        f.projects[0].root().join("docs/published.md"),
        format!(
            "---\nid: published-note\ntitle: Published Knowledge\n---\n# Published Knowledge\n{KNOWLEDGE_TWO}\n"
        ),
    )?;
    let broker = BrokerFixture::start(&f).await?;
    let result = main_nn_journey(&f, &broker, &[]).await;
    let shutdown = broker.shutdown().await;
    match (result, shutdown) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; Broker shutdown failed: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(()), shutdown) => shutdown,
    }
}

async fn mcp_call(
    f: &Fixture,
    tool: &str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<rmcp::model::CallToolResult> {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
    command
        .env_clear()
        .env("HOME", f._temp.path())
        .env(
            f.paths.brand().environment_variable("GLOBAL_DIR")?,
            f.paths.root(),
        )
        .current_dir(f.projects[0].root())
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
            let cleanup = child.kill().await;
            let _ = child.wait().await;
            return Err(io::Error::other(format!(
                "MCP handshake failed: {cause}; child cleanup: {cleanup:?}"
            ))
            .into());
        }
    };
    let result: Result<_> = async {
        let tools = sdk.peer().list_tools(Default::default()).await?;
        if !tools.tools.iter().any(|candidate| candidate.name == tool) {
            return Err(
                io::Error::other(format!("{tool} missing from actual MCP advertisement")).into(),
            );
        }
        Ok(sdk
            .peer()
            .call_tool(
                CallToolRequestParams::new(tool.to_owned())
                    .with_arguments(json_arguments(arguments)),
            )
            .await?)
    }
    .await;
    let cancelled = sdk.cancel().await;
    let joined = match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
        Ok(joined) => joined,
        Err(timeout) => {
            let killed = child.kill().await;
            let reaped = child.wait().await;
            Err(io::Error::other(format!(
                "MCP join deadline: {timeout}; kill: {killed:?}; reap: {reaped:?}"
            )))
        }
    };
    let cleanup: Result = async {
        cancelled?;
        let status = joined?;
        if !status.success() {
            return Err("MCP process exited unsuccessfully".into());
        }
        Ok(())
    }
    .await;
    match (result, cleanup) {
        (Err(primary), Err(cleanup)) => {
            Err(io::Error::other(format!("{primary}; MCP cleanup: {cleanup}")).into())
        }
        (Err(primary), _) => Err(primary),
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(cleanup)) => Err(cleanup),
    }
}

#[path = "native_publication/file_installation.rs"]
mod file_installation;

#[path = "native_publication/file_publication.rs"]
mod file_publication;

#[path = "native_publication/project_metadata.rs"]
mod project_metadata;

#[path = "native_publication/file_lifecycle.rs"]
mod file_lifecycle;
#[path = "native_publication/global_lifecycle.rs"]
mod global_lifecycle;
#[path = "native_publication/remote_artifacts.rs"]
mod remote_artifacts;

#[path = "native_publication/local_file_install.rs"]
mod local_file_install;

#[path = "native_publication/hub_unpublish.rs"]
mod hub_unpublish;
