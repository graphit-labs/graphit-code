use super::*;
use crate::task_server::tests::{
    browser_selection, browser_workspace_fixture, reopen_browser, seed_browser,
};

#[tokio::test]
async fn contexts_admit_each_retained_native_scope_and_echo_exact_path_and_current() {
    let (_temp, tasks, raw) = browser_workspace_fixture(None, Duration::from_secs(5));
    let snapshot = tasks
        .record_snapshot()
        .unwrap_or_else(|_| panic!("live metadata fixture"));
    let cancellation = graphit_core::file_lock::Cancellation::default();
    for (index, (paths, id, _)) in snapshot.projects.iter().enumerate() {
        let name = if index == 1 {
            "display-sibling"
        } else {
            "display-project"
        };
        graphit_core::project_identity::ProjectIdentityStore::at(paths)
            .writer(Duration::from_secs(5), &cancellation)
            .unwrap()
            .update(
                *id,
                &graphit_core::project_identity::ProjectMetadata::new(name).unwrap(),
            )
            .unwrap();
    }
    let session = seed_browser(&tasks, &raw, "create_session", json!({"idempotency_key":"workspace-session","title":"Retained workspace","description":"Native selector","strategy":"native"})).await;
    let task = seed_browser(&tasks, &raw, "create_task", json!({"idempotency_key":"workspace-task","session_id":session["id"],"title":"Native calendar","description":"Persisted selection","task_type":"TASK","priority":1,"schedule":{"start_date":"2024-02-28","end_date":"2024-03-01"},"checks":[{"kind":"acceptance","text":"Select exact path"},{"kind":"test","text":"Read the calendar"}]})).await;
    let expected = browser_selection(&tasks);
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    fixture.issuer.profile.store(false, Ordering::Release);
    *fixture.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let cookie = fixture.login().await;
    let response = fixture.task_read(&cookie, "contexts", json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body = response.text().await.unwrap();
    assert!(!body.contains("access_token") && !body.contains("claim_token"));
    let contexts: Value = serde_json::from_str(&body).unwrap();
    let rows = contexts["contexts"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(contexts["current"], expected);
    assert_eq!(rows[0]["project_id"], rows[2]["project_id"]);
    assert_eq!(rows[0]["project_version"], rows[2]["project_version"]);
    assert_ne!(rows[0]["project_dir"], rows[2]["project_dir"]);
    assert_ne!(rows[0]["project_id"], rows[1]["project_id"]);
    let projects = contexts["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2);
    assert!(
        projects.contains(&json!({"project_id":rows[0]["project_id"],"name":"display-project"}))
    );
    assert!(
        projects.contains(&json!({"project_id":rows[1]["project_id"],"name":"display-sibling"}))
    );
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), 3);
    for row in rows {
        assert_eq!(row.as_object().unwrap().len(), 3);
        let response = fixture
            .task_read(&cookie, "context", json!({"selection":row}))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.json::<Value>().await.unwrap(), *row);
    }
    let ambiguous = fixture.task_read(&cookie, "context", json!({"selection":{"project_id":rows[0]["project_id"],"project_version":rows[0]["project_version"]}})).await;
    assert_eq!(ambiguous.status(), StatusCode::BAD_REQUEST);
    let catalogue = fixture
        .task_read(
            &cookie,
            "catalogue",
            json!({"selection":contexts["current"],"input":{"filter":{},"page_size":20}}),
        )
        .await;
    assert_eq!(catalogue.status(), StatusCode::OK);
    let catalogue = catalogue.json::<Value>().await.unwrap();
    assert_eq!(catalogue["results"][0]["id"], task["specification"]["id"]);
    assert_eq!(
        catalogue["results"][0]["schedule"]["start_date"],
        "2024-02-28"
    );
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn second_project_denial_returns_no_partial_contexts_and_sibling_remains_readable() {
    let (_temp, tasks, raw) = browser_workspace_fixture(None, Duration::from_secs(5));
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    fixture.issuer.hold_project.store(true, Ordering::Release);
    let deny_second = async {
        fixture.issuer.project_started.notified().await;
        fixture.issuer.release_project.notify_one();
        fixture.issuer.project_started.notified().await;
        fixture.issuer.project_grant.store(false, Ordering::Release);
        fixture.issuer.release_project.notify_one();
    };
    let (response, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            fixture.task_read(&cookie, "contexts", json!({})),
            deny_second
        )
    })
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let error = response.json::<Value>().await.unwrap();
    assert_eq!(error["error"]["code"], "project_access_denied");
    assert!(error.get("contexts").is_none() && error.get("current").is_none());
    assert!(error.get("projects").is_none());
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), 2);
    fixture.issuer.hold_project.store(false, Ordering::Release);
    fixture.issuer.project_grant.store(true, Ordering::Release);
    let sibling = fixture.task_read(&cookie, "context", json!({})).await;
    assert_eq!(sibling.status(), StatusCode::OK);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn contexts_reject_input_origin_and_missing_cookie_before_project_work() {
    let (_temp, tasks, raw) = browser_workspace_fixture(None, Duration::from_secs(5));
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    for arguments in [
        json!({"input":{}}),
        json!({"selection":{"project_version":"main"}}),
        json!({"unknown":true}),
    ] {
        assert_eq!(
            fixture
                .task_read(&cookie, "contexts", arguments)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        fixture.task_read("", "contexts", json!({})).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let response = fixture
        .client
        .post(format!("{}/api/tasks/contexts", fixture.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, "https://foreign.example")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), 0);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn contexts_enforce_collection_and_body_response_limits() {
    for (output, entries) in [(65536, 2), (512, 64)] {
        let content = graphit_hub::native::ContentLimits {
            max_input_bytes: 65536,
            max_output_bytes: output,
            max_depth: 32,
            max_nodes: 4096,
            max_scalar_bytes: 4096,
            max_collection_entries: entries,
        };
        let (_temp, tasks, raw) = browser_workspace_fixture(Some(content), Duration::from_secs(5));
        let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
        let cookie = fixture.login().await;
        let response = fixture.task_read(&cookie, "contexts", json!({})).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let error = response.json::<Value>().await.unwrap();
        assert_eq!(error["error"]["code"], "response_limit");
        assert!(error.get("contexts").is_none());
        assert!(error.get("projects").is_none());
        assert_eq!(
            fixture.issuer.project_calls.load(Ordering::SeqCst),
            if entries == 2 { 0 } else { 3 }
        );
        fixture.finish_tasks().await;
    }
}

#[tokio::test]
async fn contexts_deadline_and_stop_drain_owned_jobs_before_session_cleanup() {
    for stop in [false, true] {
        let (_temp, tasks, raw) = browser_workspace_fixture(None, Duration::from_secs(1));
        let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
        let cookie = fixture.login().await;
        fixture.issuer.hold_project.store(true, Ordering::Release);
        let control = async {
            fixture.issuer.project_started.notified().await;
            if stop {
                fixture.service.shutdown().await.unwrap();
            }
        };
        let (response, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(fixture.task_read(&cookie, "contexts", json!({})), control)
        })
        .await
        .unwrap();
        assert!(!response.status().is_success());
        let error = response.json::<Value>().await.unwrap();
        assert!(error.get("contexts").is_none());
        assert!(error.get("projects").is_none());
        // Remote handler is deliberately still held; only local request ownership is proved.
        fixture.issuer.hold_project.store(false, Ordering::Release);
        fixture.issuer.release_project.notify_one();
        if !stop {
            let response = fixture.task_read(&cookie, "context", json!({})).await;
            assert_eq!(response.status(), StatusCode::OK);
        }
        fixture.finish_tasks().await;
    }
}
