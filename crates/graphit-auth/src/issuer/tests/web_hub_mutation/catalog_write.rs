use super::*;

#[cfg(unix)]
#[tokio::test]
async fn remote_catalog_write_retains_session_without_local_source_or_admin() {
    incoming_catalog_session_survives_project_root_change_attempt().await;
}

#[cfg(windows)]
#[tokio::test]
async fn remote_catalog_write_refused_project_rename_preserves_session_without_admin() {
    incoming_catalog_session_survives_project_root_change_attempt().await;
}

async fn incoming_catalog_session_survives_project_root_change_attempt() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let issuer = issuer(
        &f,
        &ctx,
        &[write_rule(&oauth.origin, ServicePurpose::HubWrite)],
    );
    let revision = add_provider(&issuer, &ctx, &oauth.origin);
    let verifier = verifier(&oauth.origin, revision);
    let flow = WebOAuth::new(
        &verifier,
        oauth_limits(),
        "https://workspace.example/auth/callback",
        false,
    )
    .unwrap();
    let context = BrokerRequestContext::new(ctx.snapshot());
    let registered = flow.register(context).await.unwrap();
    let raw = token(
        &key,
        "one",
        &claims(
            &oauth.origin,
            registered.resource(),
            registered.client_id(),
            "web-a",
        ),
    );
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    #[cfg(unix)]
    std::fs::rename(
        f.project.owner_root(),
        f.project.owner_root().with_extension("unused"),
    )
    .unwrap();
    #[cfg(windows)]
    crate::issuer::tests::native_installation::assert_retained_scope_refuses_rename(
        &f.project,
        &f.project.owner_root().with_extension("unused"),
        &ctx,
    );
    decision(&oauth, "web-a", "read", 403);
    let before = oauth.count("/v1/project/access/authorize");
    let mut selection = issuer
        .capture_web_hub_catalog_write_service(&caller, &verifier, &ctx, "broker", f.realm())
        .unwrap();
    assert_eq!(selection.mode().unwrap(), ServiceMode::BrokerIncoming);
    let lease = selection.credential().unwrap();
    assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
    assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
    assert!(matches!(
        lease.binding().unwrap(),
        ServiceBindingView::Broker {
            purpose: ServicePurpose::HubWrite,
            provider: "broker",
            ..
        }
    ));
    assert_eq!(
        lease.user_subject().unwrap().unwrap().subject().unwrap(),
        "web-a"
    );
    assert!(lease.standalone_s3().is_err());
    let snapshot = issuer.local.load(&ctx).unwrap();
    assert!(snapshot.document().profiles().is_empty());
    assert!(snapshot.document().web_hub_administrators().is_empty());
    assert_eq!(oauth.count("/v1/project/access/authorize"), before);
    assert_eq!(oauth.count("/userinfo"), 0);
    assert_eq!(oauth.count("/token"), 0);
    for provider in ["", "unknown", &"x".repeat(limits().max_scope_bytes + 1)] {
        assert!(
            issuer
                .capture_web_hub_catalog_write_service(
                    &caller,
                    &verifier,
                    &ctx,
                    provider,
                    f.realm()
                )
                .is_err()
        );
    }
    let other = Fixture::new(&ctx);
    assert!(
        issuer
            .capture_web_hub_catalog_write_service(
                &caller,
                &verifier,
                &ctx,
                "broker",
                other.realm()
            )
            .is_err()
    );
    let different_verifier = self::verifier(&oauth.origin, revision);
    assert!(
        issuer
            .capture_web_hub_catalog_write_service(
                &caller,
                &different_verifier,
                &ctx,
                "broker",
                f.realm()
            )
            .is_err()
    );
    cancel.cancel();
    assert!(lease.credential().is_err());
}

#[tokio::test]
async fn remote_catalog_write_requires_exact_web_hub_write_rule() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    let cancel = Cancellation::default();
    let ctx = request(&cancel);
    let f = Fixture::new(&ctx);
    let provider_owner = issuer(&f, &ctx, &[]);
    let revision = add_provider(&provider_owner, &ctx, &oauth.origin);
    let verifier = verifier(&oauth.origin, revision);
    let flow = WebOAuth::new(
        &verifier,
        oauth_limits(),
        "https://workspace.example/auth/callback",
        false,
    )
    .unwrap();
    let context = BrokerRequestContext::new(ctx.snapshot());
    let registered = flow.register(context).await.unwrap();
    let raw = token(
        &key,
        "one",
        &claims(
            &oauth.origin,
            registered.resource(),
            registered.client_id(),
            "web-a",
        ),
    );
    let caller = flow
        .verify_access(&registered, &raw, context)
        .await
        .unwrap();
    for mismatch in 0..7 {
        let mut rule = write_rule(&oauth.origin, ServicePurpose::HubWrite);
        if let ServiceRuleInput::Remote {
            origin,
            binding:
                RemoteServiceBindingInput::Broker {
                    purpose,
                    endpoint,
                    resource,
                    ..
                },
            credential,
            ..
        } = &mut rule
        {
            match mismatch {
                0 => *purpose = ServicePurpose::HubRead,
                1 => *endpoint = "https://different.example",
                2 => *resource = Some("https://different.example/mcp"),
                3 => *credential = CredentialSource::NativeAccess,
                4 => *credential = CredentialSource::Anonymous,
                5 => *origin = ServiceOrigin::Local,
                _ => *credential = CredentialSource::LocalBrokerKey,
            }
        }
        let policy = super::super::super::service_credentials::policy(
            &[rule],
            &[],
            limits(),
            ServiceAdmissionLimits {
                max_rules: 16,
                max_binding_bytes: 8192,
            },
            Duration::ZERO,
        );
        if let Ok(policy) = policy {
            let denied = RoleIssuer::new(
                f.realm(),
                Arc::clone(&f.runtime),
                f.auth_store_policy(&ctx, document_limits()),
                policy,
                ttl(),
                &ctx,
            )
            .unwrap();
            assert!(
                denied
                    .capture_web_hub_catalog_write_service(
                        &caller,
                        &verifier,
                        &ctx,
                        "broker",
                        f.realm()
                    )
                    .is_err()
            );
        } else {
            assert!(mismatch >= 3);
            assert!(matches!(policy, Err(AuthError::Invalid)));
        }
    }
    assert!(
        provider_owner
            .capture_web_hub_catalog_write_service(&caller, &verifier, &ctx, "broker", f.realm())
            .is_err()
    );
    assert_eq!(oauth.count("/v1/project/access/authorize"), 0);
    assert_eq!(oauth.count("/userinfo"), 0);
}

#[tokio::test]
async fn remote_catalog_write_rechecks_observation_installation_and_original() {
    let key = pair();
    let oauth = OAuthFixture::new(&key).await;
    for ending in 0..4 {
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(if ending == 3 { 2 } else { 30 });
        let setup = request_before(&cancel, deadline);
        let f = Fixture::new(&setup);
        let issuer = issuer(
            &f,
            &setup,
            &[write_rule(&oauth.origin, ServicePurpose::HubWrite)],
        );
        let revision = add_provider(&issuer, &setup, &oauth.origin);
        let verifier = verifier(&oauth.origin, revision);
        let flow = WebOAuth::new(
            &verifier,
            oauth_limits(),
            "https://workspace.example/auth/callback",
            false,
        )
        .unwrap();
        let context = BrokerRequestContext::new(setup.snapshot());
        let registered = flow.register(context).await.unwrap();
        let raw = token(
            &key,
            "one",
            &claims(
                &oauth.origin,
                registered.resource(),
                registered.client_id(),
                "web-a",
            ),
        );
        let caller = flow
            .verify_access(&registered, &raw, context)
            .await
            .unwrap();
        let mut selection = issuer
            .capture_web_hub_catalog_write_service(&caller, &verifier, &setup, "broker", f.realm())
            .unwrap();
        let lease = selection.credential().unwrap();
        assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
        match ending {
            0 => configure(&issuer, &setup, json!([])),
            1 => {
                let root = f.installation.owner_root();
                #[cfg(unix)]
                {
                    std::fs::rename(root, root.with_extension("replaced")).unwrap();
                    std::fs::create_dir(root).unwrap();
                }
                #[cfg(windows)]
                crate::issuer::tests::native_installation::assert_retained_scope_refuses_rename(
                    &f.installation,
                    &root.with_extension("replaced"),
                    &setup,
                );
            }
            2 => cancel.cancel(),
            _ => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
        }
        if cfg!(windows) && ending == 1 {
            // The refused filesystem operation did not replace the bound
            // installation. Its incoming session remains the same authority.
            assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
            assert!(matches!(
                lease.binding().unwrap(),
                ServiceBindingView::Broker {
                    purpose: ServicePurpose::HubWrite,
                    provider: "broker",
                    ..
                }
            ));
            assert_eq!(
                lease.user_subject().unwrap().unwrap().subject().unwrap(),
                "web-a"
            );
        } else {
            assert!(lease.credential().is_err());
            assert!(lease.binding().is_err());
            assert!(lease.user_subject().is_err());
        }
    }
    assert_eq!(oauth.count("/v1/project/access/authorize"), 0);
    assert_eq!(oauth.count("/userinfo"), 0);
}
