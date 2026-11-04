//! Actual loopback listener, Runtime issuer, signed Native JWT and owned corpus SDK.
#[path = "support/native_install_auth.rs"]
mod auth;
use graphit_app::{
    native_install::{
        FailureCategory, InstallOutcome, InstallRequest, PayloadEffect, RegistryEffect,
    },
    native_install_http::{NativeInstallHttp, NativeInstallPolicy},
    source_server::{SourcePolicy, SourceServer},
};
use graphit_core::{
    artifact_store::{CapturedSourceReader, CopyPolicy, SourceReadLimits},
    config::{ConfigDocument, ConfigKey, ConfigSnapshot, ConfigSource},
    file_lock::Cancellation,
    git_checkout::GitCheckout,
    native_document_file::NativeDocumentFileLimits,
    project_identity::{ProjectIdentityStore, ProjectMetadata},
    project_paths::ProjectPaths,
    tcp_listener::{self, Options, PortSelection},
    version_context::VersionContext,
};
use graphit_hub::{
    NativeBinding,
    native_package::{self, PackageKind},
    policy::{RequestContext, ResourceSetting},
};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
fn context(cancel: &Cancellation) -> RequestContext<'_> {
    let config = ConfigSnapshot::new(&graphit_core::brand::Brand::default(), []).unwrap();
    RequestContext::new(
        graphit_hub::policy::HubResourcePolicy::resolve(&config)
            .unwrap()
            .content(),
        cancel,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap()
}
fn native_test(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(work)
        })
        .unwrap()
        .join()
        .unwrap();
}
fn package(f: &auth::Fixture, ctx: &RequestContext<'_>) -> PathBuf {
    let path = f.temp.path().join("corpus");
    fs::create_dir_all(path.join("graph.icebug")).unwrap();
    fs::write(path.join("graph.icebug/icebug.json"), serde_json::to_vec(&json!({
        "manifest_version":3,"format":"icebug-canonical","storage":"direct-parquet","finished":true,
        "add_reverse_edges":false,"schema_file":"schema.cypher","node_tables":[],"relationships":[],
        "relation_uids":true,"generation":"native-http-fixture","n_edges":0,"repaired_strings":0,
        "invariants":{"indptr_row_groups":1,"self_loops":"forward-once"}
    })).unwrap()).unwrap();
    fs::write(
        path.join("graph.icebug/schema.cypher"),
        "opaque native metadata",
    )
    .unwrap();
    let captured = CapturedSourceReader::capture(
        &path,
        graphit_fs::NodeKind::Directory,
        &f.policy
            .snapshot_policy(CopyPolicy::RejectFileLinks, [])
            .unwrap(),
        SourceReadLimits::new(65536, 4096, 536870912).unwrap(),
        Arc::clone(ctx.snapshot()),
    )
    .unwrap();
    let zip = f.temp.path().join("corpus.zip");
    native_package::export(&captured, PackageKind::Ast, &zip, ctx.snapshot()).unwrap();
    zip
}
fn request(input: PathBuf, id: &str) -> InstallRequest {
    InstallRequest {
        input,
        producer: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
        id: id.into(),
        version: "v1".into(),
        kind: None,
    }
}
async fn serve(
    f: &auth::Fixture,
    policy: NativeInstallPolicy,
    ctx: &RequestContext<'_>,
) -> (
    SourceServer,
    String,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let root = f.temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let project = ProjectPaths::at(&root, &f.paths).unwrap();
    let id = ProjectIdentityStore::at(&project)
        .writer(
            ctx.remaining().unwrap().expect("finite fixture deadline"),
            ctx.cancellation(),
        )
        .unwrap()
        .ensure(&ProjectMetadata::new("native-http-project").unwrap())
        .unwrap()
        .id();
    let checkout = GitCheckout::inspect(
        &root,
        ctx.remaining().unwrap().expect("finite fixture deadline"),
        ctx.cancellation(),
    )
    .unwrap();
    let version = VersionContext::select(&project, checkout.as_checkout(), "", None, "").unwrap();
    let hub = policy.hub.clone();
    let server = SourceServer::new(
        Arc::clone(&f.auth),
        Arc::new(vec![(project, id, version)]),
        0,
        SourcePolicy {
            graph_query: graphit_ast::icebug_query::QueryOptions {
                buffer_pool_bytes: 128 * 1024 * 1024,
                query_threads: 2,
                max_rows: (hub.content()).max_collection_entries,
                max_bytes: (hub.content()).max_output_bytes,
                max_depth: (hub.content()).max_depth,
            },
            installation: f.paths.clone(),
            native_install: policy,
            content: hub.content(),
            timeout: Duration::from_secs(30),
            scope: auth::scopes(),
            file: hub
                .snapshot_policy(CopyPolicy::RejectFileLinks, [])
                .unwrap(),
            read: SourceReadLimits::new(65536, 4096, 536870912).unwrap(),
            path: hub.retained(),
            index: NativeDocumentFileLimits::new(65536, 65536, 256, 65536).unwrap(),
            binding: NativeBinding::KiroV3,
            registry: f.registry.clone(),
        },
        CancellationToken::new(),
    )
    .unwrap();
    let listener = tcp_listener::reserve(
        Options {
            address: "127.0.0.1".parse().unwrap(),
            ports: PortSelection::Ephemeral,
            timeout: ctx.remaining().unwrap().expect("finite fixture deadline"),
        },
        ctx.cancellation(),
    )
    .unwrap();
    let url = format!("http://{}", listener.address());
    let owner = server.clone();
    let task = tokio::spawn(async move { owner.serve(listener).await });
    (server, url, task)
}
fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}
async fn post(
    client: &reqwest::Client,
    url: &str,
    key: &str,
    request: &InstallRequest,
) -> reqwest::Response {
    client
        .post(format!("{url}/admin/packages/install"))
        .bearer_auth(key)
        .header("x-graphit-auth-origin", "runtime")
        .json(request)
        .send()
        .await
        .unwrap()
}
#[test]
fn actual_source_listener_native_install_and_protected_negatives() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let broker = auth::Broker::start().await;
        let f = auth::Fixture::new(&ctx, true);
        f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        let key = f.key(&ctx);
        let input = package(&f, &ctx);
        let policy = NativeInstallPolicy::capture(&f.config, ctx.snapshot()).unwrap();
        let (server, url, task) = serve(&f, policy, &ctx).await;
        let client = client();
        let req = request(input, "http/positive");
        let response = post(&client, &url, &key, &req).await;
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("application/json")
        );
        let outcome: InstallOutcome = response.json().await.unwrap();
        assert!(outcome.succeeded(), "{outcome:?}");
        assert!(outcome.destination.unwrap().join("graph.icebug").is_dir());
        assert_eq!(broker.requests(), [1, 1, 1]);
        let before = f.registry.load().unwrap();
        let denied = request(req.input.clone(), "http/denied");
        let response = client
            .post(format!("{url}/admin/packages/install"))
            .bearer_auth(&*key)
            .header("x-graphit-auth-origin", "local")
            .json(&denied)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        let response = client
            .post(format!("{url}/admin/packages/install"))
            .json(&denied)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        let response = client
            .post(format!("{url}/admin/packages/install"))
            .bearer_auth(&*key)
            .header("x-graphit-auth-origin", "runtime")
            .json(&json!({"input":req.input,"producer":req.producer,
                "id":"http/unknown","version":"v1","kind":null}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        let outcome: InstallOutcome = post(&client, &url, "wrong-runtime-key", &denied)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(outcome.payload, PayloadEffect::NotPublished);
        assert_eq!(outcome.registry, RegistryEffect::NotAttempted);
        assert_eq!(
            outcome.failure.unwrap().category,
            FailureCategory::Authentication
        );
        let relative = request("relative.zip".into(), "http/relative");
        let outcome: InstallOutcome = post(&client, &url, &key, &relative)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(
            outcome.failure.unwrap().category,
            FailureCategory::InvalidInput
        );
        assert_eq!(f.registry.load().unwrap(), before);
        assert_eq!(broker.requests(), [1, 1, 1]);
        assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
        server.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
        f.remove(&ctx);
        broker.shutdown().await;
    });
}
#[test]
fn actual_body_output_bounds_and_global_config_keep_published_effects() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let broker = auth::Broker::start().await;
        let f = auth::Fixture::new(&ctx, true);
        f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        let input = package(&f, &ctx);
        let global = ConfigDocument::new(
            f.paths.brand(),
            ConfigSource::Global,
            [
                (
                    ConfigKey::new(ResourceSetting::ContentInputBytes.spec().key).unwrap(),
                    json!(1024),
                ),
                (
                    ConfigKey::new(ResourceSetting::ContentOutputBytes.spec().key).unwrap(),
                    json!(512),
                ),
                (
                    ConfigKey::new(ResourceSetting::ContentScalarBytes.spec().key).unwrap(),
                    json!(512),
                ),
            ],
        )
        .unwrap();
        let project = ConfigDocument::new(
            f.paths.brand(),
            ConfigSource::Project,
            [(
                ConfigKey::new("data.local_graph").unwrap(),
                json!({"max_tables":0}),
            )],
        )
        .unwrap();
        let key = ConfigKey::new(graphit_data::LocalGraphPolicy::CONFIG_KEY).unwrap();
        let environment = ConfigSnapshot::capture_environment(
            f.paths.brand(),
            std::slice::from_ref(&key),
            [(
                key.environment_variable(f.paths.brand()).into(),
                "{\"max_tables\":1}".into(),
            )],
        )
        .unwrap();
        let config = ConfigSnapshot::new(f.paths.brand(), [global, project, environment]).unwrap();
        let policy = NativeInstallPolicy::capture(&config, ctx.snapshot()).unwrap();
        assert_eq!(policy.graph.limits().max_tables, 1);
        let (server, url, task) = serve(&f, policy, &ctx).await;
        let client = client();
        let runtime = f.key(&ctx);
        let response = client
            .post(format!("{url}/admin/packages/install"))
            .bearer_auth(&*runtime)
            .header("x-graphit-auth-origin", "runtime")
            .body(vec![b'x'; 1025])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 413);
        assert_eq!(broker.requests(), [0, 0, 0]);
        let id = format!("large/{}/{}", "a".repeat(120), "b".repeat(120));
        let req = request(input, &id);
        let response = post(&client, &url, &runtime, &req).await;
        assert_eq!(response.status(), 200);
        assert!(
            response.bytes().await.is_err(),
            "output failure must not fabricate a terminal"
        );
        let document = f.registry.load().unwrap();
        let version = graphit_hub::ConcreteVersion::new("v1", f.policy.projection()).unwrap();
        let actual = document
            .get_version(graphit_hub::ArtifactType::Ast, &id, &version)
            .unwrap();
        assert!(
            actual.installations[0]
                .original_source
                .directory()
                .as_path()
                .join("graph.icebug")
                .is_dir()
        );
        let _delivery = server.shutdown().await;
        task.await.unwrap().unwrap();
        f.remove(&ctx);
        broker.shutdown().await;
    });
}
#[test]
fn actual_readonly_native_policy_denies_before_publication() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let broker = auth::Broker::start().await;
        let f = auth::Fixture::new(&ctx, false);
        f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        let req = request(package(&f, &ctx), "http/readonly");
        let (server, url, task) = serve(
            &f,
            NativeInstallPolicy::capture(&f.config, ctx.snapshot()).unwrap(),
            &ctx,
        )
        .await;
        let outcome: InstallOutcome = post(&client(), &url, &f.key(&ctx), &req)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(outcome.payload, PayloadEffect::NotPublished);
        assert_eq!(outcome.registry, RegistryEffect::NotAttempted);
        assert_eq!(
            outcome.failure.unwrap().category,
            FailureCategory::Authentication
        );
        assert_eq!(f.registry.load().unwrap().entries().count(), 0);
        server.shutdown().await.unwrap();
        task.await.unwrap().unwrap();
        f.remove(&ctx);
        broker.shutdown().await;
    });
}

#[test]
fn actual_active_installer_registry_wait_observes_owner_stop_after_publication() {
    native_test(async {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let broker = auth::Broker::start().await;
        let f = auth::Fixture::new(&ctx, true);
        f.profile(&broker, auth::now() + 600, "broker-api", false, &ctx);
        let req = request(package(&f, &ctx), "http/active-stop");
        let source = graphit_hub::SourceRef::hub_cache(
            &f.paths,
            req.producer,
            graphit_hub::ArtifactType::Ast,
            graphit_hub::ArtifactId::new(&req.id, f.policy.projection()).unwrap(),
            None,
            graphit_hub::ConcreteVersion::new(&req.version, f.policy.projection()).unwrap(),
            f.policy.projection(),
        )
        .unwrap();
        let installer = NativeInstallHttp::new(
            Arc::clone(&f.auth),
            f.paths.clone(),
            f.registry.clone(),
            NativeInstallPolicy::capture(&f.config, ctx.snapshot()).unwrap(),
            CancellationToken::new(),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (stop_listener, listener_stopping) = tokio::sync::oneshot::channel();
        let router = installer.router();
        let serving = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = listener_stopping.await;
                })
                .await
        });
        let held = f
            .registry
            .writer(
                ctx.remaining().unwrap().expect("finite fixture deadline"),
                ctx.cancellation(),
            )
            .unwrap();
        let response = post(&client(), &url, &f.key(&ctx), &req).await;
        assert_eq!(response.status(), 200);
        tokio::time::timeout(Duration::from_secs(10), async {
            while !source.directory().as_path().join("graph.icebug").is_dir() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
        // The installer really ran Auth, staging and native admission and published;
        // only the actual global registry writer remains unavailable.
        let (shutdown, outcome) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(installer.shutdown(), response.json::<InstallOutcome>())
        })
        .await
        .unwrap();
        shutdown.unwrap();
        let outcome = outcome.unwrap();
        assert_eq!(outcome.payload, PayloadEffect::Published);
        assert_eq!(outcome.registry, RegistryEffect::NotAttempted);
        assert_eq!(
            outcome.failure.unwrap().category,
            FailureCategory::Cancelled
        );
        stop_listener.send(()).unwrap();
        serving.await.unwrap().unwrap();
        assert!(source.directory().as_path().join("graph.icebug").is_dir());
        assert!(f.auth.runtime().check(ctx.snapshot()).is_ok());
        drop(held);
        assert_eq!(f.registry.load().unwrap().entries().count(), 0);
        assert_eq!(broker.requests(), [1, 1, 1]);
        f.remove(&ctx);
        broker.shutdown().await;
    });
}
