use super::*;
use graphit_auth::{ProjectCapture, ProjectWriteOperation, ScopeCapture, ScopeInput};
use graphit_records::memory_store::{
    CreateMemory, MemoryStoreReadLimits, MemoryStoreWriteLimits, NativeMemoryStore,
};
use std::{num::NonZeroUsize, path::Path};

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Graphit fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn select_checkout(server: &TaskServer, root: &Path) {
    let snapshot = server.0.live_projects.snapshot().unwrap();
    let (paths, id, _) = &snapshot.projects[0];
    let checkout =
        GitCheckout::inspect(root, Duration::from_secs(5), &Cancellation::default()).unwrap();
    let version = VersionContext::select(paths, checkout.as_checkout(), "", None, "").unwrap();
    server
        .0
        .live_projects
        .replace(Arc::new(vec![(paths.clone(), *id, version)]), 0)
        .unwrap();
}
async fn memory(
    server: &TaskServer,
    raw: &str,
    key: &str,
) -> graphit_records::memory_store::Memory {
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let live = server.0.live_projects.snapshot().unwrap();
    let (paths, id, version) = &live.projects[0];
    let scope =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let binding = scope.binding().unwrap();
    let input = ScopeInput {
        installation: server.0.auth.installation(),
        project: Some(ProjectCapture {
            scope: ScopeCapture {
                live: &scope,
                record: &binding,
            },
            version,
        }),
        providers: &[],
        binding: server.0.policy.binding,
        global_version: "",
    };
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let write = authority
        .admit_authenticated_write(&caller, &original, input, ProjectWriteOperation::Memory)
        .unwrap();
    let n = |value| NonZeroUsize::new(value).unwrap();
    NativeMemoryStore::create(
        original.snapshot(),
        &write,
        CreateMemory {
            idempotency_key: key.into(),
            title: key.into(),
            body: format!("memory {key}"),
            attributes: graphit_records::memory_attributes::MemoryAttributes {
                classification: graphit_records::memory_attributes::MemoryClass::Fact,
                mandatory: false,
                important: false,
            },
            tags: vec![],
            references: None,
        },
        MemoryStoreWriteLimits {
            read: MemoryStoreReadLimits {
                max_query_bytes: n(65536),
                batch_rows: server.0.policy.read.batch_rows,
                native: server.0.policy.read.native,
            },
            artifacts: server.0.policy.write.artifacts,
            max_input_bytes: n(65536),
            max_row_bytes: n(65536),
        },
    )
    .await
    .unwrap()
}
async fn session(server: &TaskServer, raw: &str, key: &str) -> Value {
    server
        .call(
            serde_json::from_value(json!({"input":{"idempotency_key":key,
        "title":key,"description":"Git record fixture","strategy":"Actual native consumers"}}))
            .unwrap(),
            "create_session",
            true,
            Credential {
                role_binding: None,
                bearer: raw.to_owned().into(),
                origin: Origin::Runtime,
            },
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|error| panic!("{}", error.code))["result"]["id"]
        .clone()
}
async fn event(
    http: &reqwest::Client,
    base: &str,
    root: &Path,
    raw: &str,
    notification: Value,
    status: reqwest::StatusCode,
) -> Value {
    let response = http.post(format!("{base}{}", crate::git_records::ROUTE))
        .bearer_auth(raw).header("x-graphit-auth-origin", "runtime")
        .json(&json!({"project_dir":root,"event":notification,"restore_previous":null,"restore":false}))
        .send().await.unwrap();
    let actual = response.status();
    let value = response.json::<Value>().await.unwrap();
    assert_eq!(actual, status, "{value}");
    value
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_records_http_commit_detached_checkout_merge_and_permissions() {
    let (_temp, server, raw) =
        fixture_with_permissions(&[Permission::Read(Module::Memory), Permission::WriteMemory]);
    let root = server.0.live_projects.snapshot().unwrap().projects[0]
        .0
        .root()
        .to_owned();
    git(&root, &["init", "-b", "main"]);
    git(&root, &["commit", "--allow-empty", "-m", "initial"]);
    let socket = listener();
    let base = format!("http://{}", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let http = reqwest::Client::new();
    let empty = event(
        &http,
        &base,
        &root,
        &raw,
        Value::Null,
        reqwest::StatusCode::OK,
    )
    .await;
    assert_eq!(empty["state"], "applied");
    let a = session(&server, &raw, "record-a").await;
    memory(&server, &raw, "record-a").await;
    git(&root, &["commit", "--allow-empty", "-m", "A"]);
    let a_commit = git(&root, &["rev-parse", "HEAD"]);
    event(
        &http,
        &base,
        &root,
        &raw,
        json!({"kind":"commit"}),
        reqwest::StatusCode::OK,
    )
    .await;
    let b = session(&server, &raw, "record-b").await;
    memory(&server, &raw, "record-b").await;
    git(&root, &["commit", "--allow-empty", "-m", "B"]);
    let b_commit = git(&root, &["rev-parse", "HEAD"]);
    event(
        &http,
        &base,
        &root,
        &raw,
        json!({"kind":"commit"}),
        reqwest::StatusCode::OK,
    )
    .await;
    git(&root, &["checkout", "--detach", &a_commit]);
    event(
        &http,
        &base,
        &root,
        &raw,
        json!({"kind":"checkout","old":b_commit,"new":a_commit,"branch":true}),
        reqwest::StatusCode::OK,
    )
    .await;
    select_checkout(&server, &root);
    let credential = || Credential {
        role_binding: None,
        bearer: raw.clone().into(),
        origin: Origin::Runtime,
    };
    let read = |id| serde_json::from_value(json!({"input":{"id":id}})).unwrap();
    assert_eq!(
        server
            .call(
                read(a.clone()),
                "get_session",
                false,
                credential(),
                CancellationToken::new()
            )
            .await
            .unwrap_or_else(|error| panic!("{}", error.code))["result"]["session"]["id"],
        a
    );
    assert!(
        server
            .call(
                read(b.clone()),
                "get_session",
                false,
                credential(),
                CancellationToken::new()
            )
            .await
            .unwrap_or_else(|error| panic!("{}", error.code))["result"]
            .is_null()
    );
    git(&root, &["checkout", "-b", "feature", &a_commit]);
    event(
        &http,
        &base,
        &root,
        &raw,
        json!({"kind":"checkout","old":a_commit,"new":a_commit,"branch":true}),
        reqwest::StatusCode::OK,
    )
    .await;
    select_checkout(&server, &root);
    let c = session(&server, &raw, "record-c").await;
    memory(&server, &raw, "record-c").await;
    git(&root, &["commit", "--allow-empty", "-m", "C"]);
    let c_commit = git(&root, &["rev-parse", "HEAD"]);
    event(
        &http,
        &base,
        &root,
        &raw,
        json!({"kind":"commit"}),
        reqwest::StatusCode::OK,
    )
    .await;
    git(&root, &["checkout", "main"]);
    event(
        &http,
        &base,
        &root,
        &raw,
        json!({"kind":"checkout","old":c_commit,"new":b_commit,"branch":true}),
        reqwest::StatusCode::OK,
    )
    .await;
    select_checkout(&server, &root);
    git(
        &root,
        &["merge", "--no-ff", "feature", "-m", "merge records"],
    );
    let merged = event(
        &http,
        &base,
        &root,
        &raw,
        json!({"kind":"commit"}),
        reqwest::StatusCode::OK,
    )
    .await;
    assert_eq!(merged["state"], "applied");
    event(
        &http,
        &base,
        &root,
        &raw,
        json!({"kind":"merge","squash":false}),
        reqwest::StatusCode::OK,
    )
    .await;
    for id in [a.clone(), b.clone(), c] {
        assert_eq!(
            server
                .call(
                    read(id.clone()),
                    "get_session",
                    false,
                    credential(),
                    CancellationToken::new()
                )
                .await
                .unwrap_or_else(|error| panic!("{}", error.code))["result"]["session"]["id"],
            id
        );
    }
    let merge_commit = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["reset", "--hard", &a_commit]);
    event(&http, &base, &root, &raw,
        json!({"kind":"reference_transaction","updates":[{"old":merge_commit,"new":a_commit,"reference":"refs/heads/main"}]}),
        reqwest::StatusCode::OK).await;
    assert!(
        server
            .call(
                read(b.clone()),
                "get_session",
                false,
                credential(),
                CancellationToken::new()
            )
            .await
            .unwrap_or_else(|error| panic!("{}", error.code))["result"]
            .is_null()
    );
    git(&root, &["reset", "--hard", &merge_commit]);
    event(&http, &base, &root, &raw,
        json!({"kind":"reference_transaction","updates":[{"old":a_commit,"new":merge_commit,"reference":"refs/heads/main"}]}),
        reqwest::StatusCode::OK).await;
    assert_eq!(
        server
            .call(
                read(b.clone()),
                "get_session",
                false,
                credential(),
                CancellationToken::new()
            )
            .await
            .unwrap_or_else(|error| panic!("{}", error.code))["result"]["session"]["id"],
        b
    );
    let selected = server.0.live_projects.snapshot().unwrap();
    let (paths, project_id, _) = &selected.projects[0];
    for module in ["memory", "task"] {
        let directory = paths.project_runtime(*project_id, &[module]).unwrap();
        assert!(directory.join("index.lance").is_dir());
        assert_eq!(std::fs::read_dir(directory).unwrap().count(), 1);
    }
    // Forged parents are not a wire field; auth and actual HEAD remain independent.
    let response = http
        .post(format!("{base}{}", crate::git_records::ROUTE))
        .header("x-graphit-auth-origin", "runtime")
        .json(&json!({"project_dir":root,"event":null,"restore_previous":null,"restore":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    let rejected = http
        .post(format!("{base}{}", crate::git_records::ROUTE))
        .bearer_auth(&raw)
        .header("x-graphit-auth-origin", "local")
        .json(&json!({"project_dir":root,"event":null,"restore_previous":null,"restore":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), reqwest::StatusCode::FORBIDDEN);
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

#[tokio::test]
async fn git_records_http_requires_both_module_permissions_and_non_git_is_unchanged() {
    let (_temp, server, raw) = fixture();
    let root = server.0.live_projects.snapshot().unwrap().projects[0]
        .0
        .root()
        .to_owned();
    let socket = listener();
    let base = format!("http://{}", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let http = reqwest::Client::new();
    assert_eq!(
        event(
            &http,
            &base,
            &root,
            &raw,
            Value::Null,
            reqwest::StatusCode::OK
        )
        .await["state"],
        "non_git"
    );
    git(&root, &["init", "-b", "main"]);
    git(&root, &["commit", "--allow-empty", "-m", "initial"]);
    event(
        &http,
        &base,
        &root,
        &raw,
        Value::Null,
        reqwest::StatusCode::FORBIDDEN,
    )
    .await;
    assert!(
        !server.0.live_projects.snapshot().unwrap().projects[0]
            .0
            .project_runtime(
                server.0.live_projects.snapshot().unwrap().projects[0].1,
                &["memory", "index.lance"]
            )
            .unwrap()
            .exists()
    );
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}

// The hook executable is this same test binary. Only the temporary Git process
// environment carries fixture authentication; hook files contain no credential.
#[cfg(unix)]
struct RewriteHooks {
    root: std::path::PathBuf,
    base: String,
    raw: String,
    receipt: std::path::PathBuf,
}

#[cfg(unix)]
impl RewriteHooks {
    fn install(root: &Path, base: String, raw: String) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let executable = std::env::current_exe().unwrap();
        let child = shlex::try_join([
            executable.to_str().unwrap(),
            "--exact",
            "task_server::tests::git_records::git_records_real_hook_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .unwrap();
        for name in ["post-commit", "post-checkout", "post-rewrite"] {
            let path = root.join(".git/hooks").join(name);
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nexport GRAPHIT_TEST_HOOK_EVENT='{name}'\nexport GRAPHIT_TEST_HOOK_ARG_1=\"${{1-}}\"\nexport GRAPHIT_TEST_HOOK_ARG_2=\"${{2-}}\"\nexport GRAPHIT_TEST_HOOK_ARG_3=\"${{3-}}\"\n{child}\nresult=$?\nif test \"$result\" -ne 0; then printf 'failed\\n' >> \"$GRAPHIT_TEST_HOOK_RECEIPT\"; fi\nexit \"$result\"\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let receipt = root.join(".git/graphit-test-hook-receipt");
        std::fs::write(&receipt, "").unwrap();
        Self {
            root: root.to_owned(),
            base,
            raw,
            receipt,
        }
    }

    fn run(&self, arguments: &[&str]) -> Vec<graphit_core::git_record_event::GitRecordEvent> {
        let previous = std::fs::read_to_string(&self.receipt).unwrap();
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args([
                "-c",
                "user.name=Graphit fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_EDITOR", "true")
            .env("GRAPHIT_TEST_HOOK_ROOT", &self.root)
            .env("GRAPHIT_TEST_HOOK_BASE", &self.base)
            .env("GRAPHIT_TEST_HOOK_RAW", &self.raw)
            .env("GRAPHIT_TEST_HOOK_RECEIPT", &self.receipt)
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let receipt = std::fs::read_to_string(&self.receipt).unwrap();
        let added = receipt.strip_prefix(&previous).unwrap();
        assert!(!added.is_empty(), "Git did not execute the expected hooks");
        added
            .lines()
            .map(|line| {
                serde_json::from_str(line).unwrap_or_else(|_| {
                    panic!(
                        "Git hook failed for {arguments:?}: {} {}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    )
                })
            })
            .collect()
    }
}

#[cfg(unix)]
#[test]
#[ignore = "child invoked only by temporary Git hooks in the real amend/rebase test"]
fn git_records_real_hook_child() {
    use graphit_core::git_record_event::{GitRecordEvent, GitRewrite};
    use std::io::{Read, Write};
    let argument = |number| std::env::var(format!("GRAPHIT_TEST_HOOK_ARG_{number}")).unwrap();
    let notification = match std::env::var("GRAPHIT_TEST_HOOK_EVENT").unwrap().as_str() {
        "post-commit" => GitRecordEvent::Commit {},
        "post-checkout" => GitRecordEvent::Checkout {
            old: argument(1),
            new: argument(2),
            branch: argument(3) == "1",
        },
        "post-rewrite" => {
            assert!(matches!(argument(1).as_str(), "amend" | "rebase"));
            let mut input = String::new();
            std::io::stdin()
                .take(65537)
                .read_to_string(&mut input)
                .unwrap();
            assert!(input.len() <= 65536);
            let pairs = input
                .lines()
                .map(|line| {
                    let mut fields = line.split_whitespace();
                    let pair = GitRewrite {
                        old: fields.next().unwrap().to_owned(),
                        new: fields.next().unwrap().to_owned(),
                    };
                    assert!(fields.next().is_none());
                    pair
                })
                .collect::<Vec<_>>();
            assert!(!pairs.is_empty());
            GitRecordEvent::Rewrite { pairs }
        }
        name => panic!("unexpected test hook {name}"),
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let http = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap();
            let response = event(
                &http,
                &std::env::var("GRAPHIT_TEST_HOOK_BASE").unwrap(),
                Path::new(&std::env::var_os("GRAPHIT_TEST_HOOK_ROOT").unwrap()),
                &std::env::var("GRAPHIT_TEST_HOOK_RAW").unwrap(),
                serde_json::to_value(&notification).unwrap(),
                reqwest::StatusCode::OK,
            )
            .await;
            assert_eq!(response["state"], "applied", "{response}");
        });
    let mut receipt = std::fs::OpenOptions::new()
        .append(true)
        .open(std::env::var_os("GRAPHIT_TEST_HOOK_RECEIPT").unwrap())
        .unwrap();
    serde_json::to_writer(&mut receipt, &notification).unwrap();
    writeln!(receipt).unwrap();
}

#[cfg(unix)]
async fn rewrite_record(
    server: &TaskServer,
    raw: &str,
    key: &str,
) -> (graphit_records::memory_store::Memory, Value) {
    let memory = memory(server, raw, key).await;
    let id = session(server, raw, key).await;
    let session = server
        .call(
            serde_json::from_value(json!({"input":{"id":id}})).unwrap(),
            "get_session",
            false,
            Credential {
                role_binding: None,
                bearer: raw.to_owned().into(),
                origin: Origin::Runtime,
            },
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|error| panic!("{}", error.code))["result"]["session"]
        .clone();
    assert_eq!(session["id"], id);
    (memory, session)
}

#[cfg(unix)]
async fn assert_rewrite_records(
    server: &TaskServer,
    raw: &str,
    records: &[(graphit_records::memory_store::Memory, Value)],
    present: &[usize],
) {
    let cancel = Cancellation::default();
    let original = RequestContext::new(
        server.0.policy.content,
        &cancel,
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let live = server.0.live_projects.snapshot().unwrap();
    let (paths, id, version) = &live.projects[0];
    let scope =
        CapturedPublicationScope::project(paths, *id, server.0.policy.scope, original.snapshot())
            .unwrap();
    let binding = scope.binding().unwrap();
    let issuer = server.0.auth.issuer();
    let caller = issuer.authenticate_runtime(raw, &original).unwrap();
    let authority = ProjectRecordAuthority::new(issuer);
    let admission = authority
        .admit_authenticated_read(
            &caller,
            &original,
            ScopeInput {
                installation: server.0.auth.installation(),
                project: Some(ProjectCapture {
                    scope: ScopeCapture {
                        live: &scope,
                        record: &binding,
                    },
                    version,
                }),
                providers: &[],
                binding: server.0.policy.binding,
                global_version: "",
            },
            graphit_auth::ProjectReadModule::Memory,
        )
        .unwrap();
    for (index, (memory, session)) in records.iter().enumerate() {
        let read = NativeMemoryStore::get(
            original.snapshot(),
            &admission,
            memory.id,
            MemoryStoreReadLimits {
                max_query_bytes: NonZeroUsize::new(65536).unwrap(),
                batch_rows: server.0.policy.read.batch_rows,
                native: server.0.policy.read.native,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            read.as_ref().map(|detail| &detail.memory),
            present.contains(&index).then_some(memory)
        );
        let read = server
            .call(
                serde_json::from_value(json!({"input":{"id":session["id"]}})).unwrap(),
                "get_session",
                false,
                Credential {
                    role_binding: None,
                    bearer: raw.to_owned().into(),
                    origin: Origin::Runtime,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("{}", error.code));
        if present.contains(&index) {
            assert_eq!(read["result"]["session"], *session);
        } else {
            assert!(read["result"].is_null(), "{read}");
        }
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_records_real_hooks_preserve_amend_and_rebase_snapshots() {
    use graphit_core::git_record_event::GitRecordEvent;
    let (_temp, server, raw) =
        fixture_with_permissions(&[Permission::Read(Module::Memory), Permission::WriteMemory]);
    let root = server.0.live_projects.snapshot().unwrap().projects[0]
        .0
        .root()
        .to_owned();
    git(&root, &["init", "-b", "main"]);
    git(&root, &["commit", "--allow-empty", "-m", "base"]);
    let baseline = git(&root, &["rev-parse", "HEAD"]);
    select_checkout(&server, &root);
    let socket = listener();
    let base = format!("http://{}", socket.address());
    let serving = server.clone();
    let served = tokio::spawn(async move { serving.serve_listener(socket).await });
    let mut records = vec![rewrite_record(&server, &raw, "rewrite-base").await];
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    event(
        &http,
        &base,
        &root,
        &raw,
        Value::Null,
        reqwest::StatusCode::OK,
    )
    .await;
    let hooks = RewriteHooks::install(&root, base, raw.clone());

    hooks.run(&["checkout", "-b", "feature"]);
    select_checkout(&server, &root);
    records.push(rewrite_record(&server, &raw, "rewrite-f1").await);
    std::fs::write(root.join("feature.txt"), "F1\n").unwrap();
    git(&root, &["add", "feature.txt"]);
    hooks.run(&["commit", "-m", "feature one"]);
    let original = git(&root, &["rev-parse", "HEAD"]);

    records.push(rewrite_record(&server, &raw, "rewrite-f2").await);
    std::fs::write(root.join("feature.txt"), "F1 plus F2\n").unwrap();
    git(&root, &["add", "feature.txt"]);
    let amend_events = hooks.run(&["commit", "--amend", "--no-edit"]);
    let amended = git(&root, &["rev-parse", "HEAD"]);
    assert_ne!(original, amended);
    assert!(
        matches!(amend_events.as_slice(), [GitRecordEvent::Commit {}, GitRecordEvent::Rewrite { pairs }] if pairs.len() == 1 && pairs[0].old == original && pairs[0].new == amended)
    );
    assert_rewrite_records(&server, &raw, &records, &[0, 1, 2]).await;

    hooks.run(&["checkout", "main"]);
    select_checkout(&server, &root);
    assert_rewrite_records(&server, &raw, &records, &[0]).await;
    records.push(rewrite_record(&server, &raw, "rewrite-main").await);
    std::fs::write(root.join("main.txt"), "upstream\n").unwrap();
    git(&root, &["add", "main.txt"]);
    hooks.run(&["commit", "-m", "upstream records"]);
    let upstream = git(&root, &["rev-parse", "HEAD"]);
    hooks.run(&["checkout", "feature"]);
    select_checkout(&server, &root);
    assert_rewrite_records(&server, &raw, &records, &[0, 1, 2]).await;

    let rebase_events = hooks.run(&["rebase", "main"]);
    let rebased = git(&root, &["rev-parse", "HEAD"]);
    assert_ne!(rebased, amended);
    assert_eq!(git(&root, &["rev-parse", "HEAD^"]), upstream);
    assert!(
        rebase_events
            .iter()
            .any(|event| matches!(event, GitRecordEvent::Checkout { new, .. } if new == &upstream))
    );
    assert!(
        rebase_events
            .iter()
            .any(|event| matches!(event, GitRecordEvent::Commit {}))
    );
    assert!(
        matches!(rebase_events.last(), Some(GitRecordEvent::Rewrite { pairs }) if pairs.len() == 1 && pairs[0].old == amended && pairs[0].new == rebased)
    );
    select_checkout(&server, &root);
    assert_rewrite_records(&server, &raw, &records, &[0, 1, 2, 3]).await;

    // Revisit every old immutable tag through the same ordinary HTTP consumer.
    for (commit, present) in [
        (baseline.as_str(), &[0][..]),
        (original.as_str(), &[0, 1][..]),
        (amended.as_str(), &[0, 1, 2][..]),
        (upstream.as_str(), &[0, 3][..]),
        (rebased.as_str(), &[0, 1, 2, 3][..]),
    ] {
        hooks.run(&["checkout", "--detach", commit]);
        select_checkout(&server, &root);
        assert_rewrite_records(&server, &raw, &records, present).await;
    }
    server.shutdown().await.unwrap();
    served.await.unwrap().unwrap();
}
