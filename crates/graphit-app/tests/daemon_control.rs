#![cfg(target_os = "linux")]

use graphit_app::{
    daemon_control::{self, Operation, State},
    foreground_source::ForegroundSource,
};
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot, ConfigStore},
    file_lock::{Cancellation, FileLock},
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::{InstallationPaths, ProjectPaths},
};
use graphit_daemon::{project_bootstrap::ProjectBootstrapInput, source_discovery};
use graphit_hub::{
    NativeBinding,
    policy::{HubResourcePolicy, ResourceSetting},
};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, ORIGIN};
use std::{
    fs, io,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct PrivateDaemon(Child);
impl Drop for PrivateDaemon {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[tokio::test]
async fn compiled_control_authenticates_and_stops_with_full_cleanup()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let brand = Brand::new(env!("GRAPHIT_COMPILED_BUILD_BRAND"))?;
    let installation = InstallationPaths::resolve(
        brand.clone(),
        temp.path(),
        None,
        Some(&temp.path().join("installation")),
    )?;
    let root = temp.path().join("project");
    fs::create_dir(&root)?;
    let project = ProjectPaths::at(&root, &installation)?;
    let cancellation = Cancellation::default();
    ProjectIdentityStore::at(&project)
        .writer(Duration::from_secs(5), &cancellation)?
        .ensure(&ProjectMetadata::new("daemon-control-fixture")?)?;
    let config = ConfigSnapshot::new(&brand, [])?;
    let policy = HubResourcePolicy::resolve(&config)?;
    let original = policy.request_context(Instant::now(), &cancellation)?;
    let bootstrap = ForegroundSource::start(
        &installation,
        &[ProjectBootstrapInput {
            project: &project,
            explicit_version: "",
            pin: None,
            detached_base: "",
        }],
        0,
        &config,
        NativeBinding::Codex,
        &original,
    )?;
    bootstrap.shutdown().await?;
    ConfigStore::global(&installation)?.set(
        ConfigKey::new("agent")?,
        serde_json::json!("codex"),
        Duration::from_secs(5),
        &cancellation,
    )?;
    let stderr = temp.path().join("daemon.log");
    let mut daemon = PrivateDaemon(
        Command::new(env!("CARGO_BIN_EXE_graphit"))
            .current_dir(project.root())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", temp.path())
            .env("GRAPHIT_MODULES__AST", "false")
            .env(
                brand.environment_variable("GLOBAL_DIR")?,
                installation.root(),
            )
            .args(["daemon", "foreground"])
            .stdout(Stdio::null())
            .stderr(fs::File::create(&stderr)?)
            .spawn()?,
    );
    let number = |setting| usize::try_from(policy.limit(setting).value);
    let native = NativeDocumentFileLimits::new(
        number(ResourceSetting::ContentInputBytes)?,
        graphit_core::native_path::MAX_PATH_BYTES,
        number(ResourceSetting::SnapshotDepth)?,
        number(ResourceSetting::ContentScalarBytes)?,
    )?;
    let until = Instant::now() + Duration::from_secs(30);
    let record = loop {
        if let Some(status) = daemon.0.try_wait()? {
            return Err(io::Error::other(format!(
                "daemon {status}: {}",
                fs::read_to_string(&stderr)?
            ))
            .into());
        }
        let cancel = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancel)?;
        if let Some(record) =
            source_discovery::read_discovery(&installation, native, request.snapshot())?
        {
            break record;
        }
        if Instant::now() >= until {
            return Err(io::Error::from(io::ErrorKind::TimedOut).into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let address = record.address();
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let cancel = Cancellation::default();
    let request = policy.request_context(Instant::now(), &cancel)?;
    let status = daemon_control::request(
        &installation,
        address,
        &client,
        native,
        &request,
        Operation::Status,
        None,
    )
    .await?;
    assert_eq!(status.pid, daemon.0.id());
    assert_eq!(status.state, State::Running);

    let headers: HeaderMap =
        graphit_app::ast_source::runtime_headers(&installation, native, &request)
            .await?
            .into_iter()
            .collect();
    let mut cases = Vec::new();
    let mut origin = headers.clone();
    origin.insert(ORIGIN, HeaderValue::from_static("http://localhost"));
    cases.push((origin, "{\"operation\":\"stop\"}", 403));
    let mut local = headers.clone();
    local.insert("x-graphit-auth-origin", HeaderValue::from_static("local"));
    cases.push((local, "{\"operation\":\"stop\"}", 403));
    let mut duplicate_origin = headers.clone();
    duplicate_origin.append("x-graphit-auth-origin", HeaderValue::from_static("runtime"));
    cases.push((duplicate_origin, "{\"operation\":\"stop\"}", 403));
    let mut duplicate_bearer = headers.clone();
    duplicate_bearer.append(AUTHORIZATION, headers[AUTHORIZATION].clone());
    cases.push((duplicate_bearer, "{\"operation\":\"stop\"}", 401));
    for value in [
        "Bearer incorrect",
        "Bearer bad,token",
        "Bearer bad token",
        "Basic invalid",
    ] {
        let mut bad = headers.clone();
        bad.insert(AUTHORIZATION, HeaderValue::from_static(value));
        cases.push((bad, "{\"operation\":\"stop\"}", 401));
    }
    for body in [
        "{\"operation\":\"restart\"}",
        "{\"operation\":\"stop\",\"operation\":\"stop\"}",
        "{\"operation\":\"stop\",\"extra\":true}",
        "invalid",
    ] {
        cases.push((headers.clone(), body, 400));
    }
    for (headers, body, expected) in cases {
        let response = client
            .post(format!("http://{address}{}", daemon_control::ROUTE))
            .headers(headers)
            .body(body)
            .send()
            .await?;
        assert_eq!(response.status().as_u16(), expected);
        let cancel = Cancellation::default();
        let request = policy.request_context(Instant::now(), &cancel)?;
        let status = daemon_control::request(
            &installation,
            address,
            &client,
            native,
            &request,
            Operation::Status,
            None,
        )
        .await?;
        assert_eq!(status.state, State::Running);
        assert_eq!(status.pid, daemon.0.id());
    }
    let canceled = Cancellation::default();
    let request = policy.request_context(Instant::now(), &canceled)?;
    canceled.cancel();
    assert_eq!(
        daemon_control::request(
            &installation,
            address,
            &client,
            native,
            &request,
            Operation::Stop,
            None,
        )
        .await
        .unwrap_err()
        .kind(),
        io::ErrorKind::Interrupted
    );
    let expired = Cancellation::default();
    let request = graphit_hub::policy::RequestContext::new(
        policy.content(),
        &expired,
        Instant::now() + Duration::from_millis(1),
    )?;
    tokio::time::sleep(Duration::from_millis(2)).await;
    assert_eq!(
        daemon_control::request(
            &installation,
            address,
            &client,
            native,
            &request,
            Operation::Stop,
            None,
        )
        .await
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
    let mut unfinished = tokio::net::TcpStream::connect(address).await?;
    unfinished
        .write_all(b"POST /admin/daemon HTTP/1.1\r\nHost: ")
        .await?;
    let barrier = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .pool_max_idle_per_host(0)
        .build()?;
    let cancel = Cancellation::default();
    let request = policy.request_context(Instant::now(), &cancel)?;
    assert_eq!(
        daemon_control::request(
            &installation,
            address,
            &barrier,
            native,
            &request,
            Operation::Status,
            None,
        )
        .await?
        .state,
        State::Running
    );
    let mut byte = [0; 1];
    assert!(
        tokio::time::timeout(Duration::from_millis(25), unfinished.read(&mut byte))
            .await
            .is_err(),
        "incomplete headers must remain open until cancellation",
    );
    // A valid Runtime credential cannot redirect an owned-child stop to another PID.
    let owned_pid = std::num::NonZeroU32::new(daemon.0.id()).unwrap();
    let other_pid = std::num::NonZeroU32::new(if owned_pid.get() == 1 { 2 } else { 1 }).unwrap();
    let request = policy.request_context(Instant::now(), &cancellation)?;
    let mismatch = daemon_control::request(
        &installation,
        address,
        &client,
        native,
        &request,
        Operation::Stop,
        Some(other_pid),
    )
    .await
    .unwrap_err();
    assert!(mismatch.to_string().contains("409"));
    let status = daemon_control::request(
        &installation,
        address,
        &client,
        native,
        &request,
        Operation::Status,
        Some(owned_pid),
    )
    .await?;
    assert_eq!(status.pid, owned_pid.get());
    assert_eq!(status.state, State::Running);
    let reply = daemon_control::request(
        &installation,
        address,
        &client,
        native,
        &request,
        Operation::Stop,
        Some(owned_pid),
    )
    .await?;
    assert_eq!(reply.pid, daemon.0.id());
    assert_eq!(reply.state, State::Stopping);
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = daemon.0.try_wait()? {
            assert!(
                status.success(),
                "{status}: {}",
                fs::read_to_string(&stderr)?
            );
            break;
        }
        if Instant::now() >= until {
            return Err(
                io::Error::new(io::ErrorKind::TimedOut, "daemon did not join after stop").into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    match tokio::time::timeout(Duration::from_secs(2), unfinished.read(&mut byte)).await? {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
            ) => {}
        result => {
            return Err(io::Error::other(format!(
                "partial HTTP connection survived shutdown: {result:?}"
            ))
            .into());
        }
    }
    assert!(source_discovery::read_discovery(&installation, native, request.snapshot())?.is_none());
    assert!(!installation.root().join("daemon/mcp.key").exists());
    let _singleton = FileLock::try_acquire(installation.root().join("daemon.pid"))?;
    Ok(())
}
