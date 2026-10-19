use super::*;
use crate::source_server::test_fixture as support;
use graphit_auth::Permission;
use graphit_core::{
    brand::Brand,
    config::{ConfigKey, ConfigSnapshot},
    installation_identity::InstallationIdentityStore,
    module_settings::Module,
};
use graphit_http::{
    client::NetworkRequestContext,
    websocket::{BoundedWebSocket, Message, WebSocketRead},
};
use graphit_hub::NativeBinding;
use rmcp::model::CallToolRequestParams;

async fn local_snapshot(f: &support::Fixture) -> Value {
    let (_, id, version) = &f.projects[0];
    let result = f
        .server
        .now_service()
        .expect("captured Now service")
        .call(
            CallToolRequestParams::new("graphit_now").with_arguments(
                json!({"mode":"local","project_id":id,"version":version.version()})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            crate::task_server::request::Credential {
                role_binding: None,
                bearer: Zeroizing::new(f.raw.clone()),
                origin: crate::task_server::request::Origin::Runtime,
            },
            CancellationToken::new(),
            None,
        )
        .await;
    assert_ne!(result.is_error, Some(true), "productive local Now succeeds");
    let encoded = serde_json::to_value(result).unwrap();
    serde_json::from_str(
        encoded["content"][0]["text"]
            .as_str()
            .expect("native Now text"),
    )
    .expect("native Now JSON")
}
async fn publish(
    socket: &mut BoundedWebSocket<'_>,
    id: &str,
    project: &str,
    version: &str,
    snapshot: &Value,
) {
    checked(socket.send(Message::Text(json!({"op":"publish","instance_id":id,"project_id":project,"version":version,"snapshot":snapshot}).to_string())).await,"real Now publication");
}
async fn frame(response: &mut reqwest::Response, pending: &mut Vec<u8>) -> Value {
    checked(
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                    let line = pending.drain(..=end).collect::<Vec<_>>();
                    return serde_json::from_slice::<Value>(&line).expect("native NDJSON frame");
                }
                let bytes =
                    checked(response.chunk().await, "Web stream chunk").expect("live Web frame");
                pending.extend_from_slice(&bytes);
            }
        })
        .await,
        "Web frame guard",
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built real Broker Web/Presence fixture and coordinated shipped UI assets"]
async fn real_broker_web_now_native_heads_updates_scopes_and_owned_shutdown() {
    use futures_util::FutureExt;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit built genuine Broker fixture");
    let mut f = support::fixture_with_projects_and_authority(
        &[
            Permission::Read(Module::Task),
            Permission::WriteTask,
            Permission::WriteOwnTask,
            Permission::CoordinateTask,
        ],
        &[Permission::ManageAuthProfiles],
        Brand::default(),
        NativeBinding::Codex,
        &["project", "denied-sibling"],
        &[],
    );
    let remote = support::fixture(
        &[Permission::Read(Module::Task)],
        Brand::default(),
        NativeBinding::Codex,
    );
    let project = f.projects[0].1.to_string();
    let broker = BrokerProcess::start_with_now(std::path::Path::new(&binary), &project, true).await;
    let mut host = None;
    let observed=std::panic::AssertUnwindSafe(Box::pin(async {
        let source=f.server.policy().clone();
        let tasks_policy=checked(crate::foreground_tasks::task_policy(&source.native_install.hub,source.scope,source.index,source.binding),"existing native Task policy");
        checked(f.server.attach_tasks(tasks_policy),"productive Task composition");
        let key=ConfigKey::new("broker.allow_loopback_http").unwrap();
        let config=ConfigSnapshot::new(f.paths.brand(),[ConfigSnapshot::capture_environment(f.paths.brand(),std::slice::from_ref(&key),[(key.environment_variable(f.paths.brand()).into(),"true".into())]).unwrap()]).unwrap();
        checked(f.server.attach_now(None,&config),"productive Now composition");
        let tasks=f.server.task_server().unwrap();
        let revision=crate::task_server::tests::browser_provider(&tasks,&f.raw,wire(&broker.ready,"endpoint"),false);
        let session=crate::task_server::tests::seed_browser(&tasks,&f.raw,"create_session",json!({"title":"Native Now session","description":"Web stream observation","strategy":"Existing native heads","idempotency_key":"now-web-session"})).await;
        let session_id=session["id"].clone();
        crate::task_server::tests::seed_browser(&tasks,&f.raw,"claim_session",json!({"id":session_id,"lease_ms":600000})).await;
        let task=crate::task_server::tests::seed_browser(&tasks,&f.raw,"create_task",json!({"title":"Native active work","description":"Actual SDK record","task_type":"TASK","priority":1,"session_id":session_id,"idempotency_key":"now-web-task","checks":[{"kind":"acceptance","text":"Read active head"},{"kind":"test","text":"Read updated stream"}]})).await;
        let task_id=task["specification"]["id"].clone();
        let claim=crate::task_server::tests::seed_browser(&tasks,&f.raw,"claim_task",json!({"id":task_id,"lease_ms":600000})).await;
        let first=local_snapshot(&f).await;
        assert_eq!(first["sections"]["tasks"]["items"][0]["id"],task_id,"actual claimed SDK Task head exists before Broker publication");
        assert_eq!(first["sections"]["sessions"]["items"][0]["total_tasks"],1);
        host=Some(WebHost::start(f.server.clone(),Some(tasks.clone()),&broker.ready,revision).await);
        let web=host.as_ref().unwrap();
        let cookie=web.login(&broker.ready).await;
        let selection=crate::task_server::tests::browser_selection(&tasks);
        let local_id=InstallationIdentityStore::new(&f.paths).unwrap().load().unwrap().unit_id().unwrap().as_str().to_owned();
        let remote_id=InstallationIdentityStore::new(&remote.paths).unwrap().load().unwrap().unit_id().unwrap().as_str().to_owned();
        assert_ne!(local_id,remote_id,"real independent installation IDs");
        for id in [&local_id,&remote_id] {
            let response=checked(web.client.put(format!("{}/v1/instances/{id}/inventory",wire(&broker.ready,"endpoint"))).bearer_auth(wire(&broker.ready,"access_bearer")).json(&json!({"projects":[{"project_id":project,"versions":[f.projects[0].2.version(),"other-version"]}]})).send().await,"real instance inventory");
            assert_eq!(response.status(),StatusCode::CREATED);
        }
        let cancel=Cancellation::default();
        let original=graphit_hub::policy::RequestContext::service(source.content,&cancel).unwrap();
        let verifier=&web.service.0.providers[0].1;
        let client=f.server.now_service().unwrap().browser_client(verifier).unwrap();
        let mut headers=HeaderMap::new();let mut value=header::HeaderValue::from_str(&format!("Bearer {}",wire(&broker.ready,"access_bearer"))).unwrap();value.set_sensitive(true);headers.insert(header::AUTHORIZATION,value);
        let url=format!("{}/v1/realtime",wire(&broker.ready,"endpoint"));
        let mut local=checked(client.connect(&url,headers.clone(),NetworkRequestContext::with_deadline(original.deadline(),original.cancellation())).await,"real local publisher socket");
        let mut other=checked(client.connect(&url,headers,NetworkRequestContext::with_deadline(original.deadline(),original.cancellation())).await,"real remote publisher socket");
        for (socket,id) in [(&mut local,&local_id),(&mut other,&remote_id)] {
            publish(socket,id,&project,f.projects[0].2.version(),&first).await;
            match checked(socket.receive().await,"publisher welcome") {WebSocketRead::Message(Message::Text(text))=>assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["type"],"welcome"),_=>panic!("real welcome required")}
        }
        let contexts=web.post(&cookie,"/api/now/web/contexts",json!({})).await;
        assert_eq!(contexts.status(),StatusCode::OK);
        let contexts:Value=contexts.json().await.unwrap();assert_eq!(contexts["contexts"],json!([selection]));assert_eq!(contexts["current"],selection);
        let mut response=web.post(&cookie,"/api/now/web/subscribe",json!({"selection":selection})).await;
        assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()[header::CACHE_CONTROL],"no-store");assert_eq!(response.headers()[header::CONTENT_TYPE],"application/x-ndjson");
        let mut pending=Vec::new();let initial=frame(&mut response,&mut pending).await;
        assert_eq!(initial["selection"],selection);let instances=initial["snapshot"]["instances"].as_array().unwrap();assert_eq!(instances.len(),2);
        for instance in instances {assert_eq!(instance["version"],f.projects[0].2.version());assert_eq!(instance["now"]["sections"]["tasks"]["items"][0]["id"],task_id);assert_eq!(instance["now"]["sections"]["memories"]["error"],"forbidden");assert!(instance["now"]["sections"]["memories"]["total"].is_null());assert_eq!(instance["now"]["sections"]["knowledge"]["error"],"forbidden");}
        let denied=web.post(&cookie,"/api/now/web/subscribe",json!({"selection":{"project_id":f.projects[1].1,"project_version":f.projects[1].2.version()}})).await;assert_eq!(denied.status(),StatusCode::FORBIDDEN);
        let wrong_version=web.post(&cookie,"/api/now/web/subscribe",json!({"selection":{"project_id":project,"project_version":"not-retained"}})).await;assert_eq!(wrong_version.status(),StatusCode::BAD_REQUEST);
        let foreign=checked(web.client.post(format!("{}/api/now/web/subscribe",web.origin)).header(header::COOKIE,&*cookie).header(header::ORIGIN,"https://foreign.example").json(&json!({"selection":selection})).send().await,"foreign-origin request");assert_eq!(foreign.status(),StatusCode::FORBIDDEN);
        let guest=web.post("","/api/now/web/subscribe",json!({"selection":selection})).await;assert_eq!(guest.status(),StatusCode::UNAUTHORIZED);
        crate::task_server::tests::seed_browser(&tasks,&f.raw,"cancel_task",json!({"id":task_id,"claim":{"token":claim["token"],"expected_revision":claim["record"]["specification"]["revision"]},"reason":"Actual cancelled record stays in the Task inventory"})).await;
        let dossier = web.post(&cookie, "/api/tasks/get", json!({"selection":selection,"input":{"id":task_id}})).await;
        assert_eq!(dossier.status(),StatusCode::OK);
        assert_eq!(dossier.json::<Value>().await.unwrap()["task"]["status"],"cancelled", "cancelled record remains in its authorized dossier");
        let updated=local_snapshot(&f).await;assert_eq!(updated["sections"]["sessions"]["items"][0]["completed_tasks"],0);assert_eq!(updated["sections"]["sessions"]["items"][0]["total_tasks"],0);
        publish(&mut local,&local_id,&project,f.projects[0].2.version(),&updated).await;
        publish(&mut other,&remote_id,&project,f.projects[0].2.version(),&updated).await;
        loop {let update=frame(&mut response,&mut pending).await;let instances=update["snapshot"]["instances"].as_array().unwrap();if instances.len()==2 && instances.iter().all(|i|i["now"]["sections"]["sessions"]["items"][0]["total_tasks"]==0){break;}}
        drop(response);
        if let Some(probe)=std::env::var_os("GRAPHIT_UI_BROWSER_PROBE_FILE") {
            use std::os::unix::fs::OpenOptionsExt;
            let probe=std::path::PathBuf::from(probe);assert!(!probe.exists());let done=probe.with_extension("done");assert!(!done.exists());
            let mut file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&probe).unwrap();
            serde_json::to_writer(&mut file,&json!({"scenario":"now_web","origin":web.origin,"path":"/workspace","username":broker.ready["web"]["username"],"password":broker.ready["web"]["password"],"project_id":project,"version":f.projects[0].2.version(),"instances":[local_id,remote_id],"session_id":session_id})).unwrap();drop(file);
            let deadline=Instant::now()+Duration::from_secs(300);let mut heartbeat=Instant::now();
            while !done.exists() && Instant::now()<deadline {
                if heartbeat.elapsed()>=Duration::from_secs(5) {let now=local_snapshot(&f).await;publish(&mut local,&local_id,&project,f.projects[0].2.version(),&now).await;publish(&mut other,&remote_id,&project,f.projects[0].2.version(),&now).await;heartbeat=Instant::now();}
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let completed=done.exists();checked(std::fs::remove_file(&probe),"remove private observer login data");assert!(completed,"real browser observation completes before cleanup");
        }
        checked(local.close(graphit_http::websocket::CloseCode::Normal).await,"local publisher close");checked(other.close(graphit_http::websocket::CloseCode::Normal).await,"remote publisher close");
        let mut live_logout=web.post(&cookie,"/api/now/web/subscribe",json!({"selection":selection})).await;
        assert_eq!(live_logout.status(),StatusCode::OK);
        let mut logout_pending=Vec::new();let _=frame(&mut live_logout,&mut logout_pending).await;
        let logout=web.post(&cookie,"/auth/logout",json!({})).await;assert!(logout.status().is_success());
        let ended=checked(tokio::time::timeout(Duration::from_secs(5),live_logout.chunk()).await,"logout terminates owned Now body");
        assert!(ended.is_err()||ended.unwrap().is_none(),"no stale Now body after logout");
        assert_eq!(web.post(&cookie,"/api/now/web/contexts",json!({})).await.status(),StatusCode::UNAUTHORIZED);
    })).catch_unwind().await;
    let remote_cleanup = std::panic::AssertUnwindSafe(remote.server.shutdown())
        .catch_unwind()
        .await;
    finish_remote_test_owners(observed, host, Some(&f.server), broker).await;
    if let Err(error) = remote_cleanup {
        std::panic::resume_unwind(error)
    }
}
