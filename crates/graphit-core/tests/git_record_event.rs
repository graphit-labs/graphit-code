use graphit_core::{
    file_lock::Cancellation,
    git_record_event::{GitRecordEvent, GitRecordFacts, GitRewrite, validate_commit_id},
};
use std::{fs, io, path::Path, process::Command, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(10);

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}
fn init(root: &Path) {
    git(root, &["init", "--initial-branch=main"]);
    git(root, &["config", "user.name", "Graphit Test"]);
    git(
        root,
        &["config", "user.email", "graphit-test@example.invalid"],
    );
}
fn commit(root: &Path, text: &str) -> String {
    fs::write(root.join("source.txt"), text).unwrap();
    git(root, &["add", "source.txt"]);
    git(root, &["commit", "--no-verify", "-m", text]);
    git(root, &["rev-parse", "HEAD"])
}
fn facts(root: &Path) -> GitRecordFacts {
    GitRecordFacts::inspect(root, TIMEOUT, &Cancellation::default())
        .unwrap()
        .unwrap()
}

#[test]
fn captures_unborn_commits_merge_parents_and_detached_worktrees() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo ação");
    fs::create_dir(&root).unwrap();
    assert!(
        GitRecordFacts::inspect(&root, TIMEOUT, &Cancellation::default())
            .unwrap()
            .is_none()
    );
    init(&root);
    let unborn = facts(&root);
    assert_eq!(unborn.head, None);
    assert_eq!(unborn.branch.as_deref(), Some("main"));
    assert!(unborn.parents.is_empty());
    let a = commit(&root, "a");
    let first = facts(&root);
    assert_eq!(first.head.as_deref(), Some(a.as_str()));
    assert!(first.parents.is_empty());
    assert_eq!(first.root, root.canonicalize().unwrap());
    assert_eq!(first.common_dir, root.join(".git").canonicalize().unwrap());
    assert!(
        first
            .previous_head(TIMEOUT, &Cancellation::default())
            .unwrap()
            .is_none()
    );
    git(&root, &["checkout", "-b", "feature"]);
    let b = commit(&root, "b");
    assert_eq!(facts(&root).parents.as_slice(), std::slice::from_ref(&a));
    git(&root, &["checkout", "main"]);
    git(&root, &["merge", "--ff-only", "feature"]);
    let ff = facts(&root);
    assert_eq!(ff.head.as_deref(), Some(b.as_str()));
    assert_eq!(
        ff.previous_head(TIMEOUT, &Cancellation::default())
            .unwrap()
            .as_deref(),
        Some(a.as_str())
    );
    assert_eq!(
        ff.merge_base(&a, &b, TIMEOUT, &Cancellation::default())
            .unwrap()
            .as_deref(),
        Some(a.as_str())
    );
    let linked = temp.path().join("linked");
    git(
        &root,
        &["worktree", "add", "--detach", linked.to_str().unwrap(), &a],
    );
    let detached = facts(&linked);
    assert_eq!(detached.head.as_deref(), Some(a.as_str()));
    assert_eq!(detached.branch, None);
    assert_eq!(detached.common_dir, ff.common_dir);
    assert_ne!(detached.root, ff.root);
    // No hook parameter can supply arbitrary revision syntax or a non-commit.
    assert!(
        ff.resolve_commit("HEAD", TIMEOUT, &Cancellation::default())
            .is_err()
    );
    let blob = git(&root, &["rev-parse", "HEAD:source.txt"]);
    assert!(
        ff.resolve_commit(&blob, TIMEOUT, &Cancellation::default())
            .is_err()
    );
    assert_eq!(
        ff.resolve_commit(&a, TIMEOUT, &Cancellation::default())
            .unwrap(),
        a
    );
}

#[test]
fn multiple_merge_bases_are_rejected_without_arbitrary_selection() {
    let temp = tempfile::tempdir().unwrap();
    init(temp.path());
    let a = commit(temp.path(), "a");
    let tree = git(temp.path(), &["rev-parse", "HEAD^{tree}"]);
    let b = git(temp.path(), &["commit-tree", &tree, "-p", &a, "-m", "b"]);
    let c = git(temp.path(), &["commit-tree", &tree, "-p", &a, "-m", "c"]);
    let left = git(
        temp.path(),
        &["commit-tree", &tree, "-p", &b, "-p", &c, "-m", "left"],
    );
    let right = git(
        temp.path(),
        &["commit-tree", &tree, "-p", &c, "-p", &b, "-m", "right"],
    );
    let error = facts(temp.path())
        .merge_base(&left, &right, TIMEOUT, &Cancellation::default())
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("unique common ancestor"));
}

#[test]
fn events_object_ids_and_cancellation_are_explicit() {
    let old = "a".repeat(40);
    let new = "b".repeat(64);
    for id in [&old, &new] {
        validate_commit_id(id).unwrap();
    }
    for id in [
        "",
        "HEAD",
        "-option",
        "abc\0",
        &"x".repeat(40),
        &"a".repeat(41),
    ] {
        assert_eq!(
            validate_commit_id(id).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    let event = GitRecordEvent::Rewrite {
        pairs: vec![GitRewrite { old, new }],
    };
    assert_eq!(
        serde_json::from_str::<GitRecordEvent>(&serde_json::to_string(&event).unwrap()).unwrap(),
        event
    );
    assert!(
        serde_json::from_str::<GitRecordEvent>(r#"{"kind":"commit","path":"untrusted"}"#).is_err()
    );
    let temp = tempfile::tempdir().unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        GitRecordFacts::inspect(temp.path(), TIMEOUT, &cancellation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(
        GitRecordFacts::inspect(temp.path(), Duration::ZERO, &Cancellation::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
}

#[test]
fn post_commit_facts_child() {
    let Some(output) = std::env::var_os("GRAPHIT_GIT_FACTS_HOOK_OUTPUT") else {
        return;
    };
    let observed = facts(&std::env::current_dir().unwrap());
    let source_parents = observed.replayed_commit.as_ref().map(|commit| {
        observed
            .parents_of(commit, TIMEOUT, &Cancellation::default())
            .unwrap()
    });
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output)
        .unwrap();
    use std::io::Write;
    writeln!(file, "{}", serde_json::json!({"head":observed.head,"parents":observed.parents,
        "replayed_commit":observed.replayed_commit,"source_parents":source_parents,"branch":observed.branch})).unwrap();
}

#[test]
fn real_rebase_post_commit_facts_identify_each_replayed_commit_and_its_parents() {
    let temp = tempfile::tempdir().unwrap();
    init(temp.path());
    let base = commit(temp.path(), "base");
    git(temp.path(), &["checkout", "-b", "feature"]);
    let old_first = commit(temp.path(), "feature-one");
    let old_second = commit(temp.path(), "feature-two");
    git(temp.path(), &["checkout", "main"]);
    fs::write(temp.path().join("main.txt"), "main change").unwrap();
    git(temp.path(), &["add", "main.txt"]);
    git(temp.path(), &["commit", "-m", "main-change"]);
    let onto = git(temp.path(), &["rev-parse", "HEAD"]);
    git(temp.path(), &["checkout", "feature"]);
    let output = temp.path().join("facts.jsonl");
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
    let executable = std::env::current_exe().unwrap();
    let command = format!(
        "GRAPHIT_GIT_FACTS_HOOK_OUTPUT={} {} --exact post_commit_facts_child --nocapture",
        quote(output.to_str().unwrap()),
        quote(executable.to_str().unwrap())
    );
    git(
        temp.path(),
        &["config", "hook.graphit-facts.event", "post-commit"],
    );
    git(
        temp.path(),
        &["config", "hook.graphit-facts.command", &command],
    );
    git(temp.path(), &["rebase", "main"]);
    let lines = fs::read_to_string(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["replayed_commit"], old_first);
    assert_eq!(lines[0]["source_parents"], serde_json::json!([base]));
    assert_eq!(lines[0]["parents"], serde_json::json!([onto]));
    assert!(lines[0]["branch"].is_null());
    assert_eq!(lines[1]["replayed_commit"], old_second);
    assert_eq!(lines[1]["source_parents"], serde_json::json!([old_first]));
    assert_eq!(lines[1]["parents"], serde_json::json!([lines[0]["head"]]));
    assert!(lines[1]["branch"].is_null());
    let final_facts = facts(temp.path());
    assert_eq!(final_facts.replayed_commit, None);
    assert_eq!(final_facts.branch.as_deref(), Some("feature"));
    assert_eq!(final_facts.head.as_deref(), lines[1]["head"].as_str());
    assert!(
        final_facts
            .parents_of("HEAD", TIMEOUT, &Cancellation::default())
            .is_err()
    );
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert_eq!(
        final_facts
            .parents_of(&old_second, TIMEOUT, &cancelled)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
}
