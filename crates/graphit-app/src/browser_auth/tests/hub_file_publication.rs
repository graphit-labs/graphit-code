//! Genuine Web SourcePROJECT publisher, Broker HubWrite and ordinary FILE consumers.
use super::*;
use graphit_auth::{
    CredentialSource, Permission, RemoteServiceBindingInput, ServiceOrigin, ServicePurpose,
    ServiceRuleInput,
};
use graphit_core::{brand::Brand, module_settings::Module};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, NativeBinding, SourceRef, policy::RequestContext,
};
use hub_remote_catalog::{page, request};
const PUBLISH: &str = "/api/hub/files/publish";
pub(super) const FILE_ID: &str = "fixture/web-publication";
pub(super) const ONE: &str = "---\nname: web-publisher\ndescription: Actual PROJECT source\n---\nPublished through actual Web HubWrite\n";
pub(super) fn resource(
    fixture: &crate::source_server::test_fixture::Fixture,
    version: &str,
    text: &str,
) -> std::path::PathBuf {
    let policy = &fixture.server.policy().native_install.hub;
    let (paths, id, _) = &fixture.projects[0];
    let source = checked(
        SourceRef::resources(
            paths,
            *id,
            ArtifactType::Skill,
            checked(
                ArtifactId::new(FILE_ID, policy.projection()),
                "full source ID",
            ),
            checked(
                ConcreteVersion::new(version, policy.projection()),
                "concrete source version",
            ),
            policy.projection(),
        ),
        "maintained PROJECT source",
    );
    let directory = source.directory().as_path().to_owned();
    checked(
        std::fs::create_dir_all(directory.join("references")),
        "actual PROJECT source directories",
    );
    checked(
        std::fs::write(directory.join("SKILL.md"), text),
        "actual PROJECT skill bytes",
    );
    checked(
        std::fs::write(
            directory.join("references/guide.md"),
            "Resource remains in ordinary ZIP\n",
        ),
        "actual PROJECT resource bytes",
    );
    directory
}
async fn failure(host: &WebHost, cookie: &str, input: &Value, status: StatusCode) -> Value {
    let response = request(host, Some(cookie), PUBLISH, &input.to_string()).await;
    assert_eq!(response.status(), status, "finite publication failure");
    assert!(
        response.headers()[header::CACHE_CONTROL]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    checked(response.json().await, "bounded public publication failure")
}

pub(super) fn administrators(
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

// Use the real startup root-registration authority; this grants no Web module access.
pub(super) fn trust_source_projects(fixture: &crate::source_server::test_fixture::Fixture) {
    let cancel = Cancellation::default();
    let original = checked(
        RequestContext::new(
            fixture.server.policy().content,
            &cancel,
            Instant::now() + Duration::from_secs(30),
        ),
        "source root registration Original",
    );
    let caller = checked(
        fixture
            .auth
            .issuer()
            .authenticate_runtime(&fixture.raw, &original),
        "actual startup Runtime owner",
    );
    for (paths, id, _) in fixture.projects.iter() {
        let scope = checked(
            graphit_core::artifact_store::CapturedPublicationScope::project(
                paths,
                *id,
                fixture.server.policy().scope,
                original.snapshot(),
            ),
            "source project physical capture",
        );
        let binding = checked(scope.binding(), "source project physical binding");
        checked(
            fixture.auth.trust_project_root(
                &caller,
                &original,
                graphit_auth::ScopeCapture {
                    live: &scope,
                    record: &binding,
                },
            ),
            "register exact source root through actual startup authority",
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the genuine Broker Web/STSv3/MinIO fixture executable"]
async fn real_web_project_file_publication_and_consumers() {
    use futures_util::FutureExt;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    // Retain a real PROJECT independently of the installation selected by Web.
    let source_owner = crate::source_server::test_fixture::fixture_with_web_read_providers(
        &[Permission::Read(Module::Hub)],
        &[Permission::ManageAuthProfiles],
        Brand::default(),
        NativeBinding::KiroV3,
        &["web-publication-source", "denied-source"],
        &[],
        (None, Some("broker")),
    );
    checked(
        source_owner.server.shutdown().await,
        "source setup owner joined",
    );
    let producer = source_owner.projects[0].1;
    let mut broker =
        BrokerProcess::start_web_publication(std::path::Path::new(&binary), &producer.to_string())
            .await;
    let mut fixture_owner = None;
    let mut host = None;
    let mut task_owner = None;
    let mut task_directory = None;
    let mut limited_host = None;
    let mut limited_tasks = None;
    let mut held_sts = false;
    let mut pending_request = None;
    let observed = std::panic::AssertUnwindSafe(Box::pin(async {
        let rules: Vec<_> = [ServicePurpose::HubRead, ServicePurpose::HubWrite].into_iter().map(|purpose| ServiceRuleInput::Remote {
            origin: ServiceOrigin::Web,
            binding: RemoteServiceBindingInput::Broker { provider: "broker", purpose, endpoint: wire(&broker.ready, "endpoint"), resource: None },
            credential: CredentialSource::IncomingBearer, incoming: None,
        }).collect();
        let mut fixture = crate::source_server::test_fixture::fixture_with_web_read_providers(
            &[Permission::Read(Module::Hub), Permission::Configure], &[Permission::ManageAuthProfiles], Brand::default(), NativeBinding::KiroV3,
            &["web-installation"], &rules, (None, Some("broker")));
        drop(rules);
        fixture.projects = Arc::clone(&source_owner.projects);
        trust_source_projects(&fixture);
        fixture_owner = Some(configure_hub_fixture(fixture).await);
        let fixture = fixture_owner.as_ref().expect("retained actual source/installation owners");
        let source = resource(fixture, "release/one", ONE);
        let revision = crate::graph_server::tests::browser_provider(&fixture.server.graph_server(), &fixture.raw, wire(&broker.ready, "endpoint"));
        let (directory, tasks, _) = crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
        task_directory = Some(directory); task_owner = Some(tasks.clone());
        host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let web = host.as_ref().unwrap();
        let cookie = web.login(&broker.ready).await;
        let sibling = web.login(&broker.ready).await;
        let identity: Value = checked(checked(web.client.get(format!("{}/auth/session", web.origin)).header(header::COOKIE, cookie.as_str()).send().await, "real Web identity").json().await, "verified identity record");
        let subject = wire(&identity, "subject");
        let issuer = wire(&broker.ready, "issuer");
        let mut input = json!({"project_id":producer,"project_version":fixture.projects[0].2.version(),"publication":{
            "provider":"broker","artifact_type":"skill","id":FILE_ID,"version":"release/one","name":"Web published PROJECT skill","description":"Real source metadata","tags":["web"],"set_latest":true}});
        let unauthorized = request(web, None, PUBLISH, &input.to_string()).await;
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        failure(web, &cookie, &input, StatusCode::FORBIDDEN).await;
        administrators(fixture, issuer, &[(subject, &["project", "global"])]);
        for body in ["{}", r#"{"project_id":null}"#, r#"{"project_id":"bad","project_id":"bad"}"#] {
            assert_eq!(request(web, Some(&cookie), PUBLISH, body).await.status(), StatusCode::BAD_REQUEST);
        }
        for (field, value) in [("project_dir", json!("/foreign")), ("binding", json!("kiro-v3")), ("payload_uri", json!("file:///foreign"))] {
            let mut invalid = input.clone(); invalid[field] = value;
            failure(web, &cookie, &invalid, StatusCode::BAD_REQUEST).await;
        }
        for (field, value) in [("version", json!("latest")), ("artifact_type", json!("ast")), ("expected_sha256", json!("invalid"))] {
            let mut invalid = input.clone(); invalid["publication"][field] = value;
            failure(web, &cookie, &invalid, StatusCode::BAD_REQUEST).await;
        }
        let mut invalid = input.clone(); invalid["publication"]["provider"] = json!("other-provider");
        failure(web, &cookie, &invalid, StatusCode::FORBIDDEN).await;
        invalid = input.clone(); invalid["project_version"] = json!("absent-data-version");
        failure(web, &cookie, &invalid, StatusCode::NOT_FOUND).await;
        invalid = input.clone(); invalid["project_id"] = json!(fixture.projects[1].1);
        failure(web, &cookie, &invalid, StatusCode::FORBIDDEN).await;
        let response = request(web, Some(&cookie), PUBLISH, &input.to_string()).await;
        let status = response.status();
        let published: Value = checked(response.json().await, "first publication public envelope");
        assert_eq!(status, StatusCode::OK, "first publication public_code={} cause={} stage={}",
            published["error"]["code"].as_str().unwrap_or("none"),
            published["outcome"]["cause"].as_str().unwrap_or("none"),
            published["outcome"]["stage"].as_str().unwrap_or("none"));
        assert_eq!(published["status"], "published");
        assert_eq!(published["publication"]["producer"], producer.to_string());
        assert_eq!(published["publication"]["id"], FILE_ID);
        assert_eq!(published["publication"]["version"], "release/one");
        assert_eq!(published["publication"]["payload"], "published");
        assert_eq!(published["publication"]["catalog"], "published");
        assert_eq!(published["project_metadata"], "published");
        let digest = published["publication"]["archive_sha256"].as_str().unwrap().to_owned();
        assert_eq!(digest.len(), 64);
        let catalog = page(web, &cookie, "/api/hub/remote/search", json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":10})).await;
        assert_eq!(catalog["artifacts"][0]["latest"], "release/one");
        let selection = json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"version":"release/one","exact":true});
        let content = page(web, &cookie, "/api/hub/files/content", json!({"remote":{"selection":selection,"path":"SKILL.md"}})).await;
        assert_eq!(content["content"]["content"], ONE);
        let installed = page(web, &cookie, "/api/hub/files/install", json!({"selection":selection,"scope":{"kind":"global_cache"}})).await;
        assert_eq!(installed["status"], "ok");
        assert_eq!(installed["version"], "release/one");
        // PROJECT metadata can remain confirmed when the artifact CAS precondition fails.
        checked(std::fs::write(source.join("SKILL.md"), ONE.to_owned()+"Changed actual source\n"), "changed actual source");
        let conflict = failure(web, &cookie, &input, StatusCode::CONFLICT).await;
        assert_eq!(conflict["outcome"]["project_metadata"], "unchanged");
        assert_eq!(conflict["outcome"]["publication"]["payload"], "not_attempted");
        assert_eq!(conflict["outcome"]["publication"]["catalog"], "not_attempted");
        assert_eq!(conflict["outcome"]["stage"], "catalog_read");
        input["publication"]["expected_sha256"] = json!(digest);
        input["publication"]["set_latest"] = false.into();
        let response = request(web, Some(&sibling), PUBLISH, &input.to_string()).await;
        assert_eq!(response.status(), StatusCode::OK, "actual conditional replacement HTTP");
        let replaced: Value = checked(response.json().await, "publication replacement DTO");
        assert_eq!(replaced["status"], "published");
        assert_ne!(replaced["publication"]["archive_sha256"], published["publication"]["archive_sha256"]);
        assert_eq!(replaced["publication"]["catalog"], "published");
        input["publication"]["version"] = json!("release/two");
        input["publication"].as_object_mut().unwrap().remove("expected_sha256");
        resource(fixture, "release/two", ONE);
        let tasks = task_owner.as_ref().unwrap();
        let (mut limits, timeout, _) = tasks.browser_policy(); limits.max_output_bytes = 256;
        let tasks = tasks.references_test_policy(limits, timeout); limited_tasks = Some(tasks.clone());
        limited_host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let limited = limited_host.as_ref().unwrap();let limited_cookie = limited.login(&broker.ready).await;
        let response = request(limited, Some(&limited_cookie), PUBLISH, &input.to_string()).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(response.headers()["x-graphit-inspection-required"], "request-selection");
        let bytes = checked(response.bytes().await, "bounded publication inspection response");assert!(bytes.len()<=256);
        assert_eq!(checked::<Value,_>(serde_json::from_slice(&bytes), "inspection response")["inspection_required"], true);
        let catalog = page(web, &sibling, "/api/hub/remote/search", json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":10})).await;
        assert_eq!(catalog["artifacts"][0]["latest"], "release/one");
        assert!(catalog["artifacts"][0]["versions"].as_array().unwrap().contains(&json!("release/two")));
        // One causal new-owner witness: the real Broker has already issued STS.
        input["publication"]["version"] = json!("release/cancelled");
        resource(fixture, "release/cancelled", ONE);
        held_sts = true;
        broker.sts_command("hold_sts").await;
        let send = web.client.post(format!("{}{PUBLISH}", web.origin))
            .header(header::ORIGIN, &web.origin).header(header::COOKIE, cookie.as_str())
            .header("X-Graphit-Request", "ui").json(&input);
        pending_request = Some(tokio::spawn(async move { checked(send.send().await, "owned held publication response") }));
        broker.sts_command("await_sts").await;
        let logout = checked(web.client.post(format!("{}/auth/logout", web.origin))
            .header(header::ORIGIN, &web.origin).header(header::COOKIE, cookie.as_str())
            .header("X-Graphit-Request", "ui").send().await, "logout during real issued STS");
        assert_eq!(logout.status(), StatusCode::NO_CONTENT);
        let response = checked(checked(tokio::time::timeout(Duration::from_secs(15), pending_request.take().unwrap()).await,
            "held publication cancels before Broker release"), "held request task joined");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let outcome: Value = checked(response.json().await, "cancelled publication envelope");
        assert!(outcome.get("error").is_some());
        broker.sts_command("release_sts").await;
        held_sts = false;
        let catalog = page(web, &sibling, "/api/hub/remote/search", json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"limit":10})).await;
        assert!(!catalog["artifacts"][0]["versions"].as_array().unwrap().contains(&json!("release/cancelled")), "cancelled pre-publication operation does not expose a new release");
    })).catch_unwind().await;
    let request_cleanup = std::panic::AssertUnwindSafe(async {
        if held_sts {
            broker.sts_command("release_sts").await;
        }
        if let Some(job) = pending_request {
            let _ = checked(
                tokio::time::timeout(Duration::from_secs(45), job).await,
                "owned publication request cleanup join",
            );
        }
    })
    .catch_unwind()
    .await;
    let limited = std::panic::AssertUnwindSafe(async {
        if let Some(web) = limited_host {
            web.close().await;
        }
        if let Some(tasks) = limited_tasks {
            checked(tasks.shutdown().await, "limited Task owner joined");
        }
    })
    .catch_unwind()
    .await;
    let cleanup = std::panic::AssertUnwindSafe(finish_remote_test_owners(
        observed.and(request_cleanup).and(limited),
        host,
        fixture_owner.as_ref().map(|f| &f.server),
        broker,
    ))
    .catch_unwind()
    .await;
    let tasks = std::panic::AssertUnwindSafe(async {
        if let Some(tasks) = task_owner {
            checked(tasks.shutdown().await, "publication Task owner joined");
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
