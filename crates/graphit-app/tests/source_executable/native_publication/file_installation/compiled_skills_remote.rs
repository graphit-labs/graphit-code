//! Explicit bundles use the real FILE producers/installation and current origin authority.
use super::*;
use graphit_core::module_settings::Module;
use graphit_runtime::skill_instructions::skill_templates;

const BUNDLE: &str = "fixture/compiled-task";
const RELEASE: &str = "release/one";

fn cli(f: &Fixture, project: &ProjectPaths, args: &[&str]) -> Result<std::process::Output> {
    Ok(f.command()
        .current_dir(project.root())
        .args(["--project-version", "main"])
        .args(args)
        .output()?)
}
fn accepted(f: &Fixture, project: &ProjectPaths, args: &[&str]) -> Result<serde_json::Value> {
    let output = cli(f, project, args)?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "compiled skill CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn refused(
    f: &Fixture,
    project: &ProjectPaths,
    args: &[&str],
    published: &Path,
    bytes: &[u8],
) -> Result {
    let output = cli(f, project, args)?;
    assert!(
        !output.status.success(),
        "a selected remote bundle must not become a local or builtin fallback"
    );
    assert_eq!(
        fs::read(published)?,
        bytes,
        "failed origin admission must preserve the existing compiled skill"
    );
    Ok(())
}
fn configure(f: &Fixture) -> Result {
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
}
fn bundle_source(f: &Fixture, policy: &HubResourcePolicy) -> Result<PathBuf> {
    let producer = ProjectIdentityStore::at(&f.projects[0])
        .load()?
        .ok_or("missing producer")?
        .id();
    let source = SourceRef::resources(
        &f.projects[0],
        producer,
        ArtifactType::Skill,
        ArtifactId::new(BUNDLE, policy.projection())?,
        ConcreteVersion::new(RELEASE, policy.projection())?,
        policy.projection(),
    )?;
    fs::create_dir_all(source.directory().as_path().join("references"))?;
    fs::write(
        source.directory().as_path().join("SKILL.md"),
        "---\nname: remote-task\ndescription: Current remote task bundle\nlicense: MIT\ncompatibility: Graphit Rust\nmetadata:\n  source: remote-fixture\n---\nRemote bundle {{brand}} {{mcp_tool_task_search}}.\n",
    )?;
    for companion in skill_templates(Module::Task)?.references() {
        fs::write(
            source.directory().as_path().join(companion.relative_path),
            "Remote companion {{brand}}.\n",
        )?;
    }
    Ok(source.directory().as_path().to_owned())
}
fn assert_compiled(f: &Fixture, project: &ProjectPaths) -> Result<(PathBuf, Vec<u8>)> {
    let root = project
        .root()
        .join(".agents/skills")
        .join(f.paths.brand().skill_name("task")?);
    let body = fs::read_to_string(root.join("SKILL.md"))?;
    assert!(body.contains(&format!(
        "Remote bundle {} {}.",
        f.paths.brand().key(),
        f.paths.brand().mcp_tool_name(&["task", "search"])?
    )));
    assert!(
        body.contains("license: MIT")
            && body.contains("compatibility: Graphit Rust")
            && body.contains("remote-fixture")
    );
    for companion in skill_templates(Module::Task)?.references() {
        assert_eq!(
            fs::read_to_string(root.join(companion.relative_path))?,
            format!("Remote companion {}.\n", f.paths.brand().key())
        );
    }
    let path = root.join("SKILL.md");
    Ok((path.clone(), fs::read(path)?))
}
fn cache_source(
    f: &Fixture,
    project: &ProjectPaths,
    id: &str,
    policy: &HubResourcePolicy,
) -> Result<SourceRef> {
    let identity = ProjectIdentityStore::at(project)
        .load()?
        .ok_or("missing consumer")?;
    let registry = HubRegistry::project(project, &identity, policy.hub())?.load()?;
    let entry = registry
        .get_for_consumer(
            &super::super::consumer_journeys::project_consumer(project, policy)?,
            ArtifactType::Skill,
            id,
        )
        .ok_or("missing installed FILE bundle")?;
    assert_eq!(entry.installations.len(), 1);
    let source = entry.installations[0].original_source.clone();
    assert!(source.directory().as_path().starts_with(f.paths.root()));
    Ok(source)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built real Broker STSv3/MinIO fixture"]
async fn compiled_remote_skill_bundle_uses_real_broker_publication_and_verified_cache() -> Result {
    let f = Fixture::new().await?;
    let broker = BrokerFixture::start(&f).await?;
    let setup = install_active_profile(&f, &broker.ready, false).and_then(|()| configure(&f));
    if let Err(cause) = setup {
        return joined(Err(cause), broker.shutdown().await);
    }
    let daemon = match f.daemon(&["--project-version", "main"]).await {
        Ok(daemon) => daemon,
        Err(cause) => return joined(Err(cause), broker.shutdown().await),
    };
    let result: Result = async {
        let policy = HubResourcePolicy::resolve(&ConfigSnapshot::new(
            f.paths.brand(),
            [HubResourcePolicy::distribution_defaults(f.paths.brand())?],
        )?)?;
        bundle_source(&f, &policy)?;
        let producer = ProjectIdentityStore::at(&f.projects[0])
            .load()?
            .ok_or("missing producer")?
            .id()
            .to_string();
        let consumer = &f.projects[1];
        accepted(
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
                BUNDLE,
                "--version",
                RELEASE,
                "--name",
                "Remote compiled task",
            ],
        )?;
        let installed = accepted(
            &f,
            consumer,
            &[
                "hub",
                "install",
                "--provider",
                PROVIDER,
                "--producer",
                &producer,
                "--kind",
                "skill",
                "--id",
                BUNDLE,
                "--version",
                RELEASE,
                "--exact",
                "--binding",
                "codex",
                "--alias",
                "remote-task-source",
            ],
        )?;
        assert_eq!(installed["status"], "ok");
        let selection = format!("task={BUNDLE}@{RELEASE}");
        let sync = [
            "roles",
            "sync",
            "--agent",
            "codex",
            "--role",
            "scribe",
            "--skill-bundle",
            selection.as_str(),
        ];
        let outcome = accepted(&f, consumer, &sync)?;
        assert!(outcome["failure"].is_null());
        let (published, bytes) = assert_compiled(&f, consumer)?;
        // A Broker origin cannot silently become a standalone identity selection.
        let extra_identity = format!("task={PROVIDER}");
        let mut invalid = sync.to_vec();
        invalid.extend(["--skill-identity-provider", &extra_identity]);
        refused(&f, consumer, &invalid, &published, &bytes)?;
        let source = cache_source(&f, consumer, BUNDLE, &policy)?;
        fs::write(
            source.directory().as_path().join("SKILL.md"),
            "---\nname: changed-cache\ndescription: Valid syntax must not replace receipt verification\n---\nchanged valid cached bytes\n",
        )?;
        refused(&f, consumer, &sync, &published, &bytes)?;
        // Reading an installed selection must never hydrate a missing cache.
        fs::remove_dir_all(source.directory().as_path())?;
        refused(&f, consumer, &sync, &published, &bytes)?;
        assert!(!source.directory().as_path().exists());
        Ok(())
    }
    .await;
    let (daemon_cleanup, broker_cleanup) = tokio::join!(daemon.stop("-TERM"), broker.shutdown());
    joined(result, joined(daemon_cleanup, broker_cleanup))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built real Broker STSv3/MinIO fixture"]
async fn compiled_remote_skill_bundle_requires_current_standalone_project_grant_and_explicit_identity()
-> Result {
    let f = Fixture::new().await?;
    let (broker, storage) = BrokerFixture::start_standalone(&f, false).await?;
    let setup = install_active_profile(&f, &broker.ready, false)
        .and_then(|()| local_profiles(&f, &broker.ready, &storage))
        .and_then(|()| configure(&f));
    if let Err(cause) = setup {
        return joined(Err(cause), broker.shutdown().await);
    }
    let daemon = match f.daemon(&["--project-version", "main"]).await {
        Ok(daemon) => daemon,
        Err(cause) => return joined(Err(cause), broker.shutdown().await),
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
        let store = storage_client(&storage)?;
        let prefix = &storage.prefix;
        let issuer = URL_SAFE_NO_PAD.encode(&broker.ready.issuer);
        let subject = URL_SAFE_NO_PAD.encode(&broker.ready.subject);
        let user_key = format!("v1/users/{issuer}/{subject}/projects.json");
        let empty = json!({"v":1,"projects":[]});
        let grant = json!({"v":1,"projects":[{"id":producer}]});
        for key in [
            "v1/global/projects.json",
            "v1/anonymous/projects.json",
            "v1/authenticated/projects.json",
        ] {
            put_json(&store, prefix, key, empty.clone()).await?;
        }
        put_json(&store, prefix, &user_key, grant.clone()).await?;
        put_json(
            &store,
            prefix,
            &graphit_hub::project_metadata::project_metadata_key(producer),
            json!({"v":1,"project":{"id":producer,"name":"standalone-fixture","status":"active"}}),
        )
        .await?;
        // Use the genuine release manifest/archive producer and normal FILE installation.
        // The archive layout is the same SKILL.md/references format as Broker publication.
        let source = bundle_source(&f, &policy)?;
        let payload = {
            let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(0o644);
            for path in std::iter::once("SKILL.md").chain(
                skill_templates(Module::Task)?
                    .references()
                    .iter()
                    .map(|r| r.relative_path),
            ) {
                archive.start_file(path, options)?;
                archive.write_all(&fs::read(source.join(path))?)?;
            }
            archive.finish()?.into_inner()
        };
        let id = ArtifactId::new(BUNDLE, policy.projection())?;
        let version = ConcreteVersion::new(RELEASE, policy.projection())?;
        let hash: [u8; 32] = Sha256::digest(&payload).into();
        let digest = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
        store
            .put(
                &object_key(
                    prefix,
                    &format!(
                        "{}payload.zip",
                        release_prefix(producer, ArtifactType::Skill, &id, &version, &hash)?
                    ),
                ),
                payload.into(),
            )
            .await?;
        let cancel = Cancellation::default();
        let original = policy.request_context(Instant::now(), &cancel)?;
        let content = policy.content();
        let manifest = RemoteArtifactManifest::encode(
            &ManifestInput {
                producer,
                artifact_type: ArtifactType::Skill,
                id: BUNDLE,
                name: "Remote compiled task",
                description: None,
                tags: &[],
                author: None,
                latest: Some(RELEASE),
                versions: &[VersionInput {
                    version: RELEASE,
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
                manifest.into(),
            )
            .await?;
        let installed = accepted(
            &f,
            consumer,
            &[
                "hub",
                "install",
                "--transport",
                "standalone",
                "--provider",
                LOCAL,
                "--profile",
                PROFILE,
                "--identity-provider",
                PROVIDER,
                "--producer",
                &producer.to_string(),
                "--kind",
                "skill",
                "--id",
                BUNDLE,
                "--version",
                RELEASE,
                "--exact",
                "--binding",
                "codex",
                "--alias",
                "standalone-task-source",
            ],
        )?;
        assert_eq!(installed["status"], "ok");
        let bundle = format!("task={BUNDLE}@{RELEASE}");
        let identity = format!("task={PROVIDER}");
        let sync = [
            "roles",
            "sync",
            "--agent",
            "codex",
            "--role",
            "scribe",
            "--skill-bundle",
            &bundle,
            "--skill-identity-provider",
            &identity,
        ];
        assert!(accepted(&f, consumer, &sync)?["failure"].is_null());
        let (published, bytes) = assert_compiled(&f, consumer)?;
        refused(&f, consumer, &sync[..8], &published, &bytes)?;
        // The signed identity stays valid. Its PROJECT grant changes independently.
        put_json(&store, prefix, &user_key, empty).await?;
        refused(&f, consumer, &sync, &published, &bytes)?;
        put_json(&store, prefix, &user_key, grant).await?;
        assert!(accepted(&f, consumer, &sync)?["failure"].is_null());
        assert_compiled(&f, consumer)?;
        Ok(())
    }
    .await;
    let (daemon_cleanup, broker_cleanup) = tokio::join!(daemon.stop("-TERM"), broker.shutdown());
    joined(result, joined(daemon_cleanup, broker_cleanup))
}
