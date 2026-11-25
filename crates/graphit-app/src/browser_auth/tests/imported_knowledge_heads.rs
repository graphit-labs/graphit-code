//! Actual browser HTTP catalogue over readonly registered Source captures.
use super::*;
use crate::task_server::{
    browser::{knowledge_fixture, records_fixture as native},
    tests::fixture_with_reference_permissions,
};
use graphit_auth::Permission;
use graphit_core::module_settings::Module;
use std::time::UNIX_EPOCH;

async fn read(fixture: &Fixture, cookie: &str, body: Value) -> reqwest::Response {
    fixture
        .client
        .post(format!("{}/api/knowledge/imported/heads", fixture.origin))
        .header(header::ORIGIN, &fixture.origin)
        .header(header::COOKIE, cookie)
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn imported_knowledge_heads_read_exact_source_without_project_model_or_userinfo() {
    let (_temp, tasks, raw) = fixture_with_reference_permissions(
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
    let body = "native document body ".repeat(2048);
    let mut first = native::page("a-first", " Título literal é ", &[]);
    first.updated_at = None;
    first.body = Some(&body);
    let mut middle = native::page("m-middle", "Middle", &[]);
    middle.updated_at = Some(UNIX_EPOCH + Duration::new(1, 123));
    let mut last = native::page("z-last", "", &[]);
    last.updated_at = Some(UNIX_EPOCH - Duration::new(1, 7));
    let published = native::publish(&tasks, &raw, 0, &[last, first, middle], None).await;
    let mut source = serde_json::to_value(
        native::register_copy(&tasks, &published, "vendor/catalogue", "exact-version", 0).await,
    )
    .unwrap();
    source["artifact"]["context_alias"] = json!(" literal alias ");
    let registry_before = native::registry_bytes(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    *fixture.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    fixture.issuer.project_grant.store(false, Ordering::Release);
    let project_calls = fixture.issuer.project_calls.load(Ordering::SeqCst);
    let response = read(&fixture, &cookie, json!({"input":{"source":source}})).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["source"], source);
    assert_eq!(response.as_object().unwrap().len(), 2);
    assert_eq!(response["native"]["state"], "published");
    assert_eq!(
        response["native"]["generation"]["version"],
        published.default.version
    );
    assert_eq!(
        response["native"]["generation"]["physical_branch"],
        Value::Null
    );
    assert_eq!(
        response["native"]["generation"]["inherited_target"],
        Value::Null
    );
    assert_eq!(
        response["native"]["heads"],
        json!([
            {"slug":"a-first","title":" Título literal é ","updated_at":null},
            {"slug":"m-middle","title":"Middle","updated_at":"1000000123"},
            {"slug":"z-last","title":"","updated_at":"-1000000007"}
        ])
    );
    assert_eq!(
        fixture.issuer.project_calls.load(Ordering::SeqCst),
        project_calls
    );
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(
        native::registry_bytes(&fixture.tasks.as_ref().unwrap().0),
        registry_before
    );
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn imported_knowledge_heads_distinguish_empty_absent_and_browser_boundaries() {
    let (_temp, tasks, raw) = fixture_with_reference_permissions(
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
    let source = serde_json::to_value(
        native::register_copy(
            &tasks,
            &published,
            "vendor/empty-catalogue",
            "exact-version",
            0,
        )
        .await,
    )
    .unwrap();
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let response = read(&fixture, &cookie, json!({"input":{"source":source}})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["native"]["state"], "published");
    assert_eq!(response["native"]["heads"], json!([]));
    let mut absent = source.clone();
    absent["artifact"]["version"] = json!("not-registered");
    let response = read(&fixture, &cookie, json!({"input":{"source":absent}})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["source"], absent);
    assert_eq!(response["native"], json!({"state":"registry_absent"}));
    for body in [
        json!({"input":{"source":{"kind":"project_knowledge"}}}),
        json!({"input":{"source":source,"unknown":true}}),
        json!({"input":{"source":source},"selection":{}}),
        json!({"input":{"source":{"kind":"imported_knowledge","artifact":{"id":"vendor/empty-catalogue","version":"exact-version","context_alias":null}}}}),
    ] {
        assert_eq!(
            read(&fixture, &cookie, body).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        read(
            &fixture,
            "graphit_session=invalid",
            json!({"input":{"source":source}})
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        fixture
            .client
            .post(format!("{}/api/knowledge/imported/heads", fixture.origin))
            .header(header::ORIGIN, "https://other.invalid")
            .header(header::COOKIE, &cookie)
            .json(&json!({"input":{"source":source}}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn imported_knowledge_heads_reject_corrupt_tail_without_a_healthy_prefix() {
    let (_temp, tasks, raw) = fixture_with_reference_permissions(
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
    let mut published = native::publish(
        &tasks,
        &raw,
        0,
        &[
            native::page("a-first", "Valid", &[]),
            native::page("z-tail", "Invalid", &[]),
        ],
        None,
    )
    .await;
    let corrupt_version = knowledge_fixture::corrupt_tail_document_state(&tasks).await;
    assert!(corrupt_version > published.default.version);
    // register_copy clones the supplied revision, not the current head.
    published.default.version = corrupt_version;
    let source = native::register_copy(
        &tasks,
        &published,
        "vendor/corrupt-catalogue",
        "exact-version",
        0,
    )
    .await;
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let response = read(&fixture, &cookie, json!({"input":{"source":source}})).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["error"]["code"], "knowledge_index_failure");
    assert!(response.get("native").is_none());
    fixture.finish_tasks().await;
}
