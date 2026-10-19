use super::*;

#[tokio::test]
async fn actual_mcp_references_follow_catalogue_preserve_clear_and_reject_without_write() {
    let (_temp, server, raw) = fixture();
    let socket = listener();
    let uri = format!("http://{}/mcp", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let client = connect(uri, Some(&raw)).await;
    let listed = client.list_all_tools().await.unwrap();
    let supported = [
        "create_task",
        "create_session",
        "add_task_comment",
        "progress_task",
        "checkpoint_session",
        "release_task",
        "release_session",
        "revise_task",
        "revise_session",
        "verify_task_check",
        "supersede_task_check",
        "update_task_links",
        "set_task_flag",
        "complete_task",
        "cancel_task",
        "complete_session",
    ];
    let mut announced = 0;
    for (name, operation, _, _) in super::super::catalog::OPERATIONS {
        let tool = listed.iter().find(|tool| tool.name == *name).unwrap();
        let schema = &tool.input_schema["properties"]["input"];
        let references = schema["properties"].get("references");
        assert_eq!(
            references.is_some(),
            supported.contains(operation),
            "{operation}"
        );
        if let Some(references) = references {
            assert_eq!(references["type"], "array");
            assert_eq!(references["items"]["additionalProperties"], false);
            assert_eq!(
                references["items"]["properties"]["target"]["additionalProperties"],
                false
            );
            assert!(
                !schema["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("references"))
            );
            announced += 1;
        }
    }
    assert_eq!(announced, supported.len());
    let session = call(
        &client,
        "graphit_task_session_create",
        json!({"input": {
            "idempotency_key":"references-session", "title":"Reference target",
            "description":"Synthetic project contract test", "strategy":"Use actual SDK relations"
        }}),
        false,
    )
    .await;
    let session_id = session["result"]["id"].clone();
    let references = json!([{"target":{"type":"session","id":session_id},"relation":"supports"}]);
    let created = call(&client, "graphit_task_create", json!({"input": {
        "idempotency_key":"references-task", "session_id":session_id, "title":"Native relation source",
        "description":"Synthetic contract data", "task_type":"task", "priority":1,
        "references":references,
        "checks":[{"kind":"acceptance","text":"Persist the actual relation"},
                  {"kind":"test","text":"Read the native relation after MCP mutations"}]
    }}), false).await;
    let task = &created["result"];
    let id = task["specification"]["id"].clone();
    assert_eq!(task["references"].as_array().unwrap().len(), 1);
    let stored = task["references"].clone();
    assert!(stored.to_string().contains(session_id.as_str().unwrap()));
    assert!(stored.to_string().contains("supports"));
    let denied = call(
        &client,
        "graphit_task_claim",
        json!({"input": {
            "id":id, "lease_ms":600000, "references":[]
        }}),
        true,
    )
    .await;
    assert_eq!(denied["code"], "invalid_arguments");
    let claimed = call(
        &client,
        "graphit_task_claim",
        json!({"input": {
            "id":id, "lease_ms":600000
        }}),
        false,
    )
    .await;
    let token = claimed["result"]["token"].as_str().unwrap().to_owned();
    let mut current = claimed["result"]["record"].clone();
    assert_eq!(current["references"], stored);
    let target = |record: &Value| {
        json!({"id":id,"claim": {
            "token":token,"expected_revision":record["specification"]["revision"]
        }})
    };
    let denied = call(
        &client,
        "graphit_task_heartbeat",
        json!({"input": {
            "target":target(&current), "lease_ms":600000, "references":[]
        }}),
        true,
    )
    .await;
    assert_eq!(denied["code"], "invalid_arguments");
    let progressed = call(&client, "graphit_task_progress", json!({"input": {
        "target":target(&current), "lease_ms":600000,
        "summary":"Omitted patch preserves the relation", "next_step":"Verify rejection and clearing"
    }}), false).await;
    current = progressed["result"].clone();
    assert_eq!(current["references"], stored);
    let before = call(
        &client,
        "graphit_task_get",
        json!({"input":{"id":id}}),
        false,
    )
    .await;
    assert_eq!(before["result"]["task"], current);
    for invalid in [
        Value::Null,
        json!({}),
        json!([{}]),
        json!([{"target":{"type":"session","id":session_id,"unknown":"x"},"relation":"supports"}]),
        json!([{"target":{"type":"session","id":session_id,"scope":null},"relation":"supports"}]),
        json!([{"target":{"type":"session","id":session_id},"relation":"supports","field":false}]),
        json!([{"target":{"type":"grant","id":session_id},"relation":"supports"}]),
    ] {
        let rejected = call(&client, "graphit_task_revise", json!({"input": {
            "target":target(&current), "lease_ms":600000, "reason":"Invalid reference must not publish",
            "references":invalid
        }}), true).await;
        assert_eq!(rejected["code"], "reference_projection_failure");
        assert!(rejected.get("committed").is_none());
        assert_eq!(
            call(
                &client,
                "graphit_task_get",
                json!({"input":{"id":id}}),
                false
            )
            .await,
            before
        );
    }
    let cleared = call(
        &client,
        "graphit_task_revise",
        json!({"input": {
            "target":target(&current), "lease_ms":600000,
            "reason":"Clear only authored relations", "references":[]
        }}),
        false,
    )
    .await;
    current = cleared["result"].clone();
    assert_eq!(current["references"], json!([]));
    let detail = call(
        &client,
        "graphit_task_get",
        json!({"input":{"id":id}}),
        false,
    )
    .await;
    assert_eq!(detail["result"]["task"], current);
    assert_eq!(detail["result"]["creation"]["record"]["references"], stored);
    assert!(!detail.to_string().contains(&token));
    client.cancel().await.unwrap();
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
    assert!(server.0.jobs.lock().await.is_empty());
}
