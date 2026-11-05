//! Real Foreground, Hub/Core publication, native Process/Codex wire and Auth/Records fixtures.
use super::*;
use graphit_auth::{
    ProjectCapture, ProjectReadModule, ProjectRecordAuthority, ProjectWriteOperation, ScopeCapture,
    ScopeInput,
};
use graphit_core::{
    artifact_store::{
        ArtifactMode, ArtifactStore, PayloadMode, PayloadOverlay, PayloadPolicy, PayloadSelection,
        RetainedFileSet, SourceSnapshot, ValidatedPayloadPlan,
    },
    brand::Brand,
    config::{ConfigDocument, ConfigSource},
    native_path::NativePath,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, ProjectionRequest, SourceRef,
    materialization::DerivedArtifact,
    native::{Role, ToolGrants},
    publication::ScopedCoordinator,
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{
        CreateMemory, Memory, MemoryListFilter, MemoryListRead, MemoryStoreReadLimits,
        MemoryStoreWriteLimits, NativeMemoryStore,
    },
};
use serde_json::json;
use std::{
    fs,
    num::{NonZeroU32, NonZeroUsize},
    path::Path,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
use crate::test_dream_peer as dream_peer;

struct Fixture {
    temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: ProjectPaths,
    id: ProjectId,
    config: ConfigSnapshot,
    policy: HubResourcePolicy,
    program: std::path::PathBuf,
    ready: tokio::net::UnixListener,
    duration: u64,
    retries: u32,
    records: usize,
    large_metadata: bool,
    input_limit: Option<u64>,
    output_limit: Option<u64>,
}
impl Fixture {
    fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let brand = Brand::default();
        let installation = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )?;
        let root = temp.path().join("project");
        fs::create_dir(&root)?;
        let project = ProjectPaths::at(&root, &installation)?;
        let cancel = Cancellation::default();
        let id = ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &cancel)?
            .ensure(&ProjectMetadata::new("dream-fixture")?)?
            .id();
        let config = ConfigSnapshot::new(&brand, [])?;
        let policy = HubResourcePolicy::resolve(&config)?;
        let program = temp.path().join("codex-dream-fixture");
        dream_peer::install(&program)?;
        let ready = tokio::net::UnixListener::bind(temp.path().join("project-ready.sock"))?;
        fs::create_dir(root.join(".codex"))?;
        fs::write(
            root.join(".codex/config.toml"),
            "[mcp_servers.\"literal.server α\"]\ncommand='unchanged'\n[mcp_servers.other]\nurl='https://fixture.invalid'\n",
        )?;
        Ok(Self {
            temp,
            installation,
            project,
            id,
            config,
            policy,
            program,
            ready,
            duration: 30,
            retries: 0,
            records: 4,
            large_metadata: false,
            input_limit: None,
            output_limit: None,
        })
    }
    fn inputs(&self) -> [ProjectBootstrapInput<'_>; 1] {
        [ProjectBootstrapInput {
            project: &self.project,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        }]
    }
    fn dream_config(&self) -> Result<ConfigSnapshot> {
        let mut entries = vec![
            (ConfigKey::new("modules.ast")?, json!(false)),
            (ConfigKey::new("modules.knowledge")?, json!(false)),
            (ConfigKey::new("modules.embedding")?, json!(false)),
            (ConfigKey::new("modules.hooks")?, json!(true)),
            (ConfigKey::new("modules.task")?, json!(false)),
            (ConfigKey::new("dream.enabled")?, json!(true)),
            (ConfigKey::new("dream.agent")?, json!("fixture/dream")),
            (
                ConfigKey::new("dream.model")?,
                json!("literal-model;$(no-shell)"),
            ),
            (ConfigKey::new("dream.program")?, json!(self.program)),
            (ConfigKey::new("dream.idle_timeout")?, json!(1)),
            (ConfigKey::new("dream.max_duration")?, json!(self.duration)),
            (ConfigKey::new("dream.retries")?, json!(self.retries)),
            (ConfigKey::new("dream.backoff")?, json!(1)),
        ];
        if let Some(limit) = self.input_limit {
            entries.push((
                ConfigKey::new("hub.limits.content.input_bytes")?,
                json!(limit),
            ));
            entries.push((
                ConfigKey::new("hub.limits.content.scalar_bytes")?,
                json!(limit),
            ));
        }
        if let Some(limit) = self.output_limit {
            entries.push((
                ConfigKey::new("hub.limits.content.output_bytes")?,
                json!(limit),
            ));
            entries.push((
                ConfigKey::new("hub.limits.content.scalar_bytes")?,
                json!(limit),
            ));
        }
        let layer = ConfigDocument::new(self.installation.brand(), ConfigSource::Command, entries)?;
        Ok(ConfigSnapshot::new(self.installation.brand(), [layer])?)
    }
    async fn prepare(&self) -> Result {
        let stop = Cancellation::default();
        let original = self.policy.request_context(Instant::now(), &stop)?;
        let owner = ForegroundSource::start(
            &self.installation,
            &self.inputs(),
            0,
            &self.config,
            NativeBinding::Codex,
            &original,
        )?;
        publish_agent(&owner, self, &original)?;
        seed(&owner, self.records, self.large_metadata).await?;
        if let Err(error) = owner.shutdown().await {
            eprintln!(
                "dream_fixture_stage=prepare_shutdown original_cancelled={} original_error={:?}",
                stop.is_cancelled(),
                original.snapshot().check().err().map(|error| error.kind())
            );
            return Err(error.into());
        }
        Ok(())
    }
    async fn start(&self, mode: &str) -> Result<ForegroundSource> {
        self.start_inputs(&self.inputs(), mode).await
    }
    async fn start_inputs(
        &self,
        inputs: &[ProjectBootstrapInput<'_>],
        mode: &str,
    ) -> Result<ForegroundSource> {
        let config = self.dream_config()?;
        let stop = Cancellation::default();
        let original = self.policy.request_context(Instant::now(), &stop)?;
        let started = ForegroundSource::start_and_publish(
            &self.installation,
            inputs,
            0,
            &config,
            NativeBinding::Codex,
            vec![
                (
                    "DREAM_PROJECT_READY".into(),
                    self.temp.path().join("project-ready.sock").into_os_string(),
                ),
                ("DREAM_FIXTURE_MODE".into(), mode.into()),
                ("DREAM_FIXTURE_PROJECT".into(), self.project.root().into()),
                (
                    "DREAM_FIXTURE_LOG".into(),
                    self.temp.path().join("attempts.jsonl").into_os_string(),
                ),
            ],
            &original,
        )
        .await;
        if started.is_err() {
            eprintln!(
                "dream_fixture_stage=start mode={mode} original_cancelled={} original_error={:?}",
                stop.is_cancelled(),
                original.snapshot().check().err().map(|error| error.kind())
            );
        }
        started.map_err(Into::into)
    }
}
pub(crate) async fn dream_shutdown_fixture() -> Result<(
    tempfile::TempDir,
    ForegroundSource,
    crate::foreground_dream::ForegroundDream,
)> {
    let fixture = Fixture::new()?;
    fixture.prepare().await?;
    let (source, dream) = {
        let stop = Cancellation::default();
        let original = fixture.policy.request_context(Instant::now(), &stop)?;
        let config = fixture.dream_config()?;
        // Use the same factory as async startup: Dream permissions must be
        // captured before DaemonAuth is constructed, without starting jobs here.
        let mut source = ForegroundSource::start_with_inventory(
            &fixture.installation,
            &fixture.inputs(),
            0,
            &config,
            NativeBinding::Codex,
            &original,
            None,
        )?;
        let dream = source
            .dream
            .take()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?;
        (source, dream)
    };
    Ok((fixture.temp, source, dream))
}

fn publish_agent(
    owner: &ForegroundSource,
    fixture: &Fixture,
    original: &RequestContext<'_>,
) -> Result {
    let hub = &owner.policy;
    let root = owner.auth.installation().live;
    let graphit_core::artifact_store::StoreOwner::Installation { installation_id } = root.owner()
    else {
        panic!("actual installation owner");
    };
    let registry = HubRegistry::global(&fixture.installation, installation_id, hub.hub())?;
    registry
        .writer(Duration::from_secs(5), original.cancellation())?
        .set_agents(&[NativeBinding::Codex])?;
    let directory = fixture.installation.root().join("dream-fixture-agent");
    fs::create_dir(&directory)?;
    fs::write(
        directory.join("AGENT.md"),
        "---\nname: dream\ndescription: PROJECT consolidation fixture\nread_only: false\nallows_shell: false\n---\nOnly the canonical authorized PROJECT recipe.\n",
    )?;
    let source = SourceRef::local_link(
        NativePath::new(directory.clone())?,
        fixture.id,
        ArtifactType::Agent,
        ArtifactId::new("fixture/dream", hub.projection())?,
        ConcreteVersion::new("v1", hub.projection())?,
        hub.projection(),
    )?;
    let retained = hub.retained();
    let snapshot = SourceSnapshot::capture(
        &directory,
        graphit_fs::NodeKind::Directory,
        &graphit_core::artifact_store::SnapshotPolicy::new(
            CopyPolicy::RejectFileLinks,
            hub.copy(),
            RetainedFileSet::new(["AGENT.md".into()], retained)?,
            retained,
        )?,
        original.snapshot(),
    )?;
    let input = snapshot
        .retained(Path::new("AGENT.md"))
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let mut render = RequestContext::new(
        hub.content(),
        original.cancellation(),
        original.snapshot().required_deadline()?,
    )?;
    let descriptor = render.parse_agent(input)?;
    let rendered = render.render_agent(
        NativeBinding::Codex,
        &descriptor,
        Some("dream"),
        &ToolGrants {
            binding: NativeBinding::Codex,
            // The parent launch enforces sandbox read-only and shell-off.
            // This pure Codex descriptor has no local read-only capability.
            read_only: false,
            allows_shell: false,
            tools: vec![],
        },
    )?;
    let root_mode = snapshot.nodes().next().unwrap().mode();
    let mode =
        PayloadMode::from_permissions(&fs::metadata(directory.join("AGENT.md"))?.permissions())?;
    let plan = ValidatedPayloadPlan::new(
        snapshot,
        [PayloadOverlay::new(
            "AGENT.md".into(),
            "rendered.toml".into(),
            rendered.bytes,
            mode,
        )?],
        &PayloadPolicy::new(graphit_fs::NodeKind::Directory, root_mode, hub.copy()),
        original.snapshot(),
    )?;
    let derived = DerivedArtifact::new(
        source.clone(),
        plan,
        NativeBinding::Codex.schema(),
        "dream",
        (
            PayloadSelection::new("AGENT.md".into(), graphit_fs::NodeKind::File)?,
            PayloadSelection::new("rendered.toml".into(), graphit_fs::NodeKind::File)?,
        ),
        hub.materialization().lineage,
    )?;
    let store = ArtifactStore::new(root.clone(), hub.artifacts());
    let coordinator = ScopedCoordinator::new(
        registry,
        store,
        NativePath::new(fixture.installation.root().to_owned())?,
    )?;
    let outcome = coordinator.writer(original)?.publish_derived(
        ProjectionRequest {
            binding: NativeBinding::Codex,
            source: &source,
            alias: Some("dream"),
            entry_name: None,
        },
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        derived,
        None,
    )?;
    assert!(outcome.change != graphit_hub::publication::PublicationChange::Unsupported);
    Ok(())
}
fn limits(owner: &ForegroundSource) -> (MemoryStoreReadLimits, MemoryStoreWriteLimits) {
    let content = owner.policy.content();
    let n = |v| NonZeroUsize::new(v).unwrap();
    let read = MemoryStoreReadLimits {
        native: owner.native,
        batch_rows: NonZeroU32::new(8192).unwrap(),
        max_query_bytes: n(content.max_output_bytes),
    };
    (
        read,
        MemoryStoreWriteLimits {
            read,
            artifacts: owner.policy.artifacts(),
            max_input_bytes: n(content.max_input_bytes),
            max_row_bytes: n(content.max_input_bytes),
        },
    )
}
async fn seed(owner: &ForegroundSource, count: usize, large_metadata: bool) -> Result<Vec<Memory>> {
    seed_project(owner, 0, count, large_metadata).await
}
async fn seed_project(
    owner: &ForegroundSource,
    project_index: usize,
    count: usize,
    large_metadata: bool,
) -> Result<Vec<Memory>> {
    let stop = Cancellation::default();
    let original = owner.policy.request_context(Instant::now(), &stop)?;
    let key = owner.auth.runtime().check(original.snapshot())?;
    let raw = std::str::from_utf8(key.secret(original.snapshot())?)?;
    let issuer = owner.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original)?;
    let session = issuer.bootstrap_admin(&caller)?;
    let parent = issuer.admin_parent(&caller, &session)?;
    let definition = graphit_auth::RoleDefinition::tool_owned(
        Role::Scribe,
        owner.auth.installation().live.brand(),
        NativeBinding::Codex,
        &[Permission::WriteMemory],
        [42; 32],
        IssuerLimits {
            max_permissions: 32,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 65536,
        },
    )?;
    let reference = issuer.configure_tool_owned_definition(&parent, definition)?;
    let (paths, id, version) = &owner.projects.as_ref().unwrap()[project_index];
    let captured = CapturedPublicationScope::project(paths, *id, owner.scope, original.snapshot())?;
    let record = captured.binding()?;
    let scope = || ScopeInput {
        installation: owner.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::Codex,
        global_version: "",
    };
    let run = issuer.start_run(&parent, &scope(), Duration::from_secs(60))?;
    let bearer = issuer.issue(
        &parent,
        &run,
        &reference,
        &[Permission::WriteMemory],
        &scope(),
    )?;
    let grant = issuer.verify(
        bearer.expose_for_transport(),
        &reference,
        scope(),
        &original,
    )?;
    let authority = ProjectRecordAuthority::new(issuer);
    let write = authority.admit_write(&grant, &original, ProjectWriteOperation::Memory)?;
    let (_, limits) = limits(owner);
    let mut records = Vec::new();
    for index in 0..count {
        records.push(
            NativeMemoryStore::create(
                original.snapshot(),
                &write,
                CreateMemory {
                    idempotency_key: format!("seed-{index}"),
                    title: format!("fixture-{index}"),
                    body: format!("confirmed fact {index}"),
                    attributes: MemoryAttributes {
                        classification: if index == 1 {
                            MemoryClass::Decision
                        } else {
                            MemoryClass::Fact
                        },
                        mandatory: index == 1,
                        important: index == 2,
                    },
                    tags: if large_metadata && index == 3 {
                        vec!["x".repeat(64 * 1024)]
                    } else {
                        vec![]
                    },
                    references: None,
                },
                limits,
            )
            .await?,
        );
    }
    issuer.close_run(&parent, &run)?;
    Ok(records)
}
async fn read(owner: &ForegroundSource) -> Result<Vec<Memory>> {
    read_project(owner, 0).await
}
async fn read_project(owner: &ForegroundSource, index: usize) -> Result<Vec<Memory>> {
    read_records(
        &owner.auth,
        &owner.policy,
        owner.scope,
        &owner.projects.as_ref().unwrap()[index],
        limits(owner).0,
    )
    .await
}
async fn read_records(
    auth: &DaemonAuth,
    policy: &HubResourcePolicy,
    scope_limits: ScopeLimits,
    project: &(ProjectPaths, ProjectId, VersionContext),
    read_limits: MemoryStoreReadLimits,
) -> Result<Vec<Memory>> {
    let stop = Cancellation::default();
    let original = policy.request_context(Instant::now(), &stop)?;
    let key = auth.runtime().check(original.snapshot())?;
    let caller = auth.issuer().authenticate_runtime(
        std::str::from_utf8(key.secret(original.snapshot())?)?,
        &original,
    )?;
    let (paths, id, version) = project;
    let captured =
        CapturedPublicationScope::project(paths, *id, scope_limits, original.snapshot())?;
    let record = captured.binding()?;
    let scope = ScopeInput {
        installation: auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::Codex,
        global_version: "",
    };
    let authority = ProjectRecordAuthority::new(auth.issuer());
    let admission =
        authority.admit_authenticated_read(&caller, &original, scope, ProjectReadModule::Memory)?;
    match NativeMemoryStore::list(
        original.snapshot(),
        &admission,
        MemoryListFilter::default(),
        read_limits,
    )
    .await?
    {
        MemoryListRead::Published { memories, .. } => Ok(memories),
        MemoryListRead::NoCorpus => Ok(vec![]),
    }
}
async fn native_ready(f: &Fixture, owner: &mut ForegroundSource) -> Result<tokio::net::UnixStream> {
    use tokio::io::AsyncReadExt;
    tokio::time::timeout(Duration::from_secs(15), async {
        tokio::select! {
            accepted = f.ready.accept() => {
                let (mut stream, _) = accepted?;
                let mut started = [0];
                stream.read_exact(&mut started).await?;
                assert_eq!(started, [1]);
                Ok(stream)
            },
            result = owner.dream.as_mut().unwrap().next() => {
                result?;
                Err(io::Error::other("Dream ended before the native peer became ready").into())
            }
        }
    })
    .await?
}

async fn wait_attempt(f: &Fixture, owner: &mut ForegroundSource, count: usize) -> Result {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if fs::read_to_string(f.temp.path().join("attempts.jsonl"))
            .is_ok_and(|v| v.lines().count() >= count)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        if let Ok(result) = tokio::time::timeout(
            Duration::from_millis(10),
            owner.dream.as_mut().unwrap().next(),
        )
        .await
        {
            result?;
        }
    }
}

async fn wait_completed(owner: &mut ForegroundSource) -> Result {
    let completion = owner.dream.as_ref().unwrap().iteration_completion();
    tokio::select! {
        biased;
        ()=completion.notified()=>Ok(()),
        result=owner.dream.as_mut().unwrap().next()=>{ result?; Err(io::Error::other("job ended before actual iteration completion").into()) },
        ()=tokio::time::sleep(Duration::from_secs(45))=>Err(io::Error::from(io::ErrorKind::TimedOut).into()),
    }
}

fn serve_source(owner: &mut ForegroundSource) -> Result<tokio::task::JoinHandle<io::Result<()>>> {
    let server = owner
        .server
        .as_ref()
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
        .clone();
    let listener = owner
        .listener
        .take()
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?;
    Ok(tokio::spawn(async move { server.serve(listener).await }))
}

async fn human_prompt(fixture: &Fixture, owner: &ForegroundSource) -> Result {
    human_prompt_project(fixture, owner, &fixture.project).await
}
async fn human_prompt_project(
    fixture: &Fixture,
    owner: &ForegroundSource,
    project: &ProjectPaths,
) -> Result {
    let cancel = Cancellation::default();
    let original = owner.policy.request_context(Instant::now(), &cancel)?;
    let arguments = crate::session_hook_entry::Arguments {
        recipient: crate::session_hook_entry::Recipient::Coordinator,
        engine: crate::session_hook_entry::Engine::Kiro,
        event: crate::session_hook_entry::Event::UserPromptSubmit,
        project_dir: Some(project.root().to_owned()),
        task_session: None,
        task_ids: Vec::new(),
    };
    let mut output = Vec::new();
    crate::session_hook_entry::call(
        &fixture.installation,
        arguments.recipient,
        crate::session_hook_entry::prepare(
            &fixture.installation,
            &arguments,
            Some("main"),
            &original,
        )?,
        owner
            .server
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
            .policy()
            .index,
        &owner.policy,
        &original,
        &mut output,
    )
    .await?;
    assert!(!output.is_empty());
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_dream_native_fold_delete_and_real_idle_activity() -> Result {
    let f = Fixture::new()?;
    f.prepare().await?;
    let mut owner = f.start("fold").await?;
    let serving = serve_source(&mut owner)?;
    let activity = owner
        .dream_activity
        .read()
        .map_err(|_| io::Error::other("activity lock poisoned"))?
        .as_ref()
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
        .1
        .subscribe();
    let before = read(&owner).await?;
    wait_attempt(&f, &mut owner, 1).await?;
    wait_completed(&mut owner).await?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let after = loop {
        let records = read(&owner).await?;
        if records.len() == 2 {
            break records;
        }
        if Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        if let Ok(result) = tokio::time::timeout(
            Duration::from_millis(10),
            owner.dream.as_mut().unwrap().next(),
        )
        .await
        {
            result?;
        }
    };
    let survivor_id = before.iter().find(|m| m.title == "fixture-0").unwrap().id;
    let important_id = before.iter().find(|m| m.title == "fixture-2").unwrap().id;
    let survivor = after.iter().find(|m| m.id == survivor_id).unwrap();
    assert_eq!(survivor.attributes.classification, MemoryClass::Decision);
    assert!(survivor.attributes.mandatory);
    assert!(
        after
            .iter()
            .any(|m| m.id == important_id && m.attributes.important)
    );
    assert_eq!(survivor.body, "confirmed fact 0; confirmed fact 1");
    fs::write(f.project.root().join("activity.txt"), "real FS activity")?;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        !activity.has_changed()?,
        "filesystem activity must not signal human activity"
    );
    assert_eq!(
        fs::read_to_string(f.temp.path().join("attempts.jsonl"))?
            .lines()
            .count(),
        1
    );
    human_prompt(&f, &owner).await?;
    assert!(
        activity.has_changed()?,
        "authenticated UserPromptSubmit must reach the actual Dream channel"
    );
    wait_attempt(&f, &mut owner, 2).await?;
    wait_completed(&mut owner).await?;
    let captures = fs::read_to_string(f.temp.path().join("attempts.jsonl"))?;
    let capture: serde_json::Value = serde_json::from_str(captures.lines().next().unwrap())?;
    assert_eq!(
        capture["fields"],
        json!([
            "body",
            "classification",
            "id",
            "important",
            "mandatory",
            "revision",
            "title"
        ])
    );
    assert_eq!(capture["version"], "main");
    assert_eq!(
        fs::read_to_string(f.project.root().join(".codex/config.toml"))?,
        "[mcp_servers.\"literal.server α\"]\ncommand='unchanged'\n[mcp_servers.other]\nurl='https://fixture.invalid'\n"
    );
    owner.shutdown().await?;
    serving.await??;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_dream_rejected_native_or_recipe_has_zero_effects() -> Result {
    for mode in [
        "foreign",
        "duplicate",
        "malformed",
        "partial",
        "exit_failure",
        "oversize",
        "noop",
        "protected",
        "stale",
        "last",
    ] {
        let mut f = Fixture::new()?;
        if mode == "oversize" {
            f.output_limit = Some(8192);
        }
        if mode == "last" {
            f.records = 1;
        }
        f.prepare().await?;
        let mut owner = f.start(mode).await?;
        let before = read(&owner).await?;
        if matches!(mode, "noop" | "protected" | "last") {
            wait_attempt(&f, &mut owner, 1).await?;
            wait_completed(&mut owner).await?;
        } else {
            assert!(
                tokio::time::timeout(
                    Duration::from_secs(15),
                    owner.dream.as_mut().unwrap().next()
                )
                .await?
                .is_err(),
                "{mode}"
            );
            assert_eq!(
                fs::read_to_string(f.temp.path().join("attempts.jsonl"))?
                    .lines()
                    .count(),
                1
            );
        }
        assert_eq!(read(&owner).await?, before, "{mode}");
        owner.shutdown().await?;
    }
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_dream_cancel_joins_native_without_memory_effects() -> Result {
    let f = Fixture::new()?;
    f.prepare().await?;
    let mut owner = f.start("hold").await?;
    let before = read(&owner).await?;
    wait_attempt(&f, &mut owner, 1).await?;
    assert!(
        tokio::time::timeout(
            Duration::from_millis(1),
            owner.dream.as_mut().unwrap().shutdown()
        )
        .await
        .is_err()
    );
    let error = owner.shutdown().await.unwrap_err();
    let mut owner = error.owner.unwrap();
    assert_eq!(read(&owner).await?, before);
    // The reported interruption is consumed; retry cleans up the retained owner.
    owner.dream.as_mut().unwrap().shutdown().await?;
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_dream_partial_receipt_reissues_same_actor_under_real_limits() -> Result {
    let mut f = Fixture::new()?;
    f.large_metadata = true;
    // The foreground watcher also loads the shipped parser profiles. Keep
    // those within the limit and make only the deliberately large record exceed it.
    f.input_limit = Some(48 * 1024);
    f.duration = 30;
    f.retries = 1;
    f.prepare().await?;
    let mut owner = f.start("fold").await?;
    wait_attempt(&f, &mut owner, 1).await?;
    assert!(owner.dream.as_mut().unwrap().next().await.is_err());
    let memories = read(&owner).await?;
    assert_eq!(memories.len(), 3);
    let survivor = memories.iter().find(|m| m.title == "fixture-0").unwrap();
    let (application, run, session, reference) = owner
        .dream
        .as_ref()
        .unwrap()
        .retained_application()
        .unwrap();
    assert!(!application.is_complete());
    assert_eq!(application.receipts().len(), 1);
    let stop = Cancellation::default();
    let original = owner.policy.request_context(Instant::now(), &stop)?;
    let key = owner.auth.runtime().check(original.snapshot())?;
    let issuer = owner.auth.issuer();
    let caller = issuer.authenticate_runtime(
        std::str::from_utf8(key.secret(original.snapshot())?)?,
        &original,
    )?;
    let parent = issuer.admin_parent(&caller, session)?;
    let (paths, id, version) = &owner.projects.as_ref().unwrap()[0];
    let captured = CapturedPublicationScope::project(paths, *id, owner.scope, original.snapshot())?;
    let record = captured.binding()?;
    let scope = || ScopeInput {
        installation: owner.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &captured,
                record: &record,
            },
            version,
        }),
        providers: &[],
        binding: NativeBinding::Codex,
        global_version: "",
    };
    let permissions = [
        Permission::Read(Module::Memory),
        Permission::WriteFencedMemory,
    ];
    let bearer = issuer.issue(&parent, run, reference, &permissions, &scope())?;
    let grant = issuer.verify(bearer.expose_for_transport(), reference, scope(), &original)?;
    let authority = ProjectRecordAuthority::new(issuer);
    let admission =
        authority.admit_write(&grant, &original, ProjectWriteOperation::FencedMemory)?;
    let mut actor = [0u8; 64];
    admission.actor()?.write_identifier(&mut actor);
    assert_eq!(survivor.updated_by, std::str::from_utf8(&actor)?);
    assert_eq!(
        fs::read_to_string(f.temp.path().join("attempts.jsonl"))?
            .lines()
            .count(),
        1,
        "retained application must not repropose"
    );
    owner.shutdown().await?;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_dream_retry_budget_and_activity_during_native_are_real() -> Result {
    for (mode, retries, expected) in [
        ("retry_first", 1, 2),
        ("exit_failure", 2, 3),
        ("slow_first", 0, 2),
        ("failed_old", 0, 2),
    ] {
        let mut f = Fixture::new()?;
        f.retries = retries;
        f.prepare().await?;
        let mut owner = f.start(mode).await?;
        let serving = serve_source(&mut owner)?;
        let before = read(&owner).await?;
        wait_attempt(&f, &mut owner, 1).await?;
        if matches!(mode, "slow_first" | "failed_old") {
            use tokio::io::AsyncWriteExt;
            let mut peer = native_ready(&f, &mut owner).await?;
            human_prompt(&f, &owner).await?;
            peer.write_all(&[1]).await?;
        }
        if mode == "exit_failure" {
            assert!(owner.dream.as_mut().unwrap().next().await.is_err());
        } else {
            if mode == "slow_first" {
                wait_completed(&mut owner).await?;
            }
            wait_attempt(&f, &mut owner, expected).await?;
            wait_completed(&mut owner).await?;
        }
        assert_eq!(
            fs::read_to_string(f.temp.path().join("attempts.jsonl"))?
                .lines()
                .count(),
            expected
        );
        assert_eq!(read(&owner).await?, before);
        owner.shutdown().await?;
        serving.await??;
    }
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_dream_deadline_keeps_auth_expiry_distinct_from_join() -> Result {
    let mut f = Fixture::new()?;
    f.duration = 1;
    f.prepare().await?;
    let mut owner = f.start("hold").await?;
    let before = read(&owner).await?;
    wait_attempt(&f, &mut owner, 1).await?;
    assert!(owner.dream.as_mut().unwrap().next().await.is_err());
    let failure = owner.shutdown().await.unwrap_err();
    let cause = failure
        .error
        .get_ref()
        .and_then(|e| e.downcast_ref::<graphit_auth::AuthError>())
        .unwrap();
    assert!(matches!(
        cause,
        graphit_auth::AuthError::Denied | graphit_auth::AuthError::Expired
    ));
    let owner = failure.owner.unwrap();
    assert_eq!(read(&owner).await?, before);
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreground_dream_disabled_does_not_grant_fenced_memory() -> Result {
    use sha2::{Digest, Sha256};
    let f = Fixture::new()?;
    f.prepare().await?;
    let stop = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &stop)?;
    let owner = ForegroundSource::start(
        &f.installation,
        &f.inputs(),
        0,
        &f.config,
        NativeBinding::Codex,
        &original,
    )?;
    assert!(owner.dream.is_none());
    let key = owner.auth.runtime().check(original.snapshot())?;
    let issuer = owner.auth.issuer();
    let caller = issuer.authenticate_runtime(
        std::str::from_utf8(key.secret(original.snapshot())?)?,
        &original,
    )?;
    let session = issuer.bootstrap_admin(&caller)?;
    let parent = issuer.admin_parent(&caller, &session)?;
    let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
        owner.auth.installation().live.owner()
    else {
        panic!("installation");
    };
    let registry = HubRegistry::global(&f.installation, installation_id, f.policy.hub())?;
    let document = registry.load()?;
    let source = &document
        .get(ArtifactType::Agent, "fixture/dream")
        .unwrap()
        .installations[0]
        .original_source;
    let body = fs::read(source.directory().as_path().join("AGENT.md"))?;
    let definition = graphit_auth::RoleDefinition::new(
        Role::Dream,
        source,
        NativeBinding::Codex,
        &[
            Permission::Read(Module::Memory),
            Permission::WriteFencedMemory,
        ],
        Sha256::digest(body).into(),
        IssuerLimits {
            max_permissions: 32,
            max_scope_bytes: 65536,
            max_parent_bearer_bytes: 65536,
        },
    )?;
    assert!(matches!(
        issuer.register_definition(&parent, definition),
        Err(graphit_auth::AuthError::Denied)
    ));
    assert!(!f.temp.path().join("attempts.jsonl").exists());
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn foreground_dream_interrupted_host_join_keeps_actual_owner() -> Result {
    let f = Fixture::new()?;
    f.prepare().await?;
    let mut owner = f.start("noop").await?;
    wait_attempt(&f, &mut owner, 1).await?;
    wait_completed(&mut owner).await?;
    owner.dream.as_ref().unwrap().cancel();
    let _terminal = owner.dream.as_mut().unwrap().next().await;
    // Poll exactly once on this current-thread runtime. The spawned Host shutdown
    // has not run yet, so dropping this pending waiter preserves its JoinHandle.
    {
        use std::{
            future::{Future, poll_fn},
            task::Poll,
        };
        let mut shutdown = Box::pin(owner.dream.as_mut().unwrap().shutdown());
        let pending = poll_fn(|context| Poll::Ready(shutdown.as_mut().poll(context))).await;
        assert!(matches!(pending, Poll::Pending));
    }
    owner.dream.as_mut().unwrap().shutdown().await?;
    owner.shutdown().await?;
    Ok(())
}

async fn cab0_cli(
    installation: &InstallationPaths,
    project: &ProjectPaths,
    arguments: &[&str],
) -> Result<Vec<u8>> {
    let app = std::env::var_os("GRAPHIT_TEST_APP")
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let mut command = tokio::process::Command::new(app);
    command
        .arg("--global-dir")
        .arg(installation.root())
        .args(arguments)
        .arg("--project-dir")
        .arg(project.root())
        .arg("--project-version")
        .arg("main")
        .env("HOME", installation.root())
        .env_remove("GRAPHIT_ROLE_BEARER")
        .env_remove("GRAPHIT_ROLE_REFERENCE")
        .env_remove("GRAPHIT_ROLE_SELECTION")
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output()).await??;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "cab0 CLI {} failed with {:?}",
            arguments[0],
            output.status.code()
        ))
        .into());
    }
    Ok(output.stdout)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "cab0 real two-project daemon, compiled Memory CLI and native Dream protocol peer"]
async fn foreground_dream_two_projects_memory_cli_cancel_and_join() -> Result {
    use crate::foreground_project_control::{ProjectCommand, send};
    use futures_util::FutureExt;
    use graphit_core::config::ConfigStore;
    let f = Fixture::new()
        .map_err(|error| io::Error::other(format!("initialize two-project fixture: {error}")))?;
    let b_root = f.temp.path().join("project-b");
    fs::create_dir(&b_root)?;
    fs::create_dir(b_root.join(".codex"))?;
    fs::copy(
        f.project.root().join(".codex/config.toml"),
        b_root.join(".codex/config.toml"),
    )?;
    let b = ProjectPaths::at(&b_root, &f.installation)
        .map_err(|error| io::Error::other(format!("capture project B paths: {error}")))?;
    let cancellation = Cancellation::default();
    ProjectIdentityStore::at(&b)
        .writer(Duration::from_secs(5), &cancellation)
        .map_err(|error| io::Error::other(format!("open project B metadata writer: {error}")))?
        .ensure(
            &ProjectMetadata::new("cab0-b").map_err(|error| {
                io::Error::other(format!("validate project B metadata: {error}"))
            })?,
        )
        .map_err(|error| io::Error::other(format!("ensure project B metadata: {error}")))?;
    for project in [&f.project, &b] {
        ConfigStore::project(project)
            .map_err(|error| io::Error::other(format!("open PROJECT Memory config: {error}")))?
            .set(
                ConfigKey::new("modules.memory").map_err(|error| {
                    io::Error::other(format!("capture PROJECT Memory config key: {error}"))
                })?,
                json!(true),
                Duration::from_secs(5),
                &cancellation,
            )
            .map_err(|error| io::Error::other(format!("set PROJECT Memory config: {error}")))?;
    }
    for (key, value) in [
        ("modules.memory", false),
        ("modules.embedding", false),
        ("modules.task", false),
        ("modules.hooks", true),
    ] {
        ConfigStore::global(&f.installation)
            .map_err(|error| io::Error::other(format!("open GLOBAL config: {error}")))?
            .set(
                ConfigKey::new(key).map_err(|error| {
                    io::Error::other(format!("capture GLOBAL config key {key}: {error}"))
                })?,
                json!(value),
                Duration::from_secs(5),
                &cancellation,
            )
            .map_err(|error| io::Error::other(format!("set GLOBAL config {key}: {error}")))?;
    }
    let inputs = [
        ProjectBootstrapInput {
            project: &f.project,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
        ProjectBootstrapInput {
            project: &b,
            explicit_version: "main",
            pin: None,
            detached_base: "",
        },
    ];
    let request = f.policy.request_context(Instant::now(), &cancellation)?;
    let prepared = ForegroundSource::start(
        &f.installation,
        &inputs,
        0,
        &f.config,
        NativeBinding::Codex,
        &request,
    )
    .map_err(|error| io::Error::other(format!("start preparation owner: {error}")))?;
    publish_agent(&prepared, &f, &request)
        .map_err(|error| io::Error::other(format!("publish GLOBAL Dream Agent: {error}")))?;
    let before_a = seed_project(&prepared, 0, 4, false)
        .await
        .map_err(|error| io::Error::other(format!("seed PROJECT A Memory: {error}")))?;
    let before_b = seed_project(&prepared, 1, 4, false)
        .await
        .map_err(|error| io::Error::other(format!("seed PROJECT B Memory: {error}")))?;
    let assert_unchanged = |actual: Vec<Memory>, expected: &[Memory]| {
        assert_eq!(actual.len(), expected.len(), "Memory record count changed");
        for record in expected {
            assert_eq!(
                actual.iter().find(|stored| stored.id == record.id),
                Some(record),
                "Memory record changed"
            );
        }
    };
    assert!(
        before_a
            .iter()
            .all(|a| before_b.iter().all(|b| a.id != b.id))
    );
    prepared
        .shutdown()
        .await
        .map_err(|error| io::Error::other(format!("join preparation owner: {error}")))?;
    let mut observations = Vec::new();
    let mut attempts = 0usize;
    for mode in ["fold", "hold"] {
        let owner = f
            .start_inputs(&inputs, mode)
            .await
            .map_err(|error| io::Error::other(format!("start {mode} Dream owner: {error}")))?;
        let address = owner
            .address()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?;
        let native = owner.native;
        let auth = Arc::clone(&owner.auth);
        let projects = Arc::clone(owner.projects.as_ref().unwrap());
        let policy = owner.policy.clone();
        let scope = owner.scope;
        let read_limits = limits(&owner).0;
        let completion = owner.dream.as_ref().unwrap().iteration_completion();
        let initial = completion.notified();
        tokio::pin!(initial);
        initial.as_mut().enable();
        let activity = owner
            .dream_activity
            .read()
            .map_err(|_| io::Error::other("activity state"))?
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
            .1
            .subscribe();
        let service = Cancellation::default();
        let run_signal = service.clone();
        let mut running = tokio::spawn(async move { owner.run(&run_signal).await });
        let mut exercise_phase = "wait native request";
        let exercise = std::panic::AssertUnwindSafe(async {
            let target = attempts + 1;
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                let count = fs::read_to_string(f.temp.path().join("attempts.jsonl")).map(|s| s.lines().count()).unwrap_or(0);
                if count >= target { break; }
                assert!(!running.is_finished(), "daemon stopped before the native Dream request");
                assert!(Instant::now() < deadline, "native Dream did not receive its authorized corpus");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            exercise_phase = "verify B Memory snapshot";
            assert_unchanged(read_records(&auth, &policy, scope, &projects[1], read_limits).await?, &before_b);
            if mode == "fold" {
                exercise_phase = "await initial fold";
                tokio::time::timeout(Duration::from_secs(30), initial.as_mut()).await?;
                exercise_phase = "read folded A";
                let after_a = read_records(&auth, &policy, scope, &projects[0], read_limits).await?;
                assert_eq!(after_a.len(), 2);
                assert_eq!(after_a.iter().find(|m| m.title == "fixture-0").unwrap().body, "confirmed fact 0; confirmed fact 1");
                exercise_phase = "verify B Memory snapshot";
            assert_unchanged(read_records(&auth, &policy, scope, &projects[1], read_limits).await?, &before_b);
                exercise_phase = "B CLI human prompt";
                let output = cab0_cli(&f.installation, &b, &["_session-hook", "--engine", "kiro", "--event", "UserPromptSubmit"]).await?;
                assert!(!output.is_empty());
                assert!(!activity.has_changed()?, "B human activity must not wake A Dream");
                tokio::time::sleep(Duration::from_millis(1200)).await;
                assert_eq!(fs::read_to_string(f.temp.path().join("attempts.jsonl"))?.lines().count(), target);
                let next = completion.notified();
                tokio::pin!(next);
                next.as_mut().enable();
                exercise_phase = "A CLI human prompt";
                cab0_cli(&f.installation, &f.project, &["_session-hook", "--engine", "kiro", "--event", "UserPromptSubmit"]).await?;
                assert!(activity.has_changed()?, "A human activity must reach its captured Dream");
                exercise_phase = "await A human-triggered fold";
                tokio::time::timeout(Duration::from_secs(30), next.as_mut()).await?;
                exercise_phase = "verify B Memory snapshot";
            assert_unchanged(read_records(&auth, &policy, scope, &projects[1], read_limits).await?, &before_b);
                attempts = target + 1;
                observations.push(json!({"name":"dream_fold_and_human_activity_are_project_isolated","project_version":"main","a_before":4,"a_after":2,"b_unchanged":4}));
            } else {
                exercise_phase = "read A before retirement";
                let before_cancel = read_records(&auth, &policy, scope, &projects[0], read_limits).await?;
                for retiring in [0usize, 1] {
                    let command = ProjectCommand::RetireCheckout { project_id: projects[retiring].1, project_dir: NativePath::new(projects[retiring].0.root().to_owned())? };
                    let http = reqwest::Client::new();
                    let stop = Cancellation::default();
                    let request = policy.request_context(Instant::now(), &stop)?;
                    exercise_phase = if retiring == 0 { "retire A HTTP" } else { "retire B HTTP" };
                    let (mut status, mut response) = send(&f.installation, address, &http, (native, policy.content()), &command, &request).await?;
                    let reported_interruption = status == axum::http::StatusCode::SERVICE_UNAVAILABLE && response["phase"] == "dream_join";
                    if reported_interruption {
                        assert!(!running.is_finished(), "retirement stopped the daemon before retained cleanup could resume");
                        let request = policy.request_context(Instant::now(), &stop)?;
                        exercise_phase = if retiring == 0 { "retry retained A retirement HTTP" } else { "retry retained B retirement HTTP" };
                        (status, response) = send(&f.installation, address, &http, (native, policy.content()), &command, &request).await?;
                    }
                    assert!(status.is_success(), "checkout retirement phase {}", response["phase"]);
                    assert!(!running.is_finished(), "retiring one checkout stopped the daemon");
                    exercise_phase = "verify A Memory after retirement";
                    assert_unchanged(read_records(&auth, &policy, scope, &projects[0], read_limits).await?, &before_cancel);
                    exercise_phase = "verify B Memory snapshot";
            assert_unchanged(read_records(&auth, &policy, scope, &projects[1], read_limits).await?, &before_b);
                    if retiring == 0 {
                        exercise_phase = "B Memory CLI index refresh and JSON output";
                        let indexed: serde_json::Value = serde_json::from_slice(&cab0_cli(&f.installation, &b, &["memory", "index", "--refresh"]).await?)?;
                        assert_eq!(indexed["scope"], "project");
                        assert_eq!(indexed["lexical"]["state"], "published");
                        assert!(indexed["lexical"]["recovery_key"].is_object());
                        assert!(indexed["lexical"]["generation"]["sdk_version"].as_u64().is_some_and(|v| v > 0));
                        assert!(!running.is_finished(), "B Memory maintenance stopped the daemon");
                        exercise_phase = "verify B Memory snapshot";
            assert_unchanged(read_records(&auth, &policy, scope, &projects[1], read_limits).await?, &before_b);
                        observations.push(json!({"name":"retiring_blocked_A_joins_Dream_and_B_Memory_CLI_continues","reported_interruption_recovered":reported_interruption,"b_unchanged":4}));
                    } else {
                        observations.push(json!({"name":"last_project_retirement_completes","reported_interruption_recovered":reported_interruption}));
                    }
                }
            }
            exercise_phase = "exercise completed";
            Result::Ok(())
        }).catch_unwind().await;
        eprintln!(
            "cab0 exercise mode={mode} phase={exercise_phase} passed={}",
            matches!(&exercise, Ok(Ok(())))
        );
        if let Ok(Err(error)) = &exercise {
            eprintln!(
                "cab0 exercise failure mode={mode} phase={exercise_phase} error={error:?} run_finished_before_cancel={}",
                running.is_finished()
            );
        }
        service.cancel();
        let (joined, missed_join_deadline) =
            match tokio::time::timeout(Duration::from_secs(45), &mut running).await {
                Ok(joined) => (joined?, false),
                Err(_) => (running.await?, true),
            };
        if let Err(mut failure) = joined {
            let runtime = failure.runtime.as_ref().map(|outcome| {
                (
                    outcome.effects(),
                    outcome.failure(),
                    outcome.cleanup_failure(),
                    outcome.generation(),
                )
            });
            let service_kind = failure.service.as_ref().map(io::Error::kind);
            eprintln!(
                "cab0 original run failure mode={mode} error={:?} service_kind={service_kind:?} runtime={runtime:?}",
                failure.error
            );
            if let Some(retained) = failure.owner.take() {
                retained.shutdown().await?;
            }
            return Err(failure.error.into());
        }
        if missed_join_deadline {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        match exercise {
            Ok(result) => result?,
            Err(panic) => std::panic::resume_unwind(panic),
        }
        assert!(
            tokio::net::TcpStream::connect(address).await.is_err(),
            "owned daemon listener survived its joined shutdown"
        );
    }
    observations.push(json!({"name":"native_daemon_owned_processes_and_listener_joined"}));
    if let Some(path) = std::env::var_os("GRAPHIT_CAB0_RECORDS_RESULT") {
        fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"task":"tsk-cab0","status":"passed_stage","coverage":"native_daemon_project_memory_cli_dream_isolation_cancel_join","observations":observations,"owned_processes_joined":true}),
            )?,
        )?;
    }
    Ok(())
}

#[path = "foreground_user_dream_tests.rs"]
mod user;
