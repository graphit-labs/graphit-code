//! Git HTTP consumers over shared authoritative S3 snapshots, without local routing evidence.
use super::*;
use std::path::{Path, PathBuf};

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Graphit S3 fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn root(client: &Client) -> PathBuf {
    client
        .server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live Git S3 project selection"))
        .projects[0]
        .0
        .root()
        .to_owned()
}
fn select_git(client: &Client) {
    let current = client
        .server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live Git S3 project selection"));
    let (paths, id, _) = &current.projects[0];
    let checkout = graphit_core::git_checkout::GitCheckout::inspect(
        paths.root(),
        Duration::from_secs(5),
        &Cancellation::default(),
    )
    .unwrap();
    let version = graphit_core::version_context::VersionContext::select(
        paths,
        checkout.as_checkout(),
        "",
        None,
        "",
    )
    .unwrap();
    let mut projects = current.projects.as_ref().clone();
    projects[0] = (paths.clone(), *id, version);
    client
        .server
        .0
        .live_projects
        .replace(Arc::new(projects), 0)
        .unwrap();
}
fn assert_no_local_git_routes(client: &Client) {
    let current = client
        .server
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live Git S3 project selection"));
    let (paths, id, _) = &current.projects[0];
    assert!(
        !paths
            .project_runtime(*id, &["git-records"])
            .unwrap()
            .exists(),
        "S3 tags must not require or create installation-local Git routes"
    );
}
fn serve(client: &Client) -> (String, tokio::task::JoinHandle<io::Result<()>>) {
    let socket = graphit_core::tcp_listener::reserve(
        graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        },
        &Cancellation::default(),
    )
    .unwrap();
    let origin = format!("http://{}", socket.address());
    let server = client.server.clone();
    (
        origin,
        tokio::spawn(async move { server.serve_listener(socket).await }),
    )
}
async fn event(
    http: &reqwest::Client,
    origin: &str,
    client: &Client,
    notification: Value,
) -> Value {
    let response=http.post(format!("{origin}{}",crate::git_records::ROUTE))
        .bearer_auth(&client.raw).header("x-graphit-auth-origin","runtime")
        .json(&json!({"project_dir":root(client),"event":notification,"restore_previous":null,"restore":false}))
        .send().await.unwrap();
    let status = response.status();
    let value = response.json::<Value>().await.unwrap();
    assert_eq!(status, reqwest::StatusCode::OK, "{value}");
    assert_eq!(value["state"], "applied", "{value}");
    value
}
async fn new_task(client: &Client, key: &str) -> Value {
    let session=task_value(client.task("session_create",json!({"idempotency_key":format!("session-{key}"),"title":key,"description":"Git S3 records","strategy":"Actual independent consumers"}),true).await);
    let task=task_value(client.task("create",json!({"idempotency_key":format!("task-{key}"),"session_id":session["result"]["id"],"title":key,"description":"Git S3 Task","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Read the shared tagged record"},{"kind":"test","text":"Independent checkout sees exact snapshot"}]}),true).await);
    task["result"]["specification"]["id"].clone()
}

#[tokio::test]
#[ignore = "requires explicit local S3/temporary-STS fixture; no external account"]
async fn independent_git_http_clients_checkout_remote_tags_without_local_routes() {
    let broker = Broker::start(S3Fixture::load()).await;
    let a = Client::new(&broker, None);
    let a_root = root(&a);
    git(&a_root, &["init", "-b", "main"]);
    git(&a_root, &["commit", "--allow-empty", "-m", "initial"]);
    select_git(&a);
    let (a_origin, a_serving) = serve(&a);
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    event(&http, &a_origin, &a, Value::Null).await;
    let memory = a
        .memory("memory_create", memory_input("git-shared"), 0)
        .await
        .unwrap();
    let memory_id = memory["native"]["id"].clone();
    let task_id = new_task(&a, "Git tagged Task").await;
    git(
        &a_root,
        &["commit", "--allow-empty", "-m", "tagged records"],
    );
    let commit = git(&a_root, &["rev-parse", "HEAD"]);
    event(&http, &a_origin, &a, json!({"kind":"commit"})).await;
    // Advance authoritative data after the tag. B must restore the named Git
    // snapshot, rather than accidentally inherit this latest default-branch state.
    let later = a
        .memory("memory_create", memory_input("git-later"), 0)
        .await
        .unwrap();
    let later_id = later["native"]["id"].clone();
    let later_task = new_task(&a, "Uncommitted later Task").await;
    let b = Client::new(&broker, Some(&a));
    let b_root = root(&b);
    let clone = tempfile::tempdir().unwrap();
    let clone_root = clone.path().join("checkout");
    git(
        clone.path(),
        &[
            "clone",
            "--no-local",
            "--no-checkout",
            a_root.to_str().unwrap(),
            clone_root.to_str().unwrap(),
        ],
    );
    std::fs::rename(clone_root.join(".git"), b_root.join(".git")).unwrap();
    git(&b_root, &["checkout", "-b", "satellite"]);
    assert_eq!(git(&b_root, &["rev-parse", "HEAD"]), commit);
    select_git(&b);
    assert_eq!(
        a.server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live Git S3 project selection"))
            .projects[0]
            .1,
        b.server
            .record_snapshot()
            .unwrap_or_else(|_| panic!("live Git S3 project selection"))
            .projects[0]
            .1
    );
    assert_no_local_git_routes(&b);
    b.assert_no_local_tables();
    let (b_origin, b_serving) = serve(&b);
    event(&http,&b_origin,&b,json!({"kind":"reference_transaction","updates":[{"old":"0".repeat(commit.len()),"new":commit,"reference":"refs/heads/satellite"}]})).await;
    // The ordinary post-checkout path must likewise work with only native S3 tags.
    event(
        &http,
        &b_origin,
        &b,
        json!({"kind":"checkout","old":commit,"new":commit,"branch":true}),
    )
    .await;
    let read = b
        .memory("memory_get", json!({"id":memory_id}), 0)
        .await
        .unwrap();
    assert_eq!(read["native"]["memory"]["revision"], 1);
    assert_eq!(read["native"]["memory"]["body"], "Original shared wording");
    let read = task_value(b.task("get", json!({"id":task_id}), false).await);
    assert_eq!(
        read["result"]["task"]["specification"]["title"],
        "Git tagged Task"
    );
    let found = b
        .memory(
            "memory_search",
            json!({"text":"shared","mode":"fts","top_k":8}),
            0,
        )
        .await
        .unwrap();
    assert_eq!(found["native"]["state"], "published");
    assert_eq!(found["native"]["total_hits"], 1);
    assert_eq!(found["native"]["hits"][0]["current"]["id"], memory_id);
    let found = task_value(
        b.task(
            "search",
            json!({"text":"tagged","mode":"fts","filter":{},"page_size":8,"top_k":8}),
            false,
        )
        .await,
    );
    assert_eq!(found["result"]["Published"]["total_hits"], 1);
    assert_eq!(
        found["result"]["Published"]["results"][0]["task"]["id"],
        task_id
    );
    assert!(
        b.memory("memory_get", json!({"id":later_id}), 0)
            .await
            .unwrap()["native"]
            .is_null()
    );
    assert!(task_value(b.task("get", json!({"id":later_task}), false).await)["result"].is_null());
    assert!(
        a.memory("memory_get", json!({"id":later_id}), 0)
            .await
            .unwrap()["native"]
            .is_object()
    );
    assert_no_local_git_routes(&a);
    assert_no_local_git_routes(&b);
    a.assert_no_local_tables();
    b.assert_no_local_tables();
    assert!(broker.state.calls.load(Ordering::SeqCst) >= 2);
    a.server.shutdown().await.unwrap();
    b.server.shutdown().await.unwrap();
    a_serving.await.unwrap().unwrap();
    b_serving.await.unwrap().unwrap();
}
