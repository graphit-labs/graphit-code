use super::*;

#[path = "open_ui/on_demand.rs"]
mod on_demand;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_ui_reuses_the_live_foreground_browser_and_preserves_headless_daemon() {
    let mut broker = Broker::new().await;
    let fixture = prepared(&broker).await;
    for browser in [true, false] {
        let auth_before = fs::read(fixture.paths.auth_file()).unwrap();
        let owner = fixture
            .start(browser.then(|| config(&broker.issuer)))
            .unwrap();
        let address = owner.address().unwrap();
        let url = format!("http://{address}/task/gantt");
        let cancellation = Cancellation::default();
        let stop = cancellation.clone();
        let running = tokio::spawn(async move { owner.run(&stop).await });
        let absent_path = fixture.temp.path().join("no-desktop-programs");
        fs::create_dir_all(&absent_path).unwrap();
        let command = |no_open| {
            let mut command = fixture.command();
            command
                .current_dir(fixture.temp.path())
                .env("PATH", &absent_path)
                .arg("ui");
            if no_open {
                command.arg("--no-open");
            }
            command
        };
        let first = command(true);
        let second = command(true);
        let (first, second) = tokio::join!(
            tokio::task::spawn_blocking(move || {
                let mut c = first;
                c.output().unwrap()
            }),
            tokio::task::spawn_blocking(move || {
                let mut c = second;
                c.output().unwrap()
            })
        );
        for output in [first.unwrap(), second.unwrap()] {
            if browser {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), url);
            } else {
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains("browser_not_configured"));
                assert!(output.stdout.is_empty());
            }
        }
        let mut opening = command(false);
        let output = tokio::task::spawn_blocking(move || opening.output().unwrap())
            .await
            .unwrap();
        assert!(!output.status.success());
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        if browser {
            assert!(
                diagnostic.contains(&url) && diagnostic.contains("desktop opener failed"),
                "{diagnostic}"
            );
        } else {
            assert!(
                diagnostic.contains("browser_not_configured"),
                "{diagnostic}"
            );
        }
        let config = fixture.snapshot(None);
        let policy = HubResourcePolicy::resolve(&config).unwrap();
        let signal = Cancellation::default();
        let original = policy.request_context(Instant::now(), &signal).unwrap();
        let native = NativeDocumentFileLimits::new(65536, 4096, 32, 4096).unwrap();
        let record = graphit_daemon::source_discovery::read_discovery(
            &fixture.paths,
            native,
            original.snapshot(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(record.address(), address);
        let status = graphit_app::daemon_control::request(
            &fixture.paths,
            address,
            &client(),
            native,
            &original,
            graphit_app::daemon_control::Operation::Status,
            None,
        )
        .await
        .unwrap();
        assert_eq!(status.pid, std::process::id());
        assert_eq!(status.state, graphit_app::daemon_control::State::Running);
        assert!(!fixture.paths.root().join("services").exists());
        assert_eq!(fs::read(fixture.paths.auth_file()).unwrap(), auth_before);
        cancellation.cancel();
        running.await.unwrap().unwrap();
        let signal = Cancellation::default();
        let original = policy.request_context(Instant::now(), &signal).unwrap();
        assert!(
            graphit_daemon::source_discovery::read_discovery(
                &fixture.paths,
                native,
                original.snapshot()
            )
            .unwrap()
            .is_none()
        );
    }
    broker.stop.cancel();
    (&mut broker.server).await.unwrap();
}
