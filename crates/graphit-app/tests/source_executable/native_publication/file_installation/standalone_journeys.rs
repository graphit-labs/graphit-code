//! Explicit standalone transport and verified Native identity remain independent.
use super::*;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
#[path = "compiled_skills_remote.rs"]
mod compiled_skills_remote;
#[path = "remote_catalog.rs"]
mod remote_catalog;
use graphit_auth::{
    profile::{
        AiService, LocalCredentials, LocalS3, OidcTokens, ProfileCredentials, ProfileData,
        ProfileExpectation, ProfileLimits, ProviderConfig, ProviderKind, S3Location, S3Source,
        Secret,
    },
    store::AuthStore,
};
use graphit_core::auth_document::{
    AuthDocumentLimits, AuthDocumentStore, publication::AuthPublicationEffect,
};
use graphit_hub::{SourceRef, registry::HubRegistry};
use std::path::{Path, PathBuf};

const LOCAL: &str = "standalone-file-storage";
const PROFILE: &str = "standalone-file-keys";
const CHAIN: &str = "standalone-file-chain";
const CHAIN_PROFILE: &str = "standalone-chain-profile";
const AWS_PROFILE: &str = "graphit-file-chain";
const FILE_ID: &str = "fixture/standalone-file";
const VERSION: &str = "standalone/one";
const CLAIMED_USER: &str = "untrusted-user";
const CLAIMED_TEAM: &str = "untrusted-team";

#[derive(Deserialize)]
pub(crate) struct StandaloneStorage {
    pub(crate) bucket: String,
    pub(crate) region: String,
    pub(crate) endpoint: String,
    pub(crate) prefix: String,
    pub(crate) access_key_id: String,
    pub(crate) secret_access_key: String,
    pub(crate) session_token: Option<String>,
    pub(crate) expires_at: Option<i64>,
}
impl Drop for StandaloneStorage {
    fn drop(&mut self) {
        self.access_key_id.zeroize();
        self.secret_access_key.zeroize();
        self.session_token.zeroize();
    }
}

fn storage_client(keys: &StandaloneStorage) -> Result<Arc<dyn ObjectStore>> {
    let mut builder = object_store::aws::AmazonS3Builder::new()
        .with_bucket_name(&keys.bucket)
        .with_region(&keys.region)
        .with_endpoint(&keys.endpoint)
        .with_allow_http(true)
        .with_access_key_id(&keys.access_key_id)
        .with_secret_access_key(&keys.secret_access_key);
    if let Some(token) = &keys.session_token {
        builder = builder.with_token(token);
    }
    Ok(Arc::new(builder.build()?))
}

fn local_profiles(f: &Fixture, ready: &Ready, keys: &StandaloneStorage) -> Result {
    let config = ConfigSnapshot::new(f.paths.brand(), [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let cancellation = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancellation)?;
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
    let publication = store.mutate(store.load(&original)?, &original, |doc, ctx| {
        for (provider, profile, chain) in [(LOCAL, PROFILE, false), (CHAIN, CHAIN_PROFILE, true)] {
            doc.add_provider(
                provider.into(),
                ProviderConfig {
                    kind: ProviderKind::Local {
                        allow_aws_credential_chain: chain,
                        broker: None,
                    },
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: Some(keys.bucket.clone()),
                        region: Some(keys.region.clone()),
                        endpoint: Some(keys.endpoint.clone()),
                        prefix: Some(keys.prefix.clone()),
                        source: if chain {
                            S3Source::AwsChain
                        } else {
                            S3Source::LocalCredentials
                        },
                    },
                },
                doc.revision(),
                1,
                ctx,
            )?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc
                    .provider(provider)
                    .ok_or(graphit_auth::profile::ProfileError::Missing)?
                    .revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(
                ProfileData {
                    name: profile.into(),
                    provider: provider.into(),
                    username: CLAIMED_USER.into(),
                    organization: None,
                    teams: vec![CLAIMED_TEAM.into()],
                    credentials: ProfileCredentials::Local(LocalCredentials {
                        anonymous: false,
                        mcp_key: None,
                        broker_key: None,
                        embedding_api_key: None,
                        rerank_api_key: None,
                        s3: Some(LocalS3 {
                            access_key_id: (!chain)
                                .then(|| Secret::new(keys.access_key_id.clone().into())),
                            secret_access_key: (!chain)
                                .then(|| Secret::new(keys.secret_access_key.clone().into())),
                            session_token: if chain {
                                None
                            } else {
                                keys.session_token
                                    .as_ref()
                                    .map(|token| Secret::new(token.clone().into()))
                            },
                            expires_at: keys.expires_at,
                            aws_profile: chain.then(|| AWS_PROFILE.into()),
                        }),
                    }),
                },
                expected,
                1,
                ctx,
            )?;
        }
        // Change only persisted presentation claims; keep the genuine signed bearer.
        let native = doc
            .profile("publication")
            .ok_or(graphit_auth::profile::ProfileError::Missing)?;
        let native_revision = native.revision();
        let updated = native.updated_at() + 1;
        let expected = ProfileExpectation {
            document_revision: doc.revision(),
            provider_revision: doc
                .provider(PROVIDER)
                .ok_or(graphit_auth::profile::ProfileError::Missing)?
                .revision(),
            profile_revision: Some(native_revision),
        };
        doc.replace_profile_data(
            ProfileData {
                name: "publication".into(),
                provider: PROVIDER.into(),
                username: CLAIMED_USER.into(),
                organization: None,
                teams: vec![CLAIMED_TEAM.into()],
                credentials: ProfileCredentials::Broker {
                    issuer: ready.issuer.clone(),
                    subject: ready.subject.clone(),
                    s3_disabled: false,
                    tokens: OidcTokens {
                        access_token: Secret::new(ready.access_bearer.clone().into()),
                        refresh_token: None,
                        id_token: Secret::new("fixture-historical-id-token".into()),
                        expires_at: ready.oauth_expires_at_micros()?,
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
        return Err("standalone fixture profiles were not published cleanly".into());
    }
    Ok(())
}

async fn put_json(
    store: &Arc<dyn ObjectStore>,
    prefix: &str,
    key: &str,
    value: serde_json::Value,
) -> Result {
    store
        .put(&object_key(prefix, key), serde_json::to_vec(&value)?.into())
        .await?;
    Ok(())
}

async fn seed_file(
    store: &Arc<dyn ObjectStore>,
    prefix: &str,
    producer: graphit_core::project_identity::ProjectId,
    file_id: &str,
    policy: &HubResourcePolicy,
) -> Result {
    let id = ArtifactId::new(file_id, policy.projection())?;
    let version = ConcreteVersion::new(VERSION, policy.projection())?;
    let bytes = payload("standalone_original_literal")?;
    let hash: [u8; 32] = Sha256::digest(&bytes).into();
    let digest = hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    store
        .put(
            &object_key(
                prefix,
                &format!(
                    "{}payload.zip",
                    release_prefix(producer, ArtifactType::Skill, &id, &version, &hash)?
                ),
            ),
            bytes.into(),
        )
        .await?;
    let cancellation = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let content = policy.content();
    let bytes = RemoteArtifactManifest::encode(
        &ManifestInput {
            producer,
            artifact_type: ArtifactType::Skill,
            id: file_id,
            name: "Standalone FILE fixture",
            description: None,
            tags: &[],
            author: None,
            latest: Some(VERSION),
            versions: &[VersionInput {
                version: VERSION,
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
            &object_key(prefix, &registry_key(producer, ArtifactType::Skill, &id)?),
            bytes.into(),
        )
        .await?;
    Ok(())
}

fn remote_arguments(producer: &str, identity: bool, profile: bool) -> Vec<&str> {
    let mut args = vec![
        "hub",
        "content",
        "remote",
        "--transport",
        "standalone",
        "--provider",
        LOCAL,
        "--producer",
        producer,
        "--kind",
        "skill",
        "--id",
        FILE_ID,
        "--version",
        VERSION,
        "--path",
        "SKILL.md",
    ];
    if profile {
        args.extend(["--profile", PROFILE]);
    }
    if identity {
        args.extend(["--identity-provider", PROVIDER]);
    }
    args
}

fn allowed(fixture: &Fixture, args: &[&str]) -> Result<serde_json::Value> {
    let output = fixture.command().args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "standalone command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    if value["status"] != "ok" {
        return Err("standalone consumer did not report success".into());
    }
    Ok(value)
}

fn denied_command(fixture: &Fixture, args: &[&str], acl: bool) -> Result {
    let output = fixture.command().args(args).output()?;
    if output.status.success()
        || (acl
            && !String::from_utf8_lossy(&output.stderr)
                .contains("authentication_or_permission_denied"))
    {
        return Err(io::Error::other(format!(
            "standalone ACL/transport denial was not preserved: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(())
}

fn original(value: &serde_json::Value) -> Result {
    if value["content"]["path"] != "SKILL.md"
        || !value["content"]["content"]
            .as_str()
            .is_some_and(|value| value.contains("standalone_original_literal"))
    {
        return Err("standalone content lost the original source".into());
    }
    Ok(())
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

#[cfg(unix)]
fn configure_aws_process(
    fixture: &Fixture,
    keys: &StandaloneStorage,
) -> Result<(PathBuf, PathBuf)> {
    use std::os::unix::fs::PermissionsExt as _;
    let root = fixture._temp.path();
    let reply = root.join("aws-process-reply.json");
    let counter = root.join("aws-process-count");
    let process = root.join("aws-process.sh");
    let config = root.join("aws-process-config");
    let mut credentials = json!({"Version":1,"AccessKeyId":keys.access_key_id,"SecretAccessKey":keys.secret_access_key});
    if let Some(token) = &keys.session_token {
        credentials["SessionToken"] = token.clone().into();
    }
    if let Some(expiry) = keys.expires_at {
        credentials["Expiration"] = chrono::DateTime::from_timestamp(expiry, 0)
            .ok_or("invalid fixture credential expiry")?
            .to_rfc3339()
            .into();
    }
    fs::write(&reply, serde_json::to_vec(&credentials)?)?;
    fs::set_permissions(&reply, fs::Permissions::from_mode(0o600))?;
    let quote = |path: &Path| -> Result<String> {
        Ok(format!(
            "'{}'",
            path.to_str()
                .ok_or("non UTF-8 fixture path")?
                .replace('\'', "'\\''")
        ))
    };
    fs::write(
        &process,
        format!(
            "#!/bin/sh\nprintf x >> {}\ncat {}\n",
            quote(&counter)?,
            quote(&reply)?
        ),
    )?;
    fs::set_permissions(&process, fs::Permissions::from_mode(0o700))?;
    fs::write(
        &config,
        format!(
            "[profile {AWS_PROFILE}]\ncredential_process = /bin/sh {}\nregion = {}\n",
            quote(&process)?,
            keys.region
        ),
    )?;
    fs::write(root.join("aws-empty-credentials"), [])?;
    Ok((config, counter))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built real Broker STSv3/MinIO fixture"]
async fn compiled_standalone_file_acl_uses_explicit_storage_and_verified_native_subject() -> Result
{
    let fixture = Fixture::new().await?;
    let (broker, storage) = BrokerFixture::start_standalone(&fixture, false).await?;
    let setup: Result = (|| {
        install_active_profile(&fixture, &broker.ready, false)?;
        local_profiles(&fixture, &broker.ready, &storage)
    })();
    if let Err(error) = setup {
        return joined(Err(error), broker.shutdown().await);
    }
    let mut daemon_command = fixture.command();
    daemon_command.args(["daemon", "foreground", "--project-version", "main"]);
    #[cfg(unix)]
    let aws_process = match configure_aws_process(&fixture, &storage) {
        Ok(process) => process,
        Err(error) => return joined(Err(error), broker.shutdown().await),
    };
    #[cfg(unix)]
    daemon_command
        .env("AWS_CONFIG_FILE", &aws_process.0)
        .env(
            "AWS_SHARED_CREDENTIALS_FILE",
            fixture._temp.path().join("aws-empty-credentials"),
        )
        .env("AWS_EC2_METADATA_DISABLED", "true");
    let daemon = match fixture.daemon_command(daemon_command).await {
        Ok(daemon) => daemon,
        Err(error) => return joined(Err(error), broker.shutdown().await),
    };
    let result: Result = async {
        let producer = ProjectIdentityStore::at(&fixture.projects[0])
            .load()?
            .ok_or("missing producer")?
            .id();
        let producer_text = producer.to_string();
        let store = storage_client(&storage)?;
        let prefix = &storage.prefix;
        let config = ConfigSnapshot::new(fixture.paths.brand(), [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let issuer = URL_SAFE_NO_PAD.encode(&broker.ready.issuer);
        let user = URL_SAFE_NO_PAD.encode(&broker.ready.subject);
        let user_key = format!("v1/users/{issuer}/{user}/projects.json");
        let empty = json!({"v":1,"projects":[]});
        let grant = json!({"v":1,"projects":[{"id":producer}]});
        for key in [
            "v1/global/projects.json",
            "v1/anonymous/projects.json",
            "v1/authenticated/projects.json",
            &user_key,
        ] {
            put_json(&store, prefix, key, empty.clone()).await?;
        }
        put_json(
            &store,
            prefix,
            &graphit_hub::project_metadata::project_metadata_key(producer),
            json!({"v":1,"project":{"id":producer,"name":"standalone-fixture","status":"active"}}),
        )
        .await?;
        put_json(
            &store,
            prefix,
            &format!(
                "v1/users/{issuer}/{}/projects.json",
                URL_SAFE_NO_PAD.encode(CLAIMED_USER)
            ),
            grant.clone(),
        )
        .await?;
        put_json(
            &store,
            prefix,
            &format!(
                "v1/teams/{issuer}/{}/projects.json",
                URL_SAFE_NO_PAD.encode(CLAIMED_TEAM)
            ),
            grant.clone(),
        )
        .await?;
        // There is no catalogue/payload yet. An ignored ACL would produce NotFound,
        // rather than the required authorization denial asserted here.
        denied_command(
            &fixture,
            &remote_arguments(&producer_text, false, true),
            true,
        )?;
        denied_command(
            &fixture,
            &remote_arguments(&producer_text, true, true),
            true,
        )?;
        seed_file(&store, prefix, producer, FILE_ID, &policy).await?;
        put_json(
            &store,
            prefix,
            "v1/global/projects.json",
            json!({"v":1,"projects":[{"name_prefix":"standalone-"}]}),
        )
        .await?;
        original(&allowed(
            &fixture,
            &remote_arguments(&producer_text, false, true),
        )?)?;
        allowed(
            &fixture,
            &[
                "hub",
                "install",
                "--global",
                "--transport",
                "standalone",
                "--provider",
                LOCAL,
                "--profile",
                PROFILE,
                "--producer",
                &producer_text,
                "--kind",
                "skill",
                "--id",
                FILE_ID,
                "--version",
                VERSION,
            ],
        )?;
        let identity = InstallationIdentityStore::new(&fixture.paths)?.load()?;
        let global = HubRegistry::global(
            &fixture.paths,
            identity.unit_id().ok_or("missing installation")?,
            policy.hub(),
        )?;
        let source = SourceRef::hub_cache(
            &fixture.paths,
            producer,
            ArtifactType::Skill,
            ArtifactId::new(FILE_ID, policy.projection())?,
            None,
            ConcreteVersion::new(VERSION, policy.projection())?,
            policy.projection(),
        )?
        .with_remote_file_origin(graphit_hub::RemoteFileOrigin::Standalone { provider: LOCAL.into(), profile: Some(PROFILE.into()) },
            Sha256::digest(payload("standalone_original_literal")?).into(), policy.projection())?;
        let cached = fs::read(source.directory().as_path().join("SKILL.md"))?;
        put_json(&store, prefix, "v1/global/projects.json", empty.clone()).await?;
        let installed = [
            "hub",
            "content",
            "installed",
            "--global",
            "--kind",
            "skill",
            "--id",
            FILE_ID,
            "--version",
            VERSION,
        ];
        denied_command(&fixture, &installed, true)?;
        if fs::read(source.directory().as_path().join("SKILL.md"))? != cached {
            return Err("ACL denial changed the verified cache".into());
        }
        put_json(&store, prefix, "v1/anonymous/projects.json", grant.clone()).await?;
        original(&allowed(
            &fixture,
            &remote_arguments(&producer_text, false, true),
        )?)?;
        // This explicitly unsigned request targets a private bucket. No LocalS3
        // profile, active Native token, or previous cache may become a fallback.
        denied_command(
            &fixture,
            &remote_arguments(&producer_text, false, false),
            false,
        )?;
        put_json(&store, prefix, "v1/anonymous/projects.json", empty.clone()).await?;
        put_json(&store, prefix, &user_key, grant).await?;
        denied_command(
            &fixture,
            &remote_arguments(&producer_text, false, true),
            true,
        )?;
        original(&allowed(
            &fixture,
            &remote_arguments(&producer_text, true, true),
        )?)?;
        let mut installed_identity = installed.to_vec();
        installed_identity.extend(["--identity-provider", PROVIDER]);
        original(&allowed(&fixture, &installed_identity)?)?;
        let listed = allowed(&fixture, &["hub", "list", "--global", "--identity-provider", PROVIDER, "--offset", "0", "--limit", "1"])?;
        if listed["entries"].as_array().is_none_or(|entries| entries.len() != 1) {
            return Err("Standalone list lost its verified identity".into());
        }
        let eof = allowed(&fixture, &["hub", "list", "--global", "--identity-provider", PROVIDER, "--offset", "10", "--limit", "1"])?;
        if eof["entries"] != json!([]) || !eof["next_offset"].is_null() {
            return Err("Standalone pagination past EOF must retain a valid identity selection".into());
        }
        denied_command(&fixture, &["hub", "list", "--global", "--identity-provider", "missing-identity-provider", "--offset", "10"], false)?;
        let mut invalid_identity = remote_arguments(&producer_text, false, true);
        invalid_identity.extend(["--identity-provider", "missing-identity-provider"]);
        denied_command(&fixture, &invalid_identity, false)?;
        put_json(&store, prefix, &user_key, empty.clone()).await?;
        put_json(
            &store,
            prefix,
            "v1/authenticated/projects.json",
            json!({"v":1,"projects":[{"all":true}]}),
        )
        .await?;
        denied_command(
            &fixture,
            &remote_arguments(&producer_text, false, true),
            true,
        )?;
        original(&allowed(
            &fixture,
            &remote_arguments(&producer_text, true, true),
        )?)?;
        if global.load()?.entries().count() != 1 {
            return Err("standalone content changed installation membership".into());
        }
        #[cfg(unix)]
        {
            let second = ProjectIdentityStore::at(&fixture.projects[1]).load()?.ok_or("missing second producer")?.id();
            put_json(&store, prefix, &graphit_hub::project_metadata::project_metadata_key(second),
                json!({"v":1,"project":{"id":second,"name":"standalone-second","status":"active"}})).await?;
            for (producer, id) in [(producer, "fixture/aws-one"), (second, "fixture/aws-two")] {
                seed_file(&store, prefix, producer, id, &policy).await?;
                allowed(&fixture, &[
                    "hub", "install", "--global", "--transport", "standalone", "--provider", CHAIN,
                    "--profile", CHAIN_PROFILE, "--identity-provider", PROVIDER, "--producer", &producer.to_string(),
                    "--kind", "skill", "--id", id, "--version", VERSION,
                ])?;
            }
            allowed(&fixture, &[
                "--project-version", "main", "hub", "install", "--transport", "standalone",
                "--provider", CHAIN, "--profile", CHAIN_PROFILE, "--identity-provider", PROVIDER,
                "--producer", &producer_text, "--kind", "skill", "--id", FILE_ID, "--version", VERSION,
                "--binding", "codex", "--alias", "chain-file",
            ])?;
            let project = HubRegistry::project(&fixture.projects[0],
                &ProjectIdentityStore::at(&fixture.projects[0]).load()?.ok_or("missing project identity")?, policy.hub())?.load()?;
            let chain_source = &project.get(ArtifactType::Skill, FILE_ID).ok_or("missing CHAIN project artifact")?
                .installations[0].original_source;
            if chain_source.id() != source.id() || chain_source.version() != source.version()
                || chain_source.archive_sha256() != source.archive_sha256()
                || chain_source.directory() == source.directory()
                || fs::read(chain_source.directory().as_path().join("SKILL.md"))? != cached
                || fs::read(source.directory().as_path().join("SKILL.md"))? != cached {
                return Err("equal bytes from distinct origin/profile must keep separate owned cache identities".into());
            }
            fs::write(&aws_process.1, [])?;
            let listed = allowed(&fixture, &["hub", "list", "--global", "--kind", "skill", "--identity-provider", PROVIDER])?;
            if listed["entries"].as_array().is_none_or(|entries| entries.len() != 3)
                || fs::read(&aws_process.1)? != b"x" {
                return Err("one page must resolve AWS credentials once per origin while authorizing both producers".into());
            }
        }
        Ok(())
    }
    .await;
    let (daemon_cleanup, broker_cleanup) = tokio::join!(daemon.stop("-TERM"), broker.shutdown());
    joined(result, joined(daemon_cleanup, broker_cleanup))
}
