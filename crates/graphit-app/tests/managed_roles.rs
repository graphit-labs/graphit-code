#![recursion_limit = "256"]
//! Actual compiled install/sync commands, daemon issuer, retained tuples and native writer.
use graphit_app::{
    foreground_source::ForegroundSource,
    managed_roles::{InstallOutcome, InstallRequest, ManagedRoleName},
};
use graphit_core::{
    brand::Brand,
    config::ConfigSnapshot,
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::{project_bootstrap::ProjectBootstrapInput, source_discovery::read_discovery};
use graphit_hub::{NativeBinding, policy::HubResourcePolicy};
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};
struct Fixture {
    temp: tempfile::TempDir,
    installation: InstallationPaths,
    project: ProjectPaths,
    config: ConfigSnapshot,
    policy: HubResourcePolicy,
}
impl Fixture {
    fn new() -> Self {
        Self::with_brand(Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap())
    }
    fn with_brand(brand: Brand) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let installation = InstallationPaths::resolve(
            brand.clone(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let project = ProjectPaths::at(&root, &installation).unwrap();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .ensure(&ProjectMetadata::new("managed-role-project").unwrap())
            .unwrap();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        Self {
            temp,
            installation,
            project,
            config,
            policy,
        }
    }
    fn start(
        &self,
    ) -> (
        Cancellation,
        tokio::task::JoinHandle<Result<(), graphit_app::foreground_source::ForegroundFailure>>,
    ) {
        let cancel = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancel)
            .unwrap();
        let owner = ForegroundSource::start(
            &self.installation,
            &[ProjectBootstrapInput {
                project: &self.project,
                explicit_version: "main",
                pin: None,
                detached_base: "",
            }],
            0,
            &self.config,
            NativeBinding::Codex,
            &original,
        )
        .unwrap();
        let stop = Cancellation::default();
        let retained = stop.clone();
        (
            stop,
            tokio::spawn(async move { owner.run(&retained).await }),
        )
    }
    fn limits(&self) -> NativeDocumentFileLimits {
        NativeDocumentFileLimits::new(
            self.policy.content().max_input_bytes,
            graphit_core::native_path::MAX_PATH_BYTES,
            128,
            self.policy.content().max_scalar_bytes,
        )
        .unwrap()
    }
    async fn executable(&self, args: &[&str]) -> std::process::Output {
        let root = self.installation.root().to_owned();
        let project = self.project.root().to_owned();
        let arguments: Vec<_> = args.iter().map(|v| (*v).to_owned()).collect();
        tokio::task::spawn_blocking(move || {
            std::process::Command::new(env!("CARGO_BIN_EXE_graphit"))
                .env_clear()
                .current_dir(project)
                .arg("--global-dir")
                .arg(root)
                .args(arguments)
                .output()
                .unwrap()
        })
        .await
        .unwrap()
    }
    fn endpoint(&self) -> String {
        let cancel = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancel)
            .unwrap();
        let discovery = read_discovery(&self.installation, self.limits(), original.snapshot())
            .unwrap()
            .unwrap();
        format!(
            "http://{}{}",
            discovery.address(),
            graphit_app::managed_roles::http::ROUTE
        )
    }
    fn input(&self, binding: NativeBinding, roles: Vec<ManagedRoleName>) -> InstallRequest {
        InstallRequest {
            destination: graphit_app::managed_roles::InstallDestination::Project,
            project_dir: Some(self.project.root().to_owned()),
            project_version: None,
            binding: Some(binding),
            roles,
            skills_only: false,
            rules: vec![],
            skill_aliases: BTreeMap::new(),
            skill_bundles: Default::default(),
            module_skill: None,
        }
    }
    fn agent(&self, role: &str) -> std::path::PathBuf {
        self.project
            .root()
            .join(".codex/agents")
            .join(format!("{}-{role}.toml", self.project.brand().key()))
    }
    async fn stop(
        &self,
        stop: Cancellation,
        job: tokio::task::JoinHandle<Result<(), graphit_app::foreground_source::ForegroundFailure>>,
    ) {
        stop.cancel();
        job.await.unwrap().unwrap();
        assert!(!self.installation.root().join("daemon/mcp.key").exists());
    }
}
fn outcome(output: &std::process::Output) -> InstallOutcome {
    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_install_sync_three_roles_and_captured_raw_precedence() {
    let f = Fixture::new();
    let (stop, job) = f.start();
    let key = fs::read(f.installation.root().join("daemon/mcp.key")).unwrap();
    let first = outcome(&f.executable(&["roles", "install"]).await);
    assert!(first.failure.is_none());
    assert_eq!(first.roles.len(), 3);
    for role in ["scout", "tracker", "scribe"] {
        let bytes = fs::read_to_string(f.agent(role)).unwrap();
        assert!(bytes.contains("developer_instructions"));
        assert!(!bytes.contains("sandbox_mode"));
        assert!(bytes.contains(if role == "tracker" {
            "shell_tool = true"
        } else {
            "shell_tool = false"
        }));
        if role == "scout" {
            assert!(bytes.contains("without changing records or code"));
        }
        assert!(bytes.contains(&format!("{}-{role}", f.project.brand().key())));
    }
    let synced = outcome(&f.executable(&["roles", "sync"]).await);
    assert!(synced.failure.is_none());
    assert!(
        synced
            .roles
            .iter()
            .all(|effect| effect.publication.as_deref() == Some("Unchanged"))
    );
    let foreign = f.project.root().join(".codex/agents/foreign.toml");
    fs::write(&foreign, "unrelated native field/file").unwrap();
    let global = f.installation.root().join("rules/roles/scout.md");
    fs::create_dir_all(global.parent().unwrap()).unwrap();
    fs::write(
        &global,
        "GLOBAL {{ agent_name }}\n{% include 'defaults/roles/scout.md' %}\n",
    )
    .unwrap();
    let global_out = outcome(&f.executable(&["roles", "sync", "--role", "scout"]).await);
    assert_eq!(global_out.roles[0].origin, "Global");
    assert!(
        fs::read_to_string(f.agent("scout"))
            .unwrap()
            .contains("GLOBAL")
    );
    let project = f.project.brand_directory().join("rules/roles/scout.md");
    fs::create_dir_all(project.parent().unwrap()).unwrap();
    fs::write(
        &project,
        "PROJECT {{ agent_name }} {{ mcp_tool_module_skill }} {{ skill_names | join(',') }}\n",
    )
    .unwrap();
    let project_out = outcome(
        &f.executable(&[
            "roles",
            "sync",
            "--role",
            "scout",
            "--skill-alias",
            "task=actual-task",
            "--module-skill",
            "actual_module_skill",
        ])
        .await,
    );
    assert_eq!(project_out.roles[0].origin, "Project");
    let bytes = fs::read_to_string(f.agent("scout")).unwrap();
    assert!(
        bytes.contains("actual-task")
            && bytes.contains("actual_module_skill")
            && bytes.contains("PROJECT")
    );
    let preserved = outcome(&f.executable(&["roles", "sync", "--role", "scout"]).await);
    assert!(preserved.failure.is_none());
    assert_eq!(fs::read_to_string(f.agent("scout")).unwrap(), bytes);
    let skills = outcome(&f.executable(&["skills", "sync"]).await);
    assert!(skills.failure.is_none());
    assert!(skills.roles.is_empty());
    assert_eq!(skills.skills.len(), 5);
    assert_eq!(fs::read_to_string(f.agent("scout")).unwrap(), bytes);
    fs::write(&project, "{{ missing_effective_name }}").unwrap();
    let rejected = f.executable(&["roles", "sync", "--role", "scout"]).await;
    assert!(!rejected.status.success());
    let result: InstallOutcome = serde_json::from_slice(&rejected.stdout).unwrap();
    assert!(result.failure.is_some());
    assert_eq!(fs::read_to_string(f.agent("scout")).unwrap(), bytes);
    assert_eq!(
        fs::read_to_string(&foreign).unwrap(),
        "unrelated native field/file"
    );
    assert_eq!(
        fs::read(f.installation.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    f.stop(stop, job).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn general_sync_cli_mcp_restores_hooks_and_preserves_user_changes() {
    let f = Fixture::new();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(f.project.root())
            .status()
            .unwrap()
            .success()
    );
    graphit_core::config::ConfigStore::project(&f.project)
        .unwrap()
        .set(
            graphit_core::config::ConfigKey::new("githooks").unwrap(),
            serde_json::json!({"pre-commit":["printf configured"]}),
            std::time::Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    let registered = |event: &str| {
        std::process::Command::new("git")
            .args([
                "config",
                "--local",
                "--get",
                &format!("hook.graphit--{event}.command"),
            ])
            .current_dir(f.project.root())
            .output()
            .unwrap()
    };
    let (stop, job) = f.start();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../templates/agent-instructions/coordinator");
    let source = source.canonicalize().unwrap();
    let args = [
        "sync",
        "--agent",
        "codex",
        "--coordinator-source",
        source.to_str().unwrap(),
    ];
    let first = f.executable(&args).await;
    assert!(
        first.status.success(),
        "{} {}",
        String::from_utf8_lossy(&first.stderr),
        String::from_utf8_lossy(&first.stdout)
    );
    let first: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["operation"], "sync");
    assert_eq!(first["status"], "ok");
    assert_eq!(first["steps"]["native"]["git_hooks"]["state"], "reconciled");
    assert_eq!(
        registered("pre-commit").stdout,
        b"graphit git hook pre-commit --\n"
    );
    assert!(registered("post-commit").status.success());
    assert_eq!(
        first["steps"]["native"]["roles_skills"]["roles"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let config_path = f.project.root().join(".codex/config.toml");
    let config = fs::read_to_string(&config_path).unwrap();
    assert!(config.contains("mcp_servers"));
    let hooks_path = f.project.root().join(".codex/hooks.json");
    let hooks = fs::read_to_string(&hooks_path).unwrap();
    assert!(hooks.contains("_session-hook"));
    fs::remove_file(&hooks_path).unwrap();
    let second = f.executable(&args).await;
    assert!(
        second.status.success(),
        "{} {}",
        String::from_utf8_lossy(&second.stderr),
        String::from_utf8_lossy(&second.stdout)
    );
    assert_eq!(fs::read_to_string(&config_path).unwrap(), config);
    assert_eq!(fs::read_to_string(&hooks_path).unwrap(), hooks);
    graphit_core::config::ConfigStore::project(&f.project)
        .unwrap()
        .set(
            graphit_core::config::ConfigKey::new("githooks").unwrap(),
            serde_json::json!({"pre-push":["printf updated"]}),
            std::time::Duration::from_secs(5),
            &Cancellation::default(),
        )
        .unwrap();
    // The MCP tool is the same producer; it retains the original Runtime authority.
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let mut output = Vec::new();
    let prepared = rmcp::model::CallToolRequestParams::new("graphit_sync").with_arguments(
        serde_json::json!({
            "agent":"codex", "project_dir":f.project.root(), "coordinator_source":source
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    graphit_app::hub_publish::call(
        &f.installation,
        prepared,
        f.limits(),
        &original,
        &mut output,
    )
    .await
    .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["status"], "ok");
    assert_eq!(
        result["steps"]["native"]["git_hooks"]["state"],
        "reconciled"
    );
    assert!(!registered("pre-commit").status.success());
    assert_eq!(
        registered("pre-push").stdout,
        b"graphit git hook pre-push --\n"
    );
    // User content must not be overwritten, and the failed step remains observable.
    let edited = f.agent("scout");
    fs::write(&edited, "user edited role").unwrap();
    let rejected = f.executable(&args).await;
    assert!(!rejected.status.success());
    assert_eq!(fs::read_to_string(&edited).unwrap(), "user edited role");
    let result: serde_json::Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(result["status"], "error");
    assert_eq!(result["outcome"]["failed_step"], "native");
    f.stop(stop, job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_publication_choices_survive_a_later_skill_conflict() {
    let f = Fixture::new();
    let (stop, job) = f.start();
    let foreign = f.project.root().join(".agents/skills/custom-task");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("SKILL.md"), "user instructions").unwrap();
    let first = f
        .executable(&["roles", "install", "--skill-alias", "task=custom-task"])
        .await;
    assert!(!first.status.success());
    assert!(
        fs::read_to_string(f.agent("scout"))
            .unwrap()
            .contains("custom-task")
    );
    let repeated = f.executable(&["roles", "sync"]).await;
    assert!(!repeated.status.success());
    assert!(
        fs::read_to_string(f.agent("scout"))
            .unwrap()
            .contains("custom-task")
    );
    assert_eq!(
        fs::read_to_string(foreign.join("SKILL.md")).unwrap(),
        "user instructions"
    );
    f.stop(stop, job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_runtime_denial_foreign_files_partial_publication_and_readonly_client() {
    let f = Fixture::new();
    let probe_cancel = Cancellation::default();
    let probe = f
        .policy
        .request_context(Instant::now(), &probe_cancel)
        .unwrap();
    let prepared = graphit_app::managed_roles::client::prepare(
        graphit_app::managed_roles::client::Arguments {
            command: graphit_app::managed_roles::client::Command::Install(
                graphit_app::managed_roles::client::Selection {
                    destination: graphit_app::managed_roles::InstallDestination::Project,
                    source_project: None,
                    agent: None,
                    role: vec![],
                    rule: vec![],
                    skill_alias: vec![],
                    skill_bundle: vec![],
                    skill_identity_provider: vec![],
                    module_skill: None,
                },
            ),
        },
        Some(&f.project),
        None,
        &f.policy,
        &probe,
    )
    .unwrap();
    let mut probe_output = Vec::new();
    let absent = graphit_app::managed_roles::client::run(
        prepared,
        &f.installation,
        &f.policy,
        &probe,
        &mut probe_output,
    )
    .await
    .unwrap_err();
    assert_eq!(absent.kind(), std::io::ErrorKind::NotFound);
    assert!(probe_output.is_empty());
    assert!(!f.installation.root().join("daemon/mcp.key").exists());
    let invalid_run = f
        .executable(&[
            "roles",
            "run",
            "--agent",
            "kiro-v3",
            "--role",
            "scout",
            "--prompt",
            "Validate local input",
            "--skill-bundle",
            "task=fixtures/task-instructions@one",
        ])
        .await;
    assert!(!invalid_run.status.success());
    assert!(invalid_run.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&invalid_run.stderr)
            .contains("skill bundles require roles install or sync")
    );
    assert!(!f.installation.root().join("daemon/mcp.key").exists());
    assert!(!f.project.root().join(".kiro/agents").exists());
    let (stop, job) = f.start();
    let key = fs::read(f.installation.root().join("daemon/mcp.key")).unwrap();
    let input = f.input(NativeBinding::Codex, vec![ManagedRoleName::Scout]);
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let response = client.post(f.endpoint()).json(&input).send().await.unwrap();
    assert_eq!(response.status(), 401);
    let response = client
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "local")
        .bearer_auth(std::str::from_utf8(&key).unwrap())
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let denied: InstallOutcome = client
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth("foreign-key")
        .json(&input)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(denied.failure.is_some());
    assert!(denied.roles.is_empty());
    assert!(!f.agent("scout").exists());
    fs::create_dir_all(f.agent("scout").parent().unwrap()).unwrap();
    fs::write(f.agent("scout"), "FOREIGN OWN NAME").unwrap();
    let refused = f.executable(&["roles", "install", "--role", "scout"]).await;
    assert!(!refused.status.success());
    let result: InstallOutcome = serde_json::from_slice(&refused.stdout).unwrap();
    assert!(result.failure.is_some());
    assert!(result.roles[0].definition_configured);
    assert_eq!(result.roles[0].publication, None);
    assert_eq!(
        fs::read_to_string(f.agent("scout")).unwrap(),
        "FOREIGN OWN NAME"
    );
    let kiro_agents = f.project.root().join(".kiro/agents");
    fs::create_dir_all(&kiro_agents).unwrap();
    let tracker = kiro_agents.join(format!("{}-tracker.md", f.project.brand().key()));
    fs::write(&tracker, "FOREIGN TRACKER").unwrap();
    let partial = f.input(
        NativeBinding::KiroV3,
        vec![ManagedRoleName::Scout, ManagedRoleName::Tracker],
    );
    let result: InstallOutcome = client
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(std::str::from_utf8(&key).unwrap())
        .json(&partial)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(result.failure.is_some());
    assert_eq!(result.roles.len(), 2);
    assert_eq!(result.roles[0].publication.as_deref(), Some("Created"));
    assert!(result.roles[1].definition_configured);
    assert_eq!(result.roles[1].publication, None);
    assert_eq!(fs::read_to_string(&tracker).unwrap(), "FOREIGN TRACKER");
    assert!(
        f.project
            .root()
            .join(".kiro/agents")
            .join(format!("{}-scout.md", f.project.brand().key()))
            .is_file()
    );
    fs::remove_file(&tracker).unwrap();
    let supported: InstallOutcome = client
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(std::str::from_utf8(&key).unwrap())
        .json(&partial)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(supported.failure.is_none());
    assert_eq!(supported.roles.len(), 2);
    assert_eq!(supported.roles[0].publication.as_deref(), Some("Unchanged"));
    assert_eq!(supported.roles[1].publication.as_deref(), Some("Created"));
    let tracker_text = fs::read_to_string(&tracker).unwrap();
    let header = tracker_text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap()
        .0;
    let tracker_json: serde_json::Value = serde_saphyr::from_str(header).unwrap();
    assert_eq!(
        tracker_json["name"],
        format!("{}-tracker", f.project.brand().key())
    );
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let prepared = graphit_app::managed_roles::client::prepare(
        graphit_app::managed_roles::client::Arguments {
            command: graphit_app::managed_roles::client::Command::Sync(
                graphit_app::managed_roles::client::Selection {
                    destination: graphit_app::managed_roles::InstallDestination::Project,
                    source_project: None,
                    agent: None,
                    role: vec![],
                    rule: vec![],
                    skill_alias: vec![],
                    skill_bundle: vec![],
                    skill_identity_provider: vec![],
                    module_skill: None,
                },
            ),
        },
        Some(&f.project),
        None,
        &f.policy,
        &original,
    )
    .unwrap();
    cancel.cancel();
    let mut output = Vec::new();
    let error = graphit_app::managed_roles::client::run(
        prepared,
        &f.installation,
        &f.policy,
        &original,
        &mut output,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    assert!(output.is_empty());
    assert_eq!(
        fs::read(f.installation.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    f.stop(stop, job).await;
    assert!(f.temp.path().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn installed_hub_rule_is_captured_and_project_override_wins() {
    use graphit_core::installation_identity::InstallationIdentityStore;
    use graphit_hub::{
        ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, SourceRef,
        registry::{
            HubRegistry, Installation, InstalledArtifact, InstalledBackend, InstalledPayload,
        },
    };
    let f = Fixture::new();
    let (stop, job) = f.start();
    let settings = InstallationIdentityStore::new(&f.installation)
        .unwrap()
        .load()
        .unwrap();
    let registry =
        HubRegistry::global(&f.installation, settings.unit_id().unwrap(), f.policy.hub()).unwrap();
    let id = ArtifactId::new("roles/custom", f.policy.projection()).unwrap();
    let version = ConcreteVersion::new("v1", f.policy.projection()).unwrap();
    let identity = ProjectIdentityStore::at(&f.project)
        .load()
        .unwrap()
        .unwrap();
    let source = SourceRef::resources(
        &f.project,
        identity.id(),
        ArtifactType::Rule,
        id.clone(),
        version,
        f.policy.projection(),
    )
    .unwrap();
    fs::create_dir_all(source.directory().as_path().join("roles")).unwrap();
    let raw = source.directory().as_path().join("roles/scout.md");
    fs::write(
        &raw,
        "HUB {{ agent_name }}\n{% include 'defaults/roles/scout.md' %}\n",
    )
    .unwrap();
    let cancellation = Cancellation::default();
    registry
        .writer(Duration::from_secs(5), &cancellation)
        .unwrap()
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: ArtifactType::Rule,
            id,
            installations: vec![Installation {
                target: InstallationTarget::for_artifact(NativeBinding::Codex, ArtifactType::Rule),
                original_source: source,
                alias: None,
                entry_name: None,
                display: None,
                backend: InstalledBackend::DynamicContext,
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })
        .unwrap();
    let args = [
        "roles",
        "install",
        "--role",
        "scout",
        "--rule",
        "scout=roles/custom@v1",
    ];
    let installed = outcome(&f.executable(&args).await);
    assert_eq!(installed.roles[0].origin, "Hub");
    assert!(
        fs::read_to_string(f.agent("scout"))
            .unwrap()
            .contains("HUB")
    );
    let project = f.project.brand_directory().join("rules/roles/scout.md");
    fs::create_dir_all(project.parent().unwrap()).unwrap();
    fs::write(&project, "PROJECT {{ agent_name }}\n").unwrap();
    let overridden = outcome(&f.executable(&args).await);
    assert_eq!(overridden.roles[0].origin, "Project");
    fs::write(&raw, "{{ missing_selected_hub_value }}").unwrap();
    fs::remove_file(&project).unwrap();
    let previous = fs::read(f.agent("scout")).unwrap();
    let rejected = f.executable(&args).await;
    assert!(!rejected.status.success());
    let rejected: InstallOutcome = serde_json::from_slice(&rejected.stdout).unwrap();
    assert!(rejected.failure.is_some());
    assert!(rejected.roles.is_empty());
    assert_eq!(fs::read(f.agent("scout")).unwrap(), previous);
    f.stop(stop, job).await;
}

#[path = "managed_roles/compiled_skills.rs"]
mod compiled_skills;

#[path = "managed_roles/global_publication.rs"]
mod global_publication;
