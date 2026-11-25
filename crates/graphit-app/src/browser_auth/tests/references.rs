//! Actual signed browser boundary over persisted native references, without response mocks.
use super::*;
use crate::task_server::{
    TaskServer,
    tests::{browser_selection, browser_workspace_fixture, reopen_browser, seed_browser},
};
use graphit_hub::native::ContentLimits;

fn content() -> ContentLimits {
    ContentLimits {
        max_input_bytes: 262144,
        max_output_bytes: 262144,
        max_depth: 32,
        max_nodes: 4096,
        max_scalar_bytes: 4096,
        max_collection_entries: 256,
    }
}
fn task_input(key: &str, session: &Value, title: &str) -> Value {
    json!({"idempotency_key":key,"session_id":session,"title":title,"description":"Native reference fixture","task_type":"TASK","priority":2,"checks":[{"kind":"acceptance","text":"Native references"},{"kind":"test","text":"Native boundary"}]})
}
async fn seed(server: &TaskServer, raw: &str) -> (Value, Value, Value) {
    let session=seed_browser(server,raw,"create_session",json!({"idempotency_key":"refs-session","title":"References session","description":"Actual native references","strategy":"bounded"})).await;
    let target = seed_browser(
        server,
        raw,
        "create_task",
        task_input("refs-target", &session["id"], "Observed target"),
    )
    .await;
    let mut authored = Vec::new();
    for n in 0..24 {
        authored.push(json!({"target":{"type":"task","id":target["specification"]["id"]},"relation":format!("reference-{n}"),"field":"evidence"}));
    }
    authored.extend([
        json!({"target":{"type":"memory","id":"literal-memory"},"relation":"supports"}),
        json!({"target":{"type":"knowledge","id":"Guide","context":"docs@native"},"relation":"uses_api"}),
        json!({"target":{"type":"memory","id":"user-memory","scope":"user","scope_id":"unavailable-user"},"relation":"relates_to"}),
        json!({"target":{"type":"task","id":target["specification"]["id"],"scope":"project","scope_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"},"relation":"foreign"}),
        json!({"target":{"type":"task","id":target["specification"]["id"]},"relation":"duplicate"}),
        json!({"target":{"type":"task","id":target["specification"]["id"]},"relation":"duplicate"}),
    ]);
    let mut input = task_input("refs-focus", &session["id"], "Reference focus");
    input["depends_on"] = json!([target["specification"]["id"]]);
    input["references"] = json!(authored);
    let focus = seed_browser(server, raw, "create_task", input).await;
    let mut child = task_input("refs-child", &session["id"], "Incoming child");
    child["depends_on"] = json!([focus["specification"]["id"]]);
    seed_browser(server, raw, "create_task", child).await;
    let claim = seed_browser(
        server,
        raw,
        "claim_session",
        json!({"id":session["id"],"lease_ms":60000}),
    )
    .await;
    seed_browser(server,raw,"checkpoint_session",json!({"target":{"id":session["id"],"claim":{"token":claim["token"],"expected_revision":claim["record"]["revision"]}},"lease_ms":60000,"summary":"Native incoming relation","next_step":"Read actual references","references":[{"target":{"type":"task","id":focus["specification"]["id"]},"relation":"tracks"}]})).await;
    (
        session["id"].clone(),
        target["specification"]["id"].clone(),
        focus["specification"]["id"].clone(),
    )
}
fn request(selection: &Value, kind: &str, id: &Value) -> Value {
    json!({"selection":selection,"input":{"kind":kind,"id":id}})
}
fn published(value: &Value) -> &Value {
    &value["native"]["published"]
}

#[tokio::test]
async fn reopened_native_pages_preserve_same_pin_identity_coverage_and_observed_navigation() {
    let (_temp, tasks, raw) = browser_workspace_fixture(Some(content()), Duration::from_secs(10));
    let (session, target, focus) = seed(&tasks, &raw).await;
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let selection = browser_selection(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let response = fixture
        .task_read(&cookie, "references", request(&selection, "task", &focus))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let first: Value = response.json().await.unwrap();
    // Capture preserves its native owner root, including its final separator.
    // Compare native paths, then use the admitted tuple on the next request.
    let requested: graphit_core::native_path::NativePath =
        serde_json::from_value(selection["project_dir"].clone()).unwrap();
    let admitted: graphit_core::native_path::NativePath =
        serde_json::from_value(first["selection"]["project_dir"].clone()).unwrap();
    assert_eq!(admitted.as_path(), requested.as_path());
    assert_eq!(first["selection"]["project_id"], selection["project_id"]);
    assert_eq!(
        first["selection"]["project_version"],
        selection["project_version"]
    );
    let current = published(&first);
    assert_eq!(current["outgoing"]["results"].as_array().unwrap().len(), 20);
    assert_eq!(current["outgoing"]["total"], 32);
    assert_eq!(current["incoming"]["results"].as_array().unwrap().len(), 2);
    assert_eq!(current["outgoing"]["complete"], true);
    assert_eq!(
        current["coverage"]["source_kinds"],
        json!(["task", "session"])
    );
    assert_eq!(
        current["coverage"]["inbound_other_sources_not_qualified"],
        true
    );
    for direction in ["outgoing", "incoming"] {
        for edge in current[direction]["results"].as_array().unwrap() {
            assert_eq!(edge["source_snapshot"], current["generation"]);
        }
        assert_eq!(
            first["navigation"][direction].as_array().unwrap().len(),
            current[direction]["results"].as_array().unwrap().len()
        );
    }
    let mut next = request(&first["selection"], "task", &focus);
    next["input"]["outgoing_cursor"] = current["outgoing"]["next_cursor"].clone();
    next["input"]["page_size"] = json!(100);
    let response = fixture.task_read(&cookie, "references", next).await;
    assert_eq!(response.status(), StatusCode::OK);
    let second: Value = response.json().await.unwrap();
    let tail = published(&second);
    assert_eq!(tail["generation"], current["generation"]);
    assert_eq!(tail["outgoing"]["results"].as_array().unwrap().len(), 12);
    assert_eq!(tail["outgoing"]["next_cursor"], Value::Null);
    for (edge, nav) in tail["outgoing"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .zip(second["navigation"]["outgoing"].as_array().unwrap())
    {
        if edge["target_resolution"] == json!("unresolved") {
            assert_eq!(nav["target_href"], Value::Null);
        } else {
            assert_eq!(
                nav["target_href"],
                format!("/task/explorer/{}", target.as_str().unwrap())
            );
        }
        assert_eq!(
            nav["source_href"],
            format!("/task/explorer/{}", focus.as_str().unwrap())
        );
    }
    assert_eq!(
        tail["outgoing"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["reference"]["relation"] == "duplicate")
            .count(),
        2
    );
    let session_read = fixture
        .task_read(
            &cookie,
            "references",
            request(&selection, "session", &session),
        )
        .await;
    assert_eq!(session_read.status(), StatusCode::OK);
    let session_read: Value = session_read.json().await.unwrap();
    assert_eq!(
        published(&session_read)["outgoing"]["results"][0]["reference"]["id"],
        focus
    );
    assert_eq!(
        session_read["navigation"]["outgoing"][0]["source_href"],
        format!("/task/sessions/{}", session.as_str().unwrap())
    );
    let rendered = first.to_string();
    assert!(
        !rendered.contains("claim_token")
            && !rendered.contains("access_token")
            && !rendered.contains("Bearer")
    );
    assert!(!rendered.contains(&cookie));
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn strict_shapes_methods_auth_and_exact_context_cursors_reject_without_project_io() {
    let (_temp, tasks, raw) = browser_workspace_fixture(Some(content()), Duration::from_secs(10));
    let (_, _, focus) = seed(&tasks, &raw).await;
    let selection = browser_selection(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let before = fixture.issuer.project_calls.load(Ordering::SeqCst);
    for patch in [
        json!({"page_size":null}),
        json!({"page_size":0}),
        json!({"page_size":101}),
        json!({"kind":"memory"}),
        json!({"id":"wrong"}),
        json!({"unknown":true}),
    ] {
        let mut value = request(&selection, "task", &focus);
        value["input"]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert_eq!(
            fixture
                .task_read(&cookie, "references", value)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    for patch in [
        json!({"project_version":"unretained-version"}),
        json!({"project_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}),
        json!({"project_dir":graphit_core::native_path::NativePath::new(
            std::env::temp_dir().join("references-unretained-root")).unwrap()}),
    ] {
        let mut value = request(&selection, "task", &focus);
        value["selection"]
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let unavailable = fixture.task_read(&cookie, "references", value).await;
        assert_eq!(unavailable.status(), StatusCode::NOT_FOUND);
        let unavailable: Value = unavailable.json().await.unwrap();
        assert_eq!(unavailable["error"]["code"], "selection_unavailable");
        assert!(unavailable.get("native").is_none());
    }
    let mut missing = request(&selection, "task", &focus);
    missing["selection"]
        .as_object_mut()
        .unwrap()
        .remove("project_dir");
    assert_eq!(
        fixture
            .task_read(&cookie, "references", missing)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let duplicate = format!(
        r#"{{"selection":{},"input":{{"kind":"task","kind":"session","id":{}}}}}"#,
        selection, focus
    );
    let duplicate = fixture
        .client
        .post(format!("{}/api/tasks/references", fixture.origin))
        .header(header::ORIGIN, &fixture.origin)
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(duplicate)
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), before);
    assert_eq!(
        fixture
            .client
            .get(format!("{}/api/tasks/references", fixture.origin))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    let anonymous = fixture
        .client
        .post(format!("{}/api/tasks/references", fixture.origin))
        .header(header::ORIGIN, &fixture.origin)
        .json(&request(&selection, "task", &focus))
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let wrong_origin = fixture
        .client
        .post(format!("{}/api/tasks/references", fixture.origin))
        .header(header::ORIGIN, "http://wrong.invalid")
        .header(header::COOKIE, &cookie)
        .json(&request(&selection, "task", &focus))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);
    let mut first = request(&selection, "task", &focus);
    first["input"]["page_size"] = json!(1);
    let first: Value = fixture
        .task_read(&cookie, "references", first)
        .await
        .json()
        .await
        .unwrap();
    let cursor = published(&first)["outgoing"]["next_cursor"].clone();
    let missing_id = json!(graphit_records::task_spec::TaskId::new(
        std::num::NonZeroU128::new(u128::MAX).unwrap()
    ));
    let missing = fixture
        .task_read(
            &cookie,
            "references",
            request(&selection, "task", &missing_id),
        )
        .await;
    assert!(missing.status().is_server_error());
    let missing: Value = missing.json().await.unwrap();
    assert!(missing.get("error").is_some());
    assert!(missing.get("native").is_none());
    let contexts: Value = fixture
        .task_read(&cookie, "contexts", json!({"selection":{}}))
        .await
        .json()
        .await
        .unwrap();
    let mirror = contexts["contexts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["project_id"] == selection["project_id"]
                && c["project_dir"] != selection["project_dir"]
        })
        .unwrap();
    let mut foreign = request(mirror, "task", &focus);
    foreign["input"]["outgoing_cursor"] = cursor.clone();
    assert_eq!(
        fixture
            .task_read(&cookie, "references", foreign)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let shared: Value = fixture
        .task_read(&cookie, "references", request(mirror, "task", &focus))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        published(&shared)["focus_revision"],
        published(&first)["focus_revision"]
    );
    assert_eq!(
        published(&shared)["outgoing"]["results"][0],
        published(&first)["outgoing"]["results"][0]
    );
    let mut swapped = request(&selection, "task", &focus);
    swapped["input"]["incoming_cursor"] = cursor;
    assert_eq!(
        fixture
            .task_read(&cookie, "references", swapped)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    fixture.issuer.project_grant.store(false, Ordering::SeqCst);
    let denied = fixture
        .task_read(&cookie, "references", request(&selection, "task", &focus))
        .await;
    assert!(!denied.status().is_success());
    let denied: Value = denied.json().await.unwrap();
    assert!(denied.get("native").is_none());
    fixture.issuer.project_grant.store(true, Ordering::SeqCst);
    let (tasks, raw) = fixture.tasks.as_ref().unwrap();
    crate::task_server::tests::browser_provider(tasks, raw, &fixture.issuer.origin, true);
    let before = fixture.issuer.project_calls.load(Ordering::SeqCst);
    let removed = fixture
        .task_read(&cookie, "references", request(&selection, "task", &focus))
        .await;
    assert_eq!(removed.status(), StatusCode::FORBIDDEN);
    assert!(
        removed
            .json::<Value>()
            .await
            .unwrap()
            .get("native")
            .is_none()
    );
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), before);
    fixture.finish_tasks().await;
    let absent = Fixture::new(policy()).await;
    assert_eq!(
        absent
            .client
            .post(format!("{}/api/tasks/references", absent.origin))
            .json(&request(&selection, "task", &focus))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    absent.service.shutdown().await.unwrap();
}

#[tokio::test]
async fn browser_shutdown_cancels_held_project_reference_read_without_healthy_prefix() {
    let (_temp, tasks, raw) = browser_workspace_fixture(Some(content()), Duration::from_secs(10));
    let (_, _, focus) = seed(&tasks, &raw).await;
    let selection = browser_selection(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    fixture.issuer.hold_project.store(true, Ordering::SeqCst);
    let client = fixture.client.clone();
    let origin = fixture.origin.clone();
    let body = request(&selection, "task", &focus);
    let running = tokio::spawn(async move {
        client
            .post(format!("{origin}/api/tasks/references"))
            .header(header::ORIGIN, &origin)
            .header(header::COOKIE, &cookie)
            .json(&body)
            .send()
            .await
    });
    tokio::time::timeout(
        Duration::from_secs(5),
        fixture.issuer.project_started.notified(),
    )
    .await
    .unwrap();
    fixture.service.shutdown().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .unwrap()
        .unwrap();
    if let Ok(response) = result {
        assert!(!response.status().is_success());
    }
    assert!(fixture.service.0.jobs.lock().await.is_empty());
    fixture.issuer.release_project.notify_one();
    fixture.tasks.as_ref().unwrap().0.shutdown().await.unwrap();
}

#[tokio::test]
async fn socket_disconnect_keeps_reference_work_owned_until_join_and_next_read_succeeds() {
    use tokio::io::AsyncWriteExt;
    let (_temp, tasks, raw) = browser_workspace_fixture(Some(content()), Duration::from_secs(10));
    let (_, _, focus) = seed(&tasks, &raw).await;
    let selection = browser_selection(&tasks);
    tasks.shutdown().await.unwrap();
    let tasks = tasks.references_test_policy(content(), Duration::from_secs(2));
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    fixture.issuer.hold_project.store(true, Ordering::Release);
    let url = reqwest::Url::parse(&fixture.origin).unwrap();
    let mut socket = tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
        .await
        .unwrap();
    let body = request(&selection, "task", &focus).to_string();
    let wire = format!(
        "POST /api/tasks/references HTTP/1.1\r\nHost: {}:{}\r\nOrigin: {}\r\nCookie: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        url.host_str().unwrap(),
        url.port().unwrap(),
        fixture.origin,
        cookie,
        body.len(),
        body
    );
    socket.write_all(wire.as_bytes()).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(5),
        fixture.issuer.project_started.notified(),
    )
    .await
    .unwrap();
    drop(socket);
    // An accepted Hyper handler can survive EOF until its finite Original expires.
    {
        let mut jobs = fixture.service.0.jobs.lock().await;
        tokio::time::timeout(Duration::from_secs(5), jobs.join_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(jobs.len(), 1); // Independent authenticated-session expiry timer.
    }
    fixture.issuer.hold_project.store(false, Ordering::Release);
    fixture.issuer.release_project.notify_waiters();
    let next = fixture
        .task_read(&cookie, "references", request(&selection, "task", &focus))
        .await;
    assert_eq!(next.status(), StatusCode::OK);
    assert!(
        next.json::<Value>().await.unwrap()["native"]
            .get("published")
            .is_some()
    );
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn project_decision_revoked_during_held_reference_read_never_returns_native_rows() {
    let (_temp, tasks, raw) = browser_workspace_fixture(Some(content()), Duration::from_secs(10));
    let (_, _, focus) = seed(&tasks, &raw).await;
    let selection = browser_selection(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    fixture.issuer.hold_project.store(true, Ordering::Release);
    let pending = fixture.task_read(&cookie, "references", request(&selection, "task", &focus));
    let revoke = async {
        tokio::time::timeout(
            Duration::from_secs(5),
            fixture.issuer.project_started.notified(),
        )
        .await
        .unwrap();
        fixture.issuer.project_grant.store(false, Ordering::Release);
        fixture.issuer.hold_project.store(false, Ordering::Release);
        fixture.issuer.release_project.notify_one();
    };
    let (response, ()) = tokio::join!(pending, revoke);
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let rejected: Value = response.json().await.unwrap();
    assert_eq!(rejected["error"]["code"], "project_access_denied");
    assert!(rejected.get("native").is_none());
    fixture.issuer.project_grant.store(true, Ordering::Release);
    assert_eq!(
        fixture
            .task_read(&cookie, "references", request(&selection, "task", &focus))
            .await
            .status(),
        StatusCode::OK
    );
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn empty_clear_changes_generation_and_rejects_previous_cursor_without_resurrection() {
    let (_temp, tasks, raw) = browser_workspace_fixture(Some(content()), Duration::from_secs(10));
    let (session, _, focus) = seed(&tasks, &raw).await;
    let selection = browser_selection(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let mut first = request(&selection, "task", &focus);
    first["input"]["page_size"] = json!(1);
    let first: Value = fixture
        .task_read(&cookie, "references", first)
        .await
        .json()
        .await
        .unwrap();
    let (tasks, raw) = fixture.tasks.as_ref().unwrap();
    let claim = seed_browser(
        tasks,
        raw,
        "claim_session",
        json!({"id":session,"lease_ms":60000}),
    )
    .await;
    seed_browser(tasks,raw,"checkpoint_session",json!({"target":{"id":session,"claim":{"token":claim["token"],"expected_revision":claim["record"]["revision"]}},"lease_ms":60000,"summary":"Clear authored relation","next_step":"Confirm current empty marker","references":[]})).await;
    let response = fixture
        .task_read(
            &cookie,
            "references",
            request(&selection, "session", &session),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let empty: Value = response.json().await.unwrap();
    assert_eq!(published(&empty)["outgoing"]["results"], json!([]));
    assert_eq!(published(&empty)["outgoing"]["complete"], true);
    assert_ne!(
        published(&empty)["generation"],
        published(&first)["generation"]
    );
    let mut stale = request(&selection, "task", &focus);
    stale["input"]["outgoing_cursor"] = published(&first)["outgoing"]["next_cursor"].clone();
    let stale = fixture.task_read(&cookie, "references", stale).await;
    assert_eq!(stale.status(), StatusCode::BAD_REQUEST);
    let stale: Value = stale.json().await.unwrap();
    assert_eq!(stale["error"]["code"], "invalid_cursor");
    assert!(stale.get("native").is_none());
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn response_and_request_byte_limits_return_errors_without_native_prefix() {
    let (_temp, tasks, raw) = browser_workspace_fixture(Some(content()), Duration::from_secs(10));
    let (_, _, focus) = seed(&tasks, &raw).await;
    let selection = browser_selection(&tasks);
    tasks.shutdown().await.unwrap();
    let mut constrained = content();
    constrained.max_output_bytes = 1024;
    let tasks = tasks.references_test_policy(constrained, Duration::from_secs(10));
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let response = fixture
        .task_read(&cookie, "references", request(&selection, "task", &focus))
        .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let value: Value = response.json().await.unwrap();
    assert!(value.get("native").is_none());
    assert_eq!(value["error"]["code"], "response_limit");
    let before = fixture.issuer.project_calls.load(Ordering::SeqCst);
    let oversized = fixture
        .client
        .post(format!("{}/api/tasks/references", fixture.origin))
        .header(header::ORIGIN, &fixture.origin)
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(" ".repeat(constrained.max_input_bytes + 1))
        .send()
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), before);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn absolute_deadline_ends_held_project_read_and_next_request_stays_valid() {
    let (_temp, tasks, raw) = browser_workspace_fixture(Some(content()), Duration::from_secs(10));
    let (_, _, focus) = seed(&tasks, &raw).await;
    let selection = browser_selection(&tasks);
    tasks.shutdown().await.unwrap();
    let tasks = tasks.references_test_policy(content(), Duration::from_secs(2));
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    fixture.issuer.hold_project.store(true, Ordering::SeqCst);
    let response = fixture
        .task_read(&cookie, "references", request(&selection, "task", &focus))
        .await;
    assert!(!response.status().is_success());
    let value: Value = response.json().await.unwrap();
    assert!(value.get("native").is_none());
    fixture.issuer.hold_project.store(false, Ordering::SeqCst);
    fixture.issuer.release_project.notify_one();
    let next = fixture
        .task_read(&cookie, "references", request(&selection, "task", &focus))
        .await;
    assert_eq!(next.status(), StatusCode::OK);
    fixture.finish_tasks().await;
}
