use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_task_cli_revises_native_session_with_current_fence() -> Result {
    let f = Fixture::new().await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let created = f.task(
        "create-session",
        Some(json!({
            "idempotency_key":"cli-session-revision", "title":"Original CLI intent",
            "description":"Keep the accepted demand", "strategy":"Keep the current strategy",
        })),
        &[],
    )?;
    let id = &created["result"]["id"];
    let claim = f.task(
        "claim-session",
        Some(json!({"id":id,"lease_ms":120000})),
        &[],
    )?;
    let token = &claim["result"]["token"];
    let before = f.task("get-session", Some(json!({"id":id})), &[])?;
    let input = json!({
        "target":{"id":id,"claim":{"token":token,"expected_revision":claim["result"]["record"]["revision"]}},
        "lease_ms":120000,"reason":"CLI accepted a new title","title":"Updated CLI intent",
    });
    let revised = f.task("revise-session", Some(input.clone()), &[])?;
    assert_eq!(revised["result"]["id"], *id);
    assert_eq!(revised["result"]["input"]["title"], "Updated CLI intent");
    for field in ["description", "strategy"] {
        assert_eq!(
            revised["result"]["input"][field],
            created["result"]["input"][field]
        );
    }
    assert_eq!(
        revised["result"]["ownership"]["claim_epoch"],
        claim["result"]["record"]["ownership"]["claim_epoch"]
    );
    assert_eq!(
        revised["result"]["revision"].as_u64().unwrap(),
        claim["result"]["record"]["revision"].as_u64().unwrap() + 1
    );
    let detail = f.task("get-session", Some(json!({"id":id})), &[])?;
    assert_eq!(detail["result"]["session"], revised["result"]);
    assert_eq!(detail["result"]["creation"], before["result"]["creation"]);
    assert!(detail["result"]["history"].is_null());
    assert!(before["result"]["history"].is_null());
    let historical = f.task(
        "get-session",
        Some(json!({"id":id,
            "history":{"page_size":100,"cursor":detail["result"]["history_start_cursor"]}
        })),
        &[],
    )?;
    let predecessor = f.task(
        "get-session",
        Some(json!({"id":id,
            "history":{"page_size":100,"cursor":before["result"]["history_start_cursor"]}
        })),
        &[],
    )?;
    assert_eq!(historical["result"]["session"], detail["result"]["session"]);
    assert_eq!(
        predecessor["result"]["session"],
        before["result"]["session"]
    );
    assert!(historical["result"]["history"]["next_cursor"].is_null());
    assert!(predecessor["result"]["history"]["next_cursor"].is_null());
    let events = historical["result"]["history"]["events"]
        .as_array()
        .unwrap();
    let previous = predecessor["result"]["history"]["events"]
        .as_array()
        .unwrap();
    assert_eq!(events.len(), previous.len() + 1);
    assert_eq!(&events[..previous.len()], previous.as_slice());
    assert_eq!(events.last().unwrap()["record"], revised["result"]);
    assert!(
        events
            .last()
            .unwrap()
            .to_string()
            .contains("CLI accepted a new title")
    );
    assert!(!detail.to_string().contains(token.as_str().unwrap()));
    assert!(!historical.to_string().contains(token.as_str().unwrap()));
    assert!(!predecessor.to_string().contains(token.as_str().unwrap()));

    let stale = f
        .command()
        .args(["task", "revise-session"])
        .args(task_flags("revise-session", &input))
        .output()?;
    assert_eq!(stale.status.code(), Some(1));
    assert!(stale.stdout.is_empty());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("revision_conflict"));
    assert_eq!(f.task("get-session", Some(json!({"id":id})), &[])?, detail);

    let other = f.task("create-session", Some(json!({
        "idempotency_key":"cli-other-session", "title":"Separate target",
        "description":"A token grants no ownership of another target", "strategy":"Keep private fences independent",
    })), &[])?;
    let other_claim = f.task(
        "claim-session",
        Some(json!({"id":other["result"]["id"],"lease_ms":120000})),
        &[],
    )?;
    // Other targets legitimately advance the shared dataset pin. Capture the
    // current envelope before the refused mutation, keeping this target intact.
    let before_foreign = f.task("get-session", Some(json!({"id":id})), &[])?;
    for field in ["session", "creation", "last_event"] {
        assert_eq!(before_foreign["result"][field], detail["result"][field]);
    }
    assert!(
        before_foreign["result"]["generation"]["version"]
            .as_u64()
            .unwrap()
            > detail["result"]["generation"]["version"].as_u64().unwrap()
    );
    let pinned = f.task(
        "get-session",
        Some(json!({"id":id,
            "history":{"page_size":100,"cursor":detail["result"]["history_start_cursor"]}
        })),
        &[],
    )?;
    assert_eq!(pinned, historical);
    let foreign_input = json!({
        "target":{"id":id,"claim":{"token":other_claim["result"]["token"],"expected_revision":revised["result"]["revision"]}},
        "lease_ms":120000,"reason":"A different target token must fail","title":"Denied title",
    });
    let foreign = f
        .command()
        .args(["task", "revise-session"])
        .args(task_flags("revise-session", &foreign_input))
        .output()?;
    assert_eq!(foreign.status.code(), Some(1));
    assert!(foreign.stdout.is_empty());
    assert!(String::from_utf8_lossy(&foreign.stderr).contains("fence_mismatch"));
    assert_eq!(
        f.task("get-session", Some(json!({"id":id})), &[])?,
        before_foreign
    );
    daemon.stop("-INT").await?;
    Ok(())
}
