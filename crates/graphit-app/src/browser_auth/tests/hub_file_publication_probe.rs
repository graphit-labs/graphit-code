//! Retained genuine SourcePROJECT fixture for the new React publication journey.
use super::*;
use graphit_auth::{
    CredentialSource, Permission, RemoteServiceBindingInput, ServiceOrigin, ServicePurpose,
    ServiceRuleInput,
};
use graphit_core::{brand::Brand, module_settings::Module};
use graphit_hub::NativeBinding;
use hub_file_publication::{FILE_ID, ONE, administrators, resource, trust_source_projects};
use sha2::{Digest, Sha256};
use std::io::Write;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a real browser observer and GRAPHIT_UI_BROWSER_PROBE_FILE"]
async fn real_web_project_file_publication_browser_probe() {
    use futures_util::FutureExt;
    let received = match std::env::var("GRAPHIT_UI_HUB_JOURNEY") {
        Err(std::env::VarError::NotPresent) => false,
        Ok(value) if value == "received_upload_update_all" => true,
        _ => panic!("unsupported browser probe journey"),
    };
    let probe = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_UI_BROWSER_PROBE_FILE").expect("explicit browser probe path"),
    );
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
        probe.with_extension("upload.zip"),
    ] {
        assert!(!path.exists(), "use fresh publication browser probe paths");
    }
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let source_owner = crate::source_server::test_fixture::fixture_with_web_read_providers(
        &[Permission::Read(Module::Hub)],
        &[Permission::ManageAuthProfiles],
        Brand::default(),
        NativeBinding::KiroV3,
        &["web-publication-source"],
        &[],
        (None, Some("broker")),
    );
    checked(
        source_owner.server.shutdown().await,
        "source setup owner joined",
    );
    let producer = source_owner.projects[0].1;
    let broker =
        BrokerProcess::start_web_publication(std::path::Path::new(&binary), &producer.to_string())
            .await;
    let mut fixture_owner = None;
    let mut host = None;
    let mut probe_finished = false;
    let mut credentials_created = false;
    let mut upload_created = false;
    let observed = std::panic::AssertUnwindSafe(Box::pin(async {
        let rules: Vec<_> = [ServicePurpose::HubRead, ServicePurpose::HubWrite]
            .into_iter()
            .map(|purpose| ServiceRuleInput::Remote {
                origin: ServiceOrigin::Web,
                binding: RemoteServiceBindingInput::Broker {
                    provider: "broker",
                    purpose,
                    endpoint: wire(&broker.ready, "endpoint"),
                    resource: None,
                },
                credential: CredentialSource::IncomingBearer,
                incoming: None,
            })
            .collect();
        let mut fixture = crate::source_server::test_fixture::fixture_with_web_read_providers(
            &[Permission::Read(Module::Hub), Permission::Configure],
            &[Permission::ManageAuthProfiles],
            Brand::default(),
            NativeBinding::KiroV3,
            &["web-installation"],
            &rules,
            (None, Some("broker")),
        );
        drop(rules);
        fixture.projects = Arc::clone(&source_owner.projects);
        fixture_owner = Some(fixture);
        trust_source_projects(
            fixture_owner
                .as_ref()
                .expect("retained source registration owner"),
        );
        let fixture = fixture_owner
            .take()
            .expect("Source owner handed to configuration");
        fixture_owner = Some(configure_hub_fixture(fixture).await);
        let fixture = fixture_owner
            .as_mut()
            .expect("retained Source before Task setup");
        let policy = fixture.server.policy();
        let tasks_policy = checked(
            crate::foreground_tasks::task_policy(
                &policy.native_install.hub,
                policy.scope,
                policy.index,
                policy.binding,
            ),
            "actual Source Task policy",
        );
        checked(
            fixture.server.attach_tasks(tasks_policy),
            "attach Task to the same live Source",
        );
        let tasks = checked(fixture.server.task_server(), "same Source Task owner");
        let fixture = fixture_owner
            .as_ref()
            .expect("retained source and installation owners");
        if !received {
        let source = resource(fixture, "release/ui", ONE);
        assert_eq!(
            checked(
                std::fs::read_to_string(source.join("SKILL.md")),
                "actual source skill"
            ),
            ONE
        );
        assert_eq!(
            checked(
                std::fs::read_to_string(source.join("references/guide.md")),
                "actual source guide"
            ),
            "Resource remains in ordinary ZIP\n"
        );
        }
        let revision = crate::graph_server::tests::browser_provider(
            &fixture.server.graph_server(),
            &fixture.raw,
            wire(&broker.ready, "endpoint"),
        );
        host = Some(
            WebHost::start(fixture.server.clone(), Some(tasks), &broker.ready, revision).await,
        );
        let web = host.as_ref().expect("retained publication Browser owner");
        let cookie = web.login(&broker.ready).await;
        let identity = checked(
            web.client
                .get(format!("{}/auth/session", web.origin))
                .header(header::COOKIE, cookie.as_str())
                .send()
                .await,
            "actual publication session",
        );
        assert_eq!(identity.status(), StatusCode::OK);
        let identity: Value = checked(identity.json().await, "verified publication principal");
        administrators(
            fixture,
            wire(&broker.ready, "issuer"),
            &[(wire(&identity, "subject"), if received { &["project", "global"][..] } else { &["project"][..] })],
        );
        if !received {
        let invalid_contexts = json!({"project_id":producer}).to_string();
        for (caller, body, status) in [
            (None, "{}", StatusCode::UNAUTHORIZED),
            (Some(cookie.as_str()), "[]", StatusCode::BAD_REQUEST),
            (
                Some(cookie.as_str()),
                invalid_contexts.as_str(),
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let response =
                hub_remote_catalog::request(web, caller, "/api/hub/contexts", body).await;
            assert_eq!(
                response.status(),
                status,
                "Hub contexts enforces session and strict empty input"
            );
            checked(
                response.bytes().await,
                "finite Hub context rejection consumed",
            );
        }
        }
        let response =
            hub_remote_catalog::request(web, Some(&cookie), "/api/hub/contexts", "{}").await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "actual authorized Hub contexts"
        );
        assert!(
            response.headers()[header::CACHE_CONTROL]
                .to_str()
                .expect("Hub context cache policy")
                .contains("no-store")
        );
        let contexts: Value = checked(
            response.json().await,
            "actual Hub context DTO without status wrapper",
        );
        let expected_context = contexts["contexts"]
            .as_array()
            .expect("Hub contexts is an actual array")
            .iter()
            .find(|context| {
                context["project_id"] == producer.to_string()
                    && context["project_version"] == fixture.projects[0].2.version()
            })
            .expect("actual source PROJECT and data version available to the shared header");
        assert!(expected_context["project_dir"].is_string(), "Hub context uses a literal Unicode path");
        assert!(
            contexts["current"] == *expected_context,
            "the actual current source selection is authorized, with no fallback"
        );
        if received {
            const UPDATE_ID: &str = "fixture/ui-update-all";
            const ONE: &str = "---\nname: ui-update-all\ndescription: Browser collective update fixture\n---\nBrowser V1 bytes\n";
            const TWO: &str = "---\nname: ui-update-all\ndescription: Browser collective update fixture\n---\nBrowser V2 bytes\n";
            let mut metadata=json!({"project_id":producer,"project_version":fixture.projects[0].2.version(),"publication":{
                "format":"file","provider":"broker","artifact_type":"skill","id":UPDATE_ID,"version":"release/one","name":"UIUpdateAll","set_latest":true}});
            let response=hub_upload::send(web,Some(&cookie),hub_upload::form(&metadata,&hub_upload::archive(ONE))).await;
            assert_eq!(response.status(),StatusCode::OK,"actual V1 seed publication");
            let publication:Value=checked(response.json().await,"actual V1 seed receipt");assert_eq!(publication["publication"]["catalog"],"published");
            let installed=hub_remote_catalog::page(web,&cookie,"/api/hub/files/install",json!({"selection":{
                "transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":UPDATE_ID,"version":"release/one","exact":true},"scope":{"kind":"global_cache"}})).await;
            assert_eq!(installed["version"],"release/one");
            metadata["publication"]["version"]=json!("release/two");
            let response=hub_upload::send(web,Some(&cookie),hub_upload::form(&metadata,&hub_upload::archive(TWO))).await;
            assert_eq!(response.status(),StatusCode::OK,"actual V2 seed publication");
            let publication:Value=checked(response.json().await,"actual V2 seed receipt");assert_eq!(publication["publication"]["catalog"],"published");
            let catalog=hub_remote_catalog::page(web,&cookie,"/api/hub/remote/search",json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":UPDATE_ID,"limit":10})).await;
            assert_eq!(catalog["artifacts"][0]["latest"],"release/two");
            let mut upload=checked(graphit_fs::create_private_file(&probe.with_extension("upload.zip")),"private actual browser upload ZIP");upload_created=true;
            checked(upload.write_all(&hub_upload::archive("---\nname: ui-received-upload\ndescription: Actual browser received package\n---\nBrowser uploaded ZIP bytes\n")),"actual private ZIP bytes");
        }
        let html = checked(
            web.client
                .get(format!("{}{}", web.origin, if received { "/hub/upload" } else { "/hub/local" }))
                .send()
                .await,
            "publication browser assets",
        );
        assert_eq!(html.status(), StatusCode::OK);
        let html=checked(html.bytes().await,"actual browser assets bytes");
        assert!(std::str::from_utf8(&html).expect("actual UTF8 browser assets").contains("<html"));
        let assets_sha256:String=Sha256::digest(&html).iter().map(|byte|format!("{byte:02x}")).collect();
        let private = checked(
            graphit_fs::create_private_file(&probe.with_extension("credentials.json")),
            "private publication browser credentials file",
        );
        credentials_created = true;
        checked(
            serde_json::to_writer(
                private,
                &json!({
                    "username":wire(&broker.ready["web"], "username"),
                    "password":wire(&broker.ready["web"], "password")
                }),
            ),
            "private publication browser credentials written",
        );
        let public = checked(
            graphit_fs::create_private_file(&probe),
            "publication browser metadata with private birth",
        );
        checked(
            serde_json::to_writer(
                public,
                &if received {json!({
                    "scenario":"received_upload_update_all","origin":web.origin,"path":"/hub/upload",
                    "project_id":producer,"project_version":fixture.projects[0].2.version(),"provider":"broker",
                    "artifact_type":"skill","artifact_id":"fixture/ui-upload","version":"release/ui","name":"UIReceivedUpload","set_latest":true,
                    "upload_file":probe.with_extension("upload.zip"),"content_path":"SKILL.md","uploaded_content":"Browser uploaded ZIP bytes\n",
                    "update_all":{"scope":{"kind":"global_cache"},"artifact_id":"fixture/ui-update-all","installed_version":"release/one","latest_version":"release/two","expected_content":"Browser V2 bytes\n"},
                    "assets_sha256":assets_sha256,"assets_bytes":html.len(),"browser_qualified":false
                })} else {json!({
                    "scenario":"project_file_publication", "origin":web.origin, "path":"/hub/local",
                    "project_id":producer, "project_version":fixture.projects[0].2.version(),
                    "provider":"broker", "artifact_type":"skill", "artifact_id":FILE_ID,
                    "version":"release/ui", "name":"Web publication", "set_latest":true,
                    "content_path":"SKILL.md", "browser_qualified":false,
                    "assets_sha256":assets_sha256,"assets_bytes":html.len()
                })},
            ),
            "publication browser metadata written",
        );
        let deadline = Instant::now() + Duration::from_secs(900);
        while Instant::now() < deadline {
            if probe.with_extension("done").exists() {
                probe_finished = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }))
    .catch_unwind()
    .await;
    let cleanup = std::panic::AssertUnwindSafe(finish_remote_test_owners(
        Ok(()),
        host,
        fixture_owner.as_ref().map(|fixture| &fixture.server),
        broker,
    ))
    .catch_unwind()
    .await;
    let private_cleanup = std::panic::AssertUnwindSafe(async {
        let credentials = credentials_created
            .then(|| std::fs::remove_file(probe.with_extension("credentials.json")));
        let upload =
            upload_created.then(|| std::fs::remove_file(probe.with_extension("upload.zip")));
        if let Some(result) = credentials {
            checked(result, "publication probe credentials removed");
        }
        if let Some(result) = upload {
            checked(result, "browser upload ZIP removed");
        }
    })
    .catch_unwind()
    .await;
    let result = checked(
        graphit_fs::create_private_file(&probe.with_extension("result.json")),
        "publication probe lifecycle result file",
    );
    checked(
        serde_json::to_writer(
            result,
            &json!({
                "finished":probe_finished, "observation_passed":observed.is_ok(), "owners_joined":cleanup.is_ok(),
                "credentials_removed":!probe.with_extension("credentials.json").exists(),
                "upload_file_removed":!probe.with_extension("upload.zip").exists(), "browser_qualified":false
            }),
        ),
        "publication probe lifecycle result written",
    );
    for result in [observed, cleanup, private_cleanup] {
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
    assert!(
        probe_finished,
        "browser observer deadline elapsed after cleanup"
    );
}
