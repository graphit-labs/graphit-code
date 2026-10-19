use super::super::service_credentials::{
    broker_config, local_config, local_profile, native_profile, policy as service_policy, put,
};
use super::*;
use crate::{
    CredentialSource, RemoteServiceBindingInput, ServiceAdmissionLimits, ServiceBindingView,
    ServiceMode, ServiceOrigin, ServicePurpose, ServiceRuleInput, ServiceUseError,
    VerifiedUserOrigin,
    issuer::service_credentials::S3CredentialView,
    profile::{LocalS3, ProfileCredentials, ProfileData, ProfileExpectation, Secret},
};

fn broker_rule(endpoint: &str) -> ServiceRuleInput<'_> {
    ServiceRuleInput::Remote {
        origin: ServiceOrigin::Web,
        binding: RemoteServiceBindingInput::Broker {
            provider: "service",
            purpose: ServicePurpose::HubRead,
            endpoint,
            resource: None,
        },
        credential: CredentialSource::IncomingBearer,
        incoming: None,
    }
}

fn standalone_rule(profile: Option<&str>) -> ServiceRuleInput<'_> {
    ServiceRuleInput::StandaloneS3 {
        purpose: ServicePurpose::HubRead,
        origin: ServiceOrigin::Web,
        provider: "storage",
        profile,
    }
}

fn policy(rules: &[ServiceRuleInput<'_>]) -> IssuerAuthorityPolicy {
    service_policy(
        rules,
        &[],
        limits(),
        ServiceAdmissionLimits {
            max_rules: 16,
            max_binding_bytes: 8192,
        },
        Duration::ZERO,
    )
    .unwrap()
    .with_web_installed_hub_read("service")
    .unwrap()
}

fn storage_profile(mcp: &str) -> ProfileData {
    let mut data = local_profile("transport", mcp, None, false);
    data.provider = "storage".into();
    data.username = "untrusted-transport-user".into();
    data.teams = vec!["untrusted-transport-team".into()];
    let ProfileCredentials::Local(keys) = &mut data.credentials else {
        unreachable!()
    };
    keys.s3 = Some(LocalS3 {
        access_key_id: Some(Secret::new(SecretString::from("selected-storage-access"))),
        secret_access_key: Some(Secret::new(SecretString::from("selected-storage-secret"))),
        session_token: None,
        expires_at: None,
        aws_profile: None,
    });
    data
}

fn add_storage(store: &AuthStore, ctx: &RequestContext<'_>) {
    let mut config = local_config();
    config.s3.bucket = Some("explicit-installed-storage".into());
    let data = storage_profile("unused-transport-mcp");
    let result = store
        .mutate(store.load(ctx).unwrap(), ctx, |doc, ctx| {
            doc.add_provider("storage".into(), config, doc.revision(), 1, ctx)?;
            let expected = ProfileExpectation {
                document_revision: doc.revision(),
                provider_revision: doc.provider("storage").unwrap().revision(),
                profile_revision: None,
            };
            doc.replace_profile_data(data, expected, 1, ctx)
        })
        .unwrap();
    assert!(result.failure().is_none());
    assert!(result.cleanup_failure().is_none());
}

#[tokio::test]
async fn installed_hub_broker_service_retains_web_proof_without_project_or_profile() {
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let revision = provider(&store, &server, &ctx);
    let rules = [broker_rule(&server.origin)];
    let allowed = issuer(&f, Arc::clone(&store), &ctx, policy(&rules));
    let other = issuer(&f, Arc::clone(&store), &ctx, policy(&rules));
    let verifier = server.verifier(false, revision);
    let flow = WebOAuth::new(
        &verifier,
        web_limits(),
        "https://workspace.example/callback",
        false,
    )
    .unwrap();
    let registered = flow
        .register(BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let raw = access(&server, registered.resource(), now_seconds() + 600);
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let keys = server.count("/keys");
    let read = allowed
        .admit_web_installed_hub_read(&proof, &verifier, &ctx, "service", f.realm())
        .unwrap();
    assert!(
        other
            .capture_installed_hub_service(&read, "service")
            .is_err()
    );
    assert!(
        allowed
            .capture_installed_hub_service(&read, "unknown")
            .is_err()
    );
    assert!(allowed.capture_installed_hub_service(&read, "").is_err());
    let mut selection = allowed
        .capture_installed_hub_service(&read, "service")
        .unwrap();
    assert_eq!(selection.mode().unwrap(), ServiceMode::BrokerIncoming);
    let lease = selection.credential().unwrap();
    assert!(std::ptr::eq(lease.original().unwrap(), &ctx));
    assert!(matches!(
        lease.binding().unwrap(),
        ServiceBindingView::Broker {
            provider: "service",
            purpose: ServicePurpose::HubRead,
            ..
        }
    ));
    assert_eq!(lease.credential().unwrap(), Some(raw.as_str()));
    let subject = lease.user_subject().unwrap().unwrap();
    assert_eq!(subject.origin().unwrap(), VerifiedUserOrigin::Web);
    assert_eq!(subject.subject().unwrap(), proof.subject());
    assert!(lease.standalone_s3().is_err());
    assert!(store.load(&ctx).unwrap().document().profiles().is_empty());
    assert_eq!(server.count("/keys"), keys);
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
    cancel.cancel();
    assert!(lease.credential().is_err());
    assert!(subject.check().is_err());
}

#[tokio::test]
async fn installed_hub_service_requires_exact_explicit_web_rule() {
    let server = Server::new().await;
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let revision = provider(&store, &server, &ctx);
    let verifier = server.verifier(false, revision);
    let flow = WebOAuth::new(
        &verifier,
        web_limits(),
        "https://workspace.example/callback",
        false,
    )
    .unwrap();
    let registered = flow
        .register(BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let raw = access(&server, registered.resource(), now_seconds() + 600);
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    for mismatch in 0..5 {
        let mut rule = broker_rule(&server.origin);
        if let ServiceRuleInput::Remote {
            origin,
            binding,
            credential,
            ..
        } = &mut rule
        {
            if mismatch == 1 {
                *origin = ServiceOrigin::Runtime;
                *credential = CredentialSource::NativeAccess;
            }
            if let RemoteServiceBindingInput::Broker {
                endpoint,
                resource,
                provider,
                ..
            } = binding
            {
                match mismatch {
                    2 => *endpoint = "https://different.example/",
                    3 => *resource = Some("https://different.example/resource"),
                    4 => *provider = "another-provider",
                    _ => (),
                }
            }
        }
        let rules = if mismatch == 0 { vec![] } else { vec![rule] };
        let denied = issuer(&f, Arc::clone(&store), &ctx, policy(&rules));
        let read = denied
            .admit_web_installed_hub_read(&proof, &verifier, &ctx, "service", f.realm())
            .unwrap();
        assert!(
            denied
                .capture_installed_hub_service(&read, "service")
                .is_err()
        );
    }
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn installed_hub_standalone_uses_explicit_transport_and_web_subject() {
    let server = Server::new().await;
    server.userinfo_status.store(503, Ordering::SeqCst);
    let cancel = Cancellation::default();
    let ctx = context(&cancel);
    let f = Fixture::new(&ctx);
    let store = store(&f, &ctx);
    let revision = provider(&store, &server, &ctx);
    add_storage(&store, &ctx);
    let verifier = server.verifier(false, revision);
    let flow = WebOAuth::new(
        &verifier,
        web_limits(),
        "https://workspace.example/callback",
        false,
    )
    .unwrap();
    let registered = flow
        .register(BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    let raw = access(&server, registered.resource(), now_seconds() + 600);
    let proof = flow
        .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
        .await
        .unwrap();
    put(
        &store,
        broker_config(&server.origin, true, false),
        native_profile(&server.origin, &raw, now_seconds() + 600),
        true,
        &ctx,
    );
    let rules = [standalone_rule(Some("transport")), standalone_rule(None)];
    let allowed = issuer(&f, Arc::clone(&store), &ctx, policy(&rules));
    let read = allowed
        .admit_web_installed_hub_read(&proof, &verifier, &ctx, "service", f.realm())
        .unwrap();
    assert!(
        allowed
            .capture_installed_hub_service(&read, "storage")
            .is_err()
    );
    assert!(
        allowed
            .capture_installed_hub_standalone_s3_service(&read, "storage", Some("native"))
            .is_err()
    );
    assert!(
        allowed
            .capture_installed_hub_standalone_s3_service(&read, "service", None)
            .is_err()
    );
    for profile in [Some("transport"), None] {
        let mut selection = allowed
            .capture_installed_hub_standalone_s3_service(&read, "storage", profile)
            .unwrap();
        assert_eq!(selection.mode().unwrap(), ServiceMode::StandaloneS3);
        let lease = selection.credential().unwrap();
        let view = lease.standalone_s3().unwrap();
        assert_eq!(
            view.location.bucket.as_deref(),
            Some("explicit-installed-storage")
        );
        match (profile, view.credential) {
            (Some(_), S3CredentialView::Local(keys)) => assert_eq!(
                keys.access_key_id.as_ref().unwrap().expose(),
                "selected-storage-access"
            ),
            (None, S3CredentialView::Anonymous) => (),
            _ => panic!("only explicitly selected transport is admitted"),
        }
        let subject = lease.user_subject().unwrap().unwrap();
        assert_eq!(subject.origin().unwrap(), VerifiedUserOrigin::Web);
        assert_eq!(subject.issuer().unwrap(), proof.issuer());
        assert_eq!(subject.subject().unwrap(), proof.subject());
        assert_eq!(subject.username().unwrap(), proof.username());
        assert_eq!(subject.teams().unwrap(), proof.teams());
        assert!(matches!(
            lease.credential(),
            Err(ServiceUseError::WrongMode)
        ));
    }
    assert_eq!(
        store
            .load(&ctx)
            .unwrap()
            .document()
            .active(&ctx)
            .unwrap()
            .1
            .name(),
        "native"
    );
    let mut selection = allowed
        .capture_installed_hub_standalone_s3_service(&read, "storage", Some("transport"))
        .unwrap();
    let lease = selection.credential().unwrap();
    let result = store
        .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
            let current = doc.profile("transport").unwrap();
            let revision = current.revision();
            let now = current.updated_at() + 1;
            let mut data = storage_profile("changed-transport-mcp");
            data.username = "changed-transport-data".into();
            doc.replace_profile_data(
                data,
                ProfileExpectation {
                    document_revision: doc.revision(),
                    provider_revision: doc.provider("storage").unwrap().revision(),
                    profile_revision: Some(revision),
                },
                now,
                ctx,
            )
        })
        .unwrap();
    assert!(result.failure().is_none());
    assert!(lease.standalone_s3().is_err());
    assert!(lease.user_subject().is_err());
    assert_eq!(server.count("/userinfo"), 0);
    assert_eq!(server.count("/token"), 0);
}

#[tokio::test]
async fn installed_hub_service_leases_recheck_current_state_and_request_end() {
    for ending in 0..5 {
        let server = Server::new().await;
        let cancel = Cancellation::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let ctx = if ending == 3 {
            request_before(&cancel, deadline)
        } else {
            context(&cancel)
        };
        let f = Fixture::new(&ctx);
        let store = store(&f, &ctx);
        let revision = provider(&store, &server, &ctx);
        add_storage(&store, &ctx);
        let rules = [
            broker_rule(&server.origin),
            standalone_rule(Some("transport")),
        ];
        let allowed = issuer(&f, Arc::clone(&store), &ctx, policy(&rules));
        let verifier = server.verifier(false, revision);
        let flow = WebOAuth::new(
            &verifier,
            web_limits(),
            "https://workspace.example/callback",
            false,
        )
        .unwrap();
        let registered = flow
            .register(BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let expiry = now_seconds() + if ending == 4 { 2 } else { 600 };
        let raw = access(&server, registered.resource(), expiry);
        let proof = flow
            .verify_access(&registered, &raw, BrokerRequestContext::new(ctx.snapshot()))
            .await
            .unwrap();
        let read = allowed
            .admit_web_installed_hub_read(&proof, &verifier, &ctx, "service", f.realm())
            .unwrap();
        let mut selection = if matches!(ending, 1 | 3) {
            allowed.capture_installed_hub_standalone_s3_service(&read, "storage", Some("transport"))
        } else {
            allowed.capture_installed_hub_service(&read, "service")
        }
        .unwrap();
        let lease = selection.credential().unwrap();
        let subject = lease.user_subject().unwrap().unwrap();
        let keys = server.count("/keys");
        match ending {
            0 => {
                let result = store
                    .mutate(store.load(&ctx).unwrap(), &ctx, |doc, ctx| {
                        let current = doc.provider("service").unwrap();
                        let config = current.config().clone();
                        let revision = current.revision();
                        let now = current.updated_at() + 1;
                        doc.update_provider("service", config, doc.revision(), revision, now, ctx)
                    })
                    .unwrap();
                assert!(result.failure().is_none());
            }
            1 => {
                let root = f.installation.owner_root();
                std::fs::rename(root, root.with_extension("replaced")).unwrap();
                std::fs::create_dir(root).unwrap();
            }
            2 => cancel.cancel(),
            3 => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
            _ => {
                tokio::time::sleep(
                    std::time::UNIX_EPOCH
                        .checked_add(Duration::from_secs(expiry))
                        .unwrap()
                        .duration_since(SystemTime::now())
                        .unwrap()
                        + Duration::from_millis(10),
                )
                .await
            }
        }
        assert!(lease.check().is_err());
        assert!(subject.check().is_err());
        assert!(selection.mode().is_err());
        assert_eq!(server.count("/keys"), keys);
        assert_eq!(server.count("/userinfo"), 0);
        assert_eq!(server.count("/token"), 0);
    }
}
