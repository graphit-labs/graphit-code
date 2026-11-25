use super::*;

#[tokio::test]
async fn projected_query_schema_mcp_http_and_authority_match_native_protocol() {
    let (_temp, server, raw) = fixture();
    let socket = listener();
    let base = format!("http://{}", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let client = connect(format!("{base}/mcp"), Some(&raw)).await;
    let tools = client.list_all_tools().await.unwrap();
    let tool = tools
        .iter()
        .find(|tool| tool.name == "graphit_task_query")
        .unwrap();
    let schema = Value::Object(tool.input_schema.as_ref().clone());
    assert_eq!(schema["required"], json!(["input"]));
    assert_eq!(schema["additionalProperties"], false);
    let input_schema = &schema["properties"]["input"];
    assert_eq!(input_schema["additionalProperties"], false);
    for field in ["selection", "columns", "page_size", "top_k"] {
        assert!(
            input_schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!(field))
        );
    }
    assert!(!schema.to_string().contains("\"$ref\""));
    assert!(
        !input_schema["properties"]
            .as_object()
            .unwrap()
            .contains_key("actor")
    );
    let native_schema = call(&client, "graphit_task_schema", json!({}), false).await;
    assert_eq!(native_schema["operation"], "schema");
    for table in ["tasks", "sessions"] {
        let columns = native_schema["result"]["tables"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["table"] == table)
            .unwrap()["columns"]
            .as_array()
            .unwrap();
        assert!(columns.iter().any(|column| column["name"] == "id"));
        assert!(columns.iter().any(|column| column["name"] == "status"));
        assert!(
            !columns
                .iter()
                .any(|column| column["name"] == "actor" || column["name"] == "token")
        );
    }
    let absent = call(
        &client,
        "graphit_task_query",
        query_arguments("tasks"),
        false,
    )
    .await;
    assert_eq!(absent["result"]["state"], "no_corpus");
    let session = call(
        &client,
        "graphit_task_session_create",
        json!({"input":{
            "idempotency_key":"query-session", "title":"Query session",
            "description":"Projected native reads", "strategy":"MCP and HTTP"
        }}),
        false,
    )
    .await;
    let session_id = &session["result"]["id"];
    let mut ids = Vec::new();
    for key in ["query-a", "query-b"] {
        let task = call(&client, "graphit_task_create", json!({"input":{
            "idempotency_key":key,"session_id":session_id,"title":"Private full specification",
            "description":"Query must not return this specification","task_type":"TASK","priority":1,
            "checks":[{"kind":"acceptance","text":"Project native rows"},
                      {"kind":"test","text":"Read finite MCP and HTTP pages"}]
        }}), false).await;
        ids.push(task["result"]["specification"]["id"].clone());
    }
    let mut query = json!({"selection":{"table":"tasks","ids":ids},
        "columns":["id","status"],"page_size":1,"top_k":2});
    let first = call(&client, "graphit_task_query", json!({"input":query}), false).await;
    assert_eq!(first["operation"], "query");
    let page = &first["result"];
    assert_eq!(page["state"], "published");
    assert_eq!(page["columns"], json!(["id", "status"]));
    let rows = page["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].as_array().unwrap().len(), 2);
    assert!(ids.contains(&rows[0][0]));
    assert_eq!(
        rows[0][1],
        serde_json::to_value(graphit_records::gates::TaskStatus::Open).unwrap()
    );
    assert!(page["next_cursor"].is_string());
    assert!(!page.to_string().contains("Private full specification"));
    assert!(!page.to_string().contains(&raw));
    query["cursor"] = page["next_cursor"].clone();
    let http = reqwest::Client::new();
    let second: Value = http
        .post(format!("{base}/api/tasks/native/query"))
        .bearer_auth(&raw)
        .header("x-graphit-auth-origin", "runtime")
        .json(&json!({"input":query}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(second["operation"], "query");
    assert_eq!(second["result"]["generation"], page["generation"]);
    assert_eq!(second["result"]["columns"], page["columns"]);
    let next_rows = second["result"]["rows"].as_array().unwrap();
    assert_eq!(next_rows.len(), 1);
    assert_ne!(next_rows[0][0], rows[0][0]);
    assert!(ids.contains(&next_rows[0][0]));
    assert!(second["result"]["next_cursor"].is_null());
    let exact = call(&client, "graphit_task_query", json!({"input":{
        "selection":{"table":"tasks","ids":[ids[1]]},"columns":["status"],"page_size":1,"top_k":1
    }}), false).await;
    assert_eq!(exact["result"]["columns"], json!(["status"]));
    assert_eq!(
        exact["result"]["rows"],
        json!([[serde_json::to_value(graphit_records::gates::TaskStatus::Open).unwrap()]])
    );
    let empty = call(&client, "graphit_task_query", json!({"input":{
        "selection":{"table":"tasks","ids":["00000000000000000000000004"]},"columns":["id"],"page_size":1,"top_k":1
    }}), false).await;
    assert_eq!(empty["result"]["state"], "published");
    assert_eq!(empty["result"]["rows"], json!([]));
    let sessions = call(&client, "graphit_task_query", json!({"input":{
        "selection":{"table":"sessions","ids":[session_id]},"columns":["id"],"page_size":1,"top_k":1
    }}), false).await;
    assert_eq!(sessions["result"]["rows"], json!([[session_id]]));
    for mut invalid in [json!({}), query_arguments("tasks")] {
        if invalid.get("input").is_some() {
            invalid["input"]["actor"] = json!("forged");
        }
        let denied = call(&client, "graphit_task_query", invalid, true).await;
        assert_eq!(denied["code"], "invalid_arguments");
    }
    let denied = call(
        &client,
        "graphit_task_query",
        json!({"selection":{"actor":"forged"},"input":{
            "selection":{"table":"tasks"},"columns":["id"],"page_size":1,"top_k":1
        }}),
        true,
    )
    .await;
    assert_eq!(denied["code"], "invalid_arguments");
    let missing = connect(format!("{base}/mcp"), None).await;
    let denied = call(
        &missing,
        "graphit_task_query",
        query_arguments("tasks"),
        true,
    )
    .await;
    assert_eq!(denied["code"], "credential_required");
    let wrong = connect(format!("{base}/mcp"), Some("incorrect-runtime-key")).await;
    let denied = call(&wrong, "graphit_task_query", query_arguments("tasks"), true).await;
    assert_eq!(denied["code"], "authentication_or_permission_denied");
    wrong.cancel().await.unwrap();
    missing.cancel().await.unwrap();
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}
