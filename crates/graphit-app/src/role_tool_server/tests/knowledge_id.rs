use super::*;

const TOOL: &str = "graphit_knowledge_id";

#[tokio::test]
async fn knowledge_id_role_catalogue_real_calls_and_formatter_need_no_corpus() {
    let (_temp, server, raw) = fixture();
    let (_session, _run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Knowledge)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let tools = client.list_all_tools().await.unwrap();
    let selected: Vec<_> = tools.iter().filter(|tool| tool.name == TOOL).collect();
    assert_eq!(selected.len(), 1);
    let tool = selected[0];
    assert!(tool.output_schema.is_none());
    assert_eq!(
        tool.annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );
    assert_eq!(tool.input_schema["required"], json!(["input"]));
    assert_eq!(
        tool.input_schema["properties"]["input"]["required"],
        json!(["path"])
    );
    assert_eq!(
        tool.input_schema,
        server.get_tool(TOOL).unwrap().input_schema
    );
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_list",
            json!({"input":{}}),
            false
        )
        .await["kind"],
        "no_corpus"
    );
    let path = std::path::Path::new("docs").join("ação com espaços 日本語.md");
    let expected =
        graphit_data::knowledge_document_lifecycle::KnowledgeSourceIdentity::from_relative_path(
            &path,
            None,
            n(server.0.task.content.max_scalar_bytes),
        )
        .unwrap();
    let arguments = json!({"input":{"path":path}});
    let value = call(&client, TOOL, arguments.clone(), false).await;
    assert_eq!(value, json!({"id":expected.slug()}));
    let id = value["id"].as_str().unwrap();
    assert_eq!(id.len(), 26);
    assert_eq!(
        id.parse::<graphit_core::project_identity::ProjectId>()
            .unwrap()
            .to_string(),
        id
    );
    assert_eq!(call(&client, TOOL, arguments.clone(), false).await, value);
    for options in [
        arguments,
        json!({"input":{"path":path},"ai_optimized":true}),
    ] {
        let result = client
            .call_tool(
                CallToolRequestParams::new(TOOL)
                    .with_arguments(options.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        assert!(result.structured_content.is_none());
        assert_eq!(
            result.content[0].as_text().unwrap().text,
            format!("id: {id}")
        );
    }
    for input in [
        json!({"path":"../escape.md"}),
        json!({"path":"/absolute.md"}),
        json!({"path":"bad\u{0000}.md"}),
        json!({"path":"doc.md","authored_id":"extra"}),
    ] {
        let result = call(&client, TOOL, json!({"input":input}), true).await;
        assert!(result.get("id").is_none());
    }
    assert_eq!(
        call(
            &client,
            "graphit_knowledge_list",
            json!({"input":{}}),
            false
        )
        .await["kind"],
        "no_corpus"
    );
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn knowledge_id_role_preserves_permission_scope_and_closed_run_checks() {
    let (_temp, server, raw) = fixture();
    let (session, run, reference, bearer) = credential(
        &server,
        &raw,
        Role::Scout,
        &[Permission::Read(Module::Knowledge)],
        Duration::from_secs(90),
    );
    let (_other_session, _other_run, other_reference, other_bearer) = credential(
        &server,
        &raw,
        Role::Tracker,
        &[Permission::Read(Module::Task)],
        Duration::from_secs(90),
    );
    let (uri, serving) = serve(&server).await;
    let denied = connect(
        &uri,
        role_credential(&other_reference, &other_bearer, server.0.task.content),
    )
    .await;
    assert!(
        !denied
            .list_all_tools()
            .await
            .unwrap()
            .iter()
            .any(|tool| tool.name == TOOL)
    );
    assert_eq!(
        call(&denied, TOOL, json!({"input":{"path":"doc.md"}}), true).await["code"],
        "authentication_or_permission_denied"
    );
    denied.cancel().await.unwrap();
    let client = connect(
        &uri,
        role_credential(&reference, &bearer, server.0.task.content),
    )
    .await;
    let foreign = graphit_core::native_path::NativePath::new(
        project_snapshot(&server).projects[1].0.root().to_owned(),
    )
    .unwrap();
    let result = call(
        &client,
        TOOL,
        json!({"selection":{"project_dir":foreign},"input":{"path":"doc.md"}}),
        true,
    )
    .await;
    assert!(result.get("id").is_none());
    let cancel = Cancellation::default();
    let original = request(&cancel, server.0.task.content);
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(&raw, &original).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    issuer.close_run(&parent, &run).unwrap();
    assert_eq!(
        call(&client, TOOL, json!({"input":{"path":"doc.md"}}), true).await["code"],
        "authentication_or_permission_denied"
    );
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    serving.await.unwrap().unwrap();
}
