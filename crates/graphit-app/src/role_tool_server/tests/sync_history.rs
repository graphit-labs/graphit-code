use super::*;

#[derive(clap::Parser)]
struct LogCli {
    #[command(flatten)]
    input: crate::knowledge_log_entry::Arguments,
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_knowledge_log_and_root_cli_read_actual_commit_history_without_models() {
    use clap::Parser;
    let (_temp, roles, raw) = fixture();
    references::knowledge_with_references(&roles, &raw).await;
    let snapshot = project_snapshot(&roles);
    let root = crate::source_server::SourceServer::new(
        Arc::clone(&roles.0.auth),
        Arc::clone(&snapshot.projects),
        0,
        roles.0.policy.source.clone(),
        CancellationToken::new(),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let original = request(&cancel, roles.0.task.content);
    let listener = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &cancel,
    )
    .unwrap();
    let address = listener.address();
    graphit_daemon::source_discovery::publish_discovery(
        &roles.0.auth,
        &listener,
        roles.0.policy.source.index,
        original.snapshot(),
    )
    .unwrap();
    let owner = root.clone();
    let root_serving = tokio::spawn(async move { owner.serve(listener).await });
    let (_session, _run, reference, bearer) = credential(
        &roles,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Knowledge)],
        Duration::from_secs(90),
    );
    let (_no_session, _no_run, no_ref, no_bearer) = credential(
        &roles,
        &raw,
        Role::Tracker,
        &[Permission::Read(Module::Task)],
        Duration::from_secs(90),
    );
    let (uri, role_serving) = serve(&roles).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, roles.0.task.content),
    )
    .await;
    let denied = connect(
        &uri,
        role_credential(&no_ref, &no_bearer, roles.0.task.content),
    )
    .await;
    assert!(
        client
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name == "graphit_wiki_log")
    );
    assert!(
        !denied
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name == "graphit_wiki_log")
    );
    let role = call(
        &client,
        "graphit_wiki_log",
        json!({"input":{"limit":1}}),
        false,
    )
    .await;
    assert_eq!(role["kind"], "published");
    assert_eq!(role["entries"].as_array().unwrap().len(), 1);
    let changes = &role["entries"][0]["changes"];
    assert!(
        changes["added"]
            .as_array()
            .unwrap()
            .iter()
            .any(|slug| slug == "alpha")
    );
    assert!(
        role["entries"][0]["timestamp"]
            .as_str()
            .unwrap()
            .ends_with('Z')
            || role["entries"][0]["timestamp"]
                .as_str()
                .unwrap()
                .ends_with("+00:00")
    );
    assert!(role["generation"].get("corpus_uri").is_none());
    call(
        &denied,
        "graphit_wiki_log",
        json!({"input":{"limit":1}}),
        true,
    )
    .await;
    for input in [
        json!({"limit":0}),
        json!({"limit":1,"cursor":null}),
        json!({"limit":1,"context":"foreign"}),
        json!({"limit":1,"cursor":"forged"}),
    ] {
        call(&client, "graphit_wiki_log", json!({"input":input}), true).await;
    }
    let args = LogCli::try_parse_from([
        "history",
        "--limit",
        "1",
        "--project-dir",
        snapshot.projects[0].0.root().to_str().unwrap(),
    ])
    .unwrap();
    assert!(LogCli::try_parse_from(["history", "--limit", "0"]).is_err());
    let mut prepared = crate::knowledge_log_entry::prepare(
        &args.input,
        Some(snapshot.projects[0].2.version()),
        &original,
    )
    .unwrap();
    crate::mcp_output::json_call(&mut prepared);
    assert_eq!(prepared.name, "graphit_wiki_log");
    let http = super::loopback_client();
    let connection = crate::ast_source::connect_source(
        &roles.0.policy.source.installation,
        address,
        &http,
        roles.0.policy.source.index,
        &original,
        (),
    )
    .await
    .unwrap();
    let result = connection.peer().call_tool(prepared.clone()).await.unwrap();
    assert_ne!(result.is_error, Some(true));
    let root_result = result.structured_content.unwrap();
    assert_eq!(root_result, role);
    connection.cancel().await.unwrap();
    let mut output = Vec::new();
    crate::knowledge_search_entry::call(
        &roles.0.policy.source.installation,
        prepared,
        roles.0.policy.source.index,
        &original,
        &mut output,
    )
    .await
    .unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), role);
    let peer_args = LogCli::try_parse_from([
        "history",
        "--limit",
        "1",
        "--project-dir",
        snapshot.projects[1].0.root().to_str().unwrap(),
    ])
    .unwrap();
    let peer_prepared = crate::knowledge_log_entry::prepare(
        &peer_args.input,
        Some(snapshot.projects[1].2.version()),
        &original,
    )
    .unwrap();
    let mut empty = Vec::new();
    crate::knowledge_search_entry::call(
        &roles.0.policy.source.installation,
        peer_prepared,
        roles.0.policy.source.index,
        &original,
        &mut empty,
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&empty).unwrap()["kind"],
        "no_corpus"
    );
    client.cancel().await.unwrap();
    denied.cancel().await.unwrap();
    roles.shutdown().await.unwrap();
    role_serving.await.unwrap().unwrap();
    root.shutdown().await.unwrap();
    root_serving.await.unwrap().unwrap();
}
