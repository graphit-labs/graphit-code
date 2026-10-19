#![cfg(target_os = "linux")]
//! Compiled CLI + real Source/Auth/discovery; the WS peer exercises the Broker protocol, not SQL/TLS.
use graphit_app::{
    native_install_http::NativeInstallPolicy,
    source_server::{SourcePolicy, SourceServer},
};
#[path = "support/knowledge_ask_server.rs"]
#[expect(
    dead_code,
    reason = "Reuse the actual Source/Auth fixture; sibling ingress helpers are used by other suites"
)]
mod fixture_support;
use fixture_support as support;
#[path = "support/knowledge_cli_daemon.rs"]
mod cli_daemon;
#[path = "support/now_fixture.rs"]
#[expect(
    dead_code,
    reason = "Reuse the actual Auth/profile fixture; the compiled CLI owns its controllable WS peer"
)]
mod wire;
use graphit_core::{
    config::{ConfigKey, ConfigStore},
    file_lock::Cancellation,
    native_document_file::NativeDocumentFileLimits,
    project_identity::ProjectId,
    project_paths::InstallationPaths,
};
use graphit_hub::NativeBinding;
use serde_json::{Value, json};
use std::{
    io,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const WAIT: Duration = Duration::from_secs(8);
const TITLE: &str = "Café β\u{001b}[31m\u{0085}\u{202e}title";

struct Fixture {
    paths: InstallationPaths,
    project: ProjectId,
    directory: std::path::PathBuf,
    daemon: cli_daemon::CliDaemon,
    peer_stop: tokio_util::sync::CancellationToken,
    peer_job: tokio::task::JoinHandle<io::Result<()>>,
    command: mpsc::Sender<Value>,
    closed: mpsc::Receiver<()>,
    calls: Arc<AtomicUsize>,
    publishers: Arc<AtomicUsize>,
    publishers_closed: Arc<AtomicUsize>,
    publisher_done: mpsc::Receiver<()>,
}
impl Fixture {
    async fn new() -> Result<Self> {
        Self::with_large_title(false).await
    }
    async fn with_large_title(large: bool) -> Result<Self> {
        use axum::{
            Router,
            extract::ws::{Message, WebSocketUpgrade},
            response::IntoResponse,
            routing::{get, put},
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let source = wire::remote_fixture(&endpoint);
        let paths = source.paths.clone();
        let project = source.projects[0].1;
        let directory = source.temp.path().to_owned();
        let allowed_project = source.projects[0].1.to_string();
        let version = source.projects[0].2.version().to_owned();
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } =
            source.auth.installation().live.owner()
        else {
            panic!("fixture installation owner")
        };
        let instance = installation_id.as_str().to_owned();
        let expected_inventory = json!({"projects":[{"project_id":project,"versions":[version]}]});
        let inventory = Arc::new(Mutex::new(None::<Value>));
        let inventory_received = inventory.clone();
        let inventory_body = expected_inventory.clone();
        let inventory_instance = instance.clone();
        let title = if large {
            format!("{TITLE}{}", "x".repeat(2500))
        } else {
            TITLE.to_owned()
        };
        let (command, receive) = mpsc::channel::<Value>(1);
        let receive = Arc::new(tokio::sync::Mutex::new(Some(receive)));
        let (closing, closed) = mpsc::channel(1);
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let publishers = Arc::new(AtomicUsize::new(0));
        let publishers_closed = Arc::new(AtomicUsize::new(0));
        let (publisher_finished, publisher_closed_events) = mpsc::channel(1);
        let published = publishers.clone();
        let publisher_done = publishers_closed.clone();
        let first_publish = Arc::new(tokio::sync::Notify::new());
        let publisher_ready = first_publish.clone();
        let inventory_route = put(
            move |axum::extract::Path(selected): axum::extract::Path<String>,
                  headers: axum::http::HeaderMap,
                  axum::Json(body): axum::Json<Value>| {
                let expected = inventory_body.clone();
                let instance = inventory_instance.clone();
                let received = inventory_received.clone();
                async move {
                    if headers.get("authorization").and_then(|v| v.to_str().ok())
                        != Some("Bearer now-protocol-fixture")
                    {
                        return (
                            axum::http::StatusCode::UNAUTHORIZED,
                            [(axum::http::header::WWW_AUTHENTICATE, "Bearer")],
                            axum::Json(json!({"error":{"code":"unauthenticated"}})),
                        )
                            .into_response();
                    }
                    if selected != instance || body != expected {
                        return (
                            axum::http::StatusCode::BAD_REQUEST,
                            axum::Json(json!({"error":{"code":"invalid_inventory"}})),
                        )
                            .into_response();
                    }
                    assert_eq!(headers.get("content-type").unwrap(), "application/json");
                    let mut previous = received.lock().unwrap();
                    let created = previous.is_none();
                    *previous = Some(body);
                    (if created { axum::http::StatusCode::CREATED } else { axum::http::StatusCode::OK }, axum::Json(json!({"instance_id":instance,"change":if created { "created" } else { "unchanged" },"created_at":1,"updated_at":1}))).into_response()
                }
            },
        );
        let realtime_route = get(
            move |headers: axum::http::HeaderMap, upgrade: WebSocketUpgrade| {
                let receive = receive.clone();
                let closing = closing.clone();
                let counted = counted.clone();
                let allowed_project = allowed_project.clone();
                let title = title.clone();
                let version = version.clone();
                let instance = instance.clone();
                let published = published.clone();
                let publisher_done = publisher_done.clone();
                let publisher_finished = publisher_finished.clone();
                let publisher_ready = publisher_ready.clone();
                async move {
                    assert_eq!(
                        headers.get("authorization").unwrap(),
                        "Bearer now-protocol-fixture"
                    );
                    upgrade.on_upgrade(move |mut socket| async move {

                    let Message::Text(first) = socket.recv().await.unwrap().unwrap() else { panic!("subscribe") };
                    let first: Value = serde_json::from_str(&first).unwrap();
                    if first["op"] == "publish" {
                        validate_publish(&first, &instance, &allowed_project, &version);
                        published.fetch_add(1, Ordering::SeqCst);
                        socket.send(Message::Text(json!({"type":"welcome","heartbeat_timeout_ns":"4000000000"}).to_string().into())).await.unwrap();
                        publisher_ready.notify_one();
                        while let Some(Ok(next)) = socket.recv().await {
                            match next {
                                Message::Text(text) => validate_publish(&serde_json::from_str(&text).unwrap(), &instance, &allowed_project, &version),
                                Message::Ping(payload) => { if socket.send(Message::Pong(payload)).await.is_err() { break; } },
                                Message::Pong(_) => {},
                                Message::Close(_) => break,
                                Message::Binary(_) => panic!("publisher must send text JSON"),
                            }
                        }
                        publisher_done.fetch_add(1, Ordering::SeqCst);
                        publisher_finished.send(()).await.unwrap();
                        return;
                    }
                    assert_eq!(first["op"], "subscribe");
                    counted.fetch_add(1, Ordering::SeqCst);
                    if first["project_id"] != allowed_project {
                        socket.send(Message::Text(json!({"type":"error","code":"forbidden"}).to_string().into())).await.unwrap();
                        drop(socket); let _ = closing.send(()).await; return;
                    }
                    let mut commands = receive.lock().await.take().unwrap();
                    assert_eq!(first["version"], "feature/β");
                    assert_eq!(first["instance_id"], "a");
                    let project = &first["project_id"];
                    socket.send(Message::Text(json!({"type":"welcome","heartbeat_timeout_ns":"4000000000"}).to_string().into())).await.unwrap();
                    let empty_section = json!({"availability":"available","items":[],"total":0,"has_more":false,"error":null});
                    let knowledge = json!({"availability":"available","items":(0..20).map(|i|json!({"slug":format!("guide-{i:02}"),"title":title,"updated_at":"9007199254740993001"})).collect::<Vec<_>>(),"total":9007199254740991_u64,"has_more":true,"error":null});
                    let now = json!({"project_id":project,"generated_at":"9007199254740993001","sections":{"tasks":empty_section,"sessions":empty_section,"memories":empty_section,"knowledge":knowledge}});
                    let frame = json!({"type":"snapshot","project_id":project,"instances":[{"instance_id":"a","version":"feature/β","now":now}]});
                    assert!(socket.send(Message::Text(frame.to_string().into())).await.is_ok());
                    loop {
                        tokio::select! {
                            next = socket.recv() => match next { Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break, _ => {} },
                            frame = commands.recv() => match frame {
                                Some(frame) => { if socket.send(Message::Text(frame.to_string().into())).await.is_err() { break; } },
                                None => break,
                            },
                        }
                    }
                    drop(socket); let _ = closing.send(()).await;
                })
                }
            },
        );
        let router = Router::new()
            .route("/v1/instances/{instance}/inventory", inventory_route)
            .route("/v1/realtime", realtime_route);
        let peer_stop = tokio_util::sync::CancellationToken::new();
        let stopped = peer_stop.clone();
        let peer_job = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
        });
        let cancel = Cancellation::default();
        let original = wire::original(&cancel);
        assert!(ConfigStore::global(&paths)?.set(
            ConfigKey::new("broker.allow_loopback_http")?,
            json!(true),
            WAIT,
            &cancel,
        )?);
        let daemon =
            cli_daemon::CliDaemon::start(source, NativeBinding::Codex, Vec::new(), &original)
                .await
                .unwrap();
        assert_eq!(*inventory.lock().unwrap(), Some(expected_inventory));
        let fixture = Self {
            paths,
            project,
            directory,
            daemon,
            peer_stop,
            peer_job,
            command,
            closed,
            calls,
            publishers,
            publishers_closed,
            publisher_done: publisher_closed_events,
        };
        if let Err(error) = tokio::time::timeout(WAIT, first_publish.notified()).await {
            fixture.shutdown().await?;
            return Err(error.into());
        }
        assert!(fixture.publishers.load(Ordering::SeqCst) > 0);
        Ok(fixture)
    }
    fn cli(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_graphit"));
        cmd.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .current_dir(&self.directory)
            .arg("--global-dir")
            .arg(self.paths.root())
            .args([
                "now",
                "--project",
                &self.project.to_string(),
                "--provider",
                "now",
                "--version",
                "feature/β",
                "--instance",
                "a",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd
    }
    async fn empty(&self) -> Result {
        self.command
            .send(json!({"type":"snapshot","project_id":self.project,"instances":[]}))
            .await?;
        Ok(())
    }
    async fn joined(&mut self) -> Result {
        tokio::time::timeout(WAIT, self.closed.recv())
            .await?
            .ok_or("missing peer close")?;
        assert_eq!(self.calls.load(Ordering::SeqCst), 1);
        Ok(())
    }
    async fn shutdown(mut self) -> Result {
        let stopped = tokio::time::timeout(WAIT, self.daemon.stop()).await;
        let publishers_closed = tokio::time::timeout(WAIT, async {
            while self.publishers_closed.load(Ordering::SeqCst)
                < self.publishers.load(Ordering::SeqCst)
            {
                self.publisher_done
                    .recv()
                    .await
                    .ok_or("missing publisher close")?;
            }
            Ok::<_, &'static str>(())
        })
        .await;
        self.peer_stop.cancel();
        let peer = tokio::time::timeout(WAIT, self.peer_job).await;
        stopped?.unwrap();
        publishers_closed??;
        peer???;
        assert_eq!(
            self.publishers.load(Ordering::SeqCst),
            self.publishers_closed.load(Ordering::SeqCst)
        );
        Ok(())
    }
}
fn validate_publish(frame: &Value, instance: &str, project: &str, version: &str) {
    assert_eq!(frame.as_object().unwrap().len(), 5);
    assert_eq!(frame["op"], "publish");
    assert_eq!(frame["instance_id"], instance);
    assert_eq!(frame["project_id"], project);
    assert_eq!(frame["version"], version);
    let snapshot = &frame["snapshot"];
    assert_eq!(snapshot["project_id"], project);
    assert!(
        snapshot["generated_at"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .is_ok()
    );
    let sections = snapshot["sections"].as_object().unwrap();
    assert_eq!(sections.len(), 4);
    for name in ["tasks", "sessions", "memories", "knowledge"] {
        assert!(sections[name].is_object());
    }
}
fn native() -> NativeDocumentFileLimits {
    NativeDocumentFileLimits::new(65536, 4096, 128, 65536).unwrap()
}
async fn signal(pid: u32, name: &str) -> Result {
    assert!(
        Command::new("kill")
            .args([name, &pid.to_string()])
            .status()
            .await?
            .success()
    );
    Ok(())
}
async fn stopped(child: &mut Child, code: i32) -> Result {
    assert_eq!(
        tokio::time::timeout(WAIT, child.wait()).await??.code(),
        Some(code)
    );
    Ok(())
}
async fn first_line(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    child: &mut Child,
) -> Result<String> {
    if let Some(frame) = tokio::time::timeout(WAIT, lines.next_line()).await?? {
        return Ok(frame);
    }
    let mut diagnostic = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut diagnostic)
        .await?;
    Err(format!("CLI ended before first frame: {}", diagnostic.trim()).into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_pipe_full_empty_filters_precision_and_both_signals_join() -> Result {
    for sig in ["-INT", "-TERM"] {
        let mut f = Fixture::new().await?;
        let mut child = f.cli().spawn()?;
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let first = first_line(&mut lines, &mut child).await?;
        assert!(!first.contains('\u{001b}'));
        let full: Value = serde_json::from_str(&first)?;
        assert_eq!(
            full["instances"][0]["now"]["generated_at"],
            "9007199254740993001"
        );
        assert_eq!(
            full["instances"][0]["now"]["sections"]["knowledge"]["items"][0]["title"],
            TITLE
        );
        assert_eq!(
            full["instances"][0]["now"]["sections"]["knowledge"]["total"],
            9007199254740991_u64
        );
        f.empty().await?;
        let empty = tokio::time::timeout(WAIT, lines.next_line())
            .await??
            .ok_or("empty frame")?;
        assert_eq!(
            serde_json::from_str::<Value>(&empty)?["instances"],
            json!([])
        );
        signal(child.id().unwrap(), sig).await?;
        stopped(&mut child, 130).await?;
        f.joined().await?;
        f.shutdown().await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_broken_pipe_and_auth_terminal_close_the_real_subscriber() -> Result {
    for auth in [false, true] {
        let mut f = Fixture::new().await?;
        let mut child = f.cli().spawn()?;
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        first_line(&mut lines, &mut child).await?;
        if auth {
            f.command
                .send(json!({"type":"error","code":"unauthenticated"}))
                .await?;
            stopped(&mut child, 1).await?;
            let mut errors = String::new();
            child
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut errors)
                .await?;
            assert!(!errors.contains("now-protocol-fixture") && !errors.contains("127.0.0.1"));
        } else {
            drop(lines);
            f.empty().await?;
            stopped(&mut child, 0).await?;
        }
        f.joined().await?;
        f.shutdown().await?;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finite_original_expiry_at_real_entry_retires_source_subscriber() -> Result {
    let mut f = Fixture::new().await?;
    let cancel = Cancellation::default();
    let original = graphit_hub::policy::RequestContext::new(
        wire::content(),
        &cancel,
        Instant::now() + Duration::from_secs(2),
    )?;
    let mut output = tempfile::tempfile()?;
    let result = graphit_app::now_entry::run_pipe(
        &f.paths,
        &reqwest::Client::new(),
        graphit_app::now_entry::Arguments {
            project: f.project,
            provider: "now".into(),
            version: Some("feature/β".into()),
            instance: Some("a".into()),
        },
        (native(), wire::content()),
        &original,
        &output,
    )
    .await;
    let failure = result.unwrap_err();
    assert_eq!(failure.to_string(), "Now request expired", "{failure:?}");
    use std::io::{Read as _, Seek as _};
    output.rewind()?;
    let mut text = String::new();
    output.read_to_string(&mut text)?;
    assert_eq!(
        serde_json::from_str::<Value>(text.trim())?["instances"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    f.joined().await?;
    f.shutdown().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_missing_flags_project_denial_and_provider_failure_are_finite() -> Result {
    let f = Fixture::new().await?;
    let project = f.project.to_string();
    for args in [
        vec!["now"],
        vec![
            "now",
            "--project",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "--provider",
            "now",
        ],
        vec!["now", "--project", &project, "--provider", "missing"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_graphit"))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .current_dir(&f.directory)
            .arg("--global-dir")
            .arg(f.paths.root())
            .args(args)
            .output()
            .await?;
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        let diagnostic = String::from_utf8(result.stderr)?;
        assert!(
            !diagnostic.contains(f.paths.root().to_str().unwrap())
                && !diagnostic.contains("now-protocol-fixture")
        );
    }
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    f.shutdown().await
}

// Python's standard-library PTY gives the compiled child a real terminal without unsafe Rust.
const PTY: &str = r#"
import errno, fcntl, os, pty, struct, subprocess, sys, termios
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
child = subprocess.Popen(sys.argv[1:], stdin=subprocess.DEVNULL, stdout=slave, stderr=subprocess.PIPE)
os.close(slave)
print('PID:' + str(child.pid), flush=True)
try:
    while True:
        try:
            data = os.read(master, 4096)
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
        if not data:
            break
        sys.stdout.buffer.write(data)
        sys.stdout.buffer.flush()
finally:
    os.close(master)
    if child.poll() is None:
        child.terminate()
    _, errors = child.communicate(timeout=8)
    sys.stdout.buffer.write(errors)
    print('EXIT:' + str(child.returncode), flush=True)
"#;
async fn terminal_until(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    text: &mut String,
    needle: &str,
) -> Result {
    tokio::time::timeout(WAIT, async {
        let mut bytes = [0; 4096];
        let mut pending = Vec::new();
        while !text.contains(needle) {
            let count = reader.read(&mut bytes).await?;
            if count == 0 {
                return Err(io::Error::other("terminal ended before expected view"));
            }
            pending.extend_from_slice(&bytes[..count]);
            let complete = match std::str::from_utf8(&pending) {
                Ok(_) => pending.len(),
                Err(error) if error.error_len().is_none() => error.valid_up_to(),
                Err(_) => return Err(io::Error::other("invalid terminal UTF-8")),
            };
            text.push_str(std::str::from_utf8(&pending[..complete]).map_err(io::Error::other)?);
            pending.drain(..complete);
            if text.len() > 65536 {
                return Err(io::Error::other("unexpected terminal output"));
            }
        }
        Ok::<_, io::Error>(())
    })
    .await??;
    Ok(())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_real_pty_replaces_shrinking_view_escapes_controls_and_restores_terminal() -> Result
{
    let mut f = Fixture::new().await?;
    let mut bridge = Command::new("python3")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .current_dir(&f.directory)
        .args([
            "-u",
            "-c",
            PTY,
            env!("CARGO_BIN_EXE_graphit"),
            "--global-dir",
        ])
        .arg(f.paths.root())
        .args([
            "now",
            "--project",
            &f.project.to_string(),
            "--provider",
            "now",
            "--version",
            "feature/β",
            "--instance",
            "a",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut reader = BufReader::new(bridge.stdout.take().unwrap());
    let mut line = String::new();
    tokio::time::timeout(WAIT, reader.read_line(&mut line)).await??;
    let pid: u32 = line.trim().strip_prefix("PID:").ok_or("PTY pid")?.parse()?;
    let mut view = String::new();
    terminal_until(&mut reader, &mut view, "\\u{202e}title").await?;
    assert!(view.contains("Café β") && view.contains("feature/β"));
    assert!(view.contains("\\u{001b}[31m\\u{0085}\\u{202e}title"));
    assert!(
        !view.contains("\u{001b}[31m") && !view.contains('\u{0085}') && !view.contains('\u{202e}')
    );
    assert!(view.contains("9007199254740991"));
    terminal_until(&mut reader, &mut view, "complete snapshot.").await?;
    assert!(view.contains("\u{001b}[1;1HNow  ") && view.contains("Provider now"));
    assert!(view.contains("\u{001b}[24;1HMore items or text"));
    assert!(!view.contains("\u{001b}[25;1H"));
    f.empty().await?;
    terminal_until(
        &mut reader,
        &mut view,
        "No online instances match these filters.",
    )
    .await?;
    assert_eq!(view.matches("\u{001b}[2J").count(), 2);
    let last = view.rsplit("\u{001b}[2J").next().unwrap();
    assert!(!last.contains("guide") && !last.contains("title"));
    signal(pid, "-TERM").await?;
    terminal_until(&mut reader, &mut view, "EXIT:130").await?;
    assert!(view.contains("\u{001b}[?1049h") && view.contains("\u{001b}[?1049l"));
    stopped(&mut bridge, 0).await?;
    f.joined().await?;
    f.shutdown().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_signal_interrupts_a_live_pipe_that_stops_reading() -> Result {
    let mut f = Fixture::with_large_title(true).await?;
    let mut child = f.cli().spawn()?;
    let mut reader = child.stdout.take().unwrap();
    let mut prefix = vec![0; 8192];
    tokio::time::timeout(WAIT, reader.read_exact(&mut prefix)).await??;
    assert!(prefix.starts_with(b"{\"type\":\"snapshot\""));
    signal(child.id().unwrap(), "-TERM").await?;
    let status = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    if status.is_err() {
        drop(reader);
        let _ = tokio::time::timeout(WAIT, child.wait()).await;
        f.joined().await?;
        f.shutdown().await?;
        return Err(
            "SIGTERM did not stop Now while its stdout pipe remained open but unread".into(),
        );
    }
    assert_eq!(status??.code(), Some(130));
    drop(reader);
    f.joined().await?;
    f.shutdown().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finite_pipe_entry_restores_only_changed_descriptor_flags() -> Result {
    for nonblocking in [false, true] {
        let mut f = Fixture::new().await?;
        let (output, mut reader) = std::os::unix::net::UnixStream::pair()?;
        output.set_nonblocking(nonblocking)?;
        let before = rustix::fs::fcntl_getfl(&output)?;
        let cancel = Cancellation::default();
        let original = graphit_hub::policy::RequestContext::new(
            wire::content(),
            &cancel,
            Instant::now() + Duration::from_secs(2),
        )?;
        let result = graphit_app::now_entry::run_pipe(
            &f.paths,
            &reqwest::Client::new(),
            graphit_app::now_entry::Arguments {
                project: f.project,
                provider: "now".into(),
                version: Some("feature/β".into()),
                instance: Some("a".into()),
            },
            (native(), wire::content()),
            &original,
            &output,
        )
        .await;
        assert_eq!(result.unwrap_err().to_string(), "Now request expired");
        assert_eq!(rustix::fs::fcntl_getfl(&output)?, before);
        drop(output);
        use std::io::Read as _;
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        assert_eq!(
            serde_json::from_str::<Value>(text.trim())?["instances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        f.joined().await?;
        f.shutdown().await?;
    }
    Ok(())
}
