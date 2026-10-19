//! Received FILE bytes use the actual Web/Broker/STSv3/storage consumers.
use super::*;
use graphit_auth::{
    CredentialSource, Permission, RemoteServiceBindingInput, ServiceOrigin, ServicePurpose,
    ServiceRuleInput,
};
use graphit_core::{brand::Brand, module_settings::Module};
use graphit_hub::{ArtifactType, NativeBinding};
use hub_file_publication::{administrators, trust_source_projects};
use hub_remote_catalog::page;
use sha2::{Digest, Sha256};
use std::io::Write;
const UPLOAD: &str = "/api/hub/upload";
const ID: &str = "fixture/received-upload";
const TEXT: &str = "---\nname: received-upload\ndescription: Actual received archive\n---\nExact received content\n";
pub(super) fn archive(text: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    checked(
        writer.set_comment("Preserve the exact received ZIP comment"),
        "received ZIP comment",
    );
    checked(
        writer.start_file(
            "SKILL.md",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o600),
        ),
        "real ZIP entry",
    );
    checked(writer.write_all(text.as_bytes()), "real ZIP content");
    checked(writer.finish(), "final ZIP bytes").into_inner()
}
fn part(body: &mut Vec<u8>, name: &str, filename: Option<&str>, bytes: &[u8]) {
    write!(
        body,
        "--received-boundary\r\nContent-Disposition: form-data; name=\"{name}\""
    )
    .unwrap();
    if let Some(filename) = filename {
        write!(body, "; filename=\"{filename}\"").unwrap();
    }
    write!(body, "\r\n\r\n").unwrap();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}
pub(super) fn form(metadata: &Value, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    part(&mut body, "metadata", None, metadata.to_string().as_bytes());
    part(
        &mut body,
        "file",
        Some("../../ignored-file-label.zip"),
        payload,
    );
    body.extend_from_slice(b"--received-boundary--\r\n");
    body
}
pub(super) async fn send(host: &WebHost, cookie: Option<&str>, body: Vec<u8>) -> reqwest::Response {
    let mut request = host
        .client
        .post(format!("{}{UPLOAD}", host.origin))
        .header(header::ORIGIN, &host.origin)
        .header("X-Graphit-Request", "ui")
        .header(
            header::CONTENT_TYPE,
            "multipart/form-data; boundary=received-boundary",
        )
        .body(body);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    checked(request.send().await, "actual received upload HTTP response")
}
async fn expect(host: &WebHost, cookie: Option<&str>, body: Vec<u8>, status: StatusCode) -> Value {
    let response = send(host, cookie, body).await;
    let actual = response.status();
    assert!(
        response.headers()[header::CACHE_CONTROL]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    let value: Value = checked(response.json().await, "finite received upload DTO");
    assert_eq!(
        actual,
        status,
        "received upload code={} cause={} stage={}",
        value["error"]["code"].as_str().unwrap_or("none"),
        value["outcome"]["cause"].as_str().unwrap_or("none"),
        value["outcome"]["stage"].as_str().unwrap_or("none")
    );
    value
}
fn incomplete_request(
    host: &WebHost,
    cookie: &str,
    input: &Value,
) -> tokio::task::JoinHandle<reqwest::Response> {
    use futures_util::StreamExt;
    let mut prefix = Vec::new();
    part(&mut prefix, "metadata", None, input.to_string().as_bytes());
    prefix.extend_from_slice(b"--received-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"file.zip\"\r\n\r\npartial");
    let stream = futures_util::stream::once(async move { Ok::<_, std::io::Error>(prefix) })
        .chain(futures_util::stream::pending());
    let request = host
        .client
        .post(format!("{}{UPLOAD}", host.origin))
        .header(header::ORIGIN, &host.origin)
        .header("X-Graphit-Request", "ui")
        .header(header::COOKIE, cookie)
        .header(
            header::CONTENT_TYPE,
            "multipart/form-data; boundary=received-boundary",
        )
        .body(reqwest::Body::wrap_stream(stream));
    tokio::spawn(
        async move { checked(request.send().await, "owned incomplete-body HTTP response") },
    )
}
async fn joined_request(
    pending: &mut Option<tokio::task::JoinHandle<reqwest::Response>>,
    timeout: Duration,
) -> Result<reqwest::Response, ()> {
    let result = tokio::time::timeout(timeout, pending.as_mut().ok_or(())?).await;
    match result {
        Ok(result) => {
            drop(pending.take());
            result.map_err(|_| ())
        }
        Err(_) => {
            let job = pending.take().ok_or(())?;
            job.abort();
            let _ = job.await;
            Err(())
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the genuine Broker Web/STSv3/MinIO fixture executable"]
async fn real_received_packages_preserve_bytes_authority_and_owned_lifecycle() {
    use futures_util::FutureExt;
    let binary = std::env::var_os("GRAPHIT_BROKER_HUB_STORAGE_FIXTURE_BIN")
        .expect("explicit real Broker fixture executable");
    let source_owner = crate::source_server::test_fixture::fixture_with_web_read_providers(
        &[Permission::Read(Module::Hub)],
        &[Permission::ManageAuthProfiles],
        Brand::default(),
        NativeBinding::KiroV3,
        &["upload-producer", "denied-producer"],
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
    let mut extra_hosts = Vec::new();
    let mut extra_tasks = Vec::new();
    let mut held_sts = false;
    let mut pending_request = None;
    let observed=std::panic::AssertUnwindSafe(Box::pin(async {
        let rules:Vec<_>=[ServicePurpose::HubRead,ServicePurpose::HubWrite].into_iter().map(|purpose|ServiceRuleInput::Remote {
            origin:ServiceOrigin::Web,binding:RemoteServiceBindingInput::Broker{provider:"broker",purpose,endpoint:wire(&broker.ready,"endpoint"),resource:None},
            credential:CredentialSource::IncomingBearer,incoming:None,
        }).collect();
        let mut fixture=crate::source_server::test_fixture::fixture_with_web_read_providers(
            &[Permission::Read(Module::Hub),Permission::Configure],&[Permission::ManageAuthProfiles],Brand::default(),NativeBinding::KiroV3,
            &["upload-installation"],&rules,(None,Some("broker")));
        fixture.projects=Arc::clone(&source_owner.projects);trust_source_projects(&fixture);
        fixture_owner=Some(configure_hub_fixture(fixture).await);
        let fixture=fixture_owner.as_ref().unwrap();
        let revision=crate::graph_server::tests::browser_provider(&fixture.server.graph_server(),&fixture.raw,wire(&broker.ready,"endpoint"));
        let(directory,tasks,_)=crate::task_server::tests::fixture_with_reference_permissions(&[],&[],None,false);
        task_directory=Some(directory);task_owner=Some(tasks.clone());
        host=Some(WebHost::start(fixture.server.clone(),Some(tasks),&broker.ready,revision).await);
        let web=host.as_ref().unwrap();let cookie=web.login(&broker.ready).await;let sibling=web.login(&broker.ready).await;
        let identity:Value=checked(checked(web.client.get(format!("{}/auth/session",web.origin)).header(header::COOKIE,cookie.as_str()).send().await,"actual session identity").json().await,"verified session record");
        let subject=wire(&identity,"subject");let issuer=wire(&broker.ready,"issuer");
        let bytes=archive(TEXT);
        let mut input=json!({"project_id":producer,"project_version":fixture.projects[0].2.version(),"publication":{
            "format":"file","provider":"broker","artifact_type":"skill","id":ID,"version":"release/one","name":"Received skill","set_latest":true}});
        expect(web,None,form(&input,&bytes),StatusCode::UNAUTHORIZED).await;
        expect(web,Some(&cookie),form(&input,&bytes),StatusCode::FORBIDDEN).await;
        administrators(fixture,issuer,&[(subject,&["project","global"])]);
        for(field,value)in[("project_dir",json!("/foreign")),("producer",json!(producer)),("project_version",Value::Null)] {
            let mut invalid=input.clone();invalid[field]=value;expect(web,Some(&cookie),form(&invalid,&bytes),StatusCode::BAD_REQUEST).await;
        }
        for(field,value)in[("artifact_type",json!("framework")),("version",json!("latest")),("expected_sha256",json!("not-a-digest"))] {
            let mut invalid=input.clone();invalid["publication"][field]=value;expect(web,Some(&cookie),form(&invalid,&bytes),StatusCode::BAD_REQUEST).await;
        }
        let mut invalid=input.clone();invalid["publication"]["provider"]=json!("foreign-provider");expect(web,Some(&cookie),form(&invalid,&bytes),StatusCode::FORBIDDEN).await;
        invalid=input.clone();invalid["project_version"]=json!("missing-version");expect(web,Some(&cookie),form(&invalid,&bytes),StatusCode::NOT_FOUND).await;
        invalid=input.clone();invalid["project_id"]=json!(fixture.projects[1].1);expect(web,Some(&cookie),form(&invalid,&bytes),StatusCode::FORBIDDEN).await;
        expect(web,Some(&cookie),form(&input,b"not ZIP"),StatusCode::BAD_REQUEST).await;
        let mut missing=Vec::new();part(&mut missing,"metadata",None,input.to_string().as_bytes());missing.extend_from_slice(b"--received-boundary--\r\n");
        expect(web,Some(&cookie),missing,StatusCode::BAD_REQUEST).await;
        let mut extra=form(&input,&bytes);extra.truncate(extra.len()-b"--received-boundary--\r\n".len());part(&mut extra,"file",Some("extra.zip"),&bytes);extra.extend_from_slice(b"--received-boundary--\r\n");
        expect(web,Some(&cookie),extra,StatusCode::BAD_REQUEST).await;
        let mut wrong_order=Vec::new();part(&mut wrong_order,"file",Some("file.zip"),&bytes);part(&mut wrong_order,"metadata",None,input.to_string().as_bytes());wrong_order.extend_from_slice(b"--received-boundary--\r\n");
        expect(web,Some(&cookie),wrong_order,StatusCode::BAD_REQUEST).await;
        // No Resources artifact exists: these exact received bytes select the publication.
        let published=expect(web,Some(&cookie),form(&input,&bytes),StatusCode::OK).await;
        let expected:String=Sha256::digest(&bytes).iter().map(|byte|format!("{byte:02x}")).collect();
        assert_eq!(published["publication"]["archive_sha256"],expected);assert_eq!(published["publication"]["producer"],producer.to_string());
        assert_eq!(published["publication"]["payload"],"published");assert_eq!(published["publication"]["catalog"],"published");
        let selection=json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":ID,"version":"release/one","exact":true});
        let content=page(web,&sibling,"/api/hub/files/content",json!({"remote":{"selection":selection,"path":"SKILL.md"}})).await;
        assert_eq!(content["content"]["content"],TEXT);
        let catalog=page(web,&sibling,"/api/hub/remote/search",json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":ID,"limit":10})).await;
        assert_eq!(catalog["artifacts"][0]["latest"],"release/one");
        {
            let packages=checked(tempfile::tempdir(),"owned SDK upload fixture parent");
            let cancellation=Cancellation::default();
            let original=checked(graphit_hub::policy::RequestContext::new(fixture.server.policy().content,&cancellation,Instant::now()+Duration::from_secs(120)),"SDK fixture Original");
            for(kind,id)in[(ArtifactType::Ast,"fixture/received-ast"),(ArtifactType::Knowledge,"fixture/received-knowledge")] {
                let (owner,path)=checked(crate::foreground_source::foreground_native_publication_tests::received_archive_fixture(kind,packages.path(),fixture.server.policy(),&original).await,"actual SDK received archive");
                let package=checked(std::fs::read(&path),"final SDK archive bytes");
                let native=json!({"project_id":producer,"project_version":fixture.projects[0].2.version(),"publication":{
                    "format":"native","provider":"broker","artifact_type":kind,"id":id,"version":"release/received","name":"ReceivedNative"}});
                let received=expect(web,Some(&cookie),form(&native,&package),StatusCode::OK).await;
                assert_eq!(received["publication"]["artifact_type"],json!(kind));assert_eq!(received["publication"]["catalog"],"published");
                assert_eq!(received["publication"]["root_sha256"].as_str().unwrap().len(),64);
                let catalog=page(web,&sibling,"/api/hub/remote/search",json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":kind,"id":id,"limit":10})).await;
                assert_eq!(catalog["artifacts"][0]["latest"],"release/received");
                let mut invalid=native.clone();invalid["publication"]["id"]=json!(format!("{id}-invalid"));invalid["publication"]["dependencies"]=json!([{"artifact_type":"rule","id":"../invalid","selector":"release/one"}]);
                expect(web,Some(&cookie),form(&invalid,&package),StatusCode::BAD_REQUEST).await;
                expect(web,Some(&cookie),form(&native,&bytes),StatusCode::BAD_REQUEST).await;
                drop(owner);
            }
        }
        // Observe the real private spool after admission, while the HTTP body cannot finish.
        {
            use futures_util::StreamExt;
            let streaming_cookie=web.login(&broker.ready).await;
            let mut streaming=input.clone();streaming["publication"]["version"]=json!("release/stream-cancelled");
            let mut witness=checked(crate::source_server::web_hub_upload::stream_observer::arm(producer,ArtifactType::Skill,ID,"release/stream-cancelled"),"arm actual upload phase observer");
            let mut prefix=Vec::new();part(&mut prefix,"metadata",None,streaming.to_string().as_bytes());
            prefix.extend_from_slice(b"--received-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"file.zip\"\r\n\r\npartial");
            let stream=futures_util::stream::once(async move {Ok::<_,std::io::Error>(prefix)}).chain(futures_util::stream::pending());
            let request=web.client.post(format!("{}{UPLOAD}",web.origin)).header(header::ORIGIN,&web.origin).header("X-Graphit-Request","ui").header(header::COOKIE,streaming_cookie.as_str())
                .header(header::CONTENT_TYPE,"multipart/form-data; boundary=received-boundary").body(reqwest::Body::wrap_stream(stream));
            pending_request=Some(tokio::spawn(async move {checked(request.send().await,"owned incomplete-body HTTP response")}));
            let spool=checked(checked(tokio::time::timeout(Duration::from_secs(30),witness.entered()).await,"actual spool phase timeout"),"actual spool phase entered");
            assert!(spool.join("payload.zip").is_file());
            let response=checked(web.client.post(format!("{}/auth/logout",web.origin)).header(header::ORIGIN,&web.origin).header(header::COOKIE,streaming_cookie.as_str()).header("X-Graphit-Request","ui").send().await,"logout during actual incoming stream");
            assert_eq!(response.status(),StatusCode::NO_CONTENT);
            let response=checked(joined_request(&mut pending_request,Duration::from_secs(15)).await,"incoming body job joined");
            assert_eq!(response.status(),StatusCode::UNAUTHORIZED);
            let _:Value=checked(response.json().await,"incoming stream finite cancellation envelope");
            assert!(!spool.exists(),"private received spool removed before response");
            let catalog=page(web,&sibling,"/api/hub/remote/search",json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":ID,"limit":10})).await;
            assert!(!catalog["artifacts"][0]["versions"].as_array().unwrap().contains(&json!("release/stream-cancelled")));
        }
        {
            let mut streaming=input.clone();streaming["publication"]["version"]=json!("release/stream-disconnected");
            let mut witness=checked(crate::source_server::web_hub_upload::stream_observer::arm(producer,ArtifactType::Skill,ID,"release/stream-disconnected"),"arm disconnect phase observer");
            pending_request=Some(incomplete_request(web,&cookie,&streaming));
            let spool=checked(checked(tokio::time::timeout(Duration::from_secs(30),witness.entered()).await,"disconnect spool phase timeout"),"disconnect spool phase entered");
            let job=pending_request.take().unwrap();job.abort();let _=job.await;
            checked(tokio::time::timeout(Duration::from_secs(10),async {while spool.exists() {tokio::time::sleep(Duration::from_millis(10)).await;}}).await,"disconnect private spool cleanup");
            let catalog=page(web,&sibling,"/api/hub/remote/search",json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":ID,"limit":10})).await;
            assert!(!catalog["artifacts"][0]["versions"].as_array().unwrap().contains(&json!("release/stream-disconnected")));
        }
        {
            let (limits,_,_)=task_owner.as_ref().unwrap().browser_policy();
            let tasks=task_owner.as_ref().unwrap().references_test_policy(limits,Duration::from_secs(3));extra_tasks.push(tasks.clone());
            extra_hosts.push(WebHost::start(fixture.server.clone(),Some(tasks),&broker.ready,revision).await);
            let deadline_web=extra_hosts.last().unwrap();let deadline_cookie=deadline_web.login(&broker.ready).await;
            let mut streaming=input.clone();streaming["publication"]["version"]=json!("release/stream-deadline");
            let mut witness=checked(crate::source_server::web_hub_upload::stream_observer::arm(producer,ArtifactType::Skill,ID,"release/stream-deadline"),"arm deadline phase observer");
            pending_request=Some(incomplete_request(deadline_web,&deadline_cookie,&streaming));
            let spool=checked(checked(tokio::time::timeout(Duration::from_secs(30),witness.entered()).await,"deadline spool phase timeout"),"deadline spool phase entered");
            let response=checked(joined_request(&mut pending_request,Duration::from_secs(15)).await,"deadline incoming job joined");
            assert_eq!(response.status(),StatusCode::GATEWAY_TIMEOUT);let _:Value=checked(response.json().await,"finite stream deadline envelope");assert!(!spool.exists());
        }
        {
            let(mut limits,timeout,_)=task_owner.as_ref().unwrap().browser_policy();limits.max_output_bytes=1;
            let tasks=task_owner.as_ref().unwrap().references_test_policy(limits,timeout);extra_tasks.push(tasks.clone());
            extra_hosts.push(WebHost::start(fixture.server.clone(),Some(tasks),&broker.ready,revision).await);
            let tiny=extra_hosts.last().unwrap();let tiny_cookie=tiny.login(&broker.ready).await;
            let mut quota=input.clone();quota["publication"]["version"]=json!("release/tiny-output");quota["publication"]["set_latest"]=json!(false);
            let response=send(tiny,Some(&tiny_cookie),form(&quota,&bytes)).await;
            assert_eq!(response.status(),StatusCode::PAYLOAD_TOO_LARGE);assert_eq!(response.headers()["x-graphit-inspection-required"],"request-selection");
            assert!(checked(response.bytes().await,"tiny output bytes").len()<=1);
            let logout=checked(tiny.client.post(format!("{}/auth/logout",tiny.origin)).header(header::ORIGIN,&tiny.origin).header(header::COOKIE,tiny_cookie.as_str()).header("X-Graphit-Request","ui").send().await,"retire actual tiny-quota session");
            assert_eq!(logout.status(),StatusCode::NO_CONTENT);
            let response=send(tiny,Some(&tiny_cookie),form(&quota,&bytes)).await;
            assert_eq!(response.status(),StatusCode::PAYLOAD_TOO_LARGE);assert_eq!(response.headers()["x-graphit-inspection-required"],"request-selection");
            assert!(response.headers().contains_key(header::SET_COOKIE));assert!(checked(response.bytes().await,"tiny early error bytes").len()<=1);
            let catalog=page(web,&sibling,"/api/hub/remote/search",json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":ID,"limit":10})).await;
            assert!(catalog["artifacts"][0]["versions"].as_array().unwrap().contains(&json!("release/tiny-output")));
        }
        // Logout while this same new ingress waits for genuine issued STS.
        input["publication"]["version"]=json!("release/cancelled");held_sts=true;broker.sts_command("hold_sts").await;
        let request=web.client.post(format!("{}{UPLOAD}",web.origin)).header(header::ORIGIN,&web.origin).header("X-Graphit-Request","ui").header(header::COOKIE,cookie.as_str())
            .header(header::CONTENT_TYPE,"multipart/form-data; boundary=received-boundary").body(form(&input,&bytes));
        pending_request=Some(tokio::spawn(async move {checked(request.send().await,"owned held upload response")}));
        broker.sts_command("await_sts").await;
        let logout=checked(web.client.post(format!("{}/auth/logout",web.origin)).header(header::ORIGIN,&web.origin).header(header::COOKIE,cookie.as_str()).header("X-Graphit-Request","ui").send().await,"logout during received upload");
        assert_eq!(logout.status(),StatusCode::NO_CONTENT);
        let response=checked(joined_request(&mut pending_request,Duration::from_secs(15)).await,"received upload response joined");
        assert_eq!(response.status(),StatusCode::UNAUTHORIZED);let outcome:Value=checked(response.json().await,"finite cancelled upload outcome");assert!(outcome.get("error").is_some());
        broker.sts_command("release_sts").await;held_sts=false;
        let catalog=page(web,&sibling,"/api/hub/remote/search",json!({"transport":"broker","provider":"broker","producer":producer,"artifact_type":"skill","id":ID,"limit":10})).await;
        assert!(!catalog["artifacts"][0]["versions"].as_array().unwrap().contains(&json!("release/cancelled")));
    })).catch_unwind().await;
    let request_cleanup = std::panic::AssertUnwindSafe(async {
        if held_sts {
            broker.sts_command("release_sts").await;
        }
        if pending_request.is_some() {
            let _ = joined_request(&mut pending_request, Duration::from_secs(45)).await;
        }
    })
    .catch_unwind()
    .await;
    let extra = std::panic::AssertUnwindSafe(async {
        for host in extra_hosts {
            host.close().await;
        }
        for tasks in extra_tasks {
            checked(tasks.shutdown().await, "extra upload Task owner joined");
        }
    })
    .catch_unwind()
    .await;
    let cleanup = std::panic::AssertUnwindSafe(finish_remote_test_owners(
        observed.and(request_cleanup).and(extra),
        host,
        fixture_owner.as_ref().map(|fixture| &fixture.server),
        broker,
    ))
    .catch_unwind()
    .await;
    let tasks = std::panic::AssertUnwindSafe(async {
        if let Some(tasks) = task_owner {
            checked(tasks.shutdown().await, "upload Task owner joined");
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
