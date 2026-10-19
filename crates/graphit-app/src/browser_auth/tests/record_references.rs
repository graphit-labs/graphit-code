//! Native records under the actual registered Web identity and signed PROJECT backend.
use super::*;

#[tokio::test]
async fn record_references_fresh_project_and_current_switch_are_visible_after_startup() {
    let (temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Memory),
            Permission::Read(Module::Knowledge),
        ],
        &["added"],
        None,
        false,
    );
    let first = browser_selection(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let added =
        native::add_current_project(&f.tasks.as_ref().unwrap().0, &temp.path().join("added"));
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    for path in ["/api/memory/contexts", "/api/knowledge/contexts"] {
        let response = post(&f, &cookie, path, json!({"input":{}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["contexts"], json!([first, added]));
        assert_eq!(body["current"], added);
        let projects = body["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects.contains(&json!({"project_id":first["project_id"],"name":"project"})));
        assert!(projects.contains(&json!({"project_id":added["project_id"],"name":"added"})));
        assert_eq!(body["current"].as_object().unwrap().len(), 3);
    }
    let response = post(
        &f,
        &cookie,
        "/api/records/references",
        request(
            &added,
            "memory",
            &json!("literal target"),
            json!([{"source":{"kind":"project_memory"},"direction":"incoming"}]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["selection"], added);
    assert_eq!(body["sources"][0]["native"], "no_corpus");
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_context_limit_counts_authorized_output_instead_of_raw_inventory() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let denied = browser_selection(&tasks);
    let allowed = memory_fixture::selection(&tasks, 1);
    let mut content = native::content_limits(&tasks);
    content.max_collection_entries = 2;
    let tasks = tasks.references_test_policy(content, Duration::from_secs(30));
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    f.issuer
        .denied_projects
        .lock()
        .unwrap()
        .push(denied["project_id"].clone());
    for path in ["/api/memory/contexts", "/api/knowledge/contexts"] {
        let response = post(&f, &cookie, path, json!({"input":{}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["contexts"], json!([allowed]));
        assert_eq!(body["current"], Value::Null);
        assert_eq!(
            body["projects"],
            json!([{"project_id":allowed["project_id"],"name":"other"}])
        );
    }
    f.issuer
        .denied_projects
        .lock()
        .unwrap()
        .push(allowed["project_id"].clone());
    let response = post(&f, &cookie, "/api/memory/contexts", json!({"input":{}})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["contexts"], json!([]));
    assert_eq!(body["current"], Value::Null);
    assert_eq!(body["projects"], json!([]));
    f.issuer.denied_projects.lock().unwrap().clear();
    let response = post(&f, &cookie, "/api/memory/contexts", json!({"input":{}})).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "response_limit");
    assert!(body.get("contexts").is_none());
    assert!(body.get("projects").is_none());
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_live_retirement_filters_contexts_retargets_operational_default_and_rejects_explicit_selection()
 {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let retired = browser_selection(&tasks);
    let remaining = memory_fixture::selection(&tasks, 1);
    let mirror = memory_fixture::selection(&tasks, 2);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    let server = &f.tasks.as_ref().unwrap().0;
    native::retire_checkout(server, 0);
    for path in ["/api/memory/contexts", "/api/knowledge/contexts"] {
        let response = post(&f, &cookie, path, json!({"input":{}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["contexts"], json!([remaining, mirror]));
        assert_eq!(body["current"], remaining);
        let projects = body["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects.contains(&json!({"project_id":remaining["project_id"],"name":"other"})));
        assert!(projects.contains(&json!({"project_id":mirror["project_id"],"name":"project"})));
    }
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = post(
        &f,
        &cookie,
        "/api/records/references",
        request(
            &retired,
            "memory",
            &json!("logical target"),
            json!([{"source":{"kind":"project_memory"},"direction":"incoming"}]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "selection_unavailable");
    assert!(body.get("sources").is_none());
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    // The fixture indexes the actual operational roster after each withdrawal.
    native::retire_checkout(server, 0);
    native::retire_checkout(server, 0);
    for path in ["/api/memory/contexts", "/api/knowledge/contexts"] {
        let response = post(&f, &cookie, path, json!({"input":{}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["contexts"], json!([]));
        assert_eq!(body["current"], Value::Null);
        assert_eq!(body["projects"], json!([]));
    }
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_live_retirement_during_accepted_request_rejects_refs_and_contexts_without_healthy_prefix()
 {
    for path in [
        "/api/records/references",
        "/api/memory/contexts",
        "/api/knowledge/contexts",
    ] {
        let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
        let selected = browser_selection(&tasks);
        let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
        let cookie = f.login().await;
        f.issuer.hold_project.store(true, Ordering::Release);
        let query = if path == "/api/records/references" {
            request(
                &selected,
                "memory",
                &json!("logical target"),
                json!([{"source":{"kind":"project_memory"},"direction":"incoming"}]),
            )
        } else {
            json!({"input":{}})
        };
        let started = f.issuer.project_started.notified();
        let response = f
            .client
            .post(format!("{}{path}", f.origin))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &f.origin)
            .json(&query)
            .send();
        tokio::pin!(started, response);
        tokio::select! {
            _ = &mut started => {},
            unexpected = &mut response => panic!("request completed before PROJECT gate: {unexpected:?}"),
        }
        native::retire_checkout(&f.tasks.as_ref().unwrap().0, 0);
        f.issuer.hold_project.store(false, Ordering::Release);
        f.issuer.release_project.notify_one();
        let response = response.await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["error"]["code"], "inactive_project_selection");
        assert!(body.get("sources").is_none());
        assert!(body.get("contexts").is_none());
        assert!(body.get("projects").is_none());
        f.finish_tasks().await;
    }
}

#[tokio::test]
async fn record_references_absent_sources_and_published_zero_matches_keep_distinct_native_states() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selection = browser_selection(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let query = json!({"selection":selection,"input":{"focus":{"kind":"memory","scope":{"project":selection["project_id"]},"id":"literal dangling target"}}});
    let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let absent: Value = response.json().await.unwrap();
    assert_eq!(absent["sources"].as_array().unwrap().len(), 3);
    for source in absent["sources"].as_array().unwrap() {
        assert_eq!(source["native"], "no_corpus");
    }
    let (server, raw) = f.tasks.as_ref().unwrap();
    seed_browser(server,raw,"create_session",json!({"idempotency_key":"published-session","title":"Published source","description":"No requested targets","strategy":"bounded"})).await;
    memory_fixture::seed(
        server,
        raw,
        0,
        memory_fixture::Seed::Create(memory("Published Memory", vec![])),
    )
    .await;
    let project = serde_json::from_value(selection["project_id"].clone()).unwrap();
    let fields = [KnowledgeReferenceFields {
        slug: "Published Knowledge",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored: &[],
    }];
    native::publish(
        server,
        raw,
        0,
        &[native::page(
            "Published Knowledge",
            "Published Knowledge",
            &[],
        )],
        Some(&fields),
    )
    .await;
    let response = post(&f, &cookie, "/api/records/references", query).await;
    assert_eq!(response.status(), StatusCode::OK);
    let published: Value = response.json().await.unwrap();
    for source in published["sources"].as_array().unwrap() {
        assert!(source["native"]["published"].is_object());
    }
    assert_eq!(
        published["sources"][0]["native"]["published"]["incoming"]["results"],
        json!([])
    );
    assert_eq!(
        published["sources"][1]["native"]["published"]["incoming"]["results"],
        json!([])
    );
    assert_eq!(
        published["sources"][2]["native"]["published"]["read"]["incoming"]["results"],
        json!([])
    );
    assert_eq!(published["incoming_complete"], true);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_cursor_binds_source_direction_path_and_current_generation() {
    let (_temp, tasks, raw) = browser_workspace_fixture(None, Duration::from_secs(30));
    let selection = browser_selection(&tasks);
    let mirror = memory_fixture::selection(&tasks, 2);
    let session=seed_browser(&tasks,&raw,"create_session",json!({"idempotency_key":"cursor-session","title":"Cursor session","description":"Native","strategy":"bounded"})).await;
    let target = seed_browser(
        &tasks,
        &raw,
        "create_task",
        task_input("cursor-target", &session["id"], "Cursor target"),
    )
    .await;
    let mut create = task_input("cursor-source", &session["id"], "Cursor source");
    create["references"]=json!((0..5).map(|n|json!({"target":{"type":"task","id":target["specification"]["id"]},"relation":format!("relation-{n}")})).collect::<Vec<_>>());
    let source = seed_browser(&tasks, &raw, "create_task", create).await;
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let mut query = request(
        &selection,
        "task",
        &source["specification"]["id"],
        json!([{"source":{"kind":"project_task"},"direction":"outgoing"}]),
    );
    query["input"]["page_size"] = json!(2);
    let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let first: Value = response.json().await.unwrap();
    let first_native = &first["sources"][0]["native"]["published"];
    assert_eq!(first_native["outgoing"]["complete"], true);
    assert_eq!(first["outgoing_complete"], false);
    let cursor = first_native["outgoing"]["next_cursor"].clone();
    assert!(cursor.is_object());
    query["input"]["pages"][0]["cursor"] = cursor.clone();
    query["input"]["page_size"] = json!(100);
    let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let tail: Value = response.json().await.unwrap();
    assert_eq!(
        tail["sources"][0]["native"]["published"]["generation"],
        first_native["generation"]
    );
    assert_eq!(
        tail["sources"][0]["native"]["published"]["outgoing"]["next_cursor"],
        Value::Null
    );
    assert_eq!(tail["outgoing_complete"], true);
    let mut wrong = query.clone();
    wrong["input"]["pages"][0]["direction"] = json!("incoming");
    assert_eq!(
        post(&f, &cookie, "/api/records/references", wrong)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut wrong = query.clone();
    wrong["selection"] = mirror;
    assert_eq!(
        post(&f, &cookie, "/api/records/references", wrong)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut wrong = query.clone();
    wrong["input"]["pages"][0]["source"] = json!({"kind":"project_memory"});
    assert_eq!(
        post(&f, &cookie, "/api/records/references", wrong)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let (server, raw) = f.tasks.as_ref().unwrap();
    seed_browser(
        server,
        raw,
        "create_task",
        task_input("cursor-advance", &session["id"], "Advances this source"),
    )
    .await;
    let stale = post(&f, &cookie, "/api/records/references", query).await;
    assert_eq!(stale.status(), StatusCode::BAD_REQUEST);
    assert!(
        stale
            .json::<Value>()
            .await
            .unwrap()
            .get("sources")
            .is_none()
    );
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_rich_target_denial_is_unresolved_without_hiding_native_edge() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selection = browser_selection(&tasks);
    let memory = memory_fixture::seed(
        &tasks,
        &raw,
        0,
        memory_fixture::Seed::Create(memory("Restricted Memory", vec![])),
    )
    .await;
    let session=seed_browser(&tasks,&raw,"create_session",json!({"idempotency_key":"rich-session","title":"Rich session","description":"Native","strategy":"bounded"})).await;
    let mut input = task_input("rich-source", &session["id"], "Rich source");
    input["references"] = json!([{"target":{"type":"memory","id":memory.id},"relation":"uses"}]);
    let source = seed_browser(&tasks, &raw, "create_task", input).await;
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer
        .denied_modules
        .lock()
        .unwrap()
        .push("memory".into());
    let response = post(
        &f,
        &cookie,
        "/api/records/references",
        request(
            &selection,
            "task",
            &source["specification"]["id"],
            json!([{"source":{"kind":"project_task"},"direction":"outgoing"}]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let frame = &body["sources"][0];
    let found = frame["native"]["published"]["outgoing"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .position(|edge| edge["reference"]["id"] == json!(memory.id))
        .unwrap();
    assert_eq!(
        frame["navigation"]["outgoing"][found]["target_rich"],
        Value::Null
    );
    assert_eq!(
        frame["native"]["published"]["outgoing"]["results"][found]["reference"]["relation"],
        "uses"
    );
    assert_eq!(body["sources"].as_array().unwrap().len(), 1);
    f.finish_tasks().await;
}
use crate::task_server::browser::user_memory_fixture as user_native;
use crate::task_server::browser::{memory_fixture, records_fixture as native};
use crate::task_server::tests::{
    browser_selection, browser_workspace_fixture, knowledge_workspace_fixture, reopen_browser,
    seed_browser,
};
use graphit_auth::Permission;
use graphit_auth::broker::BrokerPolicy;
use graphit_core::module_settings::Module;
use graphit_data::knowledge_references::{
    KnowledgeAuthoredReference, KnowledgeDocumentState, KnowledgeReferenceFields,
    KnowledgeReferenceNamespace, KnowledgeReferenceScope, KnowledgeReferenceTargetKind,
};
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{CreateMemory, MemoryReference, MemoryReferenceScope},
    reference_projection::ReferenceTargetKind,
};

async fn post(f: &Fixture, cookie: &str, path: &str, body: Value) -> reqwest::Response {
    f.client
        .post(format!("{}{path}", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .json(&body)
        .send()
        .await
        .unwrap()
}
fn task_input(key: &str, session: &Value, title: &str) -> Value {
    json!({"idempotency_key":key,"session_id":session,"title":title,"description":"Native references fixture","task_type":"TASK","priority":1,"checks":[{"kind":"acceptance","text":"Literal identity"},{"kind":"test","text":"Own reader"}]})
}
fn request(selection: &Value, kind: &str, id: &Value, pages: Value) -> Value {
    json!({"selection":selection,"input":{"focus":{"kind":kind,"scope":{"project":selection["project_id"]},"id":id},"pages":pages,"page_size":100}})
}
#[tokio::test]
async fn record_references_own_module_discovery_excludes_denied_projects_and_preserves_task_boundary()
 {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let expected = browser_selection(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.denied_modules.lock().unwrap().push("task".into());
    for path in ["/api/memory/contexts", "/api/knowledge/contexts"] {
        let response = post(&f, &cookie, path, json!({"input":{}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["current"], expected);
        assert_eq!(body["contexts"].as_array().unwrap().len(), 3);
        for context in body["contexts"].as_array().unwrap() {
            assert_eq!(context.as_object().unwrap().len(), 3);
        }
        let projects = body["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects.contains(&json!({"project_id":expected["project_id"],"name":"project"})));
        assert!(
            projects
                .contains(&json!({"project_id":body["contexts"][1]["project_id"],"name":"other"}))
        );
        assert_eq!(
            body["contexts"][0]["project_id"],
            body["contexts"][2]["project_id"]
        );
        assert_ne!(
            body["contexts"][0]["project_dir"],
            body["contexts"][2]["project_dir"]
        );
    }
    let denied = post(&f, &cookie, "/api/tasks/contexts", json!({})).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert!(
        denied
            .json::<Value>()
            .await
            .unwrap()
            .get("projects")
            .is_none()
    );
    f.issuer
        .denied_projects
        .lock()
        .unwrap()
        .push(expected["project_id"].clone());
    for path in ["/api/memory/contexts", "/api/knowledge/contexts"] {
        let response = post(&f, &cookie, path, json!({"input":{}})).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["current"], Value::Null);
        assert_eq!(body["contexts"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["projects"],
            json!([{"project_id":body["contexts"][0]["project_id"],"name":"other"}])
        );
        assert_ne!(body["contexts"][0]["project_id"], expected["project_id"]);
        assert!(body["contexts"][0]["project_dir"].is_string());
        assert_eq!(
            body["contexts"][0]["project_version"],
            expected["project_version"]
        );
    }
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    assert_eq!(
        post(
            &f,
            &cookie,
            "/api/memory/contexts",
            json!({"input":{"module":"task"}})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(&f, "", "/api/knowledge/contexts", json!({"input":{}}))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_literal_backlinks_and_native_reader_descriptors_survive_reopen() {
    let (_temp, tasks, raw) = browser_workspace_fixture(None, Duration::from_secs(30));
    let session=seed_browser(&tasks,&raw,"create_session",json!({"idempotency_key":"records-session","title":"Actual session","description":"Native records","strategy":"bounded"})).await;
    let target = seed_browser(
        &tasks,
        &raw,
        "create_task",
        task_input("records-target", &session["id"], "Actual target"),
    )
    .await;
    let mut input = task_input("records-source", &session["id"], "Actual source");
    input["references"] = json!([
        {"target":{"type":"memory","id":"literal-memory"},"relation":"supports"},
        {"target":{"type":"task","id":target["specification"]["id"]},"relation":"tracks"},
        {"target":{"type":"task","id":target["specification"]["id"]},"relation":"tracks"}
    ]);
    let source = seed_browser(&tasks, &raw, "create_task", input).await;
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let selection = browser_selection(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = post(
        &f,
        &cookie,
        "/api/records/references",
        request(
            &selection,
            "memory",
            &json!("literal-memory"),
            json!([{"source":{"kind":"project_task"},"direction":"incoming"}]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["selection"], selection);
    let native = &body["sources"][0]["native"]["published"];
    assert_eq!(native["incoming"]["results"].as_array().unwrap().len(), 1);
    assert_eq!(
        native["incoming"]["results"][0]["reference"]["id"],
        "literal-memory"
    );
    assert_eq!(
        body["sources"][0]["navigation"]["incoming"][0]["target_rich"],
        Value::Null
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before + 1);
    let response = post(
        &f,
        &cookie,
        "/api/records/references",
        request(
            &selection,
            "task",
            &source["specification"]["id"],
            json!([{"source":{"kind":"project_task"},"direction":"outgoing"}]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let frame = &body["sources"][0];
    let native = &frame["native"]["published"];
    assert!(native["incoming"].is_object());
    assert_eq!(native["outgoing"]["complete"], true);
    assert_eq!(body["outgoing_complete"], true);
    assert_eq!(
        native["coverage"]["inbound_other_sources_not_qualified"],
        true
    );
    assert_eq!(
        frame["navigation"]["outgoing"].as_array().unwrap().len(),
        native["outgoing"]["results"].as_array().unwrap().len()
    );
    for (edge, nav) in native["outgoing"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .zip(frame["navigation"]["outgoing"].as_array().unwrap())
    {
        assert_eq!(edge["source_snapshot"], native["generation"]);
        if edge["reference"]["id"] == "literal-memory" {
            assert_eq!(nav["target_rich"], Value::Null);
        }
        if edge["reference"]["relation"] == "tracks" {
            let rich = &nav["target_rich"];
            assert_eq!(rich["title"], "Actual target");
            assert_eq!(rich["reader"]["method"], "POST");
            assert_eq!(rich["reader"]["path"], "/api/tasks/get");
            assert_eq!(rich["reader"]["selection"], selection);
            let response = post(
                &f,
                &cookie,
                rich["reader"]["path"].as_str().unwrap(),
                json!({"selection":rich["reader"]["selection"],"input":rich["reader"]["input"]}),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let read: Value = response.json().await.unwrap();
            assert_eq!(read["task"]["specification"]["title"], "Actual target");
        }
    }
    assert_eq!(
        native["outgoing"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|edge| edge["reference"]["relation"] == "tracks")
            .count(),
        2
    );
    f.finish_tasks().await;
}

fn memory(key: &str, references: Vec<MemoryReference>) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: key.into(),
        body: format!("Complete body {key}"),
        attributes: MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory: false,
            important: false,
        },
        tags: vec!["literal".into()],
        references: Some(references),
    }
}
#[tokio::test]
async fn record_references_native_sources_keep_independent_dtos_and_causal_source_denial() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selection = browser_selection(&tasks);
    let project = serde_json::from_value(selection["project_id"].clone()).unwrap();
    let session=seed_browser(&tasks,&raw,"create_session",json!({"idempotency_key":"all-records-session","title":"All records","description":"Native","strategy":"bounded"})).await;
    let task = seed_browser(
        &tasks,
        &raw,
        "create_task",
        task_input("all-records-task", &session["id"], "All records target"),
    )
    .await;
    let task_id = task["specification"]["id"].as_str().unwrap();
    let memory = memory_fixture::seed(
        &tasks,
        &raw,
        0,
        memory_fixture::Seed::Create(memory(
            "Memory source",
            vec![MemoryReference {
                target_kind: ReferenceTargetKind::Task,
                scope: MemoryReferenceScope::Project(project),
                id: task_id.into(),
                context: None,
                relation: "uses".into(),
                field: Some("evidence".into()),
            }],
        )),
    )
    .await;
    let authored = [KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Task,
        scope: KnowledgeReferenceScope::Project(project),
        context: None,
        id: task_id,
        relation: "uses",
        field: Some("technical"),
    }];
    let fields = [
        KnowledgeReferenceFields {
            slug: "Knowledge source",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project),
                context: None,
            },
            authored: &authored,
        },
        KnowledgeReferenceFields {
            slug: "No outgoing",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project),
                context: None,
            },
            authored: &[],
        },
    ];
    native::publish(
        &tasks,
        &raw,
        0,
        &[
            native::page("Knowledge source", "Knowledge source", &[]),
            native::page("No outgoing", "Empty projection", &[]),
        ],
        Some(&fields),
    )
    .await;
    tasks.shutdown().await.unwrap();
    let tasks = reopen_browser(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let request = json!({"selection":selection,"input":{"focus":{"kind":"task","scope":{"project":project},"id":task_id},"include_user":false}});
    let response = post(&f, &cookie, "/api/records/references", request.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let sources = body["sources"].as_array().unwrap();
    assert_eq!(
        sources
            .iter()
            .map(|source| source["source"]["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["project_task", "project_memory", "project_knowledge"]
    );
    assert_eq!(
        sources[1]["native"]["published"]["incoming"]["results"][0]["source"],
        json!(memory.id)
    );
    assert_eq!(sources[1]["native"]["published"]["outgoing"], Value::Null);
    let knowledge = &sources[2]["native"]["published"];
    assert_eq!(
        knowledge["read"]["incoming"]["results"][0]["source"]["slug"],
        "Knowledge source"
    );
    assert_eq!(
        knowledge["read"]["incoming"]["results"][0]["source"]["compilation_digest"],
        serde_json::to_value([7_u8; 32]).unwrap()
    );
    assert_eq!(
        knowledge["read"]["incoming"]["results"][0]["payload"]["authored"]["field"],
        "technical"
    );
    assert!(knowledge["generation"].is_object());
    assert_eq!(body["incoming_complete"], true);
    f.issuer
        .denied_modules
        .lock()
        .unwrap()
        .push("memory".into());
    let denied = post(&f, &cookie, "/api/records/references", request).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let denied: Value = denied.json().await.unwrap();
    assert_eq!(denied["error"]["code"], "project_access_denied");
    assert!(denied.get("sources").is_none());
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_import_http_uses_same_realm_registered_native_source_and_full16_page() {
    let (_temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        None,
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let published = native::publish(
        &tasks,
        &raw,
        0,
        &[native::page("Imported page", "Imported title", &[])],
        None,
    )
    .await;
    let source =
        native::register_copy(&tasks, &published, "vendor/references", "exact-version", 0).await;
    let registry_before = native::registry_bytes(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = post(
        &f,
        &cookie,
        "/api/knowledge/imported/contexts",
        json!({"input":{}}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let contexts: Value = response.json().await.unwrap();
    assert_eq!(
        contexts["contexts"],
        serde_json::to_value([&source]).unwrap()
    );
    let response = post(
        &f,
        &cookie,
        "/api/knowledge/imported/page",
        json!({"input":{"source":source,"slug":"Imported page"}}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["native"]["state"], "published");
    assert_eq!(body["native"]["page"]["state"], "found");
    assert_eq!(body["focused_source"]["slug"], "Imported page");
    assert_eq!(body["focused_source"]["title"], "Imported title");
    assert_eq!(body["focused_source"]["namespace"], Value::Null);
    assert_eq!(body["focused_source"]["witness"], Value::Null);
    let document = &body["native"]["page"]["document"];
    assert_eq!(
        body["focused_source"]["source_path"],
        document["source_path"]
    );
    assert_eq!(
        body["focused_source"]["compilation_digest"],
        document["compilation_digest"]
    );
    assert_eq!(document.as_object().unwrap().len(), 16);
    assert_eq!(document["title"], "Imported title");
    assert_eq!(document["body"], "# Complete native body é\n");
    assert_eq!(document["updated_at"], "-1");
    assert_eq!(
        document["compilation_digest"],
        serde_json::to_value([7_u8; 32]).unwrap()
    );
    assert_eq!(document["physical_digest"], Value::Null);
    assert_eq!(document["recipe_parameters"]["max_summary_bytes"], 32);
    let absent = post(
        &f,
        &cookie,
        "/api/knowledge/imported/page",
        json!({"input":{"source":source,"slug":"Absent page"}}),
    )
    .await;
    assert_eq!(absent.status(), StatusCode::OK);
    let absent: Value = absent.json().await.unwrap();
    assert_eq!(absent["native"]["page"]["state"], "page_absent");
    assert_eq!(absent["focused_source"], Value::Null);
    let source_value = serde_json::to_value(&source).unwrap();
    let mut wrong = source_value.clone();
    wrong["artifact"]["version"] = json!("not-registered");
    let absent = post(
        &f,
        &cookie,
        "/api/knowledge/imported/page",
        json!({"input":{"source":wrong,"slug":"Imported page"}}),
    )
    .await;
    assert_eq!(absent.status(), StatusCode::OK);
    let absent: Value = absent.json().await.unwrap();
    assert_eq!(absent["native"]["state"], "registry_absent");
    assert_eq!(absent["focused_source"], Value::Null);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    assert_eq!(
        native::registry_bytes(&f.tasks.as_ref().unwrap().0),
        registry_before
    );
    f.finish_tasks().await;
}

fn incoming_user(f: &Fixture, revision: u64) -> BrokerVerifier {
    BrokerVerifier::new(
        BrokerPolicy {
            endpoint: &f.issuer.origin,
            mcp_resource: Some(user_native::RESOURCE),
            provider_revision: revision,
            network: ClientPolicy {
                origin: &f.issuer.origin,
                allow_loopback_http: true,
                proxy: None,
                additional_roots: &[],
            },
        },
        NetworkLimits {
            url_bytes: 8192,
            request_body_bytes: 8192,
            response_body_bytes: 32768,
            max_in_flight: 8,
            header_bytes: 16384,
            header_count: 32,
            max_idle_per_host: 2,
            idle_timeout: Duration::from_secs(30),
            max_certificates: 2,
            max_certificate_bytes: 8192,
        },
        VerifierLimits {
            token_bytes: 4096,
            json: JsonLimits {
                bytes: 32768,
                nodes: 256,
                depth: 16,
                scalar_bytes: 8192,
                collection_entries: 64,
            },
            max_keys: 8,
            metadata_ttl: Duration::from_secs(60),
            key_ttl: Duration::from_secs(60),
            unknown_key_cooldown: Duration::from_secs(1),
        },
    )
    .unwrap()
}
#[tokio::test]
async fn record_references_user_only_uses_observed_owner_without_project_io_and_false_is_preflight_rejected()
 {
    let (_temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[Permission::ManageAuthProfiles],
        &[],
        Some(&[Permission::ReadUser, Permission::Read(Module::Memory)]),
        false,
    );
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let (server, raw) = f.tasks.as_ref().unwrap();
    let revision = user_native::provider(server, raw, &f.issuer.origin);
    let verifier = incoming_user(&f, revision);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let token=f.issuer.jwt(json!({"iss":f.issuer.origin,"sub":"web-subject","aud":user_native::RESOURCE,"client_id":"native-client","jti":"records-native-user-seed","token_use":"access","iat":now,"exp":now+600}));
    let first = user_native::seed(
        server,
        &verifier,
        &token,
        user_native::Seed::Create(memory("Own USER target", vec![])),
    )
    .await;
    let cookie = f.login().await;
    let response = post(
        &f,
        &cookie,
        "/api/user-memory/get",
        json!({"input":{"id":first.id}}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let own: Value = response.json().await.unwrap();
    let scope = own["scope_id"].as_str().unwrap();
    user_native::seed(
        server,
        &verifier,
        &token,
        user_native::Seed::Create(memory(
            "Own USER source",
            vec![MemoryReference {
                target_kind: ReferenceTargetKind::Memory,
                scope: MemoryReferenceScope::User(scope.into()),
                id: first.id.to_string(),
                context: None,
                relation: "supports".into(),
                field: None,
            }],
        )),
    )
    .await;
    f.issuer.project_grant.store(false, Ordering::Release);
    let body = json!({"input":{"focus":{"kind":"memory","scope":{"user":scope},"id":first.id},"include_user":true,"pages":[{"source":{"kind":"user_memory"},"direction":"incoming"}]}});
    let response = post(&f, &cookie, "/api/records/references", body.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let output: Value = response.json().await.unwrap();
    assert_eq!(output["selection"], Value::Null);
    assert_eq!(output["sources"].as_array().unwrap().len(), 1);
    let native = &output["sources"][0]["native"]["published"];
    assert_eq!(native["incoming"]["results"].as_array().unwrap().len(), 1);
    assert_eq!(native["incoming"]["results"][0]["source_scope"], scope);
    let rich = &output["sources"][0]["navigation"]["incoming"][0]["target_rich"];
    assert_eq!(rich["title"], "Own USER target");
    assert_eq!(rich["reader"]["path"], "/api/user-memory/get");
    assert_eq!(rich["reader"]["selection"], Value::Null);
    let mut denied = body.clone();
    denied["input"]["include_user"] = json!(false);
    assert_eq!(
        post(&f, &cookie, "/api/records/references", denied)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut foreign = body;
    foreign["input"]["focus"]["scope"] = json!({"user":"foreign literal owner"});
    foreign["input"]["pages"][0]["direction"] = json!("outgoing");
    assert_eq!(
        post(&f, &cookie, "/api/records/references", foreign)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );

    let paged = user_native::seed(
        server,
        &verifier,
        &token,
        user_native::Seed::Create(memory(
            "Paged USER source",
            (0..3)
                .map(|n| MemoryReference {
                    target_kind: ReferenceTargetKind::Memory,
                    scope: MemoryReferenceScope::User(scope.into()),
                    id: first.id.to_string(),
                    context: None,
                    relation: format!("supports-{n}"),
                    field: None,
                })
                .collect(),
        )),
    )
    .await;
    let mut query = json!({"input":{
        "focus":{"kind":"memory","scope":{"user":scope},"id":paged.id},"include_user":true,"page_size":1,
        "pages":[{"source":{"kind":"user_memory"},"direction":"outgoing"}],
    }});
    let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let first_page: Value = response.json().await.unwrap();
    assert_eq!(
        first_page["sources"][0]["native"]["published"]["outgoing"]["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(first_page["outgoing_complete"], false);
    let cursor = first_page["sources"][0]["native"]["published"]["outgoing"]["next_cursor"].clone();
    assert!(cursor.is_object());
    query["input"]["pages"][0]["cursor"] = cursor;
    query["input"]["page_size"] = json!(100);
    let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let tail: Value = response.json().await.unwrap();
    assert_eq!(
        tail["sources"][0]["native"]["published"]["outgoing"]["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(tail["outgoing_complete"], true);
    user_native::seed(
        server,
        &verifier,
        &token,
        user_native::Seed::Create(memory("Advance USER only", vec![])),
    )
    .await;
    let response = post(&f, &cookie, "/api/records/references", query).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        response
            .json::<Value>()
            .await
            .unwrap()
            .get("sources")
            .is_none()
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_zero_edge_knowledge_uses_stored_namespace_instead_of_mount_alias() {
    let (temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &["producer"],
        None,
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let registered_producer = browser_selection(&tasks);
    let selection = native::add_current_project(&tasks, &temp.path().join("producer"));
    let project = serde_json::from_value(selection["project_id"].clone()).unwrap();
    let fields = [KnowledgeReferenceFields {
        slug: "Zero edge",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored: &[],
    }];
    let published = native::publish(
        &tasks,
        &raw,
        1,
        &[native::page("Zero edge", "Stored title", &[])],
        Some(&fields),
    )
    .await;
    let mut imported = serde_json::to_value(
        native::register_copy(&tasks, &published, "vendor/zero", "exact-version", 0).await,
    )
    .unwrap();
    imported["artifact"]["context_alias"] = json!("mount alias");
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    for source in [json!({"kind":"project_knowledge"}), imported.clone()] {
        let is_import = source["kind"] == "imported_knowledge";
        let mut query = json!({"input":{
            "focus":{"kind":"knowledge","scope":{"project":project},"id":"Zero edge"},
            "outgoing_source":source,"pages":[{"source":source,"direction":"outgoing"}],"page_size":100,
        }});
        if is_import {
            query["input"]["contexts"] = json!([source["artifact"]]);
        } else {
            query["selection"] = selection.clone();
        }
        let before = f.issuer.project_calls.load(Ordering::SeqCst);
        let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        let read = &body["sources"][0]["native"];
        let read = if is_import {
            &read["read"]
        } else {
            &read["published"]["read"]
        };
        assert_eq!(read["focused_source"]["namespace"]["context"], Value::Null);
        assert_eq!(
            read["focused_source"]["namespace"]["scope"],
            json!({"project":project})
        );
        if is_import {
            assert_eq!(
                body["sources"][0]["native"]["producer"],
                registered_producer["project_id"]
            );
            assert_ne!(registered_producer["project_id"], selection["project_id"]);
        }
        assert_eq!(read["focused_source"]["slug"], "Zero edge");
        assert!(read["focused_source"]["witness"].is_array());
        assert_eq!(read["outgoing"]["results"], json!([]));
        assert_eq!(body["outgoing_complete"], true);
        for context in [json!("stored namespace"), json!("mount alias")] {
            let mut wrong = query.clone();
            wrong["input"]["focus"]["context"] = context;
            let response = post(&f, &cookie, "/api/records/references", wrong).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body: Value = response.json().await.unwrap();
            assert_eq!(
                body["error"]["code"],
                if is_import {
                    "invalid_import_namespace"
                } else {
                    "invalid_knowledge_namespace"
                }
            );
            assert!(body.get("sources").is_none());
        }
        let mut wrong_scope = query;
        wrong_scope["input"]["focus"]["scope"] = json!({"user":"wrong literal namespace"});
        let response = post(&f, &cookie, "/api/records/references", wrong_scope).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body: Value = response.json().await.unwrap();
        assert_eq!(
            body["error"]["code"],
            if is_import {
                "invalid_import_namespace"
            } else {
                "invalid_knowledge_namespace"
            }
        );
        assert!(body.get("sources").is_none());
        if is_import {
            assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
        }
    }
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_knowledge_target_candidates_are_ambiguous_while_source_reader_stays_exact()
 {
    let (_temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        None,
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let selection = browser_selection(&tasks);
    let project = serde_json::from_value(selection["project_id"].clone()).unwrap();
    let authored = [KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Knowledge,
        scope: KnowledgeReferenceScope::Project(project),
        context: None,
        id: "Target",
        relation: "uses",
        field: None,
    }];
    let fields = [
        KnowledgeReferenceFields {
            slug: "Source",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project),
                context: None,
            },
            authored: &authored,
        },
        KnowledgeReferenceFields {
            slug: "Target",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project),
                context: None,
            },
            authored: &[],
        },
    ];
    let published = native::publish(
        &tasks,
        &raw,
        0,
        &[
            native::page("Source", "Source title", &[]),
            native::page("Target", "Target title", &[]),
        ],
        Some(&fields),
    )
    .await;
    let import = serde_json::to_value(
        native::register_copy(&tasks, &published, "vendor/candidates", "exact-version", 0).await,
    )
    .unwrap();
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let mut query = request(
        &selection,
        "knowledge",
        &json!("Source"),
        json!([{"source":{"kind":"project_knowledge"},"direction":"outgoing"}]),
    );
    let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["sources"][0]["navigation"]["outgoing"][0]["target_rich"]["title"],
        "Target title"
    );
    query["input"]["contexts"] = json!([import["artifact"]]);
    query["input"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"source":import,"direction":"incoming"}));
    let response = post(&f, &cookie, "/api/records/references", query).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let nav = &body["sources"][0]["navigation"]["outgoing"][0];
    assert_eq!(nav["target_rich"], Value::Null);
    assert_eq!(
        nav["source_reader"]["source"],
        json!({"kind":"project_knowledge"})
    );
    assert_eq!(nav["source_reader"]["selection"], selection);
    assert_eq!(nav["source_reader"]["focus"]["context"], Value::Null);
    let response = post(
        &f,
        &cookie,
        nav["source_reader"]["path"].as_str().unwrap(),
        json!({"selection":selection,"input":nav["source_reader"]["input"]}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let mut alias_a = import.clone();
    alias_a["artifact"]["context_alias"] = json!("alias a");
    let mut alias_b = import;
    alias_b["artifact"]["context_alias"] = json!("alias b");
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let query = json!({"input":{
        "focus":{"kind":"knowledge","scope":{"project":project},"id":"Source"},
        "outgoing_source":alias_a,
        "contexts":[alias_a["artifact"],alias_b["artifact"]],
        "pages":[{"source":alias_a,"direction":"outgoing"},{"source":alias_b,"direction":"incoming"}],
    }});
    let response = post(&f, &cookie, "/api/records/references", query).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let nav = &body["sources"][0]["navigation"]["outgoing"][0];
    assert_eq!(nav["target_rich"]["title"], "Target title");
    assert_eq!(nav["source_reader"]["source"], alias_a);
    assert_eq!(body["selection"], Value::Null);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_imported_dangling_target_resolves_another_admitted_exact_namespace() {
    let (temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &["other"],
        None,
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let selected = browser_selection(&tasks);
    let project = serde_json::from_value(selected["project_id"].clone()).unwrap();
    let target_fields = [KnowledgeReferenceFields {
        slug: "Target",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored: &[],
    }];
    native::publish(
        &tasks,
        &raw,
        0,
        &[native::page("Target", "Native target", &[])],
        Some(&target_fields),
    )
    .await;
    let other = native::add_current_project(&tasks, &temp.path().join("other"));
    let other_project = serde_json::from_value(other["project_id"].clone()).unwrap();
    let authored = [KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Knowledge,
        scope: KnowledgeReferenceScope::Project(project),
        context: None,
        id: "Target",
        relation: "uses",
        field: None,
    }];
    let source_fields = [KnowledgeReferenceFields {
        slug: "Source",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(other_project),
            context: None,
        },
        authored: &authored,
    }];
    let published = native::publish(
        &tasks,
        &raw,
        1,
        &[native::page("Source", "Imported source", &[])],
        Some(&source_fields),
    )
    .await;
    let import = serde_json::to_value(
        native::register_copy(&tasks, &published, "vendor/dangling", "exact-version", 1).await,
    )
    .unwrap();
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let query = json!({"selection":selected,"input":{
        "focus":{"kind":"knowledge","scope":{"project":other_project},"id":"Source"},
        "contexts":[import["artifact"]],"outgoing_source":import,
        "pages":[{"source":{"kind":"project_knowledge"},"direction":"incoming"},{"source":import,"direction":"outgoing"}],
    }});
    let response = post(&f, &cookie, "/api/records/references", query).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let nav = &body["sources"][1]["navigation"]["outgoing"][0];
    assert_eq!(nav["source_reader"]["source"], import);
    assert_eq!(
        nav["source_reader"]["focus"]["scope"],
        json!({"project":other_project})
    );
    assert_eq!(nav["target_rich"]["title"], "Native target");
    assert_eq!(
        nav["target_rich"]["reader"]["source"],
        json!({"kind":"project_knowledge"})
    );
    assert_eq!(nav["target_rich"]["reader"]["selection"], selected);
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_shared_stop_rejects_accepted_request_before_delivery() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selected = browser_selection(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.hold_project.store(true, Ordering::Release);
    let started = f.issuer.project_started.notified();
    let response = f
        .client
        .post(format!("{}/api/records/references", f.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &f.origin)
        .json(&request(
            &selected,
            "memory",
            &json!("logical target"),
            json!([{"source":{"kind":"project_memory"},"direction":"incoming"}]),
        ))
        .send();
    tokio::pin!(started, response);
    tokio::select! {
        _ = &mut started => {},
        unexpected = &mut response => panic!("completed before acceptance gate: {unexpected:?}"),
    }
    native::stop(&f.tasks.as_ref().unwrap().0);
    f.issuer.hold_project.store(false, Ordering::Release);
    f.issuer.release_project.notify_one();
    let response = response.await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = response.json().await.unwrap();
    // The accepted HTTP loop and the retained reader guard race to observe the same stop.
    assert!(body["error"] == "authentication interrupted" || body["error"]["code"] == "stopped");
    assert!(body.get("sources").is_none());
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_physical_output_limit_aborts_without_healthy_prefix() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selected = browser_selection(&tasks);
    let mut content = native::content_limits(&tasks);
    content.max_output_bytes = 64;
    let tasks = tasks.references_test_policy(content, Duration::from_secs(30));
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let response = post(
        &f,
        &cookie,
        "/api/records/references",
        request(
            &selected,
            "memory",
            &json!("logical target"),
            json!([{"source":{"kind":"project_memory"},"direction":"incoming"}]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "response_limit");
    assert!(body.get("sources").is_none());
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_native_default_incoming_keeps_foreign_target_namespace_without_outgoing_owner()
 {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selected = browser_selection(&tasks);
    let project = serde_json::from_value(selected["project_id"].clone()).unwrap();
    let authored = [KnowledgeAuthoredReference {
        target_kind: KnowledgeReferenceTargetKind::Knowledge,
        scope: KnowledgeReferenceScope::Project(project),
        context: Some("literal target context"),
        id: "Target",
        relation: "uses",
        field: None,
    }];
    let fields = [KnowledgeReferenceFields {
        slug: "Source",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored: &authored,
    }];
    native::publish(
        &tasks,
        &raw,
        0,
        &[native::page("Source", "Canonical source", &[])],
        Some(&fields),
    )
    .await;
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let query = json!({"selection":selected,"input":{
        "focus":{"kind":"knowledge","scope":{"project":project},"context":"literal target context","id":"Target"},
    }});
    let response = post(&f, &cookie, "/api/records/references", query).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let frame = &body["sources"][2];
    let read = &frame["native"]["published"]["read"];
    assert_eq!(read["outgoing"], Value::Null);
    assert_eq!(read["focused_source"], Value::Null);
    assert_eq!(read["incoming"]["results"].as_array().unwrap().len(), 1);
    assert_eq!(
        read["incoming"]["results"][0]["payload"]["authored"]["target"]["namespace"]["context"],
        "literal target context"
    );
    assert_eq!(
        frame["navigation"]["incoming"][0]["target_rich"],
        Value::Null
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_missing_knowledge_namespace_preserves_incompleteness_without_source_identity()
 {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(30));
    let selected = browser_selection(&tasks);
    native::publish(
        &tasks,
        &raw,
        0,
        &[
            native::page("Source", "Source without projection", &["Target"]),
            native::page("Target", "Target without projection", &[]),
        ],
        None,
    )
    .await;
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let response = post(
        &f,
        &cookie,
        "/api/records/references",
        request(
            &selected,
            "knowledge",
            &json!("Source"),
            json!([{"source":{"kind":"project_knowledge"},"direction":"outgoing"}]),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let read = &body["sources"][0]["native"]["published"]["read"];
    assert_eq!(read["focused_source"]["namespace"], Value::Null);
    assert_eq!(read["focused_source"]["witness"], Value::Null);
    assert_eq!(body["outgoing_complete"], false);
    assert!(!read["warnings"].as_array().unwrap().is_empty());
    assert_eq!(read["outgoing"]["results"], json!([]));
    assert_eq!(body["sources"][0]["navigation"]["outgoing"], json!([]));
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_memory_and_knowledge_cursors_advance_independently_and_bind_exact_source()
 {
    let (_temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
            Permission::Read(Module::Memory),
            Permission::WriteMemory,
        ],
        &[],
        None,
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let selected = browser_selection(&tasks);
    let project = serde_json::from_value(selected["project_id"].clone()).unwrap();
    let paged_memory = memory_fixture::seed(
        &tasks,
        &raw,
        0,
        memory_fixture::Seed::Create(memory(
            "Paged Memory",
            (0..3)
                .map(|n| MemoryReference {
                    target_kind: ReferenceTargetKind::Knowledge,
                    scope: MemoryReferenceScope::Project(project),
                    id: "Target".into(),
                    context: None,
                    relation: format!("uses-{n}"),
                    field: None,
                })
                .collect(),
        )),
    )
    .await;
    let authored = (0..3)
        .map(|n| KnowledgeAuthoredReference {
            target_kind: KnowledgeReferenceTargetKind::Knowledge,
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
            id: "Target",
            relation: ["uses-0", "uses-1", "uses-2"][n],
            field: None,
        })
        .collect::<Vec<_>>();
    let fields = [
        KnowledgeReferenceFields {
            slug: "Source",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project),
                context: None,
            },
            authored: &authored,
        },
        KnowledgeReferenceFields {
            slug: "Target",
            state: KnowledgeDocumentState::Active,
            namespace: KnowledgeReferenceNamespace {
                scope: KnowledgeReferenceScope::Project(project),
                context: None,
            },
            authored: &[],
        },
    ];
    let published = native::publish(
        &tasks,
        &raw,
        0,
        &[
            native::page("Source", "Paged Knowledge", &[]),
            native::page("Target", "Target", &[]),
        ],
        Some(&fields),
    )
    .await;
    let imported = serde_json::to_value(
        native::register_copy(&tasks, &published, "vendor/cursors", "exact-version", 0).await,
    )
    .unwrap();
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let mut continuations = Vec::new();
    for (kind, id, source) in [
        (
            "memory",
            json!(paged_memory.id),
            json!({"kind":"project_memory"}),
        ),
        (
            "knowledge",
            json!("Source"),
            json!({"kind":"project_knowledge"}),
        ),
        ("knowledge", json!("Source"), imported.clone()),
    ] {
        let import = source["kind"] == "imported_knowledge";
        let mut query = request(
            &selected,
            kind,
            &id,
            json!([{"source":source,"direction":"outgoing"}]),
        );
        query["input"]["page_size"] = json!(1);
        if import {
            query.as_object_mut().unwrap().remove("selection");
            query["input"]["outgoing_source"] = source.clone();
            query["input"]["contexts"] = json!([source["artifact"]]);
        }
        let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let first: Value = response.json().await.unwrap();
        let read = if import {
            &first["sources"][0]["native"]["read"]
        } else if kind == "knowledge" {
            &first["sources"][0]["native"]["published"]["read"]
        } else {
            &first["sources"][0]["native"]["published"]
        };
        assert_eq!(read["outgoing"]["results"].as_array().unwrap().len(), 1);
        assert_eq!(first["outgoing_complete"], false);
        let cursor = read["outgoing"]["next_cursor"].clone();
        assert!(cursor.is_object());
        query["input"]["pages"][0]["cursor"] = cursor;
        query["input"]["page_size"] = json!(100);
        let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let tail: Value = response.json().await.unwrap();
        let tail_read = if import {
            &tail["sources"][0]["native"]["read"]
        } else if kind == "knowledge" {
            &tail["sources"][0]["native"]["published"]["read"]
        } else {
            &tail["sources"][0]["native"]["published"]
        };
        assert_eq!(
            tail_read["outgoing"]["results"].as_array().unwrap().len(),
            2
        );
        assert_eq!(tail["outgoing_complete"], true);
        let mut wrong = query.clone();
        wrong["input"]["pages"][0]["direction"] = json!("incoming");
        let response = post(&f, &cookie, "/api/records/references", wrong).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(
            response
                .json::<Value>()
                .await
                .unwrap()
                .get("sources")
                .is_none()
        );
        continuations.push(query);
    }
    let (server, raw) = f.tasks.as_ref().unwrap();
    memory_fixture::seed(
        server,
        raw,
        0,
        memory_fixture::Seed::Create(memory("Advances Memory only", vec![])),
    )
    .await;
    for (index, query) in continuations.iter().enumerate() {
        let response = post(&f, &cookie, "/api/records/references", query.clone()).await;
        if index == 0 {
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert!(
                response
                    .json::<Value>()
                    .await
                    .unwrap()
                    .get("sources")
                    .is_none()
            );
        } else {
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.json::<Value>().await.unwrap()["outgoing_complete"],
                true
            );
        }
    }
    native::update_preserving(
        server,
        raw,
        0,
        &[native::page("Source", "Knowledge changed title", &[])],
    )
    .await;
    let stale = post(
        &f,
        &cookie,
        "/api/records/references",
        continuations[1].clone(),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::BAD_REQUEST);
    assert!(
        stale
            .json::<Value>()
            .await
            .unwrap()
            .get("sources")
            .is_none()
    );
    let retained_import = post(
        &f,
        &cookie,
        "/api/records/references",
        continuations[2].clone(),
    )
    .await;
    assert_eq!(retained_import.status(), StatusCode::OK);
    assert_eq!(
        retained_import.json::<Value>().await.unwrap()["outgoing_complete"],
        true
    );
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_deadline_bounds_accepted_project_wait_and_joins_on_teardown() {
    let (_temp, tasks, raw) = knowledge_workspace_fixture(None, Duration::from_secs(2));
    let selected = browser_selection(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    f.issuer.hold_project.store(true, Ordering::Release);
    let started = f.issuer.project_started.notified();
    let response = f
        .client
        .post(format!("{}/api/records/references", f.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &f.origin)
        .json(&request(
            &selected,
            "memory",
            &json!("logical target"),
            json!([{"source":{"kind":"project_memory"},"direction":"incoming"}]),
        ))
        .send();
    tokio::pin!(started, response);
    tokio::select! {
        _=&mut started=>{},
        unexpected=&mut response=>panic!("completed before PROJECT gate: {unexpected:?}"),
    }
    let bounded = tokio::time::timeout(Duration::from_secs(5), response).await;
    f.issuer.hold_project.store(false, Ordering::Release);
    f.issuer.release_project.notify_one();
    let response = bounded
        .expect("original deadline must bound accepted network wait")
        .unwrap();
    // Deadline may be observed first by the accepted HTTP loop or the PROJECT request.
    assert!(
        response.status().is_server_error() || response.status() == StatusCode::REQUEST_TIMEOUT
    );
    let body: Value = response.json().await.unwrap();
    assert!(body.get("error").is_some());
    assert!(body.get("sources").is_none());
    tokio::time::timeout(Duration::from_secs(5), f.finish_tasks())
        .await
        .expect("accepted jobs must join before teardown");
}

#[tokio::test]
async fn record_references_import_http_manual_zero_edge_focus_keeps_namespace_b_and_physical_producer_a()
 {
    let (temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &["producer"],
        None,
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let producer = browser_selection(&tasks);
    let logical = native::add_current_project(&tasks, &temp.path().join("producer"));
    let project = serde_json::from_value(logical["project_id"].clone()).unwrap();
    let fields = [KnowledgeReferenceFields {
        slug: "Manual zero",
        state: KnowledgeDocumentState::Active,
        namespace: KnowledgeReferenceNamespace {
            scope: KnowledgeReferenceScope::Project(project),
            context: None,
        },
        authored: &[],
    }];
    let published = native::publish(
        &tasks,
        &raw,
        1,
        &[native::page("Manual zero", "Observed manual title", &[])],
        Some(&fields),
    )
    .await;
    let mut source = serde_json::to_value(
        native::register_copy(&tasks, &published, "vendor/manual-zero", "exact-version", 0).await,
    )
    .unwrap();
    source["artifact"]["context_alias"] = json!("mount alias");
    let registry_before = native::registry_bytes(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = post(
        &f,
        &cookie,
        "/api/knowledge/imported/page",
        json!({"input":{"source":source,"slug":"Manual zero"}}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["source"], source);
    let focus = &body["focused_source"];
    assert_eq!(focus.as_object().unwrap().len(), 6);
    assert_eq!(focus["slug"], "Manual zero");
    assert_eq!(focus["title"], "Observed manual title");
    assert_eq!(focus["namespace"]["scope"], json!({"project":project}));
    assert_ne!(
        focus["namespace"]["scope"]["project"],
        producer["project_id"]
    );
    assert_eq!(focus["namespace"]["context"], Value::Null);
    assert_eq!(focus["witness"].as_array().unwrap().len(), 32);
    assert_eq!(body["native"]["page"]["state"], "found");
    let document = &body["native"]["page"]["document"];
    assert_eq!(document.as_object().unwrap().len(), 16);
    assert_eq!(focus["source_path"], document["source_path"]);
    assert_eq!(focus["compilation_digest"], document["compilation_digest"]);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    assert_eq!(
        native::registry_bytes(&f.tasks.as_ref().unwrap().0),
        registry_before
    );
    f.finish_tasks().await;
}

#[tokio::test]
async fn record_references_import_http_manual_published_empty_keeps_absent_focus() {
    let (_temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        None,
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let published = native::publish(&tasks, &raw, 0, &[], None).await;
    let source = native::register_copy(
        &tasks,
        &published,
        "vendor/manual-empty",
        "exact-version",
        0,
    )
    .await;
    let registry_before = native::registry_bytes(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let response = post(
        &f,
        &cookie,
        "/api/knowledge/imported/page",
        json!({"input":{"source":source,"slug":"Absent page"}}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["native"]["state"], "published");
    assert_eq!(body["native"]["page"]["state"], "published_empty");
    assert_eq!(body["focused_source"], Value::Null);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    assert_eq!(
        native::registry_bytes(&f.tasks.as_ref().unwrap().0),
        registry_before
    );
    f.finish_tasks().await;
}

#[tokio::test]
async fn knowledge_imported_lexical_search_uses_registered_pin_without_project_grant() {
    let (_temp, tasks, raw) = crate::task_server::tests::fixture_with_reference_permissions(
        &[
            Permission::ManageAuthProfiles,
            Permission::Read(Module::Knowledge),
            Permission::MutateModule(Module::Knowledge),
        ],
        &[],
        None,
        true,
    );
    let tasks = native::with_import_registry(&tasks);
    let published = native::publish(
        &tasks,
        &raw,
        0,
        &[
            native::page("Imported first", "First title", &[]),
            native::page("Imported second", "Second title", &[]),
        ],
        None,
    )
    .await;
    let source =
        native::register_copy(&tasks, &published, "vendor/search", "exact-version", 0).await;
    let source_value = serde_json::to_value(&source).unwrap();
    let registry_before = native::registry_bytes(&tasks);
    let f = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    f.issuer.project_grant.store(false, Ordering::SeqCst);
    let before = f.issuer.project_calls.load(Ordering::SeqCst);
    let query = json!({"query":"Complete","mode":"fts","preview":true,"page_size":1});
    let response = post(
        &f,
        &cookie,
        "/api/knowledge/imported/search",
        json!({"input":{"source":source_value,"search":query}}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let first: Value = response.json().await.unwrap();
    assert_eq!(first["source"], source_value);
    assert!(first.get("selection").is_none());
    assert_eq!(first["native"]["state"], "hits");
    assert_eq!(first["native"]["total"], 2);
    let hits = first["native"]["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0]["focused_source"]["namespace"].is_null());
    assert!(hits[0]["focused_source"]["witness"].is_null());
    assert!(hits[0]["lexical_score"].as_f64().unwrap().is_finite());
    assert_eq!(hits[0]["preview"]["source"], "summary");
    assert_eq!(hits[0]["preview"]["text"], "complete native summary");
    let cursor = first["native"]["next_cursor"].clone();
    assert!(cursor.is_object());
    let mut continued = query.clone();
    continued["cursor"] = cursor.clone();
    let response = post(
        &f,
        &cookie,
        "/api/knowledge/imported/search",
        json!({"input":{"source":source_value,"search":continued}}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let second: Value = response.json().await.unwrap();
    assert_eq!(
        second["native"]["generation"],
        first["native"]["generation"]
    );
    assert!(second["native"]["next_cursor"].is_null());
    assert_ne!(
        second["native"]["hits"][0]["focused_source"]["slug"],
        hits[0]["focused_source"]["slug"]
    );
    let mut changed = query.clone();
    changed["cursor"] = cursor;
    changed["query"] = json!("different");
    assert_eq!(
        post(
            &f,
            &cookie,
            "/api/knowledge/imported/search",
            json!({"input":{"source":source_value,"search":changed}})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let mut absent_source = source_value;
    absent_source["artifact"]["version"] = json!("not-registered");
    let response = post(
        &f,
        &cookie,
        "/api/knowledge/imported/search",
        json!({"input":{"source":absent_source,"search":query}}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["native"]["state"],
        "registry_absent"
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), before);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(
        native::registry_bytes(&f.tasks.as_ref().unwrap().0),
        registry_before
    );
    f.finish_tasks().await;
}
