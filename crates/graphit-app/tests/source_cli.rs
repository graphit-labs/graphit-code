//! SDK entry component fixtures. Compiled CLI/foreground qualification remains ff08.
mod support {
    include!("support/source_client.rs");

    impl Fixture {
        pub async fn publish_named(
            &self,
            project: usize,
            rows: &[(&str, Option<&str>)],
        ) -> Result<(), Box<dyn std::error::Error>> {
            let corpus = self.corpus(project);
            let branch = self.projects[project].2.lance_branch();
            assert!(!branch.is_empty());
            let columns: Vec<ArrayRef> = vec![
                Arc::new(StringArray::from(
                    rows.iter().map(|r| r.0).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter().map(|r| r.1).collect::<Vec<_>>(),
                )),
            ];
            let batch = RecordBatch::try_new(source_schema(), columns)?;
            let store = graphit_data::lance_branch_store::LanceBranchStore::open(
                corpus.to_str().unwrap(),
                source_schema(),
                None,
            )
            .await?;
            store
                .capture_target(Some(&branch))
                .await?
                .materialize()
                .await?
                .prepare_merge(vec![batch], vec!["path".into()])
                .await?
                .commit()
                .await?;
            Ok(())
        }

        pub async fn advertised(
            &self,
        ) -> (Endpoint, graphit_daemon::source_discovery::DiscoveryRecord) {
            let endpoint = self.start().await;
            let record = endpoint.discovery.clone();
            (endpoint, record)
        }

        pub async fn advertised_current(
            &self,
            current: usize,
        ) -> (Endpoint, graphit_daemon::source_discovery::DiscoveryRecord) {
            let cancel = Cancellation::default();
            let original = request(&cancel);
            let listener = tcp_listener::reserve(
                Options {
                    address: std::net::Ipv4Addr::LOCALHOST.into(),
                    ports: PortSelection::Ephemeral,
                    timeout: Duration::from_secs(5),
                },
                &cancel,
            )
            .unwrap();
            let address = listener.address();
            let stop = CancellationToken::new();
            let handler = SourceServer::new(
                Arc::clone(&self.auth),
                Arc::clone(&self.projects),
                current,
                self.source_policy(Duration::from_secs(30)),
                stop.clone(),
            )
            .unwrap();
            let record = graphit_daemon::source_discovery::publish_discovery(
                &self.auth,
                &listener,
                key_limits(),
                original.snapshot(),
            )
            .unwrap();
            let task = tokio::spawn(async move { SourceServer::serve(&handler, listener).await });
            (
                Endpoint {
                    auth: Arc::clone(&self.auth),
                    discovery: record.clone(),
                    stop,
                    task,
                    address,
                },
                record,
            )
        }

        pub fn retract(&self, record: &graphit_daemon::source_discovery::DiscoveryRecord) {
            let cancel = Cancellation::default();
            let original = request(&cancel);
            assert!(
                graphit_daemon::source_discovery::remove_discovery(
                    &self.auth,
                    record,
                    key_limits(),
                    original.snapshot(),
                )
                .unwrap()
            );
            assert!(self.auth.runtime().check(original.snapshot()).is_ok());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_current_project_and_version_preserve_explicit_and_sibling_selection() -> TestResult {
    let mut fixture = support::Fixture::new();
    // The server defaults to first/main; the caller selects second/feature.
    fixture.projects = std::sync::Arc::new(vec![
        fixture.projects[0].clone(),
        fixture.projects[2].clone(),
        fixture.projects[3].clone(),
    ]);
    fixture
        .publish(0, &[("selected", Some("sibling main"))])
        .await;
    fixture
        .publish(1, &[("selected", Some("current main"))])
        .await;
    fixture
        .publish_named(2, &[("selected", Some("current feature"))])
        .await?;
    let (endpoint, _) = fixture.advertised().await;
    let current_root = fixture.projects[1].0.root().to_path_buf();
    let current_main = fixture.projects[1].2.version();
    let current_feature = fixture.projects[2].2.version();
    let sibling_root = fixture.projects[0].0.root();
    let sibling_main = fixture.projects[0].2.version();
    let client = reqwest::Client::new();
    for global in [Some(current_feature), Some("unavailable-version"), None] {
        let (local, remote) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(remote);
        let installation = fixture.paths.clone();
        let transport = client.clone();
        let root = current_root.clone();
        let version = global.map(str::to_owned);
        let address = endpoint.address;
        let serving = tokio::spawn(async move {
            let cancel = Cancellation::default();
            let startup = support::request(&cancel);
            // Selection belongs to the read-only SDK proxy; automatic checkout
            // maintenance is covered by the compiled Foreground CLI consumer.
            let proxy = graphit_app::mcp_source::SourceProxy::new(
                installation,
                address,
                transport,
                (
                    support::key_limits(),
                    support::content(),
                    Duration::from_secs(30),
                ),
                version
                    .as_deref()
                    .map(|version| (root.as_path(), Some(version))),
                &startup,
            )
            .await?;
            proxy.serve_stream(read, write).await
        });
        let peer = ().serve(local).await?;
        let cases = if global == Some(current_feature) {
            vec![
                (json!({"path":"selected"}), Ok("current feature")),
                (
                    json!({"path":"selected","project_dir":current_root}),
                    Ok("current feature"),
                ),
                (
                    json!({"path":"selected","project_version":current_main}),
                    Ok("current main"),
                ),
                (
                    json!({"path":"selected","project_dir":current_root,"project_version":current_main}),
                    Ok("current main"),
                ),
                (
                    json!({"path":"selected","project_dir":sibling_root}),
                    Ok("sibling main"),
                ),
                (
                    json!({"path":"selected","project_dir":sibling_root,"project_version":sibling_main}),
                    Ok("sibling main"),
                ),
                (
                    json!({"path":"selected","project_version":"unavailable-version"}),
                    Err("Source project/version unavailable"),
                ),
            ]
        } else if global.is_some() {
            vec![
                (
                    json!({"path":"selected"}),
                    Err("Source project/version unavailable"),
                ),
                (
                    json!({"path":"selected","project_version":current_main}),
                    Ok("current main"),
                ),
                (
                    json!({"path":"selected","project_dir":sibling_root}),
                    Ok("sibling main"),
                ),
            ]
        } else {
            vec![
                (json!({"path":"selected"}), Ok("sibling main")),
                (
                    json!({"path":"selected","project_version":sibling_main}),
                    Ok("sibling main"),
                ),
                (
                    json!({"path":"selected","project_dir":current_root,"project_version":current_main}),
                    Ok("current main"),
                ),
                (
                    json!({"path":"selected","project_dir":current_root}),
                    Err("Source project/version selection ambiguous"),
                ),
            ]
        };
        for (args, expected) in cases {
            let result = peer
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_ast_source")
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await?;
            assert_eq!(result.is_error == Some(true), expected.is_err());
            assert_eq!(
                result.content[0].as_text().unwrap().text,
                match expected {
                    Ok(text) | Err(text) => text,
                }
            );
        }
        for args in [
            json!({"path":"selected","project_version":null}),
            json!({"path":"selected","project_dir":null}),
        ] {
            assert!(
                peer.peer()
                    .call_tool(
                        CallToolRequestParams::new("graphit_ast_source")
                            .with_arguments(args.as_object().unwrap().clone())
                    )
                    .await
                    .is_err()
            );
        }
        peer.cancel().await?;
        serving.await??;
    }
    assert!(endpoint.stop().await?);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn source_proxy_rejects_non_utf8_root_before_connecting() -> TestResult {
    use std::os::unix::ffi::OsStringExt;
    let fixture = support::Fixture::new();
    let root = std::path::PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 0xff]));
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    for version in [Some("main"), None] {
        let result = graphit_app::mcp_source::SourceProxy::new(
            fixture.paths.clone(),
            "127.0.0.1:1".parse()?,
            reqwest::Client::new(),
            (
                support::key_limits(),
                support::content(),
                Duration::from_secs(30),
            ),
            Some((&root, version)),
            &original,
        )
        .await;
        match result {
            Err(error) => assert_eq!(error.kind(), io::ErrorKind::InvalidInput),
            Ok(_) => panic!("non-UTF-8 root accepted"),
        }
    }
    Ok(())
}
use graphit_app::{ast_source::SourceArguments, source_entry};
use graphit_core::file_lock::Cancellation;
use rmcp::{ServiceExt, model::CallToolRequestParams};
use serde_json::json;
use std::{fs, io, time::Duration};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_entry_uses_current_discovery_exact_source_and_preserves_read_errors() -> TestResult {
    let fixture = support::Fixture::new();
    let client = reqwest::Client::new();
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    let mut output = Vec::new();
    let arguments: SourceArguments = serde_json::from_value(json!({"path":"raw"}))?;
    let discovery = fixture.paths.root().join("daemon/mcp.json");
    // A reachable unpublished endpoint does not become an implicit destination.
    let (unpublished, unpublished_record) = fixture.advertised().await;
    fixture.retract(&unpublished_record);
    let error = source_entry::call_source(
        &fixture.paths,
        &client,
        &arguments,
        support::key_limits(),
        &original,
        &mut output,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(output.is_empty());
    assert!(!discovery.exists());
    assert!(!unpublished.stop().await?);

    fixture.file(0, "raw", "\u{feff}A\r\nβ\0\n");
    fixture.file(2, "raw", "second checkout");
    fixture.file(0, "indexed", "disk loses to available empty");
    fixture.file(0, "nullable", "physical fallback");
    fixture.file(0, "empty", "");
    fixture
        .publish(0, &[("indexed", Some("")), ("nullable", None)])
        .await;
    let (endpoint, record) = fixture.advertised().await;
    assert_eq!(record.address(), endpoint.address);
    let original_record = fs::read(&discovery)?;
    let key_path = fixture.paths.root().join("daemon/mcp.key");
    let key_before = fs::read(&key_path)?;
    for (value, expected) in [
        (json!({"path":"raw"}), "\u{feff}A\r\nβ\0\n"),
        (json!({"path":"raw","head":1}), "\u{feff}A\r"),
        (json!({"path":"raw","pattern":"absent"}), ""),
        (json!({"path":"indexed"}), ""),
        (json!({"path":"nullable"}), "physical fallback"),
        (json!({"path":"empty"}), ""),
        (
            json!({"path":"raw","project_version":"feature"}),
            "\u{feff}A\r\nβ\0\n",
        ),
        (
            json!({"path":"raw","project_dir":fixture.projects[2].0.root(), "project_version":fixture.projects[2].2.version()}),
            "second checkout",
        ),
    ] {
        output.clear();
        let arguments = serde_json::from_value(value)?;
        source_entry::call_source(
            &fixture.paths,
            &client,
            &arguments,
            support::key_limits(),
            &original,
            &mut output,
        )
        .await?;
        assert_eq!(output, expected.as_bytes());
    }
    assert_eq!(fs::read(&key_path)?, key_before);

    fs::write(&discovery, b"{\"pid\":0,\"address\":\"127.0.0.1:1\"}")?;
    output.clear();
    assert!(
        source_entry::call_source(
            &fixture.paths,
            &client,
            &arguments,
            support::key_limits(),
            &original,
            &mut output
        )
        .await
        .is_err()
    );
    assert!(output.is_empty());
    fs::write(&discovery, &original_record)?;
    let expired_cancel = Cancellation::default();
    let expired = support::request_until(
        &expired_cancel,
        std::time::Instant::now() + Duration::from_millis(50),
    );
    tokio::time::sleep(Duration::from_millis(60)).await;
    let error = source_entry::call_source(
        &fixture.paths,
        &client,
        &arguments,
        support::key_limits(),
        &expired,
        &mut output,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(output.is_empty());
    cancel.cancel();
    let error = source_entry::call_source(
        &fixture.paths,
        &client,
        &arguments,
        support::key_limits(),
        &original,
        &mut output,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(output.is_empty());

    assert!(endpoint.stop().await?);
    // Preserve the stale-discovery case after Endpoint's owned retirement.
    fs::write(&discovery, &original_record)?;
    let fresh_cancel = Cancellation::default();
    let fresh = support::request(&fresh_cancel);
    assert!(
        source_entry::call_source(
            &fixture.paths,
            &client,
            &arguments,
            support::key_limits(),
            &fresh,
            &mut output
        )
        .await
        .is_err()
    );
    assert!(output.is_empty());
    fixture.retract(&record);
    assert!(!discovery.exists());
    assert_eq!(fs::read(&key_path)?, key_before);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_entry_joins_eof_and_service_cancellation_without_retiring_the_server() -> TestResult
{
    let fixture = support::Fixture::new();
    fixture.file(0, "raw", "real service payload");
    let (endpoint, _) = fixture.advertised().await;
    let client = reqwest::Client::new();
    for cancelled in [false, true] {
        let (local, remote) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(remote);
        let installation = fixture.paths.clone();
        let transport = client.clone();
        let service = Cancellation::default();
        let signal = service.clone();
        let serving = tokio::spawn(async move {
            let startup_cancel = Cancellation::default();
            let startup = support::request(&startup_cancel);
            source_entry::stdio(
                &installation,
                &transport,
                (
                    support::key_limits(),
                    support::content(),
                    Duration::from_secs(30),
                ),
                None,
                &startup,
                &service,
                read,
                write,
            )
            .await
        });
        let peer = ().serve(local).await?;
        let tools = peer.peer().list_all_tools().await?;
        assert_eq!(
            tools
                .iter()
                .filter(|tool| tool.name == "graphit_ast_source")
                .count(),
            1
        );
        let result = peer
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_source")
                    .with_arguments(json!({"path":"raw"}).as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(result.is_error, Some(true));
        assert_eq!(
            result.content[0].as_text().unwrap().text,
            "real service payload"
        );
        if cancelled {
            signal.cancel();
            let error = serving.await?.unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Interrupted);
            let _ = peer.cancel().await;
        } else {
            peer.cancel().await?;
            serving.await??;
        }
        let cancel = Cancellation::default();
        let original = support::request(&cancel);
        let arguments = serde_json::from_value(json!({"path":"raw"}))?;
        let mut output = Vec::new();
        source_entry::call_source(
            &fixture.paths,
            &client,
            &arguments,
            support::key_limits(),
            &original,
            &mut output,
        )
        .await?;
        assert_eq!(output, b"real service payload");
    }
    assert!(endpoint.stop().await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_omitted_version_uses_retained_current_and_preserves_sibling_ambiguity() -> TestResult
{
    let fixture = support::Fixture::new();
    fixture
        .publish(0, &[("selected", Some("current main"))])
        .await;
    fixture
        .publish_named(1, &[("selected", Some("current feature"))])
        .await?;
    fixture
        .publish(2, &[("selected", Some("sibling main"))])
        .await;
    fixture
        .publish_named(3, &[("selected", Some("sibling feature"))])
        .await?;
    let root = fixture.projects[0].0.root().to_path_buf();
    let sibling = fixture.projects[2].0.root();
    let client = reqwest::Client::new();
    for (current, expected_current) in [(0, "current main"), (1, "current feature")] {
        let (endpoint, _) = fixture.advertised_current(current).await;
        let (local, remote) = tokio::io::duplex(65536);
        let (read, write) = tokio::io::split(remote);
        let installation = fixture.paths.clone();
        let transport = client.clone();
        let physical = root.clone();
        let address = endpoint.address;
        let serving = tokio::spawn(async move {
            let cancel = Cancellation::default();
            let startup = support::request(&cancel);
            let proxy = graphit_app::mcp_source::SourceProxy::new(
                installation,
                address,
                transport,
                (
                    support::key_limits(),
                    support::content(),
                    Duration::from_secs(30),
                ),
                Some((physical.as_path(), None)),
                &startup,
            )
            .await?;
            proxy.serve_stream(read, write).await
        });
        let peer = ().serve(local).await?;
        for (args, expected) in [
            (json!({"path":"selected"}), Ok(expected_current)),
            (
                json!({"path":"selected","project_dir":root}),
                Ok(expected_current),
            ),
            (
                json!({"path":"selected","project_version":"main"}),
                Ok("current main"),
            ),
            (
                json!({"path":"selected","project_version":"feature"}),
                Ok("current feature"),
            ),
            (
                json!({"path":"selected","project_dir":root,"project_version":"main"}),
                Ok("current main"),
            ),
            (
                json!({"path":"selected","project_dir":sibling}),
                Err("Source project/version selection ambiguous"),
            ),
            (
                json!({"path":"selected","project_dir":sibling,"project_version":"main"}),
                Ok("sibling main"),
            ),
            (
                json!({"path":"selected","project_dir":sibling,"project_version":"feature"}),
                Ok("sibling feature"),
            ),
            (
                json!({"path":"selected","project_version":"missing"}),
                Err("Source project/version unavailable"),
            ),
            (
                json!({"path":"selected","project_version":""}),
                Err("Source project selection invalid"),
            ),
        ] {
            let result = peer
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_ast_source")
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await?;
            assert_eq!(result.is_error == Some(true), expected.is_err());
            assert_eq!(
                result.content[0].as_text().unwrap().text,
                match expected {
                    Ok(text) | Err(text) => text,
                }
            );
        }
        peer.cancel().await?;
        serving.await??;
        assert!(endpoint.stop().await?);
    }
    Ok(())
}
