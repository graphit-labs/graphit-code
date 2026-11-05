//! Compiled Linux CLI/daemon journeys with SDK datasets and real signed Native authentication.
#![cfg(target_os = "linux")]
#[path = "support/native_install_fixture.rs"]
mod fixture;
use fixture::{Fixture, auth::Broker};
use graphit_app::native_install::{FailureCategory, InstallOutcome, PayloadEffect, RegistryEffect};
use graphit_core::{
    file_lock::{Cancellation, FileLock},
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::ProjectPaths,
};
use graphit_hub::native_package::PackageKind;
use rmcp::{ServiceExt, model::CallToolRequestParams};
use std::{
    fs,
    path::Path,
    process::{Child, Command, Output, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

fn journey(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap()
                .block_on(work)
        })
        .unwrap()
        .join()
        .unwrap();
}
async fn child_output(mut child: tokio::process::Child) -> Output {
    use tokio::io::AsyncReadExt;
    async fn read(pipe: Option<impl tokio::io::AsyncRead + Unpin>) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut bytes).await?;
        }
        Ok(bytes)
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    match tokio::time::timeout(Duration::from_secs(30), async {
        tokio::try_join!(child.wait(), read(stdout), read(stderr))
    })
    .await
    {
        Ok(Ok((status, stdout, stderr))) => Output {
            status,
            stdout,
            stderr,
        },
        result => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            panic!("compiled command did not complete: {result:?}");
        }
    }
}
async fn output(command: Command) -> Output {
    child_output(
        tokio::process::Command::from(command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap(),
    )
    .await
}
async fn install(
    f: &Fixture,
    cwd: &Path,
    zip: &Path,
    id: &str,
    version: &str,
    args: &[&str],
) -> Output {
    output(install_command(f, cwd, zip, id, version, args)).await
}
fn install_command(
    f: &Fixture,
    cwd: &Path,
    zip: &Path,
    id: &str,
    version: &str,
    args: &[&str],
) -> Command {
    let mut command = f.command(cwd);
    command
        .args(["package", "install", "--input"])
        .arg(zip)
        .args([
            "--producer",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "--id",
            id,
            "--version",
            version,
        ])
        .args(args);
    command
}
fn outcome(output: &Output, code: i32) -> InstallOutcome {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout.iter().filter(|b| **b == b'\n').count(), 1);
    serde_json::from_slice(&output.stdout).unwrap()
}
struct Daemon {
    child: Child,
    log: std::path::PathBuf,
}
impl Daemon {
    async fn start(f: &Fixture, project: &Path) -> Self {
        let log = f.temp.path().join("compiled-daemon.log");
        let child = f
            .command(project)
            .args([
                "-c",
                "agent=kiro-v3",
                "daemon",
                "foreground",
                "--project-version",
                "main",
            ])
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        let mut daemon = Self { child, log };
        let deadline = Instant::now() + Duration::from_secs(30);
        while !f.paths.root().join("daemon/mcp.json").exists() {
            assert!(
                daemon.child.try_wait().unwrap().is_none(),
                "{}",
                fs::read_to_string(&daemon.log).unwrap()
            );
            assert!(Instant::now() < deadline, "daemon discovery timeout");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        daemon
    }
    async fn stop(mut self) {
        assert!(
            Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(
                    status.code(),
                    Some(130),
                    "{}",
                    fs::read_to_string(&self.log).unwrap()
                );
                break;
            }
            assert!(Instant::now() < deadline, "daemon shutdown timeout");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
async fn project(f: &Fixture, ctx: &graphit_hub::policy::RequestContext<'_>) -> std::path::PathBuf {
    let root = f.temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let paths = ProjectPaths::at(&root, &f.paths).unwrap();
    ProjectIdentityStore::at(&paths)
        .writer(
            ctx.remaining().unwrap().expect("finite fixture deadline"),
            ctx.cancellation(),
        )
        .unwrap()
        .ensure(&ProjectMetadata::new("native-cli-project").unwrap())
        .unwrap();
    for args in [
        vec!["init", "-b", "main"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ],
    ] {
        let mut command = Command::new("git");
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", f.temp.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "tag.gpgsign=false",
                "-c",
                "gc.auto=0",
                "-c",
                "maintenance.auto=false",
                "-c",
                "core.fsmonitor=false",
            ])
            .args(args)
            .current_dir(&root);
        let result = output(command).await;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::write(root.join("sibling"), "sibling actual FILE").unwrap();
    root
}
#[test]
fn compiled_active_install_then_exact_cli_mcp_and_independent_versions() {
    journey(async {
        let cancel = Cancellation::default();
        let ctx = fixture::original(&cancel);
        let broker = Broker::start().await;
        let f = Fixture::new(&broker, &ctx, false);
        let root = project(&f, &ctx).await;
        let ast = f.package("first", PackageKind::Ast, false, &ctx).await;
        let ast2 = f.package("second", PackageKind::Ast, false, &ctx).await;
        let knowledge = f
            .package("knowledge", PackageKind::Knowledge, false, &ctx)
            .await;
        let daemon = Daemon::start(&f, &root).await;
        let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
        let discovery = fs::read(f.paths.root().join("daemon/mcp.json")).unwrap();
        f.activate(&broker, &ctx, "broker-api", false);
        let authdoc = fs::read(f.paths.root().join("auth.json")).unwrap();
        for (zip, id, version, kind) in [
            (&ast, "vendor/context", "release-1", PackageKind::Ast),
            (&ast2, "vendor/context", "release-2", PackageKind::Ast),
            (
                &knowledge,
                "vendor/knowledge",
                "release-1",
                PackageKind::Knowledge,
            ),
        ] {
            let installed = outcome(&install(&f, &root, zip, id, version, &[]).await, 0);
            assert!(installed.succeeded());
            assert_eq!(installed.kind, Some(kind));
            assert!(installed.destination.unwrap().is_dir());
        }
        assert_eq!(f.registry.load().unwrap().entries().count(), 3);
        for (version, name) in [("release-1", "first"), ("release-2", "second")] {
            let mut command = f.command(&root);
            command.args([
                "ast",
                "source",
                "file.any",
                "--context",
                &format!("vendor/context@{version}"),
            ]);
            let output = output(command).await;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                format!("{name} compiled installed source\nsecond line")
            );
        }
        let duplicate = outcome(
            &install(&f, &root, &ast, "vendor/context", "release-1", &[]).await,
            1,
        );
        assert_eq!(
            duplicate.failure.unwrap().category,
            FailureCategory::AlreadyExists
        );
        assert_eq!(f.registry.load().unwrap().entries().count(), 3);
        let mut command: tokio::process::Command = f.command(&root).into();
        command
            .args(["mcp", "--stdio", "--non-interactive"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().unwrap();
        let sdk =
            ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap()))
                .await
                .unwrap();
        for (args, text) in [
            (
                serde_json::json!({"path":"file.any","context":"vendor/context@release-1"}),
                "first compiled installed source\nsecond line",
            ),
            (serde_json::json!({"path":"sibling"}), "sibling actual FILE"),
        ] {
            let result = sdk
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_ast_source")
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            assert_ne!(result.is_error, Some(true), "{result:?}");
            let actual: String = result
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .map(|t| t.text.as_str())
                .collect();
            assert_eq!(actual, text);
        }
        sdk.cancel().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(10), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        assert_eq!(
            fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
            key
        );
        assert_eq!(
            fs::read(f.paths.root().join("daemon/mcp.json")).unwrap(),
            discovery
        );
        assert_eq!(fs::read(f.paths.root().join("auth.json")).unwrap(), authdoc);
        assert!(broker.requests().iter().all(|n| *n >= 3));
        daemon.stop().await;
        assert!(!f.paths.root().join("daemon/mcp.key").exists());
        assert!(!f.paths.root().join("daemon/mcp.json").exists());
        broker.shutdown().await;
    });
}
#[test]
fn compiled_offline_absence_actual_native_and_rejections_preserve_effects() {
    journey(async {
        let cancel = Cancellation::default();
        let ctx = fixture::original(&cancel);
        let broker = Broker::start().await;
        let f = Fixture::new(&broker, &ctx, false);
        f.activate(&broker, &ctx, "broker-api", false);
        let zip = f.package("offline", PackageKind::Ast, false, &ctx).await;
        let authdoc = fs::read(f.paths.root().join("auth.json")).unwrap();
        let installed = outcome(
            &install(&f, f.temp.path(), &zip, "vendor/offline", "v1", &[]).await,
            0,
        );
        assert!(installed.succeeded());
        assert!(f.registry.load().unwrap().entries().count() == 1);
        assert!(!f.paths.root().join("daemon/mcp.key").exists());
        assert!(!f.paths.root().join("daemon/mcp.json").exists());
        assert_eq!(fs::read(f.paths.root().join("auth.json")).unwrap(), authdoc);
        let mismatch = outcome(
            &install(
                &f,
                f.temp.path(),
                &zip,
                "vendor/mismatch",
                "v1",
                &["--kind", "knowledge"],
            )
            .await,
            1,
        );
        assert_eq!(mismatch.payload, PayloadEffect::NotPublished);
        assert_eq!(mismatch.registry, RegistryEffect::NotAttempted);
        let bad = f.package("bad-schema", PackageKind::Ast, true, &ctx).await;
        let rejected = outcome(
            &install(&f, f.temp.path(), &bad, "vendor/bad", "v1", &[]).await,
            1,
        );
        assert_eq!(rejected.failure.unwrap().category, FailureCategory::Lance);
        let lock = FileLock::try_acquire(f.paths.root().join("daemon.pid")).unwrap();
        let occupied = install(&f, f.temp.path(), &zip, "vendor/occupied", "v1", &[]).await;
        assert_eq!(occupied.status.code(), Some(1));
        assert!(occupied.stdout.is_empty());
        assert!(!f.paths.root().join("daemon/mcp.key").exists());
        drop(lock);
        for body in [
            b"invalid".as_slice(),
            b"{\"pid\":1,\"address\":\"127.0.0.1:9\"}".as_slice(),
        ] {
            fs::write(f.paths.root().join("daemon/mcp.json"), body).unwrap();
            let failed = install(&f, f.temp.path(), &zip, "vendor/discovery", "v1", &[]).await;
            assert_eq!(failed.status.code(), Some(1));
            assert!(failed.stdout.is_empty());
            assert!(!f.paths.root().join("daemon/mcp.key").exists());
            assert_eq!(
                fs::read(f.paths.root().join("daemon/mcp.json")).unwrap(),
                body
            );
        }
        fs::remove_file(f.paths.root().join("daemon/mcp.json")).unwrap();
        fs::write(
            f.paths.root().join("daemon/mcp.json"),
            b"{\"pid\":1,\"address\":\"127.0.0.1:9\"}",
        )
        .unwrap();
        fs::write(f.paths.root().join("daemon/mcp.key"), b"invalid-key").unwrap();
        let badkey = install(&f, f.temp.path(), &zip, "vendor/bad-key", "v1", &[]).await;
        assert_eq!(badkey.status.code(), Some(1));
        assert!(badkey.stdout.is_empty());
        assert_eq!(
            fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
            b"invalid-key"
        );
        fs::remove_file(f.paths.root().join("daemon/mcp.key")).unwrap();
        fs::remove_file(f.paths.root().join("daemon/mcp.json")).unwrap();
        let mut permissions = fs::metadata(f.registry.path()).unwrap().permissions();
        let restored = permissions.clone();
        permissions.set_readonly(true);
        fs::set_permissions(f.registry.path(), permissions).unwrap();
        let partial = outcome(
            &install(&f, f.temp.path(), &zip, "vendor/readonly", "v1", &[]).await,
            1,
        );
        fs::set_permissions(f.registry.path(), restored).unwrap();
        assert_eq!(partial.payload, PayloadEffect::Published);
        assert_eq!(partial.registry, RegistryEffect::NotRegistered);
        assert_eq!(partial.failure.unwrap().category, FailureCategory::Registry);
        assert!(partial.destination.unwrap().exists());
        f.activate(&broker, &ctx, "wrong-audience", false);
        let denied = outcome(
            &install(&f, f.temp.path(), &zip, "vendor/denied", "v1", &[]).await,
            1,
        );
        assert_eq!(
            denied.failure.unwrap().category,
            FailureCategory::Authentication
        );
        assert!(!f.paths.root().join("daemon/mcp.key").exists());
        assert_eq!(f.registry.load().unwrap().entries().count(), 1);
        broker.shutdown().await;
    });
}
#[test]
fn compiled_cancel_deadline_and_stdout_failure_keep_published_effects() {
    journey(async {
        let cancel = Cancellation::default();
        let ctx = fixture::original(&cancel);
        let broker = Broker::start().await;
        let f = Fixture::new(&broker, &ctx, false);
        f.activate(&broker, &ctx, "broker-api", false);
        let zip = f.package("cancel", PackageKind::Ast, false, &ctx).await;
        for (id, timeout, signal, code, category) in [
            (
                "vendor/cancel",
                "30000",
                true,
                130,
                FailureCategory::Cancelled,
            ),
            (
                "vendor/deadline",
                "2000",
                false,
                1,
                FailureCategory::Deadline,
            ),
        ] {
            let lock = FileLock::try_acquire(f.registry.lock_path()).unwrap();
            let mut command = tokio::process::Command::from(install_command(
                &f,
                f.temp.path(),
                &zip,
                id,
                "v1",
                &[
                    "-c",
                    &format!(
                        "{}={timeout}",
                        graphit_hub::policy::ResourceSetting::RequestDeadlineMs
                            .spec()
                            .key
                    ),
                ],
            ));
            let mut child = command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let destination = graphit_hub::SourceRef::hub_cache(
                &f.paths,
                "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
                graphit_hub::ArtifactType::Ast,
                graphit_hub::ArtifactId::new(id, f.policy.projection()).unwrap(),
                None,
                graphit_hub::ConcreteVersion::new("v1", f.policy.projection()).unwrap(),
                f.policy.projection(),
            )
            .unwrap();
            let until = Instant::now() + Duration::from_secs(20);
            while !destination.directory().as_path().exists() {
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "installer exited before registry lock wait"
                );
                assert!(Instant::now() < until);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            if signal {
                assert!(
                    Command::new("kill")
                        .args([
                            "-INT",
                            &child.id().expect("installer must be running").to_string()
                        ])
                        .status()
                        .unwrap()
                        .success()
                );
            }
            let result = child_output(child).await;
            let known = outcome(&result, code);
            assert_eq!(known.payload, PayloadEffect::Published);
            assert_eq!(known.registry, RegistryEffect::NotAttempted);
            assert_eq!(known.failure.unwrap().category, category);
            assert!(destination.directory().as_path().exists());
            assert!(!f.paths.root().join("daemon/mcp.key").exists());
            drop(lock);
        }
        let mut command = tokio::process::Command::from(install_command(
            &f,
            f.temp.path(),
            &zip,
            "vendor/output",
            "v1",
            &[],
        ));
        let child = command
            .stdout(
                fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            )
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let output = child_output(child).await;
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("Published") && stderr.contains("Registered"),
            "{stderr}"
        );
        assert_eq!(f.registry.load().unwrap().entries().count(), 1);
        assert!(!f.paths.root().join("daemon/mcp.key").exists());
        broker.shutdown().await;
    });
}
#[tokio::test]
async fn compiled_install_rejects_cli_before_initialization_and_physical_commands_remain() {
    let directory = tempfile::tempdir().unwrap();
    let installation = directory.path().join("missing-installation");
    let base = [
        "package",
        "install",
        "--input",
        "x.zip",
        "--producer",
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "--id",
        "vendor/corpus",
        "--version",
        "v1",
    ];
    for args in [
        vec!["--output", "out"],
        vec!["--host", "kiro-v3"],
        vec!["--unexpected"],
        vec!["--project-version", "main"],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .env_clear()
            .env("HOME", directory.path())
            .current_dir(directory.path())
            .arg("--global-dir")
            .arg(&installation)
            .args(base)
            .args(args);
        let output = output(command).await;
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(!installation.exists());
    }
    for sub in ["install", "export", "inspect", "import"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .env_clear()
            .current_dir(directory.path())
            .args(["package", sub, "--help"]);
        let output = output(command).await;
        assert!(output.status.success());
        assert!(!installation.exists());
    }
    for (index, value) in [(5, "not-project"), (7, "../alias"), (9, "")] {
        let mut arguments = base.to_vec();
        arguments[index] = value;
        let mut command = Command::new(env!("CARGO_BIN_EXE_graphit"));
        command
            .env_clear()
            .env("HOME", directory.path())
            .current_dir(directory.path())
            .arg("--global-dir")
            .arg(&installation)
            .args(arguments);
        let output = output(command).await;
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(!installation.exists());
    }
}

#[test]
fn compiled_client_closed_listener_and_real_ingress_terminal_failure_never_fall_back() {
    journey(async {
        use graphit_app::native_install_http::{NativeInstallHttp, NativeInstallPolicy};
        use graphit_core::{
            config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
            native_document_file::NativeDocumentFileLimits,
            tcp_listener::{self, Options, PortSelection},
        };
        use graphit_hub::policy::ResourceSetting;
        let cancel = Cancellation::default();
        let ctx = fixture::original(&cancel);
        let broker = Broker::start().await;
        let (f, auth, config) = Fixture::with_owner(&broker, &ctx);
        let zip = f.package("network", PackageKind::Ast, false, &ctx).await;
        let global = ConfigDocument::new(
            f.paths.brand(),
            ConfigSource::Command,
            [
                (
                    ConfigKey::new(ResourceSetting::ContentOutputBytes.spec().key).unwrap(),
                    serde_json::json!(512),
                ),
                (
                    ConfigKey::new(ResourceSetting::ContentScalarBytes.spec().key).unwrap(),
                    serde_json::json!(512),
                ),
            ],
        )
        .unwrap();
        let config = ConfigSnapshot::new(
            f.paths.brand(),
            [
                config.layer(ConfigSource::Distribution).unwrap().clone(),
                global,
            ],
        )
        .unwrap();
        let policy = NativeInstallPolicy::capture(&config, ctx.snapshot()).unwrap();
        let stop = tokio_util::sync::CancellationToken::new();
        let http = NativeInstallHttp::new(
            Arc::clone(&auth),
            f.paths.clone(),
            f.registry.clone(),
            policy,
            stop.clone(),
        )
        .unwrap();
        let limits = NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap();
        let reserve = || {
            tcp_listener::reserve(
                Options {
                    address: "127.0.0.1".parse().unwrap(),
                    ports: PortSelection::Ephemeral,
                    timeout: Duration::from_secs(5),
                },
                &cancel,
            )
            .unwrap()
        };
        let listener = reserve();
        graphit_daemon::source_discovery::publish_discovery(
            &auth,
            &listener,
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        drop(listener);
        let key = fs::read(f.paths.root().join("daemon/mcp.key")).unwrap();
        let discovery = fs::read(f.paths.root().join("daemon/mcp.json")).unwrap();
        let connection = install(&f, f.temp.path(), &zip, "vendor/closed", "v1", &[]).await;
        assert_eq!(connection.status.code(), Some(1));
        assert!(connection.stdout.is_empty());
        assert!(String::from_utf8_lossy(&connection.stderr).contains("MayHaveApplied"));
        assert_eq!(
            fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
            key
        );
        assert_eq!(
            fs::read(f.paths.root().join("daemon/mcp.json")).unwrap(),
            discovery
        );
        assert!(f.registry.load().unwrap().entries().next().is_none());
        let listener = reserve();
        let expected = graphit_daemon::source_discovery::publish_discovery(
            &auth,
            &listener,
            limits,
            ctx.snapshot(),
        )
        .unwrap();
        let native = listener.into_listener();
        native.set_nonblocking(true).unwrap();
        let native = tokio::net::TcpListener::from_std(native).unwrap();
        let router = http.router();
        let stopping = stop.clone();
        let serve = tokio::spawn(async move {
            axum::serve(native, router)
                .with_graceful_shutdown(stopping.cancelled_owned())
                .await
                .unwrap()
        });
        let id = format!("vendor/{}/{}", "x".repeat(150), "y".repeat(150));
        let lost = install(&f, f.temp.path(), &zip, &id, "v1", &[]).await;
        assert_eq!(lost.status.code(), Some(1));
        assert!(lost.stdout.is_empty());
        assert!(String::from_utf8_lossy(&lost.stderr).contains("MayHaveApplied"));
        assert_eq!(f.registry.load().unwrap().entries().count(), 1);
        assert_eq!(
            fs::read(f.paths.root().join("daemon/mcp.key")).unwrap(),
            key
        );
        assert!(http.shutdown().await.is_err());
        serve.await.unwrap();
        assert!(
            graphit_daemon::source_discovery::remove_discovery(
                &auth,
                &expected,
                limits,
                ctx.snapshot()
            )
            .unwrap()
        );
        let removed = auth.remove_runtime(ctx.snapshot());
        assert!(removed.failure().is_none() && removed.cleanup_failure().is_none());
        assert!(broker.requests().iter().all(|n| *n >= 1));
        broker.shutdown().await;
    });
}
