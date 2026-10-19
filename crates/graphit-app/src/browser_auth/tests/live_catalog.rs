//! Actual signed Web sessions and independent USER/PROJECT catalogue admissions.
use super::*;
use crate::task_server::{
    browser::records_fixture as native,
    tests::{browser_selection, fixture_with_reference_permissions},
};
use graphit_auth::Permission;
use graphit_core::{file_lock::Cancellation, module_settings::Module};
use graphit_hub::{ArtifactType, ConcreteVersion, registry::ArtifactDisplayMetadata};

async fn read(fixture: &Fixture, cookie: &str) -> reqwest::Response {
    fixture
        .client
        .get(format!("{}/api/live/artifacts", fixture.origin))
        .header(header::ORIGIN, &fixture.origin)
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn live_catalog_uses_exact_installed_metadata_and_fresh_independent_project_grants() {
    let (temp, tasks, raw) = fixture_with_reference_permissions(
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
    let first = browser_selection(&tasks);
    let published = native::publish(
        &tasks,
        &raw,
        0,
        &[native::page(
            "private-body",
            "Body must not be read for metadata",
            &[],
        )],
        None,
    )
    .await;
    native::register_copy(&tasks, &published, "vendor/catalogue", "exact-version", 0).await;
    native::register_copy(&tasks, &published, "vendor/catalogue", "second-version", 0).await;
    {
        // Author metadata enters the real SDK writer; this does not qualify the remote installer.
        let snapshot = tasks
            .record_snapshot()
            .unwrap_or_else(|_| panic!("fixture record snapshot unavailable"));
        let resources = tasks.record_resources(&snapshot);
        let registry = &resources
            .policy
            .imported_knowledge
            .as_ref()
            .unwrap()
            .registry;
        let document = registry.load().unwrap();
        let cancellation = Cancellation::default();
        let mut writer = registry
            .writer(Duration::from_secs(5), &cancellation)
            .unwrap();
        for (version, name, description, tags) in [
            (
                "exact-version",
                "Published Catalogue",
                "**Published** description é",
                vec!["rust".to_owned(), "catalogue".to_owned()],
            ),
            (
                "second-version",
                "Second Catalogue",
                "Another literal description",
                vec!["second-only".to_owned()],
            ),
        ] {
            let version = ConcreteVersion::new(version, registry.limits().projection()).unwrap();
            let mut installed = document
                .get_version(ArtifactType::Knowledge, "vendor/catalogue", &version)
                .unwrap()
                .clone();
            installed.installations[0].display = Some(
                ArtifactDisplayMetadata::new(name, Some(description), &tags, registry.limits())
                    .unwrap(),
            );
            assert!(writer.put_metadata(installed).unwrap());
        }
    }
    let second = native::add_current_project(&tasks, &temp.path().join("other"));
    let before = native::registry_bytes(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    *fixture.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;

    let response = read(&fixture, &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body.as_object().unwrap().len(), 4);
    assert_eq!(body["errors"], json!([]));
    assert_eq!(body["projects"].as_array().unwrap().len(), 2);
    assert_eq!(body["bindings"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["bindings"][0]["binding"],
        serde_json::to_value(graphit_hub::NativeBinding::KiroV3).unwrap()
    );
    assert!(body["bindings"][0].get("preview").is_none());
    assert_eq!(body["bindings"][0]["available"], false);
    assert_eq!(body["bindings"][0]["model_selection"], "explicit");
    let installed = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["ref"]["source"] == "hub" && entry["ref"]["version"] == "exact-version")
        .unwrap();
    assert_eq!(
        installed["ref"],
        json!({
            "source":"hub", "project_id":first["project_id"],
            "instance_id":null, "project_kind":null, "type":"knowledge",
            "id":"vendor/catalogue", "version":"exact-version"
        })
    );
    assert_eq!(installed["name"], "Published Catalogue");
    assert_eq!(installed["description"], "**Published** description é");
    assert_eq!(installed["tags"], json!(["rust", "catalogue"]));
    let other_version = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["ref"]["source"] == "hub" && entry["ref"]["version"] == "second-version"
        })
        .unwrap();
    assert_eq!(other_version["ref"]["id"], installed["ref"]["id"]);
    assert_eq!(other_version["name"], "Second Catalogue");
    assert_eq!(other_version["description"], "Another literal description");
    assert_eq!(other_version["tags"], json!(["second-only"]));
    let project_entries: Vec<_> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["ref"]["source"] == "project")
        .collect();
    let published_entry = project_entries
        .iter()
        .find(|entry| entry["ref"]["project_id"] == first["project_id"])
        .unwrap();
    assert!(published_entry.get("unavailable").is_none());
    assert!(
        published_entry["project_name"]
            .as_str()
            .is_some_and(|name| !name.is_empty())
    );
    assert!(
        published_entry["name"]
            .as_str()
            .unwrap()
            .contains(published_entry["project_name"].as_str().unwrap())
    );
    let absent_entry = project_entries
        .iter()
        .find(|entry| entry["ref"]["project_id"] == second["project_id"])
        .unwrap();
    assert_eq!(
        absent_entry["unavailable"],
        "project_knowledge_source_unavailable"
    );
    assert!(!body.to_string().contains("private-body"));
    assert!(!body.to_string().contains(temp.path().to_str().unwrap()));

    // A Hub PROJECT grant cannot stand in for a Knowledge PROJECT grant.
    *fixture.issuer.denied_modules.lock().unwrap() = vec!["knowledge".into()];
    let response = read(&fixture, &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["projects"].as_array().unwrap().len(), 2);
    assert!(
        body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| { entry["ref"]["source"] != "project" })
    );
    assert!(
        body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| { entry["ref"]["source"] == "hub" })
    );

    *fixture.issuer.denied_modules.lock().unwrap() = vec!["hub".into()];
    fixture
        .issuer
        .denied_projects
        .lock()
        .unwrap()
        .push(second["project_id"].clone());
    let response = read(&fixture, &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["projects"],
        json!([{"project_id":first["project_id"],"project_version":first["project_version"]}])
    );
    let project = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["ref"]["source"] == "project")
        .unwrap();
    assert_eq!(
        project["ref"],
        json!({
            "source":"project", "project_id":first["project_id"],
            "instance_id":null, "project_kind":null, "type":"knowledge",
            "id":first["project_id"], "version":first["project_version"]
        })
    );
    assert!(
        !body
            .to_string()
            .contains(second["project_id"].as_str().unwrap())
    );

    fixture.issuer.project_grant.store(false, Ordering::Release);
    let response = read(&fixture, &cookie).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["projects"], json!([]));
    assert_eq!(body["entries"].as_array().unwrap().len(), 2);
    assert!(
        body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["ref"]["source"] == "hub")
    );
    assert!(fixture.issuer.project_calls.load(Ordering::SeqCst) > 0);
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(
        native::registry_bytes(&fixture.tasks.as_ref().unwrap().0),
        before
    );
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn live_catalog_preserves_cookie_origin_and_payload_boundaries() {
    let (_temp, tasks, raw) =
        fixture_with_reference_permissions(&[Permission::ManageAuthProfiles], &[], None, true);
    let tasks = native::with_import_registry(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let url = format!("{}/api/live/artifacts", fixture.origin);
    assert_eq!(
        read(&fixture, "graphit_session=invalid").await.status(),
        StatusCode::UNAUTHORIZED
    );
    for request in [
        fixture
            .client
            .get(&url)
            .header(header::ORIGIN, "https://other.invalid"),
        fixture
            .client
            .get(&url)
            .header("sec-fetch-site", "cross-site")
            .header("sec-fetch-mode", "cors"),
    ] {
        let response = request
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    let response = fixture
        .client
        .get(&url)
        .header("sec-fetch-site", "same-origin")
        .header("sec-fetch-mode", "cors")
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    for request in [
        fixture.client.get(format!("{url}?project_id=untrusted")),
        fixture.client.get(&url).body("{}"),
    ] {
        let response = request
            .header(header::ORIGIN, &fixture.origin)
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    let maximum = native::content_limits(&fixture.tasks.as_ref().unwrap().0).max_input_bytes;
    let calls = fixture.issuer.project_calls.load(Ordering::SeqCst);
    let response = fixture
        .client
        .get(&url)
        .header(header::ORIGIN, &fixture.origin)
        .header(header::COOKIE, &cookie)
        .body(vec![b'x'; maximum + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), calls);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn live_catalog_requires_user_hub_admission_even_with_a_valid_jwt_and_project_grants() {
    let (_temp, tasks, raw) =
        fixture_with_reference_permissions(&[Permission::ManageAuthProfiles], &[], None, false);
    let tasks = native::with_import_registry(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let calls = fixture.issuer.project_calls.load(Ordering::SeqCst);
    let response = read(&fixture, &cookie).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(fixture.issuer.project_calls.load(Ordering::SeqCst), calls);
    assert_eq!(fixture.issuer.userinfo.load(Ordering::SeqCst), 0);
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn live_catalog_logout_cancels_an_inflight_project_admission() {
    let (_temp, tasks, raw) =
        fixture_with_reference_permissions(&[Permission::ManageAuthProfiles], &[], None, true);
    let tasks = native::with_import_registry(&tasks);
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    fixture.issuer.hold_project.store(true, Ordering::Release);
    let request = read(&fixture, &cookie);
    let logout = async {
        tokio::time::timeout(
            Duration::from_secs(5),
            fixture.issuer.project_started.notified(),
        )
        .await
        .unwrap();
        let response = fixture
            .client
            .post(format!("{}/auth/logout", fixture.origin))
            .header(header::ORIGIN, &fixture.origin)
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    };
    let (response, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(request, logout)
    })
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    fixture.issuer.hold_project.store(false, Ordering::Release);
    fixture.issuer.release_project.notify_waiters();
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn live_catalog_encoding_limit_returns_no_partial_catalogue() {
    let (_temp, tasks, raw) =
        fixture_with_reference_permissions(&[Permission::ManageAuthProfiles], &[], None, true);
    let tasks = native::with_import_registry(&tasks);
    let mut content = native::content_limits(&tasks);
    content.max_output_bytes = 64;
    let tasks = tasks.references_test_policy(content, Duration::from_secs(5));
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    // Even without PROJECT entries, the complete catalogue must fit the reply limit.
    fixture.issuer.project_grant.store(false, Ordering::Release);
    let response = read(&fixture, &cookie).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "response_limit");
    assert!(body.get("entries").is_none());
    fixture.finish_tasks().await;
}

#[tokio::test]
async fn live_catalog_arrival_deadline_includes_waiting_for_the_session_lock() {
    let (_temp, tasks, raw) =
        fixture_with_reference_permissions(&[Permission::ManageAuthProfiles], &[], None, true);
    let tasks = native::with_import_registry(&tasks);
    let tasks =
        tasks.references_test_policy(native::content_limits(&tasks), Duration::from_millis(100));
    let fixture = Fixture::with_tasks(policy(), Some((tasks, raw))).await;
    let cookie = fixture.login().await;
    let sessions = fixture.service.0.sessions.lock().await;
    let response = tokio::time::timeout(Duration::from_secs(2), read(&fixture, &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    drop(sessions);
    // A timed-out read does not invalidate a still-active login.
    assert_eq!(fixture.service.0.sessions.lock().await.len(), 1);
    fixture.finish_tasks().await;
}
