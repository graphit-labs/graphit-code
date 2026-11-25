//! Default regression coverage uses the existing local OAuth fixture, no vendor or Broker process.
use super::*;
use crate::source_server::test_fixture as source;
use graphit_auth::Permission;
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot},
    module_settings::Module,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cookie_now_ingress_keeps_origin_session_selection_and_local_jwt_boundaries() {
    let mut sources = source::fixture_with_projects_and_authority(
        &[
            Permission::Read(Module::Ast),
            Permission::Read(Module::Task),
        ],
        &[Permission::ManageAuthProfiles],
        Brand::default(),
        graphit_hub::NativeBinding::Codex,
        &["project"],
        &[],
    );
    let key = ConfigKey::new("broker.allow_loopback_http").unwrap();
    let config = ConfigSnapshot::new(
        sources.paths.brand(),
        [ConfigSnapshot::capture_environment(
            sources.paths.brand(),
            std::slice::from_ref(&key),
            [(
                key.environment_variable(sources.paths.brand()).into(),
                "true".into(),
            )],
        )
        .unwrap()],
    )
    .unwrap();
    sources.server.attach_now(None, &config).unwrap();
    let f = Fixture::with_graphs(policy(), (sources.server.clone(), sources.raw.clone())).await;
    f.issuer.profile.store(false, Ordering::Release);
    *f.issuer.user_status.lock().unwrap() = StatusCode::SERVICE_UNAVAILABLE;
    let cookie = f.login().await;
    let selection = json!({"project_id":sources.projects[0].1,"project_version":sources.projects[0].2.version()});
    for path in ["/api/now/web/contexts", "/api/now/web/subscribe"] {
        let body = if path.ends_with("contexts") {
            json!({})
        } else {
            json!({"selection":selection})
        };
        let guest = f
            .client
            .post(format!("{}{path}", f.origin))
            .header(header::ORIGIN, &f.origin)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(guest.status(), StatusCode::UNAUTHORIZED);
        let foreign = f
            .client
            .post(format!("{}{path}", f.origin))
            .header(header::ORIGIN, "https://foreign.example")
            .header(header::COOKIE, &cookie)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
        let valid = f
            .client
            .post(format!("{}{path}", f.origin))
            .header(header::ORIGIN, &f.origin)
            .header(header::COOKIE, &cookie)
            .json(&body)
            .send()
            .await
            .unwrap();
        // The controlled issuer has no realtime endpoint. Authentication reaches
        // the transport; unavailable UserInfo cannot convert it into HTTP 401.
        assert_eq!(valid.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(valid.headers()[header::CACHE_CONTROL], "no-store");
    }
    let invalid=f.client.post(format!("{}/api/now/web/subscribe",f.origin)).header(header::ORIGIN,&f.origin).header(header::COOKIE,&cookie).json(&json!({"selection":{"project_id":sources.projects[0].1,"project_version":"not-retained"}})).send().await.unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert_eq!(f.issuer.userinfo.load(Ordering::SeqCst), 0);
    f.finish_graphs().await;
    sources.server.shutdown().await.unwrap();
}
