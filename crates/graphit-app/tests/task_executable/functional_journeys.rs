use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_functional_cli_uses_native_records_and_human_default() -> Result {
    let f = Fixture::new().await?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let human = f.human_command().args(["projects", "list"]).output()?;
    assert!(
        human.status.success(),
        "{}",
        String::from_utf8_lossy(&human.stderr)
    );
    assert!(human.stderr.is_empty());
    let text = String::from_utf8(human.stdout)?;
    assert!(!text.trim_start().starts_with('{'));
    assert!(text.contains("current"), "{text}");
    let projects = f.command().args(["projects", "list"]).output()?;
    assert!(
        projects.status.success(),
        "{}",
        String::from_utf8_lossy(&projects.stderr)
    );
    let projects: Value = serde_json::from_slice(&projects.stdout)?;
    assert!(projects.to_string().contains("checkouts"));

    let body = f._temp.path().join("memory.md");
    fs::write(&body, "A durable native terminal record.\nSecond line.")?;
    let created = f
        .command()
        .args([
            "memory",
            "create",
            "--idempotency-key",
            "compiled-functional-memory",
            "--classification",
            "fact",
            "--title",
            "Functional terminal record",
            "--body-file",
        ])
        .arg(&body)
        .output()?;
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let created: Value = serde_json::from_slice(&created.stdout)?;
    assert_eq!(created["native"]["state"], "completed");
    let memory_id = created["native"]["id"].as_str().unwrap();
    let fetched = f
        .command()
        .args(["memory", "get", "--id", memory_id])
        .output()?;
    assert!(
        fetched.status.success(),
        "{}",
        String::from_utf8_lossy(&fetched.stderr)
    );
    let fetched: Value = serde_json::from_slice(&fetched.stdout)?;
    assert!(fetched.to_string().contains("Functional terminal record"));
    let listed = f.command().args(["memory", "list"]).output()?;
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let listed: Value = serde_json::from_slice(&listed.stdout)?;
    assert!(listed.to_string().contains("Functional terminal record"));

    let session = f.task("create-session", Some(json!({"idempotency_key":"compiled-cancel-session", "title":"Cancelled demand", "description":"Explicit cancellation", "strategy":"Preserve history"})), &[])?;
    let id = session["result"]["id"].as_str().unwrap();
    let cancelled = f
        .command()
        .args([
            "task",
            "cancel-session",
            "--id",
            id,
            "--expected-revision",
            "1",
            "--reason",
            "Demand withdrawn",
        ])
        .output()?;
    assert!(
        cancelled.status.success(),
        "{}",
        String::from_utf8_lossy(&cancelled.stderr)
    );
    let cancelled: Value = serde_json::from_slice(&cancelled.stdout)?;
    assert_eq!(cancelled["result"]["status"], "cancelled");
    let audit = f._temp.path().join("cancelled-session.json");
    let exported = f
        .human_command()
        .args([
            "task",
            "export-session",
            "--id",
            id,
            "--page-size",
            "1",
            "--output",
        ])
        .arg(&audit)
        .output()?;
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&exported.stdout)
            .trim_start()
            .starts_with('{')
    );
    let audit: Value = serde_json::from_slice(&fs::read(audit)?)?;
    assert_eq!(audit["session"]["status"], "cancelled");
    assert_eq!(audit["history"]["events"].as_array().unwrap().len(), 1);
    let references = f
        .command()
        .args(["references", "task", "--session", id, "--page-size", "1"])
        .output()?;
    assert!(
        references.status.success(),
        "{}",
        String::from_utf8_lossy(&references.stderr)
    );
    let _: Value = serde_json::from_slice(&references.stdout)?;
    let status = f.command().args(["index", "status"]).output()?;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let _: Value = serde_json::from_slice(&status.stdout)?;
    let catalogue = f
        .command()
        .args(["knowledge", "list", "--page-size", "1"])
        .output()?;
    assert!(
        catalogue.status.success(),
        "{}",
        String::from_utf8_lossy(&catalogue.stderr)
    );
    let _: Value = serde_json::from_slice(&catalogue.stdout)?;
    let lint = f.command().args(["knowledge", "lint"]).output()?;
    assert!(
        matches!(lint.status.code(), Some(0 | 1 | 3)),
        "{}",
        String::from_utf8_lossy(&lint.stderr)
    );
    let _: Value = serde_json::from_slice(&lint.stdout)?;
    daemon.stop("-TERM").await?;
    Ok(())
}
