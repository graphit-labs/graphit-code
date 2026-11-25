use super::*;
use crate::issuer::service_credentials::S3CredentialView;
use crate::profile::LocalS3;

fn rule() -> ServiceRuleInput<'static> {
    ServiceRuleInput::StandaloneS3 {
        purpose: ServicePurpose::HubRead,
        origin: ServiceOrigin::Runtime,
        provider: "service",
        profile: Some("transport"),
    }
}
fn profile(expiry: Option<i64>) -> ProfileData {
    let mut data = local_profile("transport", MCP_A, None, false);
    data.username = "untrusted-profile-name".into();
    data.teams = vec!["untrusted-profile-team".into()];
    let ProfileCredentials::Local(keys) = &mut data.credentials else {
        unreachable!()
    };
    keys.s3 = Some(LocalS3 {
        access_key_id: Some(secret("selected-s3-access")),
        secret_access_key: Some(secret("selected-s3-secret")),
        session_token: Some(secret("selected-s3-session")),
        expires_at: expiry,
        aws_profile: None,
    });
    data
}

#[test]
fn standalone_transport_uses_only_explicit_pair_and_never_profile_identity() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    put(&store, local_config(), profile(None), false, &ctx);
    put(
        &store,
        local_config(),
        local_profile("other-active", MCP_B, None, false),
        true,
        &ctx,
    );
    let issuer = issuer(&f, Arc::clone(&store), &ctx, &[rule()]);
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(
        issuer
            .capture_standalone_s3_service(&caller, "service", Some("other-active"))
            .is_err()
    );
    assert!(issuer.capture_hub_service(&caller, "service").is_err());
    let mut selected = issuer
        .capture_standalone_s3_service(&caller, "service", Some("transport"))
        .unwrap();
    assert_eq!(selected.mode().unwrap(), ServiceMode::StandaloneS3);
    let lease = selected.credential().unwrap();
    assert!(lease.user_subject().unwrap().is_none());
    assert!(matches!(
        lease.credential(),
        Err(ServiceUseError::WrongMode)
    ));
    let view = lease.standalone_s3().unwrap();
    let S3CredentialView::Local(keys) = view.credential else {
        panic!("explicit local keys required")
    };
    assert_eq!(
        keys.access_key_id.as_ref().unwrap().expose(),
        "selected-s3-access"
    );
    assert_eq!(
        keys.session_token.as_ref().unwrap().expose(),
        "selected-s3-session"
    );
    assert_eq!(
        store
            .load(&ctx)
            .unwrap()
            .document()
            .active(&ctx)
            .unwrap()
            .1
            .name(),
        "other-active"
    );
    cancel.cancel();
    assert!(lease.standalone_s3().is_err());
}

#[test]
fn standalone_capture_rejects_stale_credentials_and_profile_replacement() {
    for expired in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        let expiry = i64::try_from(now_seconds()).unwrap() + if expired { -1 } else { 3600 };
        put(&store, local_config(), profile(Some(expiry)), false, &ctx);
        let issuer = issuer(&f, Arc::clone(&store), &ctx, &[rule()]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_standalone_s3_service(&caller, "service", Some("transport"))
            .unwrap();
        if expired {
            assert!(matches!(
                selected.credential(),
                Err(ServiceUseError::NeedsRefresh)
            ));
        } else {
            let lease = selected.credential().unwrap();
            put(
                &store,
                local_config(),
                profile(Some(expiry + 1)),
                false,
                &ctx,
            );
            assert!(lease.standalone_s3().is_err());
        }
    }
}

#[test]
fn standalone_anonymous_and_aws_chain_require_explicit_selected_configuration() {
    for anonymous in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        let mut config = local_config();
        let mut data = local_profile("transport", MCP_A, None, anonymous);
        if !anonymous {
            config.kind = ProviderKind::Local {
                allow_aws_credential_chain: true,
                broker: None,
            };
            config.s3.source = S3Source::AwsChain;
            let ProfileCredentials::Local(keys) = &mut data.credentials else {
                unreachable!()
            };
            keys.s3 = Some(LocalS3 {
                access_key_id: None,
                secret_access_key: None,
                session_token: None,
                expires_at: None,
                aws_profile: Some("explicit-aws-profile".into()),
            });
        }
        if anonymous {
            let before = store.load(&ctx).unwrap();
            applied(
                store
                    .mutate(before, &ctx, |doc, ctx| {
                        doc.add_provider("service".into(), config, doc.revision(), 1, ctx)
                    })
                    .unwrap(),
            );
        } else {
            put(&store, config, data, false, &ctx);
        }
        let selected_rule = if anonymous {
            ServiceRuleInput::StandaloneS3 {
                purpose: ServicePurpose::HubRead,
                origin: ServiceOrigin::Runtime,
                provider: "service",
                profile: None,
            }
        } else {
            rule()
        };
        let issuer = issuer(&f, store, &ctx, &[selected_rule]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let chosen_profile = (!anonymous).then_some("transport");
        assert!(
            issuer
                .capture_standalone_s3_service(
                    &caller,
                    "service",
                    if anonymous { Some("transport") } else { None }
                )
                .is_err()
        );
        let mut selected = issuer
            .capture_standalone_s3_service(&caller, "service", chosen_profile)
            .unwrap();
        let lease = selected.credential().unwrap();
        match lease.standalone_s3().unwrap().credential {
            S3CredentialView::Anonymous => assert!(anonymous),
            S3CredentialView::AwsChain { profile } => {
                assert!(!anonymous);
                assert_eq!(profile, Some("explicit-aws-profile"));
            }
            S3CredentialView::Local(_) => panic!("no ambient local keys"),
        }
        assert!(lease.user_subject().unwrap().is_none());
    }
}

mod multiple_profiles;
