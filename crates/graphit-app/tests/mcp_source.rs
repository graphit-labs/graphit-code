#[path = "support/source_client.rs"]
mod support;
use graphit_app::mcp_source::SourceProxy;
use graphit_core::file_lock::Cancellation;
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ErrorCode},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::json;
use std::time::Duration;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("loopback fixture HTTP client")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn imported_context_through_proxy_never_injects_current_project_or_version() -> TestResult {
    let fixture = support::Fixture::in_directory(
        tempfile::Builder::new()
            .prefix(if cfg!(unix) {
                "source café Ω \"quote\\ "
            } else {
                "source café Ω "
            })
            .tempdir()?,
    );
    fixture.imported("vendorA/context", "A raw").await;
    fixture.imported("vendorB/context", "B raw").await;
    let endpoint = fixture.start().await;
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    let proxy = SourceProxy::new(
        fixture.paths.clone(),
        endpoint.address,
        http_client(),
        (
            support::key_limits(),
            support::content(),
            Duration::from_secs(30),
        ),
        Some((fixture.projects[0].0.root(), Some("feature"))),
        &original,
    )
    .await?;
    std::fs::remove_dir_all(fixture.projects[0].0.root())?;
    assert_eq!(
        graphit_app::source_entry::address(&fixture.paths, support::key_limits(), &original)
            .await?,
        endpoint.address
    );
    let headers =
        graphit_app::ast_source::runtime_headers(&fixture.paths, support::key_limits(), &original)
            .await?;
    let direct = ()
        .serve(StreamableHttpClientTransport::with_client(
            http_client(),
            StreamableHttpClientTransportConfig::with_uri(format!(
                "http://{}/mcp",
                endpoint.address
            ))
            .custom_headers(headers),
        ))
        .await?;
    let (local, remote) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote);
    let server = proxy.clone();
    let joined = tokio::spawn(async move { server.serve_stream(read, write).await });
    let client = ().serve(local).await?;
    for (id, text) in [("vendorA/context", "A raw"), ("vendorB/context", "B raw")] {
        let call = CallToolRequestParams::new("graphit_ast_source").with_arguments(
            json!({"path":"same","context":format!("{id}@release-1")})
                .as_object()
                .unwrap()
                .clone(),
        );
        let direct_result = direct.peer().call_tool(call.clone()).await?;
        assert_ne!(
            direct_result.is_error,
            Some(true),
            "direct Source: {direct_result:?}"
        );
        let result = client.peer().call_tool(call).await?;
        assert_ne!(result.is_error, Some(true), "stdio proxy: {result:?}");
        assert_eq!(result.content[0].as_text().unwrap().text, text);
    }
    direct.cancel().await?;
    client.cancel().await?;
    joined.await??;
    proxy.shutdown().await?;
    assert!(endpoint.stop().await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sdk_stdio_forwards_real_source_catalog_text_empty_and_schema() -> TestResult {
    let fixture = support::Fixture::new();
    fixture.file(0, "raw", "\u{feff}a\r\nβ\0\r\n");
    fixture.file(0, "empty", "");
    fixture.file(0, "indexed", "disk");
    fixture.publish(0, &[("indexed", Some(""))]).await;
    let endpoint = fixture.start().await;
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    let proxy = SourceProxy::new(
        fixture.paths.clone(),
        endpoint.address,
        http_client(),
        (
            support::key_limits(),
            support::content(),
            Duration::from_secs(30),
        ),
        None,
        &original,
    )
    .await?;
    let (local, remote) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote);
    let server = proxy.clone();
    let joined = tokio::spawn(async move { server.serve_stream(read, write).await });
    let client = ().serve(local).await?;
    let tools = client.peer().list_all_tools().await?;
    let headers =
        graphit_app::ast_source::runtime_headers(&fixture.paths, support::key_limits(), &original)
            .await?;
    let direct = ()
        .serve(StreamableHttpClientTransport::with_client(
            http_client(),
            StreamableHttpClientTransportConfig::with_uri(format!(
                "http://{}/mcp",
                endpoint.address
            ))
            .custom_headers(headers),
        ))
        .await?;
    let actual_catalog = direct.peer().list_all_tools().await?;
    assert_eq!(
        serde_json::to_value(&tools)?,
        serde_json::to_value(&actual_catalog)?
    );
    assert_eq!(tools[0].name, "graphit_ast_source");
    #[cfg(target_os = "linux")]
    {
        assert!(
            tools
                .iter()
                .any(|tool| tool.name == "graphit_memory_search")
        );
        let memory = || {
            CallToolRequestParams::new("graphit_memory_search").with_arguments(
                json!({"text":"catalog authority", "top_k":1})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
        };
        let expected = direct.peer().call_tool(memory()).await?;
        let forwarded = client.peer().call_tool(memory()).await?;
        // Read(Ast) never authorizes Memory, on either authenticated path.
        assert_eq!(expected.is_error, Some(true));
        assert_eq!(
            serde_json::to_value(&forwarded)?,
            serde_json::to_value(&expected)?
        );
    }
    let unknown = client
        .peer()
        .call_tool(CallToolRequestParams::new("unlisted_tool"))
        .await
        .unwrap_err();
    let rmcp::ServiceError::McpError(error) = unknown else {
        panic!("expected MCP error");
    };
    assert_eq!(error.code, ErrorCode::METHOD_NOT_FOUND);
    direct.cancel().await?;
    assert_eq!(
        tools[0].input_schema.get("additionalProperties"),
        Some(&json!(false))
    );
    for (args, expected) in [
        (json!({"path":"raw"}), "\u{feff}a\r\nβ\0\r\n"),
        (json!({"path":"empty"}), ""),
        (json!({"path":"indexed"}), ""),
        (json!({"path":"raw","pattern":"no-match"}), ""),
    ] {
        let result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_source")
                    .with_arguments(args.as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(result.is_error, Some(true));
        assert_eq!(
            result
                .content
                .iter()
                .map(|c| c.as_text().unwrap().text.as_str())
                .collect::<String>(),
            expected
        );
    }
    for args in [
        json!({"path":"raw","head":null}),
        json!({"path":"raw","regex":null}),
        json!({"path":"raw","head":1.5}),
        json!({"path":"raw","unknown":true}),
    ] {
        assert!(
            client
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_ast_source")
                        .with_arguments(args.as_object().unwrap().clone())
                )
                .await
                .is_err()
        );
    }
    client.cancel().await?;
    joined.await??;
    proxy.shutdown().await?;
    assert!(endpoint.stop().await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_credentials_and_proxy_shutdown_are_independent() -> TestResult {
    let fixture = support::Fixture::new();
    fixture.file(0, "file", "real independent payload");
    let endpoint = fixture.start().await;
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    let client = http_client();
    let first = SourceProxy::new(
        fixture.paths.clone(),
        endpoint.address,
        client.clone(),
        (
            support::key_limits(),
            support::content(),
            Duration::from_secs(30),
        ),
        None,
        &original,
    )
    .await?;
    let second = SourceProxy::new(
        fixture.paths.clone(),
        endpoint.address,
        client,
        (
            support::key_limits(),
            support::content(),
            Duration::from_secs(30),
        ),
        None,
        &original,
    )
    .await?;
    let (local_a, remote_a) = tokio::io::duplex(65536);
    let (read_a, write_a) = tokio::io::split(remote_a);
    let owner_a = first.clone();
    let join_a = tokio::spawn(async move { owner_a.serve_stream(read_a, write_a).await });
    let sdk_a = ().serve(local_a).await?;
    let (local_b, remote_b) = tokio::io::duplex(65536);
    let (read_b, write_b) = tokio::io::split(remote_b);
    let owner_b = second.clone();
    let join_b = tokio::spawn(async move { owner_b.serve_stream(read_b, write_b).await });
    let sdk_b = ().serve(local_b).await?;
    let call = || {
        CallToolRequestParams::new("graphit_ast_source")
            .with_arguments(json!({"path":"file"}).as_object().unwrap().clone())
    };
    let (a, b) = tokio::join!(
        sdk_a.peer().call_tool(call()),
        sdk_b.peer().call_tool(call())
    );
    let a = a?;
    let b = b?;
    assert_ne!(a.is_error, Some(true), "{a:?}");
    assert_ne!(b.is_error, Some(true), "{b:?}");
    let pending = sdk_a
        .peer()
        .send_cancellable_request(
            rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(call())),
            Default::default(),
        )
        .await?;
    pending.cancel(Some("caller cancelled".into())).await?;
    let pending = sdk_a
        .peer()
        .send_cancellable_request(
            rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(call())),
            Default::default(),
        )
        .await?;
    sdk_a.cancel().await?;
    drop(pending);
    join_a.await??;
    first.shutdown().await?;
    let result = sdk_b.peer().call_tool(call()).await?;
    assert_eq!(
        result.content[0].as_text().unwrap().text,
        "real independent payload"
    );
    let key = fixture.paths.global_path(&["daemon", "mcp.key"])?;
    let before = std::fs::read(&key)?;
    std::fs::write(&key, vec![b'0'; 64])?;
    let result = sdk_b.peer().call_tool(call()).await?;
    assert_eq!(result.is_error, Some(true));
    assert!(!result.content.iter().any(|c| {
        c.as_text()
            .is_some_and(|text| text.text.contains("real independent payload"))
    }));
    std::fs::write(&key, &before)?;
    // Restoring bytes after direct tampering does not restore the owner's
    // captured physical observation; only its real lifecycle can rotate keys.
    assert_eq!(sdk_b.peer().call_tool(call()).await?.is_error, Some(true));
    sdk_b.cancel().await?;
    join_b.await??;
    second.shutdown().await?;
    let cleanup = endpoint.stop().await.unwrap_err();
    assert!(
        matches!(
            cleanup,
            graphit_daemon::source_discovery::DiscoveryError::Runtime(
                graphit_core::runtime_key::RuntimeKeyError::Conflict
            )
        ),
        "{cleanup:?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_stdio_proxy_discovers_replacement_endpoint_for_each_new_request() -> TestResult {
    let fixture = support::Fixture::new();
    fixture.file(0, "file", "first endpoint payload");
    let first = fixture.start().await;
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    let proxy = SourceProxy::new(
        fixture.paths.clone(),
        first.address,
        http_client(),
        (
            support::key_limits(),
            support::content(),
            Duration::from_secs(30),
        ),
        Some((
            fixture.projects[0].0.root(),
            Some(fixture.projects[0].2.version()),
        )),
        &original,
    )
    .await?;
    let (local, remote) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote);
    let server = proxy.clone();
    let joined = tokio::spawn(async move { server.serve_stream(read, write).await });
    let client = ().serve(local).await?;
    let source = || {
        CallToolRequestParams::new("graphit_ast_source")
            .with_arguments(json!({"path":"file"}).as_object().unwrap().clone())
    };
    let first_result = client.peer().call_tool(source()).await?;
    assert_ne!(first_result.is_error, Some(true));
    assert_eq!(
        first_result.content[0].as_text().unwrap().text,
        "first endpoint payload"
    );
    #[cfg(target_os = "linux")]
    let memory = || {
        CallToolRequestParams::new("graphit_memory_search").with_arguments(
            json!({"text":"endpoint authority","top_k":1})
                .as_object()
                .unwrap()
                .clone(),
        )
    };
    #[cfg(target_os = "linux")]
    let expected_memory = {
        let result = client.peer().call_tool(memory()).await?;
        assert_eq!(result.is_error, Some(true));
        assert!(!result.content.iter().any(|content| {
            content
                .as_text()
                .is_some_and(|text| text.text.contains("Source forwarding failed ("))
        }));
        serde_json::to_value(result)?
    };
    let first_address = first.address;
    assert!(first.stop().await?);
    assert_eq!(
        client.peer().call_tool(source()).await?.is_error,
        Some(true)
    );
    #[cfg(target_os = "linux")]
    {
        let missing = client.peer().call_tool(memory()).await?;
        assert_eq!(missing.is_error, Some(true));
        assert!(missing.content.iter().any(|content| {
            content
                .as_text()
                .is_some_and(|text| text.text.contains("stage=discovery, category=not_found"))
        }));
        assert_ne!(serde_json::to_value(missing)?, expected_memory);
    }
    assert!(!fixture.paths.global_path(&["daemon", "mcp.json"])?.exists());
    // Keep the old port reserved, proving that the next request cannot reuse it.
    let old_port = std::net::TcpListener::bind(first_address)?;
    fixture.file(0, "file", "replacement endpoint payload");
    let second = fixture.start().await;
    assert_ne!(second.address, first_address);
    let replacement = client.peer().call_tool(source()).await?;
    assert_ne!(replacement.is_error, Some(true));
    assert_eq!(
        replacement.content[0].as_text().unwrap().text,
        "replacement endpoint payload"
    );
    #[cfg(target_os = "linux")]
    {
        // The Source's real Read(Ast) authority still denies Memory after replacement.
        assert_eq!(
            serde_json::to_value(client.peer().call_tool(memory()).await?)?,
            expected_memory
        );
    }
    client.cancel().await?;
    joined.await??;
    proxy.shutdown().await?;
    assert!(second.stop().await?);
    drop(old_port);
    Ok(())
}

#[tokio::test]
async fn role_context_denies_root_before_credential_access() -> TestResult {
    const PROBE: &str = "GRAPHIT_ROOT_MCP_DENIAL_PROBE";
    let keys = [
        "GRAPHIT_ROLE_BEARER",
        "GRAPHIT_ROLE_REFERENCE",
        "GRAPHIT_ROLE_SELECTION",
    ];
    if std::env::var_os(PROBE).is_some() {
        let temp = tempfile::tempdir()?;
        let paths = graphit_core::project_paths::InstallationPaths::resolve(
            graphit_core::brand::Brand::default(),
            temp.path(),
            None,
            Some(&temp.path().join("absent-installation")),
        )?;
        let cancel = Cancellation::default();
        let original = support::request(&cancel);
        let result = SourceProxy::new(
            paths.clone(),
            "127.0.0.1:9".parse()?,
            http_client(),
            (
                support::key_limits(),
                support::content(),
                Duration::from_secs(1),
            ),
            None,
            &original,
        )
        .await;
        let error = match result {
            Ok(_) => panic!("Role entered Root proxy"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            error.to_string(),
            "Root MCP is unavailable in a managed-role transport context"
        );
        assert!(!paths.root().exists());
        return Ok(());
    }
    let cases = keys.iter().map(|key| (*key, std::ffi::OsString::new()));
    #[cfg(unix)]
    let cases = {
        use std::os::unix::ffi::OsStringExt;
        cases.chain(std::iter::once((
            keys[0],
            std::ffi::OsString::from_vec(vec![0xff]),
        )))
    };
    for (key, value) in cases {
        let stderr = tempfile::NamedTempFile::new()?;
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "role_context_denies_root_before_credential_access",
                "--nocapture",
            ])
            .env(PROBE, "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::from(stderr.reopen()?))
            .kill_on_drop(true);
        for input in keys {
            command.env_remove(input);
        }
        let mut child = command.env(key, value).spawn()?;
        let status = match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
            Ok(status) => status?,
            Err(error) => {
                child.kill().await?;
                child.wait().await?;
                return Err(error.into());
            }
        };
        assert!(
            status.success(),
            "isolated denial probe failed: {}",
            std::fs::read_to_string(stderr.path())?
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn root_proxy_project_defaults_preserve_explicit_other_project_and_version() -> TestResult {
    let fixture = support::Fixture::in_directory(
        tempfile::Builder::new()
            .prefix(if cfg!(unix) {
                "source café Ω \"quote\\ "
            } else {
                "source café Ω "
            })
            .tempdir()?,
    );
    fixture.file(0, "selected", "physical first");
    fixture.file(2, "selected", "physical second");
    fixture.publish(0, &[("selected", Some("FIRST"))]).await;
    fixture.publish(2, &[("selected", Some("SECOND"))]).await;
    let endpoint = fixture.start().await;
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    let first = fixture.projects[0].0.root();
    let second = fixture.projects[2].0.root();
    let proxy = SourceProxy::new(
        fixture.paths.clone(),
        endpoint.address,
        http_client(),
        (
            support::key_limits(),
            support::content(),
            Duration::from_secs(30),
        ),
        Some((first, Some("feature"))),
        &original,
    )
    .await?;
    let (local, remote) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote);
    let server = proxy.clone();
    let joined = tokio::spawn(async move { server.serve_stream(read, write).await });
    let client = ().serve(local).await?;
    for (arguments, expected) in [
        (json!({"path":"selected"}), "FIRST"),
        (json!({"path":"selected","project_dir":first}), "FIRST"),
        (json!({"path":"selected","project_version":"main"}), "FIRST"),
        (
            json!({"path":"selected","project_dir":second,"project_version":"feature"}),
            "SECOND",
        ),
    ] {
        let result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_source")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(result.is_error, Some(true));
        assert_eq!(result.content[0].as_text().unwrap().text, expected);
    }

    #[cfg(target_os = "linux")]
    {
        let second_path = serde_json::to_value(graphit_core::native_path::NativePath::new(
            second.to_path_buf(),
        )?)?;
        // Valid semantic selections reach the missing hook (503). A different
        // explicit root without its required version remains an invalid request (400).
        for arguments in [
            json!({"ai_optimized":false,"query":"selection"}),
            json!({"ai_optimized":false,"query":"selection","project_dir":second_path,"project_version":"main"}),
        ] {
            let result = client
                .peer()
                .call_tool(
                    CallToolRequestParams::new("graphit_ast_search")
                        .with_arguments(arguments.as_object().unwrap().clone()),
                )
                .await?;
            assert_eq!(result.is_error, Some(true));
            assert_eq!(
                result.structured_content,
                Some(json!({"status":"error","http_status":503,"message":"not connected"}))
            );
        }
        let result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_search").with_arguments(
                    json!({"ai_optimized":false,"query":"selection","project_dir":second_path})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await?;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content,
            Some(json!({
                "status":"error","http_status":400,"message":"invalid graph request"
            }))
        );
    }
    for arguments in [
        json!({"path":"selected","project_dir":17}),
        json!({"path":"selected","project_version":17}),
    ] {
        let result = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("graphit_ast_source")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await;
        assert!(result.is_err());
    }
    client.cancel().await?;
    joined.await??;
    proxy.shutdown().await?;
    assert!(endpoint.stop().await?);
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_proxy_preserves_protocol_error_code_and_domain_error_result() -> TestResult {
    let fixture = support::Fixture::new();
    let endpoint = fixture.start().await;
    let cancel = Cancellation::default();
    let original = support::request(&cancel);
    let proxy = SourceProxy::new(
        fixture.paths.clone(),
        endpoint.address,
        http_client(),
        (
            support::key_limits(),
            support::content(),
            Duration::from_secs(30),
        ),
        None,
        &original,
    )
    .await?;
    let headers =
        graphit_app::ast_source::runtime_headers(&fixture.paths, support::key_limits(), &original)
            .await?;
    let direct = ()
        .serve(StreamableHttpClientTransport::with_client(
            http_client(),
            StreamableHttpClientTransportConfig::with_uri(format!(
                "http://{}/mcp",
                endpoint.address
            ))
            .custom_headers(headers),
        ))
        .await?;
    let (local, remote) = tokio::io::duplex(65536);
    let (read, write) = tokio::io::split(remote);
    let server = proxy.clone();
    let joined = tokio::spawn(async move { server.serve_stream(read, write).await });
    let client = ().serve(local).await?;
    assert!(
        client
            .peer()
            .list_all_tools()
            .await?
            .iter()
            .any(|tool| tool.name == "graphit_memory_search")
    );
    let invalid = CallToolRequestParams::new("graphit_memory_search")
        .with_arguments(json!({"ai_optimized":null}).as_object().unwrap().clone());
    let error = client
        .peer()
        .call_tool(invalid)
        .await
        .expect_err("invalid flag is a JSON-RPC error");
    let rmcp::ServiceError::McpError(error) = error else {
        panic!("expected forwarded MCP protocol error")
    };
    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert!(error.data.is_none());
    assert!(error.message.contains("category=rpc_error"));
    let domain = CallToolRequestParams::new("graphit_memory_search").with_arguments(
        json!({"text":"catalog authority","top_k":1,"ai_optimized":false})
            .as_object()
            .unwrap()
            .clone(),
    );
    let expected = direct.peer().call_tool(domain.clone()).await?;
    assert_eq!(expected.is_error, Some(true));
    let actual = client.peer().call_tool(domain).await?;
    assert_eq!(
        serde_json::to_value(actual)?,
        serde_json::to_value(expected)?
    );
    direct.cancel().await?;
    client.cancel().await?;
    joined.await??;
    proxy.shutdown().await?;
    assert!(endpoint.stop().await?);
    Ok(())
}
