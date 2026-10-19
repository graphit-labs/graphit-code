//! Current Main → Source FILE handler → real Broker STS → S3 → native projection.
use super::*;
use graphit_hub::{
    remote_metadata::{ManifestInput, MetadataLimits, RemoteArtifactManifest, VersionInput},
    remote_storage::{registry_key, release_prefix},
};
use std::io::{Cursor, Write as _};

fn payload(body: &str) -> Result<Vec<u8>> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);
    writer.start_file("SKILL.md", options)?;
    writer.write_all(
        format!("---\nname: file-skill\ndescription: Remote FILE consumer fixture\n---\n{body}\n")
            .as_bytes(),
    )?;
    writer.start_file("assets/literal.txt", options)?;
    writer.write_all(body.as_bytes())?;
    Ok(writer.finish()?.into_inner())
}
fn object_key(prefix: &str, relative: &str) -> object_store::path::Path {
    let prefix = prefix.trim_end_matches('/');
    object_store::path::Path::from(if prefix.is_empty() {
        relative.into()
    } else {
        format!("{prefix}/{relative}")
    })
}
fn install_file(
    fixture: &Fixture,
    producer: graphit_core::project_identity::ProjectId,
    version: &str,
) -> Result<serde_json::Value> {
    let output = fixture
        .command()
        .args([
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
            "fixture/file-skill",
            "--version",
            version,
            "--binding",
            "codex",
            "--alias",
            "installed-file",
        ])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "FILE install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_hub_file_install_uses_real_broker_and_verified_cache() -> Result {
    let fixture = Fixture::new().await?;
    let broker = BrokerFixture::start(&fixture).await?;
    let setup = install_active_profile(&fixture, &broker.ready, false);
    if let Err(error) = setup {
        broker.shutdown().await?;
        return Err(error);
    }
    let daemon = match fixture.daemon(&["--project-version", "main"]).await {
        Ok(daemon) => daemon,
        Err(error) => {
            broker.shutdown().await?;
            return Err(error);
        }
    };
    let result: Result = async {
        let producer = ProjectIdentityStore::at(&fixture.projects[0])
            .load()?
            .ok_or("missing producer")?
            .id();
        let other = ProjectIdentityStore::at(&fixture.projects[1])
            .load()?
            .ok_or("missing sibling")?
            .id();
        let denied = fixture
            .command()
            .args([
                "--project-version",
                "main",
                "hub",
                "install",
                "--provider",
                PROVIDER,
                "--producer",
                &other.to_string(),
                "--kind",
                "skill",
                "--id",
                "fixture/file-skill",
                "--version",
                "release/one",
            ])
            .output()?;
        if denied.status.success()
            || !String::from_utf8_lossy(&denied.stderr)
                .contains("authentication_or_permission_denied")
        {
            return Err(
                "foreign remote project must be denied before payload/cache publication".into(),
            );
        }
        let store = broker.store(producer).await?;
        let config = ConfigSnapshot::new(fixture.paths.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel)?;
        let id = ArtifactId::new("fixture/file-skill", policy.projection())?;
        let version = ConcreteVersion::new("release/one", policy.projection())?;
        let bytes = payload("file_payload_one")?;
        let hash = Sha256::digest(&bytes);
        let digest = hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let key = object_key(
            &broker.ready.prefix,
            &format!(
                "{}payload.zip",
                release_prefix(producer, ArtifactType::Skill, &id, &version, &hash.into())?
            ),
        );
        store.put(&key, bytes.into()).await?;
        let content = policy.content();
        let metadata = RemoteArtifactManifest::encode(
            &ManifestInput {
                producer,
                artifact_type: ArtifactType::Skill,
                id: id.as_str(),
                name: "Remote FILE fixture",
                description: None,
                tags: &[],
                author: None,
                latest: Some(version.as_str()),
                versions: &[VersionInput {
                    version: version.as_str(),
                    hash: &digest,
                }],
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
                    &registry_key(producer, ArtifactType::Skill, &id)?,
                ),
                metadata.into(),
            )
            .await?;
        let first = install_file(&fixture, producer, version.as_str())?;
        if first["status"] != "ok" || first["version"] != version.as_str() {
            return Err(io::Error::other(format!("invalid FILE install outcome: {first}")).into());
        }
        let installed = fixture.projects[0]
            .root()
            .join(".agents/skills/installed-file");
        let text = fs::read_to_string(installed.join("SKILL.md"))?;
        if !text.contains("name: installed-file")
            || !text.contains("file_payload_one")
            || fs::read_to_string(installed.join("assets/literal.txt"))? != "file_payload_one"
        {
            return Err("native skill projection lost alias rendering or companion content".into());
        }
        // The actual STS publisher grant intentionally has no DeleteObject authority.
        // An authorized overwrite makes an unwanted GET fail the unchanged catalog digest.
        store
            .put(
                &key,
                b"invalid remote archive after verified installation"
                    .to_vec()
                    .into(),
            )
            .await?;
        let second = install_file(&fixture, producer, version.as_str())?;
        if second["status"] != "ok" || second["cache"] != "Unchanged" {
            return Err(io::Error::other(format!(
                "verified owned cache was not reused after remote payload corruption: {second}"
            ))
            .into());
        }
        let registry = graphit_hub::registry::HubRegistry::project(
            &fixture.projects[0],
            &ProjectIdentityStore::at(&fixture.projects[0])
                .load()?
                .ok_or("missing identity")?,
            policy.hub(),
        )?
        .load()?;
        let installed = registry
            .get(ArtifactType::Skill, id.as_str())
            .ok_or("missing FILE registry entry")?;
        if !installed.installations.iter().any(|entry| {
            entry.original_source.remote_file_origin()
                == Some(&graphit_hub::RemoteFileOrigin::Broker {
                    provider: PROVIDER.into(),
                })
        }) {
            return Err("installed FILE lost its bound Broker origin".into());
        }
        Ok(())
    }
    .await;
    let daemon_cleanup = daemon.stop("-TERM").await;
    let broker_cleanup = broker.shutdown().await;
    let cleanup = match (daemon_cleanup, broker_cleanup) {
        (Err(daemon), Err(broker)) => Err(io::Error::other(format!(
            "daemon shutdown failed: {daemon}; Broker shutdown failed: {broker}"
        ))
        .into()),
        (Err(cause), _) | (_, Err(cause)) => Err(cause),
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

#[path = "file_installation/consumer_journeys.rs"]
mod consumer_journeys;

#[path = "file_installation/standalone_journeys.rs"]
pub(super) mod standalone_journeys;
