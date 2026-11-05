//! Signed OAuth/PROJECT HTTP reads over reopened native Memory data.
use super::*;
use crate::task_server::{
    browser::memory_fixture::{self, Seed},
    tests::{memory_workspace_fixture, reopen_browser},
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{CreateMemory, DeleteMemory, MemoryId, UpdateMemory},
};

fn input(key: &str, mandatory: bool, important: bool) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: key.into(),
        body: format!("Body {key}"),
        attributes: MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory,
            important,
        },
        tags: vec!["literal".into()],
        references: None,
    }
}
async fn read(f: &Fixture, cookie: &str, operation: &str, body: Value) -> reqwest::Response {
    f.client
        .post(format!("{}/api/memory/{operation}", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .json(&body)
        .send()
        .await
        .unwrap()
}
fn body(selection: &Value, input: Value) -> Value {
    json!({"selection":selection,"input":input})
}
fn update() -> UpdateMemory {
    UpdateMemory {
        expected_revision: 1,
        idempotency_key: "update".into(),
        reason: "Revise current body".into(),
        title: None,
        body: Some("Updated body".into()),
        classification: Some(MemoryClass::Decision),
        mandatory: None,
        important: None,
        tags: None,
        references: None,
    }
}

#[tokio::test]
async fn web_crud_receipts_cas_idempotency_and_separate_read_history() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let created = input("web-create", false, false);
    let response = read(
        &f,
        &cookie,
        "create",
        body(&selection, serde_json::to_value(&created).unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Value = response.json().await.unwrap();
    assert_eq!(receipt["selection"], selection);
    assert_eq!(receipt["native"]["state"], "completed");
    assert_eq!(receipt["native"]["revision"], 1);
    assert!(receipt["native"].get("memory").is_none());
    assert!(receipt["native"].get("body").is_none());
    let id = receipt["native"]["id"].clone();
    let replay: Value = read(
        &f,
        &cookie,
        "create",
        body(&selection, serde_json::to_value(&created).unwrap()),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(replay["native"], receipt["native"]);
    let mut conflict = serde_json::to_value(&created).unwrap();
    conflict["body"] = json!("changed same key");
    let response = read(&f, &cookie, "create", body(&selection, conflict)).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        "idempotency_conflict"
    );
    let detail: Value = read(&f, &cookie, "get", body(&selection, json!({"id":id})))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(detail["native"]["memory"]["body"], created.body);
    let actor = detail["native"]["creation"]["actor"].as_str().unwrap();
    assert!(!actor.is_empty());
    assert_eq!(detail["native"]["memory"]["created_by"], actor);
    let mut change = serde_json::to_value(update()).unwrap();
    change["id"] = id.clone();
    let response = read(&f, &cookie, "update", body(&selection, change.clone())).await;
    assert_eq!(response.status(), StatusCode::OK);
    let changed: Value = response.json().await.unwrap();
    assert_eq!(changed["native"]["revision"], 2);
    assert!(changed["native"].get("memory").is_none());
    let replay: Value = read(&f, &cookie, "update", body(&selection, change))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(replay["native"], changed["native"]);
    let stale = json!({"id":id,"expected_revision":9007199254740993u64,"idempotency_key":"stale","reason":"CAS","body":"new"});
    let response = read(&f, &cookie, "update", body(&selection, stale)).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let literal = response.text().await.unwrap();
    assert!(literal.contains("9007199254740993"));
    let error: Value = serde_json::from_str(&literal).unwrap();
    assert_eq!(error["error"]["expected"].as_u64(), Some(9007199254740993));
    assert_eq!(error["error"]["actual"], 2);
    let history: Value = read(&f, &cookie, "history", body(&selection, json!({"id":id})))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(history["native"]["revisions"].as_array().unwrap().len(), 1);
    assert_eq!(history["native"]["revisions"][0]["actor"], actor);
    let response = read(
        &f,
        &cookie,
        "delete",
        body(
            &selection,
            json!({"id":id,"expected_revision":2,"idempotency_key":"web-delete","reason":"Delete"}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["native"]["revision"],
        3
    );
    let gone: Value = read(&f, &cookie, "get", body(&selection, json!({"id":id})))
        .await
        .json()
        .await
        .unwrap();
    assert!(gone["native"].is_null());
    let history: Value = read(&f, &cookie, "history", body(&selection, json!({"id":id})))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(history["native"]["deleted"], true);
    assert_eq!(history["native"]["revisions"].as_array().unwrap().len(), 2);
    f.finish_tasks().await;
}

#[tokio::test]
async fn web_write_denial_preserves_read_and_revoked_grant_denies_new_write() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.project_write_grant.store(false, Ordering::Release);
    let response = read(
        &f,
        &cookie,
        "create",
        body(
            &selection,
            serde_json::to_value(input("denied", false, false)).unwrap(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let list: Value = read(&f, &cookie, "list", body(&selection, json!({})))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(list["native"]["state"], "no_corpus");
    f.issuer.project_write_grant.store(true, Ordering::Release);
    let response = read(
        &f,
        &cookie,
        "create",
        body(
            &selection,
            serde_json::to_value(input("allowed", false, false)).unwrap(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Value = response.json().await.unwrap();
    f.issuer.project_grant.store(false, Ordering::Release);
    let response=read(&f,&cookie,"update",body(&selection,json!({"id":receipt["native"]["id"],
        "expected_revision":1,"idempotency_key":"removed-grant","reason":"Denied","body":"attempt"}))).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    f.finish_tasks().await;
}

#[tokio::test]
async fn native_web_search_fts_semantic_hybrid_share_sdk_generation_with_model_peer() {
    use crate::foreground_source::foreground_memory_jobs_tests::{
        browser_search_fixture_configured, stop_browser_model,
    };

    let (issuer, issuer_service) = Fixture::start_issuer().await;
    let (mut fixture, mut foreground, tasks, raw, memories) =
        browser_search_fixture_configured(Some(&issuer.origin)).await;
    let selection = memory_fixture::selection(&tasks, 0);
    // Startup captures the loopback policy before record storage constructs its verifier.
    // The Browser fixture still registers its own current provider through the usual path.
    crate::task_server::tests::browser_provider(&tasks, &raw, &issuer.origin, true);
    let f = Fixture::configured_with_issuer(
        policy(),
        Some((tasks, raw)),
        None,
        None,
        issuer,
        issuer_service,
    )
    .await;
    let cookie = f.login().await;
    let userinfo = f.issuer.userinfo.load(Ordering::SeqCst);
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let mut generation = None;
    for mode in ["fts", "semantic", "hybrid"] {
        let response = read(
            &f,
            &cookie,
            "search",
            body(
                &selection,
                json!({"text":"Paris","top_k":4,"mode":mode,
            "filter":{"classification":"decision","tag":"fixture"}}),
            ),
        )
        .await;
        let status = response.status();
        let value: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{mode}: {value}");
        assert_eq!(value["selection"], selection);
        assert_eq!(value["native"]["mode"], mode);
        assert_eq!(value["native"]["state"], "published");
        assert_eq!(value["native"]["total_hits"], 1);
        assert_eq!(
            value["native"]["hits"][0]["current"]["id"],
            json!(memories[0].id)
        );
        assert!(
            value["native"]["generation"]["sdk_version"]
                .as_u64()
                .unwrap()
                > 0
        );
        let observed = &value["native"]["generation"];
        if let Some(expected) = &generation {
            assert_eq!(observed, expected, "{mode} must read the same publication");
        } else {
            generation = Some(observed.clone());
        }
        if mode == "fts" {
            assert!(value["native"]["model"].is_null());
            assert!(value["native"]["hits"][0]["score"].is_number());
        } else {
            assert!(value["native"]["model"]["dimensions"].as_u64().unwrap() > 0);
            assert_eq!(
                value["native"]["model"]["vector_id"]
                    .as_array()
                    .unwrap()
                    .len(),
                32
            );
        }
        if mode == "semantic" {
            assert!(value["native"]["hits"][0]["distance"].is_number());
        }
        if mode == "hybrid" {
            assert!(value["native"]["hits"][0]["score"].is_number());
            assert!(value["native"]["hits"][0]["lexical"]["score"].is_number());
            assert!(value["native"]["hits"][0]["semantic"]["distance"].is_number());
        }
    }
    f.tasks
        .as_ref()
        .unwrap()
        .0
        .attach_memory_model(None)
        .unwrap();
    for mode in ["semantic", "hybrid"] {
        let response = read(
            &f,
            &cookie,
            "search",
            body(&selection, json!({"text":"Paris","top_k":4,"mode":mode})),
        )
        .await;
        let status = response.status();
        let value: Value = response.json().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{mode}: {value}");
        assert_eq!(value["native"]["state"], "unavailable");
        assert_eq!(value["native"]["reason"], "model_unavailable");
        assert!(value["native"].get("hits").is_none());
    }
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), userinfo);
    f.finish_tasks().await;
    stop_browser_model(&mut fixture, &mut foreground).await;
    foreground.shutdown().await.unwrap();
}

#[tokio::test]
async fn reopened_current_creation_full_history_filters_and_deleted_are_canonical() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let ordinary = memory_fixture::seed(
        &tasks,
        &raw,
        0,
        Seed::Create(input("ordinary", false, false)),
    )
    .await;
    let important = memory_fixture::seed(
        &tasks,
        &raw,
        0,
        Seed::Create(input("important", false, true)),
    )
    .await;
    let mandatory = memory_fixture::seed(
        &tasks,
        &raw,
        0,
        Seed::Create(input("mandatory", true, false)),
    )
    .await;
    let changed = memory_fixture::seed(&tasks, &raw, 0, Seed::Update(ordinary.id, update())).await;
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let selection = memory_fixture::selection(&tasks, 0);
    let other_project = memory_fixture::selection(&tasks, 1);
    let mirror = memory_fixture::selection(&tasks, 2);
    assert_eq!(mirror["project_id"], selection["project_id"]);
    assert_eq!(mirror["project_version"], selection["project_version"]);
    assert_ne!(mirror["project_dir"], selection["project_dir"]);
    assert_ne!(other_project["project_id"], selection["project_id"]);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let response = read(&f, &cookie, "list", body(&selection, json!({}))).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["selection"], selection);
    let memories = value["native"]["memories"].as_array().unwrap();
    assert_eq!(
        memories.iter().map(|m| m["id"].clone()).collect::<Vec<_>>(),
        vec![json!(mandatory.id), json!(important.id), json!(ordinary.id)]
    );
    let detail: Value = read(
        &f,
        &cookie,
        "get",
        body(&selection, json!({"id":ordinary.id})),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        detail["native"]["memory"],
        serde_json::to_value(&changed).unwrap()
    );
    assert_eq!(
        detail["native"]["creation"]["input"]["idempotency_key"],
        "ordinary"
    );
    assert_eq!(
        detail["native"]["creation"]["memory"],
        serde_json::to_value(&ordinary).unwrap()
    );
    let history: Value = read(
        &f,
        &cookie,
        "history",
        body(&selection, json!({"id":ordinary.id})),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        history["native"]["current"],
        serde_json::to_value(&changed).unwrap()
    );
    assert_eq!(history["native"]["deleted"], false);
    assert_eq!(
        history["native"]["revisions"][0]["operation"]["Update"]["classification"],
        "decision"
    );
    assert_eq!(
        history["native"]["revisions"][0]["before"],
        serde_json::to_value(&ordinary).unwrap()
    );
    for (filter, expected) in [
        (json!({"classification":"decision"}), vec![ordinary.id]),
        (json!({"mandatory":true}), vec![mandatory.id]),
        (json!({"important":true}), vec![important.id]),
        (
            json!({"tag":"literal","exclude_mandatory":true,"important":false}),
            vec![ordinary.id],
        ),
    ] {
        let value: Value = read(&f, &cookie, "list", body(&selection, filter))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(
            value["native"]["memories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| serde_json::from_value::<MemoryId>(m["id"].clone()).unwrap())
                .collect::<Vec<_>>(),
            expected
        );
    }
    let mirrored: Value = read(&f, &cookie, "list", body(&mirror, json!({})))
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(mirrored["selection"], mirror);
    assert_eq!(mirrored["native"], value["native"]);
    let mirrored_history: Value = read(
        &f,
        &cookie,
        "history",
        body(&mirror, json!({"id":ordinary.id})),
    )
    .await
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(mirrored_history["selection"], mirror);
    assert_eq!(mirrored_history["native"], history["native"]);
    let absent: Value = read(&f, &cookie, "list", body(&other_project, json!({})))
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(absent["native"], json!({"state":"no_corpus"}));
    assert_eq!(absent["selection"], other_project);
    let (tasks, raw) = f.tasks.as_ref().unwrap();
    let only = memory_fixture::seed(
        tasks,
        raw,
        1,
        Seed::Create(input("other-project-only", false, false)),
    )
    .await;
    let foreign: Value = read(&f, &cookie, "get", body(&mirror, json!({"id":only.id})))
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(foreign["native"], Value::Null);
    memory_fixture::seed(
        tasks,
        raw,
        1,
        Seed::Delete(
            only.id,
            DeleteMemory {
                expected_revision: 1,
                idempotency_key: "other-project-delete".into(),
                reason: "Empty this corpus".into(),
            },
        ),
    )
    .await;
    let empty: Value = read(&f, &cookie, "list", body(&other_project, json!({})))
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(empty["native"]["state"], "published");
    assert_eq!(empty["selection"], other_project);
    assert_eq!(empty["native"]["memories"], json!([]));
    let (tasks, raw) = f.tasks.as_ref().unwrap();
    memory_fixture::seed(
        tasks,
        raw,
        0,
        Seed::Delete(
            ordinary.id,
            DeleteMemory {
                expected_revision: 2,
                idempotency_key: "delete".into(),
                reason: "Remove selected fact".into(),
            },
        ),
    )
    .await;
    let deleted: Value = read(&f, &cookie, "get", body(&mirror, json!({"id":ordinary.id})))
        .await
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(deleted["native"], Value::Null);
    let history: Value = read(
        &f,
        &cookie,
        "history",
        body(&mirror, json!({"id":ordinary.id})),
    )
    .await
    .error_for_status()
    .unwrap()
    .json()
    .await
    .unwrap();
    assert_eq!(history["selection"], mirror);
    assert_eq!(history["native"]["deleted"], true);
    assert_eq!(history["native"]["revisions"].as_array().unwrap().len(), 2);
    assert!(
        history["native"]["generation"]["sdk_version"]
            .as_u64()
            .unwrap()
            > value["native"]["generation"]["sdk_version"]
                .as_u64()
                .unwrap()
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.finish_tasks().await;
}

#[tokio::test]
async fn strict_duplicate_inputs_and_exact_selection_reject_before_project_io() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    for (op, value) in [
        ("list", json!({"input":{}})),
        ("list", body(&selection, json!({"unknown":true}))),
        ("list", body(&selection, json!({"tag":"  "}))),
        ("get", body(&selection, json!({"id":"invalid"}))),
        ("history", body(&selection, json!({"id":null}))),
        ("list", body(&selection, json!({"exclude_mandatory":null}))),
    ] {
        assert_eq!(
            read(&f, &cookie, op, value).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        read(
            &f,
            &cookie,
            "list",
            body(&selection, json!({"tag":"x".repeat(4097)}))
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        read(
            &f,
            &cookie,
            "list",
            body(&selection, json!({"tag":"x".repeat(65536)}))
        )
        .await
        .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let duplicate = format!(
        "{{\"selection\":{},\"input\":{{\"important\":true,\"important\":false}}}}",
        selection
    );
    assert_eq!(
        f.client
            .post(format!("{}/api/memory/list", f.origin))
            .header(header::ORIGIN, &f.origin)
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(duplicate)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut missing = selection.clone();
    missing["project_version"] = json!("unavailable");
    assert_eq!(
        read(&f, &cookie, "list", body(&missing, json!({})))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn fresh_memory_project_grant_is_separate_from_task_jwt_and_browser_origin() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.project_module.lock().unwrap() = Some("task".into());
    assert_eq!(
        read(&f, &cookie, "list", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        f.task_read(&cookie, "view", json!({})).await.status(),
        StatusCode::OK
    );
    *f.issuer.project_module.lock().unwrap() = None;
    f.issuer.project_grant.store(false, Ordering::Release);
    assert_eq!(
        read(&f, &cookie, "list", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    f.issuer.project_grant.store(true, Ordering::Release);
    assert_eq!(
        read(
            &f,
            "graphit_session=invalid",
            "list",
            body(&selection, json!({}))
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.client
            .post(format!("{}/api/memory/list", f.origin))
            .header(header::ORIGIN, "https://foreign.example")
            .header(header::COOKIE, &cookie)
            .json(&body(&selection, json!({})))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.client
            .get(format!("{}/api/memory/list", f.origin))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn output_limit_does_not_return_healthy_prefix() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    memory_fixture::seed(&tasks, &raw, 0, Seed::Create(input("one", false, false))).await;
    memory_fixture::seed(&tasks, &raw, 0, Seed::Create(input("two", false, false))).await;
    let selection = memory_fixture::selection(&tasks, 0);
    let (mut content, _, _) = tasks.browser_policy();
    content.max_output_bytes = 16;
    let output = tasks.references_test_policy(content, Duration::from_secs(30));
    let f = Fixture::with_tasks(policy(), Some((output, raw.clone()))).await;
    let cookie = f.login().await;
    let response = read(&f, &cookie, "list", body(&selection, json!({}))).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        "response_limit"
    );
}
#[tokio::test]
async fn request_input_bound_does_not_cap_stored_memory_text() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let (mut content, _, _) = tasks.browser_policy();
    let mut large = input("physical-bytes", false, false);
    large.body = "x".repeat(5000);
    let stored = memory_fixture::seed(&tasks, &raw, 0, Seed::Create(large)).await;
    let mut request = body(&selection, json!({}));
    content.max_input_bytes = serde_json::to_vec(&request).unwrap().len() + 16;
    let capped = tasks.references_test_policy(content, Duration::from_secs(30));
    let f = Fixture::with_tasks(policy(), Some((capped, raw))).await;
    let cookie = f.login().await;
    let response = read(&f, &cookie, "list", request.take()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["selection"], selection);
    let memories = value["native"]["memories"].as_array().unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["id"], json!(stored.id));
    assert_eq!(memories[0]["body"], "x".repeat(5000));
}

#[tokio::test]
async fn deadline_and_shared_stop_never_return_data_after_held_project_admission() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(2));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.hold_project.store(true, Ordering::Release);
    let response = read(&f, &cookie, "list", body(&selection, json!({}))).await;
    assert!(!response.status().is_success());
    assert!(
        response
            .json::<Value>()
            .await
            .unwrap()
            .get("native")
            .is_none()
    );
    f.issuer.hold_project.store(false, Ordering::Release);
    f.issuer.release_project.notify_one();
    assert_eq!(
        read(&f, &cookie, "list", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::OK
    );
    f.tasks.as_ref().unwrap().0.shutdown().await.unwrap();
    let response = read(&f, &cookie, "list", body(&selection, json!({}))).await;
    assert!(!response.status().is_success());
    assert!(
        response
            .json::<Value>()
            .await
            .unwrap()
            .get("native")
            .is_none()
    );
}
#[tokio::test]
async fn provider_removal_fences_memory_before_sdk_io() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    assert_eq!(
        read(&f, &cookie, "list", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::OK
    );
    let (tasks, raw) = f.tasks.as_ref().unwrap();
    crate::task_server::tests::browser_provider(tasks, raw, &f.issuer.origin, true);
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = read(&f, &cookie, "list", body(&selection, json!({}))).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        response
            .json::<Value>()
            .await
            .unwrap()
            .get("native")
            .is_none()
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn corrupt_native_manifest_is_an_error_and_never_no_corpus() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    memory_fixture::seed(
        &tasks,
        &raw,
        0,
        Seed::Create(input("corrupt", false, false)),
    )
    .await;
    let selection = memory_fixture::selection(&tasks, 0);
    memory_fixture::corrupt_manifest(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let response = read(&f, &cookie, "list", body(&selection, json!({}))).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let error: Value = response.json().await.unwrap();
    assert!(error.get("native").is_none());
    assert!(matches!(
        error["error"]["code"].as_str(),
        Some("store_failure" | "branch_failure" | "artifact_failure")
    ));
    f.finish_tasks().await;
}
#[tokio::test]
async fn expired_access_without_refresh_cannot_read_memory() {
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    // JWT timestamps have whole-second precision; leave a full second for login.
    f.issuer.access_seconds.store(2, Ordering::Release);
    f.issuer.issue_refresh.store(false, Ordering::Release);
    let cookie = f.login().await;
    wait_until_access_expiration(&f).await;
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    assert_eq!(
        read(&f, &cookie, "list", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    f.finish_tasks().await;
}

#[tokio::test]
async fn disconnected_memory_request_remains_owned_until_its_original_ends() {
    use tokio::io::AsyncWriteExt;
    let (_temp, tasks, raw) = memory_workspace_fixture(None, Duration::from_secs(2));
    let selection = memory_fixture::selection(&tasks, 0);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.hold_project.store(true, Ordering::Release);
    let url = reqwest::Url::parse(&f.origin).unwrap();
    let mut socket = tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
        .await
        .unwrap();
    let payload = body(&selection, json!({})).to_string();
    let wire = format!(
        "POST /api/memory/list HTTP/1.1\r\nHost: {}:{}\r\nOrigin: {}\r\nCookie: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        url.host_str().unwrap(),
        url.port().unwrap(),
        f.origin,
        cookie,
        payload.len(),
        payload
    );
    socket.write_all(wire.as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), f.issuer.project_started.notified())
        .await
        .unwrap();
    drop(socket);
    {
        let mut jobs = f.service.0.jobs.lock().await;
        tokio::time::timeout(Duration::from_secs(5), jobs.join_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    f.issuer.hold_project.store(false, Ordering::Release);
    f.issuer.release_project.notify_waiters();
    assert_eq!(
        read(&f, &cookie, "list", body(&selection, json!({})))
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn two_admitted_versions_of_same_project_keep_native_branches_and_selection_exact() {
    let (_temp, base, raw) = memory_workspace_fixture(None, Duration::from_secs(30));
    let tasks = memory_fixture::with_named_version(&base, "memory-feature");
    let main = memory_fixture::seed(
        &tasks,
        &raw,
        0,
        Seed::Create(input("main-fact", false, false)),
    )
    .await;
    let feature = memory_fixture::seed(
        &tasks,
        &raw,
        3,
        Seed::Create(input("feature-fact", false, false)),
    )
    .await;
    let main_selection = memory_fixture::selection(&tasks, 0);
    let feature_selection = memory_fixture::selection(&tasks, 3);
    assert_eq!(
        main_selection["project_id"],
        feature_selection["project_id"]
    );
    assert_eq!(
        main_selection["project_dir"],
        feature_selection["project_dir"]
    );
    assert_ne!(
        main_selection["project_version"],
        feature_selection["project_version"]
    );
    tasks.shutdown().await.unwrap();
    let f = Fixture::with_tasks(policy(), Some((reopen_browser(&tasks), raw))).await;
    let cookie = f.login().await;
    let main_read: Value = read(&f, &cookie, "list", body(&main_selection, json!({})))
        .await
        .json()
        .await
        .unwrap();
    let feature_read: Value = read(&f, &cookie, "list", body(&feature_selection, json!({})))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        main_read["native"]["memories"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].clone())
            .collect::<Vec<_>>(),
        vec![json!(main.id)]
    );
    assert!(
        feature_read["native"]["memories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == json!(feature.id))
    );
    assert_eq!(
        feature_read["selection"]["project_version"],
        "memory-feature"
    );
    assert_eq!(
        feature_read["native"]["generation"]["project_version"],
        "memory-feature"
    );
    assert_ne!(
        main_read["native"]["generation"]["physical_branch"],
        feature_read["native"]["generation"]["physical_branch"]
    );
    let absent: Value = read(
        &f,
        &cookie,
        "get",
        body(&main_selection, json!({"id":feature.id})),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(absent["native"], Value::Null);
    let current: Value = read(
        &f,
        &cookie,
        "get",
        body(&feature_selection, json!({"id":feature.id})),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        current["native"]["memory"],
        serde_json::to_value(feature).unwrap()
    );
}
