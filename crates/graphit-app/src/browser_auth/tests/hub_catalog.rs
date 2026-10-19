//! Actual Web OAuth + physical installed registry. No remote payload is fetched.
use super::*;
use crate::source_server::{SourceServer, test_fixture as source_fixture};
use graphit_auth::Permission;
use graphit_core::{brand::Brand, file_lock::Cancellation};
use graphit_hub::{
    ArtifactId, ArtifactType, ConcreteVersion, InstallationTarget, NativeBinding, SourceRef,
    registry::{
        ArtifactDisplayMetadata, Installation, InstalledArtifact, InstalledBackend,
        InstalledPayload,
    },
    remote_native::RemoteNativeRecord,
};

fn hub_source(provider: Option<&str>) -> source_fixture::Fixture {
    source_fixture::fixture_with_web_read_providers(
        &[],
        &[Permission::ManageAuthProfiles],
        Brand::default(),
        NativeBinding::KiroV3,
        &["producer"],
        &[],
        (None, provider),
    )
}

async fn setup(
    provider: Option<&str>,
    browser_policy: BrowserPolicy,
) -> (source_fixture::Fixture, tempfile::TempDir, Fixture) {
    let source = hub_source(provider);
    let (task_temp, tasks, raw) =
        crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
    let fixture = Fixture::configured(
        browser_policy,
        Some((tasks, raw)),
        None,
        Some((source.server.clone(), source.raw.clone())),
    )
    .await;
    (source, task_temp, fixture)
}
fn publish(source: &source_fixture::Fixture, kind: ArtifactType, version: &str) {
    let registry = &source.server.policy().registry;
    let limits = registry.limits();
    let id = "vendor/installed";
    let reference = SourceRef::hub_cache(
        &source.paths,
        source.projects[0].1,
        kind,
        ArtifactId::new(id, limits.projection()).unwrap(),
        None,
        ConcreteVersion::new(version, limits.projection()).unwrap(),
        limits.projection(),
    )
    .unwrap();
    let remote = RemoteNativeRecord::new(
        "published-provider",
        "private-bucket",
        "us-east-1",
        Some("https://private-storage.invalid"),
        "private-prefix",
        [7; 32],
        limits.projection(),
    )
    .unwrap();
    let cancellation = Cancellation::default();
    let mut writer = registry
        .writer(Duration::from_secs(5), &cancellation)
        .unwrap();
    writer
        .put_metadata(InstalledArtifact {
            consumer: graphit_core::artifact_store::PublicationSlot::Scope,
            artifact_type: kind,
            id: reference.id().clone(),
            installations: vec![Installation {
                target: InstallationTarget::GlobalNative,
                original_source: reference,
                display: Some(
                    ArtifactDisplayMetadata::new(
                        &format!("Published {version}"),
                        Some("**Actual** author metadata"),
                        &["library".into()],
                        limits,
                    )
                    .unwrap(),
                ),
                alias: None,
                entry_name: None,
                backend: if kind == ArtifactType::Language {
                    InstalledBackend::Native
                } else {
                    InstalledBackend::RemoteNative(remote)
                },
                payload: InstalledPayload::Original {
                    selection_binding: None,
                },
            }],
        })
        .unwrap();
}
async fn read(f: &Fixture, cookie: &str, body: &str) -> reqwest::Response {
    f.client
        .post(format!("{}/api/hub/installed/native", f.origin))
        .header(header::ORIGIN, &f.origin)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Graphit-Request", "ui")
        .body(body.to_owned())
        .send()
        .await
        .unwrap()
}
async fn finish(f: &mut Fixture, source: &SourceServer) {
    f.service.shutdown().await.unwrap();
    source.shutdown().await.unwrap();
    f.stop.cancel();
    for server in f.servers.drain(..) {
        server.abort();
        let result = server.await;
        assert!(result.is_ok() || result.unwrap_err().is_cancelled());
    }
}
#[tokio::test]
async fn cookie_hub_contexts_expose_only_authorized_project_names() {
    let (source, _tasks, mut f) = setup(Some("broker"), policy()).await;
    let (paths, id, _) = &source.projects[0];
    let cancellation = Cancellation::default();
    graphit_core::project_identity::ProjectIdentityStore::at(paths)
        .writer(Duration::from_secs(5), &cancellation)
        .unwrap()
        .update(
            *id,
            &graphit_core::project_identity::ProjectMetadata::new("configured-hub").unwrap(),
        )
        .unwrap();
    let cookie = f.login().await;
    for denied in [false, true] {
        if denied {
            f.issuer.denied_modules.lock().unwrap().push("hub".into());
        }
        let response = f
            .client
            .post(format!("{}/api/hub/contexts", f.origin))
            .header(header::ORIGIN, &f.origin)
            .header(header::COOKIE, &cookie)
            .header("X-Graphit-Request", "ui")
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body: Value = response.json().await.unwrap();
        if denied {
            assert_eq!(body, json!({"contexts":[],"current":null,"projects":[]}));
        } else {
            assert_eq!(body["contexts"].as_array().unwrap().len(), 1);
            assert_eq!(body["current"], body["contexts"][0]);
            assert_eq!(body["current"].as_object().unwrap().len(), 3);
            assert_eq!(body["current"]["project_id"], json!(id));
            assert_eq!(
                body["projects"],
                json!([{"project_id":id,"name":"configured-hub"}])
            );
        }
    }
    finish(&mut f, &source.server).await;
}
#[tokio::test]
async fn installed_native_metadata_uses_web_admission_and_registry_pagination() {
    let (source, _tasks, mut f) = setup(Some("broker"), policy()).await;
    let cookie = f.login().await;
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let keys = f.issuer.keys_calls.load(Ordering::SeqCst);
    let response = read(&f, &cookie, r#"{"scope":"global_installation","limit":1}"#).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let empty: Value = response.json().await.unwrap();
    assert_eq!(
        empty,
        json!({"scope":"global_installation","entries":[],"next_offset":null})
    );
    for (kind, version) in [
        (ArtifactType::Ast, "release-1"),
        (ArtifactType::Ast, "release-2"),
        (ArtifactType::Knowledge, "knowledge-1"),
        (ArtifactType::Language, "language-1"),
    ] {
        publish(&source, kind, version);
    }
    let first: Value = read(
        &f,
        &cookie,
        r#"{"scope":"global_installation","artifact_type":"ast","limit":1}"#,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(first["entries"].as_array().unwrap().len(), 1);
    assert_eq!(first["next_offset"], 1);
    let second: Value = read(
        &f,
        &cookie,
        r#"{"scope":"global_installation","artifact_type":"ast","offset":1,"limit":1}"#,
    )
    .await
    .json()
    .await
    .unwrap();
    assert!(second["next_offset"].is_null());
    assert_ne!(
        first["entries"][0]["version"],
        second["entries"][0]["version"]
    );
    let filtered:Value=read(&f,&cookie,r#"{"scope":"global_installation","artifact_type":"knowledge","id":"vendor/installed","version":"knowledge-1","limit":10}"#).await.json().await.unwrap();
    let entry = &filtered["entries"][0];
    assert_eq!(entry["version"], "knowledge-1");
    assert_eq!(entry["provider"], "published-provider");
    assert_eq!(entry["target"], "global_native");
    assert_eq!(entry["origin"], "HubCache");
    assert_eq!(
        entry["display"]["description"],
        "**Actual** author metadata"
    );
    assert_eq!(entry.as_object().unwrap().len(), 10);
    let all: Value = read(&f, &cookie, r#"{"scope":"global_installation","limit":10}"#)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(all["entries"].as_array().unwrap().len(), 4);
    let text = all.to_string();
    for private in [
        "private-bucket",
        "private-storage.invalid",
        "private-prefix",
        "root_digest",
        "original_source",
        source.temp.path().to_str().unwrap(),
    ] {
        assert!(!text.contains(private));
    }
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(f.issuer.keys_calls.load(Ordering::SeqCst), keys);
    finish(&mut f, &source.server).await;
}
#[tokio::test]
async fn installed_native_metadata_denies_unconfigured_policy_and_changed_installation() {
    for provider in [None, Some("another-provider")] {
        let (source, _tasks, mut f) = setup(provider, policy()).await;
        let cookie = f.login().await;
        assert_eq!(
            read(&f, &cookie, r#"{"scope":"global_installation","limit":10}"#)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
        finish(&mut f, &source.server).await;
    }
    let (source, _tasks, mut f) = setup(Some("broker"), policy()).await;
    let cookie = f.login().await;
    std::fs::rename(
        source.paths.root(),
        source.paths.root().with_extension("replaced"),
    )
    .unwrap();
    std::fs::create_dir(source.paths.root()).unwrap();
    let response = read(&f, &cookie, r#"{"scope":"global_installation","limit":10}"#).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    finish(&mut f, &source.server).await;
}
#[tokio::test]
async fn installed_native_metadata_rejects_invalid_input_and_storage_failure() {
    let (source, _tasks, mut f) = setup(Some("broker"), policy()).await;
    let cookie = f.login().await;
    for body in [
        r#"{"limit":1}"#,
        r#"{"scope":null,"limit":1}"#,
        r#"{"scope":"project","limit":1}"#,
        r#"{"scope":"global_installation","limit":1,"project_id":"other"}"#,
        r#"{"scope":"global_installation","limit":1,"id":null}"#,
        r#"{"scope":"global_installation","limit":1,"version":null}"#,
        r#"{"scope":"global_installation","limit":1,"artifact_type":"agent"}"#,
        r#"{"scope":"global_installation","limit":1,"limit":2}"#,
        r#"{"scope":"global_installation","limit":0}"#,
    ] {
        assert_eq!(
            read(&f, &cookie, body).await.status(),
            StatusCode::BAD_REQUEST,
            "{body}"
        );
    }
    publish(&source, ArtifactType::Knowledge, "release-1");
    std::fs::write(source.server.policy().registry.path(), b"invalid registry").unwrap();
    let response = read(&f, &cookie, r#"{"scope":"global_installation","limit":10}"#).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"error":{"code":"hub_inventory_unavailable"}})
    );
    finish(&mut f, &source.server).await;
}
#[tokio::test]
async fn installed_native_metadata_shutdown_drains_pending_refresh() {
    let mut p = policy();
    p.refresh_margin = Duration::from_secs(600);
    let (source, _tasks, mut f) = setup(Some("broker"), p).await;
    let cookie = f.login().await;
    f.issuer.hold_refresh.store(true, Ordering::Release);
    let client = f.client.clone();
    let origin = f.origin.clone();
    let request = tokio::spawn(async move {
        client
            .post(format!("{origin}/api/hub/installed/native"))
            .header(header::ORIGIN, &origin)
            .header(header::COOKIE, cookie)
            .header("X-Graphit-Request", "ui")
            .json(&json!({"scope":"global_installation","limit":10}))
            .send()
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), f.issuer.refresh_started.notified())
        .await
        .unwrap();
    f.service.shutdown().await.unwrap();
    let response = request.await.unwrap().unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    f.issuer.release_refresh.notify_one();
    finish(&mut f, &source.server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a real browser observer and GRAPHIT_UI_BROWSER_PROBE_FILE"]
async fn installed_native_browser_probe_signed_oauth_assets_and_inventory() {
    let probe = std::path::PathBuf::from(
        std::env::var_os("GRAPHIT_UI_BROWSER_PROBE_FILE").expect("explicit browser probe path"),
    );
    let done = probe.with_extension("done");
    assert!(!probe.exists() && !done.exists(), "use fresh probe paths");
    let source = hub_source(Some("broker"));
    for (kind, version) in [
        (ArtifactType::Ast, "release-1"),
        (ArtifactType::Ast, "release-2"),
        (ArtifactType::Knowledge, "knowledge-1"),
        (ArtifactType::Knowledge, "knowledge-2"),
        (ArtifactType::Language, "language-1"),
    ] {
        publish(&source, kind, version);
    }
    for index in 0..27 {
        publish(
            &source,
            ArtifactType::Knowledge,
            &format!("library-{index:02}"),
        );
    }
    let (_task_temp, tasks, raw) =
        crate::task_server::tests::fixture_with_reference_permissions(&[], &[], None, false);
    let (issuer, first) = Fixture::start_issuer().await;
    let mut p = policy();
    // Optional browser interaction only; production token/session durations are unchanged.
    p.session_lifetime = Duration::from_secs(900);
    p.login_timeout = Duration::from_secs(60);
    let mut fixture = Fixture::configured_host(
        p,
        Some((tasks, raw)),
        None,
        Some((source.server.clone(), source.raw.clone())),
        issuer,
        first,
        true,
    )
    .await;
    *fixture.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let html = fixture
        .client
        .get(format!("{}/hub/installed", fixture.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(html.status(), StatusCode::OK);
    assert!(html.text().await.unwrap().contains("<html"));
    let public = json!({"scenario":"installed_native_libraries","origin":fixture.origin,"path":"/hub/installed","scope":"global_installation","provider":"broker","artifact_id":"vendor/installed","versions":["release-1","release-2","knowledge-1","knowledge-2","language-1"],"installed_entries":32,"browser_qualified":false});
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&probe)
            .unwrap();
        serde_json::to_writer(file, &public).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(900);
    let mut finished = false;
    while Instant::now() < deadline {
        if done.exists() {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    finish(&mut fixture, &source.server).await;
    let userinfo_calls = fixture.issuer.userinfo.load(Ordering::SeqCst);
    std::fs::write(probe.with_extension("result.json"),serde_json::to_vec(&json!({"finished":finished,"owners_joined":true,"userinfo_calls":userinfo_calls,"browser_qualified":false})).unwrap()).unwrap();
    assert!(finished, "browser observer deadline elapsed after cleanup");
    assert_eq!(userinfo_calls, 0);
}

#[tokio::test]
async fn installed_native_metadata_denies_expired_signed_access_before_registry_io() {
    let mut p = policy();
    p.refresh_margin = Duration::from_secs(600);
    let (source, _tasks, mut f) = setup(Some("broker"), p).await;
    publish(&source, ArtifactType::Knowledge, "release-1");
    let cookie = f.login().await;
    let refreshes = f.issuer.refresh.load(Ordering::SeqCst);
    let mut claims = {
        let issued = f.issuer.issued.lock().unwrap();
        let payload = URL_SAFE_NO_PAD
            .decode(issued.split('.').nth(1).unwrap())
            .unwrap();
        serde_json::from_slice::<Value>(&payload).unwrap()
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    claims["iat"] = json!(now.checked_sub(7200).unwrap());
    claims["exp"] = json!(now.checked_sub(3600).unwrap());
    *f.issuer.access_override.lock().unwrap() = Some(f.issuer.jwt(claims));
    // A registry read would return 500: the signed access expiry must win first.
    std::fs::write(source.server.policy().registry.path(), b"invalid registry").unwrap();
    let response = read(&f, &cookie, r#"{"scope":"global_installation","limit":10}"#).await;
    let status = response.status();
    let body = response.json::<Value>().await.unwrap();
    finish(&mut f, &source.server).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // Invalid access cancels the session; the final HTTP recheck owns this error.
    assert_eq!(body, json!({"error":"session unavailable"}));
    assert_eq!(f.issuer.refresh.load(Ordering::SeqCst), refreshes + 1);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn installed_native_metadata_rechecks_installation_changed_during_refresh() {
    let mut p = policy();
    p.refresh_margin = Duration::from_secs(600);
    let (source, _tasks, mut f) = setup(Some("broker"), p).await;
    publish(&source, ArtifactType::Knowledge, "release-1");
    let cookie = f.login().await;
    f.issuer.hold_refresh.store(true, Ordering::Release);
    let timeout = f.tasks.as_ref().unwrap().0.browser_policy().1;
    let response = tokio::time::timeout(timeout + Duration::from_secs(5), async {
        let request = async {
            let response = read(&f, &cookie, r#"{"scope":"global_installation","limit":10}"#).await;
            (response.status(), response.json::<Value>().await.unwrap())
        };
        let invalidate = async {
            tokio::time::timeout(Duration::from_secs(2), f.issuer.refresh_started.notified())
                .await
                .unwrap();
            // execute has retained its Original, but has not reached registry I/O.
            std::fs::rename(
                source.paths.root(),
                source
                    .paths
                    .root()
                    .with_extension("replaced-during-refresh"),
            )
            .unwrap();
            let registry = source.server.policy().registry.path();
            std::fs::create_dir_all(registry.parent().unwrap()).unwrap();
            std::fs::write(registry, b"invalid replacement registry").unwrap();
            f.issuer.release_refresh.notify_one();
        };
        let (response, ()) = tokio::join!(request, invalidate);
        response
    })
    .await;
    f.issuer.release_refresh.notify_one();
    // The standard owner drains the request and the issuer even on a guard failure.
    finish(&mut f, &source.server).await;
    let (status, body) = response.expect("in-flight installation recheck response");
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, json!({"error":{"code":"permission_denied"}}));
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn installed_native_metadata_preserves_request_deadline_during_refresh() {
    let mut p = policy();
    p.refresh_margin = Duration::from_secs(600);
    let (source, _tasks, mut f) = setup(Some("broker"), p).await;
    publish(&source, ArtifactType::Knowledge, "release-1");
    let cookie = f.login().await;
    f.issuer.hold_refresh.store(true, Ordering::Release);
    let timeout = f.tasks.as_ref().unwrap().0.browser_policy().1;
    let started = Instant::now();
    let response = tokio::time::timeout(timeout + Duration::from_secs(5), async {
        let request = async {
            let response = read(&f, &cookie, r#"{"scope":"global_installation","limit":10}"#).await;
            (response.status(), response.json::<Value>().await.unwrap())
        };
        let entered = async {
            tokio::time::timeout(Duration::from_secs(2), f.issuer.refresh_started.notified())
                .await
                .unwrap();
        };
        let (response, ()) = tokio::join!(request, entered);
        response
    })
    .await;
    let elapsed = started.elapsed();
    f.issuer.release_refresh.notify_one();
    finish(&mut f, &source.server).await;
    let (status, body) = response.expect("original inventory operation deadline response");
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(body, json!({"error":"provider deadline exceeded"}));
    assert!(
        elapsed >= timeout,
        "the captured request deadline must expire"
    );
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    assert_eq!(f.issuer.project_calls.load(Ordering::SeqCst), 0);
}
