//! Real notifications retain the authenticated Task/Memory producer while checks share each event.
use super::*;

fn command(f: &Fixture, executable: &str, args: &[&str]) -> Result<std::process::Output> {
    let source = f.command();
    let mut child = Command::new(executable);
    child
        .env_clear()
        .current_dir(f.projects[0].root())
        .args(args);
    for (key, value) in source.get_envs() {
        if let Some(value) = value {
            child.env(key, value);
        }
    }
    let path = std::env::join_paths(
        std::iter::once(
            std::path::Path::new(env!("CARGO_BIN_EXE_graphit"))
                .parent()
                .unwrap()
                .to_owned(),
        )
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )?;
    child.env("PATH", path);
    let output = child.output()?;
    assert!(
        output.status.success(),
        "{args:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}
fn git(f: &Fixture, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(command(f, "git", args)?.stdout)?)
}
fn graphit_git(f: &Fixture, args: &[&str]) -> Result {
    let args = std::iter::once("git")
        .chain(args.iter().copied())
        .collect::<Vec<_>>();
    command(f, env!("CARGO_BIN_EXE_graphit"), &args)?;
    Ok(())
}
fn create(f: &Fixture, key: &str, version: &str) -> Result<Value> {
    let selected = ["--project-version", version];
    let session = f.task(
        "create-session",
        Some(
            json!({"idempotency_key":format!("session-{key}"),"title":key,
        "description":"Git hook integration","strategy":"native Git events"}),
        ),
        &selected,
    )?;
    let id = session["result"]["id"].clone();
    f.task(
        "claim-session",
        Some(json!({"id":id,"lease_ms":120000})),
        &selected,
    )?;
    let task=f.task("create-task",Some(json!({"idempotency_key":format!("task-{key}"),"session_id":id,"title":key,
        "description":"Git notification preserves native record","task_type":"TASK","priority":1,
        "checks":[{"kind":"acceptance","text":"Preserve record"},{"kind":"test","text":"Read after Git transition"}]})),&selected)?;
    Ok(task["result"]["specification"]["id"].clone())
}

#[tokio::test(flavor = "current_thread")]
async fn native_git_hooks_keep_record_snapshots_checkout_rewrite_and_merge() -> Result {
    let f = Fixture::new().await?;
    ConfigStore::project(&f.projects[0])?.set(
        ConfigKey::new("githooks")?,
        json!({
            "post-commit":["printf x >> commit-hook-count"],
            "post-rewrite":["cat > rewrite-payload"],
            "post-checkout":["printf x >> checkout-hook-count"],
            "post-merge":["printf x >> merge-hook-count"]
        }),
        Duration::from_secs(5),
        &Cancellation::default(),
    )?;
    let daemon = f.daemon(&["--project-version", "main"]).await?;
    let main = create(&f, "main-record", "main")?;
    graphit_git(&f, &["install"])?;
    git(
        &f,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "main hook snapshot",
        ],
    )?;
    let old = git(&f, &["rev-parse", "HEAD"])?;
    git(
        &f,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--amend",
            "--allow-empty",
            "-m",
            "amended hook snapshot",
        ],
    )?;
    let new = git(&f, &["rev-parse", "HEAD"])?;
    assert_eq!(
        fs::read_to_string(f.projects[0].root().join("rewrite-payload"))?,
        format!("{} {}\n", old.trim(), new.trim())
    );
    assert_eq!(
        fs::read_to_string(f.projects[0].root().join("commit-hook-count"))?,
        "xx"
    );
    git(&f, &["switch", "feature"])?;
    let feature = create(&f, "feature-record", "feature")?;
    git(
        &f,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "feature hook snapshot",
        ],
    )?;
    git(&f, &["switch", "main"])?;
    let view = f.task(
        "query",
        Some(query_input("tasks")),
        &["--project-version", "main"],
    )?;
    assert!(view.to_string().contains(main.as_str().unwrap()));
    assert!(!view.to_string().contains(feature.as_str().unwrap()));
    git(
        &f,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "merge",
            "--no-edit",
            "feature",
        ],
    )?;
    let view = f.task(
        "query",
        Some(query_input("tasks")),
        &["--project-version", "main"],
    )?;
    assert!(view.to_string().contains(main.as_str().unwrap()));
    assert!(view.to_string().contains(feature.as_str().unwrap()));
    assert_eq!(
        git(
            &f,
            &[
                "config",
                "--local",
                "--get-all",
                "hook.graphit--post-commit.event"
            ]
        )?,
        "post-commit\n"
    );
    assert_eq!(
        fs::read_to_string(f.projects[0].root().join("merge-hook-count"))?,
        "x"
    );
    daemon.stop("-INT").await?;
    Ok(())
}
