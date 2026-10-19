mod hub;
mod web_embedding;
#[path = "web_hub_mutation.rs"]
mod web_hub_mutation;

use super::*;
use crate::broker::{
    BrokerPolicy, BrokerRequestContext,
    oauth::{OAuthLimits, tests::Fixture as OAuthFixture, web::WebOAuth},
    project_access::{
        BrokerProjectAccess, BrokerProjectCaller, BrokerProjectError, BrokerProjectScope,
        BrokerRecordRead, BrokerRecordWrite, ProjectAccessPolicy,
    },
    tests::{limits as verifier_limits, network_limits, pair, token},
};
use crate::profile::{
    AiService, BrokerService, ProviderConfig, ProviderKind, S3Location, S3Source,
};
use graphit_http::client::ClientPolicy;
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn oauth_limits() -> OAuthLimits {
    OAuthLimits {
        authorization_url_bytes: 8192,
        callback_bytes: 8192,
        callback_query_entries: 8,
        form_body_bytes: 8192,
        token_bytes: 4096,
        response_nodes: 32,
        response_scalar_bytes: 8192,
    }
}
fn verifier(origin: &str, revision: u64) -> crate::broker::BrokerVerifier {
    crate::broker::BrokerVerifier::new(
        BrokerPolicy {
            endpoint: origin,
            mcp_resource: None,
            provider_revision: revision,
            network: network(origin),
        },
        network_limits(),
        verifier_limits(),
    )
    .unwrap()
}
fn network(origin: &str) -> ClientPolicy<'_> {
    ClientPolicy {
        origin,
        allow_loopback_http: true,
        proxy: None,
        additional_roots: &[],
    }
}
fn backend(endpoint: &str) -> BrokerProjectAccess {
    BrokerProjectAccess::new(
        ProjectAccessPolicy {
            endpoint,
            network: network(endpoint),
        },
        network_limits(),
        verifier_limits().json,
    )
    .unwrap()
}
fn add_provider(issuer: &RoleIssuer, original: &RequestContext<'_>, origin: &str) -> u64 {
    let snapshot = issuer.local.load(original).unwrap();
    let revision = snapshot.document().revision();
    let _ = issuer
        .local
        .mutate(snapshot, original, |document, request| {
            document.add_provider(
                "broker".into(),
                ProviderConfig {
                    kind: ProviderKind::Broker(BrokerService {
                        endpoint: origin.into(),
                        mcp_resource: None,
                        allow_anonymous: false,
                    }),
                    embedding: AiService::Disabled,
                    rerank: AiService::Disabled,
                    s3: S3Location {
                        bucket: None,
                        region: None,
                        endpoint: None,
                        prefix: None,
                        source: S3Source::BrokerScope,
                    },
                },
                revision,
                1,
                request,
            )
        })
        .unwrap();
    issuer
        .local
        .load(original)
        .unwrap()
        .document()
        .provider("broker")
        .unwrap()
        .revision()
}
fn project_scope(fixture: &Fixture) -> BrokerProjectScope<'_> {
    let scope = fixture.scope();
    BrokerProjectScope {
        installation: scope.installation,
        project: scope.project.unwrap(),
        providers: &[],
    }
}
fn reply(issuer: &str) -> Value {
    json!({"subject":{"issuer":issuer,"subject":"native-subject"},
        "authorization_revision":4,"project_id":"01KSH1CRFFG8Z74B5ZS78WW808","module":"task","action":"read"})
}

#[tokio::test]
async fn broker_memory_write_requires_exact_write_reply_and_verified_actor() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation);
    let fixture = Fixture::new(&original);
    let issuer = fixture.issuer(&original, limits());
    let revision = add_provider(&issuer, &original, &oauth.origin);
    let verifier = verifier(&oauth.origin, revision);
    let flow = WebOAuth::new(
        &verifier,
        oauth_limits(),
        "https://workspace.example/auth/callback",
        false,
    )
    .unwrap();
    let context = BrokerRequestContext::new(original.snapshot());
    let registered = flow.register(context).await.unwrap();
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let claims = json!({"iss":oauth.origin,"sub":"native-subject","aud":["native-api",registered.resource()],
        "client_id":registered.client_id(),"token_use":"access","jti":"write-test","scope":"","iat":now,"exp":now+600,
        "preferred_username":"display-only"});
    let raw = token(&key, "one", &claims);
    *oauth.state.userinfo_status.lock().unwrap() = 503;
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    for (field, value) in [
        ("exp", json!(now - 1)),
        ("aud", json!(["other", registered.resource()])),
        ("token_use", json!("id")),
        ("client_id", json!("other-client")),
    ] {
        let mut invalid = claims.clone();
        invalid[field] = value;
        assert!(
            flow.verify_access(&registered, &token(&key, "one", &invalid), context)
                .await
                .is_err()
        );
    }
    assert!(
        flow.verify_access(&registered, "invalid", context)
            .await
            .is_err()
    );
    let access = backend(&format!("{}/v1/project/access/authorize", oauth.origin));
    let authority = ProjectRecordAuthority::new(&issuer);
    let input = || BrokerRecordWrite {
        caller: &caller,
        verifier: &verifier,
        backend: &access,
        original: &original,
        scope: project_scope(&fixture),
        provider: "broker",
        operation: ProjectWriteOperation::Memory,
    };
    assert!(matches!(
        authority.admit_broker_write(input()).await,
        Err(BrokerProjectError::Denied)
    ));
    let mut good = reply(&oauth.origin);
    good["module"] = json!("memory");
    good["action"] = json!("write");
    *oauth.state.project_response.lock().unwrap() = (200, good.to_string());
    let write = authority.admit_broker_write(input()).await.unwrap();
    assert_eq!(write.recheck(), Ok(Obligation::Unconditional));
    assert!(std::ptr::eq(write.request().unwrap(), &original));
    assert_eq!(
        write.project().unwrap().version.version(),
        fixture.version.version()
    );
    let actor = write.actor().unwrap();
    let mut fenced = input();
    fenced.operation = ProjectWriteOperation::FencedMemory;
    let fenced = authority.admit_broker_write(fenced).await.unwrap();
    assert_eq!(fenced.recheck(), Ok(Obligation::MemoryFenceRequired));
    assert!(actor == fenced.actor().unwrap());
    let mut changed = claims.clone();
    changed["preferred_username"] = json!("different-display");
    let changed_raw = token(&key, "one", &changed);
    let changed = flow
        .verify_access(&registered, &changed_raw, context)
        .await
        .unwrap();
    let mut renamed = input();
    renamed.caller = &changed;
    assert!(
        actor
            == authority
                .admit_broker_write(renamed)
                .await
                .unwrap()
                .actor()
                .unwrap()
    );
    let before = oauth.count("/v1/project/access/authorize");
    let foreign = request(&cancellation);
    let mut wrong = input();
    wrong.original = &foreign;
    assert!(authority.admit_broker_write(wrong).await.is_err());
    let mut wrong = input();
    wrong.operation = ProjectWriteOperation::Task;
    assert!(authority.admit_broker_write(wrong).await.is_err());
    let mut wrong = input();
    wrong.provider = "other";
    assert!(authority.admit_broker_write(wrong).await.is_err());
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);
    for (field, value) in [
        ("action", json!("read")),
        ("module", json!("task")),
        ("project_id", json!("01KSH1CRFFG8Z74B5ZS78WW809")),
        ("subject", json!({"issuer":oauth.origin,"subject":"other"})),
        (
            "subject",
            json!({"issuer":"https://other.example","subject":"native-subject"}),
        ),
    ] {
        let mut bad = good.clone();
        bad[field] = value;
        *oauth.state.project_response.lock().unwrap() = (200, bad.to_string());
        assert!(matches!(
            authority.admit_broker_write(input()).await,
            Err(BrokerProjectError::InvalidResponse)
        ));
    }
    *oauth.state.project_response.lock().unwrap() = (403, "{}".into());
    assert!(matches!(
        authority.admit_broker_write(input()).await,
        Err(BrokerProjectError::Denied)
    ));
    write.recheck().unwrap(); // A new operation observes denial; no network on local barriers.
    let before = oauth.count("/v1/project/access/authorize");
    write.actor().unwrap();
    write.project().unwrap();
    let (project, observed_request) = write.project_and_request().unwrap();
    assert!(std::ptr::eq(observed_request, &original));
    assert!(std::ptr::eq(project.scope.live, &fixture.project));
    assert!(std::ptr::eq(project.version, &fixture.version));
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/keys"), 1);
    cancellation.cancel();
    assert_eq!(write.recheck(), Err(AuthError::Request));
    assert!(write.actor().is_err());
    assert!(write.project_and_request().is_err());
    assert!(!fixture.source_root.join("records").exists());
}

#[tokio::test]
async fn broker_records_local_jwt_fresh_project_and_physical_recheck() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation);
    let fixture = Fixture::new(&original);
    let issuer = fixture.issuer(&original, limits());
    let revision = add_provider(&issuer, &original, &oauth.origin);
    let verifier = verifier(&oauth.origin, revision);
    let flow = WebOAuth::new(
        &verifier,
        oauth_limits(),
        "https://workspace.example/auth/callback",
        false,
    )
    .unwrap();
    let context = BrokerRequestContext::new(original.snapshot());
    let registered = flow.register(context).await.unwrap();
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let claims = json!({"iss":oauth.origin,"sub":"native-subject",
        "aud":["native-api",registered.resource()],"client_id":registered.client_id(),
        "token_use":"access","jti":"records-test","scope":"","iat":now,"exp":now+600});
    let raw = token(&key, "one", &claims);
    *oauth.state.userinfo_status.lock().unwrap() = 503;
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    for (field, value) in [
        ("exp", json!(now - 1)),
        ("aud", json!(["another-service", registered.resource()])),
        ("token_use", json!("id")),
    ] {
        let mut invalid = claims.clone();
        invalid[field] = value;
        assert!(
            flow.verify_access(&registered, &token(&key, "one", &invalid), context)
                .await
                .is_err()
        );
    }
    assert!(
        flow.verify_access(&registered, "invalid", context)
            .await
            .is_err()
    );
    let access = backend(&format!("{}/v1/project/access/authorize", oauth.origin));
    let authority = ProjectRecordAuthority::new(&issuer);
    let read_input = || BrokerRecordRead {
        caller: BrokerProjectCaller::Web(&caller),
        verifier: &verifier,
        backend: &access,
        original: &original,
        scope: project_scope(&fixture),
        provider: "broker",
        module: ProjectReadModule::Task,
    };
    // A valid JWT alone never grants PROJECT access.
    assert!(matches!(
        authority.admit_broker_read(read_input()).await,
        Err(BrokerProjectError::Denied)
    ));
    *oauth.state.project_response.lock().unwrap() = (200, reply(&oauth.origin).to_string());
    let admission = authority.admit_broker_read(read_input()).await.unwrap();
    assert_eq!(admission.module(), ProjectReadModule::Task);
    assert!(std::ptr::eq(admission.request().unwrap(), &original));
    assert_eq!(
        admission.project().unwrap().version.version(),
        fixture.version.version()
    );
    let requests = oauth.count("/v1/project/access/authorize");
    admission.recheck().unwrap();
    let (project, observed_request) = admission.project_and_request().unwrap();
    assert!(std::ptr::eq(observed_request, &original));
    assert!(std::ptr::eq(project.scope.live, &fixture.project));
    assert!(std::ptr::eq(project.version, &fixture.version));
    assert_eq!(oauth.count("/v1/project/access/authorize"), requests);
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/keys"), 1);
    // A new operation observes ACL denial; the old request is not a remote lease.
    *oauth.state.project_response.lock().unwrap() = (403, "{}".into());
    assert!(matches!(
        authority.admit_broker_read(read_input()).await,
        Err(BrokerProjectError::Denied)
    ));
    admission.recheck().unwrap();
    let foreign = request(&cancellation);
    let mut input = read_input();
    input.original = &foreign;
    let before = oauth.count("/v1/project/access/authorize");
    assert!(authority.admit_broker_read(input).await.is_err());
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);
    let snapshot = issuer.local.load(&original).unwrap();
    let revision = snapshot.document().revision();
    let _ = issuer
        .local
        .mutate(snapshot, &original, |document, request| {
            document.remove_provider("broker", false, revision, request)
        })
        .unwrap();
    assert!(admission.recheck().is_err());
    assert!(authority.admit_broker_read(read_input()).await.is_err());
    assert!(admission.project_and_request().is_err());
    cancellation.cancel();
    assert!(admission.recheck().is_err());
    assert!(admission.project_and_request().is_err());
    assert!(!fixture.source_root.join("records").exists());
}

#[tokio::test]
async fn broker_records_rejects_invalid_wire_without_identity_or_permission_fallback() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation);
    let fixture = Fixture::new(&original);
    let issuer = fixture.issuer(&original, limits());
    let revision = add_provider(&issuer, &original, &oauth.origin);
    let verifier = verifier(&oauth.origin, revision);
    let flow = WebOAuth::new(
        &verifier,
        oauth_limits(),
        "https://workspace.example/auth/callback",
        false,
    )
    .unwrap();
    let context = BrokerRequestContext::new(original.snapshot());
    let registered = flow.register(context).await.unwrap();
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let raw = token(
        &key,
        "one",
        &json!({"iss":oauth.origin,"sub":"native-subject",
        "aud":["native-api",registered.resource()],"client_id":registered.client_id(),
        "token_use":"access","jti":"records-test","scope":"","iat":now,"exp":now+600}),
    );
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    let access = backend(&format!("{}/v1/project/access/authorize", oauth.origin));
    let authority = ProjectRecordAuthority::new(&issuer);
    let good = reply(&oauth.origin);
    let mut invalid = vec![
        "[]".to_owned(),
        "{}".to_owned(),
        format!("{good}{{}}"),
        good.to_string().replacen(
            "\"action\":\"read\"",
            "\"action\":\"read\",\"action\":\"read\"",
            1,
        ),
    ];
    for edit in 0..9 {
        let mut value = good.clone();
        match edit {
            0 => value["subject"]["issuer"] = "https://foreign.example".into(),
            1 => value["subject"]["subject"] = "another-subject".into(),
            2 => value["module"] = "memory".into(),
            3 => value["action"] = "write".into(),
            4 => value["project_id"] = "01D39ZY06FGSCTVN4T2V9PKHFZ".into(),
            5 => value["authorization_revision"] = "4".into(),
            6 => value["extra"] = true.into(),
            7 => value["subject"]["extra"] = true.into(),
            8 => value["subject"] = json!([oauth.origin, "native-subject"]),
            _ => unreachable!(),
        }
        invalid.push(value.to_string());
    }
    for response in invalid {
        *oauth.state.project_response.lock().unwrap() = (200, response);
        let result = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(&caller),
                verifier: &verifier,
                backend: &access,
                original: &original,
                scope: project_scope(&fixture),
                provider: "broker",
                module: ProjectReadModule::Task,
            })
            .await;
        assert!(result.is_err());
    }
    *oauth.state.project_response.lock().unwrap() =
        (200, " ".repeat(network_limits().response_body_bytes + 1));
    assert!(matches!(
        authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(&caller),
                verifier: &verifier,
                backend: &access,
                original: &original,
                scope: project_scope(&fixture),
                provider: "broker",
                module: ProjectReadModule::Task,
            })
            .await,
        Err(BrokerProjectError::Network(_))
    ));
    *oauth.state.project_response.lock().unwrap() = (503, raw.clone());
    let error = authority
        .admit_broker_read(BrokerRecordRead {
            caller: BrokerProjectCaller::Web(&caller),
            verifier: &verifier,
            backend: &access,
            original: &original,
            scope: project_scope(&fixture),
            provider: "broker",
            module: ProjectReadModule::Task,
        })
        .await
        .err()
        .unwrap();
    assert_eq!(
        error,
        BrokerProjectError::Http(http::StatusCode::SERVICE_UNAVAILABLE)
    );
    assert!(!format!("{error:?} {error}").contains(&raw));
    assert!(!fixture.source_root.join("records").exists());
}

async fn ipc(
    input: &mut tokio::process::ChildStdin,
    output: &mut BufReader<tokio::process::ChildStdout>,
    value: Value,
) -> Value {
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    input.write_all(&bytes).await.unwrap();
    input.flush().await.unwrap();
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(180), output.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
#[ignore = "requires the qualified public Broker PostgreSQL/TLS PROJECT fixture binary and Docker"]
async fn broker_records_native_public_project_fixture() {
    let binary = std::env::var_os("GRAPHIT_BROKER_PROJECT_FIXTURE_BIN")
        .expect("configured public fixture binary");
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancellation = Cancellation::default();
    let original = request_before(&cancellation, Instant::now() + Duration::from_secs(240));
    let fixture = Fixture::new(&original);
    let issuer = fixture.issuer(&original, limits());
    let revision = add_provider(&issuer, &original, &oauth.origin);
    let mut process = tokio::process::Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = process.stdin.take().unwrap();
    let mut output = BufReader::new(process.stdout.take().unwrap());
    let ready = ipc(&mut input, &mut output, json!({"command":"start","issuer":oauth.origin,
        "api_audience":"native-api","resource":format!("{}/v1",oauth.origin),"client_id":"web-client",
        "subject":"native-subject","scope":"","project_ids":["01KSH1CRFFG8Z74B5ZS78WW808"],"access_ttl_seconds":600})).await;
    assert_eq!(ready["event"], "ready");
    assert_eq!(ready["required_scopes"], json!([]));
    *oauth.state.keys.lock().unwrap() = ready["trusted_jwks"].clone();
    *oauth.state.userinfo_status.lock().unwrap() = 503;
    let verifier = verifier(&oauth.origin, revision);
    let flow = WebOAuth::new(
        &verifier,
        oauth_limits(),
        "https://workspace.example/auth/callback",
        false,
    )
    .unwrap();
    let context = BrokerRequestContext::new(original.snapshot());
    let registered = flow.register(context).await.unwrap();
    let caller = flow
        .verify_access(
            &registered,
            ready["access_bearer"].as_str().unwrap(),
            context,
        )
        .await
        .unwrap();
    let access = backend(ready["authorize_endpoint"].as_str().unwrap());
    let authority = ProjectRecordAuthority::new(&issuer);
    for module in [
        ProjectReadModule::Task,
        ProjectReadModule::Memory,
        ProjectReadModule::Knowledge,
    ] {
        let admission = authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(&caller),
                verifier: &verifier,
                backend: &access,
                original: &original,
                scope: project_scope(&fixture),
                provider: "broker",
                module,
            })
            .await
            .unwrap();
        assert_eq!(admission.module(), module);
        assert_eq!(admission.recheck(), Ok(()));
    }
    let write_input = || BrokerRecordWrite {
        caller: &caller,
        verifier: &verifier,
        backend: &access,
        original: &original,
        scope: project_scope(&fixture),
        provider: "broker",
        operation: ProjectWriteOperation::Memory,
    };
    let write = authority.admit_broker_write(write_input()).await.unwrap();
    assert_eq!(write.recheck(), Ok(Obligation::Unconditional));
    assert!(std::ptr::eq(write.request().unwrap(), &original));
    assert_eq!(
        write.project().unwrap().version.version(),
        fixture.version.version()
    );
    let actor = write.actor().unwrap();
    let mut fenced_input = write_input();
    fenced_input.operation = ProjectWriteOperation::FencedMemory;
    let fenced = authority.admit_broker_write(fenced_input).await.unwrap();
    assert_eq!(fenced.recheck(), Ok(Obligation::MemoryFenceRequired));
    assert!(actor == fenced.actor().unwrap());
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/keys"), 1);
    let ack = ipc(&mut input, &mut output, json!({"command":"deny_access"})).await;
    assert_eq!(ack["ok"], true);
    assert!(matches!(
        authority.admit_broker_write(write_input()).await,
        Err(BrokerProjectError::Denied)
    ));
    assert!(matches!(
        authority
            .admit_broker_read(BrokerRecordRead {
                caller: BrokerProjectCaller::Web(&caller),
                verifier: &verifier,
                backend: &access,
                original: &original,
                scope: project_scope(&fixture),
                provider: "broker",
                module: ProjectReadModule::Task,
            })
            .await,
        Err(BrokerProjectError::Denied)
    ));
    let ack = ipc(&mut input, &mut output, json!({"command":"shutdown"})).await;
    assert_eq!(ack["ok"], true);
    drop(input);
    assert!(
        tokio::time::timeout(Duration::from_secs(30), process.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}
