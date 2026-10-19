use super::*;
use crate::{
    ServiceBindingView, ServiceMode, ServiceOrigin, ServicePurpose, ServiceRuleInput,
    ServiceUseError,
    profile::{LocalCredentials, LocalDevice, LocalExecution, ProfileCredentials, ProfileData},
};

fn execution() -> LocalExecution {
    LocalExecution {
        device: LocalDevice::Cpu,
        device_id: 0,
    }
}

fn model_profile(username: &str) -> ProfileData {
    ProfileData {
        name: "local-model".into(),
        provider: "service".into(),
        username: username.into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous: false,
            mcp_key: None,
            broker_key: None,
            embedding_api_key: None,
            rerank_api_key: None,
            s3: None,
        }),
    }
}

fn model_config() -> ProviderConfig {
    ProviderConfig {
        kind: ProviderKind::Local {
            allow_aws_credential_chain: false,
            broker: None,
        },
        embedding: AiService::Local(execution()),
        rerank: AiService::Local(execution()),
        s3: S3Location {
            bucket: None,
            region: None,
            endpoint: None,
            prefix: None,
            source: S3Source::LocalCredentials,
        },
    }
}

#[tokio::test]
async fn web_ai_requires_exact_web_project_read_and_live_local_lease() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation);
    let fixture = Fixture::new(&original);
    let store = fixture.auth_store(&original);
    super::super::service_credentials::put(
        &store,
        model_config(),
        model_profile("configured-model"),
        true,
        &original,
    );
    let rules: Vec<_> = [ServicePurpose::Embedding, ServicePurpose::Rerank]
        .into_iter()
        .map(|purpose| ServiceRuleInput::Local {
            origin: ServiceOrigin::Web,
            provider: "service",
            purpose,
            execution: execution(),
        })
        .collect();
    let issuer = super::super::service_credentials::issuer(&fixture, store, &original, &rules);
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
        "token_use":"access","jti":"web-embedding","scope":"","iat":now,"exp":now+600}),
    );
    *oauth.state.userinfo_status.lock().unwrap() = 503;
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    let other_caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    let access = backend(&format!("{}/v1/project/access/authorize", oauth.origin));
    let authority = ProjectRecordAuthority::new(&issuer);
    let input = |module| BrokerRecordRead {
        caller: BrokerProjectCaller::Web(&caller),
        verifier: &verifier,
        backend: &access,
        original: &original,
        scope: project_scope(&fixture),
        provider: "broker",
        module,
    };
    assert!(matches!(
        authority
            .admit_broker_read(input(ProjectReadModule::Ast))
            .await,
        Err(BrokerProjectError::Denied)
    ));
    let foreign_original = request(&cancellation);
    let foreign_issuer = fixture.issuer(&original, limits());
    for (module, name) in [
        (ProjectReadModule::Ast, "ast"),
        (ProjectReadModule::Knowledge, "knowledge"),
        (ProjectReadModule::Task, "task"),
    ] {
        let mut response = reply(&oauth.origin);
        response["module"] = json!(name);
        *oauth.state.project_response.lock().unwrap() = (200, response.to_string());
        let admission = authority.admit_broker_read(input(module)).await.unwrap();
        for denied in [
            issuer.capture_web_rerank_service(&other_caller, &admission, &original),
            issuer.capture_web_rerank_service(&caller, &admission, &foreign_original),
            foreign_issuer.capture_web_rerank_service(&caller, &admission, &original),
            issuer.capture_web_embedding_service(&other_caller, &admission, &original),
            issuer.capture_web_embedding_service(&caller, &admission, &foreign_original),
            foreign_issuer.capture_web_embedding_service(&caller, &admission, &original),
        ] {
            assert!(matches!(
                denied,
                Err(ServiceUseError::Auth(AuthError::Denied))
            ));
        }
        for purpose in [ServicePurpose::Embedding, ServicePurpose::Rerank] {
            let mut service = match purpose {
                ServicePurpose::Embedding => {
                    issuer.capture_web_embedding_service(&caller, &admission, &original)
                }
                _ => issuer.capture_web_rerank_service(&caller, &admission, &original),
            }
            .unwrap();
            assert_eq!(service.mode().unwrap(), ServiceMode::LocalAuthorized);
            assert!(matches!(
                service.binding().unwrap(),
                ServiceBindingView::Local {
                    provider: "service",
                    purpose: actual,
                    ..
                } if actual == purpose
            ));
            assert!(matches!(
                service.credential(),
                Err(ServiceUseError::WrongMode)
            ));
            let lease = service.local().unwrap();
            assert_eq!(lease.execution().unwrap(), execution());
            assert!(Arc::ptr_eq(
                lease.original().unwrap().snapshot(),
                original.snapshot()
            ));
            lease.check().unwrap();
        }
    }
    let mut response = reply(&oauth.origin);
    response["module"] = json!("knowledge");
    *oauth.state.project_response.lock().unwrap() = (200, response.to_string());
    let wrong_provider = super::super::service_credentials::issuer(
        &fixture,
        Arc::clone(&issuer.local),
        &original,
        &[ServiceRuleInput::Local {
            origin: ServiceOrigin::Web,
            provider: "another-model-provider",
            purpose: ServicePurpose::Embedding,
            execution: execution(),
        }],
    );
    let wrong_authority = ProjectRecordAuthority::new(&wrong_provider);
    let wrong_admission = wrong_authority
        .admit_broker_read(input(ProjectReadModule::Knowledge))
        .await
        .unwrap();
    assert!(matches!(
        wrong_provider.capture_web_embedding_service(&caller, &wrong_admission, &original),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    let mut response = reply(&oauth.origin);
    response["module"] = json!("memory");
    *oauth.state.project_response.lock().unwrap() = (200, response.to_string());
    let memory = authority
        .admit_broker_read(input(ProjectReadModule::Memory))
        .await
        .unwrap();
    assert!(matches!(
        issuer.capture_web_embedding_service(&caller, &memory, &original),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    let mut response = reply(&oauth.origin);
    response["module"] = json!("knowledge");
    *oauth.state.project_response.lock().unwrap() = (200, response.to_string());
    let admission = authority
        .admit_broker_read(input(ProjectReadModule::Knowledge))
        .await
        .unwrap();
    let mut service = issuer
        .capture_web_embedding_service(&caller, &admission, &original)
        .unwrap();
    let lease = service.local().unwrap();
    super::super::service_credentials::put(
        &issuer.local,
        model_config(),
        model_profile("changed"),
        true,
        &original,
    );
    assert!(lease.check().is_err());
    drop(service);
    let admission = authority
        .admit_broker_read(input(ProjectReadModule::Knowledge))
        .await
        .unwrap();
    let mut service = issuer
        .capture_web_embedding_service(&caller, &admission, &original)
        .unwrap();
    let lease = service.local().unwrap();
    cancellation.cancel();
    assert!(lease.check().is_err());
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/keys"), 1);
}

#[tokio::test]
async fn web_remote_ai_keeps_project_read_broker_and_original_without_an_active_profile() {
    use crate::{CredentialSource, RemoteServiceBindingInput};
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation);
    let fixture = Fixture::new(&original);
    let store = fixture.auth_store(&original);
    let result = store
        .mutate(
            store.load(&original).unwrap(),
            &original,
            |document, request| {
                document.add_provider(
                    "broker".into(),
                    super::super::service_credentials::broker_config(&oauth.origin, true, false),
                    document.revision(),
                    1,
                    request,
                )
            },
        )
        .unwrap();
    assert!(result.failure().is_none());
    let revision = store
        .load(&original)
        .unwrap()
        .document()
        .provider("broker")
        .unwrap()
        .revision();
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
        "token_use":"access","jti":"web-ai","scope":"","iat":now,"exp":now+600}),
    );
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    let access = backend(&format!("{}/v1/project/access/authorize", oauth.origin));
    let foreign_original = request(&cancellation);
    for variant in 0..3 {
        let rules: Vec<_> = [ServicePurpose::Embedding, ServicePurpose::Rerank]
            .into_iter()
            .map(|purpose| ServiceRuleInput::Remote {
                origin: ServiceOrigin::Web,
                binding: RemoteServiceBindingInput::Broker {
                    provider: if variant == 1 { "other" } else { "broker" },
                    purpose,
                    endpoint: &oauth.origin,
                    resource: (variant == 2).then_some("https://other.example/resource"),
                },
                credential: CredentialSource::IncomingBearer,
                incoming: None,
            })
            .collect();
        let issuer = super::super::service_credentials::issuer(
            &fixture,
            Arc::clone(&store),
            &original,
            &rules,
        );
        let authority = ProjectRecordAuthority::new(&issuer);
        for (module, name) in [
            (ProjectReadModule::Ast, "ast"),
            (ProjectReadModule::Knowledge, "knowledge"),
        ] {
            let mut response = reply(&oauth.origin);
            response["action"] = json!("read");
            response["module"] = json!(name);
            *oauth.state.project_response.lock().unwrap() = (200, response.to_string());
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
            for purpose in [ServicePurpose::Embedding, ServicePurpose::Rerank] {
                let selected = match purpose {
                    ServicePurpose::Embedding => {
                        issuer.capture_web_embedding_service(&caller, &admission, &original)
                    }
                    _ => issuer.capture_web_rerank_service(&caller, &admission, &original),
                };
                if variant != 0 {
                    assert!(matches!(
                        selected,
                        Err(ServiceUseError::Auth(AuthError::Denied))
                    ));
                    continue;
                }
                let mut selected = selected.unwrap();
                assert_eq!(selected.mode().unwrap(), ServiceMode::BrokerIncoming);
                assert!(
                    matches!(selected.binding().unwrap(), ServiceBindingView::Broker { provider: "broker", purpose: actual, provider_revision, .. } if actual == purpose && provider_revision == revision)
                );
                let lease = selected.credential().unwrap();
                assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
                assert!(Arc::ptr_eq(
                    lease.original().unwrap().snapshot(),
                    original.snapshot()
                ));
                lease.check().unwrap();
                assert!(
                    issuer
                        .capture_web_rerank_service(&caller, &admission, &foreign_original)
                        .is_err()
                );
            }
        }
    }
    assert_eq!(oauth.count("/userinfo"), 0);
    assert!(
        store
            .load(&original)
            .unwrap()
            .document()
            .profiles()
            .is_empty()
    );
}
