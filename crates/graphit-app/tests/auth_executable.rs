//! Actual executable: installation-only Local authentication and native cleanup.
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigStore},
    file_lock::Cancellation,
    project_paths::InstallationPaths,
};
use serde_json::{Value, json};
use std::{
    fs,
    process::{Command, Output},
    time::Duration,
};
struct Fixture {
    _temp: tempfile::TempDir,
    paths: InstallationPaths,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallationPaths::resolve(
            Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap(),
            temp.path(),
            None,
            Some(&temp.path().join("installation")),
        )
        .unwrap();
        Self { _temp: temp, paths }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_graphit"));
        c.current_dir(self._temp.path())
            .env_clear()
            .arg("--global-dir")
            .arg(self.paths.root())
            .args(args);
        c
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn publish(&self, args: &[&str]) {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"Applied\n");
        assert!(out.stderr.is_empty());
        assert!(!self.paths.root().join("daemon/mcp.key").exists());
    }
    fn read(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        serde_json::from_slice(&out.stdout).unwrap()
    }
}
#[test]
fn local_lifecycle_outside_project_preserves_native_cas_redaction_and_cleanup() {
    let f = Fixture::new();
    f.publish(&["provider", "add", "local", "--type", "local"]);
    for name in ["alice", "bob"] {
        f.publish(&[
            "login",
            "--provider",
            "local",
            "--profile",
            name,
            "--username",
            name,
            "--mcp-key",
            "private-auth-executable-canary",
        ]);
    }
    let accounts = f.read(&["account", "list"]);
    assert_eq!(accounts["active"], "bob");
    assert_eq!(accounts["profiles"].as_array().unwrap().len(), 2);
    assert!(
        !accounts
            .to_string()
            .contains("private-auth-executable-canary")
    );
    assert!(!accounts.to_string().contains("mcp_key"));
    f.publish(&["account", "use", "alice"]);
    assert_eq!(f.read(&["account", "show"])["profile"]["name"], "alice");
    let profile = f.read(&["account", "show"]);
    assert!(profile["profile"]["created_at"].as_i64().is_some());
    assert!(profile["profile"]["updated_at"].as_i64().is_some());
    let bytes = fs::read(f.paths.auth_file()).unwrap();
    let failed = f.run(&[
        "provider",
        "update",
        "local",
        "--expected-revision",
        "0",
        "--rerank-mode",
        "disabled",
    ]);
    assert!(!failed.status.success());
    assert_eq!(fs::read(f.paths.auth_file()).unwrap(), bytes);
    f.publish(&["logout"]);
    assert!(f.read(&["account", "show"])["profile"].is_null());
    f.publish(&["provider", "update", "local", "--rerank-mode", "disabled"]);
    assert_eq!(
        f.read(&["provider", "show", "local"])["config"]["rerank"]["mode"],
        "disabled"
    );
    f.publish(&["provider", "remove", "local", "--yes", "--cascade"]);
    assert!(
        f.read(&["provider", "list"])["providers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(f.read(&["provider", "list"])["revision"].as_u64().unwrap() > 1);
}
#[test]
fn global_configuration_is_native_and_project_agent_and_version_are_irrelevant() {
    let f = Fixture::new();
    fs::create_dir(f._temp.path().join(f.paths.brand().directory())).unwrap();
    fs::write(
        f._temp
            .path()
            .join(f.paths.brand().directory())
            .join("project.json"),
        b"invalid unrelated project config",
    )
    .unwrap();
    let cancel = Cancellation::default();
    ConfigStore::global(&f.paths)
        .unwrap()
        .set(
            ConfigKey::new("agent").unwrap(),
            json!("invalid-agent"),
            Duration::from_secs(3),
            &cancel,
        )
        .unwrap();
    let mut command = f.command(&[
        "--non-interactive",
        "--project-version",
        "ignored-auth-version",
        "provider",
        "list",
    ]);
    // Auth does not select a project version.
    command.env(
        f.paths
            .brand()
            .environment_variable("PROJECT_VERSION")
            .unwrap(),
        "",
    );
    let out = command.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::write(f.paths.config_file(), b"invalid native global JSON").unwrap();
    let out = f.run(&["provider", "list"]);
    assert!(!out.status.success());
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
}
#[test]
fn typed_globals_override_environment_policy_and_explicit_installation_wins() {
    let f = Fixture::new();
    let other = f._temp.path().join("other");
    let mut c = f.command(&[
        "-c",
        "hub.limits.content.input_bytes=8388608",
        "provider",
        "list",
    ]);
    c.env(
        f.paths.brand().environment_variable("GLOBAL_DIR").unwrap(),
        &other,
    );
    c.env(
        ConfigKey::new(
            graphit_hub::policy::ResourceSetting::ContentInputBytes
                .spec()
                .key,
        )
        .unwrap()
        .environment_variable(f.paths.brand()),
        "invalid",
    );
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!other.exists());
    let out = f
        .command(&["provider", "list"])
        .env(
            ConfigKey::new(
                graphit_hub::policy::ResourceSetting::ContentInputBytes
                    .spec()
                    .key,
            )
            .unwrap()
            .environment_variable(f.paths.brand()),
            "invalid",
        )
        .output()
        .unwrap();
    assert!(!out.status.success());
    let out = f.run(&["--version", "provider", "list"]);
    assert_eq!(out.status.code(), Some(2));
    f.publish(&[
        "provider",
        "add",
        "openai",
        "--type",
        "local",
        "--embedding-mode",
        "direct",
        "--embedding-protocol",
        "openai",
        "--embedding-endpoint",
        "https://example.invalid/v1/embeddings",
        "--embedding-model",
        "text-embedding-3-small",
        "--embedding-dimensions",
        "512",
        "--embedding-request-dimensions",
        "512",
    ]);
    let provider = f.read(&["provider", "show", "openai"]);
    assert_eq!(provider["config"]["embedding"]["request_dimensions"], 512);
    assert!(provider["config"]["embedding"]["batch_endpoint"].is_null());
    f.publish(&[
        "provider",
        "add",
        "google",
        "--type",
        "local",
        "--embedding-mode",
        "direct",
        "--embedding-protocol",
        "google-gemini-1",
        "--embedding-endpoint",
        "https://example.invalid/v1beta/models/gemini-embedding-001:embedContent",
        "--embedding-model",
        "models/gemini-embedding-001",
        "--embedding-dimensions",
        "3",
        "--embedding-batch-endpoint",
        "https://example.invalid/v1beta/models/gemini-embedding-001:batchEmbedContents",
    ]);
    let provider = f.read(&["provider", "show", "google"]);
    assert!(provider["config"]["embedding"]["request_dimensions"].is_null());
    assert_eq!(
        provider["config"]["embedding"]["batch_endpoint"],
        "https://example.invalid/v1beta/models/gemini-embedding-001:batchEmbedContents"
    );
}
#[cfg(unix)]
#[test]
fn closed_output_preserves_applied_publication_and_removes_owned_key() {
    use std::process::Stdio;
    let f = Fixture::new();
    let mut child = f
        .command(&["provider", "add", "applied", "--type", "local"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("authentication"));
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
    assert!(f.read(&["provider", "show", "applied"])["config"].is_object());
}
#[cfg(target_os = "linux")]
#[tokio::test]
async fn retained_unserved_daemon_refuses_transport_without_touching_foreign_files() {
    use graphit_app::foreground_source::ForegroundSource;
    use graphit_core::{
        config::ConfigSnapshot,
        project_identity::{ProjectIdentityStore, ProjectMetadata},
        project_paths::ProjectPaths,
    };
    use graphit_daemon::project_bootstrap::ProjectBootstrapInput;
    use graphit_hub::policy::HubResourcePolicy;
    use std::time::Instant;
    let f = Fixture::new();
    let root = f._temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let project = ProjectPaths::at(&root, &f.paths).unwrap();
    let cancel = Cancellation::default();
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(3), &cancel)
        .unwrap()
        .ensure(&ProjectMetadata::new("actual").unwrap())
        .unwrap();
    let config = ConfigSnapshot::new(f.paths.brand(), []).unwrap();
    let policy = HubResourcePolicy::resolve(&config).unwrap();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let inputs = [ProjectBootstrapInput {
        project: &project,
        explicit_version: "",
        pin: None,
        detached_base: "",
    }];
    let owner = ForegroundSource::start(
        &f.paths,
        &inputs,
        0,
        &config,
        graphit_hub::NativeBinding::Codex,
        &original,
    )
    .unwrap();
    let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
    let auth = fs::read(f.paths.auth_file()).ok();
    let out = f.run(&[
        "-c",
        "hub.limits.request.deadline_ms=250",
        "provider",
        "list",
    ]);
    assert!(!out.status.success());
    assert_eq!(
        fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    assert_eq!(fs::read(f.paths.auth_file()).ok(), auth);
    owner.shutdown().await.unwrap();
    assert!(f.run(&["provider", "list"]).status.success());
}
