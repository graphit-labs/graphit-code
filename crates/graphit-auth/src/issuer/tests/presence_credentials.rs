use super::service_credentials::{
    Server, USER_SUB, broker_config, context, issuer, native_profile, native_store, now_seconds,
    put,
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

fn rule(
    endpoint: &str,
    origin: ServiceOrigin,
    purpose: ServicePurpose,
    credential: CredentialSource,
) -> ServiceRuleInput<'_> {
    ServiceRuleInput::Remote {
        origin,
        binding: RemoteServiceBindingInput::Broker {
            provider: "service",
            purpose,
            endpoint,
            resource: None,
        },
        credential,
        incoming: None,
    }
}

fn policy(rules: &[ServiceRuleInput<'_>]) -> Result<crate::IssuerAuthorityPolicy, AuthError> {
    let all = all_permissions();
    let sessions = crate::AdminSessionPolicy {
        lifetime: Duration::from_secs(7200),
        maximum: Duration::from_secs(7200),
    };
    crate::IssuerAuthorityPolicy::new(
        crate::SourceAuthorityPolicy {
            ordinary: &all,
            administrative: &all,
            sessions,
        },
        crate::SourceAuthorityPolicy {
            ordinary: &all,
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

fn local_profile() -> ProfileData {
    ProfileData {
        name: "local-presence".into(),
        provider: "service".into(),
        username: "display".into(),
        organization: None,
        teams: vec![],
        credentials: ProfileCredentials::Local(LocalCredentials {
            anonymous: false,
            mcp_key: Some(Secret::new(SecretString::from("presence-mcp"))),
            broker_key: Some(Secret::new(SecretString::from("presence-broker-key"))),
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

#[test]
fn presence_rules_reject_unrelated_modes_and_preserve_explicit_bindings() {
    for purpose in [
        ServicePurpose::PresenceRead,
        ServicePurpose::PresencePublish,
    ] {
        for origin in [ServiceOrigin::Runtime, ServiceOrigin::Local] {
            for credential in [
                CredentialSource::NativeAccess,
                CredentialSource::LocalBrokerKey,
            ] {
                assert!(
                    policy(&[rule("https://broker.example", origin, purpose, credential)]).is_ok()
                );
            }
        }
        for credential in [
            CredentialSource::Anonymous,
            CredentialSource::DirectKey,
            CredentialSource::IncomingBearer,
        ] {
            assert!(matches!(
                policy(&[rule(
                    "https://broker.example",
                    ServiceOrigin::Runtime,
                    purpose,
                    credential
                )]),
                Err(AuthError::Invalid)
            ));
        }
        assert!(matches!(
            policy(&[rule(
                "https://broker.example",
                ServiceOrigin::BrokerIncoming,
                purpose,
                CredentialSource::NativeAccess
            )]),
            Err(AuthError::Invalid)
        ));
        let local = ServiceRuleInput::Local {
            origin: ServiceOrigin::Runtime,
            provider: "service",
            purpose,
            execution: crate::profile::LocalExecution {
                device: crate::profile::LocalDevice::Cpu,
                device_id: 0,
            },
        };
        assert!(matches!(policy(&[local]), Err(AuthError::Invalid)));
        let inventory = ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::BrokerInventory {
                provider: "service",
                purpose,
                endpoint: "https://broker.example",
            },
            credential: CredentialSource::NativeAccess,
            incoming: None,
        };
        assert!(matches!(policy(&[inventory]), Err(AuthError::Invalid)));
        let direct = ServiceRuleInput::Remote {
            origin: ServiceOrigin::Runtime,
            binding: RemoteServiceBindingInput::Direct {
                provider: "service",
                purpose,
                protocol: crate::profile::DirectProtocol::OpenAi,
                endpoint: "https://broker.example",
                model: "model",
                model_revision: "fixture-deployment-2026-10",
                recipe: "fixture-vector-v1",
                dimensions: 3,
                request_dimensions: None,
                batch_endpoint: None,
            },
            credential: CredentialSource::DirectKey,
            incoming: None,
        };
        assert!(matches!(policy(&[direct]), Err(AuthError::Invalid)));
    }
}

#[test]
fn runtime_and_local_presence_use_the_real_key_and_explicit_provider() {
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = f.auth_store(&ctx);
    put(
        &store,
        broker_config("https://broker.example", false, false),
        local_profile(),
        true,
        &ctx,
    );
    let rules: Vec<_> = [ServiceOrigin::Runtime, ServiceOrigin::Local]
        .into_iter()
        .flat_map(|origin| {
            [
                ServicePurpose::PresenceRead,
                ServicePurpose::PresencePublish,
            ]
            .map(|purpose| {
                rule(
                    "https://broker.example",
                    origin,
                    purpose,
                    CredentialSource::LocalBrokerKey,
                )
            })
        })
        .collect();
    let issuer = issuer(&f, store, &ctx, &rules);
    let runtime = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    let local = issuer.authenticate_local("presence-mcp", &ctx).unwrap();
    for caller in [&runtime, &local] {
        for purpose in [
            ServicePurpose::PresenceRead,
            ServicePurpose::PresencePublish,
        ] {
            assert!(issuer.capture_service(caller, purpose).is_err());
            let mut selected = match purpose {
                ServicePurpose::PresenceRead => {
                    issuer.capture_presence_read_service(caller, "service")
                }
                ServicePurpose::PresencePublish => {
                    issuer.capture_presence_publish_service(caller, "service")
                }
                _ => unreachable!(),
            }
            .unwrap();
            assert_eq!(selected.mode().unwrap(), ServiceMode::BrokerKey);
            let lease = selected.credential().unwrap();
            assert_eq!(lease.credential().unwrap(), Some("presence-broker-key"));
            assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
            assert!(
                matches!(lease.binding().unwrap(), ServiceBindingView::Broker { purpose: p, .. } if p == purpose)
            );
            assert!(lease.user_subject().unwrap().is_none());
        }
        assert!(
            issuer
                .capture_presence_read_service(caller, "another-provider")
                .is_err()
        );
        assert!(
            issuer
                .capture_presence_publish_service(caller, "another-provider")
                .is_err()
        );
    }
}

#[tokio::test]
async fn native_role_read_keeps_current_grant_and_never_admits_role_publish() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    let raw = server.token(true, now_seconds() + 3600);
    put(
        &store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    let issuer = issuer(
        &f,
        store,
        &ctx,
        &[
            rule(
                &server.origin,
                ServiceOrigin::Runtime,
                ServicePurpose::PresenceRead,
                CredentialSource::NativeAccess,
            ),
            rule(
                &server.origin,
                ServiceOrigin::Runtime,
                ServicePurpose::PresencePublish,
                CredentialSource::NativeAccess,
            ),
        ],
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
        assert!(
            issuer
                .capture_role_service(&grant, ServicePurpose::PresencePublish)
                .is_err()
        );
        assert!(
            issuer
                .capture_role_service(&grant, ServicePurpose::PresenceRead)
                .is_err()
        );
        if permission == Permission::InspectGit {
            assert!(
                issuer
                    .capture_role_presence_read_service(&grant, "service")
                    .is_err()
            );
        } else {
            let mut selected = issuer
                .capture_role_presence_read_service(&grant, "service")
                .unwrap();
            let ServiceBindingView::Broker {
                provider_revision,
                endpoint,
                purpose,
                ..
            } = selected.binding().unwrap()
            else {
                unreachable!()
            };
            assert_eq!(endpoint, server.origin);
            assert_eq!(purpose, ServicePurpose::PresenceRead);
            let verifier = server.verifier(false, provider_revision);
            let lease = selected.native_credential(&verifier).await.unwrap();
            assert_eq!(
                lease.user_subject().unwrap().unwrap().subject().unwrap(),
                USER_SUB
            );
            assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
            lease.check().unwrap();
            issuer.revoke(&parent, &bearer).unwrap();
            assert!(lease.check().is_err());
            assert!(lease.credential().is_err());
        }
        issuer.close_run(&parent, &run).unwrap();
    }
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn native_runtime_read_and_publish_bind_current_profile_revision_and_original() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = native_store(&f, &ctx);
    let raw = server.token(true, now_seconds() + 3600);
    let mut config = broker_config(&server.origin, true, false);
    config.embedding = AiService::Disabled;
    config.rerank = AiService::Disabled;
    put(
        &store,
        config.clone(),
        native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    let provider_revision = revision(&store, &ctx);
    let verifier = server.verifier(false, provider_revision);
    let issuer = issuer(
        &f,
        Arc::clone(&store),
        &ctx,
        &[
            rule(
                &server.origin,
                ServiceOrigin::Runtime,
                ServicePurpose::PresenceRead,
                CredentialSource::NativeAccess,
            ),
            rule(
                &server.origin,
                ServiceOrigin::Runtime,
                ServicePurpose::PresencePublish,
                CredentialSource::NativeAccess,
            ),
        ],
    );
    let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
    for purpose in [
        ServicePurpose::PresenceRead,
        ServicePurpose::PresencePublish,
    ] {
        let mut selected = if purpose == ServicePurpose::PresenceRead {
            issuer.capture_presence_read_service(&caller, "service")
        } else {
            issuer.capture_presence_publish_service(&caller, "service")
        }
        .unwrap();
        let lease = selected.native_credential(&verifier).await.unwrap();
        assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
        assert!(
            matches!(lease.binding().unwrap(), ServiceBindingView::Broker { provider_revision: r, purpose: p, .. } if r == provider_revision && p == purpose)
        );
        lease.check().unwrap();
    }
    let wrong_revision = server.verifier(false, provider_revision + 1);
    let mut selected = issuer
        .capture_presence_read_service(&caller, "service")
        .unwrap();
    assert!(selected.native_credential(&wrong_revision).await.is_err());
    let mut selected = issuer
        .capture_presence_read_service(&caller, "service")
        .unwrap();
    let lease = selected.native_credential(&verifier).await.unwrap();
    put(
        &store,
        config,
        native_profile(&server.origin, &raw, now_seconds() + 3600),
        true,
        &ctx,
    );
    assert!(lease.check().is_err());
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn presence_native_expiry_and_wrong_resource_never_produce_a_lease() {
    for bad_resource in [false, true] {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = native_store(&f, &ctx);
        let expiration = if bad_resource {
            now_seconds() + 3600
        } else {
            now_seconds().saturating_sub(1)
        };
        let raw = server.sign(&json!({
            "iss": server.origin, "sub": USER_SUB,
            "aud": if bad_resource { json!(["wrong-resource"]) } else { json!(["broker-api"]) },
            "client_id": "graphit-cli", "token_use": "access", "jti": "presence",
            "iat": now_seconds().saturating_sub(60), "exp": expiration
        }));
        put(
            &store,
            broker_config(&server.origin, true, false),
            native_profile(&server.origin, &raw, now_seconds() + 3600),
            true,
            &ctx,
        );
        let verifier = server.verifier(false, revision(&store, &ctx));
        let issuer = issuer(
            &f,
            store,
            &ctx,
            &[rule(
                &server.origin,
                ServiceOrigin::Runtime,
                ServicePurpose::PresenceRead,
                CredentialSource::NativeAccess,
            )],
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selection = issuer
            .capture_presence_read_service(&caller, "service")
            .unwrap();
        assert!(selection.native_credential(&verifier).await.is_err());
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/service"), 0);
    }
}

#[tokio::test]
async fn presence_native_wait_preserves_original_cancellation_and_deadline() {
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
            native_profile(&server.origin, &raw, now_seconds() + 3600),
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
            &[rule(
                &server.origin,
                ServiceOrigin::Runtime,
                ServicePurpose::PresenceRead,
                CredentialSource::NativeAccess,
            )],
        );
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let mut selected = issuer
            .capture_presence_read_service(&caller, "service")
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
        match future.await {
            Err(
                ServiceUseError::Broker(BrokerError::Deadline)
                | ServiceUseError::Profile(ProfileError::Deadline),
            ) if deadline_case => {}
            Err(
                ServiceUseError::Broker(BrokerError::Cancelled)
                | ServiceUseError::Profile(ProfileError::Cancelled),
            ) if !deadline_case => {}
            _ => panic!("original cause was not preserved"),
        }
    }
}

#[test]
fn presence_role_expiry_run_close_and_policy_revocation_invalidate_retained_lease() {
    for change in 0..3 {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = f.auth_store(&ctx);
        put(
            &store,
            broker_config("https://broker.example", false, false),
            local_profile(),
            true,
            &ctx,
        );
        let rules = [rule(
            "https://broker.example",
            ServiceOrigin::Runtime,
            ServicePurpose::PresenceRead,
            CredentialSource::LocalBrokerKey,
        )];
        let clock = clock();
        let issuer = RoleIssuer::with_sources(
            f.realm(),
            Arc::clone(&f.runtime),
            store,
            policy(&rules).unwrap(),
            ttl(),
            &ctx,
            clock.clone(),
            Arc::new(OsEntropy),
        )
        .unwrap();
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        let session = issuer.bootstrap_admin(&caller).unwrap();
        let parent = issuer.admin_parent(&caller, &session).unwrap();
        let permissions = [Permission::External(ExternalEffect::RemoteRequest)];
        let definition = f.definition(Role::Coordinator, &permissions, 1, limits());
        let reference = definition.reference().clone();
        issuer.register_definition(&parent, definition).unwrap();
        let run = issuer
            .start_run(&parent, &f.scope(), Duration::from_secs(3600))
            .unwrap();
        let bearer = issuer
            .issue(&parent, &run, &reference, &permissions, &f.scope())
            .unwrap();
        let grant = issuer
            .verify(bearer.expose_for_transport(), &reference, f.scope(), &ctx)
            .unwrap();
        let mut selected = issuer
            .capture_role_presence_read_service(&grant, "service")
            .unwrap();
        let lease = selected.credential().unwrap();
        assert_eq!(lease.credential().unwrap(), Some("presence-broker-key"));
        match change {
            0 => clock.seconds.store(900, Ordering::SeqCst),
            1 => issuer.close_run(&parent, &run).unwrap(),
            _ => issuer.revoke_service_policy(&parent).unwrap(),
        }
        assert!(lease.check().is_err());
        assert!(lease.credential().is_err());
        assert!(
            issuer
                .capture_role_presence_read_service(&grant, "service")
                .is_err()
        );
    }
}

#[test]
fn presence_missing_profile_or_mismatched_resource_does_not_fallback() {
    for active in [false, true] {
        let cancel = Cancellation::default();
        let ctx = context(&cancel);
        let f = Fixture::new(&ctx);
        let store = f.auth_store(&ctx);
        put(
            &store,
            broker_config("https://broker.example", false, true),
            local_profile(),
            active,
            &ctx,
        );
        let mut binding = rule(
            "https://broker.example",
            ServiceOrigin::Runtime,
            ServicePurpose::PresenceRead,
            CredentialSource::LocalBrokerKey,
        );
        if active {
            let ServiceRuleInput::Remote {
                binding: RemoteServiceBindingInput::Broker { resource, .. },
                ..
            } = &mut binding
            else {
                unreachable!()
            };
            *resource = Some("https://different-resource.example");
        }
        let issuer = issuer(&f, store, &ctx, &[binding]);
        let caller = issuer.authenticate_runtime(&f.secret(), &ctx).unwrap();
        assert!(
            issuer
                .capture_presence_read_service(&caller, "service")
                .is_err()
        );
    }
}
