use super::service_credentials::{
    INCOMING_RESOURCE, Server, USER_SUB, broker_config, context, issuer, native_profile,
    native_store, now_seconds, put,
};
use super::*;
use crate::{
    CredentialSource, RemoteServiceBindingInput, ServiceAdmissionLimits, ServiceAuthorityInput,
    ServiceBindingView, ServiceMode, ServiceOrigin, ServicePurpose, ServiceRuleInput,
    ServiceUseError,
    broker::BrokerError,
    profile::{AiService, LocalCredentials, ProfileCredentials, ProfileData, ProfileError, Secret},
};
use serde_json::json;
use std::future::{Future, poll_fn};
use std::task::Poll;

// Hub transport fixtures need S3 enabled even when AI services are disabled.
// The shared helper intentionally disables S3 for its AI-only consumers.
fn hub_native_profile(origin: &str, raw: &str, expiration: u64) -> ProfileData {
    let mut profile = native_profile(origin, raw, expiration);
    let ProfileCredentials::Broker { s3_disabled, .. } = &mut profile.credentials else {
        unreachable!()
    };
    *s3_disabled = false;
    profile
}

fn rule(endpoint: &str, credential: CredentialSource) -> ServiceRuleInput<'_> {
    ServiceRuleInput::Remote {
        origin: ServiceOrigin::Runtime,
        binding: RemoteServiceBindingInput::Broker {
            provider: "service",
            purpose: ServicePurpose::HubWrite,
            endpoint,
            resource: None,
        },
        credential,
        incoming: None,
    }
}

fn policy(
    rules: &[ServiceRuleInput<'_>],
    ordinary: &[Permission],
) -> Result<crate::IssuerAuthorityPolicy, AuthError> {
    let all = all_permissions();
    let sessions = crate::AdminSessionPolicy {
        lifetime: Duration::from_secs(7200),
        maximum: Duration::from_secs(7200),
    };
    crate::IssuerAuthorityPolicy::new(
        crate::SourceAuthorityPolicy {
            ordinary,
            administrative: &all,
            sessions,
        },
        crate::SourceAuthorityPolicy {
            ordinary,
            administrative: &all,
            sessions,
        },
        limits(),
        ServiceAuthorityInput {
            rules,
            limits: ServiceAdmissionLimits {
                max_rules: 16,
                max_binding_bytes: 8192,
            },
            native_refresh_margin: Duration::ZERO,
        },
    )
}

fn local_profile(key: Option<&str>) -> ProfileData {
    ProfileData {
        name: "public".into(),
        provider: "service".into(),
        username: if key.is_none() {
            "anonymous"
        } else {
            "display-only"
        }
        .into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous: key.is_none(),
            mcp_key: Some(Secret::new(SecretString::from("local-hub-mcp"))),
            broker_key: key.map(|key| Secret::new(SecretString::from(key))),
            embedding_api_key: None,
            rerank_api_key: None,
            s3: None,
        }),
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

#[tokio::test]
async fn opaque_hub_write_has_its_own_binding_and_checks_auth_replacement() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    put(
        &store,
        broker_config(&server.origin, false, false),
        local_profile(Some("explicit-opaque-write")),
        true,
        &ctx,
    );
    let issuer = issuer(
        &f,
        Arc::clone(&store),
        &ctx,
        &[rule(&server.origin, CredentialSource::LocalBrokerKey)],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(issuer.capture_hub_service(&caller, "service").is_err());
    assert!(
        issuer
            .capture_service(&caller, ServicePurpose::HubWrite)
            .is_err()
    );
    let mut selected = issuer
        .capture_hub_write_service(&caller, "service")
        .unwrap();
    assert_eq!(selected.mode().unwrap(), ServiceMode::BrokerKey);
    let verifier = server.verifier(false, revision(&store, &ctx));
    assert!(matches!(
        selected.native_credential(&verifier).await,
        Err(ServiceUseError::WrongMode)
    ));
    let mut another = issuer
        .capture_hub_write_service(&caller, "service")
        .unwrap();
    let lease = selected.credential().unwrap();
    assert_eq!(lease.credential().unwrap(), Some("explicit-opaque-write"));
    assert!(lease.user_subject().unwrap().is_none());
    assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
    assert!(matches!(
        lease.binding().unwrap(),
        ServiceBindingView::Broker {
            purpose: ServicePurpose::HubWrite,
            ..
        }
    ));
    put(
        &store,
        broker_config(&server.origin, false, false),
        local_profile(Some("replacement-write-key")),
        true,
        &ctx,
    );
    assert!(another.provider_config().is_err());
    assert!(lease.check().is_err());
    assert!(lease.credential().is_err());
    assert!(lease.user_subject().is_err());
    assert_eq!(server.count("/service"), 0);
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn hub_read_and_missing_profiles_do_not_authorize_write() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    let raw = server.token(true, now_seconds() + 3600);
    put(
        &store,
        broker_config(&server.origin, true, false),
        hub_native_profile(&server.origin, &raw, now_seconds() + 3600),
        false,
        &ctx,
    );
    let allowed = issuer(
        &f,
        Arc::clone(&store),
        &ctx,
        &[rule(&server.origin, CredentialSource::NativeAccess)],
    );
    let caller = allowed.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(
        allowed
            .capture_hub_write_service(&caller, "service")
            .is_err()
    );
    put(
        &store,
        broker_config(&server.origin, true, false),
        hub_native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    let mut read = rule(&server.origin, CredentialSource::NativeAccess);
    let ServiceRuleInput::Remote {
        binding: RemoteServiceBindingInput::Broker { purpose, .. },
        ..
    } = &mut read
    else {
        unreachable!()
    };
    *purpose = ServicePurpose::HubRead;
    let read_issuer = issuer(&f, Arc::clone(&store), &ctx, &[read]);
    let caller = read_issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(read_issuer.capture_hub_service(&caller, "service").is_ok());
    assert!(
        read_issuer
            .capture_hub_write_service(&caller, "service")
            .is_err()
    );
    let local = Fixture::new(&ctx);
    let store = native_store(&local, &ctx);
    put(
        &store,
        broker_config(&server.origin, false, true),
        local_profile(None),
        true,
        &ctx,
    );
    let native_only = issuer(
        &local,
        store,
        &ctx,
        &[rule(&server.origin, CredentialSource::NativeAccess)],
    );
    let caller = native_only
        .authenticate_runtime(&local.secret(), &ctx)
        .unwrap();
    assert!(
        native_only
            .capture_hub_write_service(&caller, "service")
            .is_err()
    );
    assert_eq!(server.count("/service"), 0);
}

#[test]
fn hub_write_policy_rejects_borrowed_modes_and_duplicate_bindings() {
    let endpoint = "https://broker.example";
    let native = rule(endpoint, CredentialSource::NativeAccess);
    let opaque = rule(endpoint, CredentialSource::LocalBrokerKey);
    assert!(policy(&[native], &all_permissions()).is_ok());
    assert!(policy(&[opaque], &all_permissions()).is_ok());
    for rules in [
        vec![native, native],
        vec![native, opaque],
        vec![rule(endpoint, CredentialSource::Anonymous)],
        vec![rule(endpoint, CredentialSource::IncomingBearer)],
        vec![rule(endpoint, CredentialSource::DirectKey)],
    ] {
        assert!(matches!(
            policy(&rules, &all_permissions()),
            Err(AuthError::Invalid)
        ));
    }
    let mut incoming = native;
    let ServiceRuleInput::Remote { origin, .. } = &mut incoming else {
        unreachable!()
    };
    *origin = ServiceOrigin::BrokerIncoming;
    assert!(matches!(
        policy(&[incoming], &all_permissions()),
        Err(AuthError::Invalid)
    ));
    let local = ServiceRuleInput::Local {
        origin: ServiceOrigin::Runtime,
        provider: "service",
        purpose: ServicePurpose::HubWrite,
        execution: crate::profile::LocalExecution {
            device: crate::profile::LocalDevice::Cpu,
            device_id: 0,
        },
    };
    assert!(matches!(
        policy(&[local], &all_permissions()),
        Err(AuthError::Invalid)
    ));
    let direct = ServiceRuleInput::Remote {
        origin: ServiceOrigin::Runtime,
        binding: RemoteServiceBindingInput::Direct {
            provider: "service",
            purpose: ServicePurpose::HubWrite,
            protocol: crate::profile::DirectProtocol::OpenAi,
            endpoint,
            model: "fixture",
            model_revision: "fixture-deployment-2026-10",
            recipe: "fixture-vector-v1",
            dimensions: 3,
            request_dimensions: None,
            batch_endpoint: None,
        },
        credential: CredentialSource::DirectKey,
        incoming: None,
    };
    assert!(matches!(
        policy(&[direct], &all_permissions()),
        Err(AuthError::Invalid)
    ));
    let inventory = ServiceRuleInput::Remote {
        origin: ServiceOrigin::Runtime,
        binding: RemoteServiceBindingInput::BrokerInventory {
            provider: "service",
            purpose: ServicePurpose::HubWrite,
            endpoint,
        },
        credential: CredentialSource::NativeAccess,
        incoming: None,
    };
    assert!(matches!(
        policy(&[inventory], &all_permissions()),
        Err(AuthError::Invalid)
    ));
}

#[tokio::test]
async fn native_hub_write_uses_explicit_provider_and_real_subject_with_ai_disabled() {
    for resource in [None, Some(INCOMING_RESOURCE)] {
        let server = Server::new().await;
        let signed = json!({"iss":server.origin,"sub":USER_SUB,"aud":
            resource.map_or(json!(["broker-api"]), |r| json!(["broker-api",r])),
            "client_id":"graphit-cli","jti":"hub-native","token_use":"access",
            "iat":now_seconds().saturating_sub(1),"exp":now_seconds()+3600});
        let raw = server.sign(&signed);
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        let mut config = broker_config(&server.origin, true, false);
        config.embedding = AiService::Disabled;
        config.rerank = AiService::Disabled;
        config.s3.bucket = Some("explicit-hub-bucket".into());
        let crate::profile::ProviderKind::Broker(broker) = &mut config.kind else {
            unreachable!()
        };
        broker.mcp_resource = resource.map(str::to_owned);
        put(
            &store,
            config,
            hub_native_profile(&server.origin, &raw, now_seconds() + 3600),
            true,
            &ctx,
        );
        let verifier = server.verifier(resource.is_some(), revision(&store, &ctx));
        let mut binding = rule(&server.origin, CredentialSource::NativeAccess);
        let ServiceRuleInput::Remote {
            binding: RemoteServiceBindingInput::Broker { resource: r, .. },
            ..
        } = &mut binding
        else {
            unreachable!()
        };
        *r = resource;
        let issuer = issuer(&f, store, &ctx, &[binding]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selection = issuer
            .capture_hub_write_service(&caller, "service")
            .unwrap();
        assert_eq!(
            selection.provider_config().unwrap().s3.bucket.as_deref(),
            Some("explicit-hub-bucket")
        );
        assert_eq!(selection.mode().unwrap(), ServiceMode::BrokerNative);
        let lease = selection.native_credential(&verifier).await.unwrap();
        assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
        assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
        assert!(matches!(
            lease.binding().unwrap(),
            ServiceBindingView::Broker {
                purpose: ServicePurpose::HubWrite,
                ..
            }
        ));
        let user = lease.user_subject().unwrap().unwrap();
        assert_eq!(user.subject().unwrap(), USER_SUB);
        assert_eq!(user.issuer().unwrap(), server.origin);
        server.post(lease.credential().unwrap()).await;
        lease.check().unwrap();
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}

#[tokio::test]
async fn native_binding_and_signed_claim_failures_never_borrow_another_purpose() {
    let server = Server::new().await;
    for case in 0..8 {
        let mut claims = json!({"iss":server.origin,"sub":USER_SUB,"aud":["broker-api"],
            "client_id":"graphit-cli","jti":"hub-negative","token_use":"access",
            "iat":now_seconds().saturating_sub(2),"exp":now_seconds()+3600});
        match case {
            0 => claims["client_id"] = json!("another-registered-client"),
            1 => claims["aud"] = json!([INCOMING_RESOURCE]),
            2 => claims["iss"] = json!("https://foreign.example"),
            3 => claims["token_use"] = json!("id"),
            4 => claims["sub"] = json!("foreign-subject"),
            5 => claims["exp"] = json!(now_seconds().saturating_sub(1)),
            _ => (),
        }
        let raw = server.sign(&claims);
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        let mut config = broker_config(&server.origin, true, false);
        let mut binding = rule(&server.origin, CredentialSource::NativeAccess);
        if case == 7 {
            let crate::profile::ProviderKind::Broker(broker) = &mut config.kind else {
                unreachable!()
            };
            broker.mcp_resource = Some(INCOMING_RESOURCE.into());
            let ServiceRuleInput::Remote {
                binding: RemoteServiceBindingInput::Broker { resource, .. },
                ..
            } = &mut binding
            else {
                unreachable!()
            };
            *resource = Some(INCOMING_RESOURCE);
        }
        put(
            &store,
            config,
            hub_native_profile(&server.origin, &raw, now_seconds() + 3600),
            true,
            &ctx,
        );
        let verifier = server.verifier(case == 7, revision(&store, &ctx) + u64::from(case == 6));
        let issuer = issuer(&f, store, &ctx, &[binding]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_hub_write_service(&caller, "service")
            .unwrap();
        assert!(
            selected.native_credential(&verifier).await.is_err(),
            "case {case}"
        );
        assert_eq!(server.count("/service"), 0);
    }
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn provider_permission_and_inventory_admission_are_checked_before_transport() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    let raw = server.token(true, now_seconds() + 3600);
    put(
        &store,
        broker_config(&server.origin, true, false),
        hub_native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    let rules = [rule(&server.origin, CredentialSource::NativeAccess)];
    let denied = RoleIssuer::new(
        f.realm(),
        Arc::clone(&f.runtime),
        Arc::clone(&store),
        policy(&rules, &[Permission::InspectGit]).unwrap(),
        ttl(),
        &ctx,
    )
    .unwrap();
    let caller = denied.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        denied.capture_hub_write_service(&caller, "service"),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    let inventory = ServiceRuleInput::Remote {
        origin: ServiceOrigin::Runtime,
        binding: RemoteServiceBindingInput::BrokerInventory {
            provider: "service",
            purpose: ServicePurpose::InventoryRead,
            endpoint: &server.origin,
        },
        credential: CredentialSource::NativeAccess,
        incoming: None,
    };
    let unrelated = issuer(&f, Arc::clone(&store), &ctx, &[inventory]);
    let caller = unrelated.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(
        unrelated
            .capture_hub_write_service(&caller, "service")
            .is_err()
    );
    let allowed = issuer(&f, Arc::clone(&store), &ctx, &rules);
    let caller = allowed.authenticate_runtime(&f.secret(), &ctx).unwrap();
    assert!(matches!(
        allowed.capture_hub_write_service(&caller, ""),
        Err(ServiceUseError::Auth(AuthError::Invalid))
    ));
    assert!(matches!(
        allowed.capture_hub_write_service(&caller, &"x".repeat(limits().max_scope_bytes + 1)),
        Err(ServiceUseError::Auth(AuthError::Limit))
    ));
    assert!(
        allowed
            .capture_hub_write_service(&caller, "not-selected")
            .is_err()
    );
    let before = store.load(&ctx).unwrap();
    let published = store
        .mutate(before, &ctx, |doc, ctx| {
            doc.add_provider(
                "other".into(),
                broker_config(&server.origin, true, false),
                doc.revision(),
                1,
                ctx,
            )
        })
        .unwrap();
    assert_eq!(
        published.effect(),
        graphit_core::auth_document::publication::AuthPublicationEffect::Applied
    );
    let other = ServiceRuleInput::Remote {
        binding: RemoteServiceBindingInput::Broker {
            provider: "other",
            purpose: ServicePurpose::HubWrite,
            endpoint: &server.origin,
            resource: None,
        },
        origin: ServiceOrigin::Runtime,
        credential: CredentialSource::NativeAccess,
        incoming: None,
    };
    let other_issuer = issuer(&f, store, &ctx, &[other]);
    let caller = other_issuer
        .authenticate_runtime(&f.secret(), &ctx)
        .unwrap();
    assert!(matches!(
        other_issuer.capture_hub_write_service(&caller, "other"),
        Err(ServiceUseError::Auth(AuthError::Denied))
    ));
    assert_eq!(server.count("/.well-known/graphit-broker"), 0);
    assert_eq!(server.count("/service"), 0);
}

#[tokio::test]
async fn native_hub_verification_keeps_original_cancellation_and_deadline_during_wait() {
    for deadline_case in [false, true] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let setup = context(&cancel);
        let f = Fixture::new(&setup);
        let store = native_store(&f, &setup);
        let raw = server.token(true, now_seconds() + 3600);
        put(
            &store,
            broker_config(&server.origin, true, false),
            hub_native_profile(&server.origin, &raw, now_seconds() + 3600),
            true,
            &setup,
        );
        let verifier = server.verifier(false, revision(&store, &setup));
        let deadline = Instant::now() + Duration::from_secs(2);
        let ctx = request_before(&cancel, deadline);
        let issuer = issuer(
            &f,
            store,
            &ctx,
            &[rule(&server.origin, CredentialSource::NativeAccess)],
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_hub_write_service(&caller, "service")
            .unwrap();
        let mut future = Box::pin(selected.native_credential(&verifier));
        poll_fn(|cx| {
            assert!(future.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        if deadline_case {
            tokio::time::sleep_until(deadline.into()).await;
        } else {
            cancel.cancel();
        }
        let result = future.await;
        if deadline_case {
            assert!(matches!(
                result,
                Err(ServiceUseError::Broker(BrokerError::Deadline)
                    | ServiceUseError::Profile(ProfileError::Deadline))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ServiceUseError::Broker(BrokerError::Cancelled)
                    | ServiceUseError::Profile(ProfileError::Cancelled))
            ));
        }
    }
}

#[tokio::test]
async fn verified_role_hub_write_keeps_real_remote_permission_and_revocation() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    let raw = server.token(true, now_seconds() + 3600);
    put(
        &store,
        broker_config(&server.origin, true, false),
        hub_native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    let verifier = server.verifier(false, revision(&store, &ctx));
    let issuer = issuer(
        &f,
        store,
        &ctx,
        &[rule(&server.origin, CredentialSource::NativeAccess)],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let session = issuer.bootstrap_admin(&caller).unwrap();
    let parent = issuer.admin_parent(&caller, &session).unwrap();
    for permission in [
        Permission::External(ExternalEffect::RemoteRequest),
        Permission::InspectGit,
    ] {
        let definition = f.definition(Role::Coordinator, &[permission], 1, limits());
        let reference = definition.reference().clone();
        issuer.register_definition(&parent, definition).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(&parent, &run, &reference, &[permission], &f.scope())
            .unwrap();
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        if permission == Permission::InspectGit {
            assert!(
                issuer
                    .capture_role_hub_write_service(&grant, "service")
                    .is_err()
            );
        } else {
            let mut selected = issuer
                .capture_role_hub_write_service(&grant, "service")
                .unwrap();
            let lease = selected.native_credential(&verifier).await.unwrap();
            assert_eq!(
                lease.user_subject().unwrap().unwrap().subject().unwrap(),
                USER_SUB
            );
            issuer.revoke(&parent, &bearer).unwrap();
            assert!(lease.check().is_err());
        }
        issuer.close_run(&parent, &run).unwrap();
    }
}
