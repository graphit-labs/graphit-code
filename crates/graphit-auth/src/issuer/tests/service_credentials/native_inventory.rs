use super::*;

fn inventory_rule(
    endpoint: &str,
    origin: ServiceOrigin,
    purpose: ServicePurpose,
) -> ServiceRuleInput<'_> {
    ServiceRuleInput::Remote {
        origin,
        binding: RemoteServiceBindingInput::BrokerInventory {
            provider: "service",
            purpose,
            endpoint,
        },
        credential: CredentialSource::NativeAccess,
        incoming: None,
    }
}
fn revision(store: &AuthStore, ctx: &RequestContext<'_>) -> u64 {
    store
        .load(ctx)
        .unwrap()
        .document()
        .provider("service")
        .unwrap()
        .revision()
}
fn claims(server: &Server) -> Value {
    json!({ "iss":server.origin, "sub":USER_SUB, "aud":["broker-api"],
        "client_id":"graphit-cli", "jti":"actual-native-inventory", "token_use":"access", "iat":now_seconds().saturating_sub(1),
        "exp":now_seconds()+3600 })
}

#[tokio::test]
async fn native_inventory_runtime_read_and_publish_keep_actual_provider_and_original() {
    for purpose in [
        ServicePurpose::InventoryRead,
        ServicePurpose::InventoryPublish,
    ] {
        for resource in [false, true] {
            let server = Server::new().await;
            let mut signed = claims(&server);
            if resource {
                signed["aud"] = json!(["broker-api", INCOMING_RESOURCE]);
            }
            let raw = server.sign(&signed);
            let cancel = Cancellation::default();
            let ctx = context(&cancel);
            let f = Fixture::new(&ctx);
            let store = native_store(&f, &ctx);
            let mut config = broker_config(&server.origin, true, false);
            if resource {
                let ProviderKind::Broker(broker) = &mut config.kind else {
                    unreachable!()
                };
                broker.mcp_resource = Some(INCOMING_RESOURCE.into());
            }
            put(
                &store,
                config,
                native_profile(&server.origin, &raw, now_seconds() + 3600),
                true,
                &ctx,
            );
            let revision = revision(&store, &ctx);
            let before = std::fs::read(f.installation.owner_root().join("auth.json")).unwrap();
            let verifier = server.verifier(resource, revision);
            let issuer = issuer(
                &f,
                store,
                &ctx,
                &[inventory_rule(
                    &server.origin,
                    ServiceOrigin::Runtime,
                    purpose,
                )],
            );
            let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
            let mut selection = issuer.capture_service(&caller, purpose).unwrap();
            assert_eq!(selection.mode().unwrap(), ServiceMode::BrokerNative);
            assert!(matches!(
                selection.credential(),
                Err(ServiceUseError::WrongMode)
            ));
            let lease = selection.native_credential(&verifier).await.unwrap();
            assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
            assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
            let ServiceBindingView::BrokerInventory {
                provider,
                purpose: actual,
                endpoint,
                provider_revision,
            } = lease.binding().unwrap()
            else {
                panic!("wrong inventory binding")
            };
            assert_eq!(provider, "service");
            assert_eq!(actual, purpose);
            assert_eq!(endpoint, url::Url::parse(&server.origin).unwrap().as_str());
            assert_eq!(provider_revision, revision);
            let user = lease.user_subject().unwrap().unwrap();
            assert_eq!(user.subject().unwrap(), USER_SUB);
            assert_eq!(user.issuer().unwrap(), server.origin);
            assert_eq!(user.origin().unwrap(), crate::VerifiedUserOrigin::Native);
            assert_eq!(
                std::fs::read(f.installation.owner_root().join("auth.json")).unwrap(),
                before
            );
            assert_eq!(server.count("/userinfo"), 0);
            assert_eq!(server.count("/token"), 0);
        }
    }
}

#[tokio::test]
async fn native_inventory_verifier_tuple_is_exact_and_has_no_local_fallback() {
    let server = Server::new().await;
    let other = Server::new().await;
    let raw = server.token(true, now_seconds() + 3600);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    put(
        &store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    let revision = revision(&store, &ctx);
    let issuer = issuer(
        &f,
        store,
        &ctx,
        &[inventory_rule(
            &server.origin,
            ServiceOrigin::Runtime,
            ServicePurpose::InventoryRead,
        )],
    );
    assert!(matches!(
        issuer.authenticate_local("arbitrary-local-key", &ctx),
        Err(AuthError::Denied)
    ));
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let mut selection = issuer
        .capture_service(&caller, ServicePurpose::InventoryRead)
        .unwrap();
    for verifier in [
        other.verifier(false, revision),
        server.verifier(false, revision + 1),
        server.verifier(true, revision),
    ] {
        assert!(matches!(
            selection.native_credential(&verifier).await,
            Err(ServiceUseError::Auth(AuthError::Denied))
        ));
    }
    let verifier = server.verifier(false, revision);
    assert!(selection.native_credential(&verifier).await.is_ok());
}

#[tokio::test]
async fn native_inventory_signed_claims_and_refresh_margin_remain_required() {
    for case in 0..6 {
        let server = Server::new().await;
        let mut signed = claims(&server);
        match case {
            0 => signed["sub"] = json!("another-subject"),
            1 => signed["client_id"] = json!("another-registered-client"),
            2 => signed["aud"] = json!([INCOMING_RESOURCE]),
            3 => signed["token_use"] = json!("id"),
            4 => signed["exp"] = json!(now_seconds().saturating_sub(1)),
            _ => (),
        }
        let raw = server.sign(&signed);
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        put(
            &store,
            broker_config(&server.origin, true, false),
            native_profile(&server.origin, &raw, now_seconds() + 3600),
            true,
            &ctx,
        );
        let verifier = server.verifier(false, revision(&store, &ctx));
        let issuer = issuer_config(
            &f,
            store,
            &ctx,
            &[inventory_rule(
                &server.origin,
                ServiceOrigin::Runtime,
                ServicePurpose::InventoryPublish,
            )],
            &all_permissions(),
            service_limits(),
            if case == 5 {
                Duration::from_secs(7200)
            } else {
                Duration::ZERO
            },
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::InventoryPublish)
            .unwrap();
        let result = selection.native_credential(&verifier).await;
        if case == 5 {
            assert!(matches!(result, Err(ServiceUseError::NeedsRefresh)));
        } else {
            assert!(matches!(result, Err(ServiceUseError::Broker(_))));
        }
        assert_eq!(server.count("/token"), 0);
    }
}

#[tokio::test]
async fn native_inventory_rechecks_real_authority_and_profile_across_jwks_await() {
    for change in 0..5 {
        let server = Server::new().await;
        server.pause_keys();
        let raw = server.token(true, now_seconds() + 3600);
        let cancel = Cancellation::default();
        let startup = context(&cancel);
        let f = Fixture::new(&startup);
        let store = native_store(&f, &startup);
        put(
            &store,
            broker_config(&server.origin, true, false),
            native_profile(&server.origin, &raw, now_seconds() + 3600),
            true,
            &startup,
        );
        let verifier = server.verifier(false, revision(&store, &startup));
        let issuer = issuer(
            &f,
            Arc::clone(&store),
            &startup,
            &[inventory_rule(
                &server.origin,
                ServiceOrigin::Runtime,
                ServicePurpose::InventoryPublish,
            )],
        );
        let ctx = if change == 4 {
            request_before(&cancel, Instant::now() + Duration::from_millis(200))
        } else {
            context(&cancel)
        };
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let mut selection = issuer
            .capture_service(&caller, ServicePurpose::InventoryPublish)
            .unwrap();
        let admission = selection.native_credential(&verifier);
        let alter = async {
            server.keys_entered().await;
            match change {
                0 => issuer.revoke_service_policy(&parent).unwrap(),
                1 => {
                    let mutation = context(&cancel);
                    put(
                        &store,
                        broker_config(&server.origin, true, false),
                        native_profile(&server.origin, &raw, now_seconds() + 7200),
                        true,
                        &mutation,
                    );
                }
                2 => cancel.cancel(),
                3 => std::fs::remove_file(f.installation.owner_root().join("daemon/mcp.key"))
                    .unwrap(),
                _ => tokio::time::sleep(Duration::from_millis(250)).await,
            }
            server.release_keys();
        };
        let (result, ()) = tokio::join!(admission, alter);
        assert!(result.is_err());
    }
}
