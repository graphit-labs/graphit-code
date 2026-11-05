//! Signed Web USER reads; seeding is an independent real incoming MCP write.
use super::*;
use crate::task_server::{
    browser::user_memory_fixture as native, tests::fixture_with_user_permissions,
};
use graphit_auth::{Permission, broker::BrokerPolicy};
use graphit_core::module_settings::Module;
use graphit_records::{
    memory_attributes::{MemoryAttributes, MemoryClass},
    memory_store::{CreateMemory, DeleteMemory, UpdateMemory},
};

fn tasks(
    permissions: Option<&[Permission]>,
) -> (tempfile::TempDir, crate::task_server::TaskServer, String) {
    fixture_with_user_permissions(&[Permission::ManageAuthProfiles], &[], permissions)
}
const READ: [Permission; 2] = [Permission::ReadUser, Permission::Read(Module::Memory)];
const WRITE: [Permission; 4] = [
    Permission::ReadUser,
    Permission::Read(Module::Memory),
    Permission::MutateUser,
    Permission::WriteMemory,
];
fn input(key: &str, mandatory: bool) -> CreateMemory {
    CreateMemory {
        idempotency_key: key.into(),
        title: key.into(),
        body: format!("literal body {key} β"),
        attributes: MemoryAttributes {
            classification: MemoryClass::Fact,
            mandatory,
            important: false,
        },
        tags: vec!["literal".into()],
        references: None,
    }
}
async fn read(f: &Fixture, cookie: &str, operation: &str, input: Value) -> reqwest::Response {
    f.client
        .post(format!("{}/api/user-memory/{operation}", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .json(&json!({"input":input}))
        .send()
        .await
        .unwrap()
}
fn incoming(f: &Fixture, revision: u64) -> BrokerVerifier {
    BrokerVerifier::new(
        BrokerPolicy {
            endpoint: &f.issuer.origin,
            mcp_resource: Some(native::RESOURCE),
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
fn seed_token(f: &Fixture, subject: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    f.issuer.jwt(json!({"iss":f.issuer.origin,"sub":subject,"aud":native::RESOURCE,"client_id":"native-client","jti":"actual-incoming-seed","token_use":"access","iat":now,"exp":now+600}))
}

#[tokio::test]
async fn native_web_user_search_keeps_published_vectors_without_project_io() {
    use crate::foreground_source::foreground_memory_jobs_tests::{
        browser_search_fixture, stop_browser_model,
    };

    let (mut model, mut foreground, model_tasks, _model_raw, _) = browser_search_fixture().await;
    let resources = model_tasks.memory_model().unwrap().unwrap();
    let local = resources
        .local
        .as_ref()
        .expect("actual local USER memory model resources");
    let (_temp, server, raw) = tasks(Some(&WRITE));
    server
        .attach_memory_model(Some(Arc::clone(&resources)))
        .unwrap();
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let (server, raw) = f.tasks.as_ref().unwrap();
    let revision = native::provider(server, raw, &f.issuer.origin);
    let verifier = incoming(&f, revision);
    let token = seed_token(&f, "web-subject");
    let mut authored = input("Paris", false);
    authored.body = "Paris is the capital of France.".into();
    let memory = native::seed(server, &verifier, &token, native::Seed::Create(authored)).await;
    let mut excluded = input("excluded", false);
    excluded.tags = vec!["other".into()];
    native::seed(server, &verifier, &token, native::Seed::Create(excluded)).await;
    native::index_for_search(server, &verifier, &token, &resources).await;
    let cookie = f.login().await;
    f.issuer.project_grant.store(false, Ordering::Release);
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let calls = f.issuer.project_calls.load(Ordering::SeqCst);
    let userinfo = f.issuer.userinfo.load(Ordering::SeqCst);
    let search_input = |mode: &str| {
        json!({
            "text":"Paris", "top_k":4, "mode":mode,
            "filter":{"classification":"fact","tag":"literal"}
        })
    };
    for mode in ["fts", "semantic", "hybrid"] {
        let response = read(&f, &cookie, "search", search_input(mode)).await;
        assert_eq!(response.status(), StatusCode::OK, "{mode}");
        let value: Value = response.json().await.unwrap();
        assert!(value.get("selection").is_none());
        assert_eq!(value["currentOwner"]["identity"]["subject"], "web-subject");
        assert_eq!(value["native"]["mode"], mode);
        assert_eq!(value["native"]["state"], "published");
        assert_eq!(value["native"]["total_hits"], 1);
        assert_eq!(
            value["native"]["hits"][0]["current"]["id"],
            json!(memory.id)
        );
        assert!(
            value["native"]["generation"]["sdk_version"]
                .as_u64()
                .unwrap()
                > 0
        );
        if mode == "fts" {
            assert!(value["native"]["hits"][0]["score"].is_number());
        } else {
            assert_eq!(
                value["native"]["model"]["vector_id"],
                json!(local.expected.vector_id())
            );
            assert_eq!(
                value["native"]["model"]["dimensions"],
                local.expected.dimensions().unwrap()
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
    let mut pending = input("unindexed", false);
    pending.body = "Paris has another document.".into();
    let response = read(
        &f,
        &cookie,
        "create",
        serde_json::to_value(pending).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Value = response.json().await.unwrap();
    assert_eq!(receipt["native"]["state"], "completed");
    let pending_id: graphit_records::memory_store::MemoryId =
        serde_json::from_value(receipt["native"]["id"].clone()).unwrap();
    let indexed_id = json!(memory.id);
    let pending_id = json!(pending_id);
    for mode in ["fts", "semantic", "hybrid"] {
        let response = read(&f, &cookie, "search", search_input(mode)).await;
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["native"]["state"], "published", "{mode}");
        let hits = value["native"]["hits"].as_array().unwrap();
        assert_eq!(hits.len(), if mode == "semantic" { 1 } else { 2 });
        let indexed = hits
            .iter()
            .find(|hit| hit["current"]["id"] == indexed_id)
            .expect("the compatible published vector remains searchable");
        let pending = hits.iter().find(|hit| hit["current"]["id"] == pending_id);
        if mode == "semantic" {
            assert!(indexed["distance"].is_number());
            assert!(pending.is_none());
        } else if mode == "hybrid" {
            let pending = pending.expect("the pending row is already available to FTS");
            assert!(pending["lexical"]["score"].is_number());
            assert!(pending["semantic"].is_null());
            assert!(indexed["semantic"]["distance"].is_number());
        } else {
            assert!(pending.expect("immediate lexical result")["score"].is_number());
        }
    }
    server.attach_memory_model(None).unwrap();
    for mode in ["semantic", "hybrid"] {
        let response = read(&f, &cookie, "search", search_input(mode)).await;
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["native"]["state"], "unavailable");
        assert_eq!(value["native"]["reason"], "model_unavailable");
        assert!(value["native"].get("hits").is_none());
    }
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), calls);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), userinfo);
    f.service.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
    model_tasks.shutdown().await.unwrap();
    drop(resources);
    stop_browser_model(&mut model, &mut foreground).await;
    foreground.shutdown().await.unwrap();
}

#[tokio::test]
async fn web_user_crud_receipts_history_cas_and_no_project_io() {
    let (_temp, server, raw) = tasks(Some(&WRITE));
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let cookie = f.login().await;
    f.issuer.project_grant.store(false, Ordering::Release);
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let calls = f.issuer.project_calls.load(Ordering::SeqCst);
    let authored = input("web-user", false);
    let response = read(
        &f,
        &cookie,
        "create",
        serde_json::to_value(&authored).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Value = response.json().await.unwrap();
    assert_eq!(receipt["native"]["state"], "completed");
    assert_eq!(
        receipt["currentOwner"]["identity"]["subject"],
        "web-subject"
    );
    assert!(receipt.get("selection").is_none());
    assert!(receipt["native"].get("memory").is_none());
    let id = receipt["native"]["id"].clone();
    let replay: Value = read(
        &f,
        &cookie,
        "create",
        serde_json::to_value(&authored).unwrap(),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(replay["native"], receipt["native"]);
    let detail: Value = read(&f, &cookie, "get", json!({"id":id}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(detail["native"]["memory"]["body"], authored.body);
    assert_eq!(
        detail["native"]["creation"]["owner"],
        receipt["currentOwner"]
    );
    assert!(
        !detail["native"]["creation"]["actor"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    let change = json!({"id":id,"expected_revision":1,"idempotency_key":"user-update","reason":"Actual change","body":"changed USER"});
    let response = read(&f, &cookie, "update", change).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["native"]["revision"],
        2
    );
    let response = read(
        &f,
        &cookie,
        "delete",
        json!({"id":id,"expected_revision":9007199254740993u64,
        "idempotency_key":"wrong-revision","reason":"CAS"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let literal = response.text().await.unwrap();
    assert!(literal.contains("9007199254740993"));
    let error: Value = serde_json::from_str(&literal).unwrap();
    assert_eq!(error["error"]["actual"], 2);
    let history: Value = read(&f, &cookie, "history", json!({"id":id}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(history["native"]["revisions"].as_array().unwrap().len(), 1);
    let response = read(
        &f,
        &cookie,
        "delete",
        json!({"id":id,"expected_revision":2,
        "idempotency_key":"user-delete","reason":"Delete"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["native"]["revision"],
        3
    );
    let gone: Value = read(&f, &cookie, "get", json!({"id":id}))
        .await
        .json()
        .await
        .unwrap();
    assert!(gone["native"].is_null());
    let history: Value = read(&f, &cookie, "history", json!({"id":id}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(history["native"]["deleted"], true);
    assert_eq!(history["native"]["revisions"].as_array().unwrap().len(), 2);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), calls);
    f.finish_tasks().await;
}

#[tokio::test]
async fn readonly_web_user_cannot_create_and_client_cannot_supply_actor_or_selection() {
    let (_temp, server, raw) = tasks(Some(&READ));
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let cookie = f.login().await;
    let response = read(
        &f,
        &cookie,
        "create",
        serde_json::to_value(input("readonly", false)).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let mut body = serde_json::to_value(input("spoof", false)).unwrap();
    body["actor"] = json!("someone");
    let response = read(&f, &cookie, "create", body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response=f.client.post(format!("{}/api/user-memory/create",f.origin))
        .header(header::ORIGIN,&f.origin).header(header::COOKIE,&cookie)
        .json(&json!({"selection":{},"input":serde_json::to_value(input("selection",false)).unwrap()})).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
    f.finish_tasks().await;
}

#[tokio::test]
async fn persisted_current_creation_history_filters_delete_and_subject_isolation() {
    let (_temp, server, raw) = tasks(Some(&READ));
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let (server, raw) = f.tasks.as_ref().unwrap();
    let revision = native::provider(server, raw, &f.issuer.origin);
    let verifier = incoming(&f, revision);
    let token = seed_token(&f, "web-subject");
    let first = native::seed(
        server,
        &verifier,
        &token,
        native::Seed::Create(input("first", false)),
    )
    .await;
    let mandatory = native::seed(
        server,
        &verifier,
        &token,
        native::Seed::Create(input("mandatory", true)),
    )
    .await;
    let changed = native::seed(
        server,
        &verifier,
        &token,
        native::Seed::Update(
            first.id,
            UpdateMemory {
                expected_revision: 1,
                idempotency_key: "update".into(),
                reason: "literal reason".into(),
                title: None,
                body: Some("updated body".into()),
                classification: Some(MemoryClass::Decision),
                mandatory: None,
                important: None,
                tags: None,
                references: None,
            },
        ),
    )
    .await;
    let other = native::seed(
        server,
        &verifier,
        &seed_token(&f, "web-subject "),
        native::Seed::Create(input("other literal subject", false)),
    )
    .await;
    let cookie = f.login().await;
    f.issuer.project_grant.store(false, Ordering::SeqCst);
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let response = read(&f, &cookie, "list", json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let list: Value = response.json().await.unwrap();
    assert_eq!(list["native"]["state"], "published");
    assert_eq!(
        list["native"]["memories"],
        serde_json::to_value([&mandatory, &changed]).unwrap()
    );
    assert_eq!(list["native"]["generation"]["physical_branch"], Value::Null);
    assert_eq!(
        list["native"]["generation"]["inherited_target"],
        Value::Null
    );
    assert_eq!(list["currentOwner"]["identity"]["subject"], "web-subject");
    let detail: Value = read(&f, &cookie, "get", json!({"id":first.id}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        detail["native"]["memory"],
        serde_json::to_value(&changed).unwrap()
    );
    assert_eq!(
        detail["native"]["creation"]["memory"],
        serde_json::to_value(&first).unwrap()
    );
    assert_eq!(detail["native"]["creation"]["owner"], list["currentOwner"]);
    let history: Value = read(&f, &cookie, "history", json!({"id":first.id}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        history["native"]["current"],
        serde_json::to_value(&changed).unwrap()
    );
    assert_eq!(history["native"]["revisions"].as_array().unwrap().len(), 1);
    assert_eq!(
        history["native"]["revisions"][0]["before"],
        serde_json::to_value(&first).unwrap()
    );
    assert_eq!(
        history["native"]["revisions"][0]["after"],
        serde_json::to_value(&changed).unwrap()
    );
    let filtered: Value = read(
        &f,
        &cookie,
        "list",
        json!({"exclude_mandatory":true,"classification":"decision","tag":"literal"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        filtered["native"]["memories"],
        serde_json::to_value([&changed]).unwrap()
    );
    let other_read: Value = read(&f, &cookie, "get", json!({"id":other.id}))
        .await
        .json()
        .await
        .unwrap();
    assert!(other_read["native"].is_null());
    native::seed(
        server,
        &verifier,
        &token,
        native::Seed::Delete(
            first.id,
            DeleteMemory {
                expected_revision: 2,
                idempotency_key: "delete".into(),
                reason: "deleted".into(),
            },
        ),
    )
    .await;
    let deleted: Value = read(&f, &cookie, "get", json!({"id":first.id}))
        .await
        .json()
        .await
        .unwrap();
    assert!(deleted["native"].is_null());
    let history: Value = read(&f, &cookie, "history", json!({"id":first.id}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(history["native"]["deleted"], true);
    assert_eq!(history["native"]["revisions"].as_array().unwrap().len(), 2);
    native::seed(
        server,
        &verifier,
        &token,
        native::Seed::Delete(
            mandatory.id,
            DeleteMemory {
                expected_revision: 1,
                idempotency_key: "delete mandatory".into(),
                reason: "empty live catalogue".into(),
            },
        ),
    )
    .await;
    let empty: Value = read(&f, &cookie, "list", json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(empty["native"]["state"], "published");
    assert_eq!(empty["native"]["memories"], json!([]));
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.service.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn default_and_each_missing_user_read_permission_are_denied() {
    for permissions in [None, Some(&READ[..1]), Some(&READ[1..])] {
        let (_temp, server, raw) = tasks(permissions);
        let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
        let cookie = f.login().await;
        assert_eq!(
            read(&f, &cookie, "list", json!({})).await.status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
        f.service.shutdown().await.unwrap();
        f.tasks.as_ref().unwrap().0.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn strict_input_cookie_origin_and_method_keep_user_authority_out_of_body() {
    let (_temp, server, raw) = tasks(Some(&READ));
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let cookie = f.login().await;
    for body in [
        r#"{"input":{},"scope_id":"fake"}"#,
        r#"{"input":{},"selection":{}}"#,
        r#"{"input":{},"input":{}}"#,
        r#"{"input":{"unknown":true}}"#,
        r#"{"input":null}"#,
    ] {
        let r = f
            .client
            .post(format!("{}/api/user-memory/list", f.origin))
            .header(header::ORIGIN, &f.origin)
            .header(header::COOKIE, &cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        read(&f, &cookie, "get", json!({"id":"bad"})).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        read(&f, "graphit_session=forged", "list", json!({}))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let r = f
        .client
        .post(format!("{}/api/user-memory/list", f.origin))
        .header(header::ORIGIN, "https://other.example")
        .header(header::COOKIE, &cookie)
        .json(&json!({"input":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        f.client
            .get(format!("{}/api/user-memory/list", f.origin))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    let oversized = " ".repeat(65537);
    let response = f
        .client
        .post(format!("{}/api/user-memory/list", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(oversized)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
    f.service.shutdown().await.unwrap();
    f.tasks.as_ref().unwrap().0.shutdown().await.unwrap();
}

#[tokio::test]
async fn bounded_output_and_current_provider_removal_never_deliver_partial_user_data() {
    let (_temp, server, raw) = tasks(Some(&READ));
    let server = native::with_limits(&server, 100);
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let cookie = f.login().await;
    let response = read(&f, &cookie, "list", json!({})).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["error"]["code"], "response_limit");
    let (server, raw) = f.tasks.as_ref().unwrap();
    crate::task_server::tests::browser_provider(server, raw, &f.issuer.origin, true);
    assert_eq!(
        read(&f, &cookie, "list", json!({})).await.status(),
        StatusCode::FORBIDDEN
    );
    native::stop(server);
    assert!(
        !read(&f, &cookie, "list", json!({}))
            .await
            .status()
            .is_success()
    );
    f.service.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn expired_access_without_refresh_is_rejected_before_user_read() {
    let (_temp, server, raw) = tasks(Some(&READ));
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    // JWT timestamps have whole-second precision; leave a full second for login.
    f.issuer.access_seconds.store(2, Ordering::SeqCst);
    f.issuer.issue_refresh.store(false, Ordering::SeqCst);
    let cookie = f.login().await;
    wait_until_access_expiration(&f).await;
    assert_eq!(
        read(&f, &cookie, "list", json!({})).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
    f.service.shutdown().await.unwrap();
    f.tasks.as_ref().unwrap().0.shutdown().await.unwrap();
}

#[tokio::test]
async fn actual_corrupt_user_manifest_returns_causal_error_without_partial_success() {
    let (_temp, server, raw) = tasks(Some(&READ));
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let (server, raw) = f.tasks.as_ref().unwrap();
    let revision = native::provider(server, raw, &f.issuer.origin);
    let verifier = incoming(&f, revision);
    let token = seed_token(&f, "web-subject");
    let memory = native::seed(
        server,
        &verifier,
        &token,
        native::Seed::Create(input("corrupt", false)),
    )
    .await;
    let cookie = f.login().await;
    native::corrupt(server, &f.issuer.origin, "web-subject");
    for (operation, input) in [
        ("list", json!({})),
        ("get", json!({"id":memory.id})),
        ("history", json!({"id":memory.id})),
    ] {
        let response = read(&f, &cookie, operation, input).await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let value: Value = response.json().await.unwrap();
        assert!(value.get("native").is_none());
        assert!(matches!(
            value["error"]["code"].as_str(),
            Some("store_failure" | "branch_failure" | "artifact_failure")
        ));
    }
    f.service.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn user_runtime_symlink_is_not_followed_into_read_payload() {
    let (_temp, server, raw) = tasks(Some(&READ));
    let f = Fixture::with_tasks(policy(), Some((server, raw))).await;
    let (server, raw) = f.tasks.as_ref().unwrap();
    let revision = native::provider(server, raw, &f.issuer.origin);
    let verifier = incoming(&f, revision);
    native::seed(
        server,
        &verifier,
        &seed_token(&f, "web-subject"),
        native::Seed::Create(input("physical", false)),
    )
    .await;
    let cookie = f.login().await;
    let runtime = native::user_runtime(server, &f.issuer.origin, "web-subject");
    let hidden = runtime.with_file_name("moved-runtime");
    std::fs::rename(&runtime, &hidden).unwrap();
    std::os::unix::fs::symlink(&hidden, &runtime).unwrap();
    let response = read(&f, &cookie, "list", json!({})).await;
    assert!(!response.status().is_success());
    let value: Value = response.json().await.unwrap();
    assert!(value.get("native").is_none());
    std::fs::remove_file(&runtime).unwrap();
    std::fs::rename(&hidden, &runtime).unwrap();
    f.service.shutdown().await.unwrap();
    server.shutdown().await.unwrap();
}
