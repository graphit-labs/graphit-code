//! Actual Web administration drives the same FILE cache and local journals.
use super::*;
use graphit_auth::{Permission, ServiceOrigin, ServiceRuleInput};
use graphit_core::{
    artifact_store::{ArtifactMode, ArtifactStore, CapturedPublicationScope, CopyPolicy},
    brand::Brand,
    module_settings::Module,
    project_identity::ProjectIdentityStore,
};
use graphit_hub::{
    ArtifactType, InstallationTarget, NativeBinding, SourceOrigin,
    policy::RequestContext,
    registry::{HubRegistry, InstalledBackend},
};
use hub_remote_catalog::{page, put_json, request};

#[path = "hub_file_mutations/authored_local.rs"]
mod authored_local;
#[path = "hub_file_mutations/update_all.rs"]
mod update_all;

const INSTALL: &str = "/api/hub/files/install";
const UPDATE: &str = "/api/hub/files/update";
const REMOVE: &str = "/api/hub/files/remove";
const RECOVER: &str = "/api/hub/files/recover";
const FORGET: &str = "/api/hub/files/forget";
const CONTENT: &str = "/api/hub/files/content";
const FILE_ID: &str = "fixture/web-file-reader";
const ONE: &str =
    "---\nname: web-mutation\ndescription: Real FILE archive\n---\nFirst mutable release\n";
const TWO: &str =
    "---\nname: web-mutation\ndescription: Real FILE archive\n---\nSecond mutable release\n";

/// Fixture setup uses the real administrative ingress and Auth codec/store.
/// The verified session subject remains private to this test's owned files.
fn administrators(
    fixture: &crate::source_server::test_fixture::Fixture,
    issuer_name: &str,
    entries: &[(&str, &[&str])],
) {
    use graphit_auth::{
        profile::{AuthDocument, ProfileLimits},
        store::AuthStore,
    };
    use graphit_core::auth_document::{AuthDocumentLimits, AuthDocumentStore};
    let cancel = Cancellation::default();
    let original = checked(
        RequestContext::new(
            fixture.server.policy().content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        ),
        "private policy fixture Original",
    );
    let issuer = fixture.auth.issuer();
    let caller = checked(
        issuer.authenticate_runtime(&fixture.raw, &original),
        "policy fixture Runtime caller",
    );
    let session = checked(issuer.bootstrap_admin(&caller), "policy fixture admin");
    let parent = checked(
        issuer.admin_parent(&caller, &session),
        "policy fixture parent",
    );
    checked(
        parent.authorize(Permission::ManageAuthProfiles),
        "policy management authority",
    );
    let limits = ProfileLimits {
        bytes: 65536,
        nodes: 4096,
        scalar_bytes: 4096,
        collection_entries: 64,
        providers: 16,
        profiles: 16,
    };
    let physical = checked(
        AuthDocumentStore::capture(
            fixture.auth.installation().live.clone(),
            checked(
                AuthDocumentLimits::new(65536, 65536, 256, 65536),
                "fixture Auth limits",
            ),
            original.snapshot(),
        ),
        "actual installation Auth store capture",
    );
    let store = checked(
        AuthStore::new(physical, limits, &original),
        "typed policy store",
    );
    let snapshot = checked(store.load(&original), "current policy snapshot");
    let publication = checked(
        store.mutate(snapshot, &original, |document, context| {
            let mut value: Value = serde_json::from_slice(&document.encode(context)?)
                .expect("typed Auth document encoding");
            value["document_revision"] = json!(document.revision().checked_add(1).expect("fixture revision"));
            value["web_hub_administrators"] = entries
                .iter()
                .map(|(subject, scopes)| json!({"provider":"broker","issuer":issuer_name,"subject":subject,"scopes":scopes}))
                .collect();
            *document = AuthDocument::decode(
                &serde_json::to_vec(&value).expect("typed policy encoding"),
                limits,
                context,
            )?;
            Ok(())
        }),
        "publish actual private administrator policy",
    );
    checked(
        parent.authorize(Permission::ManageAuthProfiles),
        "policy authority after writer",
    );
    assert!(
        publication.failure().is_none() && publication.cleanup_failure().is_none(),
        "policy publication confirmed"
    );
    assert!(
        publication.into_pending_cleanup().is_none(),
        "policy publication cleanup joined"
    );
}

async fn failure(
    host: &WebHost,
    cookie: &str,
    path: &str,
    input: &Value,
    status: StatusCode,
) -> Value {
    let response = request(host, Some(cookie), path, &input.to_string()).await;
    assert_eq!(response.status(), status, "finite Web mutation status");
    assert!(
        response.headers()[header::CACHE_CONTROL]
            .to_str()
            .expect("private cache policy")
            .contains("no-store")
    );
    checked(response.json().await, "public mutation failure")
}

async fn limited_response(
    host: &WebHost,
    cookie: &str,
    path: &str,
    input: &Value,
    maximum: usize,
) -> Value {
    let response = request(host, Some(cookie), path, &input.to_string()).await;
    assert_eq!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "actual oversized effect envelope"
    );
    assert!(
        response
            .headers()
            .get("x-graphit-inspection-required")
            .is_some_and(|value| value == "request-selection"),
        "inspect the exact captured request target"
    );
    let bytes = checked(response.bytes().await, "bounded inspection envelope");
    assert!(
        bytes.len() <= maximum,
        "success and error envelopes obey the same real quota"
    );
    let body: Value = checked(
        serde_json::from_slice(&bytes),
        "finite inspection instruction",
    );
    assert!(
        body["inspection_required"] == true,
        "never mistake omitted receipt for no effect"
    );
    body
}

fn project_destination(
    fixture: &crate::source_server::test_fixture::Fixture,
) -> std::path::PathBuf {
    let policy = fixture.server.policy();
    let cancellation = Cancellation::default();
    let original = checked(
        policy
            .native_install
            .hub
            .request_context(Instant::now(), &cancellation),
        "physical observation Original",
    );
    let (paths, id, _) = &fixture.projects[0];
    let scope = checked(
        CapturedPublicationScope::project(paths, *id, policy.scope, original.snapshot()),
        "actual project physical capture",
    );
    let store = ArtifactStore::new(scope, policy.native_install.hub.artifacts());
    let key = checked(
        InstallationTarget::for_artifact(NativeBinding::KiroV3, ArtifactType::Skill)
            .key(ArtifactType::Skill, FILE_ID),
        "actual installed key",
    );
    checked(store.load(), "actual Core catalog")
        .expect("actual physical receipt")
        .entries()
        .iter()
        .find(|entry| entry.key() == &key)
        .expect("actual Web-created projection receipt")
        .destination()
        .to_owned()
}

fn project_registry(fixture: &crate::source_server::test_fixture::Fixture) -> HubRegistry {
    let (paths, _, _) = &fixture.projects[0];
    let identity = checked(
        ProjectIdentityStore::at(paths).load(),
        "current project identity",
    )
    .expect("owned project identity");
    checked(
        HubRegistry::project(
            paths,
            &identity,
            fixture.server.policy().native_install.hub.hub(),
        ),
        "actual project Hub registry",
    )
}

fn project_slot(
    fixture: &crate::source_server::test_fixture::Fixture,
) -> graphit_core::artifact_store::PublicationSlot {
    let policy = fixture.server.policy();
    let cancel = Cancellation::default();
    let request = checked(
        policy
            .native_install
            .hub
            .request_context(Instant::now(), &cancel),
        "project receipt request",
    );
    let (paths, id, _) = &fixture.projects[0];
    checked(
        CapturedPublicationScope::project(paths, *id, policy.scope, request.snapshot()),
        "project receipt scope",
    )
    .publication_slot()
}

fn assert_project_mode(
    fixture: &crate::source_server::test_fixture::Fixture,
    expected: ArtifactMode,
    version: &str,
) {
    let registry = checked(project_registry(fixture).load(), "physical mode registry");
    let entry = registry
        .get_for_consumer(&project_slot(fixture), ArtifactType::Skill, FILE_ID)
        .expect("actual FILE tuple")
        .installations
        .iter()
        .find(|entry| {
            entry.target
                == InstallationTarget::for_artifact(NativeBinding::KiroV3, ArtifactType::Skill)
        })
        .expect("actual physical target");
    assert!(
        matches!(&entry.backend, InstalledBackend::Physical { mode, .. } if *mode == expected),
        "registry retains the actual issued physical mode"
    );
    assert_eq!(entry.original_source.origin(), SourceOrigin::HubCache);
    assert_eq!(entry.original_source.producer(), fixture.projects[0].1);
    assert_eq!(entry.original_source.version().as_str(), version);
    assert!(
        entry.original_source.remote_file_origin().is_some(),
        "physical Copy retains remote provenance"
    );
    assert_eq!(entry.alias.as_deref(), Some("web-file-mutable"));
    assert_eq!(entry.entry_name.as_deref(), Some("web-file-entry"));
    let destination = project_destination(fixture);
    let metadata = checked(
        std::fs::symlink_metadata(&destination),
        "actual physical mode observation",
    );
    assert_eq!(
        metadata.file_type().is_symlink(),
        matches!(expected, ArtifactMode::Link),
        "Copy is a real directory and Link is a real filesystem link"
    );
}

async fn project_physical_mode_journey(
    fixture: &crate::source_server::test_fixture::Fixture,
    host: &WebHost,
    cookie: &str,
    input: &mut Value,
) -> std::path::PathBuf {
    let installed = page(host, cookie, INSTALL, input.clone()).await;
    assert!(
        installed["cache"] != "not_attempted" && installed["publication"] != "not_attempted",
        "actual cache and local publication"
    );
    assert_project_mode(
        fixture,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        "release/one",
    );
    let destination = project_destination(fixture);
    assert!(
        destination.join("SKILL.md").is_file(),
        "actual Core-owned projection"
    );
    assert!(
        checked(
            std::fs::read_to_string(destination.join("SKILL.md")),
            "rendered FILE"
        )
        .contains("First mutable release")
    );
    let content = json!({"installed":{"scope":input["scope"],"artifact_type":"skill","id":FILE_ID,"version":"release/one","binding":NativeBinding::KiroV3,"path":"SKILL.md"}});
    assert!(
        page(host, cookie, CONTENT, content.clone()).await["content"]["content"] == ONE,
        "verified existing cache bytes"
    );
    assert!(
        page(host, cookie, INSTALL, input.clone()).await["cache"] == "Unchanged",
        "receipt-based cache fast path"
    );
    let mut nonportable = input.clone();
    nonportable["mode"] = json!("link");
    let before_link = hub_file_reads::registry_bytes(&project_registry(fixture).path());
    failure(host, cookie, UPDATE, &nonportable, StatusCode::BAD_REQUEST).await;
    assert_eq!(
        hub_file_reads::registry_bytes(&project_registry(fixture).path()),
        before_link
    );
    assert_project_mode(
        fixture,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        "release/one",
    );
    input
        .as_object_mut()
        .expect("omitted update options")
        .remove("mode");
    input
        .as_object_mut()
        .expect("omitted update options")
        .remove("entry_name");
    input["selection"]["version"] = json!("release/two");
    input.as_object_mut().expect("typed update").remove("alias");
    let updated = page(host, cookie, UPDATE, input.clone()).await;
    assert!(
        updated["version"] == "release/two",
        "actual concrete update"
    );
    assert_project_mode(
        fixture,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        "release/two",
    );
    let registry = checked(
        project_registry(fixture).load(),
        "Web-updated project registry",
    );
    let selected = registry
        .get_for_consumer(&project_slot(fixture), ArtifactType::Skill, FILE_ID)
        .expect("actual FILE registry entry")
        .installations
        .iter()
        .find(|entry| {
            entry.target
                == InstallationTarget::for_artifact(NativeBinding::KiroV3, ArtifactType::Skill)
        })
        .expect("actual bound target");
    assert!(
        selected.alias.as_deref() == Some("web-file-mutable")
            && selected.original_source.version().as_str() == "release/two",
        "update preserves alias and exact tuple"
    );
    assert!(
        project_destination(fixture) == destination,
        "alias-derived destination stable"
    );
    assert!(
        checked(
            std::fs::read_to_string(destination.join("SKILL.md")),
            "updated physical FILE"
        )
        .contains("Second mutable release")
    );
    input["mode"] = json!("copy");
    page(host, cookie, UPDATE, input.clone()).await;
    assert_project_mode(
        fixture,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        "release/two",
    );
    input
        .as_object_mut()
        .expect("omitted Copy update")
        .remove("mode");
    page(host, cookie, UPDATE, input.clone()).await;
    assert_project_mode(
        fixture,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        "release/two",
    );

    // The genuine seeded Command cannot use KiroV3 hook projection as Physical.
    let mut unsupported = input.clone();
    unsupported["selection"]["artifact_type"] = json!("command");
    unsupported["selection"]["version"] = json!("release/unsupported");
    unsupported["mode"] = json!("copy");
    let before_unsupported = hub_file_reads::registry_bytes(&project_registry(fixture).path());
    let unsupported_result = failure(
        host,
        cookie,
        INSTALL,
        &unsupported,
        StatusCode::NOT_IMPLEMENTED,
    )
    .await;
    assert_eq!(unsupported_result["error"]["code"], "unsupported");
    assert_eq!(
        unsupported_result["outcome"]["publication"]["committed"],
        false
    );
    assert_eq!(
        unsupported_result["outcome"]["publication"]["hub_pending"],
        false
    );
    assert!(
        unsupported_result["outcome"]["cache"] != "not_attempted",
        "honest cache effect precedes projection rejection"
    );
    assert!(
        hub_file_reads::registry_bytes(&project_registry(fixture).path()) == before_unsupported,
        "unsupported projection publishes no PROJECT tuple"
    );
    assert_project_mode(
        fixture,
        ArtifactMode::Copy(CopyPolicy::RejectFileLinks),
        "release/two",
    );
    assert!(
        checked(
            std::fs::read_to_string(destination.join("SKILL.md")),
            "physical FILE after Unsupported"
        )
        .contains("Second mutable release")
    );

    destination
}

fn spawn_mutation(
    host: &WebHost,
    cookie: &str,
    path: &str,
    input: Value,
) -> tokio::task::JoinHandle<reqwest::Response> {
    let client = checked(
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(45))
            .build(),
        "owned in-flight mutation client",
    );
    let request = client
        .post(format!("{}{path}", host.origin))
        .header(header::ORIGIN, &host.origin)
        .header(header::COOKIE, cookie)
        .header("X-Graphit-Request", "ui")
        .json(&input);
    tokio::spawn(async move { checked(request.send().await, "real mutation response") })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO fixture and coordinated App emission"]
async fn real_web_file_mutations_preserve_authority_tuple_and_recovery() {
    Box::pin(run_file_mutation_fixture(None, false)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a real browser observer and GRAPHIT_UI_BROWSER_PROBE_FILE"]
async fn real_web_file_browser_probe_catalog_and_global_cache() {
    let probe = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_UI_BROWSER_PROBE_FILE").expect("explicit browser probe path"),
    );
    Box::pin(run_file_mutation_fixture(Some(probe), false)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO fixture and coordinated App emission"]
async fn real_web_update_all_preserves_local_and_reports_unattempted() {
    Box::pin(run_file_mutation_fixture(None, true)).await;
}

async fn run_file_mutation_fixture(
    browser_probe: Option<std::path::PathBuf>,
    update_all_only: bool,
) {
    use futures_util::FutureExt;
    if let Some(probe) = browser_probe.as_ref() {
        let parent = checked(
            std::fs::File::open(probe.parent().expect("private browser probe parent")),
            "browser probe parent handle",
        );
        checked(
            graphit_fs::verify_private_directory(&parent),
            "private browser probe parent",
        );
        for path in [
            probe.clone(),
            probe.with_extension("credentials.json"),
            probe.with_extension("done"),
            probe.with_extension("result.json"),
        ] {
            assert!(!path.exists(), "use fresh browser probe paths");
        }
    }
    let mut probe_finished = false;
    let mut probe_credentials_created = false;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let rules = [
        ServiceRuleInput::StandaloneS3 {
            purpose: graphit_auth::ServicePurpose::HubRead,
            origin: ServiceOrigin::Web,
            provider: "catalog-storage",
            profile: Some("catalog-keys"),
        },
        ServiceRuleInput::StandaloneS3 {
            purpose: graphit_auth::ServicePurpose::HubRead,
            origin: ServiceOrigin::Web,
            provider: "alternate-storage",
            profile: Some("alternate-keys"),
        },
    ];
    let fixture = configure_hub_fixture(
        crate::source_server::test_fixture::fixture_with_web_read_providers(
            &[Permission::Read(Module::Hub)],
            &[Permission::ManageAuthProfiles],
            Brand::default(),
            NativeBinding::KiroV3,
            &["mutable-primary", "mutable-sibling"],
            &rules,
            (None, Some("broker")),
        ),
    )
    .await;
    let producer = fixture.projects[0].1;
    let second = checked(
        "01D39ZY06FGSCTVN4T2V9PKHG0".parse::<graphit_core::project_identity::ProjectId>(),
        "independent real grant",
    );
    let broker = match std::panic::AssertUnwindSafe(BrokerProcess::start_web_projects(
        std::path::Path::new(&binary),
        &producer.to_string(),
        Some(&second.to_string()),
        false,
        true,
    ))
    .catch_unwind()
    .await
    {
        Ok(broker) => broker,
        Err(error) => {
            let _ = fixture.server.shutdown().await;
            std::panic::resume_unwind(error);
        }
    };
    let mut host = None;
    let mut task_owner = None;
    let mut limited_host = None;
    let mut limited_tasks = None;
    let mut task_directory = None;
    let mut relay_owner = None;
    let mut requests = Vec::new();
    let observed = std::panic::AssertUnwindSafe(Box::pin(async {
        let relay = hub_remote_catalog::StorageRelay::start(wire(&broker.ready, "storage_endpoint")).await;
        relay_owner = Some(relay);
        let relay = relay_owner.as_ref().expect("owned real FILE relay");
        hub_remote_catalog::configure_standalone(&fixture, &broker.ready, &relay.endpoint).await;
        hub_remote_catalog::configure_standalone_named(&fixture, &broker.ready, &relay.endpoint, "alternate-storage", "alternate-keys").await;
        let native = seed_store(&broker.ready, producer).await;
        let publisher = hub_remote_catalog::standalone_publisher_store(&broker.ready);
        let store = hub_remote_catalog::standalone_store(&broker.ready);
        let prefix = wire(&broker.ready["standalone_storage"], "prefix");
        hub_remote_catalog::seed_project_metadata(&fixture.server, store.as_ref(), prefix, producer).await;
        for (version, text) in [("release/one", ONE), ("release/two", TWO)] {
            hub_file_reads::seed_file(&fixture, native.as_ref(), wire(&broker.ready, "prefix"), producer, version, text).await;
            hub_file_reads::seed_file(&fixture, publisher.as_ref(), prefix, producer, version, text).await;
        }
        // Keep SDK fixture uploads in setup, outside the physical mutation future.
        hub_file_reads::seed_file_payload(
            &fixture, publisher.as_ref(), prefix, producer, "release/unsupported",
            "---\nname: unsupported-physical\ndescription: Real command fixture\n---\nCommand body\n",
            crate::remote_file_install::client::Kind::Command,
        ).await;
        let revision = crate::graph_server::tests::browser_provider(&fixture.server.graph_server(), &fixture.raw, wire(&broker.ready, "endpoint"));
        let (directory, tasks, _) = crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
        task_directory = Some(directory); task_owner = Some(tasks.clone());
        host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let host = host.as_ref().expect("owned mutable Web host");
        let cookie = host.login(&broker.ready).await;
        let sibling = host.login(&broker.ready).await;
        let mut other_ready = broker.ready.clone();
        assert!(other_ready["web_second"].is_object(), "second genuine Web identity");
        other_ready["web"] = broker.ready["web_second"].clone();
        let other = host.login(&other_ready).await;
        let identity = checked(host.client.get(format!("{}/auth/session", host.origin)).header(header::COOKIE, cookie.as_str()).send().await, "actual mutation session");
        assert_eq!(identity.status(), StatusCode::OK);
        let identity: Value = checked(identity.json().await, "verified mutation subject");
        let subject = wire(&identity, "subject");
        let issuer = wire(&broker.ready, "issuer");
        for key in ["v1/global/projects.json", "v1/anonymous/projects.json", "v1/authenticated/projects.json"] {
            put_json(store.as_ref(), prefix, key, json!({"v":1,"projects":[]})).await;
        }
        let user_key = format!("v1/users/{}/{}/projects.json", URL_SAFE_NO_PAD.encode(issuer), URL_SAFE_NO_PAD.encode(subject));
        put_json(store.as_ref(), prefix, &user_key, json!({"v":1,"projects":[{"id":producer}]})).await;
        if update_all_only {
            Box::pin(update_all::journey(&fixture, host, &cookie, (issuer, subject), publisher.as_ref(), prefix)).await;
            return;
        }
        if let Some(probe) = browser_probe.as_ref() {
            administrators(&fixture, issuer, &[(subject, &["global"])]);
            let catalog = page(host, &cookie, "/api/hub/remote/search", json!({"transport":"standalone","provider":"catalog-storage","profile":"catalog-keys","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":1})).await;
            assert!(catalog["artifacts"][0]["id"] == FILE_ID && catalog["artifacts"][0]["versions"].as_array().is_some_and(|versions| versions.contains(&json!("release/two"))), "browser catalog contains the published installable FILE release");
            let content = page(host, &cookie, CONTENT, json!({"remote":{"selection":{"transport":"standalone","provider":"catalog-storage","profile":"catalog-keys","producer":producer,"artifact_type":"skill","id":FILE_ID,"version":"release/two","exact":true},"path":"SKILL.md"}})).await;
            assert!(content["cache"] == "ephemeral" && content["content"]["content"] == TWO, "browser FILE entry has a genuine SDK archive before installation");
            let html = checked(host.client.get(format!("{}/hub/registry", host.origin)).send().await, "browser FILE assets response");
            assert_eq!(html.status(), StatusCode::OK);
            assert!(checked(html.text().await, "browser FILE assets body").contains("<html"));
            let private = checked(graphit_fs::create_private_file(&probe.with_extension("credentials.json")), "private browser fixture credentials file");
            probe_credentials_created = true;
            checked(serde_json::to_writer(private, &json!({"username":wire(&broker.ready["web"], "username"),"password":wire(&broker.ready["web"], "password")})), "private browser fixture credentials written");
            let public = checked(graphit_fs::create_private_file(probe), "public browser probe file with private birth");
            checked(serde_json::to_writer(public, &json!({"scenario":"file_catalog_global_cache","origin":host.origin,"path":"/hub/registry","scope":"global_cache","transport":"standalone","provider":"catalog-storage","profile":"catalog-keys","producer":producer,"artifact_type":"skill","artifact_id":FILE_ID,"version":"release/two","content_path":"SKILL.md","browser_qualified":false})), "public browser probe metadata written");
            let deadline = Instant::now() + Duration::from_secs(900);
            while Instant::now() < deadline {
                if probe.with_extension("done").exists() {
                    probe_finished = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            return;
        }
        let project = json!({"kind":"project","project_id":producer,"project_version":fixture.projects[0].2.version()});
        let mut input = json!({"selection":{"transport":"standalone","provider":"catalog-storage","profile":"catalog-keys","producer":producer,"artifact_type":"skill","id":FILE_ID,"version":"release/one","exact":true},"scope":project,"binding":NativeBinding::KiroV3,"alias":"web-file-mutable","entry_name":"web-file-entry"});
        let baseline = hub_file_reads::installation_children(fixture.paths.root());
        let registry_before = hub_file_reads::registry_bytes(&project_registry(&fixture).path());
        let unauthenticated = request(host, None, INSTALL, &input.to_string()).await;
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
        failure(host, &cookie, INSTALL, &input, StatusCode::FORBIDDEN).await;
        assert!(hub_file_reads::installation_children(fixture.paths.root()) == baseline, "read-only Web caller creates no persistent cache parents");
        assert!(hub_file_reads::registry_bytes(&project_registry(&fixture).path()) == registry_before, "read-only Web caller publishes nothing");
        administrators(&fixture, issuer, &[(subject, &["project"])]);
        let mut global = input.clone();
        global["scope"] = json!({"kind":"global_cache"});
        for field in ["binding", "alias", "entry_name"] { global.as_object_mut().expect("typed global input").remove(field); }
        failure(host, &cookie, INSTALL, &global, StatusCode::FORBIDDEN).await;
        for body in ["{}", r#"{"scope":null}"#, r#"{"scope":{},"scope":{}}"#, r#"{"global":true}"#] {
            assert_eq!(request(host, Some(&cookie), INSTALL, body).await.status(), StatusCode::BAD_REQUEST);
        }
        for (field, value) in [("binding", Value::Null), ("alias", Value::Null), ("mode", Value::Null), ("mode", json!("unknown")), ("entry_name", json!("../escape")), ("project_dir", json!("/foreign"))] {
            let mut invalid = input.clone(); invalid[field] = value;
            failure(host, &cookie, INSTALL, &invalid, StatusCode::BAD_REQUEST).await;
        }

        for (field, value) in [
            ("mode", json!("link")), ("mode", json!("copy")),
            ("binding", json!(NativeBinding::KiroV3)),
            ("alias", json!("invalid-global")), ("entry_name", json!("invalid-global")),
        ] {
            let mut invalid = global.clone(); invalid[field] = value;
            for path in [INSTALL, UPDATE] {
                failure(host, &cookie, path, &invalid, StatusCode::BAD_REQUEST).await;
            }
        }
        let mut duplicate_mode = input.to_string(); duplicate_mode.pop();
        duplicate_mode.push_str(r#","mode":"copy","mode":"link"}"#);
        for path in [INSTALL, UPDATE] {
            assert_eq!(request(host, Some(&cookie), path, &duplicate_mode).await.status(), StatusCode::BAD_REQUEST);
        }
        assert!(hub_file_reads::installation_children(fixture.paths.root()) == baseline, "invalid mode inputs create no cache parents");
        assert!(hub_file_reads::registry_bytes(&project_registry(&fixture).path()) == registry_before, "invalid mode inputs publish nothing");
        let mut invalid = input.clone(); invalid["selection"]["identity_provider"] = json!("broker");
        failure(host, &cookie, INSTALL, &invalid, StatusCode::BAD_REQUEST).await;
        let oversized = request(host, Some(&cookie), INSTALL, &" ".repeat(65537)).await;
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let mut foreign = input.clone(); foreign["scope"]["project_id"] = json!(fixture.projects[1].1);
        failure(host, &cookie, INSTALL, &foreign, StatusCode::FORBIDDEN).await;
        foreign["scope"]["project_id"] = json!(second);
        failure(host, &cookie, INSTALL, &foreign, StatusCode::NOT_FOUND).await;
        foreign = input.clone(); foreign["scope"]["project_version"] = json!("missing-version");
        failure(host, &cookie, INSTALL, &foreign, StatusCode::NOT_FOUND).await;
        // Private policy alone cannot substitute the actual caller's HubRead.
        administrators(&fixture, issuer, &[(subject, &["project"]), (wire(&broker.ready["web_second"], "subject"), &["project"])]);
        failure(host, &other, INSTALL, &input, StatusCode::FORBIDDEN).await;
        administrators(&fixture, issuer, &[(subject, &["project"])]);
        Box::pin(authored_local::journey(&fixture, host, &cookie, issuer, subject)).await;
        let destination = Box::pin(project_physical_mode_journey(
            &fixture, host, &cookie, &mut input,
        )).await;
        let mut changed_origin = input.clone(); changed_origin["selection"]["transport"] = json!("standalone"); changed_origin["selection"]["provider"] = json!("alternate-storage"); changed_origin["selection"]["profile"] = json!("alternate-keys");
        failure(host, &cookie, UPDATE, &changed_origin, StatusCode::CONFLICT).await;
        let mut lifecycle = json!({"scope":project,"artifact_type":"skill","id":FILE_ID,"version":"release/one","binding":NativeBinding::KiroV3});
        failure(host, &cookie, REMOVE, &lifecycle, StatusCode::CONFLICT).await;
        lifecycle["version"] = json!("release/two");
        let removed = page(host, &cookie, REMOVE, lifecycle.clone()).await;
        assert!(removed["publication"]["change"] == "Physical(Removed)", "actual PROJECT removal receipt");
        assert!(!destination.exists(), "actual PROJECT projection removed");
        failure(host, &cookie, UPDATE, &input, StatusCode::NOT_FOUND).await;
        failure(host, &cookie, RECOVER, &lifecycle, StatusCode::NOT_FOUND).await;
        // Global administration is separately scoped and keeps cached bytes.
        administrators(&fixture, issuer, &[(subject, &["global"])]);
        failure(host, &cookie, INSTALL, &input, StatusCode::FORBIDDEN).await;
        failure(host, &cookie, UPDATE, &global, StatusCode::NOT_FOUND).await;
        page(host, &cookie, INSTALL, global.clone()).await;
        let global_registry = checked(fixture.server.policy().registry.load(), "actual GlobalCache registry");
        let cached = global_registry.entries().find(|(kind, id, _)| *kind == ArtifactType::Skill && *id == FILE_ID).expect("actual GlobalCache tuple").2.installations[0].original_source.directory().as_path().to_owned();
        global["selection"]["version"] = json!("release/two");
        page(host, &cookie, UPDATE, global.clone()).await;
        let global_remove = json!({"scope":{"kind":"global_cache"},"artifact_type":"skill","id":FILE_ID,"version":"release/one"});
        for path in [RECOVER, FORGET] { failure(host, &cookie, path, &global_remove, StatusCode::BAD_REQUEST).await; }
        let removed = page(host, &cookie, REMOVE, global_remove).await;
        assert!(removed["payload_preserved"] == true && cached.join("SKILL.md").is_file(), "GlobalCache Remove preserves payload");
        administrators(&fixture, issuer, &[(subject, &["project", "global"])]);
        let tasks = task_owner.as_ref().expect("owned Task policy");
        let (mut limits, timeout, _) = tasks.browser_policy();
        limits.max_output_bytes = 256;
        let tasks = tasks.references_test_policy(limits, timeout);
        limited_tasks = Some(tasks.clone());
        limited_host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let limited = limited_host.as_ref().expect("owned limited-response host");
        let limited_cookie = limited.login(&broker.ready).await;
        limited_response(limited, &limited_cookie, INSTALL, &input, limits.max_output_bytes).await;
        assert!(checked(project_registry(&fixture).load(), "publication after limited success").get_for_consumer(&project_slot(&fixture), ArtifactType::Skill, FILE_ID).is_some(), "response quota never rolls back a confirmed publication");

        let destination = project_destination(&fixture);
        let foreign = destination.join("foreign.txt");
        checked(std::fs::write(&foreign, "foreign payload must remain"), "foreign payload added outside Graphit stores");
        limited_response(limited, &limited_cookie, REMOVE, &lifecycle, limits.max_output_bytes).await;
        assert!(foreign.is_file(), "oversized error response retains foreign payload");
        assert!(checked(project_registry(&fixture).load(), "pending registry").intent().is_some(), "real Hub intent remains");
        let recovery = request(host, Some(&cookie), RECOVER, &lifecycle.to_string()).await;
        assert!(!recovery.status().is_success());
        let recovery: Value = checked(recovery.json().await, "actual recovery failure");
        assert!(recovery["outcome"]["publication"]["hub_pending"] == true, "recovery retains same-key pending");
        let mut wrong_key = lifecycle.clone(); wrong_key["id"] = json!("fixture/other");
        assert!(failure(host, &cookie, FORGET, &wrong_key, StatusCode::CONFLICT).await["outcome"]["blocked_by_pending"] == true, "foreign pending cannot be forgotten");
        let forgotten = page(host, &cookie, FORGET, lifecycle.clone()).await;
        assert!(forgotten["publication"]["change"] == "Forgotten" && !forgotten["forget_preview"].is_null(), "actual writer Forget preview");
        assert!(checked(std::fs::read_to_string(&foreign), "preserved foreign payload") == "foreign payload must remain");
        assert!(checked(project_registry(&fixture).load(), "registry after Forget").intent().is_none());

        // Retain a genuine archive GET after backend I/O, then revoke private
        // administration before the operation reaches persistent cache parents.
        hub_file_reads::seed_file(&fixture, publisher.as_ref(), prefix, producer, "release/three", ONE).await;
        let mut held = global.clone(); held["selection"]["version"] = json!("release/three");
        held["selection"]["transport"] = json!("standalone"); held["selection"]["provider"] = json!("catalog-storage"); held["selection"]["profile"] = json!("catalog-keys");
        let before = hub_file_reads::installation_children(fixture.paths.root());
        let before_registry = hub_file_reads::registry_bytes(&fixture.server.policy().registry.path());
        relay.hold_next_file();
        requests.push(spawn_mutation(host, &cookie, INSTALL, held.clone()));
        relay.entered().await;
        administrators(&fixture, issuer, &[]);
        relay.release();
        let response = checked(checked(tokio::time::timeout(Duration::from_secs(15), requests.last_mut().expect("owned policy-race request")).await, "policy-race join guard"), "policy-race client join");
        requests.pop();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "retired private policy rejected before local effect");
        assert!(hub_file_reads::installation_children(fixture.paths.root()) == before && hub_file_reads::registry_bytes(&fixture.server.policy().registry.path()) == before_registry, "no cache parents or registry after policy retirement");
        administrators(&fixture, issuer, &[(subject, &["project", "global"])]);
        page(host, &cookie, INSTALL, held.clone()).await;
        // The registry writer blocks the second local phase after the cache
        // commits. Observe that real receipt before retiring current authority.
        let receipt = hub_file_reads::seed_file(&fixture, publisher.as_ref(), prefix, producer, "release/cache-observed", TWO).await;
        let cache_digest = std::array::from_fn(|index| checked(u8::from_str_radix(&receipt.archive_sha256[index * 2..index * 2 + 2], 16), "actual fixture receipt digest"));
        let policy = fixture.server.policy();
        let hub = &policy.native_install.hub;
        let expected = checked(graphit_hub::SourceRef::hub_cache(
            &policy.installation, producer, ArtifactType::Skill,
            checked(graphit_hub::ArtifactId::new(FILE_ID, hub.projection()), "observed cache FILE ID"),
            None,
            checked(graphit_hub::ConcreteVersion::new("release/cache-observed", hub.projection()), "observed cache release"),
            hub.projection(),
        ), "actual expected cache source");
        let expected = checked(expected.with_remote_file_origin(
            graphit_hub::RemoteFileOrigin::Standalone { provider: "catalog-storage".into(), profile: Some("catalog-keys".into()) },
            cache_digest, hub.projection(),
        ), "actual observed origin and digest");
        let publication_boundary = crate::source_server::web_file_mutations::cache_commit_pause::after_cache(&expected);
        let observation_cancellation = Cancellation::default();
        let observation = checked(hub.request_context(Instant::now(), &observation_cancellation), "cache observation Original");
        let capture = fixture.auth.installation();
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } = capture.live.owner() else { panic!("actual installation capture required"); };
        let cache_scope = checked(CapturedPublicationScope::installation(
            &policy.installation, installation_id.clone(), [policy.installation.root().to_owned()], policy.scope, observation.snapshot(),
        ), "cache observation physical scope");
        let cache = ArtifactStore::new(cache_scope, hub.artifacts());
        let mut post_cache = held.clone(); post_cache["selection"]["version"] = json!("release/cache-observed");
        let registry_before_cache = hub_file_reads::registry_bytes(&policy.registry.path());
        relay.hold_next_file();
        requests.push(spawn_mutation(host, &cookie, INSTALL, post_cache));
        relay.entered().await;
        let registry_writer = checked(policy.registry.writer(Duration::from_secs(5), &observation_cancellation), "block actual registry publication after archive lookup");
        relay.release();
        checked(tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if publication_boundary.entered()
                    && let Some(snapshot) = checked(cache.load(), "observe actual cache catalog").filter(|snapshot| snapshot.pending().is_none())
                    && let Some(entry) = snapshot.entries().iter().find(|entry| entry.destination() == expected.directory().as_path()) {
                        let writer = checked(cache.writer(Duration::from_secs(5), &observation_cancellation), "verify actual observed cache receipt");
                        assert!(checked(writer.verify_owned_copy(entry.key(), entry.destination(), graphit_fs::NodeKind::Directory), "verify committed receipt and bytes"));
                        assert_eq!(checked(std::fs::read_to_string(entry.destination().join("SKILL.md")), "actual committed cache text"), TWO);
                        break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await, "confirmed cache observation deadline");
        administrators(&fixture, issuer, &[]);
        drop(registry_writer);
        publication_boundary.release();
        let response = checked(checked(tokio::time::timeout(Duration::from_secs(15), requests.last_mut().expect("owned post-cache retirement request")).await, "post-cache response guard"), "post-cache request joined");
        requests.pop();
        let retired_status = response.status();
        let outcome: Value = checked(response.json().await, "post-cache authority outcome");
        assert_eq!(retired_status, StatusCode::FORBIDDEN, "authority retired before registry publication; error={}, cache={}, publication={}", outcome["error"]["code"], outcome["outcome"]["cache"], outcome["outcome"]["publication"]);
        assert!(outcome["outcome"]["cache"] == "Created", "confirmed cache effect survives later authority loss");
        assert!(outcome["outcome"]["publication"] == "not_attempted", "registry phase never committed");
        assert!(hub_file_reads::registry_bytes(&policy.registry.path()) == registry_before_cache, "post-cache authority failure preserves registry");
        assert!(expected.directory().as_path().join("SKILL.md").is_file(), "confirmed cache remains after failure");
        administrators(&fixture, issuer, &[(subject, &["project", "global"])]);
        hub_file_reads::seed_file(&fixture, publisher.as_ref(), prefix, producer, "release/four", ONE).await;
        held["selection"]["version"] = json!("release/four");
        let before_registry = hub_file_reads::registry_bytes(&fixture.server.policy().registry.path());
        // Same actual owner as catalog reads: timers stay live while a mutable
        // worker is observed through its JoinHandle, without count inference.
        { let mut jobs = host.service.0.jobs.lock().await; while let Some(result) = jobs.try_join_next() { checked(result, "completed mutation job joined"); } }
        relay.hold_next_file();
        requests.push(spawn_mutation(host, &cookie, INSTALL, held.clone()));
        relay.entered().await;
        hub_remote_catalog::assert_catalog_job_pending(host).await;
        let response = checked(checked(tokio::time::timeout(Duration::from_secs(40), requests.last_mut().expect("owned deadline mutation")).await, "mutation deadline response guard"), "deadline client joined");
        requests.pop();
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT, "Original deadline reaches mutable owner");
        relay.release(); hub_remote_catalog::join_catalog_read_job(host).await;
        assert!(hub_file_reads::registry_bytes(&fixture.server.policy().registry.path()) == before_registry, "cancelled archive cannot publish registry");
        let healthy = json!({"scope":{"kind":"global_cache"},"artifact_type":"skill","id":FILE_ID,"limit":10});
        page(host, &sibling, "/api/hub/installed/files", healthy.clone()).await;
        relay.hold_next_file();
        requests.push(spawn_mutation(host, &cookie, INSTALL, held.clone()));
        relay.entered().await; hub_remote_catalog::assert_catalog_job_pending(host).await;
        let request = requests.last_mut().expect("owned disconnect mutation"); request.abort();
        assert!(request.await.is_err_and(|error| error.is_cancelled())); requests.pop();
        relay.release(); hub_remote_catalog::join_catalog_read_job(host).await;
        assert!(hub_file_reads::registry_bytes(&fixture.server.policy().registry.path()) == before_registry, "disconnected mutable request cannot publish registry");
        page(host, &sibling, "/api/hub/installed/files", healthy.clone()).await;
        relay.hold_next_file();
        requests.push(spawn_mutation(host, &cookie, INSTALL, held.clone()));
        relay.entered().await;
        let logout = checked(host.client.post(format!("{}/auth/logout", host.origin)).header(header::ORIGIN, &host.origin).header(header::COOKIE, cookie.as_str()).send().await, "logout during real FILE I/O");
        assert_eq!(logout.status(), StatusCode::NO_CONTENT);
        let response = checked(checked(tokio::time::timeout(Duration::from_secs(15), requests.last_mut().expect("owned logout mutation")).await, "logout mutation join guard"), "logout client join"); requests.pop();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let outcome: Value = checked(response.json().await, "terminal mutable response");
        assert!(outcome["outcome"]["cache"] == "not_attempted", "terminal failure retains admitted operation outcome");
        relay.release();
        // Logout also completes the removed session timer. A single join_next
        // cannot identify the FILE worker here. The401 outcome follows its
        // awaited receiver; the real shutdown below collects every handle.
        page(host, &sibling, "/api/hub/installed/files", healthy).await;
        relay.hold_next_file();
        requests.push(spawn_mutation(host, &sibling, INSTALL, held));
        relay.entered().await;
        checked(checked(tokio::time::timeout(Duration::from_secs(15), host.service.shutdown()).await, "actual Browser stop drain guard"), "actual Browser owned job drain");
        let response = checked(checked(tokio::time::timeout(Duration::from_secs(15), requests.last_mut().expect("owned stop mutation")).await, "stop response guard"), "stop client join"); requests.pop();
        assert!([StatusCode::UNAUTHORIZED, StatusCode::SERVICE_UNAVAILABLE].contains(&response.status()), "shutdown ends the session or service, never succeeds");
        let stopped: Value = checked(response.json().await, "stopped mutation outcome");
        assert!(stopped["outcome"]["cache"] == "not_attempted", "owned physical phase quiesced before stop response");
        relay.release();
        assert!(hub_file_reads::registry_bytes(&fixture.server.policy().registry.path()) == before_registry, "real stop cannot publish the held archive");
    })).catch_unwind().await;
    for request in requests {
        request.abort();
        let _ = request.await;
    }
    let relay = std::panic::AssertUnwindSafe(async {
        if let Some(relay) = relay_owner {
            relay.close().await;
        }
    })
    .catch_unwind()
    .await;
    let limited = std::panic::AssertUnwindSafe(async {
        if let Some(host) = limited_host {
            host.close().await;
        }
        if let Some(tasks) = limited_tasks {
            checked(tasks.shutdown().await, "limited Task owner shutdown");
        }
    })
    .catch_unwind()
    .await;
    let cleanup = std::panic::AssertUnwindSafe(finish_remote_test_owners(
        observed.and(relay).and(limited),
        host,
        Some(&fixture.server),
        broker,
    ))
    .catch_unwind()
    .await;
    let tasks = std::panic::AssertUnwindSafe(async {
        if let Some(tasks) = task_owner {
            checked(tasks.shutdown().await, "mutable FILE Task owner shutdown");
        }
    })
    .catch_unwind()
    .await;
    let private_cleanup = std::panic::AssertUnwindSafe(async {
        if probe_credentials_created {
            let probe = browser_probe
                .as_ref()
                .expect("owned browser credential path");
            checked(
                std::fs::remove_file(probe.with_extension("credentials.json")),
                "private browser fixture credentials removed",
            );
        }
    })
    .catch_unwind()
    .await;
    drop(task_directory);
    for result in [cleanup, tasks, private_cleanup] {
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
    if let Some(probe) = browser_probe {
        let result = checked(
            graphit_fs::create_private_file(&probe.with_extension("result.json")),
            "browser probe result file",
        );
        checked(
            serde_json::to_writer(
                result,
                &json!({"finished":probe_finished,"owners_joined":true,"credentials_removed":true,"browser_qualified":false}),
            ),
            "browser probe lifecycle result written",
        );
        assert!(
            probe_finished,
            "browser observer deadline elapsed after cleanup"
        );
    }
}
