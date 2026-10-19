//! One actual CLI-launched private daemon, two PROJECTs and the signed local Browser issuer.
use super::*;
use std::path::Path;

fn command(fixture: &Prepared, runtime: &Path) -> Command {
    let mut command = fixture.command();
    command
        .current_dir(fixture.temp.path())
        .env("XDG_RUNTIME_DIR", runtime)
        .env("XDG_CONFIG_HOME", runtime)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", runtime.join("absent-bus").display()),
        );
    command
}

// Panic cleanup uses the existing authenticated stop for this installation.
// It cannot address the resident daemon because every command retains its
// fixture's GLOBAL_DIR, HOME and absent private user-manager bus.
struct PrivateOwner<'a> {
    fixture: &'a Prepared,
    runtime: &'a Path,
    stopped: bool,
}
impl Drop for PrivateOwner<'_> {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = command(self.fixture, self.runtime)
                .args(["daemon", "stop"])
                .output();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_ui_starts_one_private_browser_for_two_projects() {
    let mut broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    let root = fixture.temp.path().join("on-demand-other");
    fs::create_dir(&root).unwrap();
    let other = ProjectPaths::at(&root, &fixture.paths).unwrap();
    let cancellation = Cancellation::default();
    let other_id = ProjectIdentityStore::at(&other)
        .writer(Duration::from_secs(5), &cancellation)
        .unwrap()
        .ensure(&ProjectMetadata::new("on-demand-other").unwrap())
        .unwrap()
        .id();
    let primary_id = ProjectIdentityStore::at(&fixture.project)
        .load()
        .unwrap()
        .unwrap()
        .id();
    assert_ne!(primary_id, other_id);
    // Use the same productive bootstrap as Seed, registering the real inventory
    // before the CLI starts its managed owner. No HTTP owner is left running.
    fixture
        .start_projects(None, &[&fixture.project, &other])
        .unwrap()
        .shutdown()
        .await
        .unwrap();
    fixture.assert_clean();
    let global = ConfigStore::global(&fixture.paths).unwrap();
    for (key, value) in [
        ("agent", json!("codex")),
        ("modules.ast", json!(false)),
        ("modules.knowledge", json!(false)),
        ("modules.memory", json!(false)),
        ("modules.embedding", json!(false)),
        ("modules.dream", json!(false)),
        ("modules.task", json!(true)),
        ("task.browser", config(&broker.issuer)),
    ] {
        global
            .set(
                ConfigKey::new(key).unwrap(),
                value,
                Duration::from_secs(5),
                &cancellation,
            )
            .unwrap();
    }
    let runtime = fixture.temp.path().join("private-runtime");
    fs::create_dir(&runtime).unwrap();
    let mut owned = PrivateOwner {
        fixture: &fixture,
        runtime: &runtime,
        stopped: false,
    };
    let first = command(&fixture, &runtime);
    let second = command(&fixture, &runtime);
    let open = |mut command: Command| {
        command.args(["ui", "--no-open"]);
        tokio::task::spawn_blocking(move || command.output().unwrap())
    };
    let (first, second) = tokio::join!(open(first), open(second));
    let first = first.unwrap();
    let second = second.unwrap();
    assert!(
        first.status.success() && second.status.success(),
        "concurrent on-demand UI failed: first ({}) {}; second ({}) {}",
        first.status,
        String::from_utf8_lossy(&first.stderr),
        second.status,
        String::from_utf8_lossy(&second.stderr),
    );
    for output in [&first, &second] {
        assert!(output.stderr.is_empty());
    }
    let entry = String::from_utf8(first.stdout).unwrap();
    assert_eq!(entry, String::from_utf8(second.stdout).unwrap());
    let entry = entry.trim();
    let record: graphit_daemon::source_discovery::DiscoveryRecord =
        serde_json::from_slice(&fs::read(fixture.paths.root().join("daemon/mcp.json")).unwrap())
            .unwrap();
    let origin = format!("http://{}", record.address());
    assert_eq!(entry, format!("{origin}/task/gantt"));
    assert_ne!(
        record.pid(),
        std::process::id(),
        "the CLI must start the real process"
    );
    let again = tokio::task::spawn_blocking({
        let mut command = command(&fixture, &runtime);
        move || command.args(["ui", "--no-open"]).output().unwrap()
    })
    .await
    .unwrap();
    assert!(again.status.success());
    assert_eq!(String::from_utf8(again.stdout).unwrap().trim(), entry);
    let retained: graphit_daemon::source_discovery::DiscoveryRecord =
        serde_json::from_slice(&fs::read(fixture.paths.root().join("daemon/mcp.json")).unwrap())
            .unwrap();
    assert_eq!(retained.pid(), record.pid());
    assert_eq!(retained.address(), record.address());

    let client = client();
    let assets = client.get(entry).send().await.unwrap();
    assert_eq!(assets.status(), StatusCode::OK);
    assert_eq!(
        assets.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    assert_eq!(
        assets.bytes().await.unwrap().as_ref(),
        include_bytes!("../../../assets/ui/index.html")
    );
    assert_eq!(
        client
            .get(format!("{origin}/auth/session"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let cookie = login(&client, &origin, &broker.issuer).await;
    let contexts = read(
        &client,
        &origin,
        &cookie,
        "contexts",
        json!({"selection":{}}),
    )
    .await;
    assert_eq!(contexts.status(), StatusCode::OK);
    let contexts = contexts.json::<Value>().await.unwrap();
    let selections = contexts["contexts"].as_array().unwrap();
    assert_eq!(selections.len(), 2);
    let primary = selections
        .iter()
        .find(|selection| selection["project_id"] == primary_id.to_string())
        .unwrap();
    let sibling = selections
        .iter()
        .find(|selection| selection["project_id"] == other_id.to_string())
        .unwrap();
    assert_eq!(primary["project_version"], "main");
    assert_eq!(sibling["project_version"], "main");
    let catalogue = read(
        &client,
        &origin,
        &cookie,
        "catalogue",
        json!({"selection":primary,"input":{"filter":{},"page_size":20}}),
    )
    .await;
    assert_eq!(catalogue.status(), StatusCode::OK);
    let catalogue = catalogue.json::<Value>().await.unwrap();
    assert!(
        catalogue["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|task| task["id"] == fixture.task)
    );
    assert!(!catalogue.to_string().contains(&fixture.fence));

    // A valid Web session is neither a fresh PROJECT grant nor daemon authority.
    broker.issuer.allowed.store(false, Ordering::SeqCst);
    for selection in [primary, sibling] {
        assert_eq!(
            read(
                &client,
                &origin,
                &cookie,
                "catalogue",
                json!({"selection":selection,"input":{"filter":{},"page_size":20}})
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
    }
    broker.issuer.allowed.store(true, Ordering::SeqCst);
    assert_eq!(
        client
            .post(format!("{origin}/admin/daemon"))
            .header(header::ORIGIN, &origin)
            .header(header::COOKIE, &cookie)
            .json(&json!({"operation":"open_ui"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let wrong_origin = client
        .post(format!("{origin}/api/tasks/context"))
        .header(header::ORIGIN, "https://foreign.invalid")
        .header(header::COOKIE, &cookie)
        .json(&json!({"selection":primary}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);
    assert!(
        !wrong_origin
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    assert_eq!(
        read(
            &client,
            &origin,
            &cookie,
            "catalogue",
            json!({"selection":primary,"input":{"filter":{},"page_size":20}})
        )
        .await
        .status(),
        StatusCode::OK
    );

    assert_eq!(
        client
            .post(format!("{origin}/auth/logout"))
            .header(header::ORIGIN, &origin)
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        client
            .get(format!("{origin}/auth/session"))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let stopped = tokio::task::spawn_blocking({
        let mut command = command(&fixture, &runtime);
        move || command.args(["daemon", "stop"]).output().unwrap()
    })
    .await
    .unwrap();
    assert!(
        stopped.status.success(),
        "private daemon stop: {}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    fixture.assert_clean();
    let connect = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(record.address()),
    )
    .await
    .expect("local listener close check timed out")
    .unwrap_err();
    assert_eq!(connect.kind(), std::io::ErrorKind::ConnectionRefused);
    owned.stopped = true;
    assert_eq!(broker.issuer.userinfo.load(Ordering::SeqCst), 0);
    broker.stop.cancel();
    (&mut broker.server).await.unwrap();
}
