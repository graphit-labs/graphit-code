//! Compiled CLI/MCP retraction through current Broker STSv3 and conditional S3 writes.
use super::*;
use graphit_core::project_identity::ProjectId;
use graphit_hub::{SourceRef, registry::HubRegistry};
use object_store::{
    aws::{AmazonS3Builder, AwsAuthorizer, S3CopyIfNotExists},
    client::{HttpRequest, HttpRequestBody},
};

const ID: &str = "fixture/web-file-reader";
const ONE: &str = "release/one";
const TWO: &str = "release/two";

fn equal<A: std::fmt::Debug + PartialEq<B>, B: std::fmt::Debug>(
    actual: &A,
    expected: &B,
) -> Result {
    if actual != expected {
        return Err(io::Error::other(format!("expected {expected:?}, got {actual:?}")).into());
    }
    Ok(())
}
fn different<A: std::fmt::Debug + PartialEq<B>, B: std::fmt::Debug>(
    actual: &A,
    unexpected: &B,
) -> Result {
    if actual == unexpected {
        return Err(io::Error::other(format!("unexpected unchanged value: {actual:?}")).into());
    }
    Ok(())
}
fn require(condition: bool) -> Result {
    if !condition {
        return Err("missing expected retraction error evidence".into());
    }
    Ok(())
}

fn publish(
    f: &Fixture,
    producer: ProjectId,
    policy: &HubResourcePolicy,
    version: &str,
    body: &str,
) -> Result<(std::path::PathBuf, String)> {
    let source = SourceRef::resources(
        &f.projects[0],
        producer,
        ArtifactType::Skill,
        ArtifactId::new(ID, policy.projection())?,
        ConcreteVersion::new(version, policy.projection())?,
        policy.projection(),
    )?;
    let root = source.directory().as_path().to_owned();
    fs::create_dir_all(root.join("assets"))?;
    fs::write(
        root.join("SKILL.md"),
        format!("---\nname: web-file-reader\ndescription: Retraction fixture\n---\n{body}\n"),
    )?;
    fs::write(root.join("assets/literal.txt"), body)?;
    let result = cli(
        f,
        &[
            "--project-version",
            "main",
            "hub",
            "publish",
            "--provider",
            PROVIDER,
            "--kind",
            "skill",
            "--id",
            ID,
            "--version",
            version,
            "--name",
            "Retraction fixture",
            "--set-latest",
        ],
        false,
    )?;
    equal(&(result["status"]), &("published"))?;
    equal(&(result["publication"]["catalog"]), &("published"))?;
    let hash = result["publication"]["archive_sha256"]
        .as_str()
        .ok_or("missing published archive hash")?
        .to_owned();
    Ok((root, hash))
}

fn cli(f: &Fixture, args: &[&str], outside_project: bool) -> Result<serde_json::Value> {
    let mut command = f.command();
    if outside_project {
        command.current_dir(f.paths.root());
    }
    let output = command.args(args).output()?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err(io::Error::other(format!(
            "Hub CLI failed: {}; {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn checked_mcp(value: rmcp::model::CallToolResult, operation: &str) -> Result<serde_json::Value> {
    if value.is_error == Some(true) {
        return Err(io::Error::other(format!(
            "{operation} MCP returned a business error: {}",
            value
                .structured_content
                .as_ref()
                .unwrap_or(&serde_json::Value::Null)
        ))
        .into());
    }
    value.structured_content.ok_or_else(|| {
        io::Error::other(format!("{operation} MCP omitted the structured receipt")).into()
    })
}

fn receipt(
    value: &serde_json::Value,
    producer: ProjectId,
    versions: &[&str],
    latest: Option<&str>,
    catalog: &str,
) -> Result {
    equal(&(value["status"]), &("unpublished"))?;
    equal(
        &(value["retraction"]),
        &(json!({
            "producer":producer, "artifact_type":"skill", "id":ID,
            "targeted_versions":versions, "desired_latest":latest, "catalog":catalog
        })),
    )?;
    Ok(())
}

fn payload_key(
    broker: &BrokerFixture,
    producer: ProjectId,
    policy: &HubResourcePolicy,
    version: &str,
    hash: &str,
) -> Result<object_store::path::Path> {
    let mut digest = [0; 32];
    if hash.len() != 64 {
        return Err("invalid published hash".into());
    }
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hash[index * 2..index * 2 + 2], 16)?;
    }
    Ok(object_store::path::Path::parse(format!(
        "{}/{}payload.zip",
        broker.ready.prefix,
        graphit_hub::remote_storage::release_prefix(
            producer,
            ArtifactType::Skill,
            &ArtifactId::new(ID, policy.projection())?,
            &ConcreteVersion::new(version, policy.projection())?,
            &digest,
        )?
    ))?)
}

async fn missing(store: &dyn ObjectStore, key: &object_store::path::Path) -> Result {
    match store.get(key).await {
        Err(object_store::Error::NotFound { .. }) => Ok(()),
        Err(cause) => Err(cause.into()),
        Ok(_) => Err("retracted catalog still exists in actual S3".into()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the real Broker fixture with GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BACKEND=filesystem"]
async fn compiled_hub_unpublish_cli_mcp_preserves_payloads_sources_and_installations() -> Result {
    unpublish_journey(true).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the real Broker fixture with the default MinIO backend"]
async fn compiled_hub_unpublish_minio_refuses_unsafe_catalog_delete() -> Result {
    unpublish_journey(false).await
}

async fn unpublish_journey(conditional_delete: bool) -> Result {
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
    let result: Result =
        async {
            let config = ConfigSnapshot::new(
                f.paths.brand(),
                [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
            )?;
            let policy = HubResourcePolicy::resolve(&config)?;
            let identity = ProjectIdentityStore::at(&f.projects[0])
                .load()?
                .ok_or("missing producer")?;
            let producer = identity.id();
            let producer_text = producer.to_string();
            let (source_one, hash_one) = publish(&f, producer, &policy, ONE, "retained_one")?;
            let (source_two, hash_two) = publish(&f, producer, &policy, TWO, "retained_two")?;
            // Keep the concrete SDK client so the negative DELETE uses its actual STSv3 credentials.
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(30))
                .build()?;
            let storage: serde_json::Value = client
                .post(format!(
                    "{}/v1/s3/credentials",
                    broker.ready.endpoint.trim_end_matches('/')
                ))
                .bearer_auth(&broker.ready.access_bearer)
                .json(&json!({"scope":"project","project_id":producer,"module":"hub"}))
                .send()
                .await
                .map_err(reqwest::Error::without_url)?
                .error_for_status()
                .map_err(reqwest::Error::without_url)?
                .json()
                .await
                .map_err(reqwest::Error::without_url)?;
            // The wire field is required: a missing field must not silently mean unsupported.
            equal(&storage["conditional_delete"], &json!(conditional_delete))?;
            let field = |name: &str| {
                storage[name].as_str().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid actual STSv3 response")
                })
            };
            equal(
                &storage["copy_if_not_exists"],
                &json!(if conditional_delete {
                    "if_none_match"
                } else {
                    "multipart"
                }),
            )?;
            let copy_strategy = match field("copy_if_not_exists")? {
                "multipart" => S3CopyIfNotExists::Multipart,
                "if_none_match" => S3CopyIfNotExists::Header("if-none-match".into(), "*".into()),
                _ => return Err("invalid issued copy strategy".into()),
            };
            let store = Arc::new(
                AmazonS3Builder::new()
                    .with_copy_if_not_exists(copy_strategy)
                    .with_bucket_name(&broker.ready.bucket)
                    .with_region(&broker.ready.region)
                    .with_endpoint(&broker.ready.storage_endpoint)
                    .with_allow_http(true)
                    .with_access_key_id(field("access_key_id")?)
                    .with_secret_access_key(field("secret_access_key")?)
                    .with_token(field("session_token")?)
                    .build()?,
            );
            drop(storage);
            let key = object_store::path::Path::parse(format!(
                "{}/{}",
                broker.ready.prefix,
                graphit_hub::remote_storage::registry_key(
                    producer,
                    ArtifactType::Skill,
                    &ArtifactId::new(ID, policy.projection())?
                )?
            ))?;
            let payload_one = payload_key(&broker, producer, &policy, ONE, &hash_one)?;
            let payload_two = payload_key(&broker, producer, &policy, TWO, &hash_two)?;
            let zip_one = store.get(&payload_one).await?.bytes().await?;
            let target = store.get(&payload_two).await?;
            let target_etag = target.meta.e_tag.clone();
            let zip_two = target.bytes().await?;
            if conditional_delete {
                // Genuine, different immutable releases: Create must not overwrite the destination.
                different(&payload_one, &payload_two)?;
                different(&zip_one, &zip_two)?;
                match store
                    .copy_opts(
                        &payload_one,
                        &payload_two,
                        object_store::CopyOptions::new().with_mode(object_store::CopyMode::Create),
                    )
                    .await
                {
                    Err(object_store::Error::AlreadyExists { .. }) => {}
                    Err(cause) => return Err(cause.into()),
                    Ok(()) => {
                        return Err("immutable Create copy replaced an existing release".into());
                    }
                }
                let unchanged = store.get(&payload_two).await?;
                equal(&unchanged.meta.e_tag, &target_etag)?;
                equal(&unchanged.bytes().await?, &zip_two)?;
            }

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
                    &producer_text,
                    "--kind",
                    "skill",
                    "--id",
                    ID,
                    "--version",
                    ONE,
                    "--binding",
                    "codex",
                    "--alias",
                    "retained-file",
                ],
                false,
            )?;
            equal(&(installed["status"]), &("ok"))?;
            let projected = f.projects[0].root().join(".agents/skills/retained-file");
            let registry = HubRegistry::project(&f.projects[0], &identity, policy.hub())?;
            let document = registry.load()?;
            let original = document
                .get(ArtifactType::Skill, ID)
                .ok_or("missing installed artifact")?
                .installations
                .first()
                .ok_or("missing installed binding")?
                .original_source
                .directory()
                .as_path()
                .to_owned();
            let registry_before = fs::read(registry.path())?;
            let installed_before = fs::read(projected.join("SKILL.md"))?;
            let cache_before = fs::read(original.join("SKILL.md"))?;
            let source_before = fs::read(source_one.join("SKILL.md"))?;
            let source_two_before = fs::read(source_two.join("SKILL.md"))?;

            let before = store.get(&key).await?;
            let etag_before = before.meta.e_tag.clone();
            let catalog_before = before.bytes().await?;

            if conditional_delete {
                // The server must reject a stale ETag itself; expected_sha256 is a separate local check.
                let wrong_etag = "\"00000000000000000000000000000000\"";
                different(&etag_before.as_deref(), &Some(wrong_etag))?;
                let mut delete_url = reqwest::Url::parse(&broker.ready.storage_endpoint)?;
                delete_url
                    .path_segments_mut()
                    .map_err(|_| "invalid fixture storage URL")?
                    .pop_if_empty()
                    .push(&broker.ready.bucket)
                    .extend(key.as_ref().split('/'));
                let mut signed = HttpRequest::new(HttpRequestBody::empty());
                *signed.method_mut() = reqwest::Method::DELETE;
                *signed.uri_mut() = delete_url.as_str().parse()?;
                signed.headers_mut().insert(
                    reqwest::header::IF_MATCH,
                    reqwest::header::HeaderValue::from_static(wrong_etag),
                );
                let credential = store.credentials().get_credential().await?;
                AwsAuthorizer::new(&credential, "s3", &broker.ready.region)
                    .try_authorize(&mut signed, None)?;
                let mut request = reqwest::Request::new(reqwest::Method::DELETE, delete_url);
                *request.headers_mut() = signed.into_parts().0.headers;
                let rejected = client
                    .execute(request)
                    .await
                    .map_err(reqwest::Error::without_url)?;
                equal(
                    &rejected.status(),
                    &reqwest::StatusCode::PRECONDITION_FAILED,
                )?;
                let unchanged = store.get(&key).await?;
                equal(&unchanged.meta.e_tag, &etag_before)?;
                equal(&unchanged.bytes().await?, &catalog_before)?;
            }

            let invalid = f
                .command()
                .current_dir(f.paths.root())
                .args([
                    "hub",
                    "unpublish",
                    "--provider",
                    PROVIDER,
                    "--producer",
                    &producer_text,
                    "--kind",
                    "skill",
                    "--id",
                    ID,
                    "--version",
                    ONE,
                    "--all-versions",
                ])
                .output()?;
            equal(&(invalid.status.code()), &(Some(2)))?;
            require(invalid.stdout.is_empty())?;
            let invalid_mcp = mcp_call(
                &f,
                "graphit_hub_unpublish",
                json!({
                    "provider":PROVIDER, "producer":producer, "artifact_type":"skill", "id":ID,
                    "target":{"kind":"artifact"}, "project_dir":f.projects[0].root()
                })
                .as_object()
                .unwrap()
                .clone(),
            )
            .await?;
            equal(&(invalid_mcp.is_error), &(Some(true)))?;
            let invalid_mcp = invalid_mcp
                .structured_content
                .ok_or("missing invalid selection error")?;
            equal(&(invalid_mcp["code"]), &("invalid_arguments"))?;
            equal(&(invalid_mcp["retraction"]["catalog"]), &("not_attempted"))?;
            let conflict = mcp_call(
                &f,
                "graphit_hub_unpublish",
                json!({
                    "provider":PROVIDER, "producer":producer, "artifact_type":"skill", "id":ID,
                    "target":{"kind":"version", "version":TWO, "expected_sha256":"0".repeat(64)}
                })
                .as_object()
                .unwrap()
                .clone(),
            )
            .await?;
            equal(&(conflict.is_error), &(Some(true)))?;
            let conflict = conflict
                .structured_content
                .ok_or("missing retraction conflict")?;
            equal(&(conflict["code"]), &("hub_unpublish_conflict"))?;
            equal(&(conflict["cause"]), &("conditional_conflict"))?;
            equal(&(conflict["retraction"]["catalog"]), &("not_attempted"))?;
            let unchanged = store.get(&key).await?;
            equal(&(unchanged.meta.e_tag), &(etag_before))?;
            equal(&(unchanged.bytes().await?), &(catalog_before))?;

            // No PROJECT selection or version, and a CWD outside every monitored PROJECT.
            let removed = cli(
                &f,
                &[
                    "hub",
                    "unpublish",
                    "--provider",
                    PROVIDER,
                    "--producer",
                    &producer_text,
                    "--kind",
                    "skill",
                    "--id",
                    ID,
                    "--version",
                    TWO,
                    "--expected-sha256",
                    &hash_two,
                ],
                true,
            )?;
            receipt(&removed, producer, &[TWO], None, "updated")?;
            let retained = store.get(&key).await?;
            different(&(retained.meta.e_tag), &(etag_before))?;
            let retained: serde_json::Value = serde_json::from_slice(&retained.bytes().await?)?;
            equal(&(retained["latest"]), &(serde_json::Value::Null))?;
            equal(
                &(retained["versions"]),
                &(json!([{"version":ONE,"hash":hash_one}])),
            )?;

            if !conditional_delete {
                // The preceding partial removal proved actual MinIO conditional PUT. Both kinds
                // of catalog deletion must now fail before any DELETE reaches this backend.
                let before = store.get(&key).await?;
                let etag = before.meta.e_tag.clone();
                let bytes = before.bytes().await?;
                for target in [
                    json!({"kind":"artifact"}),
                    json!({"kind":"version","version":ONE,"expected_sha256":hash_one}),
                ] {
                    let unsupported = mcp_call(&f, "graphit_hub_unpublish", json!({
                    "provider":PROVIDER, "producer":producer, "artifact_type":"skill", "id":ID,
                    "target":target
                }).as_object().unwrap().clone()).await?;
                    equal(&unsupported.is_error, &Some(true))?;
                    let unsupported = unsupported
                        .structured_content
                        .ok_or("missing unsupported receipt")?;
                    equal(&unsupported["status"], &"error")?;
                    equal(&unsupported["code"], &"hub_unpublish_unsupported")?;
                    equal(&unsupported["cause"], &"unsupported_storage_operation")?;
                    equal(&unsupported["stage"], &"catalog_delete")?;
                    equal(&unsupported["retraction"]["catalog"], &"not_attempted")?;
                    equal(
                        &unsupported["retraction"]["targeted_versions"],
                        &json!([ONE]),
                    )?;
                    let unchanged = store.get(&key).await?;
                    equal(&unchanged.meta.e_tag, &etag)?;
                    equal(&unchanged.bytes().await?, &bytes)?;
                }
                equal(&store.get(&payload_one).await?.bytes().await?, &zip_one)?;
                equal(&store.get(&payload_two).await?.bytes().await?, &zip_two)?;
                equal(&fs::read(source_one.join("SKILL.md"))?, &source_before)?;
                equal(&fs::read(source_two.join("SKILL.md"))?, &source_two_before)?;
                equal(&fs::read(original.join("SKILL.md"))?, &cache_before)?;
                equal(&fs::read(projected.join("SKILL.md"))?, &installed_before)?;
                equal(&fs::read(registry.path())?, &registry_before)?;
                return Ok(());
            }

            let removed = mcp_call(
                &f,
                "graphit_hub_unpublish",
                json!({
                    "provider":PROVIDER, "producer":producer, "artifact_type":"skill", "id":ID,
                    "target":{"kind":"artifact"}
                })
                .as_object()
                .unwrap()
                .clone(),
            )
            .await?;
            let removed = checked_mcp(removed, "whole-artifact retraction")?;
            receipt(&removed, producer, &[ONE], None, "removed")?;
            missing(store.as_ref(), &key).await?;
            equal(&(store.get(&payload_one).await?.bytes().await?), &(zip_one))?;
            equal(&(store.get(&payload_two).await?.bytes().await?), &(zip_two))?;
            equal(&(fs::read(source_one.join("SKILL.md"))?), &(source_before))?;
            equal(
                &(fs::read(source_two.join("SKILL.md"))?),
                &(source_two_before),
            )?;
            equal(&(fs::read(original.join("SKILL.md"))?), &(cache_before))?;
            equal(
                &(fs::read(projected.join("SKILL.md"))?),
                &(installed_before),
            )?;
            equal(
                &(fs::read_to_string(projected.join("assets/literal.txt"))?),
                &("retained_one"),
            )?;
            equal(&(fs::read(registry.path())?), &(registry_before))?;

            // Version-targeted removal of the final entry must also delete the actual object.
            let (last_source, last_hash) = publish(&f, producer, &policy, TWO, "retained_last")?;
            let last_source_before = fs::read(last_source.join("SKILL.md"))?;
            let last_payload = payload_key(&broker, producer, &policy, TWO, &last_hash)?;
            let last_bytes = store.get(&last_payload).await?.bytes().await?;
            let last_catalog: serde_json::Value =
                serde_json::from_slice(&store.get(&key).await?.bytes().await?)?;
            equal(&(last_catalog["latest"]), &(TWO))?;
            equal(
                &(last_catalog["versions"]),
                &(json!([{"version":TWO,"hash":last_hash}])),
            )?;

            let last = mcp_call(
                &f,
                "graphit_hub_unpublish",
                json!({
                    "provider":PROVIDER, "producer":producer, "artifact_type":"skill", "id":ID,
                    "target":{"kind":"version","version":TWO,"expected_sha256":last_hash}
                })
                .as_object()
                .unwrap()
                .clone(),
            )
            .await?;
            let last = checked_mcp(last, "last-version retraction")?;
            receipt(&last, producer, &[TWO], None, "removed")?;
            missing(store.as_ref(), &key).await?;
            equal(
                &(store.get(&last_payload).await?.bytes().await?),
                &(last_bytes),
            )?;
            equal(
                &(fs::read(last_source.join("SKILL.md"))?),
                &(last_source_before),
            )?;
            equal(&(fs::read(registry.path())?), &(registry_before))?;
            equal(&(fs::read(original.join("SKILL.md"))?), &(cache_before))?;
            equal(
                &(fs::read(projected.join("SKILL.md"))?),
                &(installed_before),
            )?;
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
