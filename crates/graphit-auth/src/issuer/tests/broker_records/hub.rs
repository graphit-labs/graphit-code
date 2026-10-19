//! PROJECT Hub/read uses the same registered Web proof and exact response binding.
use super::*;

#[tokio::test]
async fn web_hub_read_requires_hub_project_decision_and_valid_jwt() {
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
        "token_use":"access","jti":"hub-read-test","scope":"","iat":now,"exp":now+600});
    *oauth.state.userinfo_status.lock().unwrap() = 503;
    let raw = token(&key, "one", &claims);
    let caller = flow
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
    // The locally valid Web token cannot supply the PROJECT capability itself.
    assert!(matches!(
        authority
            .admit_broker_read(input(ProjectReadModule::Hub))
            .await,
        Err(BrokerProjectError::Denied)
    ));

    // A Knowledge decision remains a different capability, even for the same
    // verified principal and project. It cannot become a Hub admission.
    let mut knowledge = reply(&oauth.origin);
    knowledge["module"] = "knowledge".into();
    *oauth.state.project_response.lock().unwrap() = (200, knowledge.to_string());
    let knowledge_read = authority
        .admit_broker_read(input(ProjectReadModule::Knowledge))
        .await
        .unwrap();
    assert_eq!(knowledge_read.module(), ProjectReadModule::Knowledge);
    assert!(matches!(
        authority
            .admit_broker_read(input(ProjectReadModule::Hub))
            .await,
        Err(BrokerProjectError::InvalidResponse)
    ));

    let mut hub = reply(&oauth.origin);
    hub["module"] = "hub".into();
    let mut other_project = hub.clone();
    other_project["project_id"] = "01KSH1CRFFG8Z74B5ZS78WW809".into();
    *oauth.state.project_response.lock().unwrap() = (200, other_project.to_string());
    assert!(matches!(
        authority
            .admit_broker_read(input(ProjectReadModule::Hub))
            .await,
        Err(BrokerProjectError::InvalidResponse)
    ));
    let mut write = hub.clone();
    write["action"] = "write".into();
    *oauth.state.project_response.lock().unwrap() = (200, write.to_string());
    assert!(matches!(
        authority
            .admit_broker_read(input(ProjectReadModule::Hub))
            .await,
        Err(BrokerProjectError::InvalidResponse)
    ));

    *oauth.state.project_response.lock().unwrap() = (200, hub.to_string());
    let read = authority
        .admit_broker_read(input(ProjectReadModule::Hub))
        .await
        .unwrap();
    assert_eq!(read.module(), ProjectReadModule::Hub);
    assert!(std::ptr::eq(read.request().unwrap(), &original));
    assert_eq!(
        read.project().unwrap().version.version(),
        fixture.version.version()
    );
    let before = oauth.count("/v1/project/access/authorize");
    read.recheck().unwrap();
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);

    let wrong_key = pair();
    let mut expired = claims.clone();
    expired["exp"] = json!(now - 1);
    for invalid in [
        "malformed".to_owned(),
        token(&wrong_key, "one", &claims),
        token(&key, "one", &expired),
    ] {
        assert!(
            flow.verify_access(&registered, &invalid, context)
                .await
                .is_err()
        );
    }
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/v1/s3/credentials"), 0);
    cancellation.cancel();
    assert_eq!(read.recheck(), Err(AuthError::Request));
}

#[tokio::test]
async fn installed_hub_source_requires_same_web_hub_project_proof() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancellation = Cancellation::default();
    let original = request(&cancellation);
    let fixture = Fixture::new(&original);
    let policy = || {
        authority(&all_permissions(), &all_permissions(), limits(), 7200)
            .with_web_installed_hub_read("broker")
            .unwrap()
    };
    let issuer = RoleIssuer::new(
        fixture.realm(),
        Arc::clone(&fixture.runtime),
        fixture.auth_store(&original),
        policy(),
        ttl(),
        &original,
    )
    .unwrap();
    let revision = add_provider(&issuer, &original, &oauth.origin);
    // Identical endpoint/configuration under another provider name still belongs
    // to a different selection. Populate it before capturing either observation.
    let snapshot = issuer.local.load(&original).unwrap();
    let config = snapshot
        .document()
        .provider("broker")
        .unwrap()
        .config()
        .clone();
    let _ = issuer
        .local
        .mutate(snapshot, &original, |document, ctx| {
            document.add_provider("alias".into(), config, document.revision(), 1, ctx)
        })
        .unwrap();
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
        "token_use":"access","jti":"paired-hub-read","scope":"","iat":now,"exp":now+600});
    let raw = token(&key, "one", &claims);
    *oauth.state.userinfo_status.lock().unwrap() = 503;
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    let installed = issuer
        .admit_web_installed_hub_read(&caller, &verifier, &original, "broker", fixture.realm())
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
    // Installed-artifact admission never manufactures the PROJECT capability.
    assert!(matches!(
        authority
            .admit_broker_read(input(ProjectReadModule::Hub))
            .await,
        Err(BrokerProjectError::Denied)
    ));
    let mut reply = reply(&oauth.origin);
    reply["module"] = "hub".into();
    *oauth.state.project_response.lock().unwrap() = (200, reply.to_string());
    let project = authority
        .admit_broker_read(input(ProjectReadModule::Hub))
        .await
        .unwrap();
    installed.check_project_source(&project).unwrap();
    let before = oauth.count("/v1/project/access/authorize");
    installed.check_project_source(&project).unwrap();
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);

    reply["module"] = "knowledge".into();
    *oauth.state.project_response.lock().unwrap() = (200, reply.to_string());
    let knowledge = authority
        .admit_broker_read(input(ProjectReadModule::Knowledge))
        .await
        .unwrap();
    assert_eq!(
        installed.check_project_source(&knowledge),
        Err(AuthError::Denied)
    );
    reply["module"] = "hub".into();
    *oauth.state.project_response.lock().unwrap() = (200, reply.to_string());
    let mut alias_input = input(ProjectReadModule::Hub);
    alias_input.provider = "alias";
    // This proof is bound to broker's provider revision. The SDK refuses
    // the alias before it can produce any PROJECT admission for the join.
    assert!(matches!(
        authority.admit_broker_read(alias_input).await,
        Err(BrokerProjectError::Authority(AuthError::Denied))
    ));

    // Even the same signed claims verified again are another borrowed Web proof.
    let other_caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    let mut other_input = input(ProjectReadModule::Hub);
    other_input.caller = BrokerProjectCaller::Web(&other_caller);
    let other = authority.admit_broker_read(other_input).await.unwrap();
    assert_eq!(
        installed.check_project_source(&other),
        Err(AuthError::Denied)
    );

    let foreign_original = request(&cancellation);
    let foreign_caller = flow
        .verify_access(
            &registered,
            &raw,
            BrokerRequestContext::new(foreign_original.snapshot()),
        )
        .await
        .unwrap();
    let mut foreign_input = input(ProjectReadModule::Hub);
    foreign_input.original = &foreign_original;
    foreign_input.caller = BrokerProjectCaller::Web(&foreign_caller);
    let foreign = authority.admit_broker_read(foreign_input).await.unwrap();
    assert_eq!(
        installed.check_project_source(&foreign),
        Err(AuthError::Denied)
    );

    let other_issuer = RoleIssuer::new(
        fixture.realm(),
        Arc::clone(&fixture.runtime),
        Arc::clone(&issuer.local),
        policy(),
        ttl(),
        &original,
    )
    .unwrap();
    let other_authority = ProjectRecordAuthority::new(&other_issuer);
    let other_project = other_authority
        .admit_broker_read(input(ProjectReadModule::Hub))
        .await
        .unwrap();
    assert_eq!(
        installed.check_project_source(&other_project),
        Err(AuthError::Denied)
    );

    let runtime = issuer
        .authenticate_runtime(&fixture.secret(), &original)
        .unwrap();
    let native = authority
        .admit_authenticated_read(&runtime, &original, fixture.scope(), ProjectReadModule::Hub)
        .unwrap();
    assert_eq!(
        installed.check_project_source(&native),
        Err(AuthError::Denied)
    );
    let before = oauth.count("/v1/project/access/authorize");
    cancellation.cancel();
    assert_eq!(
        installed.check_project_source(&project),
        Err(AuthError::Request)
    );
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/v1/s3/credentials"), 0);
}
