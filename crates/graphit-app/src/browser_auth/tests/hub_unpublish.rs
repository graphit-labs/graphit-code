//! Real Web catalog CAS, authority and effects without a local publication source.
use super::*;
use futures_util::FutureExt;
use graphit_auth::{
    CredentialSource, Permission, RemoteServiceBindingInput, ServiceOrigin, ServicePurpose,
    ServiceRuleInput,
};
use graphit_core::{brand::Brand, module_settings::Module, project_identity::ProjectId};
use graphit_hub::{ArtifactId, ArtifactType, NativeBinding, policy::RequestContext};
use hub_remote_catalog::{page, request};
use object_store::{ObjectStore, ObjectStoreExt as _, path::Path as ObjectPath};
use std::collections::BTreeMap;

const UNPUBLISH: &str = "/api/hub/unpublish";
const FILE_ID: &str = "fixture/web-file-reader";
const TEXT: &str = "---\nname: web-file-reader\ndescription: Actual SDK archive fixture\n---\nPreserved publication payload\n";

async fn selected_payload_keys(
    fixture: &crate::source_server::test_fixture::Fixture,
    store: &dyn ObjectStore,
    prefix: &str,
    producer: ProjectId,
    expected_releases: usize,
) -> Vec<ObjectPath> {
    let policy = &fixture.server.policy().native_install.hub;
    let cancel = Cancellation::default();
    let original = checked(
        RequestContext::new(
            policy.content(),
            &cancel,
            Instant::now() + Duration::from_secs(30),
        ),
        "fixture catalog Original",
    );
    let id = checked(
        ArtifactId::new(FILE_ID, policy.projection()),
        "published artifact ID",
    );
    let catalog = checked(
        crate::native_publication::read_catalog(
            store,
            crate::native_publication::CatalogIdentity {
                storage_prefix: prefix,
                producer,
                artifact_type: ArtifactType::Skill,
                id: &id,
                limits: crate::native_publication::metadata_limits(
                    policy.content(),
                    policy.projection(),
                ),
            },
            &original,
            &|| original.check(),
        )
        .await,
        "actual SDK catalog capture",
    );
    let manifest = catalog.current.expect("four published SDK releases");
    assert_eq!(
        manifest.versions().len(),
        expected_releases,
        "declared fixture releases"
    );
    manifest
        .versions()
        .iter()
        .map(|version| {
            let release = checked(
                graphit_hub::remote_storage::release_prefix(
                    producer,
                    ArtifactType::Skill,
                    &id,
                    version.version(),
                    version.hash(),
                ),
                "actual SDK immutable release prefix",
            );
            checked(
                ObjectPath::parse(format!("{prefix}/{release}payload.zip")),
                "actual FILE publisher payload key",
            )
        })
        .collect()
}
async fn retained_objects(
    store: &dyn ObjectStore,
    payload_keys: &[ObjectPath],
) -> BTreeMap<String, Vec<u8>> {
    assert!(
        !payload_keys.is_empty() && payload_keys.len() <= 4,
        "bounded declared immutable payload set"
    );
    let mut result = BTreeMap::new();
    for key in payload_keys {
        let object = checked(store.get(key).await, "actual retained fixture payload");
        let bytes = checked(object.bytes().await, "actual retained fixture bytes");
        assert!(result.insert(key.to_string(), bytes.to_vec()).is_none());
    }
    result
}
async fn retracted(host: &WebHost, cookie: &str, input: &Value) -> Value {
    let response = request(host, Some(cookie), UNPUBLISH, &input.to_string()).await;
    assert_eq!(response.status(), StatusCode::OK, "real unpublish response");
    assert!(
        response.headers()[header::CACHE_CONTROL]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    let body = checked(response.bytes().await, "actual unpublish response bytes");
    let value: Value = checked(
        serde_json::from_slice(&body),
        "typed remote unpublish receipt",
    );
    assert_eq!(value["status"], "unpublished");
    for private in [
        "access_token",
        "refresh_token",
        "secret_access_key",
        "session_token",
        "authorization_revision",
        "subject",
        "endpoint",
        "hash",
    ] {
        assert!(
            !String::from_utf8_lossy(&body).contains(&format!("\"{private}\"")),
            "no private transport fields"
        );
    }
    value
}

async fn failed(
    host: &WebHost,
    cookie: Option<&str>,
    input: &Value,
    expected: StatusCode,
) -> Value {
    let response = request(host, cookie, UNPUBLISH, &input.to_string()).await;
    assert_eq!(response.status(), expected, "finite unpublish status");
    assert!(
        response.headers()[header::CACHE_CONTROL]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    checked(response.json().await, "bounded unpublish failure")
}

async fn joined_request(
    request: &mut Option<tokio::task::JoinHandle<reqwest::Response>>,
    label: &'static str,
) -> reqwest::Response {
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        request.as_mut().expect("owned request handle"),
    )
    .await;
    match result {
        Ok(joined) => {
            request.take();
            checked(joined, label)
        }
        Err(_) => {
            let job = request.take().expect("owned timed-out handle");
            job.abort();
            let _ = job.await;
            panic!("{label}: request timed out after abort/join");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO fixture and coordinated App emission"]
async fn real_web_unpublish_version_cas_preserves_payload_cache_and_authority() {
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let producer: ProjectId = checked(
        "01D39ZY06FGSCTVN4T2V9PKHFZ".parse(),
        "actual remote producer",
    );
    let second: ProjectId = checked(
        "01D39ZY06FGSCTVN4T2V9PKHG0".parse(),
        "independent readonly producer",
    );
    let mut broker = BrokerProcess::start_web_selected(
        std::path::Path::new(&binary),
        &producer.to_string(),
        Some(&second.to_string()),
        false,
        false,
        true,
    )
    .await;
    let mut fixture_owner = None;
    let mut host = None;
    let mut task_owner = None;
    let mut task_directory = None;
    let mut limited_host = None;
    let mut limited_tasks = None;
    let mut timeout_host = None;
    let mut timeout_tasks = None;
    let mut held_sts = false;
    let mut pending_request = None;
    let observed = std::panic::AssertUnwindSafe(Box::pin(async {
        let rules: Vec<_> = [ServicePurpose::HubRead, ServicePurpose::HubWrite].into_iter().map(|purpose| ServiceRuleInput::Remote {
            origin: ServiceOrigin::Web,
            binding: RemoteServiceBindingInput::Broker { provider: "broker", purpose, endpoint: wire(&broker.ready, "endpoint"), resource: None },
            credential: CredentialSource::IncomingBearer, incoming: None,
        }).collect();
        let fixture = crate::source_server::test_fixture::fixture_with_web_read_providers(
            &[Permission::Read(Module::Hub), Permission::Configure], &[Permission::ManageAuthProfiles],
            Brand::default(), NativeBinding::KiroV3, &["unrelated-local-project"], &rules, (None, Some("broker")));
        drop(rules);
        fixture_owner = Some(configure_hub_fixture(fixture).await);
        let fixture = fixture_owner.as_ref().expect("owned installation");
        assert_ne!(fixture.projects[0].1, producer, "local PROJECT never supplies remote identity");
        let store = seed_store(&broker.ready, producer).await;
        let prefix = wire(&broker.ready, "prefix").to_owned();
        let long_version = "release/".to_owned() + &"oversized".repeat(10);
        let mut first_digest = String::new();
        for version in ["release/one", "release/two", "release/retained", long_version.as_str()] {
            let receipt = hub_file_reads::seed_file(fixture, store.as_ref(), &prefix, producer, version, TEXT).await;
            if version == "release/one" { first_digest = receipt.archive_sha256; }
        }
        let payload_keys = selected_payload_keys(fixture, store.as_ref(), &prefix, producer, 4).await;
        let retained = retained_objects(store.as_ref(), &payload_keys).await;
        assert_eq!(retained.len(), 4, "four real SDK releases retained");
        let revision = crate::graph_server::tests::browser_provider(&fixture.server.graph_server(), &fixture.raw, wire(&broker.ready, "endpoint"));
        let (directory, tasks, _) = crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
        task_directory = Some(directory); task_owner = Some(tasks.clone());
        host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let web = host.as_ref().unwrap();
        let cookie = web.login(&broker.ready).await;
        let sibling = web.login(&broker.ready).await;
        let mut readonly_ready = broker.ready.clone();
        assert!(readonly_ready["web_second"].is_object(), "second real LOCAL identity required");
        readonly_ready["web"] = broker.ready["web_second"].clone();
        let readonly_cookie = web.login(&readonly_ready).await;
        let identity: Value = checked(checked(web.client.get(format!("{}/auth/session", web.origin)).header(header::COOKIE, cookie.as_str()).send().await, "actual Web identity").json().await, "verified subject");
        assert_ne!(wire(&identity, "subject"), wire(&broker.ready["web_second"], "subject"));
        // An existing installation is evidence to preserve, not remote authority.
        hub_file_publication::administrators(fixture, wire(&broker.ready, "issuer"), &[(wire(&identity, "subject"), &["global"])]);
        let installed = page(web, &cookie, "/api/hub/files/install", json!({"selection":{"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"version":"release/one","exact":true},"scope":{"kind":"global_cache"}})).await;
        assert_eq!(installed["status"], "ok");
        hub_file_publication::administrators(fixture, wire(&broker.ready, "issuer"), &[]);
        let registry_before = hub_file_reads::registry_bytes(&fixture.server.policy().registry.path());
        assert!(registry_before.is_some(), "preexisting installation registry");
        let project_root = fixture.projects[0].0.root();
        checked(std::fs::rename(project_root, project_root.with_extension("unavailable")), "make unrelated local source unavailable");
        let children_before = hub_file_reads::installation_children(fixture.paths.root());
        let mut input = json!({"provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"target":{"kind":"version","version":"release/two"}});
        failed(web, None, &input, StatusCode::UNAUTHORIZED).await;
        for body in ["{}", r#"{"provider":null}"#, r#"{"provider":"broker","provider":"broker"}"#] {
            assert_eq!(request(web, Some(&cookie), UNPUBLISH, body).await.status(), StatusCode::BAD_REQUEST);
        }
        for field in ["project_dir", "project_version", "source_id", "profile", "unknown"] {
            let mut invalid = input.clone(); invalid[field] = json!("forbidden");
            failed(web, Some(&cookie), &invalid, StatusCode::BAD_REQUEST).await;
        }
        let maximum_input = task_owner.as_ref().unwrap().browser_policy().0.max_input_bytes.min(fixture.server.policy().native_install.hub.content().max_input_bytes);
        assert_eq!(request(web, Some(&cookie), UNPUBLISH, &" ".repeat(maximum_input.checked_add(1).unwrap())).await.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let mut wrong = input.clone(); wrong["provider"] = json!("other");
        failed(web, Some(&cookie), &wrong, StatusCode::FORBIDDEN).await;
        wrong = input.clone(); wrong["target"]["version"] = json!("latest");
        failed(web, Some(&cookie), &wrong, StatusCode::BAD_REQUEST).await;
        wrong = input.clone(); wrong["target"]["expected_sha256"] = Value::Null;
        failed(web, Some(&cookie), &wrong, StatusCode::BAD_REQUEST).await;
        failed(web, Some(&readonly_cookie), &input, StatusCode::FORBIDDEN).await;
        wrong = input.clone(); wrong["target"] = json!({"kind":"version","version":"release/one","expected_sha256":"00".repeat(32)});
        let conflict = failed(web, Some(&cookie), &wrong, StatusCode::CONFLICT).await;
        assert_eq!(conflict["outcome"]["retraction"]["catalog"], "not_attempted");
        let unpublished = retracted(web, &cookie, &input).await;
        assert_eq!(unpublished["status"], "unpublished");
        assert_eq!(unpublished["retraction"]["catalog"], "updated");
        assert_eq!(unpublished["retraction"]["targeted_versions"], json!(["release/two"]));
        assert_eq!(unpublished["retraction"]["desired_latest"], long_version);
        failed(web, Some(&cookie), &input, StatusCode::NOT_FOUND).await;
        let (mut limits, timeout, _) = task_owner.as_ref().unwrap().browser_policy(); limits.max_output_bytes = 256;
        let tasks = task_owner.as_ref().unwrap().references_test_policy(limits, timeout); limited_tasks = Some(tasks.clone());
        limited_host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let limited = limited_host.as_ref().unwrap(); let limited_cookie = limited.login(&broker.ready).await;
        input["target"] = json!({"kind":"version","version":long_version});
        let response = request(limited, Some(&limited_cookie), UNPUBLISH, &input.to_string()).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(response.headers()["x-graphit-inspection-required"], "request-selection");
        assert!(checked(response.bytes().await, "bounded post-effect retraction").len() <= 256);
        let catalog = page(web, &sibling, "/api/hub/remote/search", json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":10})).await;
        assert_eq!(catalog["artifacts"][0]["latest"], Value::Null, "removing latest does not promote an older version");
        assert_eq!(catalog["artifacts"][0]["versions"], json!(["release/one", "release/retained"]));
        // A real STS response held before catalog I/O makes deadline causal.
        let (limits, _, _) = task_owner.as_ref().unwrap().browser_policy();
        let tasks = task_owner.as_ref().unwrap().references_test_policy(limits, Duration::from_secs(2)); timeout_tasks = Some(tasks.clone());
        timeout_host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let timed = timeout_host.as_ref().unwrap(); let timed_cookie = timed.login(&broker.ready).await;
        input["target"] = json!({"kind":"version","version":"release/one","expected_sha256":first_digest});
        broker.sts_command("hold_sts").await; held_sts = true;
        let send = timed.client.post(format!("{}{UNPUBLISH}", timed.origin)).header(header::ORIGIN, &timed.origin).header(header::COOKIE, timed_cookie.as_str()).header("X-Graphit-Request", "ui").json(&input);
        pending_request = Some(tokio::spawn(async move { checked(send.send().await, "deadline-owned unpublish response") }));
        broker.sts_command("await_sts").await;
        let response = joined_request(&mut pending_request, "deadline request task joined").await;
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        broker.sts_command("release_sts").await; held_sts = false;
        // Logout cancels the same retained Original; another session stays healthy.
        broker.sts_command("hold_sts").await; held_sts = true;
        let send = web.client.post(format!("{}{UNPUBLISH}", web.origin)).header(header::ORIGIN, &web.origin).header(header::COOKIE, cookie.as_str()).header("X-Graphit-Request", "ui").json(&input);
        pending_request = Some(tokio::spawn(async move { checked(send.send().await, "logout-owned unpublish response") }));
        broker.sts_command("await_sts").await;
        let logout = checked(web.client.post(format!("{}/auth/logout", web.origin)).header(header::ORIGIN, &web.origin).header(header::COOKIE, cookie.as_str()).header("X-Graphit-Request", "ui").send().await, "actual logout during retained STS");
        assert_eq!(logout.status(), StatusCode::NO_CONTENT);
        let response = joined_request(&mut pending_request, "logout request task joined").await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        broker.sts_command("release_sts").await; held_sts = false;
        // A separate real Browser owner is closed while STS remains held. Only
        // after its jobs join do we release STS and inspect through the sibling.
        let disconnected_host = timeout_host.as_ref().expect("independent response owner");
        let disconnected_cookie = disconnected_host.login(&broker.ready).await;
        broker.sts_command("hold_sts").await; held_sts = true;
        let send = disconnected_host.client.post(format!("{}{UNPUBLISH}", disconnected_host.origin)).header(header::ORIGIN, &disconnected_host.origin).header(header::COOKIE, disconnected_cookie.as_str()).header("X-Graphit-Request", "ui").json(&input);
        pending_request = Some(tokio::spawn(async move { checked(send.send().await, "disconnect-owned unpublish response") }));
        broker.sts_command("await_sts").await;
        let disconnected = pending_request.take().unwrap(); disconnected.abort();
        assert!(disconnected.await.is_err_and(|error| error.is_cancelled()));
        timeout_host.take().expect("retained response owner").close().await;
        broker.sts_command("release_sts").await; held_sts = false;
        let healthy = page(web, &sibling, "/api/hub/remote/search", json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":10})).await;
        assert_eq!(healthy["artifacts"][0]["versions"], catalog["artifacts"][0]["versions"]);
        // Whole-artifact removal uses the genuine Web writer and remote catalog
        // authority, with the unrelated local source still unavailable.
        assert!(!project_root.exists());
        input["target"] = json!({"kind":"artifact"});
        let whole = retracted(web, &sibling, &input).await;
        assert_eq!(whole["status"], "unpublished");
        assert_eq!(whole["retraction"]["catalog"], "removed");
        assert_eq!(whole["retraction"]["targeted_versions"], healthy["artifacts"][0]["versions"]);
        assert_eq!(whole["retraction"]["desired_latest"], Value::Null);
        let absent = page(web, &sibling, "/api/hub/remote/search", json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":10})).await;
        assert_eq!(absent["artifacts"], json!([]));
        failed(web, Some(&sibling), &input, StatusCode::NOT_FOUND).await;
        // Re-seeding is setup through the actual publisher, not authority lent to
        // Web unpublish. Restore the unrelated source only for that setup and
        // make it unavailable again before the last-version operation.
        checked(std::fs::rename(project_root.with_extension("unavailable"), project_root), "restore owned fixture source for reseed");
        let republished = hub_file_reads::seed_file(fixture, store.as_ref(), &prefix, producer, "release/last", TEXT).await;
        checked(std::fs::rename(project_root, project_root.with_extension("unavailable")), "make reseeded fixture source unavailable");
        let last_payload_keys = selected_payload_keys(fixture, store.as_ref(), &prefix, producer, 1).await;
        let last_payload = retained_objects(store.as_ref(), &last_payload_keys).await;
        let single = page(web, &sibling, "/api/hub/remote/search", json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":10})).await;
        assert_eq!(single["artifacts"][0]["versions"], json!(["release/last"]));
        assert_eq!(single["artifacts"][0]["latest"], "release/last");
        assert!(!project_root.exists());
        input["target"] = json!({"kind":"version","version":"release/last","expected_sha256":republished.archive_sha256});
        let last = retracted(web, &sibling, &input).await;
        assert_eq!(last["status"], "unpublished");
        assert_eq!(last["retraction"]["catalog"], "removed");
        assert_eq!(last["retraction"]["targeted_versions"], json!(["release/last"]));
        assert_eq!(last["retraction"]["desired_latest"], Value::Null);
        let absent = page(web, &sibling, "/api/hub/remote/search", json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":10})).await;
        assert_eq!(absent["artifacts"], json!([]));
        failed(web, Some(&sibling), &input, StatusCode::NOT_FOUND).await;
        assert!(retained_objects(store.as_ref(), &last_payload_keys).await == last_payload, "new last-version payload bytes and key retained after catalog deletion");
        let cached = page(web, &sibling, "/api/hub/files/content", json!({"installed":{"scope":{"kind":"global_cache"},"artifact_type":"skill","id":FILE_ID,"version":"release/one","path":"SKILL.md"}})).await;
        assert_eq!(cached["content"]["content"], TEXT);
        assert!(retained_objects(store.as_ref(), &payload_keys).await == retained, "four original immutable payload bytes and keys retained");
        assert!(hub_file_reads::registry_bytes(&fixture.server.policy().registry.path()) == registry_before, "installed tuple unchanged");
        assert!(hub_file_reads::installation_children(fixture.paths.root()) == children_before, "installation top-level children unchanged");
    })).catch_unwind().await;
    let request_cleanup = std::panic::AssertUnwindSafe(async {
        if held_sts {
            broker.sts_command("release_sts").await;
        }
        if let Some(mut job) = pending_request {
            job.abort();
            if tokio::time::timeout(Duration::from_secs(45), &mut job)
                .await
                .is_err()
            {
                job.abort();
                let _ = job.await;
                panic!("owned unpublish cleanup request remained after abort/join");
            }
        }
    })
    .catch_unwind()
    .await;
    let additional = std::panic::AssertUnwindSafe(async {
        for web in [timeout_host, limited_host].into_iter().flatten() {
            web.close().await;
        }
    })
    .catch_unwind()
    .await;
    let cleanup = std::panic::AssertUnwindSafe(finish_remote_test_owners(
        observed.and(request_cleanup).and(additional),
        host,
        fixture_owner.as_ref().map(|fixture| &fixture.server),
        broker,
    ))
    .catch_unwind()
    .await;
    let tasks = std::panic::AssertUnwindSafe(async {
        for tasks in [timeout_tasks, limited_tasks, task_owner]
            .into_iter()
            .flatten()
        {
            checked(tasks.shutdown().await, "unpublish Task owner joined");
        }
    })
    .catch_unwind()
    .await;
    drop(task_directory);
    for result in [cleanup, tasks] {
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
