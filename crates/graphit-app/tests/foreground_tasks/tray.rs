use super::*;
use futures_util::{FutureExt, StreamExt};
use std::collections::BTreeMap;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use zbus::zvariant::{OwnedValue, Value as BusValue};

#[path = "../support/broker_oauth.rs"]
mod oauth;

struct Watcher {
    registered: Arc<Mutex<Vec<String>>>,
    announce: tokio::sync::mpsc::UnboundedSender<String>,
}

fn terminate_tray(pid: u32) -> std::io::Result<()> {
    let pid = rustix::process::Pid::from_raw(i32::try_from(pid).map_err(std::io::Error::other)?)
        .ok_or_else(|| std::io::Error::other("invalid owned tray PID"))?;
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(std::io::Error::from)
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(&self, service: String) {
        self.registered.lock().unwrap().push(service.clone());
        self.announce.send(service).unwrap();
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.registered.lock().unwrap().clone()
    }
}

type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

fn flatten_layout((id, properties, children): Layout, entries: &mut BTreeMap<String, i32>) {
    if let Some(label) = properties.get("label") {
        entries.insert(<&str>::try_from(label).unwrap().to_owned(), id);
    }
    for child in children {
        let BusValue::Structure(fields) = &*child else {
            panic!("Dbusmenu child is not a structure")
        };
        let fields = fields.fields();
        let id = i32::try_from(&fields[0]).unwrap();
        let properties = HashMap::try_from(OwnedValue::try_from(&fields[1]).unwrap()).unwrap();
        let children = Vec::try_from(OwnedValue::try_from(&fields[2]).unwrap()).unwrap();
        flatten_layout((id, properties, children), entries);
    }
}

async fn menu_entries(menu: &zbus::Proxy<'_>) -> BTreeMap<String, i32> {
    let (_, root): (u32, Layout) = menu
        .call("GetLayout", &(0i32, -1i32, Vec::<String>::new()))
        .await
        .unwrap();
    let mut entries = BTreeMap::new();
    flatten_layout(root, &mut entries);
    entries
}

async fn wait_menu(
    menu: &zbus::Proxy<'_>,
    expected: &str,
    predicate: impl Fn(&BTreeMap<String, i32>) -> bool,
) -> BTreeMap<String, i32> {
    let mut layout = menu.receive_signal("LayoutUpdated").await.unwrap();
    let mut properties = menu.receive_signal("ItemsPropertiesUpdated").await.unwrap();
    let mut observed = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let entries = menu_entries(menu).await;
            observed = entries.keys().cloned().collect::<Vec<_>>();
            if predicate(&entries) {
                return entries;
            }
            tokio::select! {
                signal=layout.next()=>{signal.expect("tray closed before expected layout");}
                signal=properties.next()=>{signal.expect("tray closed before expected properties");}
            }
        }
    })
    .await;
    result.unwrap_or_else(|_| {
        panic!("expected tray state not published ({expected}); observed labels: {observed:?}")
    })
}

async fn click(menu: &zbus::Proxy<'_>, label: &str) {
    let entries = menu_entries(menu).await;
    let id = *entries
        .get(label)
        .unwrap_or_else(|| panic!("missing tray action {label}"));
    menu.call::<_, _, ()>("Event", &(id, "clicked", BusValue::from(0i32), 0u32))
        .await
        .unwrap();
}

async fn private_bus(test_name: &str) -> bool {
    if std::env::var_os("GRAPHIT_PRIVATE_TRAY_BUS").is_some() {
        return true;
    }
    let exe = std::env::current_exe().unwrap();
    let test_name = test_name.to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new("dbus-run-session")
            .arg("--")
            .arg(exe)
            .args([
                "--exact",
                &test_name,
                "--include-ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GRAPHIT_PRIVATE_TRAY_BUS", "1")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed;"),
        "private D-Bus child did not run its selected test"
    );
    false
}

fn tray_command(fixture: &Prepared, opener: &std::path::Path) -> Command {
    let mut command = fixture.command();
    command
        .current_dir(fixture.temp.path())
        .env("PATH", opener)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            std::env::var_os("DBUS_SESSION_BUS_ADDRESS").unwrap(),
        )
        .env("XDG_RUNTIME_DIR", fixture.temp.path())
        .arg("tray");
    command
}

async fn status(fixture: &Prepared) -> graphit_app::daemon_control::Reply {
    let policy = HubResourcePolicy::resolve(&fixture.snapshot(None)).unwrap();
    let cancel = Cancellation::default();
    let original = policy.request_context(Instant::now(), &cancel).unwrap();
    let native = NativeDocumentFileLimits::new(65536, 4096, 32, 4096).unwrap();
    let discovery = graphit_daemon::source_discovery::read_discovery(
        &fixture.paths,
        native,
        original.snapshot(),
    )
    .unwrap()
    .unwrap();
    graphit_app::daemon_control::request(
        &fixture.paths,
        discovery.address(),
        &client(),
        native,
        &original,
        graphit_app::daemon_control::Operation::Status,
        None,
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "native desktop qualification requires dbus-run-session and dbus-daemon"]
async fn late_watcher_registers_the_existing_tray_and_signal_preserves_the_headless_daemon() {
    if !private_bus(
        "tray::late_watcher_registers_the_existing_tray_and_signal_preserves_the_headless_daemon",
    )
    .await
    {
        return;
    }
    let mut broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    let owner = fixture.start(None).unwrap();
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let running = tokio::spawn(async move { owner.run(&signal).await });
    let before = status(&fixture).await;
    let empty = fixture.temp.path().join("empty-path");
    fs::create_dir(&empty).unwrap();
    let observer = zbus::Connection::session().await.unwrap();
    let bus = zbus::fdo::DBusProxy::new(&observer).await.unwrap();
    assert!(
        !bus.name_has_owner(
            zbus::names::BusName::try_from("org.kde.StatusNotifierWatcher").unwrap(),
        )
        .await
        .unwrap()
    );
    let mut names = bus.receive_name_owner_changed().await.unwrap();
    let mut tray = tokio::process::Command::from(tray_command(&fixture, &empty));
    tray.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut tray = tray.spawn().unwrap();
    let tray_pid = tray.id().unwrap();
    let service = format!("org.kde.StatusNotifierItem-{tray_pid}-1");
    let mut watcher = None;
    let result = std::panic::AssertUnwindSafe(async {
        tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(change) = names.next().await {
                let args = change.args().unwrap();
                if args.name().as_str() == service {
                    return;
                }
            }
            panic!("private bus closed before tray acquired its SNI name");
        })
        .await
        .unwrap();
        let item = zbus::Proxy::new(
            &observer,
            service.as_str(),
            "/StatusNotifierItem",
            "org.kde.StatusNotifierItem",
        )
        .await
        .unwrap();
        let path: zbus::zvariant::OwnedObjectPath = item.get_property("Menu").await.unwrap();
        let menu = zbus::Proxy::new(
            &observer,
            service.as_str(),
            path.as_str(),
            "com.canonical.dbusmenu",
        )
        .await
        .unwrap();
        let expected = format!("Daemon running (PID {})", before.pid);
        // Refresh follows spawn: this state proves the initial offline registration returned.
        wait_menu(&menu, "tray ready before watcher", |entries| {
            entries.keys().any(|label| label.contains(&expected))
        })
        .await;
        assert!(tray.try_wait().unwrap().is_none());
        let (registered, mut registrations) = tokio::sync::mpsc::unbounded_channel();
        watcher = Some(
            zbus::connection::Builder::session()
                .unwrap()
                .name("org.kde.StatusNotifierWatcher")
                .unwrap()
                .serve_at(
                    "/StatusNotifierWatcher",
                    Watcher {
                        registered: Arc::new(Mutex::new(Vec::new())),
                        announce: registered,
                    },
                )
                .unwrap()
                .build()
                .await
                .unwrap(),
        );
        let registered = tokio::time::timeout(Duration::from_secs(20), registrations.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(registered, service);
        assert_eq!(tray.id(), Some(tray_pid));
        let after = status(&fixture).await;
        assert_eq!(before.pid, after.pid);
        assert_eq!(after.state, graphit_app::daemon_control::State::Running);
    })
    .catch_unwind()
    .await;
    let was_running = tray.try_wait().unwrap().is_none();
    let terminated = was_running.then(|| terminate_tray(tray_pid));
    let joined = tokio::time::timeout(Duration::from_secs(20), tray.wait()).await;
    if joined.is_err() {
        tray.kill().await.unwrap();
    }
    let output = tray.wait_with_output().await;
    let after_signal = status(&fixture).await;
    cancel.cancel();
    running.await.unwrap().unwrap();
    broker.stop.cancel();
    (&mut broker.server).await.unwrap();
    if let Some(watcher) = watcher {
        watcher.close().await.unwrap();
    }
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
    assert!(was_running);
    terminated.unwrap().unwrap();
    assert!(joined.is_ok(), "tray did not join after SIGTERM");
    let output = output.unwrap();
    assert_eq!(
        output.status.code(),
        Some(130),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(after_signal.pid, before.pid);
    assert_eq!(
        after_signal.state,
        graphit_app::daemon_control::State::Running
    );
    assert!(
        !bus.name_has_owner(zbus::names::BusName::try_from(service.as_str()).unwrap())
            .await
            .unwrap()
    );
}

async fn opened_url(
    listener: &tokio::net::UnixListener,
    menu: &zbus::Proxy<'_>,
) -> (String, tokio::net::UnixStream) {
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let (peer, _) = listener.accept().await.unwrap();
        let mut peer = BufReader::new(peer);
        let mut url = String::new();
        assert_ne!(peer.read_line(&mut url).await.unwrap(), 0);
        (url.trim_end().to_owned(), peer.into_inner())
    })
    .await;
    match result {
        Ok(url) => url,
        Err(_) => panic!(
            "controlled desktop helper did not announce URL; menu labels: {:?}",
            menu_entries(menu).await.keys().collect::<Vec<_>>()
        ),
    }
}

async fn desktop_alive(peer: &mut tokio::net::UnixStream) {
    tokio::time::timeout(Duration::from_secs(5), async {
        peer.write_all(b"p").await.unwrap();
        let mut reply = [0];
        peer.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, *b"p");
    })
    .await
    .expect("desktop application must survive launcher and tray completion");
}

fn controlled_opener(fixture: &Prepared) -> (std::path::PathBuf, tokio::net::UnixListener) {
    let directory = fixture.temp.path().join("desktop-programs");
    fs::create_dir(&directory).unwrap();
    let socket = fixture.temp.path().join("desktop.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    // Model a desktop helper that exits after spawning an application. Closing
    // the test's connection ends the application, including during panic cleanup.
    let file = directory.join("xdg-open");
    dream_peer::install(&file).unwrap();
    fs::write(
        file.with_extension("json"),
        serde_json::to_vec(&socket).unwrap(),
    )
    .unwrap();
    (directory, listener)
}

async fn runtime_command(fixture: &Prepared, arguments: &[&str]) -> std::process::Output {
    let mut command = fixture.command();
    command
        .current_dir(fixture.temp.path())
        .args(arguments)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            std::env::var_os("DBUS_SESSION_BUS_ADDRESS").unwrap(),
        )
        .env("XDG_RUNTIME_DIR", fixture.temp.path());
    tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "native desktop qualification requires dbus-run-session and dbus-daemon"]
async fn native_menu_login_quit_failure_recovery_and_signal_join_use_real_consumers() {
    if !private_bus(
        "tray::native_menu_login_quit_failure_recovery_and_signal_join_use_real_consumers",
    )
    .await
    {
        return;
    }
    let mut web = Broker::new().await;
    let fixture = prepared(&web).await;
    let cancel = Cancellation::default();
    let global = ConfigStore::global(&fixture.paths).unwrap();
    for (key, value) in [
        ("agent", json!("codex")),
        ("mcp.port", json!(0)),
        ("task.browser", config(&web.issuer)),
    ] {
        global
            .set(
                ConfigKey::new(key).unwrap(),
                value,
                Duration::from_secs(5),
                &cancel,
            )
            .unwrap();
    }
    let (registered, mut registrations) = tokio::sync::mpsc::unbounded_channel();
    let watcher = zbus::connection::Builder::session()
        .unwrap()
        .name("org.kde.StatusNotifierWatcher")
        .unwrap()
        .serve_at(
            "/StatusNotifierWatcher",
            Watcher {
                registered: Arc::new(Mutex::new(Vec::new())),
                announce: registered,
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let (opener, announcements) = controlled_opener(&fixture);
    let mut native = oauth::Broker::new().await;
    let mut tray = tokio::process::Command::from(tray_command(&fixture, &opener));
    tray.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut tray = tray.spawn().unwrap();
    let service = tokio::time::timeout(Duration::from_secs(20), registrations.recv())
        .await
        .unwrap()
        .unwrap();
    let item = zbus::Proxy::new(
        &watcher,
        service.as_str(),
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
    )
    .await
    .unwrap();
    let menu_path: zbus::zvariant::OwnedObjectPath = item.get_property("Menu").await.unwrap();
    let menu = zbus::Proxy::new(
        &watcher,
        service.as_str(),
        menu_path.as_str(),
        "com.canonical.dbusmenu",
    )
    .await
    .unwrap();
    let result = std::panic::AssertUnwindSafe(async {
        wait_menu(&menu, "initial daemon stopped", |entries| {
            entries.keys().any(|key| key.contains("Daemon stopped"))
        })
        .await;
        click(&menu, "Start daemon").await;
        wait_menu(&menu, "Start daemon acknowledged running", |entries| {
            entries.keys().any(|key| key.contains("Daemon running"))
        })
        .await;
        let first = status(&fixture).await;
        assert_ne!(first.pid, tray.id().unwrap());
        assert!(!fixture.paths.root().join("services").exists());
        click(&menu, "Restart daemon").await;
        // Wait for the authoritative new PID, rather than the still-visible pre-action label.
        wait_menu(&menu, "Restart daemon acknowledged new PID", |entries| {
            entries
                .keys()
                .any(|key| key.contains("Daemon running") && !key.contains(&first.pid.to_string()))
        })
        .await;
        let restarted = status(&fixture).await;
        assert_ne!(first.pid, restarted.pid);
        menu.call::<_, _, bool>("AboutToShow", &(0i32,))
            .await
            .unwrap();
        click(&menu, "Open interface").await;
        let (opened, mut window) = opened_url(&announcements, &menu).await;
        let opened = url::Url::parse(&opened).unwrap();
        assert_eq!(opened.path(), "/task/gantt");
        assert_eq!(opened.host_str(), Some("127.0.0.1"));
        wait_menu(&menu, "Open interface completed", |entries| {
            entries.keys().any(|key| key.contains("Daemon running"))
        })
        .await;
        desktop_alive(&mut window).await;
        let original_account = runtime_command(&fixture, &["account", "list"]).await;
        assert!(original_account.status.success());
        let original_accounts: Value = serde_json::from_slice(&original_account.stdout).unwrap();
        assert!(original_accounts["active"].is_null());
        let provider = runtime_command(
            &fixture,
            &[
                "provider",
                "add",
                "native",
                "--type",
                "broker",
                "--broker-endpoint",
                native.origin(),
                "--embedding-mode",
                "broker",
                "--rerank-mode",
                "broker",
                "--s3-credential-source",
                "broker",
            ],
        )
        .await;
        assert!(
            provider.status.success(),
            "native provider configuration failed"
        );
        let mut login = tokio::process::Command::from(fixture.command());
        login
            .current_dir(fixture.temp.path())
            .args([
                "--non-interactive",
                "login",
                "--provider",
                "native",
                "--profile",
                "tray-profile",
                "--s3-disabled",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut login = login.spawn().unwrap();
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut announcement = BufReader::new(login.stderr.take().unwrap());
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(20), announcement.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        let authorization = line
            .trim()
            .strip_prefix("Authorize native Broker login: ")
            .expect("native login did not announce authorization");
        let callback = native.callback(authorization);
        assert!(
            oauth::loopback_client()
                .get(callback)
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
        );
        assert!(login.wait_with_output().await.unwrap().status.success());
        click(&menu, "Status").await;
        wait_menu(&menu, "configured native profile listed", |entries| {
            entries.contains_key("tray-profile")
        })
        .await;
        click(&menu, "tray-profile").await;
        let (authorization, _login_window) = opened_url(&announcements, &menu).await;
        assert!(
            client()
                .get(native.callback(&authorization))
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
        );
        wait_menu(
            &menu,
            "native profile reauthentication completed",
            |entries| entries.keys().any(|key| key.contains("Daemon running")),
        )
        .await;
        let before = runtime_command(&fixture, &["account", "show", "tray-profile"]).await;
        assert!(before.status.success());
        let profile: Value = serde_json::from_slice(&before.stdout).unwrap();
        assert_eq!(
            profile["profile"]["s3_disabled"],
            json!(true),
            "tray reauthentication must preserve the existing S3 choice"
        );
        let _ = native.state.started.notified().now_or_never();
        native.state.pause.store(true, Ordering::SeqCst);
        click(&menu, "tray-profile").await;
        let (authorization, _cancelled_window) = opened_url(&announcements, &menu).await;
        let callback = native.callback(&authorization);
        let callback =
            tokio::spawn(async move { oauth::loopback_client().get(callback).send().await });
        tokio::time::timeout(Duration::from_secs(20), native.state.started.notified())
            .await
            .unwrap();
        let key_path = fixture.paths.root().join("daemon/mcp.key");
        let key = fs::read(&key_path).unwrap();
        let key_permissions = fs::metadata(&key_path).unwrap().permissions();
        let saved_key = key_path.with_extension("fixture-original");
        let invalid = key
            .iter()
            .map(|b| if *b == b'a' { b'b' } else { b'a' })
            .collect::<Vec<_>>();
        // Retain the original file observation; restoring only its bytes would change mtime.
        fs::rename(&key_path, &saved_key).unwrap();
        fs::write(&key_path, &invalid).unwrap();
        fs::set_permissions(&key_path, key_permissions).unwrap();
        let rejected = std::panic::AssertUnwindSafe(async {
            click(&menu, "Quit and stop daemon").await;
            let failed = wait_menu(&menu, "Quit Stop rejected by Runtime 401", |entries| {
                entries.keys().any(|key| key.contains("Action failed"))
            })
            .await;
            assert!(
                failed.keys().any(|key| key.contains("401")),
                "Stop must be denied by the real Runtime control"
            );
            assert!(tray.try_wait().unwrap().is_none());
        })
        .catch_unwind()
        .await;
        fs::rename(&saved_key, &key_path).unwrap();
        if let Err(panic) = rejected {
            std::panic::resume_unwind(panic);
        }
        native.state.pause.store(false, Ordering::SeqCst);
        native.state.release.notify_waiters();
        let _ = tokio::time::timeout(Duration::from_secs(20), callback)
            .await
            .unwrap()
            .unwrap();
        let after = runtime_command(&fixture, &["account", "show", "tray-profile"]).await;
        assert!(
            after.status.success(),
            "account read after cancelled Login failed: {}",
            String::from_utf8_lossy(&after.stderr)
        );
        assert_eq!(
            profile,
            serde_json::from_slice::<Value>(&after.stdout).unwrap(),
            "cancelled login must not publish a profile"
        );
        click(&menu, "Status").await;
        wait_menu(
            &menu,
            "Status recovered after restoring original key",
            |entries| entries.keys().any(|key| key.contains("Daemon running")),
        )
        .await;
        assert_eq!(status(&fixture).await.pid, restarted.pid);
        click(&menu, "Quit and stop daemon").await;
        let exit = tokio::time::timeout(Duration::from_secs(20), tray.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(exit.success());
        desktop_alive(&mut window).await;
        let bus = zbus::fdo::DBusProxy::new(&watcher).await.unwrap();
        assert!(
            !bus.name_has_owner(zbus::names::BusName::try_from(service.as_str()).unwrap())
                .await
                .unwrap()
        );
        fixture.assert_clean();
        // The OAuth fixture proves Login, not Native inventory/Presence service hosting.
        // Restore its original account selection for the independent signal journey.
        let restored = runtime_command(
            &fixture,
            &["--non-interactive", "logout", "--profile", "tray-profile"],
        )
        .await;
        assert!(
            restored.status.success(),
            "original profile restoration failed: {}",
            String::from_utf8_lossy(&restored.stderr)
        );
        let mut signalled = tokio::process::Command::from(tray_command(&fixture, &opener));
        signalled
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut signalled = signalled.spawn().unwrap();
        let service = tokio::time::timeout(Duration::from_secs(20), registrations.recv())
            .await
            .unwrap()
            .unwrap();
        let item = zbus::Proxy::new(
            &watcher,
            service.as_str(),
            "/StatusNotifierItem",
            "org.kde.StatusNotifierItem",
        )
        .await
        .unwrap();
        let path: zbus::zvariant::OwnedObjectPath = item.get_property("Menu").await.unwrap();
        let menu = zbus::Proxy::new(
            &watcher,
            service.as_str(),
            path.as_str(),
            "com.canonical.dbusmenu",
        )
        .await
        .unwrap();
        wait_menu(&menu, "second tray initially stopped", |entries| {
            entries.keys().any(|key| key.contains("Daemon stopped"))
        })
        .await;
        click(&menu, "Start daemon").await;
        wait_menu(&menu, "second tray Start acknowledged running", |entries| {
            entries.keys().any(|key| key.contains("Daemon running"))
        })
        .await;
        let before = status(&fixture).await;
        terminate_tray(signalled.id().unwrap()).unwrap();
        let exit = tokio::time::timeout(Duration::from_secs(20), signalled.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(exit.code(), Some(130));
        assert_eq!(status(&fixture).await.pid, before.pid);
        assert!(
            !bus.name_has_owner(zbus::names::BusName::try_from(service.as_str()).unwrap())
                .await
                .unwrap()
        );
    })
    .catch_unwind()
    .await;
    native.state.pause.store(false, Ordering::SeqCst);
    native.state.release.notify_waiters();
    let stop = runtime_command(&fixture, &["daemon", "stop"]).await;
    if !stop.status.success() {
        eprintln!(
            "owned daemon cleanup failed: {}",
            String::from_utf8_lossy(&stop.stderr)
        );
    }
    if tray.try_wait().unwrap().is_none() {
        tray.kill().await.unwrap();
    }
    native.state.release.notify_waiters();
    native.stop.cancel();
    (&mut native.server).await.unwrap();
    web.stop.cancel();
    (&mut web.server).await.unwrap();
    watcher.close().await.unwrap();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
    assert!(stop.status.success(), "owned daemon cleanup failed");
}
