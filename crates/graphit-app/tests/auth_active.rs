#![recursion_limit = "256"]
//! Real existing foreground issuer/listener and actual administrative CLI transport.
use clap::Parser;
use graphit_app::{
    admin_auth::ROUTE,
    auth_active, auth_cli,
    foreground_source::{ForegroundFailure, ForegroundSource},
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
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, ResourceSetting},
};
use serde_json::Value;
use std::{
    fs, io,
    process::ExitCode,
    time::{Duration, Instant},
};
#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: auth_cli::Command,
}
fn command(args: &[&str]) -> auth_cli::Command {
    Args::parse_from(std::iter::once("graphit").chain(args.iter().copied())).command
}

async fn first_admin_event(response: &mut reqwest::Response) -> Vec<u8> {
    assert!(response.status().is_success(), "{}", response.status());
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut frame = Vec::new();
        loop {
            let part = response
                .chunk()
                .await
                .unwrap()
                .expect("authentication stream ended before the first event");
            let end = part.iter().position(|byte| *byte == b'\n');
            let bytes = &part[..end.unwrap_or(part.len())];
            assert!(frame.len() + bytes.len() <= 4096, "oversized auth event");
            frame.extend_from_slice(bytes);
            if end.is_some() {
                break frame;
            }
        }
    })
    .await
    .expect("authentication stream did not produce an event")
}

struct Fixture {
    temp: tempfile::TempDir,
    paths: InstallationPaths,
    policy: HubResourcePolicy,
    config: ConfigSnapshot,
    project: ProjectPaths,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND")).unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let paths = InstallationPaths::resolve(
            brand.clone(),
            &base,
            None,
            Some(&base.join("installation")),
        )
        .unwrap();
        let root = base.join("project");
        fs::create_dir(&root).unwrap();
        let project = ProjectPaths::at(&root, &paths).unwrap();
        ProjectIdentityStore::at(&project)
            .writer(Duration::from_secs(5), &Cancellation::default())
            .unwrap()
            .ensure(&ProjectMetadata::new("admin-fixture").unwrap())
            .unwrap();
        let config = ConfigSnapshot::new(
            &brand,
            [HubResourcePolicy::distribution_defaults(&brand).unwrap()],
        )
        .unwrap();
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        Self {
            temp,
            paths,
            policy,
            config,
            project,
        }
    }
    fn limits(&self) -> NativeDocumentFileLimits {
        let value = |s| self.policy.limit(s).value as usize;
        NativeDocumentFileLimits::new(
            value(ResourceSetting::ContentInputBytes),
            graphit_core::native_path::MAX_PATH_BYTES,
            value(ResourceSetting::SnapshotDepth),
            value(ResourceSetting::ContentScalarBytes),
        )
        .unwrap()
    }
    fn start(
        &self,
    ) -> (
        Cancellation,
        tokio::task::JoinHandle<Result<(), ForegroundFailure>>,
    ) {
        let cancel = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancel)
            .unwrap();
        let owner = ForegroundSource::start(
            &self.paths,
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
        let service = stop.clone();
        (stop, tokio::spawn(async move { owner.run(&service).await }))
    }
    async fn execute(&self, args: &[&str]) -> (ExitCode, String, String) {
        let cancel = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancel)
            .unwrap();
        let mut output = Vec::new();
        let mut diagnostic = Vec::new();
        let code = auth_active::run_if_active(
            &command(args),
            &self.paths,
            &loopback_client(),
            self.limits(),
            self.policy.content().max_output_bytes,
            &original,
            &mut output,
            &mut diagnostic,
            |_| Err(io::Error::other("unexpected Broker URL")),
        )
        .await
        .unwrap()
        .unwrap();
        (
            code,
            String::from_utf8(output).unwrap(),
            String::from_utf8(diagnostic).unwrap(),
        )
    }
    async fn read(&self, args: &[&str]) -> Value {
        let (code, out, err) = self.execute(args).await;
        assert_eq!(code, ExitCode::SUCCESS, "{err}");
        serde_json::from_str(&out).unwrap()
    }
    async fn publish(&self, args: &[&str]) {
        let (code, out, err) = self.execute(args).await;
        assert_eq!(code, ExitCode::SUCCESS, "{err}");
        assert_eq!(out, "Applied\n");
        assert!(err.is_empty());
    }
    fn endpoint(&self) -> String {
        let cancel = Cancellation::default();
        let original = self
            .policy
            .request_context(Instant::now(), &cancel)
            .unwrap();
        let address = read_discovery(&self.paths, self.limits(), original.snapshot())
            .unwrap()
            .unwrap()
            .address();
        format!("http://{address}{ROUTE}")
    }
    async fn executable(&self, args: &[&str]) -> std::process::Output {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .current_dir(self.temp.path())
            .env_clear()
            .arg("--global-dir")
            .arg(self.paths.root())
            .arg("--non-interactive")
            .args(args)
            .kill_on_drop(true);
        tokio::time::timeout(Duration::from_secs(20), command.output())
            .await
            .unwrap_or_else(|_| panic!("active authentication CLI timed out: {args:?}"))
            .unwrap()
    }
    async fn stop(
        &self,
        stop: Cancellation,
        job: tokio::task::JoinHandle<Result<(), ForegroundFailure>>,
    ) {
        stop.cancel();
        job.await.unwrap().unwrap();
        assert!(!self.paths.root().join("daemon/mcp.key").exists());
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn active_runtime_local_commands_preserve_same_key_cas_redaction_and_denials() {
    let f = Fixture::new();
    let (stop, job) = f.start();
    let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
    let scratch = f
        .paths
        .auth_file()
        .parent()
        .unwrap()
        .join(".auth.json.stage");
    fs::write(&scratch, b"prior-process scratch must never be interpreted").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&scratch, fs::Permissions::from_mode(0o600)).unwrap();
    }
    f.publish(&["provider", "add", "local", "--type", "local"])
        .await;
    assert!(!scratch.exists());
    f.publish(&[
        "login",
        "--provider",
        "local",
        "--profile",
        "alice",
        "--username",
        "alice",
        "--mcp-key",
        "admin-private-canary",
    ])
    .await;
    let account = f.read(&["account", "show"]).await;
    assert_eq!(account["profile"]["name"], "alice");
    assert!(!account.to_string().contains("admin-private-canary"));
    let bytes = fs::read(f.paths.auth_file()).unwrap();
    let conflict = f
        .execute(&[
            "provider",
            "update",
            "local",
            "--expected-revision",
            "0",
            "--rerank-mode",
            "disabled",
        ])
        .await;
    assert_eq!(conflict.0, ExitCode::FAILURE);
    assert_eq!(fs::read(f.paths.auth_file()).unwrap(), bytes);
    for (origin, bearer, status) in [
        ("runtime", "wrong-private-key", 401),
        ("local", "admin-private-canary", 403),
        ("broker", "admin-private-canary", 403),
    ] {
        let response = loopback_client()
            .post(f.endpoint())
            .header("x-graphit-auth-origin", origin)
            .bearer_auth(bearer)
            .header("content-type", "application/json")
            .body(
                serde_json::to_vec(&command(&[
                    "provider",
                    "remove",
                    "local",
                    "--yes",
                    "--cascade",
                ]))
                .unwrap(),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(fs::read(f.paths.auth_file()).unwrap(), bytes);
    }
    let response = loopback_client()
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(std::str::from_utf8(&key).unwrap())
        .header("content-type", "application/json")
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
    assert_eq!(fs::read(f.paths.auth_file()).unwrap(), bytes);
    assert_eq!(
        fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    f.publish(&["logout"]).await;
    assert!(f.read(&["account", "show"]).await["profile"].is_null());
    f.stop(stop, job).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_cli_uses_active_owner_outside_project_without_key_rotation() {
    let f = Fixture::new();
    let (stop, job) = f.start();
    let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
    let out = f
        .executable(&["provider", "add", "actual", "--type", "local"])
        .await;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"Applied\n");
    let out = f
        .executable(&[
            "login",
            "--provider",
            "actual",
            "--profile",
            "native",
            "--username",
            "native",
            "--mcp-key",
            "compiled-private-canary",
        ])
        .await;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = f.executable(&["account", "list"]).await;
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("compiled-private-canary"));
    assert_eq!(
        fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    f.stop(stop, job).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn absent_discovery_and_cancelled_original_do_not_replace_active_owner() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let original = f.policy.request_context(Instant::now(), &cancel).unwrap();
    assert!(
        auth_active::run_if_active(
            &command(&["provider", "list"]),
            &f.paths,
            &loopback_client(),
            f.limits(),
            f.policy.content().max_output_bytes,
            &original,
            &mut Vec::new(),
            &mut Vec::new(),
            |_| Ok(())
        )
        .await
        .unwrap()
        .is_none()
    );
    let (stop, job) = f.start();
    cancel.cancel();
    let error = auth_active::run_if_active(
        &command(&["provider", "list"]),
        &f.paths,
        &loopback_client(),
        f.limits(),
        f.policy.content().max_output_bytes,
        &original,
        &mut Vec::new(),
        &mut Vec::new(),
        |_| Ok(()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    f.stop(stop, job).await;
}

#[path = "support/broker_oauth.rs"]
mod broker_oauth;
use broker_oauth::{Broker, loopback_client};
use std::sync::atomic::Ordering;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn active_broker_signed_callback_and_refresh_preserve_native_identity_and_key() {
    let f = Fixture::new();
    let (stop, job) = f.start();
    let broker = Broker::new().await;
    f.publish(&[
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
    .await;
    let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
    let cancellation = Cancellation::default();
    let original = f
        .policy
        .request_context(Instant::now(), &cancellation)
        .unwrap();
    let input = command(&[
        "login",
        "--provider",
        "broker",
        "--profile",
        "signed",
        "--s3-disabled",
    ]);
    let (announced, callback) = tokio::sync::oneshot::channel();
    let mut announced = Some(announced);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let client = loopback_client();
    let work = auth_active::run_if_active(
        &input,
        &f.paths,
        &client,
        f.limits(),
        f.policy.content().max_output_bytes,
        &original,
        &mut out,
        &mut err,
        |url| {
            announced
                .take()
                .unwrap()
                .send(url.to_owned())
                .map_err(|_| io::Error::other("callback owner closed"))
        },
    );
    let callback = async {
        let url = broker.callback(&callback.await.unwrap());
        let response = loopback_client().get(url).send().await.unwrap();
        assert!(response.status().is_success());
    };
    let (result, ()) = tokio::join!(work, callback);
    assert_eq!(result.unwrap(), Some(ExitCode::SUCCESS));
    assert_eq!(out, b"Applied\n");
    assert!(err.is_empty());
    let account = f.read(&["account", "show"]).await;
    assert_eq!(account["profile"]["username"], "signed-native-user");
    assert_eq!(
        account["profile"]["broker_identity"]["subject"],
        "native-subject"
    );
    assert!(!account.to_string().contains("refresh-token"));
    f.publish(&["refresh"]).await;
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 2);
    assert_eq!(
        fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    f.stop(stop, job).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_eof_and_service_stop_close_owned_native_broker_callback() {
    let f = Fixture::new();
    let (stop, job) = f.start();
    let broker = Broker::new().await;
    f.publish(&[
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
    .await;
    let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
    let before = fs::read(f.paths.auth_file()).unwrap();
    let client = loopback_client();
    let mut response = client
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(std::str::from_utf8(&key).unwrap())
        .header("content-type", "application/json")
        .body(
            serde_json::to_vec(&command(&[
                "login",
                "--provider",
                "broker",
                "--profile",
                "unfinished",
                "--s3-disabled",
            ]))
            .unwrap(),
        )
        .send()
        .await
        .unwrap();
    let chunk = first_admin_event(&mut response).await;
    let event: graphit_app::admin_auth::Event = serde_json::from_slice(&chunk).unwrap();
    let url = match event {
        graphit_app::admin_auth::Event::AuthorizationUrl { url } => url,
        _ => panic!("expected native URL"),
    };
    let callback = broker.callback(&url);
    let address = format!(
        "{}:{}",
        callback.host_str().unwrap(),
        callback.port().unwrap()
    );
    drop(response);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match tokio::net::TcpStream::connect(&address).await {
                Ok(stream) => drop(stream),
                Err(_) => break,
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fs::read(f.paths.auth_file()).unwrap(), before);
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 0);
    let mut second = client
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(std::str::from_utf8(&key).unwrap())
        .header("content-type", "application/json")
        .body(
            serde_json::to_vec(&command(&[
                "login",
                "--provider",
                "broker",
                "--profile",
                "shutdown",
                "--s3-disabled",
            ]))
            .unwrap(),
        )
        .send()
        .await
        .unwrap();
    let event: graphit_app::admin_auth::Event =
        serde_json::from_slice(&first_admin_event(&mut second).await).unwrap();
    let second_url = match event {
        graphit_app::admin_auth::Event::AuthorizationUrl { url } => url,
        _ => panic!("expected pending URL"),
    };
    let callback = broker.callback(&second_url);
    let second_address = format!(
        "{}:{}",
        callback.host_str().unwrap(),
        callback.port().unwrap()
    );
    tokio::time::timeout(Duration::from_secs(5), f.stop(stop, job))
        .await
        .unwrap();
    assert!(tokio::net::TcpStream::connect(&address).await.is_err());
    assert!(
        tokio::net::TcpStream::connect(&second_address)
            .await
            .is_err()
    );
    assert_eq!(fs::read(f.paths.auth_file()).unwrap(), before);
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 0);
    drop(second);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_broker_login_announces_checked_url_and_refreshes_through_active_daemon() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let f = Fixture::new();
    let (stop, job) = f.start();
    let broker = Broker::new().await;
    f.publish(&[
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
    .await;
    let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_graphit"))
        .current_dir(f.temp.path())
        .env_clear()
        .arg("--global-dir")
        .arg(f.paths.root())
        .arg("--non-interactive")
        .args([
            "login",
            "--provider",
            "broker",
            "--profile",
            "executable",
            "--s3-disabled",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stderr.take().unwrap());
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(15), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let announced = line
        .trim()
        .strip_prefix("Authorize native Broker login: ")
        .unwrap();
    let callback = broker.callback(announced);
    assert!(
        loopback_client()
            .get(callback)
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    let output = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output())
        .await
        .expect("active Broker login did not exit after the callback")
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Applied\n");
    let output = f.executable(&["refresh"]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Applied\n");
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 2);
    assert_eq!(
        fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    f.stop(stop, job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_device_login_uses_active_http_and_shutdown_cancels_pending_publication() {
    let f = Fixture::new();
    let (stop, job) = f.start();
    let mut broker = Broker::new().await;
    broker.state.device.store(true, Ordering::SeqCst);
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
    ])
    .await;
    let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
    let login = f
        .executable(&[
            "login",
            "--device",
            "--provider",
            "broker",
            "--profile",
            "device",
            "--s3-disabled",
        ])
        .await;
    assert!(
        login.status.success(),
        "{}",
        String::from_utf8_lossy(&login.stderr)
    );
    assert_eq!(login.stdout, b"Applied\n");
    let public = String::from_utf8(login.stderr).unwrap();
    assert_eq!(
        public,
        format!(
            "Open {origin}/device and enter code ABCD-EFGH\nAuthorize native Broker login: {origin}/device?user_code=ABCD-EFGH\n",
            origin = broker.origin()
        )
    );
    for private in [
        "device-private-canary",
        "access_token",
        "refresh-token",
        "id_token",
    ] {
        assert!(!public.contains(private));
    }
    let account = f.read(&["account", "show"]).await;
    assert_eq!(account["profile"]["name"], "device");
    assert_eq!(account["profile"]["username"], Value::Null);
    assert_eq!(account["profile"]["organization"], Value::Null);
    assert_eq!(account["profile"]["teams"], Value::Null);
    assert_eq!(
        account["profile"]["broker_identity"]["subject"],
        "native-subject"
    );
    assert_eq!(account["profile"]["active"], true);
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
        key
    );
    let logout = f.executable(&["logout", "--profile", "device"]).await;
    assert!(logout.status.success());
    assert_eq!(logout.stdout, b"Applied\n");
    assert!(f.read(&["account", "show"]).await["profile"].is_null());
    // Consume the first request notification before awaiting the paused request.
    tokio::time::timeout(Duration::from_secs(5), broker.state.started.notified())
        .await
        .expect("completed login never reached the Broker token endpoint");
    broker.state.pause.store(true, Ordering::SeqCst);
    let before = fs::read(f.paths.auth_file()).unwrap();
    let mut response = loopback_client()
        .post(f.endpoint())
        .header("x-graphit-auth-origin", "runtime")
        .bearer_auth(std::str::from_utf8(&key).unwrap())
        .header("content-type", "application/json")
        .body(
            serde_json::to_vec(&command(&[
                "login",
                "--device",
                "--provider",
                "broker",
                "--profile",
                "unfinished",
            ]))
            .unwrap(),
        )
        .send()
        .await
        .unwrap();
    let chunk = first_admin_event(&mut response).await;
    let event: graphit_app::admin_auth::Event = serde_json::from_slice(&chunk).unwrap();
    match event {
        graphit_app::admin_auth::Event::DeviceAuthorization {
            verification_uri,
            verification_uri_complete,
            user_code,
        } => {
            assert_eq!(verification_uri, format!("{}/device", broker.origin()));
            assert_eq!(user_code, "ABCD-EFGH");
            assert_eq!(
                verification_uri_complete,
                Some(format!("{}/device?user_code=ABCD-EFGH", broker.origin()))
            );
        }
        _ => panic!("expected only public Device instructions"),
    }
    assert!(
        !std::str::from_utf8(&chunk)
            .unwrap()
            .contains("device-private-canary")
    );
    tokio::time::timeout(Duration::from_secs(5), broker.state.started.notified())
        .await
        .unwrap();
    drop(response);
    tokio::time::timeout(Duration::from_secs(5), f.stop(stop, job))
        .await
        .unwrap();
    assert_eq!(fs::read(f.paths.auth_file()).unwrap(), before);
    assert_eq!(broker.state.rotation.load(Ordering::SeqCst), 1);
    broker.state.release.notify_one();
    broker.stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), &mut broker.server)
        .await
        .unwrap()
        .unwrap();
}
