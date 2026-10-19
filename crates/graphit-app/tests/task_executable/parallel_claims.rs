use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_root_stdio_clients_keep_distinct_target_claims_for_the_same_actor() -> Result {
    let f = Fixture::new().await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let mut clients = Vec::new();
    for _ in 0..2 {
        let mut command = tokio::process::Command::from(f.command());
        command
            .args(["mcp", "--stdio", "--non-interactive"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let sdk = ().serve((child.stdout.take().unwrap(), child.stdin.take().unwrap())).await?;
        clients.push((child, sdk));
    }
    macro_rules! request {
        ($client:expr, $name:expr, $input:expr) => {
            $client.peer().call_tool(
                CallToolRequestParams::new($name).with_arguments(json_arguments(
                    json!({"input": $input}).as_object().unwrap().clone(),
                )),
            )
        };
    }
    macro_rules! call {
        ($client:expr, $name:expr, $input:expr) => {
            wire(request!($client, $name, $input).await?)?
        };
    }
    let session = call!(
        clients[0].1,
        "graphit_task_session_create",
        json!({
            "idempotency_key":"parallel-session", "title":"Parallel Root clients",
            "description":"Same authenticated actor, independent target fences", "strategy":"Actual stdio Source SDK",
        })
    );
    let session_id = &session["result"]["id"];
    let mut ids = Vec::new();
    for key in ["parallel-a", "parallel-b"] {
        let created = call!(
            clients[0].1,
            "graphit_task_create",
            json!({
                "idempotency_key":key,"session_id":session_id,"title":key,
                "description":"A target claim remains independent of other targets",
                "task_type":"TASK","priority":1,"checks":[
                    {"kind":"acceptance","text":"Keep private target fences"},
                    {"kind":"test","text":"Use two actual Root processes"},
                ],
            })
        );
        ids.push(created["result"]["specification"]["id"].clone());
    }
    let (left, right) = tokio::try_join!(
        request!(
            clients[0].1,
            "graphit_task_claim",
            json!({"id":ids[0],"lease_ms":120000})
        ),
        request!(
            clients[1].1,
            "graphit_task_claim",
            json!({"id":ids[1],"lease_ms":120000})
        ),
    )?;
    let claims = [wire(left)?, wire(right)?];
    let owner = &claims[0]["result"]["record"]["ownership"]["owner"];
    assert!(owner.is_string());
    assert_eq!(owner, &claims[1]["result"]["record"]["ownership"]["owner"]);
    assert_ne!(claims[0]["result"]["token"], claims[1]["result"]["token"]);
    let mut details = Vec::new();
    for id in &ids {
        let detail = call!(
            clients[0].1,
            "graphit_task_get",
            json!({"id":id,"history":{"page_size":100}})
        );
        assert_eq!(detail["result"]["task"]["status"], "in_progress");
        assert_eq!(
            detail["result"]["history"]["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        for claim in &claims {
            assert!(
                !detail
                    .to_string()
                    .contains(claim["result"]["token"].as_str().unwrap())
            );
        }
        details.push(detail);
    }
    // This identity already owns this target: retry returns its existing fence, without renewal.
    let retry = call!(
        clients[1].1,
        "graphit_task_claim",
        json!({"id":ids[0],"lease_ms":1})
    );
    assert_eq!(retry, claims[0]);
    for (index, id) in ids.iter().enumerate() {
        let denied = request!(clients[index].1, "graphit_task_heartbeat", json!({
            "target":{"id":id,"claim":{
                "token":claims[1-index]["result"]["token"],
                "expected_revision":claims[index]["result"]["record"]["specification"]["revision"],
            }},"lease_ms":120000,
        })).await?;
        assert_eq!(denied.is_error, Some(true));
        assert!(format!("{denied:?}").contains("fence_mismatch"));
        let detail = call!(
            clients[index].1,
            "graphit_task_get",
            json!({"id":id,"history":{"page_size":100}})
        );
        assert_eq!(detail, details[index]);
    }
    let mut right = clients.pop().unwrap();
    let left = clients.pop().unwrap();
    left.1.cancel().await?;
    let mut left_child = left.0;
    assert!(
        tokio::time::timeout(Duration::from_secs(10), left_child.wait())
            .await??
            .success()
    );
    let current = call!(
        right.1,
        "graphit_task_heartbeat",
        json!({
            "target":{"id":ids[1],"claim":{
                "token":claims[1]["result"]["token"],
                "expected_revision":claims[1]["result"]["record"]["specification"]["revision"],
            }},"lease_ms":120000,
        })
    );
    assert_eq!(current["result"]["ownership"]["owner"], *owner);
    let stale = request!(
        right.1,
        "graphit_task_heartbeat",
        json!({
            "target":{"id":ids[1],"claim":{
                "token":claims[1]["result"]["token"],
                "expected_revision":claims[1]["result"]["record"]["specification"]["revision"],
            }},"lease_ms":120000,
        })
    )
    .await?;
    assert_eq!(stale.is_error, Some(true));
    assert!(format!("{stale:?}").contains("revision_conflict"));
    let after = call!(
        right.1,
        "graphit_task_get",
        json!({"id":ids[1],"history":{"page_size":100}})
    );
    assert_eq!(after["result"]["task"], current["result"]);
    assert_eq!(
        after["result"]["history"]["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        call!(
            right.1,
            "graphit_task_get",
            json!({"id":ids[0],"history":{"page_size":100}})
        )["result"]["task"],
        details[0]["result"]["task"]
    );
    right.1.cancel().await?;
    assert!(
        tokio::time::timeout(Duration::from_secs(10), right.0.wait())
            .await??
            .success()
    );
    daemon.stop("-INT").await?;
    Ok(())
}
