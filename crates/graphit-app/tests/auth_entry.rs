use clap::Parser;
use graphit_app::{auth_cli::Command, auth_entry};
use graphit_core::{
    brand::Brand, config::ConfigSnapshot, file_lock::Cancellation, project_paths::InstallationPaths,
};
use graphit_hub::policy::HubResourcePolicy;
use serde_json::Value;
use std::{
    io::{self, Write},
    process::ExitCode,
    time::{Duration, Instant},
};

#[path = "support/broker_oauth.rs"]
mod broker_oauth;

#[derive(Parser)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}
fn command(args: &[&str]) -> Command {
    Arguments::try_parse_from(std::iter::once("graphit").chain(args.iter().copied()))
        .unwrap()
        .command
}
struct Fixture {
    _directory: tempfile::TempDir,
    paths: InstallationPaths,
    policy: HubResourcePolicy,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(directory.path()).unwrap();
        let paths = InstallationPaths::resolve(
            Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap(),
            &root,
            None,
            Some(&root.join("installation")),
        )
        .unwrap();
        let config = ConfigSnapshot::new(
            paths.brand(),
            [HubResourcePolicy::distribution_defaults(paths.brand()).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        Self {
            _directory: directory,
            paths,
            policy,
        }
    }
    fn run(&self, args: &[&str]) -> (ExitCode, String, String) {
        let cancel = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancel)
            .unwrap();
        let mut output = Vec::new();
        let mut diagnostic = Vec::new();
        let result = auth_entry::run(
            command(args),
            &self.paths,
            &self.policy,
            &original,
            &mut output,
            &mut diagnostic,
            |_| panic!("unexpected Broker authorization URL"),
        )
        .unwrap();
        assert!(!self.paths.root().join("daemon/mcp.key").exists());
        (
            result,
            String::from_utf8(output).unwrap(),
            String::from_utf8(diagnostic).unwrap(),
        )
    }
    fn read(&self, args: &[&str]) -> Value {
        let (exit, output, diagnostic) = self.run(args);
        assert_eq!(exit, ExitCode::SUCCESS, "{diagnostic}");
        serde_json::from_str(&output).unwrap()
    }
    fn publish(&self, args: &[&str]) {
        let (exit, output, diagnostic) = self.run(args);
        assert_eq!(exit, ExitCode::SUCCESS, "{diagnostic}");
        assert_eq!(output, "Applied\n");
        assert!(diagnostic.is_empty());
    }
    fn executable(&self, args: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(self._directory.path())
            .env_clear()
            .arg("--global-dir")
            .arg(self.paths.root())
            .args(args)
            .kill_on_drop(true);
        command
    }
}

#[test]
fn installation_only_local_commands_keep_secrets_redacted_and_cleanup_each_owner() {
    let f = Fixture::new();
    f.publish(&["provider", "add", "workstation", "--type", "local"]);
    for name in ["alice", "bob"] {
        f.publish(&[
            "login",
            "--provider",
            "workstation",
            "--profile",
            name,
            "--username",
            name,
            "--mcp-key",
            "auth-entry-private-canary",
        ]);
    }
    let listed = f.read(&["account", "list"]);
    assert_eq!(listed["active"], "bob");
    assert_eq!(listed["profiles"].as_array().unwrap().len(), 2);
    assert!(!listed.to_string().contains("auth-entry-private-canary"));
    assert!(!listed.to_string().contains("mcp_key"));
    f.publish(&["account", "use", "alice"]);
    assert_eq!(f.read(&["account", "show"])["profile"]["name"], "alice");
    f.publish(&["logout"]);
    assert!(f.read(&["account", "show"])["profile"].is_null());
    f.publish(&[
        "provider",
        "update",
        "workstation",
        "--rerank-mode",
        "disabled",
    ]);
    assert_eq!(
        f.read(&["provider", "show", "workstation"])["config"]["rerank"]["mode"],
        "disabled"
    );
    f.publish(&["provider", "remove", "workstation", "--yes", "--cascade"]);
    assert!(
        f.read(&["provider", "list"])["providers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn denied_and_stale_native_mutations_leave_authentication_document_unchanged() {
    let f = Fixture::new();
    f.publish(&["provider", "add", "workstation", "--type", "local"]);
    let before = std::fs::read(f.paths.auth_file()).unwrap();
    for args in [
        vec![
            "provider",
            "update",
            "workstation",
            "--expected-revision",
            "0",
            "--rerank-mode",
            "disabled",
        ],
        vec!["account", "use", "missing-profile"],
        vec![
            "login",
            "--provider",
            "workstation",
            "--profile",
            "invalid-local",
        ],
    ] {
        let (exit, _, diagnostic) = f.run(&args);
        assert_eq!(exit, ExitCode::FAILURE);
        assert!(!diagnostic.is_empty());
        assert_eq!(std::fs::read(f.paths.auth_file()).unwrap(), before);
    }
}

#[test]
fn cancelled_and_expired_originals_do_not_initialize_an_installation() {
    for expired in [false, true] {
        let f = Fixture::new();
        let cancel = Cancellation::default();
        let deadline = if expired {
            Instant::now() + Duration::from_millis(100)
        } else {
            Instant::now() + Duration::from_secs(20)
        };
        let original = graphit_hub::policy::RequestContext::new(
            graphit_hub::native::ContentLimits {
                max_input_bytes: 65536,
                max_output_bytes: 65536,
                max_depth: 32,
                max_nodes: 4096,
                max_scalar_bytes: 4096,
                max_collection_entries: 64,
            },
            &cancel,
            deadline,
        )
        .unwrap();
        if expired {
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        } else {
            cancel.cancel();
        }
        let mut output = Vec::new();
        let mut diagnostic = Vec::new();
        let error = auth_entry::run(
            command(&["provider", "list"]),
            &f.paths,
            &f.policy,
            &original,
            &mut output,
            &mut diagnostic,
            |_| panic!("unexpected Broker authorization URL"),
        )
        .unwrap_err();
        assert!(!error.has_resources());
        assert!(output.is_empty());
        assert!(!f.paths.root().exists());
    }
}

struct BrokenOutput;
impl Write for BrokenOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn output_failure_keeps_applied_publication_and_cleans_owned_runtime() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let error = auth_entry::run(
        command(&["provider", "add", "workstation", "--type", "local"]),
        &f.paths,
        &f.policy,
        &original,
        &mut BrokenOutput,
        &mut Vec::new(),
        |_| panic!("unexpected Broker authorization URL"),
    )
    .unwrap_err();
    assert_eq!(error.cause().kind(), io::ErrorKind::BrokenPipe);
    assert!(!error.has_resources());
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
    assert_eq!(
        f.read(&["provider", "list"])["providers"][0]["name"],
        "workstation"
    );
}

struct CancellingOutput<'a>(&'a Cancellation);
impl Write for CancellingOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.cancel();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn cancelled_delivery_uses_fresh_finite_cleanup_and_allows_owned_reacquisition() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let error = auth_entry::run(
        command(&["provider", "list"]),
        &f.paths,
        &f.policy,
        &original,
        &mut CancellingOutput(&cancel),
        &mut Vec::new(),
        |_| panic!("unexpected Broker authorization URL"),
    )
    .unwrap_err();
    assert_eq!(error.cause().kind(), io::ErrorKind::Interrupted);
    assert!(!error.has_resources());
    assert!(
        f.read(&["provider", "list"])["providers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_authentication_document_is_causal_and_owned_runtime_is_removed() {
    let f = Fixture::new();
    f.publish(&["provider", "add", "workstation", "--type", "local"]);
    std::fs::write(
        f.paths.auth_file(),
        b"invalid native authentication document",
    )
    .unwrap();
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let mut output = Vec::new();
    let mut diagnostic = Vec::new();
    let exit = auth_entry::run(
        command(&["provider", "list"]),
        &f.paths,
        &f.policy,
        &original,
        &mut output,
        &mut diagnostic,
        |_| panic!("unexpected Broker authorization URL"),
    )
    .unwrap();
    assert!(output.is_empty());
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(!diagnostic.is_empty());
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
    assert_eq!(
        std::fs::read(f.paths.auth_file()).unwrap(),
        b"invalid native authentication document"
    );
}

#[cfg(unix)]
struct ReplacingOutput {
    key: std::path::PathBuf,
    replaced: bool,
}
#[cfg(unix)]
impl Write for ReplacingOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        use std::os::unix::fs::OpenOptionsExt;
        if !self.replaced {
            std::fs::rename(&self.key, self.key.with_file_name("owned-key.saved"))?;
            let mut foreign = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&self.key)?;
            foreign
                .write_all(b"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")?;
            self.replaced = true;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(unix)]
#[test]
fn cleanup_conflict_retains_owner_and_never_removes_a_replacement_key() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    let key = f.paths.root().join("daemon/mcp.key");
    let error = auth_entry::run(
        command(&["provider", "add", "workstation", "--type", "local"]),
        &f.paths,
        &f.policy,
        &original,
        &mut ReplacingOutput {
            key: key.clone(),
            replaced: false,
        },
        &mut Vec::new(),
        |_| panic!("unexpected Broker authorization URL"),
    )
    .unwrap_err();
    assert!(error.has_resources());
    assert!(error.cleanup_failure().is_some());
    let foreign = std::fs::read(&key).unwrap();
    let auth = std::fs::read(f.paths.auth_file()).unwrap();
    let second = auth_entry::run(
        command(&["provider", "list"]),
        &f.paths,
        &f.policy,
        &original,
        &mut Vec::new(),
        &mut Vec::new(),
        |_| panic!("unexpected Broker authorization URL"),
    )
    .unwrap_err();
    assert!(!second.has_resources());
    let still_pending = error.cleanup(&f.policy).unwrap_err();
    assert!(still_pending.has_resources());
    assert_eq!(std::fs::read(key).unwrap(), foreign);
    assert_eq!(std::fs::read(f.paths.auth_file()).unwrap(), auth);
}

#[cfg(unix)]
#[test]
fn native_scratch_recovery_discards_stage_without_promoting_its_contents() {
    use std::os::unix::fs::OpenOptionsExt;
    let f = Fixture::new();
    f.publish(&["provider", "add", "workstation", "--type", "local"]);
    let stage = f.paths.root().join(".auth.json.stage");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&stage)
        .unwrap();
    file.write_all(b"scratch-canary-not-an-authentication-document")
        .unwrap();
    drop(file);
    f.publish(&[
        "provider",
        "update",
        "workstation",
        "--rerank-mode",
        "disabled",
    ]);
    assert!(!stage.exists());
    let shown = f.read(&["provider", "show", "workstation"]);
    assert_eq!(shown["config"]["rerank"]["mode"], "disabled");
    assert!(!shown.to_string().contains("scratch-canary"));
}

#[tokio::test]
async fn compiled_standalone_device_login_polls_and_publishes_only_verified_identity() {
    use std::sync::atomic::Ordering;

    let f = Fixture::new();
    let broker = broker_oauth::Broker::new().await;
    f.publish(&[
        "provider",
        "add",
        "broker",
        "--type",
        "broker",
        "--broker-endpoint",
        broker.origin(),
        "--embedding-mode",
        "disabled",
        "--rerank-mode",
        "disabled",
        "--s3-credential-source",
        "broker",
    ]);
    let login = |profile| {
        let mut command = f.executable(&[
            "--non-interactive",
            "login",
            "--device",
            "--provider",
            "broker",
            "--profile",
            profile,
            "--s3-disabled",
        ]);
        async move {
            tokio::time::timeout(Duration::from_secs(20), command.output())
                .await
                .expect("standalone device login timed out")
                .unwrap()
        }
    };
    let assert_clean = || {
        assert!(!f.paths.root().join("daemon/mcp.key").exists());
        assert!(!f.paths.root().join("daemon/mcp.json").exists());
    };
    let initial = std::fs::read(f.paths.auth_file()).unwrap();
    let unsupported = login("unsupported").await;
    assert!(!unsupported.status.success());
    assert!(unsupported.stdout.is_empty());
    let diagnostic = String::from_utf8(unsupported.stderr).unwrap();
    assert!(diagnostic.contains("Unsupported"), "{diagnostic}");
    assert!(!diagnostic.contains("Authorize native Broker login"));
    assert_eq!(broker.state.device_polls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read(f.paths.auth_file()).unwrap(), initial);
    assert_clean();

    broker.state.device.store(true, Ordering::SeqCst);
    *broker.state.invalid.lock().unwrap() = Some("pending");
    let output = login("device").await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Applied\n");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "Open {origin}/device and enter code ABCD-EFGH\nAuthorize native Broker login: {origin}/device?user_code=ABCD-EFGH\n",
            origin = broker.origin()
        )
    );
    assert_eq!(broker.state.device_polls.load(Ordering::SeqCst), 2);
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 1);
    assert_clean();

    let account = f.read(&["account", "show"]);
    assert_eq!(account["profile"]["name"], "device");
    assert_eq!(account["profile"]["active"], true);
    assert_eq!(account["profile"]["username"], Value::Null);
    assert_eq!(
        account["profile"]["broker_identity"]["subject"],
        "native-subject"
    );
    assert!(!account.to_string().contains("access_token"));
    let published = std::fs::read(f.paths.auth_file()).unwrap();
    for (invalid, reason) in [
        ("denied", "AuthorizationRejected"),
        ("expired", "DeviceExpired"),
        ("aud", "InvalidToken"),
    ] {
        *broker.state.invalid.lock().unwrap() = Some(invalid);
        let rejected = login(invalid).await;
        assert!(!rejected.status.success());
        assert!(rejected.stdout.is_empty());
        let diagnostic = String::from_utf8(rejected.stderr).unwrap();
        assert!(diagnostic.contains("ABCD-EFGH"));
        assert!(diagnostic.contains(reason), "{invalid}: {diagnostic}");
        assert!(!diagnostic.contains("device-private-canary"));
        assert!(!diagnostic.contains("access_token"));
        assert_eq!(std::fs::read(f.paths.auth_file()).unwrap(), published);
        assert_clean();
    }
    assert_eq!(broker.state.device_polls.load(Ordering::SeqCst), 5);
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 2);
    f.publish(&["logout"]);
    assert!(f.read(&["account", "show"])["profile"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_standalone_native_refresh_preserves_cas_rotation_redaction_and_cleanup() {
    compiled_native_refresh(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_native_refresh_recovers_ended_owner_and_preserves_live_owner_errors() {
    compiled_native_refresh(true).await;
}

async fn compiled_native_refresh(ended_owner: bool) {
    use std::{process::Stdio, sync::atomic::Ordering};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    let f = Fixture::new();
    let mut broker = broker_oauth::Broker::new().await;
    let executable = |args: &[&str]| f.executable(args);
    let output = executable(&[
        "provider",
        "add",
        "broker",
        "--type",
        "broker",
        "--broker-endpoint",
        broker.origin(),
        "--embedding-mode",
        "broker",
        "--rerank-mode",
        "broker",
        "--s3-credential-source",
        "broker",
    ])
    .output()
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Applied\n");
    let mut child = executable(&[
        "--non-interactive",
        "login",
        "--provider",
        "broker",
        "--profile",
        "remote",
        "--s3-disabled",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    let mut diagnostic = BufReader::new(child.stderr.take().unwrap());
    let mut announcement = String::new();
    tokio::time::timeout(
        Duration::from_secs(15),
        diagnostic.read_line(&mut announcement),
    )
    .await
    .unwrap()
    .unwrap();
    let announced = announcement
        .trim()
        .strip_prefix("Authorize native Broker login: ")
        .unwrap();
    let callback = broker.callback(announced);
    let address = callback.socket_addrs(|| None).unwrap()[0];
    assert!(
        broker_oauth::loopback_client()
            .get(callback)
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    let mut remaining = String::new();
    diagnostic.read_to_string(&mut remaining).await.unwrap();
    let output = child.wait_with_output().await.unwrap();
    assert!(output.status.success(), "{remaining}");
    assert_eq!(output.stdout, b"Applied\n");
    assert!(remaining.is_empty());
    assert!(matches!(
        tokio::net::TcpStream::connect(address).await,
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused
    ));
    let output = executable(&["account", "show"]).output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let before: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(before["profile"]["name"], "remote");
    assert_eq!(before["profile"]["provider"], "broker");
    assert_eq!(before["profile"]["active"], true);
    assert!(!before.to_string().contains("access_token"));
    assert!(!before.to_string().contains("refresh-token"));
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
    assert!(!f.paths.root().join("daemon/mcp.json").exists());
    let bytes = std::fs::read(f.paths.auth_file()).unwrap();
    let discovery = if ended_owner {
        let cancel = Cancellation::default();
        let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
        let owner = administrative_owner(&f.paths, &original);
        let limits = graphit_core::native_document_file::NativeDocumentFileLimits::new(
            65536, 65536, 256, 65536,
        )
        .unwrap();
        let listener = graphit_core::tcp_listener::reserve(
            graphit_core::tcp_listener::Options {
                address: std::net::Ipv4Addr::LOCALHOST.into(),
                ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
                timeout: Duration::from_secs(1),
            },
            &cancel,
        )
        .unwrap();
        let address = listener.address();
        graphit_daemon::source_discovery::publish_discovery(
            &owner,
            &listener,
            limits,
            original.snapshot(),
        )
        .unwrap();
        drop(listener);
        let discovery = std::fs::read(f.paths.root().join("daemon/mcp.json")).unwrap();
        let key = std::fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
        // A live singleton with no listener is still causal; no standalone retry.
        let denied = executable(&["refresh"]).output().await.unwrap();
        assert!(!denied.status.success());
        assert!(denied.stdout.is_empty());
        assert!(!denied.stderr.is_empty());
        assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 1);
        assert_eq!(std::fs::read(f.paths.auth_file()).unwrap(), bytes);
        assert_eq!(
            std::fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
            key
        );
        assert_eq!(
            std::fs::read(f.paths.root().join("daemon/mcp.json")).unwrap(),
            discovery
        );
        // Authorization errors from a still-owned runtime cannot become standalone either.
        for status in [
            axum::http::StatusCode::UNAUTHORIZED,
            axum::http::StatusCode::FORBIDDEN,
        ] {
            let listener = tokio::net::TcpListener::bind(address).await.unwrap();
            let stop = Cancellation::default();
            let stopped = stop.clone();
            let server = tokio::spawn(async move {
                axum::serve(
                    listener,
                    axum::Router::new().route(
                        graphit_app::admin_auth::ROUTE,
                        axum::routing::post(move || async move { status }),
                    ),
                )
                .with_graceful_shutdown(async move { stopped.cancelled().await })
                .await
                .unwrap();
            });
            let denied = executable(&["refresh"]).output().await.unwrap();
            stop.cancel();
            server.await.unwrap();
            assert!(!denied.status.success());
            assert!(String::from_utf8_lossy(&denied.stderr).contains(&format!("HTTP {status}")));
            assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 1);
            assert_eq!(std::fs::read(f.paths.auth_file()).unwrap(), bytes);
            assert_eq!(
                std::fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
                key
            );
            assert_eq!(
                std::fs::read(f.paths.root().join("daemon/mcp.json")).unwrap(),
                discovery
            );
        }
        // Simulate process termination: Drop releases the kernel lock without file I/O.
        drop(owner);
        assert!(
            !graphit_core::file_lock::FileLock::is_locked(f.paths.root().join("daemon.pid"))
                .unwrap()
        );
        Some(discovery)
    } else {
        None
    };
    let stale = executable(&["refresh", "--expected-revision", "0"])
        .output()
        .await
        .unwrap();
    assert!(!stale.status.success());
    assert!(stale.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("authentication document revision changed")
    );
    assert_eq!(std::fs::read(f.paths.auth_file()).unwrap(), bytes);
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 1);
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
    assert_eq!(
        std::fs::read(f.paths.root().join("daemon/mcp.json")).ok(),
        discovery
    );
    let revision = before["revision"].as_u64().unwrap().to_string();
    let output = executable(&["refresh", "--expected-revision", &revision])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Applied\n");
    assert!(output.stderr.is_empty());
    let output = executable(&["account", "show"]).output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(after["revision"], before["revision"].as_u64().unwrap() + 1);
    assert_eq!(
        after["profile"]["token_revision"],
        before["profile"]["token_revision"].as_u64().unwrap() + 1
    );
    for field in [
        "name",
        "provider",
        "active",
        "provider_revision",
        "broker_identity",
        "s3_disabled",
    ] {
        assert_eq!(after["profile"][field], before["profile"][field]);
    }
    assert!(!after.to_string().contains("access_token"));
    assert!(!after.to_string().contains("refresh-token"));
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 2);
    assert!(!f.paths.root().join("daemon/mcp.key").exists());
    assert_eq!(
        std::fs::read(f.paths.root().join("daemon/mcp.json")).ok(),
        discovery
    );
    broker.stop.cancel();
    (&mut broker.server).await.unwrap();
}

fn administrative_owner(
    paths: &InstallationPaths,
    original: &graphit_hub::policy::RequestContext<'_>,
) -> graphit_daemon::auth_startup::DaemonAuth {
    use graphit_auth::{
        AdminSessionPolicy, IssuerAuthorityPolicy, IssuerLimits, Permission,
        ServiceAdmissionLimits, ServiceAuthorityInput, SourceAuthorityPolicy, TtlPolicy,
        profile::ProfileLimits,
    };
    use graphit_core::{
        artifact_store::ScopeLimits, auth_document::AuthDocumentLimits,
        installation_bootstrap::BootstrapLimits, runtime_key::RuntimeKeyLimits,
    };
    use graphit_daemon::auth_startup::{DaemonAuth, StartupPolicy};
    let origin = || SourceAuthorityPolicy {
        ordinary: &[],
        administrative: &[Permission::ManageAuthProfiles],
        sessions: AdminSessionPolicy {
            lifetime: Duration::from_secs(900),
            maximum: Duration::from_secs(3600),
        },
    };
    DaemonAuth::start(
        paths,
        &[],
        StartupPolicy {
            bootstrap: BootstrapLimits::new(65536, 256).unwrap(),
            runtime: RuntimeKeyLimits::new(65536, 256, 65536).unwrap(),
            scope: ScopeLimits::new(1, 65536, 256).unwrap(),
            document: AuthDocumentLimits::new(65536, 65536, 256, 65536).unwrap(),
            profiles: ProfileLimits {
                bytes: 65536,
                nodes: 65536,
                scalar_bytes: 65536,
                collection_entries: 256,
                providers: 256,
                profiles: 256,
            },
            authority: IssuerAuthorityPolicy::new(
                origin(),
                origin(),
                IssuerLimits {
                    max_permissions: 256,
                    max_scope_bytes: 65536,
                    max_parent_bearer_bytes: 65536,
                },
                ServiceAuthorityInput {
                    rules: &[],
                    limits: ServiceAdmissionLimits {
                        max_rules: 256,
                        max_binding_bytes: 65536,
                    },
                    native_refresh_margin: Duration::ZERO,
                },
            )
            .unwrap(),
            ttl: TtlPolicy {
                ttl: Duration::from_secs(1800),
                maximum: Duration::from_secs(86400),
                refresh_margin: Duration::from_secs(60),
            },
        },
        original,
    )
    .unwrap()
}
