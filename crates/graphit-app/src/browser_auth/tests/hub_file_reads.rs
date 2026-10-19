//! Real FILE reader ingress. SDK publication seeds the owned remote fixture;
//! it does not qualify publication authority or installed cache mutation.
use super::*;
use graphit_core::{
    artifact_store::{
        CapturedPublicationScope, CapturedSourceReader, CopyPolicy, SourceReadLimits,
    },
    project_identity::ProjectId,
};
use graphit_hub::{ArtifactId, ArtifactType, ConcreteVersion, SourceRef, policy::RequestContext};

const CONTENT: &str = "/api/hub/files/content";
const FILE_ID: &str = "fixture/web-file-reader";
const TEXT: &str = "---\nname: web-file-reader\ndescription: Actual SDK archive fixture\n---\nExact FILE release one\n";

pub(super) fn seed_file<'a>(
    fixture: &'a crate::source_server::test_fixture::Fixture,
    store: &'a dyn object_store::ObjectStore,
    prefix: &'a str,
    producer: ProjectId,
    version: &'a str,
    text: &'a str,
) -> impl std::future::Future<Output = crate::file_publication::Receipt> + 'a {
    seed_file_payload(
        fixture,
        store,
        prefix,
        producer,
        version,
        text,
        crate::remote_file_install::client::Kind::Skill,
    )
}

/// Publishes another FILE kind through the same production archive/catalogue SDK.
pub(super) fn seed_file_payload<'a>(
    fixture: &'a crate::source_server::test_fixture::Fixture,
    store: &'a dyn object_store::ObjectStore,
    prefix: &'a str,
    producer: ProjectId,
    version: &'a str,
    text: &'a str,
    kind: crate::remote_file_install::client::Kind,
) -> impl std::future::Future<Output = crate::file_publication::Receipt> + 'a {
    seed_named_file_payload(
        fixture,
        store,
        prefix,
        producer,
        (FILE_ID, version),
        text,
        kind,
    )
}

pub(super) async fn seed_named_file_payload(
    fixture: &crate::source_server::test_fixture::Fixture,
    store: &dyn object_store::ObjectStore,
    prefix: &str,
    producer: ProjectId,
    identity: (&str, &str),
    text: &str,
    kind: crate::remote_file_install::client::Kind,
) -> crate::file_publication::Receipt {
    let (id, version) = identity;
    let policy = &fixture.server.policy().native_install.hub;
    let cancellation = Cancellation::default();
    let original = checked(
        RequestContext::new(
            policy.content(),
            &cancellation,
            Instant::now() + Duration::from_secs(30),
        ),
        "FILE publisher fixture Original",
    );
    let mut descriptor_context = checked(
        graphit_hub::native::RenderContext::new(
            policy.content(),
            &cancellation,
            original
                .snapshot()
                .required_deadline()
                .expect("fixture deadline"),
        ),
        "Skill fixture descriptor context",
    );
    let entry = match kind {
        crate::remote_file_install::client::Kind::Skill => {
            checked(
                graphit_hub::native::SkillDescriptor::parse(
                    text.as_bytes(),
                    &mut descriptor_context,
                ),
                "valid positive Skill fixture descriptor",
            );
            "SKILL.md"
        }
        crate::remote_file_install::client::Kind::Command => "COMMAND.md",
        _ => panic!("FILE fixture supports only its two actual seeded kinds"),
    };
    let (paths, local_id, _) = &fixture.projects[0];
    let scope = checked(
        CapturedPublicationScope::project(
            paths,
            *local_id,
            fixture.server.policy().scope,
            original.snapshot(),
        ),
        "actual FILE source capture",
    );
    let source = checked(
        SourceRef::resources(
            paths,
            *local_id,
            kind.artifact_type(),
            checked(ArtifactId::new(id, policy.projection()), "FILE fixture ID"),
            checked(
                ConcreteVersion::new(version, policy.projection()),
                "FILE concrete release",
            ),
            policy.projection(),
        ),
        "maintained FILE resource source",
    );
    checked(
        std::fs::create_dir_all(source.directory().as_path()),
        "owned FILE resource directory",
    );
    checked(
        std::fs::write(source.directory().as_path().join(entry), text),
        "owned FILE resource bytes",
    );
    let snapshot_policy = checked(
        policy.snapshot_policy(CopyPolicy::RejectFileLinks, []),
        "FILE source policy",
    );
    let reader = checked(
        CapturedSourceReader::capture_project(
            &scope,
            checked(
                source.directory().as_path().strip_prefix(paths.root()),
                "captured FILE relative source",
            ),
            graphit_fs::NodeKind::Directory,
            &snapshot_policy,
            checked(
                SourceReadLimits::new(64 * 1024, 64 * 1024, policy.copy().total_file_bytes_limit()),
                "FILE source limits",
            ),
            original.snapshot(),
        ),
        "captured actual FILE bytes",
    );
    let metadata = crate::file_publication::Metadata {
        provider: "broker".into(),
        artifact_type: kind,
        id: id.into(),
        version: version.into(),
        name: "Web FILE reader".into(),
        description: None,
        tags: vec![],
        author: None,
        dependencies: None,
        expected_sha256: None,
        set_latest: true,
    };
    let check = || original.check();
    let prepared = checked(
        crate::file_publication::prepare(&metadata, producer, &reader, policy, &original, &check),
        "SDK FILE archive and canonical manifest",
    );
    // Keep this SDK upload future off the test thread poll stack.
    let receipt = Box::pin(crate::file_publication::publish(
        crate::file_publication::Input {
            store,
            storage_prefix: prefix,
            prepared,
        },
        &original,
        check,
    ))
    .await
    .unwrap_or_else(|error| {
        let Some(failure) = error
            .get_ref()
            .and_then(|source| source.downcast_ref::<crate::file_publication::Failure>())
        else {
            panic!(
                "FILE fixture publication before receipt: kind={:?}",
                error.kind()
            );
        };
        // Inspect locally, print only fixed protocol categories. Never echo
        // ObjectStore errors: their display can contain private endpoints.
        let mut current: Option<&(dyn std::error::Error + 'static)> = Some(&failure.cause);
        let mut code = "unclassified";
        while let Some(error) = current {
            let detail = error.to_string();
            if let Some(found) = [
            "AccessDenied",
            "NoSuchKey",
            "InvalidAccessKeyId",
            "SignatureDoesNotMatch",
            "ExpiredToken",
            "RequestTimeTooSkewed",
            "ServiceUnavailable",
            "InvalidPart",
            "InvalidPartOrder",
            "EntityTooSmall",
            "InvalidRequest",
            "NotImplemented",
            "MalformedXML",
        ]
        .into_iter()
            .find(|code| detail.contains(code)) {
                code = found;
                break;
            }
            current = if let Some(error) = error.downcast_ref::<std::io::Error>() {
                error.get_ref().map(|inner| inner as &(dyn std::error::Error + 'static))
            } else if let Some(error) = error.downcast_ref::<std::sync::Arc<std::io::Error>>() {
                Some(error.as_ref())
            } else {
                error.source()
            };
        }
        panic!(
            "FILE fixture publication: stage={}, kind={:?}, cause={}, code={}, payload={}, catalog={}",
            failure.stage,
            failure.cause.kind(),
            crate::native_publication::cause_code(&failure.cause),
            code,
            failure.receipt.payload,
            failure.receipt.catalog
        );
    });
    assert!(
        receipt.payload == "published" || receipt.payload == "unchanged",
        "confirmed immutable FILE payload"
    );
    assert_eq!(
        receipt.archive_sha256.len(),
        64,
        "actual FILE archive digest"
    );
    receipt
}

pub(super) fn installation_children(root: &std::path::Path) -> Vec<std::ffi::OsString> {
    let mut entries: Vec<_> = checked(std::fs::read_dir(root), "owned installation directory")
        .map(|entry| checked(entry, "owned installation entry").file_name())
        .collect();
    entries.sort();
    entries
}
pub(super) fn registry_bytes(path: &std::path::Path) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => panic!("owned registry snapshot"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO fixture and coordinated App emission"]
async fn real_web_remote_file_content_uses_sdk_archive_without_installation_effects() {
    use futures_util::FutureExt;
    use graphit_auth::{ServiceOrigin, ServiceRuleInput};
    use hub_remote_catalog::{
        StorageRelay, configure_standalone, page, put_json, rejected, request,
    };
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let producer = checked(
        "01D39ZY06FGSCTVN4T2V9PKHFZ".parse::<ProjectId>(),
        "actual granted FILE producer",
    );
    let second = checked(
        "01D39ZY06FGSCTVN4T2V9PKHG0".parse::<ProjectId>(),
        "independent granted producer",
    );
    let broker = BrokerProcess::start_web_projects(
        std::path::Path::new(&binary),
        &producer.to_string(),
        Some(&second.to_string()),
        false,
        true,
    )
    .await;
    let mut fixture_owner = None;
    let mut host = None;
    let mut task_owner = None;
    let mut task_directory = None;
    let mut relay_owner = None;
    let mut readers = Vec::new();
    let mut read_baseline = None;
    let observed = std::panic::AssertUnwindSafe(async {
        let extra = [ServiceRuleInput::StandaloneS3 { purpose: graphit_auth::ServicePurpose::HubRead, origin: ServiceOrigin::Web, provider: "catalog-storage", profile: Some("catalog-keys") }];
        fixture_owner = Some(fixture_with_web_admissions(&broker.ready, (None, Some("broker")), &extra).await);
        let fixture = fixture_owner.as_ref().expect("owned FILE Source");
        let store = seed_store(&broker.ready, producer).await;
        seed_file(fixture, store.as_ref(), wire(&broker.ready, "prefix"), producer, "release/one", TEXT).await;
        relay_owner = Some(StorageRelay::start(wire(&broker.ready, "storage_endpoint")).await);
        let relay = relay_owner.as_ref().expect("owned FILE storage relay");
        configure_standalone(fixture, &broker.ready, &relay.endpoint).await;
        let revision = crate::graph_server::tests::browser_provider(&fixture.server.graph_server(), &fixture.raw, wire(&broker.ready, "endpoint"));
        let (directory, tasks, _) = crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
        task_directory = Some(directory);
        task_owner = Some(tasks.clone());
        host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let host = host.as_ref().expect("owned FILE Web host");
        let cookie = host.login(&broker.ready).await;
        let sibling = host.login(&broker.ready).await;
        let mut second_ready = broker.ready.clone();
        assert!(second_ready["web_second"].is_object(), "required second actual Web identity");
        second_ready["web"] = broker.ready["web_second"].clone();
        let second_cookie = host.login(&second_ready).await;
        let session = checked(host.client.get(format!("{}/auth/session", host.origin)).header(header::COOKIE, cookie.as_str()).send().await, "actual FILE session identity");
        assert_eq!(session.status(), StatusCode::OK);
        let identity: Value = checked(session.json().await, "verified FILE Web identity");
        assert!(wire(&identity, "subject") != wire(&broker.ready["web_second"], "subject"), "separate real subjects");
        let store = hub_remote_catalog::standalone_store(&broker.ready);
        let prefix = wire(&broker.ready["standalone_storage"], "prefix");
        hub_remote_catalog::seed_project_metadata(&fixture.server, store.as_ref(), prefix, producer).await;
        let publisher_store = hub_remote_catalog::standalone_publisher_store(&broker.ready);
        seed_file(
            fixture,
            publisher_store.as_ref(),
            prefix,
            producer,
            "release/one",
            TEXT,
        )
        .await;
        for key in ["v1/global/projects.json", "v1/anonymous/projects.json", "v1/authenticated/projects.json"] {
            put_json(store.as_ref(), prefix, key, json!({"v":1,"projects":[]})).await;
        }
        let user_key = format!("v1/users/{}/{}/projects.json", URL_SAFE_NO_PAD.encode(wire(&broker.ready,"issuer")), URL_SAFE_NO_PAD.encode(wire(&identity,"subject")));
        put_json(store.as_ref(), prefix, &user_key, json!({"v":1,"projects":[{"id":producer}]})).await;
        let registry = registry_bytes(&fixture.server.policy().registry.path());
        let children = installation_children(fixture.paths.root());
        read_baseline = Some((registry.clone(), children.clone()));
        let remote = json!({"remote":{"selection":{"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":FILE_ID,"version":"latest","exact":false},"path":"SKILL.md"}});
        rejected(host, None, CONTENT, &remote.to_string(), StatusCode::UNAUTHORIZED, "unauthorized").await;
        for transport in ["broker", "standalone"] {
            let mut input = remote.clone();
            input["remote"]["selection"]["transport"] = json!(transport);
            if transport == "standalone" {
                input["remote"]["selection"]["provider"] = json!("catalog-storage");
                input["remote"]["selection"]["profile"] = json!("catalog-keys");
            }
            let body = page(host, &cookie, CONTENT, input.clone()).await;
            assert!(body["cache"] == "ephemeral" && body["source"] == "remote" && body["version"] == "release/one", "resolved concrete FILE release, ephemeral source");
            assert!(body["content"]["path"] == "SKILL.md" && body["content"]["content"] == TEXT, "exact SDK archive content");
            rejected(host, Some(&second_cookie), CONTENT, &input.to_string(), StatusCode::FORBIDDEN, "permission_denied").await;
            input["remote"]["path"] = json!("../SKILL.md");
            rejected(host, Some(&cookie), CONTENT, &input.to_string(), StatusCode::BAD_REQUEST, "invalid_request").await;
            input["remote"]["path"] = json!("absent.md");
            rejected(host, Some(&cookie), CONTENT, &input.to_string(), StatusCode::NOT_FOUND, "file_not_found").await;
            assert!(registry_bytes(&fixture.server.policy().registry.path()) == registry, "remote FILE read never publishes registry or pins");
            assert!(installation_children(fixture.paths.root()) == children, "temporary FILE stages closed on success and failure");
        }
        // Retain the actual archive response after backend I/O, before the SDK
        // receives it. This exercises archive download, not extraction.
        let mut held = remote.clone();
        held["remote"]["selection"]["transport"] = json!("standalone");
        held["remote"]["selection"]["provider"] = json!("catalog-storage");
        held["remote"]["selection"]["profile"] = json!("catalog-keys");
        held["remote"]["selection"]["version"] = json!("release/one");
        held["remote"]["selection"]["exact"] = json!(true);
        let lifecycle_client = checked(reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(45)).build(), "FILE lifecycle client exceeds server deadline");
        for disconnect in [false, true] {
            // Drain completed jobs; the pending oracle below polls this client.
            { let mut jobs = host.service.0.jobs.lock().await; while let Some(result) = jobs.try_join_next() { checked(result, "completed FILE read job joined"); } }
            relay.hold_next_file();
            let request = lifecycle_client.post(format!("{}{CONTENT}", host.origin))
                .header(header::ORIGIN, &host.origin)
                .header(header::COOKIE, cookie.as_str())
                .header("X-Graphit-Request", "ui")
                .json(&held);
            readers.push(tokio::spawn(async move { checked(request.send().await, "real in-flight FILE response") }));
            relay.entered().await;
            let pending = tokio::time::timeout(Duration::from_millis(100), readers.last_mut().expect("owned current FILE client")).await;
            if let Ok(result) = pending {
                readers.pop();
                let phase = if disconnect { "disconnect" } else { "deadline" };
                match result {
                    Ok(response) => panic!("{phase}: current FILE client returned status {} before held archive release", response.status().as_u16()),
                    Err(_) => panic!("{phase}: current FILE client task failed before held archive release"),
                }
            }
            if disconnect {
                let reader = readers.last_mut().expect("owned disconnected FILE reader");
                reader.abort();
                let outcome = reader.await;
                readers.pop();
                assert!(outcome.is_err_and(|error| error.is_cancelled()), "actual FILE client disconnected");
            } else {
                let outcome = tokio::time::timeout(Duration::from_secs(40), readers.last_mut().expect("owned deadline FILE reader")).await;
                if outcome.is_ok() { readers.pop(); }
                let response = checked(checked(outcome, "FILE deadline response guard"), "deadline FILE client joined");
                assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT, "Original deadline during actual archive GET");
            }
            relay.release();
            // This joins an owner completion; final shutdown below drains all jobs.
            hub_remote_catalog::join_catalog_read_job(host).await;
            assert!(registry_bytes(&fixture.server.policy().registry.path()) == registry, "cancelled FILE read preserves registry bytes");
            assert!(installation_children(fixture.paths.root()) == children, "installation children unchanged after cancelled request");
            let healthy = page(host, &sibling, CONTENT, held.clone()).await;
            assert!(healthy["cache"] == "ephemeral" && healthy["content"]["content"] == TEXT, "sibling retains genuine FILE access");
            assert!(installation_children(fixture.paths.root()) == children, "healthy sibling closes its temporary FILE stage");
        }
        // Retire only storage after the genuine archive GET has entered. The
        // identity provider stays live for an independent Broker sibling read.
        { let mut jobs = host.service.0.jobs.lock().await; while let Some(result) = jobs.try_join_next() { checked(result, "completed FILE read before provider retirement"); } }
        relay.hold_next_file();
        let retirement_request = lifecycle_client.post(format!("{}{CONTENT}", host.origin))
            .header(header::ORIGIN, &host.origin)
            .header(header::COOKIE, cookie.as_str())
            .header("X-Graphit-Request", "ui")
            .json(&held);
        readers.push(tokio::spawn(async move { checked(retirement_request.send().await, "real retiring FILE response") }));
        relay.entered().await;
        let pending = tokio::time::timeout(Duration::from_millis(100), readers.last_mut().expect("owned current retirement FILE client")).await;
        if let Ok(result) = pending {
            readers.pop();
            match result {
                Ok(response) => panic!("retirement: current FILE client returned status {} before held archive release", response.status().as_u16()),
                Err(_) => panic!("retirement: current FILE client task failed before held archive release"),
            }
        }
        {
            use graphit_auth::profile_manager::{NativeProfileManager, ProfileManagerAuthority};
            let cancel = Cancellation::default();
            let original = checked(RequestContext::new(fixture.server.policy().content, &cancel, Instant::now() + Duration::from_secs(30)), "provider retirement Original");
            let issuer = fixture.auth.issuer();
            let caller = checked(issuer.authenticate_runtime(&fixture.raw, &original), "real provider-retirement manager caller");
            let session = checked(issuer.bootstrap_admin(&caller), "real provider-retirement admin session");
            let parent = checked(issuer.admin_parent(&caller, &session), "real provider-retirement authority");
            let manager = NativeProfileManager::new(issuer);
            let snapshot = checked(manager.begin(ProfileManagerAuthority::Admin(&parent)), "actual storage provider snapshot");
            let revision = snapshot.revision();
            let publication = checked(manager.remove_provider(snapshot, "catalog-storage", true, revision), "retire storage provider with dependent profile");
            assert!(publication.failure().is_none() && publication.cleanup_failure().is_none(), "actual storage provider removal committed");
            assert!(publication.into_pending_cleanup().is_none(), "provider retirement cleanup completed");
            let snapshot = checked(manager.begin(ProfileManagerAuthority::Admin(&parent)), "observe committed provider retirement");
            assert!(snapshot.providers().all(|provider| provider.name() != "catalog-storage"), "retired storage provider absent");
            assert!(snapshot.profiles().all(|profile| profile.name() != "catalog-keys" && profile.provider() != "catalog-storage"), "dependent storage profile removed");
        }
        relay.release();
        let retired = tokio::time::timeout(Duration::from_secs(15), readers.last_mut().expect("owned retiring FILE reader")).await;
        if retired.is_ok() { readers.pop(); }
        let retired = checked(checked(retired, "provider-retirement response guard"), "retiring FILE client joined");
        assert_eq!(retired.status(), StatusCode::INTERNAL_SERVER_ERROR, "retired SDK lease remains a finite IO failure");
        let retired: Value = checked(retired.json().await, "finite provider-retirement error envelope");
        assert_eq!(retired["error"]["code"], "file_read_unavailable");
        hub_remote_catalog::join_catalog_read_job(host).await;
        assert!(registry_bytes(&fixture.server.policy().registry.path()) == registry, "retired FILE read preserves registry bytes");
        assert!(installation_children(fixture.paths.root()) == children, "installation children unchanged after retired request");
        let healthy = page(host, &sibling, CONTENT, remote.clone()).await;
        assert!(healthy["cache"] == "ephemeral" && healthy["content"]["content"] == TEXT, "Broker sibling retains genuine FILE access after storage retirement");
        assert!(installation_children(fixture.paths.root()) == children, "Broker sibling closes its temporary FILE stage");
        for body in [r#"{}"#, r#"{"remote":null}"#, r#"{"installed":null}"#, r#"{"unknown":true}"#, r#"{"remote":{},"remote":{}}"#] {
            rejected(host, Some(&cookie), CONTENT, body, StatusCode::BAD_REQUEST, "invalid_request").await;
        }
        let oversized = request(host, Some(&cookie), CONTENT, &" ".repeat(65537)).await;
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE, "existing body ceiling");
        remove_current_provider(fixture);
        rejected(host, Some(&cookie), CONTENT, &remote.to_string(), StatusCode::FORBIDDEN, "permission_denied").await;
    }).catch_unwind().await;
    for reader in readers {
        reader.abort();
        let _ = reader.await;
    }
    let relay_result = std::panic::AssertUnwindSafe(async {
        if let Some(relay) = relay_owner {
            relay.close().await;
        }
    })
    .catch_unwind()
    .await;
    let cleanup = std::panic::AssertUnwindSafe(finish_remote_test_owners(
        observed.and(relay_result),
        host,
        fixture_owner.as_ref().map(|v| &v.server),
        broker,
    ))
    .catch_unwind()
    .await;
    let tasks = std::panic::AssertUnwindSafe(async {
        if let Some(tasks) = task_owner {
            checked(tasks.shutdown().await, "FILE Task owner shutdown");
        }
    })
    .catch_unwind()
    .await;
    let effects = std::panic::AssertUnwindSafe(async {
        if let (Some(fixture), Some((registry, children))) =
            (fixture_owner.as_ref(), read_baseline.as_ref())
        {
            assert!(
                registry_bytes(&fixture.server.policy().registry.path()).eq(registry),
                "FILE registry unchanged after complete owner shutdown"
            );
            assert!(
                installation_children(fixture.paths.root()).eq(children),
                "installation children unchanged after complete owner shutdown"
            );
        }
    })
    .catch_unwind()
    .await;
    drop(task_directory);
    for result in [cleanup, tasks, effects] {
        if let Err(failure) = result {
            std::panic::resume_unwind(failure);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires genuine Broker Web/STSv3/MinIO fixture and coordinated App emission"]
async fn real_web_installed_file_reads_join_project_and_verify_existing_cache() {
    use futures_util::FutureExt;
    use graphit_auth::{ExternalEffect, Permission, ServiceOrigin, ServiceRuleInput};
    use graphit_core::{brand::Brand, module_settings::Module};
    use graphit_hub::{InstallationTarget, NativeBinding};
    use hub_remote_catalog::{configure_standalone, page, put_json, rejected};
    use rmcp::{
        model::{CallToolRequestParams, ProtocolVersion},
        service::{ClientLifecycleMode, ClientServiceExt},
        transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        },
    };
    const LIST: &str = "/api/hub/installed/files";
    const TEXT_TWO: &str = "---\nname: web-file-reader\ndescription: Actual SDK archive fixture\n---\nExact FILE release two\n";
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let rules = [
        ServiceRuleInput::StandaloneS3 {
            purpose: graphit_auth::ServicePurpose::HubRead,
            origin: ServiceOrigin::Runtime,
            provider: "catalog-storage",
            profile: Some("catalog-keys"),
        },
        ServiceRuleInput::StandaloneS3 {
            purpose: graphit_auth::ServicePurpose::HubRead,
            origin: ServiceOrigin::Web,
            provider: "catalog-storage",
            profile: Some("catalog-keys"),
        },
    ];
    // Genuine local identities exist before Broker grants are authored. No
    // identity file, policy store or request can rewrite the local ProjectId.
    let fixture = configure_hub_fixture(
        crate::source_server::test_fixture::fixture_with_web_read_providers(
            &[
                Permission::Read(Module::Hub),
                Permission::External(ExternalEffect::RemoteRequest),
            ],
            &[
                Permission::ManageAuthProfiles,
                Permission::Configure,
                Permission::ManageCache,
                Permission::External(ExternalEffect::PublishedFilesystem),
            ],
            Brand::default(),
            NativeBinding::KiroV3,
            &["file-primary", "file-sibling"],
            &rules,
            (None, Some("broker")),
        ),
    )
    .await;
    let producer = fixture.projects[0].1;
    let second = checked(
        "01D39ZY06FGSCTVN4T2V9PKHG0".parse::<ProjectId>(),
        "independent Web grant",
    );
    let started = std::panic::AssertUnwindSafe(BrokerProcess::start_web_projects(
        std::path::Path::new(&binary),
        &producer.to_string(),
        Some(&second.to_string()),
        false,
        true,
    ))
    .catch_unwind()
    .await;
    let broker = match started {
        Ok(broker) => broker,
        Err(failure) => {
            let _ = fixture.server.shutdown().await;
            std::panic::resume_unwind(failure);
        }
    };
    let mut host = None;
    let mut task_owner = None;
    let mut task_directory = None;
    let mut native_server = None;
    let mut foreign_source = None;
    let mut foreign_host = None;
    let mut foreign_directory = None;
    let observed = std::panic::AssertUnwindSafe(async {
        configure_standalone(&fixture, &broker.ready, wire(&broker.ready, "storage_endpoint")).await;
        let store = hub_remote_catalog::standalone_store(&broker.ready);
        let prefix = wire(&broker.ready["standalone_storage"], "prefix");
        hub_remote_catalog::seed_project_metadata(&fixture.server, store.as_ref(), prefix, producer).await;
        // This ACL admits the separate Native setup principal. Remove it before
        // Web proofs so the Local profile's claimed identity grants nothing.
        put_json(store.as_ref(), prefix, "v1/global/projects.json", json!({"v":1,"projects":[{"id":producer}]})).await;
        let publisher_store = hub_remote_catalog::standalone_publisher_store(&broker.ready);
        seed_file(
            &fixture,
            publisher_store.as_ref(),
            prefix,
            producer,
            "release/one",
            TEXT,
        )
        .await;
        seed_file(
            &fixture,
            publisher_store.as_ref(),
            prefix,
            producer,
            "release/two",
            TEXT_TWO,
        )
        .await;
        let listener = checked(graphit_core::tcp_listener::reserve(graphit_core::tcp_listener::Options {
            address: std::net::Ipv4Addr::LOCALHOST.into(),
            ports: graphit_core::tcp_listener::PortSelection::Ephemeral,
            timeout: Duration::from_secs(5),
        }, &Cancellation::default()), "actual Native Source listener");
        let uri = format!("http://{}/mcp", listener.address());
        let serving = fixture.server.clone();
        native_server = Some(tokio::spawn(async move { serving.serve(listener).await }));
        let mut headers = std::collections::HashMap::new();
        let mut bearer = checked(axum::http::HeaderValue::from_str(&format!("Bearer {}", fixture.raw)), "private Native setup authorization");
        bearer.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, bearer);
        headers.insert(axum::http::HeaderName::from_static("x-graphit-auth-origin"), axum::http::HeaderValue::from_static("runtime"));
        let client = checked(().serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(reqwest::Client::new(), StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers)),
            ClientLifecycleMode::Discover { preferred_versions: vec![ProtocolVersion::LATEST] },
        ).await, "actual Native MCP setup client");
        let installed = std::panic::AssertUnwindSafe(async {
            for (version, global) in [("release/one", true), ("release/two", true), ("release/two", false)] {
                let mut input = json!({"selection":{"transport":"standalone","provider":"catalog-storage","profile":"catalog-keys","producer":producer,"artifact_type":"skill","id":FILE_ID,"version":version,"exact":true},"global":global});
                if !global {
                    input["project_dir"] = json!(fixture.projects[0].0.root());
                    input["project_version"] = json!(fixture.projects[0].2.version());
                    input["binding"] = json!(NativeBinding::KiroV3);
                    input["alias"] = json!("web-file-project");
                }
                let result = checked(client.call_tool(CallToolRequestParams::new("graphit_hub_install").with_arguments(input.as_object().expect("typed install setup").clone())).await, "productive Native FILE installation");
                let output = result.structured_content.as_ref().expect("Native FILE setup structured outcome");
                let code = output["code"].as_str().filter(|code| matches!(*code, "invalid_arguments" | "authentication_or_permission_denied" | "artifact_not_found" | "installation_conflict" | "remote_file_too_large" | "cancelled" | "deadline" | "unavailable" | "unsupported" | "invalid_remote_file" | "file_installation_failed" | "file_job_failed" | "request_cancelled_or_expired" | "stopped")).unwrap_or("unclassified_public_error");
                let effects = output.get("outcome").unwrap_or(output);
                let cache_phase = effects["cache"].as_str().filter(|phase| matches!(*phase, "not_attempted" | "in_progress" | "Created" | "Updated" | "Unchanged" | "unknown")).unwrap_or("unclassified");
                let publication_unattempted = effects["publication"] == "not_attempted";
                assert!(result.is_error == Some(false) && output["status"] == "ok", "Native FILE setup must succeed; is_error={:?} public_code={code} cache_phase={cache_phase} publication_unattempted={publication_unattempted} global={global}", result.is_error);
                assert!(matches!(output["cache"].as_str(), Some("Created" | "Unchanged")), "Native setup confirms actual cache receipt");
                assert!(output["publication"] != "not_attempted", "Native setup confirms actual registry phase");
            }
        }).catch_unwind().await;
        let cancelled = client.cancel().await;
        if let Err(failure) = installed { std::panic::resume_unwind(failure); }
        checked(cancelled, "Native MCP setup owner joined");
        let revision = crate::graph_server::tests::browser_provider(&fixture.server.graph_server(), &fixture.raw, wire(&broker.ready, "endpoint"));
        let (directory, tasks, _) = crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
        task_directory = Some(directory);
        task_owner = Some(tasks.clone());
        host = Some(WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await);
        let host = host.as_ref().expect("owned installed FILE Web host");
        let cookie = host.login(&broker.ready).await;
        let mut second_ready = broker.ready.clone();
        assert!(second_ready["web_second"].is_object(), "required actual second Web identity");
        second_ready["web"] = broker.ready["web_second"].clone();
        let second_cookie = host.login(&second_ready).await;
        let identity = checked(host.client.get(format!("{}/auth/session", host.origin)).header(header::COOKIE,cookie.as_str()).send().await, "actual installed FILE Web session");
        assert_eq!(identity.status(), StatusCode::OK);
        let identity: Value = checked(identity.json().await, "actual installed FILE subject");
        for key in ["v1/global/projects.json", "v1/anonymous/projects.json", "v1/authenticated/projects.json"] {
            put_json(store.as_ref(), prefix, key, json!({"v":1,"projects":[]})).await;
        }
        let user_key = format!("v1/users/{}/{}/projects.json", URL_SAFE_NO_PAD.encode(wire(&broker.ready,"issuer")), URL_SAFE_NO_PAD.encode(wire(&identity,"subject")));
        put_json(store.as_ref(), prefix, &user_key, json!({"v":1,"projects":[{"id":producer}]})).await;
        let global = json!({"kind":"global_cache"});
        let project = json!({"kind":"project","project_id":producer,"project_version":fixture.projects[0].2.version()});
        let listing = json!({"scope":global,"artifact_type":"skill","id":FILE_ID,"limit":1});
        let first = page(host, &cookie, LIST, listing.clone()).await;
        assert!(first["entries"].as_array().is_some_and(|v|v.len()==1) && first["next_offset"] == 1, "real installed page position");
        let mut next = listing.clone(); next["offset"] = first["next_offset"].clone();
        let last = page(host, &cookie, LIST, next).await;
        assert!(last["entries"].as_array().is_some_and(|v|v.len()==1) && last["next_offset"].is_null(), "installed pagination EOF");
        assert!(first["entries"][0]["version"] != last["entries"][0]["version"], "two concrete installed releases");
        assert!(first["entries"][0]["target"] == json!(InstallationTarget::GlobalCache), "GlobalCache excludes GlobalNative and PROJECT");
        for entry in [&first["entries"][0], &last["entries"][0]] {
            assert_eq!(entry["source_origin"], "HubCache");
            assert_eq!(entry["origin"]["transport"], "standalone");
        }
        rejected(host, Some(&second_cookie), LIST, &listing.to_string(), StatusCode::FORBIDDEN, "permission_denied").await;
        let mut project_list = listing.clone();project_list["scope"] = project.clone(); project_list["binding"] = json!(NativeBinding::KiroV3);
        let project_page = page(host, &cookie, LIST, project_list.clone()).await;
        assert!(project_page["entries"][0]["version"] == "release/two", "same caller and Original PROJECT-read join");
        assert_eq!(project_page["entries"][0]["source_origin"], "HubCache");
        assert_eq!(project_page["entries"][0]["origin"]["transport"], "standalone");
        rejected(host, Some(&second_cookie), LIST, &project_list.to_string(), StatusCode::FORBIDDEN, "permission_denied").await;
        let mut foreign = project_list.clone(); foreign["scope"]["project_id"] = json!(fixture.projects[1].1);
        rejected(host, Some(&cookie), LIST, &foreign.to_string(), StatusCode::FORBIDDEN, "permission_denied").await;
        foreign["scope"]["project_id"] = json!(second);
        rejected(host, Some(&cookie), LIST, &foreign.to_string(), StatusCode::NOT_FOUND, "file_not_found").await;
        foreign = project_list.clone(); foreign["scope"]["project_version"] = json!("absent-version");
        rejected(host, Some(&cookie), LIST, &foreign.to_string(), StatusCode::NOT_FOUND, "file_not_found").await;
        let installed = json!({"installed":{"scope":global,"artifact_type":"skill","id":FILE_ID,"version":"release/one","path":"SKILL.md"}});
        let registry = registry_bytes(&fixture.server.policy().registry.path());
        let content = page(host, &cookie, CONTENT, installed.clone()).await;
        assert!(content["cache"] == "Verified" && content["content"]["content"] == TEXT, "existing receipt verifies exact first release");
        let mut project_content = installed.clone(); project_content["installed"]["scope"] = project; project_content["installed"]["version"] = json!("release/two");project_content["installed"]["binding"] = json!(NativeBinding::KiroV3);
        assert!(page(host,&cookie,CONTENT,project_content.clone()).await["content"]["content"] == TEXT_TWO, "installed PROJECT exact version and binding");
        rejected(host,Some(&second_cookie),CONTENT,&project_content.to_string(),StatusCode::FORBIDDEN,"permission_denied").await;
        let one = checked(ConcreteVersion::new("release/one", fixture.server.policy().native_install.hub.projection()), "installed concrete version");
        let cache = {
            let document = checked(fixture.server.policy().registry.load(), "actual Native-created registry");
            document.get_version(ArtifactType::Skill,FILE_ID,&one).and_then(|v|v.installations.iter().find(|v|v.target==InstallationTarget::GlobalCache)).expect("actual installed global tuple").original_source.directory().as_path().to_owned()
        };
        checked(std::fs::remove_dir_all(&cache), "remove owned cache fixture payload");
        rejected(host,Some(&cookie),CONTENT,&installed.to_string(),StatusCode::NOT_FOUND,"file_not_found").await;
        assert!(!cache.exists() && registry_bytes(&fixture.server.policy().registry.path()) == registry, "installed read neither hydrates cache nor repins registry");
        assert!(page(host,&cookie,CONTENT,project_content).await["content"]["content"] == TEXT_TWO, "healthy other exact release after missing cache");
        let two = checked(ConcreteVersion::new("release/two", fixture.server.policy().native_install.hub.projection()), "second installed concrete version");
        let cache_two = {
            let document = checked(fixture.server.policy().registry.load(), "actual second installed tuple");
            document.get_version(ArtifactType::Skill, FILE_ID, &two).and_then(|v|v.installations.iter().find(|v|v.target==InstallationTarget::GlobalCache)).expect("second actual GlobalCache tuple").original_source.directory().as_path().to_owned()
        };
        checked(std::fs::write(cache_two.join("SKILL.md"), "changed owned payload"), "corrupt owned cache fixture bytes");
        let mut altered = installed.clone(); altered["installed"]["version"] = json!("release/two");
        rejected(host,Some(&cookie),CONTENT,&altered.to_string(),StatusCode::CONFLICT,"state_conflict").await;
        assert!(checked(std::fs::read(cache_two.join("SKILL.md")), "read owned changed cache") == b"changed owned payload" && registry_bytes(&fixture.server.policy().registry.path()) == registry, "corruption is not repaired or repinned by an installed read");
        for invalid in [
            json!({"scope":{"kind":"global_cache"},"limit":0}),
            json!({"scope":{"kind":"global_cache"},"limit":1,"binding":NativeBinding::KiroV3}),
            json!({"scope":{"kind":"global_cache"},"limit":1,"unknown":true}),
            json!({"scope":{"kind":"global_cache"},"limit":1,"version":null}),
        ] { rejected(host,Some(&cookie),LIST,&invalid.to_string(),StatusCode::BAD_REQUEST,"invalid_request").await; }
        foreign_directory = Some(checked(tempfile::tempdir(), "foreign registry fixture root"));
        let directory = foreign_directory.as_ref().expect("owned foreign registry directory");
        let foreign_paths = checked(graphit_core::project_paths::InstallationPaths::resolve(fixture.paths.brand().clone(), directory.path(), None, Some(&directory.path().join("other-installation"))), "distinct physical registry root");
        checked(std::fs::create_dir_all(foreign_paths.root()), "owned foreign installation directory");
        let graphit_core::artifact_store::StoreOwner::Installation { installation_id } = fixture.auth.installation().live.owner() else { panic!("actual Installation owner"); };
        let mut policy = fixture.server.policy().clone();
        policy.registry = checked(graphit_hub::registry::HubRegistry::global(&foreign_paths, installation_id, policy.native_install.hub.hub()), "same ID with foreign physical registry");
        assert!(policy.registry.path().parent() != Some(fixture.paths.root()), "physical mismatch is the only registry binding change");
        foreign_source = Some(checked(crate::source_server::SourceServer::new(Arc::clone(&fixture.auth), Arc::clone(&fixture.projects), 0, policy, CancellationToken::new()), "Source with deliberate mismatched registry fixture"));
        foreign_host = Some(WebHost::start(foreign_source.as_ref().expect("owned foreign Source").clone(), Some(task_owner.as_ref().expect("owned Task fixture").clone()), &broker.ready, revision).await);
        let other_host = foreign_host.as_ref().expect("owned foreign registry Web host");
        let other_cookie = other_host.login(&broker.ready).await;
        rejected(other_host,Some(&other_cookie),LIST,&listing.to_string(),StatusCode::FORBIDDEN,"permission_denied").await;
        assert!(!foreign_paths.root().join(fixture.server.policy().registry.path().file_name().expect("registry filename")).exists(), "foreign empty registry never becomes an admitted read");
        remove_current_provider(&fixture);
        rejected(host,Some(&cookie),LIST,&listing.to_string(),StatusCode::FORBIDDEN,"permission_denied").await;
    }).catch_unwind().await;
    let foreign_web = std::panic::AssertUnwindSafe(async {
        if let Some(host) = foreign_host {
            host.close().await;
        }
    })
    .catch_unwind()
    .await;
    let foreign_source = std::panic::AssertUnwindSafe(async {
        if let Some(source) = foreign_source {
            checked(source.shutdown().await, "foreign registry Source shutdown");
        }
    })
    .catch_unwind()
    .await;
    let cleanup = std::panic::AssertUnwindSafe(finish_remote_test_owners(
        observed,
        host,
        Some(&fixture.server),
        broker,
    ))
    .catch_unwind()
    .await;
    let native = std::panic::AssertUnwindSafe(async {
        if let Some(mut server) = native_server {
            match tokio::time::timeout(Duration::from_secs(15), &mut server).await {
                Ok(joined) => checked(
                    checked(joined, "Native Source server join"),
                    "Native Source server shutdown",
                ),
                Err(_) => {
                    server.abort();
                    let _ = server.await;
                    panic!("Native Source shutdown after abort and join");
                }
            }
        }
    })
    .catch_unwind()
    .await;
    let tasks = std::panic::AssertUnwindSafe(async {
        if let Some(tasks) = task_owner {
            checked(tasks.shutdown().await, "installed FILE Task owner shutdown");
        }
    })
    .catch_unwind()
    .await;
    drop(task_directory);
    drop(foreign_directory);
    for result in [foreign_web, foreign_source, cleanup, native, tasks] {
        if let Err(failure) = result {
            std::panic::resume_unwind(failure);
        }
    }
}
